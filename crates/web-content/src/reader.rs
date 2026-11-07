use web_atoms::Atom;
use web_dom::{Document, NodeId, NodeKind};
use web_style::{Display, Styles};

const MAX_DEPTH: usize = 256;
const MIN_PARAGRAPH: usize = 25;
const READABLE_PARAGRAPH: usize = 140;
const READABLE_SCORE: f32 = 20.0;
const MIN_ARTICLE: usize = 500;
const LINK_HEAVY: f32 = 0.5;
const CLASS_WEIGHT: f32 = 25.0;
const SIBLING_SHARE: f32 = 0.2;
const SIBLING_MINIMUM: f32 = 10.0;

const SKIPPED: &[&str] = &[
    "script", "style", "noscript", "nav", "aside", "footer", "form", "button", "iframe", "svg",
    "canvas", "select", "input", "textarea", "template", "dialog", "menu", "object", "embed",
    "video", "audio", "head", "link", "meta", "title",
];
const KEPT_BLOCKS: &[&str] = &[
    "p",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "ul",
    "ol",
    "li",
    "blockquote",
    "pre",
    "hr",
    "figure",
    "figcaption",
    "dl",
    "dt",
    "dd",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "td",
    "th",
    "caption",
];
const KEPT_INLINE: &[&str] = &[
    "em", "i", "strong", "b", "code", "sub", "sup", "small", "abbr", "kbd", "mark", "q", "cite",
    "var", "samp", "time", "u", "s", "del", "ins",
];
const NEGATIVE: &[&str] = &[
    "comment",
    "sidebar",
    "footer",
    "masthead",
    "promo",
    "related",
    "share",
    "social",
    "advert",
    "cookie",
    "banner",
    "newsletter",
    "breadcrumb",
    "navbox",
    "navigation",
    "menu",
    "popup",
    "modal",
    "sponsor",
    "widget",
    "infobox",
    "toc",
    "subscribe",
    "reference-edit",
    "mw-editsection",
];
const POSITIVE: &[&str] = &[
    "article", "body", "content", "entry", "main", "page", "post", "story", "text", "blog",
];

struct Scan<'a> {
    document: &'a Document,
    styles: Option<&'a Styles>,
    scores: Vec<f32>,
    candidates: Vec<NodeId>,
    readable: f32,
}

fn escape(text: &str, out: &mut String) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            character => out.push(character),
        }
    }
}

fn name<'a>(document: &'a Document, node: NodeId) -> Option<&'a str> {
    let name = document.html_element_name(node)?;
    Some(document.atoms().name(name))
}

fn is_element(document: &Document, node: NodeId) -> bool {
    document.kind(node) == Some(NodeKind::Element)
}

fn identity(document: &Document, node: NodeId) -> String {
    let mut text = String::new();
    if let Some(id) = document.element_id(node) {
        text.push_str(document.atoms().name(id));
    }
    for class in document.classes(node) {
        text.push(' ');
        text.push_str(document.atoms().name(*class));
    }
    text.make_ascii_lowercase();
    text
}

fn class_weight(document: &Document, node: NodeId) -> f32 {
    let identity = identity(document, node);
    let mut weight = 0.0;
    if NEGATIVE.iter().any(|word| identity.contains(word)) {
        weight -= CLASS_WEIGHT;
    }
    if POSITIVE.iter().any(|word| identity.contains(word)) {
        weight += CLASS_WEIGHT;
    }
    weight
}

fn hidden(document: &Document, styles: Option<&Styles>, node: NodeId) -> bool {
    document.attribute(node, Atom::HIDDEN).is_some()
        || styles
            .and_then(|styles| styles.get(node))
            .is_some_and(|style| style.display() == Display::None)
}

fn skipped(document: &Document, styles: Option<&Styles>, node: NodeId) -> bool {
    name(document, node).is_none_or(|name| SKIPPED.contains(&name))
        || hidden(document, styles, node)
}

