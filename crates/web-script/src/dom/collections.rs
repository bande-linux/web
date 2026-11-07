use web_atoms::{Atom, Namespace};
use web_dom::{NodeId, NodeKind};

use crate::bindings::*;
use crate::convert::{self, Payload, Platform};
use crate::dom::tree::{self, Result, descendants, is_element};
use crate::dom::{Api, attr};
use crate::engine::{
    Context, Finalize, FunctionObjectBuilder, JsNativeError, JsObject, JsResult, JsString, JsSymbol,
    JsValue, NativeFunction, PropertyDescriptor, PropertyKey, Trace, js_string, text,
};
use crate::exception::Exception;
use crate::host::{self, Host};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    ChildNodes = 0,
    Children = 1,
    ClassList = 2,
    Attributes = 3,
    Dataset = 4,
    RelList = 5,
}

#[derive(Trace, Finalize)]
pub enum Filter {
    ChildNodes,
    Children,
    Static(#[unsafe_ignore_trace] Vec<NodeId>),
    Tag(String, String),
    TagNs(Option<String>, String),
    Class(Vec<String>),
    Name(String),
    Links,
    Anchors,
    HtmlTag(#[unsafe_ignore_trace] Atom),
    Empty,
}

#[derive(Trace, Finalize)]
pub enum Kind {
    Nodes(Filter),
    Tokens(String),
    Attributes,
    Dataset,
}

#[derive(Trace, Finalize)]
pub struct Collection {
    #[unsafe_ignore_trace]
    pub root: NodeId,
    pub kind: Kind,
    #[unsafe_ignore_trace]
    pub cache: Vec<NodeId>,
    pub version: u64,
}

fn matches_filter(host: &Host, filter: &Filter, root: NodeId, node: NodeId) -> bool {
    let document = &host.document;
    let Some((namespace, local)) = document.element_name(node) else {
        return false;
    };
    match filter {
        Filter::Tag(html_name, other_name) => {
            if html_name == "*" {
                return true;
            }
            let qualified = tree::element_qualified_name(host, node);
            if namespace == Namespace::Html && host.is_html(host.owner(node)) {
                qualified == *html_name
            } else {
                qualified == *other_name
            }
        }
        Filter::TagNs(wanted, name) => {
            let local = document.atoms().name(local);
            (name == "*" || local == name)
                && (wanted.as_deref() == Some("*")
                    || tree::element_namespace(host, node).as_deref() == wanted.as_deref())
        }
        Filter::Class(names) => {
            let classes = document.classes(node);
            let quirks = document.quirks_mode() == web_dom::QuirksMode::Quirks && root == root;
            !names.is_empty()
                && names.iter().all(|name| {
                    classes.iter().any(|&class| {
                        let class = document.atoms().name(class);
                        if quirks {
                            class.eq_ignore_ascii_case(name)
                        } else {
                            class == name
                        }
                    })
                })
        }
        Filter::Name(name) => document.attribute(node, Atom::NAME) == Some(name),
        Filter::Links => {
            matches!(document.html_element_name(node), Some(Atom::A | Atom::AREA))
                && document.attribute(node, Atom::HREF).is_some()
        }
        Filter::Anchors => {
            document.html_element_name(node) == Some(Atom::A)
                && document.attribute(node, Atom::NAME).is_some()
        }
        Filter::HtmlTag(tag) => document.html_element_name(node) == Some(*tag),
        _ => false,
    }
}

fn compute(host: &Host, root: NodeId, kind: &Kind) -> Vec<NodeId> {
    let document = &host.document;
    match kind {
        Kind::Nodes(Filter::ChildNodes) => document.children(root).collect(),
        Kind::Nodes(Filter::Children) => document
            .children(root)
            .filter(|&child| is_element(document, child))
            .collect(),
        Kind::Nodes(Filter::Static(nodes)) => nodes.clone(),
        Kind::Nodes(Filter::Empty) => Vec::new(),
        Kind::Nodes(filter) => descendants(document, root)
            .into_iter()
            .filter(|&node| matches_filter(host, filter, root, node))
            .collect(),
        _ => Vec::new(),
    }
}

fn with_collection<R>(object: &JsObject, f: impl FnOnce(&mut Collection) -> R) -> Option<R> {
    let mut platform = object.downcast_mut::<Platform>()?;
    match &mut platform.payload {
        Payload::Collection(collection) => Some(f(collection)),
        _ => None,
    }
}

pub fn nodes(host: &Host, object: &JsObject) -> Vec<NodeId> {
    with_collection(object, |collection| {
        if collection.version != host.version || collection.version == 0 {
            collection.cache = compute(host, collection.root, &collection.kind);
            collection.version = host.version;
        }
        collection.cache.clone()
    })
    .unwrap_or_default()
}

fn node_at(host: &Host, object: &JsObject, index: usize) -> Option<NodeId> {
    with_collection(object, |collection| {
        if collection.version != host.version || collection.version == 0 {
            collection.cache = compute(host, collection.root, &collection.kind);
            collection.version = host.version;
        }
        collection.cache.get(index).copied()
    })
    .flatten()
}

fn node_count(host: &Host, object: &JsObject) -> usize {
    with_collection(object, |collection| {
        if collection.version != host.version || collection.version == 0 {
            collection.cache = compute(host, collection.root, &collection.kind);
            collection.version = host.version;
        }
        collection.cache.len()
    })
    .unwrap_or(0)
}

pub fn make(interface: Interface, root: NodeId, kind: Kind, context: &mut Context) -> JsResult<JsObject> {
    let (prototype, handler, constructor) = host::with(|host| {
        (
            host.prototype(interface),
            host.proxy_handler.clone(),
            host.proxy_constructor.clone(),
        )
    })?;
    let (Some(prototype), Some(handler), Some(constructor)) = (prototype, handler, constructor) else {
        return Err(JsNativeError::error().with_message("no realm").into());
    };
    let target = JsObject::from_proto_and_data(
        prototype,
        Platform::new(
            interface,
            Payload::Collection(Box::new(Collection {
                root,
                kind,
                cache: Vec::new(),
                version: 0,
            })),
        ),
    );
    constructor.construct(
        &[JsValue::from(target), JsValue::from(handler)],
        None,
        context,
    )
}

pub fn same_object(context: &mut Context, node: NodeId, slot: Slot) -> Result<JsObject> {
    let key = (node, slot as u8);
    if let Some(found) = host::with(|host| host.same_objects.get(&key).cloned())? {
        return Ok(found);
    }
    let (interface, kind) = match slot {
        Slot::ChildNodes => (Interface::NodeList, Kind::Nodes(Filter::ChildNodes)),
        Slot::Children => (Interface::HTMLCollection, Kind::Nodes(Filter::Children)),
        Slot::ClassList => (Interface::DOMTokenList, Kind::Tokens("class".to_owned())),
        Slot::RelList => (Interface::DOMTokenList, Kind::Tokens("rel".to_owned())),
        Slot::Attributes => (Interface::NamedNodeMap, Kind::Attributes),
        Slot::Dataset => (Interface::DOMStringMap, Kind::Dataset),
    };
    let object = make(interface, node, kind, context)?;
    host::with(|host| host.same_objects.insert(key, object.clone()))?;
    Ok(object)
}

pub fn filtered(context: &mut Context, root: NodeId, filter: Filter) -> Result<JsObject> {
    Ok(make(Interface::HTMLCollection, root, Kind::Nodes(filter), context)?)
}

pub fn static_list(context: &mut Context, root: NodeId, list: Vec<NodeId>) -> Result<JsObject> {
    Ok(make(
        Interface::NodeList,
        root,
        Kind::Nodes(Filter::Static(list)),
        context,
    )?)
}

pub fn tag_filter(name: &str) -> Filter {
    Filter::Tag(name.to_ascii_lowercase(), name.to_owned())
}

pub fn tag_ns_filter(namespace: Option<String>, local: String) -> Filter {
    Filter::TagNs(namespace.filter(|namespace| !namespace.is_empty()), local)
}

pub fn class_filter(names: &str) -> Filter {
    Filter::Class(ordered_tokens(names))
}

pub fn ordered_tokens(value: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    for token in value.split_ascii_whitespace() {
        if !tokens.iter().any(|known| known == token) {
            tokens.push(token.to_owned());
        }
    }
    tokens
}

fn collection_root(object: &JsObject) -> Option<(NodeId, String)> {
    with_collection(object, |collection| match &collection.kind {
        Kind::Tokens(name) => Some((collection.root, name.clone())),
        _ => None,
    })
    .flatten()
}

fn root_of_collection(object: &JsObject) -> Option<NodeId> {
    with_collection(object, |collection| collection.root)
}

fn is_dataset(object: &JsObject) -> bool {
    with_collection(object, |collection| matches!(collection.kind, Kind::Dataset)).unwrap_or(false)
}

fn is_attributes(object: &JsObject) -> bool {
    with_collection(object, |collection| matches!(collection.kind, Kind::Attributes)).unwrap_or(false)
}

fn is_tokens(object: &JsObject) -> bool {
    with_collection(object, |collection| matches!(collection.kind, Kind::Tokens(_))).unwrap_or(false)
}

fn is_html_collection(object: &JsObject) -> bool {
    object
        .downcast_ref::<Platform>()
        .is_some_and(|platform| platform.interface == Interface::HTMLCollection)
}

fn token_name(host: &mut Host, name: &str) -> web_atoms::QualName {
    let atom = host.document.intern(name);
    web_atoms::QualName::attribute(atom)
}

fn tokens_of(host: &mut Host, object: &JsObject) -> Option<(NodeId, web_atoms::QualName, Vec<String>)> {
    let (root, name) = collection_root(object)?;
    let name = token_name(host, &name);
    let value = host.document.qualified_attribute(root, name).unwrap_or_default();
    let tokens = ordered_tokens(value);
    Some((root, name, tokens))
}

fn store_tokens(host: &mut Host, root: NodeId, name: web_atoms::QualName, tokens: &[String]) {
    if host.document.qualified_attribute(root, name).is_none() && tokens.is_empty() {
        return;
    }
    tree::change_attribute(host, root, name, &tokens.join(" "));
}

fn validate_token(token: &str) -> Result<()> {
    if token.is_empty() {
        return Err(Exception::dom("SyntaxError", "the token is empty"));
    }
    if token.chars().any(|ch| matches!(ch, '\t' | '\n' | '\u{c}' | '\r' | ' ')) {
        return Err(Exception::dom(
            "InvalidCharacterError",
            "the token contains whitespace",
        ));
    }
    Ok(())
}

impl DOMTokenListApi for Api {
    fn length(host: &mut Host, this: JsObject) -> Result<u32> {
        Ok(tokens_of(host, &this).map_or(0, |(_, _, tokens)| tokens.len() as u32))
    }

    fn item(host: &mut Host, this: JsObject, index: u32) -> Result<Option<String>> {
        Ok(tokens_of(host, &this).and_then(|(_, _, tokens)| tokens.get(index as usize).cloned()))
    }

    fn contains(host: &mut Host, this: JsObject, token: String) -> Result<bool> {
        Ok(tokens_of(host, &this).is_some_and(|(_, _, tokens)| tokens.contains(&token)))
    }

    fn add(host: &mut Host, this: JsObject, tokens: Vec<String>) -> Result<()> {
        for token in &tokens {
            validate_token(token)?;
        }
        if let Some((root, name, mut known)) = tokens_of(host, &this) {
            for token in tokens {
                if !known.contains(&token) {
                    known.push(token);
                }
            }
            store_tokens(host, root, name, &known);
        }
        Ok(())
    }

    fn remove(host: &mut Host, this: JsObject, tokens: Vec<String>) -> Result<()> {
        for token in &tokens {
            validate_token(token)?;
        }
        if let Some((root, name, mut known)) = tokens_of(host, &this) {
            known.retain(|existing| !tokens.contains(existing));
            store_tokens(host, root, name, &known);
        }
        Ok(())
    }

    fn toggle(host: &mut Host, this: JsObject, token: String, force: Option<bool>) -> Result<bool> {
        validate_token(&token)?;
        let Some((root, name, mut known)) = tokens_of(host, &this) else {
            return Ok(false);
        };
        if known.contains(&token) {
            if force != Some(true) {
                known.retain(|existing| *existing != token);
                store_tokens(host, root, name, &known);
                return Ok(false);
            }
            return Ok(true);
        }
        if force != Some(false) {
            known.push(token);
            store_tokens(host, root, name, &known);
            return Ok(true);
        }
        Ok(false)
    }

    fn replace(host: &mut Host, this: JsObject, token: String, new_token: String) -> Result<bool> {
        if token.is_empty() || new_token.is_empty() {
            return Err(Exception::dom("SyntaxError", "the token is empty"));
        }
        validate_token(&token)?;
        validate_token(&new_token)?;
        let Some((root, name, mut known)) = tokens_of(host, &this) else {
            return Ok(false);
        };
        let Some(position) = known.iter().position(|existing| *existing == token) else {
            return Ok(false);
        };
        let other = known.iter().position(|existing| *existing == new_token);
        match other {
            Some(other) if other != position => {
                let first = position.min(other);
                if let Some(slot) = known.get_mut(first) {
                    *slot = new_token.clone();
                }
                let last = position.max(other);
                known.remove(last);
            }
            _ => {
                if let Some(slot) = known.get_mut(position) {
                    *slot = new_token;
                }
            }
        }
        store_tokens(host, root, name, &known);
        Ok(true)
    }

    fn supports(_host: &mut Host, _this: JsObject, _token: String) -> Result<bool> {
        Err(Exception::type_error("this attribute has no supported tokens"))
    }

    fn value(host: &mut Host, this: JsObject) -> Result<String> {
        let Some((root, name)) = collection_root(&this) else {
            return Ok(String::new());
        };
        let name = token_name(host, &name);
        Ok(host
            .document
            .qualified_attribute(root, name)
            .unwrap_or_default()
            .to_owned())
    }

    fn set_value(host: &mut Host, this: JsObject, value: String) -> Result<()> {
        if let Some((root, name)) = collection_root(&this) {
            let name = token_name(host, &name);
            tree::change_attribute(host, root, name, &value);
        }
        Ok(())
    }
}

impl NodeListApi for Api {
    fn item(host: &mut Host, this: JsObject, index: u32) -> Result<Option<NodeId>> {
        Ok(node_at(host, &this, index as usize))
    }

    fn length(host: &mut Host, this: JsObject) -> Result<u32> {
        Ok(node_count(host, &this) as u32)
    }
}

fn named_element(host: &Host, list: &[NodeId], name: &str) -> Option<NodeId> {
    if name.is_empty() {
        return None;
    }
    list.iter().copied().find(|&node| {
        host.document.attribute(node, Atom::ID) == Some(name)
            || (matches!(host.document.element_name(node), Some((Namespace::Html, _)))
                && host.document.attribute(node, Atom::NAME) == Some(name))
    })
}

fn supported_names(host: &Host, list: &[NodeId]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for &node in list {
        let mut add = |name: &str| {
            if !name.is_empty() && !names.iter().any(|known| known == name) {
                names.push(name.to_owned());
            }
        };
        if let Some(id) = host.document.attribute(node, Atom::ID) {
            add(id);
        }
        if matches!(host.document.element_name(node), Some((Namespace::Html, _)))
            && let Some(name) = host.document.attribute(node, Atom::NAME)
        {
            add(name);
        }
    }
    names
}

impl HTMLCollectionApi for Api {
    fn length(host: &mut Host, this: JsObject) -> Result<u32> {
        Ok(node_count(host, &this) as u32)
    }

    fn item(host: &mut Host, this: JsObject, index: u32) -> Result<Option<NodeId>> {
        Ok(node_at(host, &this, index as usize))
    }

    fn named_item(host: &mut Host, this: JsObject, name: String) -> Result<Option<NodeId>> {
        let list = nodes(host, &this);
        Ok(named_element(host, &list, &name))
    }
}

fn map_owner(this: &JsObject) -> Result<NodeId> {
    root_of_collection(this).ok_or_else(|| Exception::type_error("not an attribute map"))
}

impl NamedNodeMapApi for Api {
    fn length(host: &mut Host, this: JsObject) -> Result<u32> {
        let element = map_owner(&this)?;
        Ok(host.document.attributes(element).count() as u32)
    }

    fn item(host: &mut Host, this: JsObject, index: u32) -> Result<Option<NodeId>> {
        let element = map_owner(&this)?;
        let name = host.document.attributes(element).nth(index as usize).map(|(name, _)| name);
        Ok(name.map(|name| attr::node_for(host, element, name)))
    }

    fn get_named_item(host: &mut Host, this: JsObject, qualified_name: String) -> Result<Option<NodeId>> {
        let element = map_owner(&this)?;
        let name = tree::find_attribute(host, element, &qualified_name);
        Ok(name.map(|name| attr::node_for(host, element, name)))
    }

    fn get_named_item_ns(host: &mut Host, this: JsObject, namespace: Option<String>, local_name: String) -> Result<Option<NodeId>> {
        let element = map_owner(&this)?;
        let name = tree::find_attribute_ns(host, element, namespace.as_deref(), &local_name);
        Ok(name.map(|name| attr::node_for(host, element, name)))
    }

    fn set_named_item(host: &mut Host, this: JsObject, attr: NodeId) -> Result<Option<NodeId>> {
        let element = map_owner(&this)?;
        crate::dom::attr::set_node(host, element, attr)
    }

    fn set_named_item_ns(host: &mut Host, this: JsObject, attr: NodeId) -> Result<Option<NodeId>> {
        let element = map_owner(&this)?;
        crate::dom::attr::set_node(host, element, attr)
    }

    fn remove_named_item(host: &mut Host, this: JsObject, qualified_name: String) -> Result<NodeId> {
        let element = map_owner(&this)?;
        let Some(name) = tree::find_attribute(host, element, &qualified_name) else {
            return Err(Exception::dom("NotFoundError", "no such attribute"));
        };
        let node = attr::node_for(host, element, name);
        tree::drop_attribute(host, element, name);
        Ok(node)
    }

    fn remove_named_item_ns(host: &mut Host, this: JsObject, namespace: Option<String>, local_name: String) -> Result<NodeId> {
        let element = map_owner(&this)?;
        let Some(name) = tree::find_attribute_ns(host, element, namespace.as_deref(), &local_name) else {
            return Err(Exception::dom("NotFoundError", "no such attribute"));
        };
        let node = attr::node_for(host, element, name);
        tree::drop_attribute(host, element, name);
        Ok(node)
    }
}

fn dataset_attribute(key: &str) -> Option<String> {
    let bytes = key.as_bytes();
    if bytes
        .windows(2)
        .any(|pair| pair.first() == Some(&b'-') && pair.get(1).is_some_and(u8::is_ascii_lowercase))
    {
        return None;
    }
    let mut name = String::from("data-");
    for ch in key.chars() {
        if ch.is_ascii_uppercase() {
            name.push('-');
            name.push(ch.to_ascii_lowercase());
        } else {
            name.push(ch);
        }
    }
    Some(name)
}

fn dataset_key(attribute: &str) -> Option<String> {
    let rest = attribute.strip_prefix("data-")?;
    if rest.chars().any(|ch| ch.is_ascii_uppercase()) {
        return None;
    }
    let mut key = String::new();
    let mut chars = rest.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '-'
            && let Some(next) = chars.peek().copied().filter(char::is_ascii_lowercase)
        {
            key.push(next.to_ascii_uppercase());
            chars.next();
        } else {
            key.push(ch);
        }
    }
    Some(key)
}

fn array_index(key: &PropertyKey) -> Option<u32> {
    match key {
        PropertyKey::Index(index) => Some(index.get()),
        _ => None,
    }
}

fn string_key(key: &PropertyKey) -> Option<String> {
    match key {
        PropertyKey::String(name) => Some(name.to_std_string_lossy()),
        _ => None,
    }
}

enum Lookup {
    Missing,
    Value(JsValue),
}

fn indexed(object: &JsObject, index: u32) -> JsResult<Lookup> {
    if is_tokens(object) {
        let found = host::with(|host| {
            tokens_of(host, object).and_then(|(_, _, tokens)| tokens.get(index as usize).cloned())
        })?;
        return Ok(found.map_or(Lookup::Missing, |token| Lookup::Value(text(&token))));
    }
    if is_attributes(object) {
        let found = host::with(|host| {
            let element = root_of_collection(object)?;
            let name = host.document.attributes(element).nth(index as usize).map(|(name, _)| name)?;
            Some(attr::node_for(host, element, name))
        })?;
        return found.map_or(Ok(Lookup::Missing), |node| {
            Ok(Lookup::Value(JsValue::from(convert::wrap(node)?)))
        });
    }
    if is_dataset(object) {
        return Ok(Lookup::Missing);
    }
    let found = host::with(|host| node_at(host, object, index as usize))?;
    found.map_or(Ok(Lookup::Missing), |node| {
        Ok(Lookup::Value(JsValue::from(convert::wrap(node)?)))
    })
}

fn named(object: &JsObject, name: &str) -> JsResult<Lookup> {
    if is_dataset(object) {
        let Some(attribute) = dataset_attribute(name) else {
            return Ok(Lookup::Missing);
        };
        let found = host::with(|host| {
            let element = root_of_collection(object)?;
            let atom = host.document.atoms().get(&attribute)?;
            host.document.attribute(element, atom).map(str::to_owned)
        })?;
        return Ok(found.map_or(Lookup::Missing, |value| Lookup::Value(text(&value))));
    }
    if is_attributes(object) {
        let found = host::with(|host| {
            let element = root_of_collection(object)?;
            let name = tree::find_attribute(host, element, name)?;
            Some(attr::node_for(host, element, name))
        })?;
        return found.map_or(Ok(Lookup::Missing), |node| {
            Ok(Lookup::Value(JsValue::from(convert::wrap(node)?)))
        });
    }
    if is_html_collection(object) {
        let found = host::with(|host| {
            let list = nodes(host, object);
            named_element(host, &list, name)
        })?;
        return found.map_or(Ok(Lookup::Missing), |node| {
            Ok(Lookup::Value(JsValue::from(convert::wrap(node)?)))
        });
    }
    Ok(Lookup::Missing)
}

fn shadowed(object: &JsObject, key: &PropertyKey, context: &mut Context) -> JsResult<bool> {
    let platform_named = is_dataset(object);
    if platform_named {
        return Ok(false);
    }
    object.has_property(key.clone(), context)
}

fn lookup(object: &JsObject, key: &PropertyKey, context: &mut Context) -> JsResult<Lookup> {
    if let Some(index) = array_index(key) {
        return indexed(object, index);
    }
    let Some(name) = string_key(key) else {
        return Ok(Lookup::Missing);
    };
    if shadowed(object, key, context)? {
        return Ok(Lookup::Missing);
    }
    named(object, &name)
}

fn target_of(arguments: &[JsValue]) -> JsResult<JsObject> {
    convert::any_object(&convert::argument(arguments, 0))
}

fn key_of(arguments: &[JsValue], context: &mut Context) -> JsResult<PropertyKey> {
    convert::argument(arguments, 1).to_property_key(context)
}

fn trap_get(_this: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = target_of(arguments)?;
    let key = key_of(arguments, context)?;
    if let PropertyKey::Symbol(symbol) = &key {
        let private = host::with(|host| host.proxy_key.clone())?;
        if private.as_ref() == Some(symbol) {
            return Ok(JsValue::from(target));
        }
    }
    match lookup(&target, &key, context)? {
        Lookup::Value(value) => Ok(value),
        Lookup::Missing if array_index(&key).is_some() => Ok(JsValue::undefined()),
        Lookup::Missing => target.get(key, context),
    }
}

fn trap_has(_this: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = target_of(arguments)?;
    let key = key_of(arguments, context)?;
    match lookup(&target, &key, context)? {
        Lookup::Value(_) => Ok(JsValue::from(true)),
        Lookup::Missing if array_index(&key).is_some() => Ok(JsValue::from(false)),
        Lookup::Missing => Ok(JsValue::from(target.has_property(key, context)?)),
    }
}

fn trap_set(_this: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = target_of(arguments)?;
    let key = key_of(arguments, context)?;
    let value = convert::argument(arguments, 2);
    if array_index(&key).is_some() && !is_dataset(&target) {
        return Ok(JsValue::from(false));
    }
    if is_dataset(&target)
        && let Some(name) = string_key(&key).or_else(|| array_index(&key).map(|index| index.to_string()))
    {
        return dataset_set(&target, &name, &value, context);
    }
    if is_html_collection(&target)
        && let Lookup::Value(_) = lookup(&target, &key, context)?
    {
        return Ok(JsValue::from(false));
    }
    Ok(JsValue::from(target.set(key, value, false, context)?))
}

fn dataset_set(target: &JsObject, name: &str, value: &JsValue, context: &mut Context) -> JsResult<JsValue> {
    let Some(attribute) = dataset_attribute(name) else {
        return Err(Exception::dom("SyntaxError", "the name has a dash before a letter").into());
    };
    if !tree::valid_attribute_name(&attribute) {
        return Err(Exception::dom("InvalidCharacterError", "the name is not valid").into());
    }
    let value = convert::string(value, context)?;
    host::with(|host| {
        if let Some(element) = root_of_collection(target) {
            let atom = host.document.intern(&attribute);
            tree::change_attribute(host, element, web_atoms::QualName::attribute(atom), &value);
        }
    })?;
    convert::after(context)?;
    Ok(JsValue::from(true))
}

fn trap_delete(_this: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = target_of(arguments)?;
    let key = key_of(arguments, context)?;
    if is_dataset(&target)
        && let Some(attribute) = string_key(&key).as_deref().and_then(dataset_attribute)
    {
        host::with(|host| {
            if let Some(element) = root_of_collection(&target)
                && let Some(atom) = host.document.atoms().get(&attribute)
            {
                tree::drop_attribute(host, element, web_atoms::QualName::attribute(atom));
            }
        })?;
        return Ok(JsValue::from(true));
    }
    if let Lookup::Value(_) = lookup(&target, &key, context)? {
        if is_attributes(&target) && array_index(&key).is_none() {
            return Ok(JsValue::from(false));
        }
        return Ok(JsValue::from(false));
    }
    if array_index(&key).is_some() {
        return Ok(JsValue::from(true));
    }
    Ok(JsValue::from(target.delete_property_or_throw(key, context)?))
}

fn trap_descriptor(_this: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = target_of(arguments)?;
    let key = key_of(arguments, context)?;
    let object_constructor = context.intrinsics().constructors().object().constructor();
    if let Lookup::Value(value) = lookup(&target, &key, context)? {
        let descriptor = JsObject::with_object_proto(context.intrinsics());
        descriptor.set(js_string!("value"), value, true, context)?;
        descriptor.set(js_string!("writable"), is_dataset(&target), true, context)?;
        descriptor.set(
            js_string!("enumerable"),
            array_index(&key).is_some() || is_dataset(&target),
            true,
            context,
        )?;
        descriptor.set(js_string!("configurable"), true, true, context)?;
        return Ok(JsValue::from(descriptor));
    }
    let method = object_constructor.get(js_string!("getOwnPropertyDescriptor"), context)?;
    let Some(method) = method.as_object() else {
        return Ok(JsValue::undefined());
    };
    method.call(
        &JsValue::undefined(),
        &[JsValue::from(target), convert::argument(arguments, 1)],
        context,
    )
}

fn trap_define(_this: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = target_of(arguments)?;
    let key = key_of(arguments, context)?;
    let descriptor = convert::any_object(&convert::argument(arguments, 2))?;
    if is_dataset(&target)
        && let Some(name) = string_key(&key)
    {
        let value = descriptor.get(js_string!("value"), context)?;
        return dataset_set(&target, &name, &value, context);
    }
    if array_index(&key).is_some() {
        return Ok(JsValue::from(false));
    }
    if let Lookup::Value(_) = lookup(&target, &key, context)? {
        return Ok(JsValue::from(false));
    }
    let descriptor: PropertyDescriptor = descriptor.to_property_descriptor(context)?;
    target.define_property_or_throw(key, descriptor, context)?;
    Ok(JsValue::from(true))
}

fn trap_keys(_this: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = target_of(arguments)?;
    let mut keys: Vec<JsValue> = Vec::new();
    let count = host::with(|host| {
        if is_tokens(&target) {
            tokens_of(host, &target).map_or(0, |(_, _, tokens)| tokens.len())
        } else if is_attributes(&target) {
            root_of_collection(&target).map_or(0, |element| host.document.attributes(element).count())
        } else if is_dataset(&target) {
            0
        } else {
            node_count(host, &target)
        }
    })?;
    for index in 0..count {
        keys.push(text(&index.to_string()));
    }
    let names: Vec<String> = host::with(|host| {
        if is_dataset(&target) {
            root_of_collection(&target)
                .map(|element| {
                    host.document
                        .attributes(element)
                        .filter_map(|(name, _)| dataset_key(host.document.atoms().name(name.local)))
                        .collect()
                })
                .unwrap_or_default()
        } else if is_attributes(&target) {
            root_of_collection(&target)
                .map(|element| {
                    let names: Vec<_> = host.document.attributes(element).map(|(name, _)| name).collect();
                    names
                        .into_iter()
                        .map(|name| tree::attribute_qualified_name(host, element, name))
                        .collect()
                })
                .unwrap_or_default()
        } else if is_html_collection(&target) {
            let list = nodes(host, &target);
            supported_names(host, &list)
        } else {
            Vec::new()
        }
    })?;
    for name in names {
        let key = PropertyKey::from(JsString::from(name.as_str()));
        if is_dataset(&target) || !shadowed(&target, &key, context)? {
            keys.push(text(&name));
        }
    }
    for key in target.own_property_keys(context)? {
        keys.push(match key {
            PropertyKey::String(name) => JsValue::from(name),
            PropertyKey::Index(index) => text(&index.get().to_string()),
            PropertyKey::Symbol(symbol) => JsValue::from(symbol),
        });
    }
    Ok(JsValue::from(crate::engine::JsArray::from_iter(keys, context)))
}

fn trap_prevent(_this: &JsValue, _arguments: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::from(false))
}

