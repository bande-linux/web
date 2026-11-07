use web_atoms::{Atom, Namespace, QualName};
use web_dom::{Document, NodeId, NodeKind};

use crate::dom::observer;
use crate::dom::ranges;
use crate::dom::traversal;
use crate::exception::Exception;
use crate::host::{self, Host};

pub type Result<T> = std::result::Result<T, Exception>;

pub fn is_element(document: &Document, node: NodeId) -> bool {
    document.kind(node) == Some(NodeKind::Element)
}

pub fn is_character_data(document: &Document, node: NodeId) -> bool {
    matches!(
        document.kind(node),
        Some(NodeKind::Text | NodeKind::Comment | NodeKind::ProcessingInstruction)
    )
}

pub fn descendants(document: &Document, root: NodeId) -> Vec<NodeId> {
    let mut found = Vec::new();
    let mut current = document.first_child(root);
    while let Some(node) = current {
        found.push(node);
        current = following(document, node, root);
    }
    found
}

pub fn inclusive_descendants(document: &Document, root: NodeId) -> Vec<NodeId> {
    let mut found = vec![root];
    found.extend(descendants(document, root));
    found
}

pub fn following(document: &Document, node: NodeId, root: NodeId) -> Option<NodeId> {
    if let Some(child) = document.first_child(node) {
        return Some(child);
    }
    following_skipping_children(document, node, root)
}

pub fn following_skipping_children(document: &Document, node: NodeId, root: NodeId) -> Option<NodeId> {
    let mut current = node;
    loop {
        if current == root {
            return None;
        }
        if let Some(next) = document.next_sibling(current) {
            return Some(next);
        }
        current = document.parent(current)?;
    }
}

pub fn preceding(document: &Document, node: NodeId, root: NodeId) -> Option<NodeId> {
    if node == root {
        return None;
    }
    match document.previous_sibling(node) {
        Some(previous) => {
            let mut current = previous;
            while let Some(last) = document.last_child(current) {
                current = last;
            }
            Some(current)
        }
        None => document.parent(node),
    }
}

pub fn root_of(document: &Document, node: NodeId) -> NodeId {
    document.ancestors(node).last().unwrap_or(node)
}

pub fn is_inclusive_ancestor(document: &Document, ancestor: NodeId, node: NodeId) -> bool {
    document.ancestors(node).any(|known| known == ancestor)
}

pub fn is_connected(document: &Document, node: NodeId) -> bool {
    document.kind(root_of(document, node)) == Some(NodeKind::Document)
}

pub fn index_of(document: &Document, node: NodeId) -> u32 {
    let mut index = 0;
    let mut current = document.previous_sibling(node);
    while let Some(previous) = current {
        index += 1;
        current = document.previous_sibling(previous);
    }
    index
}

pub fn length_of(document: &Document, node: NodeId) -> u32 {
    match document.kind(node) {
        Some(NodeKind::Doctype | NodeKind::Attribute) => 0,
        Some(NodeKind::Text | NodeKind::Comment | NodeKind::ProcessingInstruction) => {
            utf16_length(document.text(node).unwrap_or_default())
        }
        _ => document.children(node).count() as u32,
    }
}

