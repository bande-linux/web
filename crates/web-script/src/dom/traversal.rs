use std::cell::RefCell;
use std::rc::Rc;

use web_dom::NodeId;

use crate::bindings::*;
use crate::convert::{self, Payload, Platform};
use crate::dom::tree::{self, Result};
use crate::dom::{Api, NodeApi};
use crate::engine::{Context, Finalize, JsObject, JsValue, Trace, js_string};
use crate::exception::Exception;
use crate::host::{self, Host};

pub struct IteratorState {
    pub root: NodeId,
    pub reference: NodeId,
    pub before: bool,
}

#[derive(Trace, Finalize)]
pub struct Traversal {
    #[unsafe_ignore_trace]
    pub root: NodeId,
    #[unsafe_ignore_trace]
    pub show: u32,
    pub filter: Option<JsObject>,
    #[unsafe_ignore_trace]
    pub current: NodeId,
    #[unsafe_ignore_trace]
    pub iterator: Option<Rc<RefCell<IteratorState>>>,
    pub active: bool,
}

const ACCEPT: u16 = 1;
const REJECT: u16 = 2;
const SKIP: u16 = 3;

pub fn before_remove(host: &mut Host, node: NodeId) {
    if host.iterators.is_empty() {
        return;
    }
    host.iterators.retain(|iterator| iterator.strong_count() > 0);
    let iterators: Vec<_> = host.iterators.iter().filter_map(|weak| weak.upgrade()).collect();
    let document = &host.document;
    for iterator in iterators {
        let mut state = iterator.borrow_mut();
        if !tree::is_inclusive_ancestor(document, node, state.reference)
            || node == state.root
            || !tree::is_inclusive_ancestor(document, state.root, node)
        {
            continue;
        }
        if state.before {
            let next = tree::following_skipping_children(document, node, state.root);
            if let Some(next) = next {
                state.reference = next;
                continue;
            }
            state.before = false;
        }
        state.reference = match document.previous_sibling(node) {
            Some(previous) => {
                let mut current = previous;
                while let Some(last) = document.last_child(current) {
                    current = last;
                }
                current
            }
            None => document.parent(node).unwrap_or(state.root),
        };
    }
}

fn state<R>(this: &JsObject, f: impl FnOnce(&mut Traversal) -> R) -> Result<R> {
    let mut platform = this
        .downcast_mut::<Platform>()
        .ok_or_else(|| Exception::type_error("Illegal invocation"))?;
    match &mut platform.payload {
        Payload::Traversal(traversal) => Ok(f(traversal)),
        _ => Err(Exception::type_error("Illegal invocation")),
    }
}

pub fn create_iterator(host: &mut Host, root: NodeId, show: u32, filter: Option<JsObject>) -> Result<JsObject> {
    let Some(prototype) = host.prototype(Interface::NodeIterator) else {
        return Err(Exception::type_error("no realm"));
    };
    let iterator = Rc::new(RefCell::new(IteratorState {
        root,
        reference: root,
        before: true,
    }));
    host.iterators.push(Rc::downgrade(&iterator));
    Ok(JsObject::from_proto_and_data(
        prototype,
        Platform::new(
            Interface::NodeIterator,
            Payload::Traversal(Box::new(Traversal {
                root,
                show,
                filter,
                current: root,
                iterator: Some(iterator),
                active: false,
            })),
        ),
    ))
}

pub fn create_walker(host: &mut Host, root: NodeId, show: u32, filter: Option<JsObject>) -> Result<JsObject> {
    let Some(prototype) = host.prototype(Interface::TreeWalker) else {
        return Err(Exception::type_error("no realm"));
    };
    Ok(JsObject::from_proto_and_data(
        prototype,
        Platform::new(
            Interface::TreeWalker,
            Payload::Traversal(Box::new(Traversal {
                root,
                show,
                filter,
                current: root,
                iterator: None,
                active: false,
            })),
        ),
    ))
}

fn run_filter(context: &mut Context, this: &JsObject, node: NodeId) -> Result<u16> {
    let (show, filter, active) = state(this, |traversal| {
        (traversal.show, traversal.filter.clone(), traversal.active)
    })?;
    if active {
        return Err(Exception::dom("InvalidStateError", "the filter is already active"));
    }
    let node_type = host::with(|host| <Api as NodeApi>::node_type(host, node))??;
    if show & (1u32 << (node_type.max(1) - 1)) == 0 {
        return Ok(SKIP);
    }
    let Some(filter) = filter else {
        return Ok(ACCEPT);
    };
    state(this, |traversal| traversal.active = true)?;
    let wrapped = JsValue::from(convert::wrap(node)?);
    let result = if filter.is_callable() {
        filter.call(&JsValue::undefined(), &[wrapped], context)
    } else {
        filter.get(js_string!("acceptNode"), context).and_then(|method| match method.as_object() {
            Some(method) if method.is_callable() => {
                method.call(&JsValue::from(filter.clone()), &[wrapped], context)
            }
            _ => Err(boa_engine::JsNativeError::typ()
                .with_message("acceptNode is not callable")
                .into()),
        })
    };
    state(this, |traversal| traversal.active = false)?;
    let value = result?;
    Ok(value.to_uint16(context)?)
}