fn trap_function(name: &str, native: crate::engine::Native, context: &mut Context) -> JsValue {
    let function: JsObject = FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(native))
        .name(JsString::from(name))
        .build()
        .into();
    JsValue::from(function)
}

pub fn install(context: &mut Context) -> JsResult<()> {
    let handler = JsObject::with_object_proto(context.intrinsics());
    let traps: [(&str, crate::engine::Native); 8] = [
        ("get", trap_get),
        ("has", trap_has),
        ("set", trap_set),
        ("deleteProperty", trap_delete),
        ("getOwnPropertyDescriptor", trap_descriptor),
        ("defineProperty", trap_define),
        ("ownKeys", trap_keys),
        ("preventExtensions", trap_prevent),
    ];
    for (name, native) in traps {
        let function = trap_function(name, native, context);
        handler.set(JsString::from(name), function, true, context)?;
    }
    let constructor = context.global_object().get(js_string!("Proxy"), context)?;
    let constructor = convert::any_object(&constructor)?;
    let function = context.global_object().get(js_string!("Function"), context)?;
    let function = convert::any_object(&function)?;
    host::with(|host| {
        host.proxy_handler = Some(handler);
        host.proxy_constructor = Some(constructor);
        host.proxy_key = Some(JsSymbol::new(None).unwrap_or_else(JsSymbol::iterator));
        host.function_constructor = Some(function);
    })
}

pub fn proxy_target(object: &JsObject, context: &mut Context) -> JsResult<Option<JsObject>> {
    if boa_engine::object::builtins::JsProxy::from_object(object.clone()).is_err() {
        return Ok(None);
    }
    let key = host::with(|host| host.proxy_key.clone())?;
    let Some(key) = key else {
        return Ok(None);
    };
    let value = object.get(key, context)?;
    Ok(value
        .as_object()
        .filter(|target| target.is::<Platform>())
        .map(|target| target.clone()))
}

pub fn document_collection(context: &mut Context, this: NodeId, tag: Atom) -> Result<JsObject> {
    filtered(context, this, Filter::HtmlTag(tag))
}

pub fn is_text_node(host: &Host, node: NodeId) -> bool {
    host.document.kind(node) == Some(NodeKind::Text)
}
