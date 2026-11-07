use web_atoms::{Atom, Namespace, QualName};
use web_css::selectors::SelectorList;
use web_dom::{NodeId, NodeKind};

use crate::bindings::*;
use crate::dom::collections::{self, Filter, Slot};
use crate::dom::tree::{self, Result};
use crate::dom::{self, Api, attr, markup};
use crate::engine::{Context, JsObject};
use crate::exception::Exception;
use crate::host::Host;

pub fn tag_name(host: &Host, element: NodeId) -> String {
    let name = tree::element_qualified_name(host, element);
    if tree::lowercases_names(host, element) {
        name.to_ascii_uppercase()
    } else {
        name
    }
}

pub fn parse_selectors(host: &mut Host, selectors: &str) -> Result<SelectorList> {
    web_css::stylesheet::parse_selector_list(
        selectors,
        host.document.atoms_mut(),
        &Default::default(),
    )
    .map_err(|_| {
        Exception::dom(
            "SyntaxError",
            format!("'{selectors}' is not a valid selector"),
        )
    })
}

pub fn reflect(host: &Host, this: NodeId, name: Atom) -> String {
    host.document.attribute(this, name).unwrap_or_default().to_owned()
}

pub fn set_reflected(host: &mut Host, this: NodeId, name: Atom, value: &str) {
    tree::change_attribute(host, this, QualName::attribute(name), value);
}

fn check_name(name: &str) -> Result<()> {
    if tree::valid_attribute_name(name) {
        Ok(())
    } else {
        Err(Exception::dom(
            "InvalidCharacterError",
            format!("\"{name}\" is not a valid attribute name"),
        ))
    }
}

fn adjacent(host: &mut Host, this: NodeId, place: &str, node: NodeId) -> Result<Option<NodeId>> {
    match place.to_ascii_lowercase().as_str() {
        "beforebegin" => {
            let Some(parent) = host.document.parent(this) else {
                return Ok(None);
            };
            tree::pre_insert(host, node, parent, Some(this)).map(Some)
        }
        "afterbegin" => {
            let first = host.document.first_child(this);
            tree::pre_insert(host, node, this, first).map(Some)
        }
        "beforeend" => tree::pre_insert(host, node, this, None).map(Some),
        "afterend" => {
            let Some(parent) = host.document.parent(this) else {
                return Ok(None);
            };
            let next = host.document.next_sibling(this);
            tree::pre_insert(host, node, parent, next).map(Some)
        }
        _ => Err(Exception::dom("SyntaxError", "the position is not valid")),
    }
}

impl ElementApi for Api {
    fn namespace_uri(host: &mut Host, this: NodeId) -> Result<Option<String>> {
        Ok(tree::element_namespace(host, this))
    }

    fn prefix(host: &mut Host, this: NodeId) -> Result<Option<String>> {
        Ok(host
            .element_prefixes
            .get(&this)
            .map(|&prefix| host.document.atoms().name(prefix).to_owned()))
    }

    fn local_name(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(host
            .document
            .element_name(this)
            .map(|(_, name)| host.document.atoms().name(name).to_owned())
            .unwrap_or_default())
    }

