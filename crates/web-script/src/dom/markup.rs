use web_atoms::{Atom, Namespace, QualName};
use web_dom::{NodeId, NodeKind};

use crate::dom::tree::{self, Result};
use crate::engine::Context;
use crate::exception::Exception;
use crate::host::{self, Host};

const VOID: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "img", "input",
    "keygen", "link", "meta", "param", "source", "track", "wbr",
];

const RAW: &[&str] = &[
    "style", "script", "xmp", "iframe", "noembed", "noframes", "plaintext",
];

fn escape(text: &str, attribute: bool, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            '"' if attribute => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
}

fn open_tag(host: &Host, node: NodeId, out: &mut String) {
    out.push('<');
    out.push_str(&tree::element_qualified_name(host, node));
    for (name, value) in host.document.attributes(node) {
        out.push(' ');
        out.push_str(&tree::attribute_qualified_name(host, node, name));
        out.push_str("=\"");
        escape(value, true, out);
        out.push('"');
    }
    out.push('>');
}

fn serialize_one(host: &Host, node: NodeId, out: &mut String) {
    let document = &host.document;
    match document.kind(node) {
        Some(NodeKind::Text) => {
            let parent_raw = document
                .parent(node)
                .and_then(|parent| document.html_element_name(parent))
                .is_some_and(|name| RAW.contains(&document.atoms().name(name)));
            let text = document.text(node).unwrap_or_default();
            if parent_raw {
                out.push_str(text);
            } else {
                escape(text, false, out);
            }
        }
        Some(NodeKind::Comment) => {
            out.push_str("<!--");
            out.push_str(document.text(node).unwrap_or_default());
            out.push_str("-->");
        }
        Some(NodeKind::ProcessingInstruction) => {
            out.push_str("<?");
            out.push_str(document.processing_instruction_target(node).unwrap_or_default());
            out.push(' ');
            out.push_str(document.text(node).unwrap_or_default());
            out.push('>');
        }
        Some(NodeKind::Doctype) => {
            out.push_str("<!DOCTYPE ");
            out.push_str(document.doctype_name(node).unwrap_or_default());
            out.push('>');
        }
        _ => {}
    }
}

fn is_void(host: &Host, node: NodeId) -> bool {
    host.document
        .html_element_name(node)
        .is_some_and(|name| VOID.contains(&host.document.atoms().name(name)))
}

enum Step {
    Open(NodeId),
    Close(NodeId),
}

fn walk(host: &Host, first: Vec<NodeId>, out: &mut String) {
    let document = &host.document;
    let mut stack: Vec<Step> = first.into_iter().rev().map(Step::Open).collect();
    while let Some(step) = stack.pop() {
        match step {
            Step::Close(node) => {
                out.push_str("</");
                out.push_str(&tree::element_qualified_name(host, node));
                out.push('>');
            }
            Step::Open(node) if document.kind(node) == Some(NodeKind::Element) => {
                open_tag(host, node, out);
                if is_void(host, node) {
                    continue;
                }
                stack.push(Step::Close(node));
                let source = document.template_contents(node).unwrap_or(node);
                let children: Vec<NodeId> = document.children(source).collect();
                stack.extend(children.into_iter().rev().map(Step::Open));
            }
            Step::Open(node) => match document.kind(node) {
                Some(NodeKind::Document | NodeKind::Fragment) => {
                    let children: Vec<NodeId> = document.children(node).collect();
                    stack.extend(children.into_iter().rev().map(Step::Open));
                }
                _ => serialize_one(host, node, out),
            },
        }
    }
}

pub fn serialize_children(host: &Host, node: NodeId) -> String {
    let mut out = String::new();
    if is_void(host, node) {
        return out;
    }
    let source = host.document.template_contents(node).unwrap_or(node);
    walk(host, host.document.children(source).collect(), &mut out);
    out
}

pub fn serialize_node(host: &Host, node: NodeId) -> String {
    let mut out = String::new();
    walk(host, vec![node], &mut out);
    out
}

fn context_name(host: &Host, element: NodeId) -> QualName {
    match host.document.element_name(element) {
        Some((ns, local)) => QualName { ns, local },
        None => QualName::html(Atom::BODY),
    }
}