pub fn utf16_length(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

pub fn descendant_text(document: &Document, node: NodeId) -> String {
    let mut text = String::new();
    for child in descendants(document, node) {
        if document.kind(child) == Some(NodeKind::Text) {
            text.push_str(document.text(child).unwrap_or_default());
        }
    }
    text
}

fn hierarchy(message: &str) -> Exception {
    Exception::dom("HierarchyRequestError", message)
}

fn validity_for_document(
    document: &Document,
    parent: NodeId,
    node: NodeId,
    child: Option<NodeId>,
    replacing: Option<NodeId>,
) -> Result<()> {
    let elements_other = |ignore: Option<NodeId>| {
        document
            .children(parent)
            .any(|existing| is_element(document, existing) && Some(existing) != ignore)
    };
    let doctype_after = |reference: Option<NodeId>| {
        reference.is_some_and(|reference| {
            std::iter::successors(Some(reference), |&current| document.next_sibling(current))
                .any(|current| document.kind(current) == Some(NodeKind::Doctype))
        })
    };
    let element_before = |reference: Option<NodeId>| match reference {
        Some(reference) => std::iter::successors(document.previous_sibling(reference), |&current| {
            document.previous_sibling(current)
        })
        .any(|current| is_element(document, current)),
        None => document.children(parent).any(|existing| is_element(document, existing)),
    };
    let reference_after = replacing.map_or(child, |replaced| document.next_sibling(replaced));
    match document.kind(node) {
        Some(NodeKind::Fragment) => {
            let elements = document
                .children(node)
                .filter(|&existing| is_element(document, existing))
                .count();
            if elements > 1 || document.children(node).any(|existing| document.kind(existing) == Some(NodeKind::Text)) {
                return Err(hierarchy("a document cannot contain this fragment"));
            }
            if elements == 1 {
                let bad = match replacing {
                    Some(replaced) => elements_other(Some(replaced)) || doctype_after(reference_after),
                    None => {
                        elements_other(None)
                            || child.is_some_and(|child| document.kind(child) == Some(NodeKind::Doctype))
                            || doctype_after(child)
                    }
                };
                if bad {
                    return Err(hierarchy("a document has one element child"));
                }
            }
        }
        Some(NodeKind::Element) => {
            let bad = match replacing {
                Some(replaced) => elements_other(Some(replaced)) || doctype_after(reference_after),
                None => {
                    elements_other(None)
                        || child.is_some_and(|child| document.kind(child) == Some(NodeKind::Doctype))
                        || doctype_after(child)
                }
            };
            if bad {
                return Err(hierarchy("a document has one element child"));
            }
        }
        Some(NodeKind::Doctype) => {
            let bad = match replacing {
                Some(replaced) => {
                    document.children(parent).any(|existing| {
                        document.kind(existing) == Some(NodeKind::Doctype) && existing != replaced
                    }) || element_before(Some(replaced))
                }
                None => {
                    document
                        .children(parent)
                        .any(|existing| document.kind(existing) == Some(NodeKind::Doctype))
                        || element_before(child)
                }
            };
            if bad {
                return Err(hierarchy("a document has one doctype, before its element"));
            }
        }
        _ => {}
    }
    Ok(())
}

fn ensure_common(document: &Document, parent: NodeId, node: NodeId) -> Result<()> {
    if !matches!(
        document.kind(parent),
        Some(NodeKind::Document | NodeKind::Fragment | NodeKind::Element)
    ) {
        return Err(hierarchy("the parent cannot have children"));
    }
    if is_inclusive_ancestor(document, node, parent) {
        return Err(hierarchy("the node is an ancestor of the parent"));
    }
    Ok(())
}

fn ensure_kind(document: &Document, parent: NodeId, node: NodeId) -> Result<()> {
    match document.kind(node) {
        Some(
            NodeKind::Fragment
            | NodeKind::Doctype
            | NodeKind::Element
            | NodeKind::Text
            | NodeKind::ProcessingInstruction
            | NodeKind::Comment,
        ) => {}
        _ => return Err(hierarchy("this node cannot be inserted")),
    }
    let parent_is_document = document.kind(parent) == Some(NodeKind::Document);
    if (document.kind(node) == Some(NodeKind::Text) && parent_is_document)
        || (document.kind(node) == Some(NodeKind::Doctype) && !parent_is_document)
    {
        return Err(hierarchy("this node cannot be a child of the parent"));
    }
    Ok(())
}

pub fn ensure_insertable(host: &Host, parent: NodeId, node: NodeId, child: Option<NodeId>) -> Result<()> {
    let document = &host.document;
    ensure_common(document, parent, node)?;
    if child.is_some_and(|child| document.parent(child) != Some(parent)) {
        return Err(Exception::dom(
            "NotFoundError",
            "the reference is not a child of the parent",
        ));
    }
    ensure_kind(document, parent, node)?;
    if document.kind(parent) == Some(NodeKind::Document) {
        validity_for_document(document, parent, node, child, None)?;
    }
    Ok(())
}

pub fn pre_insert(host: &mut Host, node: NodeId, parent: NodeId, child: Option<NodeId>) -> Result<NodeId> {
    ensure_insertable(host, parent, node, child)?;
    let reference = if child == Some(node) {
        host.document.next_sibling(node)
    } else {
        child
    };
    let owner = host.node_document(parent);
    adopt(host, node, owner);
    insert(host, node, parent, reference, false);
    Ok(node)
}

pub fn insert(host: &mut Host, node: NodeId, parent: NodeId, child: Option<NodeId>, suppress: bool) {
    let is_fragment = host.document.kind(node) == Some(NodeKind::Fragment);
    let nodes: Vec<NodeId> = if is_fragment {
        host.document.children(node).collect()
    } else {
        vec![node]
    };
    if nodes.is_empty() {
        return;
    }
    if is_fragment {
        for &moving in &nodes {
            remove(host, moving, true);
        }
        observer::queue_tree(host, node, Vec::new(), nodes.clone(), None, None);
    }
    if let Some(child) = child {
        ranges::before_insert(host, parent, child, nodes.len() as u32);
    }
    let previous = match child {
        Some(child) => host.document.previous_sibling(child),
        None => host.document.last_child(parent),
    };
    for &moving in &nodes {
        let owner = host.node_document(parent);
        adopt(host, moving, owner);
        host.document.insert(parent, moving, child);
    }
    if !suppress {
        observer::queue_tree(host, parent, nodes.clone(), Vec::new(), previous, child);
    }
    host.touched();
    children_changed(host, parent);
    for &inserted in &nodes {
        inserted_steps(host, inserted);
    }
}

fn children_changed(host: &mut Host, parent: NodeId) {
    match host.document.html_element_name(parent) {
        Some(Atom::STYLE) => host.styles_changed = true,
        Some(Atom::SCRIPT) if is_connected(&host.document, parent) => schedule_script(host, parent),
        _ => {}
    }
}

fn inserted_steps(host: &mut Host, node: NodeId) {
    if !is_connected(&host.document, node) {
        return;
    }
    for current in inclusive_descendants(&host.document, node) {
        match host.document.html_element_name(current) {
            Some(Atom::SCRIPT) => schedule_script(host, current),
            Some(Atom::STYLE | Atom::LINK) => host.styles_changed = true,
            _ => {}
        }
    }
}

pub fn schedule_script(host: &mut Host, script: NodeId) {
    if host.started_scripts.contains(&script)
        || host.pending_scripts.contains(&script)
        || host.node_document(script) != NodeId::DOCUMENT
    {
        return;
    }
    host.pending_scripts.push(script);
    host::mark_pending();
}

pub fn remove(host: &mut Host, node: NodeId, suppress: bool) {
    let Some(parent) = host.document.parent(node) else {
        return;
    };
    let index = if host.ranges.is_empty() {
        0
    } else {
        index_of(&host.document, node)
    };
    ranges::before_remove(host, node, parent, index);
    traversal::before_remove(host, node);
    let previous = host.document.previous_sibling(node);
    let next = host.document.next_sibling(node);
    observer::before_remove(host, node, parent);
    host.document.detach(node);
    if !suppress {
        observer::queue_tree(host, parent, Vec::new(), vec![node], previous, next);
    }
    host.touched();
    children_changed(host, parent);
    let style_touched = inclusive_descendants(&host.document, node).into_iter().any(|current| {
        matches!(
            host.document.html_element_name(current),
            Some(Atom::STYLE | Atom::LINK)
        )
    });
    host.styles_changed |= style_touched;
}

pub fn pre_remove(host: &mut Host, child: NodeId, parent: NodeId) -> Result<NodeId> {
    if host.document.parent(child) != Some(parent) {
        return Err(Exception::dom("NotFoundError", "the node is not a child"));
    }
    remove(host, child, false);
    Ok(child)
}

pub fn replace(host: &mut Host, child: NodeId, node: NodeId, parent: NodeId) -> Result<NodeId> {
    let document = &host.document;
    ensure_common(document, parent, node)?;
    if document.parent(child) != Some(parent) {
        return Err(Exception::dom("NotFoundError", "the node to replace is not a child"));
    }
    ensure_kind(document, parent, node)?;
    if document.kind(parent) == Some(NodeKind::Document) {
        validity_for_document(document, parent, node, Some(child), Some(child))?;
    }
    let mut reference = document.next_sibling(child);
    if reference == Some(node) {
        reference = document.next_sibling(node);
    }
    let previous = document.previous_sibling(child);
    let owner = host.node_document(parent);
    adopt(host, node, owner);
    let mut removed = Vec::new();
    if host.document.parent(child).is_some() {
        removed.push(child);
        remove(host, child, true);
    }
    let nodes: Vec<NodeId> = if host.document.kind(node) == Some(NodeKind::Fragment) {
        host.document.children(node).collect()
    } else {
        vec![node]
    };
    insert(host, node, parent, reference, true);
    observer::queue_tree(host, parent, nodes, removed, previous, reference);
    Ok(child)
}

pub fn replace_all(host: &mut Host, node: Option<NodeId>, parent: NodeId) {
    let removed: Vec<NodeId> = host.document.children(parent).collect();
    let added: Vec<NodeId> = match node {
        Some(node) if host.document.kind(node) == Some(NodeKind::Fragment) => {
            host.document.children(node).collect()
        }
        Some(node) => vec![node],
        None => Vec::new(),
    };
    for &child in &removed {
        remove(host, child, true);
    }
    if let Some(node) = node {
        insert(host, node, parent, None, true);
    }
    if !added.is_empty() || !removed.is_empty() {
        observer::queue_tree(host, parent, added, removed, None, None);
    }
}

pub fn adopt(host: &mut Host, node: NodeId, owner: NodeId) {
    if host.document.parent(node).is_some() {
        remove(host, node, false);
    }
    let old = host.node_document(node);
    if old == owner {
        return;
    }
    for current in inclusive_descendants(&host.document, node) {
        host.set_owner(current, owner);
        let names: Vec<QualName> = host.document.attributes(current).map(|(name, _)| name).collect();
        for name in names {
            if let Some(&attribute) = host.attribute_nodes.get(&(current, name)) {
                host.set_owner(attribute, owner);
            }
        }
        if let Some(contents) = host.document.template_contents(current) {
            host.set_owner(contents, owner);
        }
    }
}

pub fn utf16_slice(text: &str, offset: u32, count: u32) -> String {
    let units: Vec<u16> = text.encode_utf16().skip(offset as usize).take(count as usize).collect();
    String::from_utf16_lossy(&units)
}

pub fn replace_data(host: &mut Host, node: NodeId, offset: u32, count: u32, data: &str) -> Result<()> {
    let old = host.document.text(node).unwrap_or_default().to_owned();
    let units: Vec<u16> = old.encode_utf16().collect();
    let length = units.len() as u32;
    if offset > length {
        return Err(Exception::dom("IndexSizeError", "the offset is past the end"));
    }
    let count = count.min(length - offset);
    observer::queue_character_data(host, node, &old);
    let start = offset as usize;
    let end = (offset + count) as usize;
    let mut merged: Vec<u16> = Vec::with_capacity(units.len() + data.len());
    merged.extend_from_slice(units.get(..start).unwrap_or_default());
    merged.extend(data.encode_utf16());
    merged.extend_from_slice(units.get(end..).unwrap_or_default());
    host.document.set_text(node, &String::from_utf16_lossy(&merged));
    ranges::after_replace_data(host, node, offset, count, utf16_length(data));
    host.touched();
    if let Some(parent) = host.document.parent(node) {
        children_changed(host, parent);
    }
    Ok(())
}

pub fn set_all_data(host: &mut Host, node: NodeId, data: &str) -> Result<()> {
    let length = utf16_length(host.document.text(node).unwrap_or_default());
    replace_data(host, node, 0, length, data)
}

pub fn namespace_uri(host: &Host, namespace: Namespace, other: Option<&String>) -> Option<String> {
    let _ = host;
    match namespace {
        Namespace::Html => Some("http://www.w3.org/1999/xhtml"),
        Namespace::Svg => Some("http://www.w3.org/2000/svg"),
        Namespace::MathMl => Some("http://www.w3.org/1998/Math/MathML"),
        Namespace::XLink => Some("http://www.w3.org/1999/xlink"),
        Namespace::Xml => Some("http://www.w3.org/XML/1998/namespace"),
        Namespace::Xmlns => Some("http://www.w3.org/2000/xmlns/"),
        Namespace::None => None,
        Namespace::Other => return other.cloned(),
    }
    .map(str::to_owned)
}

pub fn namespace_from(uri: Option<&str>) -> Namespace {
    match uri {
        None | Some("") => Namespace::None,
        Some("http://www.w3.org/1999/xhtml") => Namespace::Html,
        Some("http://www.w3.org/2000/svg") => Namespace::Svg,
        Some("http://www.w3.org/1998/Math/MathML") => Namespace::MathMl,
        Some("http://www.w3.org/1999/xlink") => Namespace::XLink,
        Some("http://www.w3.org/XML/1998/namespace") => Namespace::Xml,
        Some("http://www.w3.org/2000/xmlns/") => Namespace::Xmlns,
        Some(_) => Namespace::Other,
    }
}

pub fn element_namespace(host: &Host, element: NodeId) -> Option<String> {
    let (namespace, _) = host.document.element_name(element)?;
    namespace_uri(host, namespace, host.other_namespaces.get(&element))
}

pub fn attribute_namespace(host: &Host, element: NodeId, name: QualName) -> Option<String> {
    namespace_uri(host, name.ns, host.other_attribute_namespaces.get(&(element, name)))
}

pub fn attribute_prefix(host: &Host, element: NodeId, name: QualName) -> Option<String> {
    if let Some(&prefix) = host.attribute_prefixes.get(&(element, name)) {
        return Some(host.document.atoms().name(prefix).to_owned());
    }
    match name.ns {
        Namespace::XLink => Some("xlink".to_owned()),
        Namespace::Xml => Some("xml".to_owned()),
        Namespace::Xmlns if host.document.atoms().name(name.local) != "xmlns" => {
            Some("xmlns".to_owned())
        }
        _ => None,
    }
}

pub fn attribute_qualified_name(host: &Host, element: NodeId, name: QualName) -> String {
    let local = host.document.atoms().name(name.local);
    match attribute_prefix(host, element, name) {
        Some(prefix) => format!("{prefix}:{local}"),
        None => local.to_owned(),
    }
}

pub fn element_qualified_name(host: &Host, element: NodeId) -> String {
    let Some((_, local)) = host.document.element_name(element) else {
        return String::new();
    };
    let local = host.document.atoms().name(local);
    match host.element_prefixes.get(&element) {
        Some(&prefix) => format!("{}:{local}", host.document.atoms().name(prefix)),
        None => local.to_owned(),
    }
}

pub fn lowercases_names(host: &Host, element: NodeId) -> bool {
    matches!(host.document.element_name(element), Some((Namespace::Html, _)))
        && host.is_html(host.owner(element))
}

pub fn find_attribute(host: &Host, element: NodeId, qualified: &str) -> Option<QualName> {
    let lowered;
    let wanted = if lowercases_names(host, element) {
        lowered = qualified.to_ascii_lowercase();
        lowered.as_str()
    } else {
        qualified
    };
    host.document
        .attributes(element)
        .map(|(name, _)| name)
        .find(|&name| attribute_qualified_name(host, element, name) == wanted)
}

pub fn find_attribute_ns(host: &Host, element: NodeId, namespace: Option<&str>, local: &str) -> Option<QualName> {
    let namespace = namespace.filter(|uri| !uri.is_empty());
    let atom = host.document.atoms().get(local)?;
    host.document
        .attributes(element)
        .map(|(name, _)| name)
        .find(|&name| {
            name.local == atom && attribute_namespace(host, element, name).as_deref() == namespace
        })
}

pub fn change_attribute(host: &mut Host, element: NodeId, name: QualName, value: &str) {
    let old = host.document.qualified_attribute(element, name).map(str::to_owned);
    observer::queue_attribute(host, element, name, old.as_deref());
    host.document.set_attribute(element, name, value);
    attribute_steps(host, element, name, old.as_deref(), Some(value));
}

pub fn drop_attribute(host: &mut Host, element: NodeId, name: QualName) {
    let Some(old) = host.document.qualified_attribute(element, name).map(str::to_owned) else {
        return;
    };
    observer::queue_attribute(host, element, name, Some(&old));
    if let Some(attribute) = host.attribute_nodes.remove(&(element, name)) {
        host.document.set_attribute_node_value(attribute, &old);
        host.attribute_owners.remove(&attribute);
        if let Some(uri) = host.other_attribute_namespaces.get(&(element, name)).cloned() {
            host.detached_attribute_namespace.insert(attribute, uri);
        }
        if let Some(&prefix) = host.attribute_prefixes.get(&(element, name)) {
            host.detached_attribute_prefix.insert(attribute, prefix);
        }
    }
    host.attribute_prefixes.remove(&(element, name));
    host.other_attribute_namespaces.remove(&(element, name));
    host.document.remove_attribute(element, name);
    attribute_steps(host, element, name, Some(&old), None);
}

fn attribute_steps(host: &mut Host, element: NodeId, name: QualName, old: Option<&str>, new: Option<&str>) {
    host.touched();
    if name.ns != Namespace::None && name.ns != Namespace::Html {
        return;
    }
    let local = host.document.atoms().name(name.local).to_owned();
    if let Some(event) = local.strip_prefix("on") {
        crate::events::attribute_changed(host, element, event, new);
    }
    match host.document.html_element_name(element) {
        Some(Atom::LINK) | Some(Atom::STYLE) => host.styles_changed = true,
        Some(Atom::SCRIPT) if local == "src" && old.is_none() && new.is_some() => {
            if is_connected(&host.document, element) {
                schedule_script(host, element);
            }
        }
        _ => {}
    }
}

pub fn valid_element_local(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|ch| matches!(ch, '\0' | '\t' | '\n' | '\u{c}' | '\r' | ' ' | '/' | '>'))
}

