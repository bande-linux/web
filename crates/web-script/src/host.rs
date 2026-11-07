use std::cell::RefCell;

use web_dom::{Document, NodeId};

use crate::bindings::Interface;
use crate::engine::{JsNativeError, JsObject, JsResult};

pub struct Host {
    pub document: Document,
    pub wrappers: Vec<Option<JsObject>>,
    pub prototypes: Vec<JsObject>,
    pub console: Vec<String>,
    pub mutated: bool,
}

impl Host {
    pub fn prototype(&self, interface: Interface) -> Option<&JsObject> {
        let index = Interface::ALL
            .iter()
            .position(|known| *known == interface)?;
        self.prototypes.get(index)
    }

    pub fn wrapper(&self, node: NodeId) -> Option<&JsObject> {
        self.wrappers.get(node.index())?.as_ref()
    }
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

pub fn enter(host: Host) {
    HOST.with_borrow_mut(|slot| *slot = Some(host));
}

pub fn leave() -> Option<Host> {
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
