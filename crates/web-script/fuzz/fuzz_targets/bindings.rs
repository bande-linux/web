#![no_main]

use libfuzzer_sys::fuzz_target;
use web_dom::Document;
use web_html::{Options, Parser};

const PAGE: &str = "<!doctype html><title>t</title><div id=a class='x y'>text<!--c--><p>p</p></div>";

const CALLS: &[&str] = &[
    "document.body",
    "document.getElementById('a')",
    "document.createElement('span')",
    "document.createTextNode('t')",
    "document.createComment('c')",
    "document.createDocumentFragment()",
    "document.documentElement",
    "document",
    "null",
    "undefined",
    "'x'",
    "1",
];

const OPERATIONS: &[&str] = &[
    "a.appendChild(b)",
    "a.insertBefore(b, c)",
    "a.replaceChild(b, c)",
    "a.removeChild(b)",
    "a.textContent = String(b)",
    "a.setAttribute(String(b), String(c))",
    "a.removeAttribute(String(b))",
    "a.toggleAttribute(String(b))",
    "a.remove()",
    "a.contains(b)",
    "a.data = String(c)",
    "a.appendData(String(b))",
    "a.className = String(b)",
    "a.childNodes.length",
    "a.getElementsByTagName(String(b))",
    "a.nodeValue = b",
    "a.wholeText",
    "a.hidden = b",
];

fuzz_target!(|data: &[u8]| {
    let mut parser = Parser::new(Document::new(), Options::default());
    parser.feed_str(PAGE);
    let mut document = parser.finish();
    let Ok(mut script) = web_script::Script::new() else {
        return;
    };
    let mut source = String::new();
    for chunk in data.chunks(4) {
        let pick = |index: usize, list: &[&'static str]| {
            list[usize::from(chunk.get(index).copied().unwrap_or(0)) % list.len()]
        };
        source.push_str(&format!(
            "try {{ const a = {}, b = {}, c = {}; {}; }} catch (e) {{}}\n",
            pick(0, CALLS),
            pick(1, CALLS),
            pick(2, CALLS),
            pick(3, OPERATIONS),
        ));
    }
    let _ = script.run(&mut document, &source);
    let _ = script.run(&mut document, std::str::from_utf8(data).unwrap_or(""));
});