pub fn valid_element_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {
            !name.chars().any(|ch| matches!(ch, '\0' | '\t' | '\n' | '\u{c}' | '\r' | ' ' | '/' | '>'))
        }
        Some(first) if first == ':' || first == '_' || (first as u32) >= 0x80 => chars.all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | ':' | '_') || (ch as u32) >= 0x80
        }),
        _ => false,
    }
}

pub fn valid_attribute_name(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|ch| matches!(ch, '\0' | '\t' | '\n' | '\u{c}' | '\r' | ' ' | '/' | '=' | '>'))
}

pub fn valid_doctype_name(name: &str) -> bool {
    !name
        .chars()
        .any(|ch| matches!(ch, '\0' | '\t' | '\n' | '\u{c}' | '\r' | ' ' | '>'))
}

pub struct Extracted {
    pub namespace: Option<String>,
    pub prefix: Option<String>,
    pub local: String,
}

pub fn validate_and_extract(
    namespace: Option<&str>,
    qualified: &str,
    element: bool,
) -> Result<Extracted> {
    let namespace = namespace.filter(|uri| !uri.is_empty());
    let (prefix, local) = match qualified.split_once(':') {
        Some((prefix, local)) => (Some(prefix), local),
        None => (None, qualified),
    };
    let invalid = |message: &str| Exception::dom("InvalidCharacterError", message);
    if let Some(prefix) = prefix
        && (prefix.is_empty()
            || prefix
                .chars()
                .any(|ch| matches!(ch, '\0' | '\t' | '\n' | '\u{c}' | '\r' | ' ' | '/' | '>')))
    {
        return Err(invalid("the prefix is not valid"));
    }
    let valid = if element {
        if prefix.is_some() {
            valid_element_local(local)
        } else {
            valid_element_name(local)
        }
    } else {
        valid_attribute_name(local)
    };
    if !valid {
        return Err(invalid("the name is not valid"));
    }
    let namespace_error = |message: &str| Exception::dom("NamespaceError", message);
    if prefix.is_some() && namespace.is_none() {
        return Err(namespace_error("a prefix needs a namespace"));
    }
    if prefix == Some("xml") && namespace != Some("http://www.w3.org/XML/1998/namespace") {
        return Err(namespace_error("the xml prefix is reserved"));
    }
    let xmlns = namespace == Some("http://www.w3.org/2000/xmlns/");
    if (qualified == "xmlns" || prefix == Some("xmlns")) && !xmlns {
        return Err(namespace_error("the xmlns name is reserved"));
    }
    if xmlns && qualified != "xmlns" && prefix != Some("xmlns") {
        return Err(namespace_error("the xmlns namespace needs the xmlns name"));
    }
    Ok(Extracted {
        namespace: namespace.map(str::to_owned),
        prefix: prefix.map(str::to_owned),
        local: local.to_owned(),
    })
}