fn text_lengths(
    document: &Document,
    styles: Option<&Styles>,
    node: NodeId,
    in_link: bool,
    depth: usize,
    lengths: &mut (usize, usize),
) {
    if depth > MAX_DEPTH {
        return;
    }
    for child in document.children(node) {
        if let Some(text) = document
            .text(child)
            .filter(|_| document.kind(child) == Some(NodeKind::Text))
        {
            let count = text
                .split_whitespace()
                .map(|word| word.len() + 1)
                .sum::<usize>();
            lengths.0 += count;
            if in_link {
                lengths.1 += count;
            }
        } else if is_element(document, child) && !skipped(document, styles, child) {
            let link = in_link || name(document, child) == Some("a");
            text_lengths(document, styles, child, link, depth + 1, lengths);
        }
    }
}

fn link_density(lengths: (usize, usize)) -> f32 {
    lengths.1 as f32 / lengths.0.max(1) as f32
}

impl Scan<'_> {
    fn visit(&mut self, node: NodeId, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        let children: Vec<NodeId> = self.document.children(node).collect();
        for child in children {
            if !is_element(self.document, child) || skipped(self.document, self.styles, child) {
                continue;
            }
            if matches!(name(self.document, child), Some("p" | "pre" | "blockquote")) {
                self.paragraph(child);
            }
            self.visit(child, depth + 1);
        }
    }

    fn paragraph(&mut self, node: NodeId) {
        let mut lengths = (0, 0);
        text_lengths(self.document, self.styles, node, false, 0, &mut lengths);
        let plain = lengths.0.saturating_sub(lengths.1);
        if plain < MIN_PARAGRAPH {
            return;
        }
        if plain >= READABLE_PARAGRAPH {
            self.readable += ((plain - READABLE_PARAGRAPH) as f32).sqrt();
        }
        let score = 1.0 + (plain as f32 / 100.0).min(3.0);
        let ancestors: Vec<NodeId> = self
            .document
            .ancestors(node)
            .skip(1)
            .filter(|ancestor| is_element(self.document, *ancestor))
            .take(3)
            .collect();
        for (level, ancestor) in ancestors.into_iter().enumerate() {
            let Some(slot) = self.scores.get_mut(ancestor.index()) else {
                continue;
            };
            if *slot == 0.0 {
                *slot = class_weight(self.document, ancestor);
                self.candidates.push(ancestor);
            }
            *slot += score / (level + 1) as f32;
        }
    }

    fn best(&self) -> Option<NodeId> {
        self.candidates
            .iter()
            .map(|candidate| {
                let mut lengths = (0, 0);
                text_lengths(
                    self.document,
                    self.styles,
                    *candidate,
                    false,
                    0,
                    &mut lengths,
                );
                let score = self.score(*candidate) * (1.0 - link_density(lengths));
                (*candidate, score)
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(candidate, _)| candidate)
    }

    fn score(&self, node: NodeId) -> f32 {
        self.scores.get(node.index()).copied().unwrap_or(0.0)
    }
}

fn scan<'a>(document: &'a Document, styles: Option<&'a Styles>) -> Scan<'a> {
    let mut scan = Scan {
        document,
        styles,
        scores: vec![0.0; document.len()],
        candidates: Vec::new(),
        readable: 0.0,
    };
    scan.visit(NodeId::DOCUMENT, 0);
    scan
}

pub fn is_readable(document: &Document, styles: &Styles) -> bool {
    let scan = scan(document, Some(styles));
    if scan.readable <= READABLE_SCORE {
        return false;
    }
    scan.best().is_some_and(|best| {
        let mut lengths = (0, 0);
        text_lengths(document, Some(styles), best, false, 0, &mut lengths);
        lengths.0 >= MIN_ARTICLE
    })
}

struct Writer<'a> {
    document: &'a Document,
    styles: Option<&'a Styles>,
    out: String,
    skip_heading: Option<String>,
}

