use web_atoms::{Namespace, QualName};
use web_dom::NodeId;

use crate::bindings::*;
use crate::convert::{Payload, Platform};
use crate::dom::collections;
use crate::dom::tree::{self, Result};
use crate::dom::Api;
use crate::engine::{Context, Finalize, JsObject, Trace};
use crate::exception::Exception;
use crate::host::{self, Host};

#[derive(Clone, Default)]
pub struct Options {
    pub child_list: bool,
    pub attributes: bool,
    pub character_data: bool,
    pub subtree: bool,
    pub attribute_old_value: bool,
    pub character_data_old_value: bool,
    pub attribute_filter: Option<Vec<String>>,
}

pub struct Observer {
    pub id: u64,
    pub callback: JsObject,
    pub object: JsObject,
    pub nodes: Vec<(NodeId, Options)>,
    pub transient: Vec<(NodeId, NodeId, Options)>,
    pub records: Vec<RecordData>,
}

#[derive(Clone)]
pub struct RecordData {
    pub kind: &'static str,
    pub target: NodeId,
    pub added: Vec<NodeId>,
    pub removed: Vec<NodeId>,
    pub previous: Option<NodeId>,
    pub next: Option<NodeId>,
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub old_value: Option<String>,
}

#[derive(Trace, Finalize)]
pub struct Record {
    #[unsafe_ignore_trace]
    pub data: RecordData,
}

fn queue(
    host: &mut Host,
    target: NodeId,
    interested: impl Fn(&Options, bool) -> Option<bool>,
    record: impl Fn(bool) -> RecordData,
) {
    if host.observers.is_empty() {
        return;
    }
    let ancestors: Vec<NodeId> = host.document.ancestors(target).collect();
    let mut any = false;
    for observer in &mut host.observers {
        let mut matched: Option<bool> = None;
        let mut consider = |options: &Options, node: NodeId| {
            let direct = node == target;
            if !direct && !options.subtree {
                return;
            }
            if let Some(wants_old) = interested(options, direct) {
                matched = Some(matched.unwrap_or(false) || wants_old);
            }
        };
        for (node, options) in &observer.nodes {
            if ancestors.contains(node) {
                consider(options, *node);
            }
        }
        for (node, _, options) in &observer.transient {
            if ancestors.contains(node) {
                consider(options, *node);
            }
        }
        if let Some(old) = matched {
            observer.records.push(record(old));
            any = true;
        }
    }
    if any && !host.observer_microtask {
        host.observer_microtask = true;
        host::mark_pending();
    }
}

pub fn queue_tree(
    host: &mut Host,
    target: NodeId,
    added: Vec<NodeId>,
    removed: Vec<NodeId>,
    previous: Option<NodeId>,
    next: Option<NodeId>,
) {
    queue(
        host,
        target,
        |options, _| options.child_list.then_some(false),
        |_| RecordData {
            kind: "childList",
            target,
            added: added.clone(),
            removed: removed.clone(),
            previous,
            next,
            name: None,
            namespace: None,
            old_value: None,
        },
    );
}

pub fn queue_character_data(host: &mut Host, target: NodeId, old: &str) {
    queue(
        host,
        target,
        |options, _| {
            options
                .character_data
                .then_some(options.character_data_old_value)
        },
        |wants_old| RecordData {
            kind: "characterData",
            target,
            added: Vec::new(),
            removed: Vec::new(),
            previous: None,
            next: None,
            name: None,
            namespace: None,
            old_value: wants_old.then(|| old.to_owned()),
        },
    );
}

pub fn queue_attribute(host: &mut Host, target: NodeId, name: QualName, old: Option<&str>) {
    if host.observers.is_empty() {
        return;
    }
    let local = host.document.atoms().name(name.local).to_owned();
    let namespace = tree::attribute_namespace(host, target, name);
    let in_namespace = name.ns != Namespace::None && name.ns != Namespace::Html;
    queue(
        host,
        target,
        |options, _| {
            if !options.attributes {
                return None;
            }
            if let Some(filter) = &options.attribute_filter
                && (in_namespace || !filter.iter().any(|known| *known == local))
            {
                return None;
            }
            Some(options.attribute_old_value)
        },
        |wants_old| RecordData {
            kind: "attributes",
            target,
            added: Vec::new(),
            removed: Vec::new(),
            previous: None,
            next: None,
            name: Some(local.clone()),
            namespace: namespace.clone(),
            old_value: if wants_old { old.map(str::to_owned) } else { None },
        },
    );
}

pub fn before_remove(host: &mut Host, node: NodeId, parent: NodeId) {
    if host.observers.is_empty() {
        return;
    }
    let ancestors: Vec<NodeId> = host.document.ancestors(parent).collect();
    for observer in &mut host.observers {
        let mut added = Vec::new();
        for (registered, options) in &observer.nodes {
            if options.subtree && ancestors.contains(registered) {
                added.push((node, *registered, options.clone()));
            }
        }
        observer.transient.extend(added);
    }
}

pub fn deliver(context: &mut Context) -> crate::engine::JsResult<()> {
    let batches = host::with(|host| {
        host.observer_microtask = false;
        let mut batches = Vec::new();
        for observer in &mut host.observers {
            observer.transient.clear();
            let records = std::mem::take(&mut observer.records);
            if !records.is_empty() {
                batches.push((observer.callback.clone(), observer.object.clone(), records));
            }
        }
        batches
    })?;
    for (callback, object, records) in batches {
        let values = host::with(|host| record_objects(host, records))??;
        let list = crate::convert::to_js(values, context)?;
        if let Err(error) = callback.call(
            &crate::engine::JsValue::from(object.clone()),
            &[list, crate::engine::JsValue::from(object)],
            context,
        ) {
            crate::events::report_exception(context, error)?;
        }
    }
    Ok(())
}