fn traverse_iterator(context: &mut Context, this: JsObject, forward: bool) -> Result<Option<NodeId>> {
    let (root, iterator) = state(&this, |traversal| (traversal.root, traversal.iterator.clone()))?;
    let Some(iterator) = iterator else {
        return Err(Exception::type_error("Illegal invocation"));
    };
    let (mut node, mut before) = {
        let current = iterator.borrow();
        (current.reference, current.before)
    };
    loop {
        if forward {
            if before {
                before = false;
            } else {
                let next = host::with(|host| tree::following(&host.document, node, root))?;
                match next {
                    Some(next) => node = next,
                    None => return Ok(None),
                }
            }
        } else if !before {
            before = true;
        } else {
            let previous = host::with(|host| tree::preceding(&host.document, node, root))?;
            match previous {
                Some(previous) => node = previous,
                None => return Ok(None),
            }
        }
        {
            let mut current = iterator.borrow_mut();
            current.reference = node;
            current.before = before;
        }
        if run_filter(context, &this, node)? == ACCEPT {
            let mut current = iterator.borrow_mut();
            current.reference = node;
            current.before = before;
            return Ok(Some(node));
        }
    }
}

impl NodeIteratorApi for Api {
    fn root(_host: &mut Host, this: JsObject) -> Result<NodeId> {
        state(&this, |traversal| traversal.root)
    }

    fn reference_node(_host: &mut Host, this: JsObject) -> Result<NodeId> {
        state(&this, |traversal| {
            traversal
                .iterator
                .as_ref()
                .map_or(traversal.root, |iterator| iterator.borrow().reference)
        })
    }

    fn pointer_before_reference_node(_host: &mut Host, this: JsObject) -> Result<bool> {
        state(&this, |traversal| {
            traversal
                .iterator
                .as_ref()
                .is_some_and(|iterator| iterator.borrow().before)
        })
    }

    fn what_to_show(_host: &mut Host, this: JsObject) -> Result<u32> {
        state(&this, |traversal| traversal.show)
    }

    fn filter(_host: &mut Host, this: JsObject) -> Result<Option<JsObject>> {
        state(&this, |traversal| traversal.filter.clone())
    }

    fn next_node(context: &mut Context, this: JsObject) -> Result<Option<NodeId>> {
        traverse_iterator(context, this, true)
    }

    fn previous_node(context: &mut Context, this: JsObject) -> Result<Option<NodeId>> {
        traverse_iterator(context, this, false)
    }

    fn detach(_host: &mut Host, _this: JsObject) -> Result<()> {
        Ok(())
    }
}

fn set_current(this: &JsObject, node: NodeId) -> Result<()> {
    state(this, |traversal| traversal.current = node)
}

fn current(this: &JsObject) -> Result<NodeId> {
    state(this, |traversal| traversal.current)
}

fn root(this: &JsObject) -> Result<NodeId> {
    state(this, |traversal| traversal.root)
}

fn traverse_children(context: &mut Context, this: &JsObject, first: bool) -> Result<Option<NodeId>> {
    let step_child = |host: &Host, node: NodeId| {
        if first {
            host.document.first_child(node)
        } else {
            host.document.last_child(node)
        }
    };
    let step_sibling = |host: &Host, node: NodeId| {
        if first {
            host.document.next_sibling(node)
        } else {
            host.document.previous_sibling(node)
        }
    };
    let start = current(this)?;
    let top = root(this)?;
    let mut node = host::with(|host| step_child(host, start))?;
    while let Some(candidate) = node {
        let result = run_filter(context, this, candidate)?;
        if result == ACCEPT {
            set_current(this, candidate)?;
            return Ok(Some(candidate));
        }
        if result == SKIP
            && let Some(child) = host::with(|host| step_child(host, candidate))?
        {
            node = Some(child);
            continue;
        }
        let mut walking = candidate;
        loop {
            if let Some(sibling) = host::with(|host| step_sibling(host, walking))? {
                node = Some(sibling);
                break;
            }
            let parent = host::with(|host| host.document.parent(walking))?;
            match parent {
                Some(parent) if parent != top && parent != start => walking = parent,
                _ => return Ok(None),
            }
        }
    }
    Ok(None)
}

