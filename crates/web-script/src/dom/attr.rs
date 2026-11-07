use web_atoms::{Namespace, QualName};
use web_dom::NodeId;

use crate::bindings::AttrApi;
use crate::dom::Api;
use crate::dom::tree::{self, Result};
use crate::exception::Exception;
use crate::host::Host;

pub fn value_of(host: &Host, attribute: NodeId) -> String {
    match owner_of(host, attribute) {
        Some((element, name)) => host
            .document
            .qualified_attribute(element, name)
            .unwrap_or_default()
            .to_owned(),
        None => host.document.attribute_node_value(attribute).to_owned(),
    }
}

pub fn owner_of(host: &Host, attribute: NodeId) -> Option<(NodeId, QualName)> {
    let element = *host.attribute_owners.get(&attribute)?;
    let name = host.document.attribute_node_name(attribute)?;
    Some((element, name))
}

pub fn identity_of(host: &Host, attribute: NodeId) -> (Option<String>, Option<String>) {
    let Some(name) = host.document.attribute_node_name(attribute) else {
        return (None, None);
    };
    if let Some((element, name)) = owner_of(host, attribute) {
        return (
            tree::attribute_namespace(host, element, name),
            tree::attribute_prefix(host, element, name),
        );
    }
    let uri = tree::namespace_uri(host, name.ns, host.detached_attribute_namespace.get(&attribute));
    let prefix = host
        .detached_attribute_prefix
        .get(&attribute)
        .map(|&prefix| host.document.atoms().name(prefix).to_owned());
    (uri, prefix)
}

pub fn qualified_name(host: &Host, attribute: NodeId) -> String {
    let local = host
        .document
        .attribute_node_name(attribute)
        .map_or("", |name| host.document.atoms().name(name.local));
    match identity_of(host, attribute).1 {
        Some(prefix) => format!("{prefix}:{local}"),
        None => local.to_owned(),
    }
}

pub fn node_for(host: &mut Host, element: NodeId, name: QualName) -> NodeId {
    if let Some(&attribute) = host.attribute_nodes.get(&(element, name)) {
        return attribute;
    }
    let attribute = host.document.create_attribute_node(name, "");
    host.attribute_nodes.insert((element, name), attribute);
    host.attribute_owners.insert(attribute, element);
    let owner = host.owner(element);
    host.set_owner(attribute, owner);
    attribute
}

pub fn set_node(host: &mut Host, element: NodeId, attribute: NodeId) -> Result<Option<NodeId>> {
    if let Some(&owner) = host.attribute_owners.get(&attribute)
        && owner != element
    {
        return Err(Exception::dom("InUseAttributeError", "the attribute is in use"));
    }
    let (uri, prefix) = identity_of(host, attribute);
    let Some(name) = host.document.attribute_node_name(attribute) else {
        return Err(Exception::type_error("not an attribute"));
    };
    let local = host.document.atoms().name(name.local).to_owned();
    let existing = tree::find_attribute_ns(host, element, uri.as_deref(), &local);
    if existing.is_some_and(|existing| {
        host.attribute_nodes.get(&(element, existing)) == Some(&attribute)
    }) {
        return Ok(Some(attribute));
    }
    let value = host.document.attribute_node_value(attribute).to_owned();
    let old = existing.map(|existing| node_for(host, element, existing));
    if let Some(existing) = existing {
        let current = host
            .document
            .qualified_attribute(element, existing)
            .unwrap_or_default()
            .to_owned();
        if let Some(old) = old {
            host.document.set_attribute_node_value(old, &current);
        }
        tree::drop_attribute(host, element, existing);
    }
    if let Some(uri) = uri.filter(|_| name.ns == Namespace::Other) {
        host.other_attribute_namespaces.insert((element, name), uri);
    }
    if let Some(prefix) = prefix {
        let prefix = host.document.intern(&prefix);
        host.attribute_prefixes.insert((element, name), prefix);
    }
    host.attribute_nodes.insert((element, name), attribute);
    host.attribute_owners.insert(attribute, element);
    tree::change_attribute(host, element, name, &value);
    Ok(old)
}

impl AttrApi for Api {
    fn namespace_uri(host: &mut Host, this: NodeId) -> Result<Option<String>> {
        Ok(identity_of(host, this).0)
    }

    fn prefix(host: &mut Host, this: NodeId) -> Result<Option<String>> {
        Ok(identity_of(host, this).1)
    }

    fn local_name(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(host
            .document
            .attribute_node_name(this)
            .map_or(String::new(), |name| host.document.atoms().name(name.local).to_owned()))
    }

    fn name(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(qualified_name(host, this))
    }

    fn value(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(value_of(host, this))
    }

    fn set_value(host: &mut Host, this: NodeId, value: String) -> Result<()> {
        match owner_of(host, this) {
            Some((element, name)) => tree::change_attribute(host, element, name, &value),
            None => host.document.set_attribute_node_value(this, &value),
        }
        Ok(())
    }

    fn owner_element(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(host.attribute_owners.get(&this).copied())
    }

    fn specified(_host: &mut Host, _this: NodeId) -> Result<bool> {
        Ok(true)
    }
}
