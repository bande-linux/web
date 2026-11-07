use web_atoms::{Atom, Namespace, QualName};
use web_dom::{NodeId, NodeKind};

use crate::bindings::*;
use crate::convert::{Payload, Platform};
use crate::dom::collections::{self, Filter, Slot};
use crate::dom::tree::{self, Result, descendants, descendant_text, is_element};
use crate::dom::{self, Api, markup, ranges, traversal};
use crate::engine::{Context, JsObject};
use crate::exception::Exception;
use crate::host::{DocumentInfo, Host};

pub fn element_by_id(host: &Host, root: NodeId, id: &str) -> Option<NodeId> {
    if id.is_empty() {
        return None;
    }
    let atom = host.document.atoms().get(id)?;
    let document = &host.document;
    descendants(document, root)
        .into_iter()
        .find(|&node| document.element_id(node) == Some(atom))
}

fn root_element(host: &Host, this: NodeId) -> Option<NodeId> {
    dom::first_element_child(host, this)
}

fn html_child(host: &Host, this: NodeId, name: Atom) -> Option<NodeId> {
    let root = root_element(host, this).filter(|&root| host.document.html_element_name(root) == Some(Atom::HTML))?;
    host.document
        .children(root)
        .find(|&child| host.document.html_element_name(child) == Some(name))
}

pub fn body_of(host: &Host, this: NodeId) -> Option<NodeId> {
    let root = root_element(host, this).filter(|&root| host.document.html_element_name(root) == Some(Atom::HTML))?;
    host.document.children(root).find(|&child| {
        matches!(
            host.document.html_element_name(child),
            Some(Atom::BODY | Atom::FRAMESET)
        )
    })
}

fn title_element(host: &Host, this: NodeId) -> Option<NodeId> {
    let document = &host.document;
    descendants(document, this)
        .into_iter()
        .find(|&node| document.html_element_name(node) == Some(Atom::TITLE))
}

pub fn new_document(host: &mut Host, interface: Interface, html: bool, content_type: &str) -> NodeId {
    let node = host.document.create_document_node();
    host.documents.insert(
        node,
        DocumentInfo {
            interface,
            html,
            content_type: content_type.to_owned(),
        },
    );
    node
}

impl DocumentApi for Api {
    fn constructor(_context: &mut Context, prototype: JsObject) -> Result<JsObject> {
        let node = crate::host::with(|host| new_document(host, Interface::Document, false, "application/xml"))?;
        Ok(crate::convert::wrap_with_prototype(node, prototype)?)
    }

    fn implementation(context: &mut Context, this: NodeId) -> Result<JsObject> {
        let _ = context;
        Ok(crate::host::with(|host| {
            if let Some(known) = host.implementations.get(&this) {
                return Ok(known.clone());
            }
            let Some(prototype) = host.prototype(Interface::DOMImplementation) else {
                return Err(Exception::type_error("no realm"));
            };
            let object = JsObject::from_proto_and_data(
                prototype,
                Platform::new(Interface::DOMImplementation, Payload::Implementation(this)),
            );
            host.implementations.insert(this, object.clone());
            Ok(object)
        })??)
    }

    fn url(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(crate::window::document_url(host, this))
    }

    fn document_uri(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(crate::window::document_url(host, this))
    }

    fn compat_mode(host: &mut Host, this: NodeId) -> Result<String> {
        let quirks = this == NodeId::DOCUMENT && host.document.quirks_mode() == web_dom::QuirksMode::Quirks;
        Ok(if quirks { "BackCompat" } else { "CSS1Compat" }.to_owned())
    }

    fn character_set(_host: &mut Host, _this: NodeId) -> Result<String> {
        Ok("UTF-8".to_owned())
    }

    fn charset(_host: &mut Host, _this: NodeId) -> Result<String> {
        Ok("UTF-8".to_owned())
    }

    fn input_encoding(_host: &mut Host, _this: NodeId) -> Result<String> {
        Ok("UTF-8".to_owned())
    }

    fn content_type(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(host
            .documents
            .get(&this)
            .map_or("text/html".to_owned(), |info| info.content_type.clone()))
    }

    fn doctype(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(host
            .document
            .children(this)
            .find(|&child| host.document.kind(child) == Some(NodeKind::Doctype)))
    }