fn record_objects(host: &mut Host, records: Vec<RecordData>) -> Result<Vec<JsObject>> {
    let Some(prototype) = host.prototype(Interface::MutationRecord) else {
        return Err(Exception::type_error("no realm"));
    };
    Ok(records
        .into_iter()
        .map(|data| {
            JsObject::from_proto_and_data(
                prototype.clone(),
                Platform::new(
                    Interface::MutationRecord,
                    Payload::Record(Box::new(Record { data })),
                ),
            )
        })
        .collect())
}

fn observer_id(object: &JsObject) -> u64 {
    object
        .downcast_ref::<Platform>()
        .and_then(|platform| match platform.payload {
            Payload::Observer(id) => Some(id),
            _ => None,
        })
        .unwrap_or(u64::MAX)
}

impl MutationObserverApi for Api {
    fn constructor(_context: &mut Context, prototype: JsObject, callback: JsObject) -> Result<JsObject> {
        Ok(host::with(|host| {
            host.next_observer += 1;
            let id = host.next_observer;
            let object = JsObject::from_proto_and_data(
                prototype,
                Platform::new(Interface::MutationObserver, Payload::Observer(id)),
            );
            host.observers.push(Observer {
                id,
                callback,
                object: object.clone(),
                nodes: Vec::new(),
                transient: Vec::new(),
                records: Vec::new(),
            });
            object
        })?)
    }

    fn observe(host: &mut Host, this: JsObject, target: NodeId, options: MutationObserverInit) -> Result<()> {
        let attributes = options
            .attributes
            .or_else(|| (options.attribute_old_value.is_some() || options.attribute_filter.is_some()).then_some(true))
            .unwrap_or(false);
        let character_data = options
            .character_data
            .or_else(|| options.character_data_old_value.is_some().then_some(true))
            .unwrap_or(false);
        if !options.child_list && !attributes && !character_data {
            return Err(Exception::type_error(
                "observe needs childList, attributes or characterData",
            ));
        }
        if options.attribute_old_value == Some(true) && !attributes {
            return Err(Exception::type_error("attributeOldValue needs attributes"));
        }
        if options.attribute_filter.is_some() && !attributes {
            return Err(Exception::type_error("attributeFilter needs attributes"));
        }
        if options.character_data_old_value == Some(true) && !character_data {
            return Err(Exception::type_error(
                "characterDataOldValue needs characterData",
            ));
        }
        let id = observer_id(&this);
        let registered = Options {
            child_list: options.child_list,
            attributes,
            character_data,
            subtree: options.subtree,
            attribute_old_value: options.attribute_old_value.unwrap_or(false),
            character_data_old_value: options.character_data_old_value.unwrap_or(false),
            attribute_filter: options.attribute_filter,
        };
        if let Some(observer) = host.observers.iter_mut().find(|observer| observer.id == id) {
            observer.transient.retain(|(_, source, _)| *source != target);
            match observer.nodes.iter_mut().find(|(node, _)| *node == target) {
                Some(existing) => existing.1 = registered,
                None => observer.nodes.push((target, registered)),
            }
        }
        Ok(())
    }

    fn disconnect(host: &mut Host, this: JsObject) -> Result<()> {
        let id = observer_id(&this);
        if let Some(observer) = host.observers.iter_mut().find(|observer| observer.id == id) {
            observer.nodes.clear();
            observer.transient.clear();
            observer.records.clear();
        }
        Ok(())
    }

    fn take_records(host: &mut Host, this: JsObject) -> Result<Vec<JsObject>> {
        let id = observer_id(&this);
        let records = host
            .observers
            .iter_mut()
            .find(|observer| observer.id == id)
            .map(|observer| std::mem::take(&mut observer.records))
            .unwrap_or_default();
        record_objects(host, records)
    }
}

fn record_field<R>(this: &JsObject, f: impl FnOnce(&RecordData) -> R) -> Result<R> {
    let platform = this.downcast_ref::<Platform>();
    match platform.as_deref().map(|platform| &platform.payload) {
        Some(Payload::Record(record)) => Ok(f(&record.data)),
        _ => Err(Exception::type_error("Illegal invocation")),
    }
}

impl MutationRecordApi for Api {
    fn type_(_host: &mut Host, this: JsObject) -> Result<String> {
        record_field(&this, |record| record.kind.to_owned())
    }

    fn target(_host: &mut Host, this: JsObject) -> Result<NodeId> {
        record_field(&this, |record| record.target)
    }

    fn added_nodes(context: &mut Context, this: JsObject) -> Result<JsObject> {
        let (target, list) = record_field(&this, |record| (record.target, record.added.clone()))?;
        collections::static_list(context, target, list)
    }

    fn removed_nodes(context: &mut Context, this: JsObject) -> Result<JsObject> {
        let (target, list) = record_field(&this, |record| (record.target, record.removed.clone()))?;
        collections::static_list(context, target, list)
    }

    fn previous_sibling(_host: &mut Host, this: JsObject) -> Result<Option<NodeId>> {
        record_field(&this, |record| record.previous)
    }

    fn next_sibling(_host: &mut Host, this: JsObject) -> Result<Option<NodeId>> {
        record_field(&this, |record| record.next)
    }

    fn attribute_name(_host: &mut Host, this: JsObject) -> Result<Option<String>> {
        record_field(&this, |record| record.name.clone())
    }

    fn attribute_namespace(_host: &mut Host, this: JsObject) -> Result<Option<String>> {
        record_field(&this, |record| record.namespace.clone())
    }

    fn old_value(_host: &mut Host, this: JsObject) -> Result<Option<String>> {
        record_field(&this, |record| record.old_value.clone())
    }
}