fn collapsed_text(document: &Document, node: NodeId, depth: usize, out: &mut String) {
    if depth > MAX_DEPTH {
        return;
    }
    for child in document.children(node) {
        match document.kind(child) {
            Some(NodeKind::Text) => out.push_str(document.text(child).unwrap_or_default()),
            Some(NodeKind::Element) => collapsed_text(document, child, depth + 1, out),
            _ => {}
        }
    }
}

fn plain_text(document: &Document, node: NodeId) -> String {
    let mut text = String::new();
    collapsed_text(document, node, 0, &mut text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Writer<'_> {
    fn attribute(&mut self, name: &str, value: &str) {
        self.out.push(' ');
        self.out.push_str(name);
        self.out.push_str("=\"");
        escape(value, &mut self.out);
        self.out.push('"');
    }

    fn image(&mut self, node: NodeId) {
        let source = self
            .document
            .attribute(node, Atom::SRC)
            .filter(|source| !source.is_empty() && !source.starts_with("data:"))
            .or_else(|| {
                self.document
                    .attributes(node)
                    .find(|(name, _)| self.document.atoms().name(name.local) == "data-src")
                    .map(|(_, value)| value)
            });
        let tiny = [Atom::WIDTH, Atom::HEIGHT].into_iter().any(|side| {
            self.document
                .attribute(node, side)
                .and_then(|value| value.parse::<f32>().ok())
                .is_some_and(|value| value <= 1.0)
        });
        let Some(source) = source.filter(|_| !tiny) else {
            return;
        };
        self.out.push_str("<img");
        self.attribute("src", source);
        if let Some(alt) = self.document.attribute(node, Atom::ALT) {
            self.attribute("alt", alt);
        }
        self.out.push('>');
    }

    fn noise(&self, node: NodeId, tag: &str) -> bool {
        let mut lengths = (0, 0);
        text_lengths(self.document, self.styles, node, false, 0, &mut lengths);
        let listing = matches!(tag, "ul" | "ol" | "table" | "div" | "section" | "dl");
        (listing && link_density(lengths) > LINK_HEAVY)
            || (class_weight(self.document, node) < 0.0 && lengths.0 < MIN_ARTICLE)
    }

    fn children(&mut self, node: NodeId, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        let children: Vec<NodeId> = self.document.children(node).collect();
        for child in children {
            match self.document.kind(child) {
                Some(NodeKind::Text) => {
                    escape(self.document.text(child).unwrap_or_default(), &mut self.out)
                }
                Some(NodeKind::Element) => self.element(child, depth + 1),
                _ => {}
            }
        }
    }

    fn element(&mut self, node: NodeId, depth: usize) {
        if skipped(self.document, self.styles, node) {
            return;
        }
        let Some(tag) = name(self.document, node) else {
            return;
        };
        match tag {
            "img" => return self.image(node),
            "br" => return self.out.push_str("<br>"),
            "hr" => return self.out.push_str("<hr>"),
            _ => {}
        }
        if self.noise(node, tag) {
            return;
        }
        if tag == "h1"
            && let Some(heading) = &self.skip_heading
            && plain_text(self.document, node) == *heading
        {
            self.skip_heading = None;
            return;
        }
        if tag == "a" {
            let href = self.document.attribute(node, Atom::HREF);
            match href.filter(|href| {
                !href
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("javascript:")
            }) {
                Some(href) => {
                    self.out.push_str("<a");
                    self.attribute("href", href);
                    self.out.push('>');
                    self.children(node, depth);
                    self.out.push_str("</a>");
                }
                None => self.children(node, depth),
            }
            return;
        }
        let kept = KEPT_BLOCKS.contains(&tag) || KEPT_INLINE.contains(&tag);
        let wrapped = !kept
            && matches!(tag, "div" | "section" | "article" | "main")
            && self.only_inline(node);
        let shown = if kept {
            tag
        } else if wrapped {
            "p"
        } else {
            ""
        };
        if !shown.is_empty() {
            self.out.push('<');
            self.out.push_str(shown);
            self.spans(node, tag);
            self.out.push('>');
        }
        self.children(node, depth);
        if !shown.is_empty() {
            self.out.push_str("</");
            self.out.push_str(shown);
            self.out.push('>');
        }
    }

    fn spans(&mut self, node: NodeId, tag: &str) {
        if matches!(tag, "td" | "th") {
            for (qualified, value) in self.document.attributes(node) {
                let attribute = self.document.atoms().name(qualified.local).to_owned();
                if matches!(attribute.as_str(), "colspan" | "rowspan") {
                    self.attribute(&attribute, value);
                }
            }
        }
    }

    fn only_inline(&self, node: NodeId) -> bool {
        let mut has_text = false;
        for child in self.document.children(node) {
            match self.document.kind(child) {
                Some(NodeKind::Text) => {
                    has_text |= self
                        .document
                        .text(child)
                        .is_some_and(|text| !text.trim().is_empty());
                }
                Some(NodeKind::Element) => {
                    let inline = name(self.document, child).is_some_and(|name| {
                        KEPT_INLINE.contains(&name) || matches!(name, "a" | "span" | "br" | "img")
                    });
                    if !inline {
                        return false;
                    }
                }
                _ => {}
            }
        }
        has_text
    }
}

pub fn article(
    document: &Document,
    styles: &Styles,
    title: Option<&str>,
    site: &str,
    style: &str,
) -> Option<String> {
    let scan = scan(document, Some(styles));
    let best = scan.best()?;
    let threshold = (scan.score(best) * SIBLING_SHARE).max(SIBLING_MINIMUM);
    let sections: Vec<NodeId> = match document
        .parent(best)
        .filter(|parent| is_element(document, *parent))
    {
        Some(parent) => document
            .children(parent)
            .filter(|sibling| {
                *sibling == best
                    || (is_element(document, *sibling) && scan.score(*sibling) >= threshold)
                    || sibling_paragraph(document, styles, *sibling)
            })
            .collect(),
        None => vec![best],
    };
    let heading = first_heading(document, styles)
        .or_else(|| title.map(str::to_owned))
        .unwrap_or_default();
    let mut writer = Writer {
        document,
        styles: Some(styles),
        out: String::new(),
        skip_heading: Some(heading.clone()),
    };
    writer
        .out
        .push_str("<!doctype html><html><head><meta charset=utf-8><title>");
    escape(title.unwrap_or(&heading), &mut writer.out);
    writer.out.push_str("</title><style>");
    writer.out.push_str(&style.replace("</", ""));
    writer.out.push_str("</style></head><body><article><h1>");
    escape(&heading, &mut writer.out);
    writer.out.push_str("</h1><p class=site>");
    escape(site, &mut writer.out);
    writer.out.push_str("</p>");
    for section in sections {
        writer.element(section, 0);
    }
    writer.out.push_str("</article></body></html>");
    Some(writer.out)
}

fn sibling_paragraph(document: &Document, styles: &Styles, node: NodeId) -> bool {
    if name(document, node) != Some("p") || hidden(document, Some(styles), node) {
        return false;
    }
    let mut lengths = (0, 0);
    text_lengths(document, Some(styles), node, false, 0, &mut lengths);
    lengths.0 >= MIN_PARAGRAPH * 3 && link_density(lengths) < 0.25
}

fn first_heading(document: &Document, styles: &Styles) -> Option<String> {
    let mut stack = vec![NodeId::DOCUMENT];
    while let Some(node) = stack.pop() {
        if name(document, node) == Some("h1") && !hidden(document, Some(styles), node) {
            let text = plain_text(document, node);
            if !text.is_empty() {
                return Some(text);
            }
        }
        if node == NodeId::DOCUMENT
            || (is_element(document, node) && !skipped(document, Some(styles), node))
        {
            let start = stack.len();
            stack.extend(document.children(node));
            stack[start..].reverse();
        }
    }
    None
}