    fn document_element(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(root_element(host, this))
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

    fn create_element(host: &mut Host, this: NodeId, local_name: String, _options: UnionDOMStringOrElementCreationOptions) -> Result<NodeId> {
        if !tree::valid_element_name(&local_name) {
            return Err(Exception::dom(
                "InvalidCharacterError",
                format!("\"{local_name}\" is not a valid element name"),
            ));
        }
        let info = host.documents.get(&this);
        let html = info.is_some_and(|info| info.html);
        let html_content = html || info.is_some_and(|info| info.content_type == "application/xhtml+xml");
        let local = if html {
            local_name.to_ascii_lowercase()
        } else {
            local_name
        };
        let namespace = html_content.then_some("http://www.w3.org/1999/xhtml");
        Ok(tree::create_element(host, this, namespace, None, &local))
    }

    fn create_element_ns(host: &mut Host, this: NodeId, namespace: Option<String>, qualified_name: String, _options: UnionDOMStringOrElementCreationOptions) -> Result<NodeId> {
        let extracted = tree::validate_and_extract(namespace.as_deref(), &qualified_name, true)?;
        Ok(tree::create_element(
            host,
            this,
            extracted.namespace.as_deref(),
            extracted.prefix.as_deref(),
            &extracted.local,
        ))
    }

    fn create_document_fragment(host: &mut Host, this: NodeId) -> Result<NodeId> {
        let node = host.document.create_fragment();
        host.set_owner(node, this);
        Ok(node)
    }

    fn create_text_node(host: &mut Host, this: NodeId, data: String) -> Result<NodeId> {
        let node = host.document.create_text(&data);
        host.set_owner(node, this);
        Ok(node)
    }

    fn create_cdata_section(host: &mut Host, this: NodeId, data: String) -> Result<NodeId> {
        if host.is_html(this) {
            return Err(Exception::dom("NotSupportedError", "HTML documents have no CDATA sections"));
        }
        if data.contains("]]>") {
            return Err(Exception::dom("InvalidCharacterError", "the data contains ]]>"));
        }
        let node = host.document.create_text(&data);
        host.set_owner(node, this);
        host.cdata.insert(node);
        Ok(node)
    }

    fn create_comment(host: &mut Host, this: NodeId, data: String) -> Result<NodeId> {
        let node = host.document.create_comment(&data);
        host.set_owner(node, this);
        Ok(node)
    }

    fn create_processing_instruction(host: &mut Host, this: NodeId, target: String, data: String) -> Result<NodeId> {
        if !tree::valid_element_name(&target) || target.contains(':') && target.starts_with(':') {
            return Err(Exception::dom("InvalidCharacterError", "the target is not valid"));
        }
        if data.contains("?>") {
            return Err(Exception::dom("InvalidCharacterError", "the data contains ?>"));
        }
        let node = host.document.create_processing_instruction(&target, &data);
        host.set_owner(node, this);
        Ok(node)
    }

    fn import_node(host: &mut Host, this: NodeId, node: NodeId, subtree: bool) -> Result<NodeId> {
        if matches!(host.document.kind(node), Some(NodeKind::Document)) {
            return Err(Exception::dom("NotSupportedError", "a document cannot be imported"));
        }
        Ok(tree::clone_node(host, node, this, subtree))
    }

    fn adopt_node(host: &mut Host, this: NodeId, node: NodeId) -> Result<NodeId> {
        if host.document.kind(node) == Some(NodeKind::Document) {
            return Err(Exception::dom("NotSupportedError", "a document cannot be adopted"));
        }
        tree::adopt(host, node, this);
        Ok(node)
    }

    fn create_attribute(host: &mut Host, this: NodeId, local_name: String) -> Result<NodeId> {
        if !tree::valid_attribute_name(&local_name) {
            return Err(Exception::dom("InvalidCharacterError", "the name is not valid"));
        }
        let name = if host.is_html(this) {
            local_name.to_ascii_lowercase()
        } else {
            local_name
        };
        let atom = host.document.intern(&name);
        let node = host.document.create_attribute_node(
            QualName {
                ns: Namespace::None,
                local: atom,
            },
            "",
        );
        host.set_owner(node, this);
        Ok(node)
    }

    fn create_attribute_ns(host: &mut Host, this: NodeId, namespace: Option<String>, qualified_name: String) -> Result<NodeId> {
        let extracted = tree::validate_and_extract(namespace.as_deref(), &qualified_name, false)?;
        let atom = host.document.intern(&extracted.local);
        let ns = tree::namespace_from(extracted.namespace.as_deref());
        let node = host.document.create_attribute_node(QualName { ns, local: atom }, "");
        host.set_owner(node, this);
        if ns == Namespace::Other
            && let Some(uri) = extracted.namespace
        {
            host.detached_attribute_namespace.insert(node, uri);
        }
        if let Some(prefix) = extracted.prefix {
            let prefix = host.document.intern(&prefix);
            host.detached_attribute_prefix.insert(node, prefix);
        }
        Ok(node)
    }

    fn create_event(host: &mut Host, _this: NodeId, interface: String) -> Result<JsObject> {
        crate::events::create_event(host, &interface)
    }

    fn create_range(host: &mut Host, this: NodeId) -> Result<JsObject> {
        ranges::create(host, this)
    }

    fn create_node_iterator(host: &mut Host, _this: NodeId, root: NodeId, what_to_show: u32, filter: Option<JsObject>) -> Result<JsObject> {
        traversal::create_iterator(host, root, what_to_show, filter)
    }

    fn create_tree_walker(host: &mut Host, _this: NodeId, root: NodeId, what_to_show: u32, filter: Option<JsObject>) -> Result<JsObject> {
        traversal::create_walker(host, root, what_to_show, filter)
    }

    fn get_element_by_id(host: &mut Host, this: NodeId, element_id: String) -> Result<Option<NodeId>> {
        Ok(element_by_id(host, this, &element_id))
    }

    fn title(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(title_element(host, this)
            .map(|title| {
                descendant_text(&host.document, title)
                    .split_ascii_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default())
    }

    fn set_title(host: &mut Host, this: NodeId, value: String) -> Result<()> {
        let title = match title_element(host, this) {
            Some(title) => title,
            None => {
                let Some(head) = html_child(host, this, Atom::HEAD) else {
                    return Ok(());
                };
                let title = tree::create_element(host, this, Some("http://www.w3.org/1999/xhtml"), None, "title");
                tree::insert(host, title, head, None, false);
                title
            }
        };
        <Api as NodeApi>::set_text_content(host, title, Some(value))
    }

    fn body(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(body_of(host, this))
    }

    fn set_body(host: &mut Host, this: NodeId, value: Option<NodeId>) -> Result<()> {
        let Some(value) = value.filter(|&value| {
            matches!(host.document.html_element_name(value), Some(Atom::BODY | Atom::FRAMESET))
        }) else {
            return Err(Exception::dom("HierarchyRequestError", "the body must be body or frameset"));
        };
        let current = body_of(host, this);
        if current == Some(value) {
            return Ok(());
        }
        if let Some(current) = current {
            let Some(parent) = host.document.parent(current) else {
                return Ok(());
            };
            tree::replace(host, current, value, parent)?;
            return Ok(());
        }
        let Some(root) = root_element(host, this) else {
            return Err(Exception::dom("HierarchyRequestError", "the document has no root"));
        };
        tree::pre_insert(host, value, root, None)?;
        Ok(())
    }

    fn head(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(html_child(host, this, Atom::HEAD))
    }

    fn images(context: &mut Context, this: NodeId) -> Result<JsObject> {
        collections::filtered(context, this, Filter::HtmlTag(Atom::IMG))
    }

    fn links(context: &mut Context, this: NodeId) -> Result<JsObject> {
        collections::filtered(context, this, Filter::Links)
    }

    fn forms(context: &mut Context, this: NodeId) -> Result<JsObject> {
        collections::filtered(context, this, Filter::HtmlTag(Atom::FORM))
    }

    fn scripts(context: &mut Context, this: NodeId) -> Result<JsObject> {
        collections::filtered(context, this, Filter::HtmlTag(Atom::SCRIPT))
    }

    fn anchors(context: &mut Context, this: NodeId) -> Result<JsObject> {
        collections::filtered(context, this, Filter::Anchors)
    }

    fn embeds(context: &mut Context, this: NodeId) -> Result<JsObject> {
        collections::filtered(context, this, Filter::HtmlTag(Atom::EMBED))
    }

    fn plugins(context: &mut Context, this: NodeId) -> Result<JsObject> {
        collections::filtered(context, this, Filter::HtmlTag(Atom::EMBED))
    }

    fn get_elements_by_name(context: &mut Context, this: NodeId, element_name: String) -> Result<JsObject> {
        let list = crate::host::with(|host| {
            let document = &host.document;
            descendants(document, this)
                .into_iter()
                .filter(|&node| document.attribute(node, Atom::NAME) == Some(element_name.as_str()) && is_element(document, node))
                .collect::<Vec<_>>()
        })?;
        collections::static_list(context, this, list)
    }

    fn current_script(host: &mut Host, _this: NodeId) -> Result<Option<NodeId>> {
        Ok(host.current_script)
    }

    fn ready_state(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(if this == NodeId::DOCUMENT { host.ready_state } else { "complete" }.to_owned())
    }

    fn dir(host: &mut Host, this: NodeId) -> Result<String> {
        Ok(root_element(host, this)
            .and_then(|root| host.document.attribute(root, Atom::DIR))
            .unwrap_or_default()
            .to_owned())
    }

    fn set_dir(host: &mut Host, this: NodeId, value: String) -> Result<()> {
        if let Some(root) = root_element(host, this) {
            tree::change_attribute(host, root, QualName::attribute(Atom::DIR), &value);
        }
        Ok(())
    }

    fn referrer(_host: &mut Host, _this: NodeId) -> Result<String> {
        Ok(String::new())
    }

    fn cookie(_host: &mut Host, _this: NodeId) -> Result<String> {
        Ok(String::new())
    }

    fn set_cookie(_host: &mut Host, _this: NodeId, _value: String) -> Result<()> {
        Ok(())
    }

    fn last_modified(_host: &mut Host, _this: NodeId) -> Result<String> {
        Ok(String::new())
    }

    fn default_view(host: &mut Host, this: NodeId) -> Result<Option<crate::convert::Global>> {
        let _ = host;
        Ok((this == NodeId::DOCUMENT).then_some(crate::convert::Global))
    }

    fn hidden(_host: &mut Host, _this: NodeId) -> Result<bool> {
        Ok(false)
    }

    fn visibility_state(_host: &mut Host, _this: NodeId) -> Result<String> {
        Ok("visible".to_owned())
    }

    fn active_element(host: &mut Host, this: NodeId) -> Result<Option<NodeId>> {
        Ok(body_of(host, this))
    }

    fn has_focus(_host: &mut Host, _this: NodeId) -> Result<bool> {
        Ok(true)
    }

    fn write(context: &mut Context, this: NodeId, text: Vec<String>) -> Result<()> {
        markup::document_write(context, this, &text.concat())
    }

    fn writeln(context: &mut Context, this: NodeId, text: Vec<String>) -> Result<()> {
        markup::document_write(context, this, &(text.concat() + "\n"))
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

impl DOMImplementationApi for Api {
    fn create_document_type(host: &mut Host, this: JsObject, name: String, public_id: String, system_id: String) -> Result<NodeId> {
        if !tree::valid_doctype_name(&name) {
            return Err(Exception::dom("InvalidCharacterError", "the name is not valid"));
        }
        let node = host.document.create_doctype(&name, &public_id, &system_id);
        let owner = implementation_document(&this);
        host.set_owner(node, owner);
        Ok(node)
    }

    fn create_document(host: &mut Host, _this: JsObject, namespace: Option<String>, qualified_name: String, doctype: Option<NodeId>) -> Result<NodeId> {
        let content_type = match namespace.as_deref() {
            Some("http://www.w3.org/1999/xhtml") => "application/xhtml+xml",
            Some("http://www.w3.org/2000/svg") => "image/svg+xml",
            _ => "application/xml",
        };
        let extracted = (!qualified_name.is_empty())
            .then(|| tree::validate_and_extract(namespace.as_deref(), &qualified_name, true))
            .transpose()?;
        let document = new_document(host, Interface::XMLDocument, false, content_type);
        if let Some(doctype) = doctype {
            tree::adopt(host, doctype, document);
            host.document.insert(document, doctype, None);
        }
        if let Some(extracted) = extracted {
            let element = tree::create_element(
                host,
                document,
                extracted.namespace.as_deref(),
                extracted.prefix.as_deref(),
                &extracted.local,
            );
            host.document.insert(document, element, None);
        }
        Ok(document)
    }

    fn create_html_document(host: &mut Host, _this: JsObject, title: Option<String>) -> Result<NodeId> {
        let document = new_document(host, Interface::HTMLDocument, true, "text/html");
        let doctype = host.document.create_doctype("html", "", "");
        host.set_owner(doctype, document);
        host.document.insert(document, doctype, None);
        const XHTML: Option<&str> = Some("http://www.w3.org/1999/xhtml");
        let root = tree::create_element(host, document, XHTML, None, "html");
        host.document.insert(document, root, None);
        let head = tree::create_element(host, document, XHTML, None, "head");
        host.document.insert(root, head, None);
        if let Some(title) = title {
            let element = tree::create_element(host, document, XHTML, None, "title");
            host.document.insert(head, element, None);
            let text = host.document.create_text(&title);
            host.set_owner(text, document);
            host.document.insert(element, text, None);
        }
        let body = tree::create_element(host, document, XHTML, None, "body");
        host.document.insert(root, body, None);
        Ok(document)
    }

    fn has_feature(_host: &mut Host, _this: JsObject) -> Result<bool> {
        Ok(true)
    }
}

fn implementation_document(object: &JsObject) -> NodeId {
    object
        .downcast_ref::<Platform>()
        .and_then(|platform| match platform.payload {
            Payload::Implementation(node) => Some(node),
            _ => None,
        })
        .unwrap_or(NodeId::DOCUMENT)
}