    fn tag_name(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(tag_name(host, this))
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

    fn class_list(context: &mut Context, this: NodeId) -> Result<JsObject> {
        collections::same_object(context, this, Slot::ClassList)
    }

    fn slot(host: &mut Host, this: NodeId) -> Result<String> {
        let atom = host.document.intern("slot");
        Ok(reflect(host, this, atom))
    }

    fn set_slot(host: &mut Host, this: NodeId, value: String) -> Result<()> {
        let atom = host.document.intern("slot");
        set_reflected(host, this, atom, &value);
        Ok(())
    }

    fn has_attributes(host: &mut Host, this: NodeId) -> Result<bool> {
        Ok(host.document.attributes(this).next().is_some())
    }

    fn attributes(context: &mut Context, this: NodeId) -> Result<JsObject> {
        collections::same_object(context, this, Slot::Attributes)
    }

    fn get_attribute_names(host: &mut Host, this: NodeId) -> Result<Vec<String>> {
        let names: Vec<QualName> = host.document.attributes(this).map(|(name, _)| name).collect();
        Ok(names
            .into_iter()
            .map(|name| tree::attribute_qualified_name(host, this, name))
            .collect())
    }

    fn get_attribute(host: &mut Host, this: NodeId, qualified_name: String) -> Result<Option<String>> {
        Ok(tree::find_attribute(host, this, &qualified_name)
            .and_then(|name| host.document.qualified_attribute(this, name))
            .map(str::to_owned))
    }

    fn get_attribute_ns(host: &mut Host, this: NodeId, namespace: Option<String>, local_name: String) -> Result<Option<String>> {
        Ok(tree::find_attribute_ns(host, this, namespace.as_deref(), &local_name)
            .and_then(|name| host.document.qualified_attribute(this, name))
            .map(str::to_owned))
    }

    fn set_attribute(host: &mut Host, this: NodeId, qualified_name: String, value: String) -> Result<()> {
        check_name(&qualified_name)?;
        let wanted = if tree::lowercases_names(host, this) {
            qualified_name.to_ascii_lowercase()
        } else {
            qualified_name
        };
        let name = match tree::find_attribute(host, this, &wanted) {
            Some(name) => name,
            None => QualName {
                ns: Namespace::None,
                local: host.document.intern(&wanted),
            },
        };
        tree::change_attribute(host, this, name, &value);
        Ok(())
    }

    fn set_attribute_ns(host: &mut Host, this: NodeId, namespace: Option<String>, qualified_name: String, value: String) -> Result<()> {
        let extracted = tree::validate_and_extract(namespace.as_deref(), &qualified_name, false)?;
        let name = QualName {
            ns: tree::namespace_from(extracted.namespace.as_deref()),
            local: host.document.intern(&extracted.local),
        };
        if name.ns == Namespace::Other
            && let Some(uri) = extracted.namespace.clone()
        {
            host.other_attribute_namespaces.insert((this, name), uri);
        }
        if host.document.qualified_attribute(this, name).is_none()
            && let Some(prefix) = &extracted.prefix
        {
            let prefix = host.document.intern(prefix);
            host.attribute_prefixes.insert((this, name), prefix);
        }
        tree::change_attribute(host, this, name, &value);
        Ok(())
    }

    fn remove_attribute(host: &mut Host, this: NodeId, qualified_name: String) -> Result<()> {
        if let Some(name) = tree::find_attribute(host, this, &qualified_name) {
            tree::drop_attribute(host, this, name);
        }
        Ok(())
    }

    fn remove_attribute_ns(host: &mut Host, this: NodeId, namespace: Option<String>, local_name: String) -> Result<()> {
        if let Some(name) = tree::find_attribute_ns(host, this, namespace.as_deref(), &local_name) {
            tree::drop_attribute(host, this, name);
        }
        Ok(())
    }

    fn toggle_attribute(host: &mut Host, this: NodeId, qualified_name: String, force: Option<bool>) -> Result<bool> {
        check_name(&qualified_name)?;
        match (tree::find_attribute(host, this, &qualified_name), force) {
            (Some(name), None | Some(false)) => {
                tree::drop_attribute(host, this, name);
                Ok(false)
            }
            (Some(_), Some(true)) => Ok(true),
            (None, None | Some(true)) => {
                let wanted = if tree::lowercases_names(host, this) {
                    qualified_name.to_ascii_lowercase()
                } else {
                    qualified_name
                };
                let name = QualName {
                    ns: Namespace::None,
                    local: host.document.intern(&wanted),
                };
                tree::change_attribute(host, this, name, "");
                Ok(true)
            }
            (None, Some(false)) => Ok(false),
        }
    }

    fn has_attribute(host: &mut Host, this: NodeId, qualified_name: String) -> Result<bool> {
        Ok(tree::find_attribute(host, this, &qualified_name).is_some())
    }

    fn has_attribute_ns(host: &mut Host, this: NodeId, namespace: Option<String>, local_name: String) -> Result<bool> {
        Ok(tree::find_attribute_ns(host, this, namespace.as_deref(), &local_name).is_some())
    }

    fn get_attribute_node(host: &mut Host, this: NodeId, qualified_name: String) -> Result<Option<NodeId>> {
        let name = tree::find_attribute(host, this, &qualified_name);
        Ok(name.map(|name| attr::node_for(host, this, name)))
    }

    fn get_attribute_node_ns(host: &mut Host, this: NodeId, namespace: Option<String>, local_name: String) -> Result<Option<NodeId>> {
        let name = tree::find_attribute_ns(host, this, namespace.as_deref(), &local_name);
        Ok(name.map(|name| attr::node_for(host, this, name)))
    }

    fn set_attribute_node(host: &mut Host, this: NodeId, attr: NodeId) -> Result<Option<NodeId>> {
        crate::dom::attr::set_node(host, this, attr)
    }

    fn set_attribute_node_ns(host: &mut Host, this: NodeId, attr: NodeId) -> Result<Option<NodeId>> {
        crate::dom::attr::set_node(host, this, attr)
    }

    fn remove_attribute_node(host: &mut Host, this: NodeId, attr: NodeId) -> Result<NodeId> {
        let owned = crate::dom::attr::owner_of(host, attr).filter(|(owner, _)| *owner == this);
        let Some((_, name)) = owned else {
            return Err(Exception::dom("NotFoundError", "the attribute is not on this element"));
        };
        tree::drop_attribute(host, this, name);
        Ok(attr)
    }

    fn closest(host: &mut Host, this: NodeId, selectors: String) -> Result<Option<NodeId>> {
        let list = parse_selectors(host, &selectors)?;
        let document = &host.document;
        Ok(document.ancestors(this).find(|&node| {
            tree::is_element(document, node) && web_style::selector_matches(document, &list, node, Some(this))
        }))
    }

    fn matches(host: &mut Host, this: NodeId, selectors: String) -> Result<bool> {
        let list = parse_selectors(host, &selectors)?;
        Ok(web_style::selector_matches(&host.document, &list, this, Some(this)))
    }

    fn webkit_matches_selector(host: &mut Host, this: NodeId, selectors: String) -> Result<bool> {
        <Api as ElementApi>::matches(host, this, selectors)
    }

    fn get_elements_by_tag_name(context: &mut Context, this: NodeId, qualified_name: String) -> Result<JsObject> {
        collections::filtered(context, this, collections::tag_filter(&qualified_name))
    }

    fn get_elements_by_tag_name_ns(context: &mut Context, this: NodeId, namespace: Option<String>, local_name: String) -> Result<JsObject> {
        collections::filtered(context, this, collections::tag_ns_filter(namespace, local_name))
    }

    fn get_elements_by_class_name(context: &mut Context, this: NodeId, class_names: String) -> Result<JsObject> {
        collections::filtered(context, this, collections::class_filter(&class_names))
    }

    fn insert_adjacent_element(context: &mut Context, this: NodeId, where_: String, element: NodeId) -> Result<Option<NodeId>> {
        let _ = context;
        crate::host::with(|host| adjacent(host, this, &where_, element))?
    }

    fn insert_adjacent_text(context: &mut Context, this: NodeId, where_: String, data: String) -> Result<()> {
        let _ = context;
        crate::host::with(|host| {
            let text = host.document.create_text(&data);
            let owner = host.owner(this);
            host.set_owner(text, owner);
            adjacent(host, this, &where_, text).map(|_| ())
        })?
    }

    fn insert_adjacent_html(context: &mut Context, this: NodeId, position: String, string: String) -> Result<()> {
        let _ = context;
        crate::host::with(|host| markup::insert_adjacent(host, this, &position, &string))?
    }

    fn inner_html(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(markup::serialize_children(host, this))
    }

    fn set_inner_html(host: &mut Host, this: NodeId, value: String) -> Result<()> {
        markup::set_inner(host, this, &value);
        Ok(())
    }

    fn outer_html(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(markup::serialize_node(host, this))
    }

    fn set_outer_html(host: &mut Host, this: NodeId, value: String) -> Result<()> {
        markup::set_outer(host, this, &value)
    }

    fn client_width(host: &mut Host, _this: NodeId) -> Result<f64> {
        Ok(host.viewport.width.min(0.0))
    }

    fn client_height(host: &mut Host, _this: NodeId) -> Result<f64> {
        Ok(host.viewport.height.min(0.0))
    }

    fn children(context: &mut Context, this: NodeId) -> Result<JsObject> {
        collections::same_object(context, this, Slot::Children)
    }

    fn first_element_child(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(dom::first_element_child(host, this))
    }

    fn last_element_child(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(dom::last_element_child(host, this))
    }

    fn child_element_count(host: &mut Host, this: NodeId) -> Result<u32> {
        Ok(dom::child_element_count(host, this))
    }

    fn prepend(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
        dom::parent_prepend(host, this, nodes)
    }

    fn append(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
        dom::parent_append(host, this, nodes)
    }

    fn replace_children(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
        dom::parent_replace_children(host, this, nodes)
    }

    fn query_selector(host: &mut Host, this: NodeId, selectors: String) -> Result<Option<NodeId>> {
        dom::parent_query_selector(host, this, &selectors)
    }

    fn query_selector_all(context: &mut Context, this: NodeId, selectors: String) -> Result<JsObject> {
        let list = crate::host::with(|host| dom::parent_query_selector_all(host, this, &selectors))??;
        collections::static_list(context, this, list)
    }

    fn previous_element_sibling(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(dom::previous_element_sibling(host, this))
    }

    fn next_element_sibling(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(dom::next_element_sibling(host, this))
    }

    fn before(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
        dom::child_before(host, this, nodes)
    }

    fn after(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
        dom::child_after(host, this, nodes)
    }

    fn replace_with(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
        dom::child_replace_with(host, this, nodes)
    }

    fn remove(host: &mut Host, this: NodeId) -> Result<()> {
        tree::remove(host, this, false);
        Ok(())
    }
}

impl DocumentFragmentApi for Api {
    fn constructor(_context: &mut Context, prototype: JsObject) -> Result<JsObject> {
        let node = crate::host::with(|host| host.document.create_fragment())?;
        Ok(crate::convert::wrap_with_prototype(node, prototype)?)
    }

