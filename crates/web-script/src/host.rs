use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Weak;

use url::Url;
use web_atoms::{Atom, Namespace, QualName};
use web_dom::{Document, NodeId, NodeKind};

use crate::bindings::{Interface, html_interface};
use crate::convert::{Payload, Platform};
use crate::dom::observer::Observer;
use crate::dom::ranges::RangeState;
use crate::dom::traversal::IteratorState;
use crate::engine::{JsNativeError, JsObject, JsResult, JsValue};
use crate::event_loop::{Task, Timers};
use crate::events::Listener;

pub struct DocumentInfo {
    pub interface: Interface,
    pub html: bool,
    pub content_type: String,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
}

#[derive(Default)]
pub struct Requests {
    pub scripts: Vec<(NodeId, String)>,
    pub navigate: Option<(String, bool)>,
    pub frame: bool,
}

pub struct Host {
    pub document: Document,
    pub url: Option<Url>,
    pub wrappers: Vec<Option<JsObject>>,
    pub prototypes: Vec<JsObject>,
    pub global: Option<JsObject>,
    pub proxy_handler: Option<JsObject>,
    pub proxy_constructor: Option<JsObject>,
    pub proxy_key: Option<crate::engine::JsSymbol>,
    pub function_constructor: Option<JsObject>,
    pub console: Vec<String>,
    pub mutated: bool,
    pub styles_changed: bool,
    pub version: u64,
    pub now: f64,
    pub time_origin: f64,
    pub viewport: Viewport,
    pub requests: Requests,
    pub ready_state: &'static str,
    pub current_script: Option<NodeId>,
    pub current_event: Option<JsObject>,
    pub script_depth: u32,
    pub window_listeners: Vec<Listener>,
    pub next_listener: u64,
    pub owners: HashMap<NodeId, NodeId>,
    pub documents: HashMap<NodeId, DocumentInfo>,
    pub element_prefixes: HashMap<NodeId, Atom>,
    pub attribute_prefixes: HashMap<(NodeId, QualName), Atom>,
    pub other_namespaces: HashMap<NodeId, String>,
    pub other_attribute_namespaces: HashMap<(NodeId, QualName), String>,
    pub cdata: HashSet<NodeId>,
    pub attribute_nodes: HashMap<(NodeId, QualName), NodeId>,
    pub attribute_owners: HashMap<NodeId, NodeId>,
    pub detached_attribute_namespace: HashMap<NodeId, String>,
    pub detached_attribute_prefix: HashMap<NodeId, Atom>,
    pub same_objects: HashMap<(NodeId, u8), JsObject>,
    pub window_objects: Vec<(u8, JsObject)>,
    pub implementations: HashMap<NodeId, JsObject>,
    pub started_scripts: HashSet<NodeId>,
    pub pending_scripts: Vec<NodeId>,
    pub ranges: Vec<Weak<RefCell<RangeState>>>,
    pub iterators: Vec<Weak<RefCell<IteratorState>>>,
    pub observers: Vec<Observer>,
    pub next_observer: u64,
    pub observed: HashSet<NodeId>,
    pub transient: Vec<(NodeId, NodeId, u64)>,
    pub observer_microtask: bool,
    pub tasks: VecDeque<Task>,
    pub timers: Timers,
    pub frame_callbacks: Vec<(u32, JsObject)>,
    pub next_frame: u32,
    pub pending_rejections: Vec<JsObject>,
    pub unsettled_rejections: Vec<JsObject>,
    pub history_state: JsValue,
    pub name: String,
    pub reported: u32,
}

impl Host {
    pub fn new(url: Option<Url>, viewport: Viewport) -> Host {
        let mut documents = HashMap::new();
        documents.insert(
            NodeId::DOCUMENT,
            DocumentInfo {
                interface: Interface::HTMLDocument,
                html: true,
                content_type: "text/html".to_owned(),
            },
        );
        Host {
            document: Document::new(),
            url,
            wrappers: Vec::new(),
            prototypes: Vec::new(),
            global: None,
            proxy_handler: None,
            proxy_constructor: None,
            proxy_key: None,
            function_constructor: None,
            console: Vec::new(),
            mutated: false,
            styles_changed: false,
            version: 0,
            now: 0.0,
            time_origin: 0.0,
            viewport,
            requests: Requests::default(),
            ready_state: "loading",
            current_script: None,
            current_event: None,
            script_depth: 0,
            window_listeners: Vec::new(),
            next_listener: 0,
            owners: HashMap::new(),
            documents,
            element_prefixes: HashMap::new(),
            attribute_prefixes: HashMap::new(),
            other_namespaces: HashMap::new(),
            other_attribute_namespaces: HashMap::new(),
            cdata: HashSet::new(),
            attribute_nodes: HashMap::new(),
            attribute_owners: HashMap::new(),
            detached_attribute_namespace: HashMap::new(),
            detached_attribute_prefix: HashMap::new(),
            same_objects: HashMap::new(),
            window_objects: Vec::new(),
            implementations: HashMap::new(),
            started_scripts: HashSet::new(),
            pending_scripts: Vec::new(),
            ranges: Vec::new(),
            iterators: Vec::new(),
            observers: Vec::new(),
            next_observer: 0,
            observed: HashSet::new(),
            transient: Vec::new(),
            observer_microtask: false,
            tasks: VecDeque::new(),
            timers: Timers::default(),
            frame_callbacks: Vec::new(),
            next_frame: 0,
            pending_rejections: Vec::new(),
            unsettled_rejections: Vec::new(),
            history_state: JsValue::null(),
            name: String::new(),
            reported: 0,
        }
    }