pub fn parse_fragment(host: &mut Host, context: QualName, markup: &str, owner: NodeId) -> NodeId {
    let fragment = host.document.parse_fragment(context, markup, true);
    host.set_owner(fragment, owner);
    for node in tree::descendants(&host.document, fragment) {
        host.set_owner(node, owner);
        if host.document.html_element_name(node) == Some(Atom::SCRIPT) {
            host.started_scripts.insert(node);
        }
    }
    fragment
}

pub fn set_inner(host: &mut Host, element: NodeId, markup: &str) {
    let context = context_name(host, element);
    let owner = host.node_document(element);
    let fragment = parse_fragment(host, context, markup, owner);
    let target = host.document.template_contents(element).unwrap_or(element);
    tree::replace_all(host, Some(fragment), target);
}

pub fn set_outer(host: &mut Host, element: NodeId, markup: &str) -> Result<()> {
    let Some(parent) = host.document.parent(element) else {
        return Ok(());
    };
    if host.document.kind(parent) == Some(NodeKind::Document) {
        return Err(Exception::dom(
            "NoModificationAllowedError",
            "the document element cannot be replaced this way",
        ));
    }
    let context = if host.document.kind(parent) == Some(NodeKind::Fragment) {
        QualName::html(Atom::BODY)
    } else {
        context_name(host, parent)
    };
    let owner = host.node_document(element);
    let fragment = parse_fragment(host, context, markup, owner);
    tree::replace(host, element, fragment, parent)?;
    Ok(())
}

pub fn insert_adjacent(host: &mut Host, element: NodeId, position: &str, markup: &str) -> Result<()> {
    let position = position.to_ascii_lowercase();
    let context_element = match position.as_str() {
        "beforebegin" | "afterend" => {
            let parent = host.document.parent(element);
            match parent {
                Some(parent) if host.document.kind(parent) != Some(NodeKind::Document) => parent,
                _ => {
                    return Err(Exception::dom(
                        "NoModificationAllowedError",
                        "the element has no parent element",
                    ));
                }
            }
        }
        "afterbegin" | "beforeend" => element,
        _ => return Err(Exception::dom("SyntaxError", "the position is not valid")),
    };
    let html_context = !host.document.kind(context_element).is_some_and(|kind| kind == NodeKind::Element)
        || (host.document.html_element_name(context_element) == Some(Atom::HTML)
            && host.is_html(host.owner(context_element)));
    let context = if html_context {
        QualName::html(Atom::BODY)
    } else {
        context_name(host, context_element)
    };
    let owner = host.node_document(element);
    let fragment = parse_fragment(host, context, markup, owner);
    match position.as_str() {
        "beforebegin" => {
            tree::pre_insert(host, fragment, context_element, Some(element))?;
        }
        "afterbegin" => {
            let first = host.document.first_child(element);
            tree::pre_insert(host, fragment, element, first)?;
        }
        "beforeend" => {
            tree::pre_insert(host, fragment, element, None)?;
        }
        _ => {
            let next = host.document.next_sibling(element);
            tree::pre_insert(host, fragment, context_element, next)?;
        }
    }
    Ok(())
}

pub fn document_write(context: &mut Context, this: NodeId, markup: &str) -> Result<()> {
    let _ = context;
    host::with(|host| {
        if !host.is_html(this) {
            return Err(Exception::dom("InvalidStateError", "XML documents cannot be written"));
        }
        let anchor = host.current_script.filter(|_| this == NodeId::DOCUMENT);
        let (parent, next) = match anchor.and_then(|anchor| host.document.parent(anchor).map(|parent| (parent, anchor))) {
            Some((parent, anchor)) => (parent, host.document.next_sibling(anchor)),
            None => match crate::dom::document::body_of(host, this) {
                Some(body) => (body, None),
                None => return Ok(()),
            },
        };
        let context_name = if host.document.kind(parent) == Some(NodeKind::Element) {
            context_name(host, parent)
        } else {
            QualName::html(Atom::BODY)
        };
        let owner = host.node_document(parent);
        let fragment = host.document.parse_fragment(context_name, markup, true);
        host.set_owner(fragment, owner);
        for node in tree::descendants(&host.document, fragment) {
            host.set_owner(node, owner);
        }
        tree::insert(host, fragment, parent, next, false);
        Ok(())
    })?
}

pub fn is_html_namespace(host: &Host, node: NodeId) -> bool {
    matches!(host.document.element_name(node), Some((Namespace::Html, _)))
}