    fn get_element_by_id(host: &mut Host, this: NodeId, element_id: String) -> Result<Option<NodeId>> {
        Ok(dom::document::element_by_id(host, this, &element_id))
    }

    fn children(context: &mut Context, this: NodeId) -> Result<JsObject> {
        collections::same_object(context, this, Slot::Children)
    }

    fn first_element_child(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(dom::first_element_child(host, this))
    }

    fn last_element_child(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(dom::last_element_child(host, this))
    }

    fn child_element_count(host: &mut Host, this: NodeId) -> Result<u32> {
        Ok(dom::child_element_count(host, this))
    }

    fn prepend(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
        dom::parent_prepend(host, this, nodes)
    }

    fn append(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
        dom::parent_append(host, this, nodes)
    }

    fn replace_children(host: &mut Host, this: NodeId, nodes: Vec<UnionNodeOrDOMString>) -> Result<()> {
        dom::parent_replace_children(host, this, nodes)
    }

    fn query_selector(host: &mut Host, this: NodeId, selectors: String) -> Result<Option<NodeId>> {
        dom::parent_query_selector(host, this, &selectors)
    }

    fn query_selector_all(context: &mut Context, this: NodeId, selectors: String) -> Result<JsObject> {
        let list = crate::host::with(|host| dom::parent_query_selector_all(host, this, &selectors))??;
        collections::static_list(context, this, list)
    }
}

pub fn get_filter(name: Atom) -> Filter {
    Filter::HtmlTag(name)
}

pub fn is_node_kind(host: &Host, node: NodeId, kind: NodeKind) -> bool {
    host.document.kind(node) == Some(kind)
}
