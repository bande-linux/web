use web_atoms::{Atom, Namespace, QualName};
use web_dom::{Document, NodeId, NodeKind, QuirksMode};

use crate::bindings::*;
use crate::convert::Global;
use crate::engine::JsValue;
use crate::exception::Exception;
use crate::host::Host;

pub struct Api;

type Result<T> = std::result::Result<T, Exception>;

pub fn interface_of(document: &Document, node: NodeId) -> Option<Interface> {
    Some(match document.kind(node)? {
        NodeKind::Document => Interface::Document,
        NodeKind::Doctype => Interface::DocumentType,
        NodeKind::Element => match document.element_name(node) {
            Some((Namespace::Html, _)) => Interface::HTMLElement,
            _ => Interface::Element,
        },
        NodeKind::Text => Interface::Text,
        NodeKind::Comment => Interface::Comment,
        NodeKind::ProcessingInstruction => Interface::CharacterData,
        NodeKind::Fragment => Interface::DocumentFragment,
    })
}

pub fn implements(document: &Document, node: NodeId, interface: Interface) -> bool {
    let mut current = interface_of(document, node);
    while let Some(known) = current {
        if known == interface {
            return true;
        }
        current = known.parent();
    }
    false
}

fn is_element(document: &Document, node: NodeId) -> bool {
    document.kind(node) == Some(NodeKind::Element)
}

fn is_character_data(document: &Document, node: NodeId) -> bool {
    matches!(
        document.kind(node),
        Some(NodeKind::Text | NodeKind::Comment | NodeKind::ProcessingInstruction)
    )
}

fn descendants(document: &Document, root: NodeId) -> Vec<NodeId> {
    let mut found = Vec::new();
    let mut stack: Vec<NodeId> = document.children(root).collect();
    stack.reverse();
    while let Some(node) = stack.pop() {
        found.push(node);
        let start = stack.len();
        stack.extend(document.children(node));
        if let Some(added) = stack.get_mut(start..) {
            added.reverse();
        }
    }
    found
}

fn text_content(document: &Document, node: NodeId) -> String {
    descendants(document, node)
        .into_iter()
        .filter(|&child| document.kind(child) == Some(NodeKind::Text))
        .filter_map(|child| document.text(child))
        .collect()
}

fn tag_name(document: &Document, node: NodeId) -> String {
    let Some((namespace, name)) = document.element_name(node) else {
        return String::new();
    };
    let name = document.atoms().name(name);
    if namespace == Namespace::Html {
        name.to_ascii_uppercase()
    } else {
        name.to_owned()
    }
}

fn attribute_name(document: &mut Document, element: NodeId, name: &str) -> QualName {
    let html = matches!(document.element_name(element), Some((Namespace::Html, _)));
    let name = if html {
        name.to_ascii_lowercase()
    } else {
        name.to_owned()
    };
    QualName::attribute(document.intern(&name))
}

fn existing_attribute(document: &Document, element: NodeId, name: &str) -> Option<QualName> {
    let html = matches!(document.element_name(element), Some((Namespace::Html, _)));
    let wanted = if html {
        name.to_ascii_lowercase()
    } else {
        name.to_owned()
    };
    let atom = document.atoms().get(&wanted)?;
    let qualified = QualName::attribute(atom);
    document
        .qualified_attribute(element, qualified)
        .map(|_| qualified)
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_' || first == ':')
        && chars.all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | ':' | '-' | '.' | '·'))
}