pub fn create_element(
    host: &mut Host,
    owner: NodeId,
    namespace: Option<&str>,
    prefix: Option<&str>,
    local: &str,
) -> NodeId {
    let ns = namespace_from(namespace);
    let atom = host.document.intern(local);
    let element = host
        .document
        .create_element(QualName { ns, local: atom }, std::iter::empty());
    host.set_owner(element, owner);
    if ns == Namespace::Other
        && let Some(namespace) = namespace
    {
        host.other_namespaces.insert(element, namespace.to_owned());
    }
    if let Some(prefix) = prefix {
        let prefix = host.document.intern(prefix);
        host.element_prefixes.insert(element, prefix);
    }
    element
}

pub fn clone_node(host: &mut Host, node: NodeId, owner: NodeId, subtree: bool) -> NodeId {
    let mut stack = vec![(node, None::<NodeId>)];
    let mut root = node;
    let mut first = true;
    while let Some((source, parent)) = stack.pop() {
        let copy = clone_single(host, source, owner);
        if first {
            root = copy;
            first = false;
        }
        if let Some(parent) = parent {
            host.document.insert(parent, copy, None);
        }
        if subtree {
            if let Some(contents) = host.document.template_contents(source) {
                let target = template_contents_of(host, copy, owner);
                let children: Vec<NodeId> = host.document.children(contents).collect();
                for child in children.into_iter().rev() {
                    stack.push((child, Some(target)));
                }
            }
            let children: Vec<NodeId> = host.document.children(source).collect();
            for child in children.into_iter().rev() {
                stack.push((child, Some(copy)));
            }
        }
    }
    root
}

