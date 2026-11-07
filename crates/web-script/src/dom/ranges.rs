use std::cell::RefCell;
use std::rc::Rc;

use web_dom::{NodeId, NodeKind};

use crate::bindings::*;
use crate::convert::{Payload, Platform};
use crate::dom::tree::{self, Result, index_of, length_of};
use crate::dom::{self, Api, markup};
use crate::engine::{Context, JsObject};
use crate::exception::Exception;
use crate::host::Host;

#[derive(Clone, Copy)]
pub struct RangeState {
    pub start: NodeId,
    pub start_offset: u32,
    pub end: NodeId,
    pub end_offset: u32,
}

fn live_ranges(host: &mut Host) -> Vec<Rc<RefCell<RangeState>>> {
    host.ranges.retain(|range| range.strong_count() > 0);
    host.ranges.iter().filter_map(|range| range.upgrade()).collect()
}

pub fn before_insert(host: &mut Host, parent: NodeId, child: NodeId, count: u32) {
    if host.ranges.is_empty() {
        return;
    }
    let index = index_of(&host.document, child);
    for range in live_ranges(host) {
        let mut state = range.borrow_mut();
        if state.start == parent && state.start_offset > index {
            state.start_offset += count;
        }
        if state.end == parent && state.end_offset > index {
            state.end_offset += count;
        }
    }
}

pub fn before_remove(host: &mut Host, node: NodeId, parent: NodeId, index: u32) {
    if host.ranges.is_empty() {
        return;
    }
    let document_ranges = live_ranges(host);
    let document = &host.document;
    for range in document_ranges {
        let mut state = range.borrow_mut();
        if tree::is_inclusive_ancestor(document, node, state.start) {
            state.start = parent;
            state.start_offset = index;
        }
        if tree::is_inclusive_ancestor(document, node, state.end) {
            state.end = parent;
            state.end_offset = index;
        }
        if state.start == parent && state.start_offset > index {
            state.start_offset -= 1;
        }
        if state.end == parent && state.end_offset > index {
            state.end_offset -= 1;
        }
    }
}

pub fn after_replace_data(host: &mut Host, node: NodeId, offset: u32, count: u32, added: u32) {
    if host.ranges.is_empty() {
        return;
    }
    for range in live_ranges(host) {
        let mut state = range.borrow_mut();
        if state.start == node && state.start_offset > offset && state.start_offset <= offset + count {
            state.start_offset = offset;
        }
        if state.end == node && state.end_offset > offset && state.end_offset <= offset + count {
            state.end_offset = offset;
        }
        if state.start == node && state.start_offset > offset + count {
            state.start_offset = state.start_offset + added - count;
        }
        if state.end == node && state.end_offset > offset + count {
            state.end_offset = state.end_offset + added - count;
        }
    }
}

pub fn after_split(host: &mut Host, node: NodeId, new_node: NodeId, parent: NodeId, offset: u32) {
    if host.ranges.is_empty() {
        return;
    }
    let index = index_of(&host.document, node);
    for range in live_ranges(host) {
        let mut state = range.borrow_mut();
        if state.start == node && state.start_offset > offset {
            state.start = new_node;
            state.start_offset -= offset;
        }
        if state.end == node && state.end_offset > offset {
            state.end = new_node;
            state.end_offset -= offset;
        }
        if state.start == parent && state.start_offset == index + 1 {
            state.start_offset += 1;
        }
        if state.end == parent && state.end_offset == index + 1 {
            state.end_offset += 1;
        }
    }
}

pub fn after_normalize_merge(host: &mut Host, merged: NodeId, into: NodeId, offset: u32) {
    if host.ranges.is_empty() {
        return;
    }
    let index = index_of(&host.document, merged);
    let parent = host.document.parent(merged);
    for range in live_ranges(host) {
        let mut state = range.borrow_mut();
        if state.start == merged {
            state.start = into;
            state.start_offset += offset;
        }
        if state.end == merged {
            state.end = into;
            state.end_offset += offset;
        }
        if Some(state.start) == parent && state.start_offset == index {
            state.start = into;
            state.start_offset = offset;
        }
        if Some(state.end) == parent && state.end_offset == index {
            state.end = into;
            state.end_offset = offset;
        }
    }
}