fn traverse_siblings(context: &mut Context, this: &JsObject, next: bool) -> Result<Option<NodeId>> {
    let sibling = |host: &Host, node: NodeId| {
        if next {
            host.document.next_sibling(node)
        } else {
            host.document.previous_sibling(node)
        }
    };
    let child = |host: &Host, node: NodeId| {
        if next {
            host.document.first_child(node)
        } else {
            host.document.last_child(node)
        }
    };
    let mut node = current(this)?;
    let top = root(this)?;
    if node == top {
        return Ok(None);
    }
    loop {
        let mut candidate = host::with(|host| sibling(host, node))?;
        while let Some(found) = candidate {
            node = found;
            let result = run_filter(context, this, found)?;
            if result == ACCEPT {
                set_current(this, found)?;
                return Ok(Some(found));
            }
            candidate = host::with(|host| child(host, found))?;
            if result == REJECT || candidate.is_none() {
                candidate = host::with(|host| sibling(host, found))?;
            }
        }
        let parent = host::with(|host| host.document.parent(node))?;
        let Some(parent) = parent.filter(|&parent| parent != top) else {
            return Ok(None);
        };
        node = parent;
        if run_filter(context, this, node)? == ACCEPT {
            return Ok(None);
        }
    }
}

impl TreeWalkerApi for Api {
    fn root(_host: &mut Host, this: JsObject) -> Result<NodeId> {
        root(&this)
    }

    fn what_to_show(_host: &mut Host, this: JsObject) -> Result<u32> {
        state(&this, |traversal| traversal.show)
    }

    fn filter(_host: &mut Host, this: JsObject) -> Result<Option<JsObject>> {
        state(&this, |traversal| traversal.filter.clone())
    }

    fn current_node(_host: &mut Host, this: JsObject) -> Result<NodeId> {
        current(&this)
    }

    fn set_current_node(_host: &mut Host, this: JsObject, value: NodeId) -> Result<()> {
        set_current(&this, value)
    }

    fn parent_node(context: &mut Context, this: JsObject) -> Result<Option<NodeId>> {
        let mut node = current(&this)?;
        let top = root(&this)?;
        while node != top {
            let parent = host::with(|host| host.document.parent(node))?;
            let Some(parent) = parent else {
                return Ok(None);
            };
            node = parent;
            if run_filter(context, &this, node)? == ACCEPT {
                set_current(&this, node)?;
                return Ok(Some(node));
            }
        }
        Ok(None)
    }

    fn first_child(context: &mut Context, this: JsObject) -> Result<Option<NodeId>> {
        traverse_children(context, &this, true)
    }

    fn last_child(context: &mut Context, this: JsObject) -> Result<Option<NodeId>> {
        traverse_children(context, &this, false)
    }

    fn previous_sibling(context: &mut Context, this: JsObject) -> Result<Option<NodeId>> {
        traverse_siblings(context, &this, false)
    }

    fn next_sibling(context: &mut Context, this: JsObject) -> Result<Option<NodeId>> {
        traverse_siblings(context, &this, true)
    }

    fn previous_node(context: &mut Context, this: JsObject) -> Result<Option<NodeId>> {
        let mut node = current(&this)?;
        let top = root(&this)?;
        while node != top {
            let mut sibling = host::with(|host| host.document.previous_sibling(node))?;
            while let Some(found) = sibling {
                node = found;
                let mut result = run_filter(context, &this, node)?;
                while result != REJECT {
                    let Some(last) = host::with(|host| host.document.last_child(node))? else {
                        break;
                    };
                    node = last;
                    result = run_filter(context, &this, node)?;
                }
                if result == ACCEPT {
                    set_current(&this, node)?;
                    return Ok(Some(node));
                }
                sibling = host::with(|host| host.document.previous_sibling(node))?;
            }
            if node == top {
                return Ok(None);
            }
            let parent = host::with(|host| host.document.parent(node))?;
            let Some(parent) = parent else {
                return Ok(None);
            };
            node = parent;
            if run_filter(context, &this, node)? == ACCEPT {
                set_current(&this, node)?;
                return Ok(Some(node));
            }
        }
        Ok(None)
    }

    fn next_node(context: &mut Context, this: JsObject) -> Result<Option<NodeId>> {
        let mut node = current(&this)?;
        let top = root(&this)?;
        let mut result = ACCEPT;
        loop {
            while result != REJECT {
                let Some(child) = host::with(|host| host.document.first_child(node))? else {
                    break;
                };
                node = child;
                result = run_filter(context, &this, node)?;
                if result == ACCEPT {
                    set_current(&this, node)?;
                    return Ok(Some(node));
                }
            }
            let following = host::with(|host| tree::following_skipping_children(&host.document, node, top))?;
            let Some(following) = following else {
                return Ok(None);
            };
            node = following;
            result = run_filter(context, &this, node)?;
            if result == ACCEPT {
                set_current(&this, node)?;
                return Ok(Some(node));
            }
        }
    }
}