fn utf16_length(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

fn is_inclusive_ancestor(document: &Document, ancestor: NodeId, node: NodeId) -> bool {
    node == ancestor || document.ancestors(node).any(|known| known == ancestor)
}

fn ensure_insertable(
    document: &Document,
    parent: NodeId,
    node: NodeId,
    child: Option<NodeId>,
) -> Result<()> {
    if !matches!(
        document.kind(parent),
        Some(NodeKind::Document | NodeKind::Fragment | NodeKind::Element)
    ) {
        return Err(Exception::dom(
            "HierarchyRequestError",
            "the parent cannot have children",
        ));
    }
    if is_inclusive_ancestor(document, node, parent) {
        return Err(Exception::dom(
            "HierarchyRequestError",
            "the node is an ancestor of the parent",
        ));
    }
    if child.is_some_and(|child| document.parent(child) != Some(parent)) {
        return Err(Exception::dom(
            "NotFoundError",
            "the reference is not a child of the parent",
        ));
    }
    match document.kind(node) {
        Some(NodeKind::Document) | None => Err(Exception::dom(
            "HierarchyRequestError",
            "a document cannot be inserted",
        )),
        Some(NodeKind::Text) if document.kind(parent) == Some(NodeKind::Document) => {
            Err(Exception::dom(
                "HierarchyRequestError",
                "text cannot be a child of a document",
            ))
        }
        Some(NodeKind::Doctype) if document.kind(parent) != Some(NodeKind::Document) => {
            Err(Exception::dom(
                "HierarchyRequestError",
                "a doctype must be a child of a document",
            ))
        }
        Some(NodeKind::Element) if document.kind(parent) == Some(NodeKind::Document) => {
            let other = document
                .children(parent)
                .any(|existing| existing != node && is_element(document, existing));
            if other {
                Err(Exception::dom(
                    "HierarchyRequestError",
                    "a document has one element child",
                ))
            } else {
                Ok(())
            }
        }
        _ => Ok(()),
    }
}

fn insert(host: &mut Host, parent: NodeId, node: NodeId, child: Option<NodeId>) -> Result<NodeId> {
    ensure_insertable(&host.document, parent, node, child)?;
    let reference = if child == Some(node) {
        host.document.next_sibling(node)
    } else {
        child
    };
    if host.document.kind(node) == Some(NodeKind::Fragment) {
        let moved: Vec<NodeId> = host.document.children(node).collect();
        for moving in moved {
            host.document.insert(parent, moving, reference);
        }
    } else {
        host.document.insert(parent, node, reference);
    }
    host.mutated = true;
    Ok(node)
}

fn remove_children(host: &mut Host, parent: NodeId) {
    let children: Vec<NodeId> = host.document.children(parent).collect();
    for child in children {
        host.document.detach(child);
    }
}

fn first_element(document: &Document, parent: NodeId, name: Atom) -> Option<NodeId> {
    document
        .children(parent)
        .find(|&child| document.html_element_name(child) == Some(name))
}

fn elements_by_tag(document: &Document, root: NodeId, name: &str) -> Vec<NodeId> {
    let lowered = name.to_ascii_lowercase();
    descendants(document, root)
        .into_iter()
        .filter(|&node| {
            document
                .element_name(node)
                .is_some_and(|(namespace, local)| {
                    let local = document.atoms().name(local);
                    name == "*"
                        || (namespace == Namespace::Html && local == lowered)
                        || (namespace != Namespace::Html && local == name)
                })
        })
        .collect()
}

fn elements_by_class(document: &Document, root: NodeId, names: &str) -> Vec<NodeId> {
    let wanted: Vec<&str> = names.split_ascii_whitespace().collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    descendants(document, root)
        .into_iter()
        .filter(|&node| {
            let classes: Vec<&str> = document
                .classes(node)
                .iter()
                .map(|&class| document.atoms().name(class))
                .collect();
            is_element(document, node) && wanted.iter().all(|name| classes.contains(name))
        })
        .collect()
}

fn reflect(host: &Host, this: NodeId, name: Atom) -> String {
    host.document
        .attribute(this, name)
        .unwrap_or_default()
        .to_owned()
}

fn set_reflected(host: &mut Host, this: NodeId, name: Atom, value: &str) {
    host.document
        .set_attribute(this, QualName::attribute(name), value);
    host.mutated = true;
}

impl EventTargetApi for Api {}

impl DocumentFragmentApi for Api {}

impl CommentApi for Api {}

impl NodeApi for Api {
    fn node_type(host: &mut Host, this: NodeId) -> Result<u16> {
        Ok(match host.document.kind(this) {
            Some(NodeKind::Element) => 1,
            Some(NodeKind::Text) => 3,
            Some(NodeKind::ProcessingInstruction) => 7,
            Some(NodeKind::Comment) => 8,
            Some(NodeKind::Document) => 9,
            Some(NodeKind::Doctype) => 10,
            Some(NodeKind::Fragment) => 11,
            None => 0,
        })
    }

    fn node_name(host: &mut Host, this: NodeId) -> Result<String> {
        let document = &host.document;
        Ok(match document.kind(this) {
            Some(NodeKind::Element) => tag_name(document, this),
            Some(NodeKind::Text) => "#text".to_owned(),
            Some(NodeKind::Comment) => "#comment".to_owned(),
            Some(NodeKind::Document) => "#document".to_owned(),
            Some(NodeKind::Fragment) => "#document-fragment".to_owned(),
            Some(NodeKind::Doctype) => document.doctype_name(this).unwrap_or_default().to_owned(),
            Some(NodeKind::ProcessingInstruction) => document
                .processing_instruction_target(this)
                .unwrap_or_default()
                .to_owned(),
            None => String::new(),
        })
    }

    fn is_connected(host: &mut Host, this: NodeId) -> Result<bool> {
        Ok(is_inclusive_ancestor(
            &host.document,
            NodeId::DOCUMENT,
            this,
        ))
    }

    fn owner_document(_host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok((this != NodeId::DOCUMENT).then_some(NodeId::DOCUMENT))
    }

    fn parent_node(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(host.document.parent(this))
    }

    fn parent_element(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(host
            .document
            .parent(this)
            .filter(|&parent| is_element(&host.document, parent)))
    }

    fn has_child_nodes(host: &mut Host, this: NodeId) -> Result<bool> {
        Ok(host.document.first_child(this).is_some())
    }

    fn child_nodes(host: &mut Host, this: NodeId) -> Result<Vec<NodeId>> {
        Ok(host.document.children(this).collect())
    }

    fn first_child(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(host.document.first_child(this))
    }

    fn last_child(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(host.document.last_child(this))
    }

    fn previous_sibling(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(host.document.previous_sibling(this))
    }

    fn next_sibling(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(host.document.next_sibling(this))
    }

    fn node_value(host: &mut Host, this: NodeId) -> Result<Option<String>> {
        Ok(host.document.text(this).map(str::to_owned))
    }

    fn set_node_value(host: &mut Host, this: NodeId, value: Option<String>) -> Result<()> {
        if is_character_data(&host.document, this) {
            host.document.set_text(this, &value.unwrap_or_default());
            host.mutated = true;
        }
        Ok(())
    }

    fn text_content(host: &mut Host, this: NodeId) -> Result<Option<String>> {
        let document = &host.document;
        Ok(match document.kind(this) {
            Some(NodeKind::Element | NodeKind::Fragment) => Some(text_content(document, this)),
            Some(NodeKind::Text | NodeKind::Comment | NodeKind::ProcessingInstruction) => {
                document.text(this).map(str::to_owned)
            }
            _ => None,
        })
    }

    fn set_text_content(host: &mut Host, this: NodeId, value: Option<String>) -> Result<()> {
        let value = value.unwrap_or_default();
        match host.document.kind(this) {
            Some(NodeKind::Element | NodeKind::Fragment) => {
                remove_children(host, this);
                if !value.is_empty() {
                    let text = host.document.create_text(&value);
                    host.document.insert(this, text, None);
                }
                host.mutated = true;
            }
            Some(NodeKind::Text | NodeKind::Comment | NodeKind::ProcessingInstruction) => {
                host.document.set_text(this, &value);
                host.mutated = true;
            }
            _ => {}
        }
        Ok(())
    }

    fn contains(host: &mut Host, this: NodeId, other: Option<NodeId>) -> Result<bool> {
        Ok(other.is_some_and(|other| is_inclusive_ancestor(&host.document, this, other)))
    }

    fn insert_before(
        host: &mut Host,
        this: NodeId,
        node: NodeId,
        child: Option<NodeId>,
    ) -> Result<NodeId> {
        insert(host, this, node, child)
    }

    fn append_child(host: &mut Host, this: NodeId, node: NodeId) -> Result<NodeId> {
        insert(host, this, node, None)
    }

    fn replace_child(host: &mut Host, this: NodeId, node: NodeId, child: NodeId) -> Result<NodeId> {
        if host.document.parent(child) != Some(this) {
            return Err(Exception::dom(
                "NotFoundError",
                "the node to replace is not a child",
            ));
        }
        if node == child {
            return Ok(child);
        }
        let reference = host.document.next_sibling(child);
        let reference = if reference == Some(node) {
            host.document.next_sibling(node)
        } else {
            reference
        };
        host.document.detach(child);
        if let Err(error) = insert(host, this, node, reference) {
            host.document.insert(this, child, reference);
            return Err(error);
        }
        Ok(child)
    }

    fn remove_child(host: &mut Host, this: NodeId, child: NodeId) -> Result<NodeId> {
        if host.document.parent(child) != Some(this) {
            return Err(Exception::dom(
                "NotFoundError",
                "the node to remove is not a child",
            ));
        }
        host.document.detach(child);
        host.mutated = true;
        Ok(child)
    }
}

impl DocumentApi for Api {
    fn compat_mode(host: &mut Host, _this: NodeId) -> Result<String> {
        Ok(match host.document.quirks_mode() {
            QuirksMode::Quirks => "BackCompat",
            _ => "CSS1Compat",
        }
        .to_owned())
    }

    fn document_element(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(host
            .document
            .children(this)
            .find(|&child| is_element(&host.document, child)))
    }

    fn head(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        let root = first_element(&host.document, this, Atom::HTML);
        Ok(root.and_then(|root| first_element(&host.document, root, Atom::HEAD)))
    }

    fn body(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        let root = first_element(&host.document, this, Atom::HTML);
        Ok(root.and_then(|root| first_element(&host.document, root, Atom::BODY)))
    }

    fn title(host: &mut Host, this: NodeId) -> Result<String> {
        let document = &host.document;
        let title = descendants(document, this)
            .into_iter()
            .find(|&node| document.html_element_name(node) == Some(Atom::TITLE));
        Ok(title
            .map(|title| {
                text_content(document, title)
                    .split_ascii_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default())
    }

    fn get_element_by_id(
        host: &mut Host,
        this: NodeId,
        element_id: String,
    ) -> Result<Option<NodeId>> {
        let document = &host.document;
        let Some(atom) = document.atoms().get(&element_id) else {
            return Ok(None);
        };
        Ok(descendants(document, this)
            .into_iter()
            .find(|&node| document.element_id(node) == Some(atom)))
    }

    fn get_elements_by_tag_name(
        host: &mut Host,
        this: NodeId,
        qualified_name: String,
    ) -> Result<Vec<NodeId>> {
        Ok(elements_by_tag(&host.document, this, &qualified_name))
    }

    fn get_elements_by_class_name(
        host: &mut Host,
        this: NodeId,
        class_names: String,
    ) -> Result<Vec<NodeId>> {
        Ok(elements_by_class(&host.document, this, &class_names))
    }

    fn create_element(host: &mut Host, _this: NodeId, local_name: String) -> Result<NodeId> {
        if !valid_name(&local_name) {
            return Err(Exception::dom(
                "InvalidCharacterError",
                format!("\"{local_name}\" is not a valid element name"),
            ));
        }
        let name = host.document.intern(&local_name.to_ascii_lowercase());
        Ok(host
            .document
            .create_element(QualName::html(name), std::iter::empty()))
    }

    fn create_document_fragment(host: &mut Host, _this: NodeId) -> Result<NodeId> {
        Ok(host.document.create_fragment())
    }

    fn create_text_node(host: &mut Host, _this: NodeId, data: String) -> Result<NodeId> {
        Ok(host.document.create_text(&data))
    }

    fn create_comment(host: &mut Host, _this: NodeId, data: String) -> Result<NodeId> {
        Ok(host.document.create_comment(&data))
    }
}

impl DocumentTypeApi for Api {
    fn name(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(host
            .document
            .doctype_name(this)
            .unwrap_or_default()
            .to_owned())
    }

    fn public_id(host: &mut Host, _this: NodeId) -> Result<String> {
        Ok(host.document.doctype_ids().0.to_owned())
    }

    fn system_id(host: &mut Host, _this: NodeId) -> Result<String> {
        Ok(host.document.doctype_ids().1.to_owned())
    }
}

impl ElementApi for Api {
    fn namespace_uri(host: &mut Host, this: NodeId) -> Result<Option<String>> {
        Ok(host
            .document
            .element_name(this)
            .and_then(|(namespace, _)| match namespace {
                Namespace::Html => Some("http://www.w3.org/1999/xhtml"),
                Namespace::Svg => Some("http://www.w3.org/2000/svg"),
                Namespace::MathMl => Some("http://www.w3.org/1998/Math/MathML"),
                _ => None,
            })
            .map(str::to_owned))
    }

    fn local_name(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(host
            .document
            .element_name(this)
            .map(|(_, name)| host.document.atoms().name(name).to_owned())
            .unwrap_or_default())
    }

    fn tag_name(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(tag_name(&host.document, this))
    }

    fn id(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(reflect(host, this, Atom::ID))
    }

    fn set_id(host: &mut Host, this: NodeId, value: String) -> Result<()> {
        set_reflected(host, this, Atom::ID, &value);
        Ok(())
    }

    fn class_name(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(reflect(host, this, Atom::CLASS))
    }

    fn set_class_name(host: &mut Host, this: NodeId, value: String) -> Result<()> {
        set_reflected(host, this, Atom::CLASS, &value);
        Ok(())
    }

    fn has_attributes(host: &mut Host, this: NodeId) -> Result<bool> {
        Ok(host.document.attributes(this).next().is_some())
    }

    fn get_attribute_names(host: &mut Host, this: NodeId) -> Result<Vec<String>> {
        let document = &host.document;
        Ok(document
            .attributes(this)
            .map(|(name, _)| document.atoms().name(name.local).to_owned())
            .collect())
    }

    fn get_attribute(
        host: &mut Host,
        this: NodeId,
        qualified_name: String,
    ) -> Result<Option<String>> {
        let document = &host.document;
        Ok(existing_attribute(document, this, &qualified_name)
            .and_then(|name| document.qualified_attribute(this, name))
            .map(str::to_owned))
    }

    fn set_attribute(
        host: &mut Host,
        this: NodeId,
        qualified_name: String,
        value: String,
    ) -> Result<()> {
        if !valid_name(&qualified_name) {
            return Err(Exception::dom(
                "InvalidCharacterError",
                format!("\"{qualified_name}\" is not a valid attribute name"),
            ));
        }
        let name = attribute_name(&mut host.document, this, &qualified_name);
        host.document.set_attribute(this, name, &value);
        host.mutated = true;
        Ok(())
    }

    fn remove_attribute(host: &mut Host, this: NodeId, qualified_name: String) -> Result<()> {
        if let Some(name) = existing_attribute(&host.document, this, &qualified_name) {
            host.document.remove_attribute(this, name);
            host.mutated = true;
        }
        Ok(())
    }

    fn toggle_attribute(
        host: &mut Host,
        this: NodeId,
        qualified_name: String,
        force: Option<bool>,
    ) -> Result<bool> {
        if !valid_name(&qualified_name) {
            return Err(Exception::dom(
                "InvalidCharacterError",
                format!("\"{qualified_name}\" is not a valid attribute name"),
            ));
        }
        let present = existing_attribute(&host.document, this, &qualified_name);
        match (present, force) {
            (Some(name), None | Some(false)) => {
                host.document.remove_attribute(this, name);
                host.mutated = true;
                Ok(false)
            }
            (Some(_), Some(true)) => Ok(true),
            (None, None | Some(true)) => {
                let name = attribute_name(&mut host.document, this, &qualified_name);
                host.document.set_attribute(this, name, "");
                host.mutated = true;
                Ok(true)
            }
            (None, Some(false)) => Ok(false),
        }
    }

    fn has_attribute(host: &mut Host, this: NodeId, qualified_name: String) -> Result<bool> {
        Ok(existing_attribute(&host.document, this, &qualified_name).is_some())
    }

    fn get_elements_by_tag_name(
        host: &mut Host,
        this: NodeId,
        qualified_name: String,
    ) -> Result<Vec<NodeId>> {
        Ok(elements_by_tag(&host.document, this, &qualified_name))
    }

    fn get_elements_by_class_name(
        host: &mut Host,
        this: NodeId,
        class_names: String,
    ) -> Result<Vec<NodeId>> {
        Ok(elements_by_class(&host.document, this, &class_names))
    }

    fn children(host: &mut Host, this: NodeId) -> Result<Vec<NodeId>> {
        let document = &host.document;
        Ok(document
            .children(this)
            .filter(|&child| is_element(document, child))
            .collect())
    }

    fn first_element_child(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        let document = &host.document;
        Ok(document
            .children(this)
            .find(|&child| is_element(document, child)))
    }

    fn last_element_child(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        let document = &host.document;
        let mut current = document.last_child(this);
        while let Some(node) = current {
            if is_element(document, node) {
                return Ok(Some(node));
            }
            current = document.previous_sibling(node);
        }
        Ok(None)
    }

    fn previous_element_sibling(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        let document = &host.document;
        let mut current = document.previous_sibling(this);
        while let Some(node) = current {
            if is_element(document, node) {
                return Ok(Some(node));
            }
            current = document.previous_sibling(node);
        }
        Ok(None)
    }

    fn next_element_sibling(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        let document = &host.document;
        let mut current = document.next_sibling(this);
        while let Some(node) = current {
            if is_element(document, node) {
                return Ok(Some(node));
            }
            current = document.next_sibling(node);
        }
        Ok(None)
    }

    fn child_element_count(host: &mut Host, this: NodeId) -> Result<u32> {
        let document = &host.document;
        Ok(document
            .children(this)
            .filter(|&child| is_element(document, child))
            .count() as u32)
    }

    fn remove(host: &mut Host, this: NodeId) -> Result<()> {
        if host.document.parent(this).is_some() {
            host.document.detach(this);
            host.mutated = true;
        }
        Ok(())
    }
}

impl CharacterDataApi for Api {
    fn data(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(host.document.text(this).unwrap_or_default().to_owned())
    }

    fn set_data(host: &mut Host, this: NodeId, value: String) -> Result<()> {
        host.document.set_text(this, &value);
        host.mutated = true;
        Ok(())
    }

    fn length(host: &mut Host, this: NodeId) -> Result<u32> {
        Ok(utf16_length(host.document.text(this).unwrap_or_default()))
    }

    fn append_data(host: &mut Host, this: NodeId, data: String) -> Result<()> {
        let mut text = host.document.text(this).unwrap_or_default().to_owned();
        text.push_str(&data);
        host.document.set_text(this, &text);
        host.mutated = true;
        Ok(())
    }
}

impl TextApi for Api {
    fn whole_text(host: &mut Host, this: NodeId) -> Result<String> {
        let document = &host.document;
        let is_text = |node: NodeId| document.kind(node) == Some(NodeKind::Text);
        let mut start = this;
        while let Some(previous) = document
            .previous_sibling(start)
            .filter(|&node| is_text(node))
        {
            start = previous;
        }
        let mut text = String::new();
        let mut current = Some(start);
        while let Some(node) = current.filter(|&node| is_text(node)) {
            text.push_str(document.text(node).unwrap_or_default());
            current = document.next_sibling(node);
        }
        Ok(text)
    }
}

impl HTMLElementApi for Api {
    fn title(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(reflect(host, this, Atom::TITLE))
    }

    fn set_title(host: &mut Host, this: NodeId, value: String) -> Result<()> {
        set_reflected(host, this, Atom::TITLE, &value);
        Ok(())
    }

    fn lang(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(reflect(host, this, Atom::LANG))
    }

    fn set_lang(host: &mut Host, this: NodeId, value: String) -> Result<()> {
        set_reflected(host, this, Atom::LANG, &value);
        Ok(())
    }

    fn hidden(host: &mut Host, this: NodeId) -> Result<bool> {
        Ok(host.document.attribute(this, Atom::HIDDEN).is_some())
    }

    fn set_hidden(host: &mut Host, this: NodeId, value: bool) -> Result<()> {
        let name = QualName::attribute(Atom::HIDDEN);
        if value {
            host.document.set_attribute(this, name, "");
        } else {
            host.document.remove_attribute(this, name);
        }
        host.mutated = true;
        Ok(())
    }
}

impl WindowApi for Api {
    fn window(_host: &mut Host, _this: ()) -> Result<Global> {
        Ok(Global)
    }

    fn self_(_host: &mut Host, _this: ()) -> Result<Global> {
        Ok(Global)
    }

    fn document(_host: &mut Host, _this: ()) -> Result<NodeId> {
        Ok(NodeId::DOCUMENT)
    }
}

fn log(host: &mut Host, level: &str, data: &[JsValue]) {
    let text: Vec<String> = data
        .iter()
        .map(|value| match value.as_string() {
            Some(string) => string.to_std_string_escaped(),
            None => value.display().to_string(),
        })
        .collect();
    let line = format!("{level}{}", text.join(" "));
    if host.console.len() < CONSOLE_LIMIT {
        host.console.push(line);
    }
}

const CONSOLE_LIMIT: usize = 1000;

impl ConsoleApi for Api {
    fn log(host: &mut Host, _this: (), data: Vec<JsValue>) -> Result<()> {
        log(host, "", &data);
        Ok(())
    }

    fn info(host: &mut Host, _this: (), data: Vec<JsValue>) -> Result<()> {
        log(host, "", &data);
        Ok(())
    }

    fn warn(host: &mut Host, _this: (), data: Vec<JsValue>) -> Result<()> {
        log(host, "warning: ", &data);
        Ok(())
    }

    fn error(host: &mut Host, _this: (), data: Vec<JsValue>) -> Result<()> {
        log(host, "error: ", &data);
        Ok(())
    }

    fn debug(host: &mut Host, _this: (), data: Vec<JsValue>) -> Result<()> {
        log(host, "", &data);
        Ok(())
    }
}
