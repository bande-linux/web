pub mod abort;
pub mod attr;
pub mod character_data;
pub mod collections;
pub mod document;
pub mod element;
pub mod markup;
pub mod observer;
pub mod ranges;
pub mod traversal;
pub mod tree;

use web_atoms::{Atom, Namespace};
use web_dom::{NodeId, NodeKind};

use crate::bindings::*;
use crate::dom::tree::{
    Result, descendant_text, descendants, inclusive_descendants, is_character_data, is_element,
    is_inclusive_ancestor, preceding, root_of,
};
use crate::engine::JsObject;
use crate::host::Host;

pub struct Api;

impl NodeApi for Api {
    fn node_type(host: &mut Host, this: NodeId) -> Result<u16> {
        Ok(match host.document.kind(this) {
            Some(NodeKind::Element) => 1,
            Some(NodeKind::Attribute) => 2,
            Some(NodeKind::Text) => {
                if host.cdata.contains(&this) {
                    4
                } else {
                    3
                }
            }
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
            Some(NodeKind::Element) => element::tag_name(host, this),
            Some(NodeKind::Attribute) => attr::qualified_name(host, this),
            Some(NodeKind::Text) => {
                if host.cdata.contains(&this) {
                    "#cdata-section".to_owned()
                } else {
                    "#text".to_owned()
                }
            }
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

    fn base_uri(host: &mut Host, this: NodeId) -> Result<String> {
        let owner = host.node_document(this);
        Ok(crate::window::document_base(host, owner))
    }

    fn is_connected(host: &mut Host, this: NodeId) -> Result<bool> {
        Ok(tree::is_connected(&host.document, this))
    }

    fn owner_document(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok((host.document.kind(this) != Some(NodeKind::Document)).then(|| host.owner(this)))
    }

    fn get_root_node(host: &mut Host, this: NodeId, _options: GetRootNodeOptions) -> Result<NodeId> {
        Ok(root_of(&host.document, this))
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

    fn child_nodes(context: &mut crate::engine::Context, this: NodeId) -> Result<JsObject> {
        collections::same_object(context, this, collections::Slot::ChildNodes)
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
        Ok(match host.document.kind(this) {
            Some(NodeKind::Attribute) => Some(attr::value_of(host, this)),
            _ => host.document.text(this).map(str::to_owned),
        })
    }

    fn set_node_value(host: &mut Host, this: NodeId, value: Option<String>) -> Result<()> {
        let value = value.unwrap_or_default();
        if is_character_data(&host.document, this) {
            tree::set_all_data(host, this, &value)?;
        } else if host.document.kind(this) == Some(NodeKind::Attribute) {
            <Api as AttrApi>::set_value(host, this, value)?;
        }
        Ok(())
    }

    fn text_content(host: &mut Host, this: NodeId) -> Result<Option<String>> {
        let document = &host.document;
        Ok(match document.kind(this) {
            Some(NodeKind::Element | NodeKind::Fragment) => Some(descendant_text(document, this)),
            Some(NodeKind::Attribute) => Some(attr::value_of(host, this)),
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
                let node = (!value.is_empty()).then(|| {
                    let text = host.document.create_text(&value);
                    let owner = host.owner(this);
                    host.set_owner(text, owner);
                    text
                });
                tree::replace_all(host, node, this);
            }
            Some(NodeKind::Attribute) => <Api as AttrApi>::set_value(host, this, value)?,
            Some(NodeKind::Text | NodeKind::Comment | NodeKind::ProcessingInstruction) => {
                tree::set_all_data(host, this, &value)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn normalize(host: &mut Host, this: NodeId) -> Result<()> {
        for node in inclusive_descendants(&host.document, this) {
            if host.document.kind(node) != Some(NodeKind::Text)
                || host.document.parent(node).is_none()
            {
                continue;
            }
            let length = tree::length_of(&host.document, node);
            if length == 0 {
                tree::remove(host, node, false);
                continue;
            }
            let previous_is_text = host
                .document
                .previous_sibling(node)
                .is_some_and(|previous| host.document.kind(previous) == Some(NodeKind::Text));
            if previous_is_text {
                continue;
            }
            let mut following = Vec::new();
            let mut current = host.document.next_sibling(node);
            while let Some(next) = current.filter(|&next| host.document.kind(next) == Some(NodeKind::Text)) {
                following.push(next);
                current = host.document.next_sibling(next);
            }
            let mut data = String::new();
            for &next in &following {
                data.push_str(host.document.text(next).unwrap_or_default());
            }
            tree::replace_data(host, node, length, 0, &data)?;
            let mut offset = length;
            for next in following {
                ranges::after_normalize_merge(host, next, node, offset);
                offset += tree::length_of(&host.document, next);
                tree::remove(host, next, false);
            }
        }
        Ok(())
    }

    fn clone_node(host: &mut Host, this: NodeId, subtree: bool) -> Result<NodeId> {
        let owner = host.node_document(this);
        Ok(tree::clone_node(host, this, owner, subtree))
    }

    fn is_equal_node(host: &mut Host, this: NodeId, other_node: Option<NodeId>) -> Result<bool> {
        let Some(other) = other_node else {
            return Ok(false);
        };
        let mut stack = vec![(this, other)];
        while let Some((a, b)) = stack.pop() {
            if !equal_single(host, a, b) {
                return Ok(false);
            }
            let left: Vec<NodeId> = host.document.children(a).collect();
            let right: Vec<NodeId> = host.document.children(b).collect();
            if left.len() != right.len() {
                return Ok(false);
            }
            stack.extend(left.into_iter().zip(right));
        }
        Ok(true)
    }

    fn is_same_node(_host: &mut Host, this: NodeId, other_node: Option<NodeId>) -> Result<bool> {
        Ok(other_node == Some(this))
    }

    fn compare_document_position(host: &mut Host, this: NodeId, other: NodeId) -> Result<u16> {
        Ok(compare_position(host, this, other))
    }

    fn contains(host: &mut Host, this: NodeId, other: Option<NodeId>) -> Result<bool> {
        Ok(other.is_some_and(|other| is_inclusive_ancestor(&host.document, this, other)))
    }

    fn lookup_prefix(host: &mut Host, this: NodeId, namespace: Option<String>) -> Result<Option<String>> {
        let Some(namespace) = namespace.filter(|namespace| !namespace.is_empty()) else {
            return Ok(None);
        };
        let Some(start) = lookup_start(host, this) else {
            return Ok(None);
        };
        for element in host.document.ancestors(start) {
            if !is_element(&host.document, element) {
                continue;
            }
            if tree::element_namespace(host, element).as_deref() == Some(&namespace)
                && let Some(&prefix) = host.element_prefixes.get(&element)
            {
                return Ok(Some(host.document.atoms().name(prefix).to_owned()));
            }
            for (name, value) in host.document.attributes(element) {
                if value == namespace && tree::attribute_prefix(host, element, name).as_deref() == Some("xmlns") {
                    return Ok(Some(host.document.atoms().name(name.local).to_owned()));
                }
            }
        }
        Ok(None)
    }

    fn lookup_namespace_uri(host: &mut Host, this: NodeId, prefix: Option<String>) -> Result<Option<String>> {
        let prefix = prefix.filter(|prefix| !prefix.is_empty());
        Ok(locate_namespace(host, this, prefix.as_deref()))
    }

    fn is_default_namespace(host: &mut Host, this: NodeId, namespace: Option<String>) -> Result<bool> {
        let namespace = namespace.filter(|namespace| !namespace.is_empty());
        Ok(locate_namespace(host, this, None) == namespace)
    }

    fn insert_before(host: &mut Host, this: NodeId, node: NodeId, child: Option<NodeId>) -> Result<NodeId> {
        tree::pre_insert(host, node, this, child)
    }

    fn append_child(host: &mut Host, this: NodeId, node: NodeId) -> Result<NodeId> {
        tree::pre_insert(host, node, this, None)
    }

    fn replace_child(host: &mut Host, this: NodeId, node: NodeId, child: NodeId) -> Result<NodeId> {
        tree::replace(host, child, node, this)
    }

    fn remove_child(host: &mut Host, this: NodeId, child: NodeId) -> Result<NodeId> {
        tree::pre_remove(host, child, this)
    }
}

fn lookup_start(host: &Host, node: NodeId) -> Option<NodeId> {
    match host.document.kind(node)? {
        NodeKind::Element => Some(node),
        NodeKind::Document => host
            .document
            .children(node)
            .find(|&child| is_element(&host.document, child)),
        NodeKind::Doctype | NodeKind::Fragment => None,
        NodeKind::Attribute => host.attribute_owners.get(&node).copied(),
        _ => host
            .document
            .parent(node)
            .filter(|&parent| is_element(&host.document, parent)),
    }
}

pub fn locate_namespace(host: &Host, node: NodeId, prefix: Option<&str>) -> Option<String> {
    let start = lookup_start(host, node)?;
    for element in host.document.ancestors(start) {
        if !is_element(&host.document, element) {
            return None;
        }
        let own_prefix = host
            .element_prefixes
            .get(&element)
            .map(|&atom| host.document.atoms().name(atom));
        if let Some(namespace) = tree::element_namespace(host, element)
            && own_prefix == prefix
        {
            return Some(namespace);
        }
        for (name, value) in host.document.attributes(element) {
            let is_xmlns_namespace = name.ns == Namespace::Xmlns;
            let local = host.document.atoms().name(name.local);
            let attribute_prefix = tree::attribute_prefix(host, element, name);
            if is_xmlns_namespace
                && ((attribute_prefix.as_deref() == Some("xmlns") && Some(local) == prefix)
                    || (prefix.is_none() && attribute_prefix.is_none() && local == "xmlns"))
            {
                return (!value.is_empty()).then(|| value.to_owned());
            }
        }
    }
    None
}

fn equal_single(host: &Host, a: NodeId, b: NodeId) -> bool {
    let document = &host.document;
    if document.kind(a) != document.kind(b) {
        return false;
    }
    match document.kind(a) {
        Some(NodeKind::Element) => {
            if document.element_name(a).map(|(_, name)| name) != document.element_name(b).map(|(_, name)| name)
                || tree::element_namespace(host, a) != tree::element_namespace(host, b)
                || host.element_prefixes.get(&a) != host.element_prefixes.get(&b)
            {
                return false;
            }
            let left: Vec<_> = document.attributes(a).collect();
            let right: Vec<_> = document.attributes(b).collect();
            left.len() == right.len()
                && left.iter().all(|&(name, value)| {
                    let uri = tree::attribute_namespace(host, a, name);
                    right.iter().any(|&(other, other_value)| {
                        other.local == name.local
                            && other_value == value
                            && tree::attribute_namespace(host, b, other) == uri
                    })
                })
        }
        Some(NodeKind::Doctype) => {
            document.doctype_name(a) == document.doctype_name(b)
                && document.doctype_ids(a) == document.doctype_ids(b)
        }
        Some(NodeKind::ProcessingInstruction) => {
            document.processing_instruction_target(a) == document.processing_instruction_target(b)
                && document.text(a) == document.text(b)
        }
        Some(NodeKind::Text | NodeKind::Comment) => document.text(a) == document.text(b),
        Some(NodeKind::Attribute) => {
            attr::identity_of(host, a).0 == attr::identity_of(host, b).0
                && document.attribute_node_name(a).map(|name| name.local)
                    == document.attribute_node_name(b).map(|name| name.local)
                && attr::value_of(host, a) == attr::value_of(host, b)
        }
        _ => true,
    }
}

pub fn compare_position(host: &Host, this: NodeId, other: NodeId) -> u16 {
    if this == other {
        return 0;
    }
    const DISCONNECTED: u16 = 1;
    const PRECEDING: u16 = 2;
    const FOLLOWING: u16 = 4;
    const CONTAINS: u16 = 8;
    const CONTAINED_BY: u16 = 16;
    const SPECIFIC: u16 = 32;
    let document = &host.document;
    let attr_owner = |node: NodeId| host.attribute_owners.get(&node).copied();
    let attr1 = (document.kind(other) == Some(NodeKind::Attribute)).then_some(other);
    let attr2 = (document.kind(this) == Some(NodeKind::Attribute)).then_some(this);
    let node1 = attr1.map_or(Some(other), attr_owner);
    let node2 = attr2.map_or(Some(this), attr_owner);
    if let (Some(a1), Some(a2), Some(o1), Some(o2)) = (attr1, attr2, node1, node2)
        && o1 == o2
    {
        for (name, _) in document.attributes(o2) {
            let known = host.attribute_nodes.get(&(o2, name)).copied();
            if known == Some(a1) {
                return SPECIFIC + PRECEDING;
            }
            if known == Some(a2) {
                return SPECIFIC + FOLLOWING;
            }
        }
        return SPECIFIC + PRECEDING;
    }
    let arbitrary = if other.index() < this.index() { PRECEDING } else { FOLLOWING };
    let (Some(node1), Some(node2)) = (node1, node2) else {
        return DISCONNECTED + SPECIFIC + arbitrary;
    };
    if root_of(document, node1) != root_of(document, node2) {
        return DISCONNECTED + SPECIFIC + arbitrary;
    }
    if attr1.is_none() && is_inclusive_ancestor(document, node1, node2) && (attr2.is_none() || node1 != node2) && node1 != node2 {
        return CONTAINS + PRECEDING;
    }
    if attr1.is_none() && attr2.is_some() && node1 == node2 {
        return CONTAINS + PRECEDING;
    }
    if attr2.is_none() && is_inclusive_ancestor(document, node2, node1) && node1 != node2 {
        return CONTAINED_BY + FOLLOWING;
    }
    if attr2.is_none() && attr1.is_some() && node1 == node2 {
        return CONTAINED_BY + FOLLOWING;
    }
    if position_before(document, node1, node2) {
        PRECEDING
    } else {
        FOLLOWING
    }
}

pub fn position_before(document: &web_dom::Document, first: NodeId, second: NodeId) -> bool {
    let chain = |node: NodeId| {
        let mut list: Vec<NodeId> = document.ancestors(node).collect();
        list.reverse();
        list
    };
    let left = chain(first);
    let right = chain(second);
    for (a, b) in left.iter().zip(right.iter()) {
        if a != b {
            return tree::index_of(document, *a) < tree::index_of(document, *b);
        }
    }
    left.len() < right.len()
}

impl EventTargetApi for Api {
    fn constructor(_context: &mut crate::engine::Context, prototype: JsObject) -> Result<JsObject> {
        Ok(JsObject::from_proto_and_data(
            prototype,
            crate::convert::Platform::new(Interface::EventTarget, crate::convert::Payload::Plain),
        ))
    }

    fn add_event_listener(
        host: &mut Host,
        this: JsObject,
        type_: String,
        callback: Option<JsObject>,
        options: UnionAddEventListenerOptionsOrBoolean,
    ) -> Result<()> {
        crate::events::add_listener(host, &this, type_, callback, options)
    }

    fn remove_event_listener(
        host: &mut Host,
        this: JsObject,
        type_: String,
        callback: Option<JsObject>,
        options: UnionEventListenerOptionsOrBoolean,
    ) -> Result<()> {
        crate::events::remove_listener(host, &this, &type_, callback.as_ref(), options);
        Ok(())
    }

    fn dispatch_event(context: &mut crate::engine::Context, this: JsObject, event: JsObject) -> Result<bool> {
        crate::events::dispatch_from_script(context, &this, &event)
    }
}

pub fn convert_nodes(host: &mut Host, nodes: Vec<UnionNodeOrDOMString>, owner: NodeId) -> NodeId {
    let mut converted: Vec<NodeId> = nodes
        .into_iter()
        .map(|node| match node {
            UnionNodeOrDOMString::Node(node) => node,
            UnionNodeOrDOMString::DOMString(text) => {
                let node = host.document.create_text(&text);
                host.set_owner(node, owner);
                node
            }
        })
        .collect();
    if converted.len() == 1
        && let Some(only) = converted.pop()
    {
        return only;
    }
    let fragment = host.document.create_fragment();
    host.set_owner(fragment, owner);
    for node in converted {
        if host.document.parent(node).is_some() {
            tree::remove(host, node, false);
        }
        tree::adopt(host, node, owner);
        host.document.insert(fragment, node, None);
    }
    fragment
}

pub fn parent_prepend(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
    let owner = host.node_document(this);
    check_nodes(host, this, &nodes, host.document.first_child(this))?;
    let node = convert_nodes(host, nodes, owner);
    let first = host.document.first_child(this);
    tree::pre_insert(host, node, this, first)?;
    Ok(())
}

fn check_nodes(host: &Host, parent: NodeId, nodes: &[UnionNodeOrDOMString], child: Option<NodeId>) -> Result<()> {
    let _ = (parent, child);
    for node in nodes {
        if let UnionNodeOrDOMString::Node(node) = node
            && host.document.kind(*node) == Some(NodeKind::Document)
        {
            return Err(crate::exception::Exception::dom(
                "HierarchyRequestError",
                "a document cannot be inserted",
            ));
        }
    }
    Ok(())
}

pub fn parent_append(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
    let owner = host.node_document(this);
    check_nodes(host, this, &nodes, None)?;
    let node = convert_nodes(host, nodes, owner);
    tree::pre_insert(host, node, this, None)?;
    Ok(())
}

pub fn parent_replace_children(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
    let owner = host.node_document(this);
    check_nodes(host, this, &nodes, None)?;
    let node = convert_nodes(host, nodes, owner);
    tree::ensure_insertable(host, this, node, None).or_else(|error| {
        let replaces_only_child_rule = host.document.kind(this) == Some(NodeKind::Document);
        if replaces_only_child_rule {
            let children: Vec<NodeId> = host.document.children(this).collect();
            let elements = host.document.kind(node) == Some(NodeKind::Element) && children.iter().any(|&child| is_element(&host.document, child));
            let doctype = host.document.kind(node) == Some(NodeKind::Doctype);
            if elements || doctype {
                return Ok(());
            }
        }
        Err(error)
    })?;
    tree::adopt(host, node, owner);
    tree::replace_all(host, Some(node), this);
    Ok(())
}

fn viable_previous(host: &Host, this: NodeId, nodes: &[UnionNodeOrDOMString]) -> Option<NodeId> {
    let excluded = |candidate: NodeId| {
        nodes
            .iter()
            .any(|node| matches!(node, UnionNodeOrDOMString::Node(node) if *node == candidate))
    };
    std::iter::successors(host.document.previous_sibling(this), |&node| host.document.previous_sibling(node))
        .find(|&node| !excluded(node))
}

fn viable_next(host: &Host, this: NodeId, nodes: &[UnionNodeOrDOMString]) -> Option<NodeId> {
    let excluded = |candidate: NodeId| {
        nodes
            .iter()
            .any(|node| matches!(node, UnionNodeOrDOMString::Node(node) if *node == candidate))
    };
    std::iter::successors(host.document.next_sibling(this), |&node| host.document.next_sibling(node))
        .find(|&node| !excluded(node))
}

pub fn child_before(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
    let Some(parent) = host.document.parent(this) else {
        return Ok(());
    };
    let previous = viable_previous(host, this, &nodes);
    let owner = host.node_document(this);
    check_nodes(host, parent, &nodes, None)?;
    let node = convert_nodes(host, nodes, owner);
    let reference = match previous {
        Some(previous) => host.document.next_sibling(previous),
        None => host.document.first_child(parent),
    };
    tree::pre_insert(host, node, parent, reference)?;
    Ok(())
}

pub fn child_after(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
    let Some(parent) = host.document.parent(this) else {
        return Ok(());
    };
    let next = viable_next(host, this, &nodes);
    let owner = host.node_document(this);
    check_nodes(host, parent, &nodes, None)?;
    let node = convert_nodes(host, nodes, owner);
    tree::pre_insert(host, node, parent, next)?;
    Ok(())
}

pub fn child_replace_with(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
    let Some(parent) = host.document.parent(this) else {
        return Ok(());
    };
    let next = viable_next(host, this, &nodes);
    let owner = host.node_document(this);
    check_nodes(host, parent, &nodes, None)?;
    let node = convert_nodes(host, nodes, owner);
    if host.document.parent(this) == Some(parent) {
        tree::replace(host, this, node, parent)?;
    } else {
        tree::pre_insert(host, node, parent, next)?;
    }
    Ok(())
}

pub fn first_element_child(host: &Host, this: NodeId) -> Option<NodeId> {
    host.document.children(this).find(|&child| is_element(&host.document, child))
}

pub fn last_element_child(host: &Host, this: NodeId) -> Option<NodeId> {
    std::iter::successors(host.document.last_child(this), |&node| host.document.previous_sibling(node))
        .find(|&node| is_element(&host.document, node))
}

pub fn previous_element_sibling(host: &Host, this: NodeId) -> Option<NodeId> {
    std::iter::successors(host.document.previous_sibling(this), |&node| host.document.previous_sibling(node))
        .find(|&node| is_element(&host.document, node))
}

pub fn next_element_sibling(host: &Host, this: NodeId) -> Option<NodeId> {
    std::iter::successors(host.document.next_sibling(this), |&node| host.document.next_sibling(node))
        .find(|&node| is_element(&host.document, node))
}

pub fn child_element_count(host: &Host, this: NodeId) -> u32 {
    host.document.children(this).filter(|&child| is_element(&host.document, child)).count() as u32
}

pub fn parent_query_selector(host: &mut Host, this: NodeId, selectors: &str) -> Result<Option<NodeId>> {
    let list = element::parse_selectors(host, selectors)?;
    let document = &host.document;
    Ok(descendants(document, this).into_iter().find(|&node| {
        is_element(document, node) && web_style::selector_matches(document, &list, node, Some(this))
    }))
}

pub fn parent_query_selector_all(host: &mut Host, this: NodeId, selectors: &str) -> Result<Vec<NodeId>> {
    let list = element::parse_selectors(host, selectors)?;
    let document = &host.document;
    Ok(descendants(document, this)
        .into_iter()
        .filter(|&node| {
            is_element(document, node) && web_style::selector_matches(document, &list, node, Some(this))
        })
        .collect())
}

pub fn simple_atom_of(host: &Host, name: &str) -> Option<Atom> {
    host.document.atoms().get(name)
}

pub fn preceding_in(host: &Host, node: NodeId, root: NodeId) -> Option<NodeId> {
    preceding(&host.document, node, root)
}