pub fn template_contents_of(host: &mut Host, template: NodeId, owner: NodeId) -> NodeId {
    let contents = web_html::TreeSink::template_contents(&mut host.document, template);
    host.set_owner(contents, owner);
    contents
}

fn clone_single(host: &mut Host, node: NodeId, owner: NodeId) -> NodeId {
    let document = &host.document;
    let copy = match document.kind(node) {
        Some(NodeKind::Element) => {
            let Some((ns, local)) = document.element_name(node) else {
                return node;
            };
            let attributes: Vec<(QualName, String)> = document
                .attributes(node)
                .map(|(name, value)| (name, value.to_owned()))
                .collect();
            let copy = host.document.create_element(
                QualName { ns, local },
                attributes.iter().map(|(name, value)| (*name, value.as_str())),
            );
            if let Some(uri) = host.other_namespaces.get(&node).cloned() {
                host.other_namespaces.insert(copy, uri);
            }
            if let Some(&prefix) = host.element_prefixes.get(&node) {
                host.element_prefixes.insert(copy, prefix);
            }
            for (name, _) in &attributes {
                if let Some(&prefix) = host.attribute_prefixes.get(&(node, *name)) {
                    host.attribute_prefixes.insert((copy, *name), prefix);
                }
                if let Some(uri) = host.other_attribute_namespaces.get(&(node, *name)).cloned() {
                    host.other_attribute_namespaces.insert((copy, *name), uri);
                }
            }
            if host.started_scripts.contains(&node) {
                host.started_scripts.insert(copy);
            }
            copy
        }
        Some(NodeKind::Text) => {
            let text = document.text(node).unwrap_or_default().to_owned();
            let copy = host.document.create_text(&text);
            if host.cdata.contains(&node) {
                host.cdata.insert(copy);
            }
            copy
        }
        Some(NodeKind::Comment) => {
            let text = document.text(node).unwrap_or_default().to_owned();
            host.document.create_comment(&text)
        }
        Some(NodeKind::ProcessingInstruction) => {
            let target = document.processing_instruction_target(node).unwrap_or_default().to_owned();
            let text = document.text(node).unwrap_or_default().to_owned();
            host.document.create_processing_instruction(&target, &text)
        }
        Some(NodeKind::Doctype) => {
            let name = document.doctype_name(node).unwrap_or_default().to_owned();
            let (public, system) = document.doctype_ids(node);
            let (public, system) = (public.to_owned(), system.to_owned());
            host.document.create_doctype(&name, &public, &system)
        }
        Some(NodeKind::Fragment) => host.document.create_fragment(),
        Some(NodeKind::Document) => {
            let copy = host.document.create_document_node();
            let info = host.documents.get(&node).map(|info| crate::host::DocumentInfo {
                interface: info.interface,
                html: info.html,
                content_type: info.content_type.clone(),
            });
            if let Some(info) = info {
                host.documents.insert(copy, info);
            }
            copy
        }
        Some(NodeKind::Attribute) => {
            let name = document.attribute_node_name(node).unwrap_or(QualName::attribute(Atom::ID));
            let value = crate::dom::attr::value_of(host, node);
            let copy = host.document.create_attribute_node(name, &value);
            let (uri, prefix) = crate::dom::attr::identity_of(host, node);
            if let Some(uri) = uri {
                host.detached_attribute_namespace.insert(copy, uri);
            }
            if let Some(prefix) = prefix {
                let prefix = host.document.intern(&prefix);
                host.detached_attribute_prefix.insert(copy, prefix);
            }
            copy
        }
        None => return node,
    };
    if host.document.kind(copy) != Some(NodeKind::Document) {
        host.set_owner(copy, owner);
    }
    copy
}
