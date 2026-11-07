use web_dom::{Document, NodeId};
use web_html::{Options, Parser};
use web_script::Script;

fn document(html: &str) -> Document {
    let mut parser = Parser::new(Document::new(), Options::default());
    parser.feed_str(html);
    parser.finish()
}

fn run(script: &mut Script, document: &mut Document, source: &str) {
    let outcome = script.run(document, source);
    assert_eq!(outcome.error, None);
}

#[test]
fn scripts_read_and_change_the_document() {
    let mut document =
        document("<!doctype html><title> A  page </title><p id=first class='a b'>Hi</p>");
    let created = Script::new();
    assert!(created.is_ok(), "{:?}", created.as_ref().err());
    let Ok(mut script) = created else {
        return;
    };
    run(
        &mut script,
        &mut document,
        r#"
        const p = document.getElementById("first");
        console.log(document.title, p.tagName, p.className, p.textContent, p instanceof HTMLElement);
        console.log(p === document.body.firstChild, document.getElementsByClassName("b a").length);
        const item = document.createElement("LI");
        item.setAttribute("data-x", "1");
        item.appendChild(document.createTextNode("new"));
        p.textContent = "changed";
        p.after = 1;
        document.body.insertBefore(item, p);
        try { document.body.appendChild(document.documentElement); } catch (error) { console.log(error.message); }
        console.log(document.body.childElementCount, item.outerHTML, item.getAttribute("DATA-X"), Node.ELEMENT_NODE);
        "#,
    );
    let console = script.take_console();
    assert_eq!(
        console,
        [
            "A page P a b Hi true",
            "true 1",
            "HierarchyRequestError: the node is an ancestor of the parent",
            "2 undefined 1 1",
        ]
    );
    let body = document
        .children(NodeId::DOCUMENT)
        .last()
        .and_then(|html| document.children(html).last());
    let texts: Vec<String> = body
        .into_iter()
        .flat_map(|body| document.children(body).collect::<Vec<_>>())
        .map(|child| {
            document
                .children(child)
                .filter_map(|text| document.text(text))
                .collect()
        })
        .collect();
    assert_eq!(texts, ["new", "changed"]);
    run(
        &mut script,
        &mut document,
        "console.log(document.body.lastChild.after)",
    );
    assert_eq!(script.take_console(), ["1"]);
}