pub fn create(host: &mut Host, document: NodeId) -> Result<JsObject> {
    let Some(prototype) = host.prototype(Interface::Range) else {
        return Err(Exception::type_error("no realm"));
    };
    Ok(new_range(host, prototype, RangeState {
        start: document,
        start_offset: 0,
        end: document,
        end_offset: 0,
    }))
}

fn new_range(host: &mut Host, prototype: JsObject, state: RangeState) -> JsObject {
    let shared = Rc::new(RefCell::new(state));
    host.ranges.push(Rc::downgrade(&shared));
    JsObject::from_proto_and_data(prototype, Platform::new(Interface::Range, Payload::Range(shared)))
}

fn shared(this: &JsObject) -> Result<Rc<RefCell<RangeState>>> {
    let platform = this.downcast_ref::<Platform>();
    match platform.as_deref().map(|platform| &platform.payload) {
        Some(Payload::Range(range)) => Ok(Rc::clone(range)),
        _ => Err(Exception::type_error("Illegal invocation")),
    }
}

fn read(this: &JsObject) -> Result<RangeState> {
    let platform = this.downcast_ref::<Platform>();
    match platform.as_deref().map(|platform| &platform.payload) {
        Some(Payload::Range(range)) => Ok(*range.borrow()),
        Some(Payload::Static(state)) => Ok(*state),
        _ => Err(Exception::type_error("Illegal invocation")),
    }
}

impl AbstractRangeApi for Api {
    fn start_container(_host: &mut Host, this: JsObject) -> Result<NodeId> {
        Ok(read(&this)?.start)
    }

    fn start_offset(_host: &mut Host, this: JsObject) -> Result<u32> {
        Ok(read(&this)?.start_offset)
    }

    fn end_container(_host: &mut Host, this: JsObject) -> Result<NodeId> {
        Ok(read(&this)?.end)
    }

    fn end_offset(_host: &mut Host, this: JsObject) -> Result<u32> {
        Ok(read(&this)?.end_offset)
    }

    fn collapsed(_host: &mut Host, this: JsObject) -> Result<bool> {
        let state = read(&this)?;
        Ok(state.start == state.end && state.start_offset == state.end_offset)
    }
}

