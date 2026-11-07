use web_dom::NodeId;

use crate::bindings::*;
use crate::dom::tree::{self, Result};
use crate::dom::{self, Api, ranges};
use crate::engine::{Context, JsObject};
use crate::exception::Exception;
use crate::host::Host;

impl CharacterDataApi for Api {
    fn data(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(host.document.text(this).unwrap_or_default().to_owned())
    }

    fn set_data(host: &mut Host, this: NodeId, value: String) -> Result<()> {
        tree::set_all_data(host, this, &value)
    }

    fn length(host: &mut Host, this: NodeId) -> Result<u32> {
        Ok(tree::utf16_length(host.document.text(this).unwrap_or_default()))
    }

    fn substring_data(host: &mut Host, this: NodeId, offset: u32, count: u32) -> Result<String> {
        let text = host.document.text(this).unwrap_or_default();
        if offset > tree::utf16_length(text) {
            return Err(Exception::dom("IndexSizeError", "the offset is past the end"));
        }
        Ok(tree::utf16_slice(text, offset, count))
    }

    fn append_data(host: &mut Host, this: NodeId, data: String) -> Result<()> {
        let length = tree::utf16_length(host.document.text(this).unwrap_or_default());
        tree::replace_data(host, this, length, 0, &data)
    }

    fn insert_data(host: &mut Host, this: NodeId, offset: u32, data: String) -> Result<()> {
        tree::replace_data(host, this, offset, 0, &data)
    }

    fn delete_data(host: &mut Host, this: NodeId, offset: u32, count: u32) -> Result<()> {
        tree::replace_data(host, this, offset, count, "")
    }

    fn replace_data(host: &mut Host, this: NodeId, offset: u32, count: u32, data: String) -> Result<()> {
        tree::replace_data(host, this, offset, count, &data)
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

impl TextApi for Api {
    fn constructor(_context: &mut Context, prototype: JsObject, data: String) -> Result<JsObject> {
        let node = crate::host::with(|host| host.document.create_text(&data))?;
        Ok(crate::convert::wrap_with_prototype(node, prototype)?)
    }

    fn split_text(host: &mut Host, this: NodeId, offset: u32) -> Result<NodeId> {
        let text = host.document.text(this).unwrap_or_default().to_owned();
        let length = tree::utf16_length(&text);
        if offset > length {
            return Err(Exception::dom("IndexSizeError", "the offset is past the end"));
        }
        let new_data = tree::utf16_slice(&text, offset, length - offset);
        let new_node = host.document.create_text(&new_data);
        let owner = host.owner(this);
        host.set_owner(new_node, owner);
        if host.cdata.contains(&this) {
            host.cdata.insert(new_node);
        }
        if let Some(parent) = host.document.parent(this) {
            let next = host.document.next_sibling(this);
            tree::insert(host, new_node, parent, next, false);
            ranges::after_split(host, this, new_node, parent, offset);
        }
        tree::replace_data(host, this, offset, length - offset, "")?;
        Ok(new_node)
    }

    fn whole_text(host: &mut Host, this: NodeId) -> Result<String> {
        let document = &host.document;
        let is_text = |node: NodeId| document.kind(node) == Some(web_dom::NodeKind::Text);
        let mut start = this;
        while let Some(previous) = document.previous_sibling(start).filter(|&node| is_text(node)) {
            start = previous;
        }
        let mut text = String::new();
        let mut current = Some(start);
        while let Some(node) = current.filter(|&node| is_text(node)) {
            text.push_str(document.text(node).unwrap_or_default());
            current = document.next_sibling(node);
        }
        Ok(text)
    }
}

impl CommentApi for Api {
    fn constructor(_context: &mut Context, prototype: JsObject, data: String) -> Result<JsObject> {
        let node = crate::host::with(|host| host.document.create_comment(&data))?;
        Ok(crate::convert::wrap_with_prototype(node, prototype)?)
    }
}

impl ProcessingInstructionApi for Api {
    fn target(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(host
            .document
            .processing_instruction_target(this)
            .unwrap_or_default()
            .to_owned())
    }
}

impl DocumentTypeApi for Api {
    fn name(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(host.document.doctype_name(this).unwrap_or_default().to_owned())
    }

    fn public_id(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(host.document.doctype_ids(this).0.to_owned())
    }

    fn system_id(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(host.document.doctype_ids(this).1.to_owned())
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