    pub fn prototype(&self, interface: Interface) -> Option<JsObject> {
        self.prototypes.get(interface as usize).cloned()
    }

    pub fn wrapper(&self, node: NodeId) -> Option<&JsObject> {
        self.wrappers.get(node.index())?.as_ref()
    }

    pub fn interface_of(&self, node: NodeId) -> Option<Interface> {
        let document = &self.document;
        Some(match document.kind(node)? {
            NodeKind::Document => self.documents.get(&node)?.interface,
            NodeKind::Doctype => Interface::DocumentType,
            NodeKind::Element => match document.element_name(node) {
                Some((Namespace::Html, name)) => html_interface(document.atoms().name(name))
                    .unwrap_or_else(|| html_fallback(document.atoms().name(name))),
                _ => Interface::Element,
            },
            NodeKind::Text => {
                if self.cdata.contains(&node) {
                    Interface::CDATASection
                } else {
                    Interface::Text
                }
            }
            NodeKind::Comment => Interface::Comment,
            NodeKind::ProcessingInstruction => Interface::ProcessingInstruction,
            NodeKind::Fragment => Interface::DocumentFragment,
            NodeKind::Attribute => Interface::Attr,
        })
    }

    pub fn wrap(&mut self, node: NodeId) -> Option<JsObject> {
        if let Some(wrapper) = self.wrapper(node) {
            return Some(wrapper.clone());
        }
        let interface = self.interface_of(node)?;
        let prototype = self.prototype(interface)?;
        self.wrap_with_prototype(node, prototype)
    }

    pub fn wrap_with_prototype(&mut self, node: NodeId, prototype: JsObject) -> Option<JsObject> {
        let interface = self.interface_of(node)?;
        let object =
            JsObject::from_proto_and_data(prototype, Platform::new(interface, Payload::Node(node)));
        let index = node.index();
        if self.wrappers.len() <= index {
            self.wrappers.resize(index + 1, None);
        }
        if let Some(slot) = self.wrappers.get_mut(index) {
            *slot = Some(object.clone());
        }
        Some(object)
    }

    pub fn is_html(&self, node_document: NodeId) -> bool {
        self.documents
            .get(&node_document)
            .is_some_and(|info| info.html)
    }

    pub fn owner(&self, node: NodeId) -> NodeId {
        self.owners.get(&node).copied().unwrap_or(NodeId::DOCUMENT)
    }

    pub fn node_document(&self, node: NodeId) -> NodeId {
        if self.document.kind(node) == Some(NodeKind::Document) {
            node
        } else {
            self.owner(node)
        }
    }

    pub fn set_owner(&mut self, node: NodeId, owner: NodeId) {
        if owner == NodeId::DOCUMENT {
            self.owners.remove(&node);
        } else {
            self.owners.insert(node, owner);
        }
    }

    pub fn touched(&mut self) {
        self.version += 1;
        self.mutated = true;
    }
}

fn html_fallback(name: &str) -> Interface {
    const KNOWN: &[&str] = &[
        "abbr", "acronym", "address", "article", "aside", "b", "basefont", "bdi", "bdo", "big",
        "center", "cite", "code", "dd", "dfn", "dt", "em", "figcaption", "figure", "footer", "header",
        "hgroup", "i", "kbd", "main", "mark", "nav", "nobr", "noembed", "noframes", "noscript", "plaintext",
        "rb", "rp", "rt", "rtc", "ruby", "s", "samp", "search", "section", "small", "strike", "strong",
        "sub", "summary", "sup", "tt", "u", "var", "wbr",
    ];
    if KNOWN.contains(&name) || name.contains('-') {
        Interface::HTMLElement
    } else {
        Interface::HTMLUnknownElement
    }
}

thread_local! {
    static HOST: RefCell<Option<Box<Host>>> = const { RefCell::new(None) };
    static PENDING: Cell<bool> = const { Cell::new(false) };
}

pub fn mark_pending() {
    PENDING.with(|pending| pending.set(true));
}

pub fn pending() -> bool {
    PENDING.with(Cell::take)
}

pub fn enter(host: Box<Host>) {
    HOST.with_borrow_mut(|slot| *slot = Some(host));
}

pub fn leave() -> Option<Box<Host>> {
    HOST.with_borrow_mut(Option::take)
}

pub fn with<R>(f: impl FnOnce(&mut Host) -> R) -> JsResult<R> {
    HOST.with(|cell| {
        let mut borrowed = cell
            .try_borrow_mut()
            .map_err(|_| JsNativeError::error().with_message("re-entered the DOM"))?;
        let host = borrowed
            .as_mut()
            .ok_or_else(|| JsNativeError::error().with_message("no document"))?;
        Ok(f(host))
    })
}

pub fn is_global(object: &JsObject) -> bool {
    HOST.with(|cell| {
        cell.try_borrow()
            .ok()
            .and_then(|host| {
                host.as_ref()
                    .and_then(|host| host.global.as_ref().map(|global| JsObject::equals(global, object)))
            })
            .unwrap_or(false)
    })
}