impl StaticRangeApi for Api {
    fn constructor(_context: &mut Context, prototype: JsObject, init: StaticRangeInit) -> Result<JsObject> {
        crate::host::with(|host| {
            let bad = |node: NodeId| {
                matches!(
                    host.document.kind(node),
                    Some(NodeKind::Doctype | NodeKind::Attribute)
                )
            };
            if bad(init.start_container) || bad(init.end_container) {
                return Err(Exception::dom("InvalidNodeTypeError", "the container is not valid"));
            }
            Ok(JsObject::from_proto_and_data(
                prototype,
                Platform::new(
                    Interface::StaticRange,
                    Payload::Static(RangeState {
                        start: init.start_container,
                        start_offset: init.start_offset,
                        end: init.end_container,
                        end_offset: init.end_offset,
                    }),
                ),
            ))
        })?
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Order {
    Before,
    Equal,
    After,
}

fn boundary_order(host: &Host, a: (NodeId, u32), b: (NodeId, u32)) -> Order {
    let document = &host.document;
    if a.0 == b.0 {
        return match a.1.cmp(&b.1) {
            std::cmp::Ordering::Less => Order::Before,
            std::cmp::Ordering::Equal => Order::Equal,
            std::cmp::Ordering::Greater => Order::After,
        };
    }
    if dom::position_before(document, b.0, a.0) {
        return match boundary_order(host, b, a) {
            Order::Before => Order::After,
            Order::After => Order::Before,
            Order::Equal => Order::Equal,
        };
    }
    if tree::is_inclusive_ancestor(document, a.0, b.0) {
        let mut child = b.0;
        while document.parent(child) != Some(a.0) {
            match document.parent(child) {
                Some(parent) => child = parent,
                None => break,
            }
        }
        if index_of(document, child) < a.1 {
            return Order::After;
        }
    }
    Order::Before
}

fn check_boundary(host: &Host, node: NodeId, offset: u32) -> Result<()> {
    if host.document.kind(node) == Some(NodeKind::Doctype) {
        return Err(Exception::dom("InvalidNodeTypeError", "a doctype cannot hold a boundary"));
    }
    if offset > length_of(&host.document, node) {
        return Err(Exception::dom("IndexSizeError", "the offset is past the end"));
    }
    Ok(())
}

fn set_start(host: &Host, this: &JsObject, node: NodeId, offset: u32) -> Result<()> {
    check_boundary(host, node, offset)?;
    let range = shared(this)?;
    let mut state = range.borrow_mut();
    let other_root = tree::root_of(&host.document, state.end) != tree::root_of(&host.document, node);
    if other_root || boundary_order(host, (node, offset), (state.end, state.end_offset)) == Order::After {
        state.end = node;
        state.end_offset = offset;
    }
    state.start = node;
    state.start_offset = offset;
    Ok(())
}

fn set_end(host: &Host, this: &JsObject, node: NodeId, offset: u32) -> Result<()> {
    check_boundary(host, node, offset)?;
    let range = shared(this)?;
    let mut state = range.borrow_mut();
    let other_root = tree::root_of(&host.document, state.start) != tree::root_of(&host.document, node);
    if other_root || boundary_order(host, (node, offset), (state.start, state.start_offset)) == Order::Before {
        state.start = node;
        state.start_offset = offset;
    }
    state.end = node;
    state.end_offset = offset;
    Ok(())
}

fn parent_of(host: &Host, node: NodeId) -> Result<NodeId> {
    host.document
        .parent(node)
        .ok_or_else(|| Exception::dom("InvalidNodeTypeError", "the node has no parent"))
}

fn common_ancestor(host: &Host, state: &RangeState) -> NodeId {
    host.document
        .ancestors(state.start)
        .find(|&ancestor| tree::is_inclusive_ancestor(&host.document, ancestor, state.end))
        .unwrap_or(state.start)
}

fn contained(host: &Host, node: NodeId, state: &RangeState) -> bool {
    let Some(parent) = host.document.parent(node) else {
        return false;
    };
    let index = index_of(&host.document, node);
    tree::root_of(&host.document, node) == tree::root_of(&host.document, state.start)
        && boundary_order(host, (parent, index), (state.start, state.start_offset)) == Order::After
        && boundary_order(host, (parent, index + 1), (state.end, state.end_offset)) == Order::Before
}

fn partially_contained(host: &Host, node: NodeId, state: &RangeState) -> bool {
    let start = tree::is_inclusive_ancestor(&host.document, node, state.start);
    let end = tree::is_inclusive_ancestor(&host.document, node, state.end);
    start != end
}

fn is_data(host: &Host, node: NodeId) -> bool {
    tree::is_character_data(&host.document, node)
}

fn clone_data(host: &mut Host, node: NodeId, offset: u32, count: u32) -> NodeId {
    let owner = host.node_document(node);
    let copy = tree::clone_node(host, node, owner, false);
    let data = tree::utf16_slice(host.document.text(node).unwrap_or_default(), offset, count);
    host.document.set_text(copy, &data);
    copy
}

fn delete_contents(host: &mut Host, this: &JsObject) -> Result<()> {
    let state = read(this)?;
    if state.start == state.end && state.start_offset == state.end_offset {
        return Ok(());
    }
    if state.start == state.end && is_data(host, state.start) {
        return tree::replace_data(host, state.start, state.start_offset, state.end_offset - state.start_offset, "");
    }
    let common = common_ancestor(host, &state);
    let doomed: Vec<NodeId> = tree::descendants(&host.document, common)
        .into_iter()
        .filter(|&node| {
            contained(host, node, &state)
                && !host
                    .document
                    .parent(node)
                    .is_some_and(|parent| contained(host, parent, &state))
        })
        .collect();
    let (new_node, new_offset) = if tree::is_inclusive_ancestor(&host.document, state.start, state.end) {
        (state.start, state.start_offset)
    } else {
        let mut reference = state.start;
        while let Some(parent) = host.document.parent(reference) {
            if tree::is_inclusive_ancestor(&host.document, parent, state.end) {
                break;
            }
            reference = parent;
        }
        let parent = host.document.parent(reference).unwrap_or(reference);
        (parent, index_of(&host.document, reference) + 1)
    };
    if is_data(host, state.start) {
        let length = length_of(&host.document, state.start);
        tree::replace_data(host, state.start, state.start_offset, length - state.start_offset, "")?;
    }
    for node in doomed {
        tree::remove(host, node, false);
    }
    if is_data(host, state.end) {
        tree::replace_data(host, state.end, 0, state.end_offset, "")?;
    }
    let range = shared(this)?;
    let mut current = range.borrow_mut();
    current.start = new_node;
    current.start_offset = new_offset;
    current.end = new_node;
    current.end_offset = new_offset;
    Ok(())
}

fn subrange(host: &Host, start: (NodeId, u32), end: (NodeId, u32)) -> RangeState {
    let _ = host;
    RangeState {
        start: start.0,
        start_offset: start.1,
        end: end.0,
        end_offset: end.1,
    }
}

fn transfer(host: &mut Host, state: RangeState, extract: bool) -> Result<(NodeId, Option<(NodeId, u32)>)> {
    let owner = host.node_document(state.start);
    let fragment = host.document.create_fragment();
    host.set_owner(fragment, owner);
    if state.start == state.end && state.start_offset == state.end_offset {
        return Ok((fragment, None));
    }
    if state.start == state.end && is_data(host, state.start) {
        let copy = clone_data(host, state.start, state.start_offset, state.end_offset - state.start_offset);
        host.document.insert(fragment, copy, None);
        if extract {
            tree::replace_data(host, state.start, state.start_offset, state.end_offset - state.start_offset, "")?;
        }
        return Ok((fragment, Some((state.start, state.start_offset))));
    }
    let common = common_ancestor(host, &state);
    let start_is_ancestor = tree::is_inclusive_ancestor(&host.document, state.start, state.end);
    let end_is_ancestor = tree::is_inclusive_ancestor(&host.document, state.end, state.start);
    let first = (!start_is_ancestor)
        .then(|| {
            host.document
                .children(common)
                .find(|&child| partially_contained(host, child, &state))
        })
        .flatten();
    let last = (!end_is_ancestor)
        .then(|| {
            std::iter::successors(host.document.last_child(common), |&node| host.document.previous_sibling(node))
                .find(|&child| partially_contained(host, child, &state))
        })
        .flatten();
    let children: Vec<NodeId> = host.document.children(common).collect();
    let middle: Vec<NodeId> = children.into_iter().filter(|&child| contained(host, child, &state)).collect();
    if middle.iter().any(|&child| host.document.kind(child) == Some(NodeKind::Doctype)) {
        return Err(Exception::dom("HierarchyRequestError", "a doctype cannot be moved"));
    }
    let new_position = if start_is_ancestor {
        (state.start, state.start_offset)
    } else {
        let mut reference = state.start;
        while let Some(parent) = host.document.parent(reference) {
            if tree::is_inclusive_ancestor(&host.document, parent, state.end) {
                break;
            }
            reference = parent;
        }
        let parent = host.document.parent(reference).unwrap_or(reference);
        (parent, index_of(&host.document, reference) + 1)
    };
    if let Some(first) = first {
        if is_data(host, first) {
            let length = length_of(&host.document, state.start);
            let copy = clone_data(host, state.start, state.start_offset, length - state.start_offset);
            host.document.insert(fragment, copy, None);
            if extract {
                tree::replace_data(host, state.start, state.start_offset, length - state.start_offset, "")?;
            }
        } else {
            let copy = tree::clone_node(host, first, owner, false);
            host.document.insert(fragment, copy, None);
            let inner = subrange(host, (state.start, state.start_offset), (first, length_of(&host.document, first)));
            let (sub, _) = transfer(host, inner, extract)?;
            tree::insert(host, sub, copy, None, true);
        }
    }
    for child in middle {
        if extract {
            tree::remove(host, child, false);
            host.document.insert(fragment, child, None);
        } else {
            let copy = tree::clone_node(host, child, owner, true);
            host.document.insert(fragment, copy, None);
        }
    }
    if let Some(last) = last {
        if is_data(host, last) {
            let copy = clone_data(host, state.end, 0, state.end_offset);
            host.document.insert(fragment, copy, None);
            if extract {
                tree::replace_data(host, state.end, 0, state.end_offset, "")?;
            }
        } else {
            let copy = tree::clone_node(host, last, owner, false);
            host.document.insert(fragment, copy, None);
            let inner = subrange(host, (last, 0), (state.end, state.end_offset));
            let (sub, _) = transfer(host, inner, extract)?;
            tree::insert(host, sub, copy, None, true);
        }
    }
    Ok((fragment, Some(new_position)))
}

fn insert_node(host: &mut Host, this: &JsObject, node: NodeId) -> Result<()> {
    let state = read(this)?;
    let start = state.start;
    if matches!(host.document.kind(start), Some(NodeKind::ProcessingInstruction | NodeKind::Comment))
        || (host.document.kind(start) == Some(NodeKind::Text) && host.document.parent(start).is_none())
        || start == node
    {
        return Err(Exception::dom("HierarchyRequestError", "cannot insert here"));
    }
    let mut reference = if host.document.kind(start) == Some(NodeKind::Text) {
        Some(start)
    } else {
        host.document.children(start).nth(state.start_offset as usize)
    };
    let parent = if host.document.kind(start) == Some(NodeKind::Text) {
        parent_of(host, start)?
    } else {
        start
    };
    tree::ensure_insertable(host, parent, node, reference)?;
    if host.document.kind(start) == Some(NodeKind::Text) {
        reference = Some(<Api as TextApi>::split_text(host, start, state.start_offset)?);
    }
    if reference == Some(node) {
        reference = host.document.next_sibling(node);
    }
    if host.document.parent(node).is_some() {
        tree::remove(host, node, false);
    }
    let new_offset = match reference {
        Some(reference) => index_of(&host.document, reference),
        None => length_of(&host.document, parent),
    };
    let added = if host.document.kind(node) == Some(NodeKind::Fragment) {
        length_of(&host.document, node)
    } else {
        1
    };
    tree::pre_insert(host, node, parent, reference)?;
    let range = shared(this)?;
    let mut current = range.borrow_mut();
    if current.start == current.end && current.start_offset == current.end_offset {
        current.end = parent;
        current.end_offset = new_offset + added;
    }
    Ok(())
}

fn range_text(host: &Host, state: &RangeState) -> String {
    let document = &host.document;
    if state.start == state.end && document.kind(state.start) == Some(NodeKind::Text) {
        return tree::utf16_slice(
            document.text(state.start).unwrap_or_default(),
            state.start_offset,
            state.end_offset - state.start_offset,
        );
    }
    let mut out = String::new();
    if document.kind(state.start) == Some(NodeKind::Text) {
        out.push_str(&tree::utf16_slice(
            document.text(state.start).unwrap_or_default(),
            state.start_offset,
            u32::MAX,
        ));
    }
    let root = tree::root_of(document, state.start);
    for node in tree::descendants(document, root) {
        if document.kind(node) == Some(NodeKind::Text) && contained(host, node, state) {
            out.push_str(document.text(node).unwrap_or_default());
        }
    }
    if document.kind(state.end) == Some(NodeKind::Text) {
        out.push_str(&tree::utf16_slice(
            document.text(state.end).unwrap_or_default(),
            0,
            state.end_offset,
        ));
    }
    out
}

impl RangeApi for Api {
    fn constructor(_context: &mut Context, prototype: JsObject) -> Result<JsObject> {
        Ok(crate::host::with(|host| {
            new_range(
                host,
                prototype,
                RangeState {
                    start: NodeId::DOCUMENT,
                    start_offset: 0,
                    end: NodeId::DOCUMENT,
                    end_offset: 0,
                },
            )
        })?)
    }

    fn common_ancestor_container(host: &mut Host, this: JsObject) -> Result<NodeId> {
        let state = read(&this)?;
        Ok(common_ancestor(host, &state))
    }

    fn set_start(host: &mut Host, this: JsObject, node: NodeId, offset: u32) -> Result<()> {
        set_start(host, &this, node, offset)
    }

    fn set_end(host: &mut Host, this: JsObject, node: NodeId, offset: u32) -> Result<()> {
        set_end(host, &this, node, offset)
    }

    fn set_start_before(host: &mut Host, this: JsObject, node: NodeId) -> Result<()> {
        let parent = parent_of(host, node)?;
        set_start(host, &this, parent, index_of(&host.document, node))
    }

    fn set_start_after(host: &mut Host, this: JsObject, node: NodeId) -> Result<()> {
        let parent = parent_of(host, node)?;
        set_start(host, &this, parent, index_of(&host.document, node) + 1)
    }

    fn set_end_before(host: &mut Host, this: JsObject, node: NodeId) -> Result<()> {
        let parent = parent_of(host, node)?;
        set_end(host, &this, parent, index_of(&host.document, node))
    }

    fn set_end_after(host: &mut Host, this: JsObject, node: NodeId) -> Result<()> {
        let parent = parent_of(host, node)?;
        set_end(host, &this, parent, index_of(&host.document, node) + 1)
    }

    fn collapse(_host: &mut Host, this: JsObject, to_start: bool) -> Result<()> {
        let range = shared(&this)?;
        let mut state = range.borrow_mut();
        if to_start {
            state.end = state.start;
            state.end_offset = state.start_offset;
        } else {
            state.start = state.end;
            state.start_offset = state.end_offset;
        }
        Ok(())
    }

    fn select_node(host: &mut Host, this: JsObject, node: NodeId) -> Result<()> {
        let parent = parent_of(host, node)?;
        let index = index_of(&host.document, node);
        let range = shared(&this)?;
        *range.borrow_mut() = RangeState {
            start: parent,
            start_offset: index,
            end: parent,
            end_offset: index + 1,
        };
        Ok(())
    }

    fn select_node_contents(host: &mut Host, this: JsObject, node: NodeId) -> Result<()> {
        if host.document.kind(node) == Some(NodeKind::Doctype) {
            return Err(Exception::dom("InvalidNodeTypeError", "a doctype cannot hold a boundary"));
        }
        let length = length_of(&host.document, node);
        let range = shared(&this)?;
        *range.borrow_mut() = RangeState {
            start: node,
            start_offset: 0,
            end: node,
            end_offset: length,
        };
        Ok(())
    }

    fn compare_boundary_points(host: &mut Host, this: JsObject, how: u16, source_range: JsObject) -> Result<i16> {
        let state = read(&this)?;
        let source = read(&source_range)?;
        if how > 3 {
            return Err(Exception::dom("NotSupportedError", "the comparison is not valid"));
        }
        if tree::root_of(&host.document, state.start) != tree::root_of(&host.document, source.start) {
            return Err(Exception::dom("WrongDocumentError", "the ranges are in different trees"));
        }
        let (mine, theirs) = match how {
            0 => ((state.start, state.start_offset), (source.start, source.start_offset)),
            1 => ((state.end, state.end_offset), (source.start, source.start_offset)),
            2 => ((state.end, state.end_offset), (source.end, source.end_offset)),
            _ => ((state.start, state.start_offset), (source.end, source.end_offset)),
        };
        Ok(match boundary_order(host, mine, theirs) {
            Order::Before => -1,
            Order::Equal => 0,
            Order::After => 1,
        })
    }

    fn delete_contents(host: &mut Host, this: JsObject) -> Result<()> {
        delete_contents(host, &this)
    }

    fn extract_contents(host: &mut Host, this: JsObject) -> Result<NodeId> {
        let state = read(&this)?;
        let (fragment, position) = transfer(host, state, true)?;
        if let Some((node, offset)) = position {
            let range = shared(&this)?;
            *range.borrow_mut() = RangeState {
                start: node,
                start_offset: offset,
                end: node,
                end_offset: offset,
            };
        }
        Ok(fragment)
    }

    fn clone_contents(host: &mut Host, this: JsObject) -> Result<NodeId> {
        let state = read(&this)?;
        Ok(transfer(host, state, false)?.0)
    }

    fn insert_node(host: &mut Host, this: JsObject, node: NodeId) -> Result<()> {
        insert_node(host, &this, node)
    }

    fn surround_contents(host: &mut Host, this: JsObject, new_parent: NodeId) -> Result<()> {
        let state = read(&this)?;
        let common = common_ancestor(host, &state);
        let partial = tree::descendants(&host.document, common).into_iter().any(|node| {
            !is_data(host, node) && partially_contained(host, node, &state)
        });
        if partial {
            return Err(Exception::dom("InvalidStateError", "the range partially contains a node"));
        }
        if matches!(
            host.document.kind(new_parent),
            Some(NodeKind::Document | NodeKind::Doctype | NodeKind::Fragment)
        ) {
            return Err(Exception::dom("InvalidNodeTypeError", "this node cannot surround"));
        }
        let (fragment, position) = transfer(host, state, true)?;
        if let Some((node, offset)) = position {
            let range = shared(&this)?;
            *range.borrow_mut() = RangeState {
                start: node,
                start_offset: offset,
                end: node,
                end_offset: offset,
            };
        }
        if host.document.first_child(new_parent).is_some() {
            tree::replace_all(host, None, new_parent);
        }
        insert_node(host, &this, new_parent)?;
        tree::pre_insert(host, fragment, new_parent, None)?;
        <Api as RangeApi>::select_node(host, this, new_parent)
    }

    fn clone_range(host: &mut Host, this: JsObject) -> Result<JsObject> {
        let state = read(&this)?;
        let Some(prototype) = host.prototype(Interface::Range) else {
            return Err(Exception::type_error("no realm"));
        };
        Ok(new_range(host, prototype, state))
    }

    fn detach(_host: &mut Host, _this: JsObject) -> Result<()> {
        Ok(())
    }

    fn is_point_in_range(host: &mut Host, this: JsObject, node: NodeId, offset: u32) -> Result<bool> {
        let state = read(&this)?;
        if tree::root_of(&host.document, node) != tree::root_of(&host.document, state.start) {
            return Ok(false);
        }
        check_boundary(host, node, offset)?;
        Ok(boundary_order(host, (node, offset), (state.start, state.start_offset)) != Order::Before
            && boundary_order(host, (node, offset), (state.end, state.end_offset)) != Order::After)
    }

    fn compare_point(host: &mut Host, this: JsObject, node: NodeId, offset: u32) -> Result<i16> {
        let state = read(&this)?;
        if tree::root_of(&host.document, node) != tree::root_of(&host.document, state.start) {
            return Err(Exception::dom("WrongDocumentError", "the node is in another tree"));
        }
        check_boundary(host, node, offset)?;
        if boundary_order(host, (node, offset), (state.start, state.start_offset)) == Order::Before {
            return Ok(-1);
        }
        if boundary_order(host, (node, offset), (state.end, state.end_offset)) == Order::After {
            return Ok(1);
        }
        Ok(0)
    }

    fn intersects_node(host: &mut Host, this: JsObject, node: NodeId) -> Result<bool> {
        let state = read(&this)?;
        if tree::root_of(&host.document, node) != tree::root_of(&host.document, state.start) {
            return Ok(false);
        }
        let Some(parent) = host.document.parent(node) else {
            return Ok(true);
        };
        let index = index_of(&host.document, node);
        Ok(boundary_order(host, (parent, index), (state.end, state.end_offset)) == Order::Before
            && boundary_order(host, (parent, index + 1), (state.start, state.start_offset)) == Order::After)
    }

    fn to_string(host: &mut Host, this: JsObject) -> Result<String> {
        let state = read(&this)?;
        Ok(range_text(host, &state))
    }

    fn create_contextual_fragment(host: &mut Host, this: JsObject, string: String) -> Result<NodeId> {
        let state = read(&this)?;
        let element = match host.document.kind(state.start) {
            Some(NodeKind::Element) => Some(state.start),
            Some(NodeKind::Text | NodeKind::Comment | NodeKind::ProcessingInstruction) => host
                .document
                .parent(state.start)
                .filter(|&parent| tree::is_element(&host.document, parent)),
            _ => None,
        };
        let context = element
            .and_then(|element| host.document.element_name(element))
            .filter(|&(namespace, name)| {
                !(namespace == web_atoms::Namespace::Html && name == web_atoms::Atom::HTML)
            })
            .map_or(web_atoms::QualName::html(web_atoms::Atom::BODY), |(ns, local)| {
                web_atoms::QualName { ns, local }
            });
        let owner = host.node_document(state.start);
        let fragment = host.document.parse_fragment(context, &string, true);
        host.set_owner(fragment, owner);
        for node in tree::descendants(&host.document, fragment) {
            host.set_owner(node, owner);
        }
        let _ = markup::is_html_namespace;
        Ok(fragment)
    }
}
