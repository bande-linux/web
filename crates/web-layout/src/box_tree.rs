use std::sync::Arc;

use crate::collapse::{Collapsed, collapse};
use web_atoms::Atom;
use web_dom::{Document, NodeId, NodeKind};
use web_style::{
    ComputedStyle, ContentPart, Display, LineHeight, ListStyleType, Pseudo, ResetStyle, Styles, TableColumn, TextTransform, VerticalAlign,
};
use web_text::{
    FontKey, Fonts, LineAlign, Paragraph, RunStyle, ShapedParagraph, content_metrics, first_letter,
    inline_extent,
};

pub(crate) struct BlockBox {
    pub(crate) node: Option<NodeId>,
    pub(crate) pseudo: Option<Pseudo>,
    pub(crate) style: ComputedStyle,
    pub(crate) contents: Contents,
    pub(crate) marker: Option<Box<InlineContent>>,
}

pub(crate) enum Contents {
    Blocks(Vec<BlockBox>),
    Inline(Box<InlineContent>),
    Replaced(Option<Arc<str>>),
    Flex(Vec<BlockBox>),
    Grid(Vec<BlockBox>),
    Table(Box<Table>),
}

pub(crate) struct Table {
    pub(crate) captions: Vec<BlockBox>,
    pub(crate) rows: Vec<TableRow>,
    pub(crate) parts: Parts,
    pub(crate) collapsed: Option<Box<Collapsed>>,
}

pub(crate) struct TableRow {
    pub(crate) node: Option<NodeId>,
    pub(crate) style: ComputedStyle,
    pub(crate) cells: Vec<TableCell>,
    pub(crate) group: Option<u32>,
}

#[derive(Default)]
pub(crate) struct Parts {
    pub(crate) groups: Vec<ComputedStyle>,
    pub(crate) columns: Vec<(ComputedStyle, Option<usize>)>,
    pub(crate) column_groups: Vec<ComputedStyle>,
}

pub(crate) struct TableCell {
    pub(crate) block: BlockBox,
    pub(crate) columns: u16,
    pub(crate) rows: u16,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ItemKind {
    Text,
    Start,
    End(u32),
    Atomic(u16),
    Resume,
    Anchor(u16),
}

pub(crate) struct InlineItem {
    pub(crate) node: NodeId,
    pub(crate) pseudo: Option<Pseudo>,
    pub(crate) style: ComputedStyle,
    pub(crate) parent: Option<u32>,
    pub(crate) kind: ItemKind,
}

pub(crate) struct InlineFrame {
    pub(crate) margin: (f32, f32),
    pub(crate) border: (f32, f32),
    pub(crate) padding: (f32, f32),
}

impl InlineFrame {
    pub(crate) fn of(style: &ComputedStyle) -> Self {
        let reset = &style.reset;
        let px = |side: Option<web_style::LengthPercentage>| side.map_or(0.0, |side| side.px);
        let (margin, border, padding) = (
            (px(reset.margin.left), px(reset.margin.right)),
            (reset.border.left, reset.border.right),
            (reset.padding.left.px, reset.padding.right.px),
        );
        if style.is_rtl() {
            InlineFrame {
                margin: (margin.1, margin.0),
                border: (border.1, border.0),
                padding: (padding.1, padding.0),
            }
        } else {
            InlineFrame {
                margin,
                border,
                padding,
            }
        }
    }

    pub(crate) fn start(&self) -> f32 {
        self.margin.0 + self.border.0 + self.padding.0
    }

    pub(crate) fn end(&self) -> f32 {
        self.margin.1 + self.border.1 + self.padding.1
    }
}

fn decorated(style: &ComputedStyle) -> bool {
    let reset = &style.reset;
    let frame = InlineFrame::of(style);
    reset.background_color.a != 0
        || frame.start() != 0.0
        || frame.end() != 0.0
        || reset.border.top != 0.0
        || reset.border.bottom != 0.0
        || style.is_positioned()
}

pub(crate) struct InlineContent {
    pub(crate) shaped: ShapedParagraph,
    pub(crate) items: Vec<InlineItem>,
    pub(crate) atomics: Vec<BlockBox>,
    pub(crate) out_of_flow: Vec<BlockBox>,
}

#[derive(Default)]
pub struct BoxTree {
    pub(crate) root: Option<BlockBox>,
}

const MAX_DEPTH: u32 = 512;

struct Builder<'a> {
    document: &'a Document,
    styles: &'a Styles,
    fonts: &'a mut Fonts,
    depth: u32,
    counters: Vec<Counter>,
    quote_depth: i32,
    first_letter: Option<(NodeId, ComputedStyle)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Origin {
    node: NodeId,
    pseudo: Option<Pseudo>,
}

impl Origin {
    fn element(node: NodeId) -> Self {
        Origin { node, pseudo: None }
    }
}

#[derive(Clone, Copy)]
enum Item {
    Node(NodeId),
    Generated(NodeId, Pseudo),
}

struct Pending {
    paragraph: Paragraph,
    items: Vec<InlineItem>,
    open: Vec<u32>,
    atomics: Vec<BlockBox>,
    out_of_flow: Vec<BlockBox>,
}

impl Pending {
    fn new(rtl: bool) -> Self {
        Self {
            paragraph: Paragraph::new(rtl),
            items: Vec::new(),
            open: Vec::new(),
            atomics: Vec::new(),
            out_of_flow: Vec::new(),
        }
    }

    fn push(&mut self, origin: Origin, style: &ComputedStyle, kind: ItemKind) -> u32 {
        let tag = self.items.len() as u32;
        self.items.push(InlineItem {
            node: origin.node,
            pseudo: origin.pseudo,
            style: style.clone(),
            parent: self.open.last().copied(),
            kind,
        });
        tag
    }
}

impl BoxTree {
    pub fn build(document: &Document, styles: &Styles, fonts: &mut Fonts) -> Self {
        let mut builder = Builder {
            document,
            styles,
            fonts,
            depth: 0,
            counters: Vec::new(),
            quote_depth: 0,
            first_letter: None,
        };
        let root = document
            .children(NodeId::DOCUMENT)
            .find(|&child| document.kind(child) == Some(NodeKind::Element))
            .and_then(|root| {
                let style = styles.get(root)?.clone();
                Some(builder.block(Origin::element(root), style, None))
            });
        BoxTree { root }
    }

    pub fn heap_bytes(&self) -> usize {
        self.root.as_ref().map_or(0, block_bytes)
    }

    pub fn refresh_styles(&mut self, styles: &Styles) {
        if let Some(root) = &mut self.root {
            refresh(root, styles, None);
        }
    }
}

fn keep_collapsed(style: &mut ComputedStyle, old: &ResetStyle) {
    if style.reset.border != old.border
        || style.reset.border_color != old.border_color
        || style.reset.padding != old.padding
    {
        let reset = Arc::make_mut(&mut style.reset);
        reset.border = old.border;
        reset.border_color = old.border_color;
        reset.padding = old.padding;
    }
}

fn refresh(block: &mut BlockBox, styles: &Styles, parent: Option<&ComputedStyle>) {
    let old = block.style.reset.clone();
    let own = block.node.and_then(|node| match block.pseudo {
        Some(pseudo) => styles.pseudo(node, pseudo),
        None => styles.get(node),
    });
    match (own, parent) {
        (Some(style), _) => block.style = style.clone(),
        (None, Some(parent)) => block.style = styles.anonymous_box(parent, old.display),
        (None, None) => {}
    }
    if let Contents::Table(table) = &block.contents
        && table.collapsed.is_some()
    {
        keep_collapsed(&mut block.style, &old);
    }
    let style = block.style.clone();
    match &mut block.contents {
        Contents::Blocks(children) => {
            for child in children {
                refresh(child, styles, Some(&style));
            }
        }
        Contents::Flex(children) | Contents::Grid(children) => {
            for child in children {
                refresh(child, styles, Some(&style));
            }
        }
        Contents::Inline(inline) => {
            for item in &mut inline.items {
                let own = match item.pseudo {
                    Some(pseudo) => styles.pseudo(item.node, pseudo),
                    None => styles.get(item.node),
                };
                if let Some(style) = own {
                    item.style = style.clone();
                }
            }
            for child in inline.atomics.iter_mut().chain(&mut inline.out_of_flow) {
                refresh(child, styles, Some(&style));
            }
        }
        Contents::Replaced(_) => {}
        Contents::Table(table) => {
            for row in &mut table.rows {
                if let Some(row_style) = row.node.and_then(|node| styles.get(node)) {
                    row.style = row_style.clone();
                }
                for cell in &mut row.cells {
                    let old = cell.block.style.reset.clone();
                    refresh(&mut cell.block, styles, Some(&style));
                    if table.collapsed.is_some() {
                        keep_collapsed(&mut cell.block.style, &old);
                    }
                }
            }
            for caption in &mut table.captions {
                refresh(caption, styles, Some(&style));
            }
        }
    }
}

fn block_bytes(block: &BlockBox) -> usize {
    let marker = block.marker.as_deref().map_or(0, |marker| {
        size_of::<InlineContent>() + inline_bytes(marker)
    });
    marker
        + match &block.contents {
            Contents::Blocks(children) | Contents::Flex(children) | Contents::Grid(children) => {
                children.capacity() * size_of::<BlockBox>()
                    + children.iter().map(block_bytes).sum::<usize>()
            }
            Contents::Inline(inline) => size_of::<InlineContent>() + inline_bytes(inline),
            Contents::Replaced(_) => 0,
            Contents::Table(table) => {
                size_of::<Table>()
                    + table.rows.capacity() * size_of::<TableRow>()
                    + table
                        .rows
                        .iter()
                        .flat_map(|row| &row.cells)
                        .map(|cell| size_of::<TableCell>() + block_bytes(&cell.block))
                        .sum::<usize>()
                    + table
                        .captions
                        .iter()
                        .map(|caption| size_of::<BlockBox>() + block_bytes(caption))
                        .sum::<usize>()
            }
        }
}

fn inline_bytes(inline: &InlineContent) -> usize {
    inline.shaped.heap_bytes()
        + inline.items.capacity() * size_of::<InlineItem>()
        + inline
            .atomics
            .iter()
            .chain(&inline.out_of_flow)
            .map(|block| size_of::<BlockBox>() + block_bytes(block))
            .sum::<usize>()
}

pub(crate) fn is_rtl(style: &ComputedStyle) -> bool {
    style.inherited.direction == web_style::Direction::Rtl
}

fn own_shift(style: &ComputedStyle) -> f32 {
    let size = style.inherited.font_size;
    match style.reset.vertical_align {
        VerticalAlign::Sub => -0.25 * size,
        VerticalAlign::Super => 0.5 * size,
        VerticalAlign::Length(length) => length.resolve(style.inherited.font_size * 1.2),
        _ => 0.0,
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Shift {
    px: f32,
    align: LineAlign,
}

fn shifted(fonts: &mut Fonts, style: &ComputedStyle, tag: u32, shift: Shift) -> RunStyle {
    RunStyle {
        align: shift.align,
        ..run_style(fonts, style, tag, shift.px)
    }
}

fn resolve_shift(
    fonts: &mut Fonts,
    parent: &ComputedStyle,
    style: &ComputedStyle,
    outer: Shift,
    atomic: bool,
    group: usize,
) -> Shift {
    let vertical = style.reset.vertical_align;
    if outer.align.is_edge() {
        return Shift {
            px: outer.px + own_shift(style),
            ..outer
        };
    }
    let parent_metrics = |fonts: &mut Fonts| {
        let run = run_style(fonts, parent, 0, 0.0);
        content_metrics(fonts, run)
    };
    let own = |fonts: &mut Fonts| {
        let run = run_style(fonts, style, 0, 0.0);
        inline_extent(fonts, run)
    };
    match vertical {
        VerticalAlign::Top => Shift {
            px: 0.0,
            align: LineAlign::top(group),
        },
        VerticalAlign::Bottom => Shift {
            px: 0.0,
            align: LineAlign::bottom(group),
        },
        VerticalAlign::Middle | VerticalAlign::TextTop | VerticalAlign::TextBottom => {
            let (ascent, descent, x_height) = parent_metrics(fonts);
            let (align, target) = match vertical {
                VerticalAlign::Middle => (LineAlign::MIDDLE, outer.px + x_height / 2.0),
                VerticalAlign::TextTop => (LineAlign::TEXT_TOP, outer.px + ascent),
                _ => (LineAlign::TEXT_BOTTOM, outer.px - descent),
            };
            if atomic {
                return Shift { px: target, align };
            }
            let (above, below) = own(fonts);
            let px = match align {
                LineAlign::MIDDLE => target - (above - below) / 2.0,
                LineAlign::TEXT_TOP => target - above,
                _ => target + below,
            };
            Shift {
                px,
                align: LineAlign::BASELINE,
            }
        }
        _ => Shift {
            px: outer.px + own_shift(style),
            align: LineAlign::BASELINE,
        },
    }
}

pub(crate) fn run_style(
    fonts: &mut Fonts,
    style: &ComputedStyle,
    tag: u32,
    baseline_shift: f32,
) -> RunStyle {
    let inherited = &style.inherited;
    let names = fonts.family_names(&inherited.font_names);
    let size = inherited.font_size;
    RunStyle {
        font: FontKey {
            family: inherited.font_family,
            names,
            weight: inherited.font_weight,
            italic: inherited.italic,
        },
        size,
        line_height: match inherited.line_height {
            LineHeight::Normal => None,
            LineHeight::Number(factor) => Some(factor * size),
            LineHeight::Px(px) => Some(px),
        },
        baseline_shift,
        align: LineAlign::BASELINE,
        white_space: inherited.white_space,
        tag,
        letter_spacing: inherited.letter_spacing,
        word_spacing: inherited.word_spacing,
        tab_columns: match inherited.tab_size {
            web_style::TabSize::Spaces(spaces) => spaces.round() as u32,
            web_style::TabSize::Px(px) => (px / (size * 0.5).max(1.0)).round() as u32,
        },
        word_break: inherited.word_break,
        overflow_wrap: inherited.overflow_wrap,
    }
}

impl Builder<'_> {
    fn block(&mut self, origin: Origin, style: ComputedStyle, ordinal: Option<i32>) -> BlockBox {
        let node = origin.node;
        if origin.pseudo.is_none() && self.document.html_element_name(node) == Some(Atom::IMG) {
            let source = self.document.attribute(node, Atom::SRC).map(Arc::from);
            return BlockBox {
                node: Some(node),
                pseudo: None,
                style,
                contents: Contents::Replaced(source),
                marker: None,
            };
        }
        if matches!(style.display(), Display::Table | Display::InlineTable) {
            let outer_letter = self.first_letter.take();
            let items = self.items_of(origin);
            let block = self.table_box(Some(origin), style, items);
            self.first_letter = outer_letter;
            return block;
        }
        let items = origin.pseudo.map(|_| Vec::new());
        self.container(Some(origin), style, items, ordinal)
    }

    fn container(
        &mut self,
        origin: Option<Origin>,
        style: ComputedStyle,
        items: Option<Vec<Item>>,
        ordinal: Option<i32>,
    ) -> BlockBox {
        let mut blocks = Vec::new();
        let mut pending = Pending::new(is_rtl(&style));
        pending
            .paragraph
            .set_nowrap(!style.inherited.white_space.wraps());
        let holder = origin.unwrap_or(Origin {
            node: NodeId::DOCUMENT,
            pseudo: None,
        });
        let inside = style.inherited.list_inside;
        if inside
            && let Some(text) =
                ordinal.and_then(|ordinal| marker_text(style.inherited.list_style_type, ordinal))
        {
            let tag = pending.push(holder, &style, ItemKind::Text);
            let mut run = run_style(self.fonts, &style, tag, 0.0);
            run.white_space = web_style::WhiteSpace::Pre;
            pending.paragraph.push_text(&format!("{text} "), run);
        }
        let flex = matches!(style.display(), Display::Flex | Display::InlineFlex);
        let grid = matches!(style.display(), Display::Grid | Display::InlineGrid);
        let outer_letter = self.first_letter.take();
        let own_letter = origin
            .filter(|origin| origin.pseudo.is_none() && !flex && !grid)
            .and_then(|origin| {
                let style = self.styles.pseudo(origin.node, Pseudo::FirstLetter)?;
                Some((origin.node, style.clone()))
            });
        if !flex && !grid {
            self.first_letter = own_letter.or(outer_letter.clone());
        }
        match (origin, items) {
            (Some(origin), None) => {
                self.children(origin.node, &mut blocks, &mut pending, Shift::default())
            }
            (Some(origin), Some(_)) if origin.pseudo.is_some() => {
                self.generated_parts(origin, &style, &mut pending, Shift::default())
            }
            (origin, Some(items)) => self.run(
                origin.map_or(NodeId::DOCUMENT, |origin| origin.node),
                &style,
                items.into_iter(),
                &mut blocks,
                &mut pending,
                Shift::default(),
            ),
            (None, None) => {}
        }
        self.first_letter = if !flex && !grid && self.first_letter.is_some() {
            outer_letter
        } else {
            None
        };
        let contents = if flex {
            self.flush(&mut blocks, pending, &style);
            Contents::Flex(blocks)
        } else if grid {
            self.flush(&mut blocks, pending, &style);
            Contents::Grid(blocks)
        } else if blocks.is_empty() {
            Contents::Inline(Box::new(self.finish(pending)))
        } else {
            self.flush(&mut blocks, pending, &style);
            Contents::Blocks(blocks)
        };
        let contents = match contents {
            Contents::Blocks(mut blocks) => {
                blocks.shrink_to_fit();
                Contents::Blocks(blocks)
            }
            Contents::Flex(mut blocks) => {
                blocks.shrink_to_fit();
                Contents::Flex(blocks)
            }
            Contents::Grid(mut blocks) => {
                blocks.shrink_to_fit();
                Contents::Grid(blocks)
            }
            contents => contents,
        };
        let marker = ordinal
            .filter(|_| !inside)
            .and_then(|ordinal| self.marker(&style, ordinal));
        BlockBox {
            node: origin.map(|origin| origin.node),
            pseudo: origin.and_then(|origin| origin.pseudo),
            style,
            contents,
            marker,
        }
    }

    fn items_of(&self, origin: Origin) -> Vec<Item> {
        if origin.pseudo.is_some() {
            return Vec::new();
        }
        let node = origin.node;
        let mut items = Vec::new();
        if self.styles.pseudo(node, Pseudo::Before).is_some() {
            items.push(Item::Generated(node, Pseudo::Before));
        }
        items.extend(self.document.children(node).map(Item::Node));
        if self.styles.pseudo(node, Pseudo::After).is_some() {
            items.push(Item::Generated(node, Pseudo::After));
        }
        items
    }

    fn item_style(&self, item: Item) -> Option<&ComputedStyle> {
        match item {
            Item::Node(node) => self.styles.get(node),
            Item::Generated(node, pseudo) => self.styles.pseudo(node, pseudo),
        }
    }

    fn is_blank_text(&self, item: Item) -> bool {
        match item {
            Item::Node(node) => {
                self.document.kind(node) == Some(NodeKind::Text)
                    && self
                        .document
                        .text(node)
                        .unwrap_or_default()
                        .chars()
                        .all(|ch| matches!(ch, ' ' | '\t' | '\n' | '\r' | '\x0c'))
            }
            Item::Generated(..) => false,
        }
    }

    fn children(
        &mut self,
        node: NodeId,
        blocks: &mut Vec<BlockBox>,
        pending: &mut Pending,
        shift: Shift,
    ) {
        let Some(parent) = self.styles.get(node).cloned() else {
            return;
        };
        let document = self.document;
        let before = self
            .styles
            .pseudo(node, Pseudo::Before)
            .map(|_| Item::Generated(node, Pseudo::Before));
        let after = self
            .styles
            .pseudo(node, Pseudo::After)
            .map(|_| Item::Generated(node, Pseudo::After));
        let items = before
            .into_iter()
            .chain(document.children(node).map(Item::Node))
            .chain(after);
        self.run(node, &parent, items, blocks, pending, shift);
    }

    fn run(
        &mut self,
        node: NodeId,
        parent: &ComputedStyle,
        items: impl Iterator<Item = Item>,
        blocks: &mut Vec<BlockBox>,
        pending: &mut Pending,
        shift: Shift,
    ) {
        let scope = self.counters.len();
        let mut items = items.peekable();
        while let Some(item) = items.next() {
            let Some(style) = self.item_style(item).cloned() else {
                continue;
            };
            if style.display().is_table_part() {
                let mut run = vec![item];
                let mut trailing = Vec::new();
                while let Some(&next) = items.peek() {
                    let proper = self
                        .item_style(next)
                        .is_some_and(|style| style.display().is_table_part());
                    if proper {
                        run.append(&mut trailing);
                        run.push(next);
                    } else if self.is_blank_text(next)
                        || self
                            .item_style(next)
                            .is_some_and(|style| style.display() == Display::None)
                    {
                        trailing.push(next);
                    } else {
                        break;
                    }
                    items.next();
                }
                self.anonymous_table(node, parent, run, blocks, pending);
                for skipped in trailing {
                    if self.is_blank_text(skipped) {
                        self.item(parent, skipped, blocks, pending, shift);
                    }
                }
                continue;
            }
            self.item(parent, item, blocks, pending, shift);
        }
        self.counters.truncate(scope);
    }

    fn anonymous_table(
        &mut self,
        node: NodeId,
        parent: &ComputedStyle,
        items: Vec<Item>,
        blocks: &mut Vec<BlockBox>,
        pending: &mut Pending,
    ) {
        let inline = parent.display() == Display::Inline;
        let display = if inline {
            Display::InlineTable
        } else {
            Display::Table
        };
        let style = self.styles.anonymous_box(parent, display);
        let block = self.table_box(None, style.clone(), items);
        if inline {
            let index = u16::try_from(pending.atomics.len()).unwrap_or(u16::MAX);
            if index < u16::MAX {
                pending.atomics.push(block);
                let origin = Origin::element(node);
                let tag = pending.push(origin, &style, ItemKind::Atomic(index));
                pending
                    .paragraph
                    .push_atomic(index, run_style(self.fonts, &style, tag, 0.0));
                self.first_letter = None;
            }
        } else {
            let taken = std::mem::replace(pending, Pending::new(pending.paragraph.is_rtl()));
            self.flush(blocks, taken, parent);
            blocks.push(block);
            self.first_letter = None;
        }
    }

    fn item(
        &mut self,
        parent: &ComputedStyle,
        item: Item,
        blocks: &mut Vec<BlockBox>,
        pending: &mut Pending,
        shift: Shift,
    ) {
        let Some(style) = self.item_style(item).cloned() else {
            return;
        };
        let origin = match item {
            Item::Node(child) => Origin::element(child),
            Item::Generated(owner, pseudo) => Origin {
                node: owner,
                pseudo: Some(pseudo),
            },
        };
        let kind = match item {
            Item::Node(child) => self.document.kind(child),
            Item::Generated(..) => Some(NodeKind::Element),
        };
        match kind {
            Some(NodeKind::Text) => {
                let text = self.document.text(origin.node).unwrap_or_default();
                self.push_text(origin, text, &style, pending, shift);
            }
            Some(NodeKind::Element) => {
                let ordinal = (style.display() != Display::None)
                    .then(|| self.apply_counters(origin, &style))
                    .flatten();
                if self.depth < MAX_DEPTH && style.display() != Display::None {
                    self.element(parent, origin, style, ordinal, blocks, pending, shift);
                }
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn element(
        &mut self,
        parent: &ComputedStyle,
        origin: Origin,
        style: ComputedStyle,
        ordinal: Option<i32>,
        blocks: &mut Vec<BlockBox>,
        pending: &mut Pending,
        shift: Shift,
    ) {
        let image = origin.pseudo.is_none()
            && self.document.html_element_name(origin.node) == Some(Atom::IMG);
        if style.is_out_of_flow() || style.reset.float != web_style::Float::None {
            self.depth += 1;
            let outer_letter = self.first_letter.take();
            let block = self.block(origin, style.clone(), None);
            self.first_letter = outer_letter;
            self.depth -= 1;
            let index = u16::try_from(pending.out_of_flow.len()).unwrap_or(u16::MAX);
            if index < u16::MAX {
                pending.out_of_flow.push(block);
                let tag = pending.push(origin, &style, ItemKind::Anchor(index));
                let run = run_style(self.fonts, parent, tag, 0.0);
                pending.paragraph.push_anchor(index, run);
            }
            return;
        }
        if (style.display().is_inline_level() && style.display() != Display::Inline)
            || (style.display() == Display::Inline && image)
        {
            self.depth += 1;
            self.first_letter = None;
            let block = self.block(origin, style.clone(), None);
            self.first_letter = None;
            self.depth -= 1;
            let index = u16::try_from(pending.atomics.len()).unwrap_or(u16::MAX);
            if index < u16::MAX {
                pending.atomics.push(block);
                let tag = pending.push(origin, &style, ItemKind::Atomic(index));
                let own = resolve_shift(
                    self.fonts,
                    parent,
                    &style,
                    shift,
                    true,
                    origin.node.index(),
                );
                pending
                    .paragraph
                    .push_atomic(index, shifted(self.fonts, &style, tag, own));
            }
            return;
        }
        match style.display() {
            Display::None => {}
            Display::Inline | Display::Contents => {
                let inner = resolve_shift(
                    self.fonts,
                    parent,
                    &style,
                    shift,
                    false,
                    origin.node.index(),
                );
                if origin.pseudo.is_none()
                    && self.document.html_element_name(origin.node) == Some(Atom::BR)
                {
                    let tag = pending.push(origin, &style, ItemKind::Text);
                    pending
                        .paragraph
                        .push_break(shifted(self.fonts, &style, tag, shift));
                } else if style.display() == Display::Inline && decorated(&style) {
                    let frame = InlineFrame::of(&style);
                    let start = pending.push(origin, &style, ItemKind::Start);
                    pending.paragraph.push_spacer(
                        frame.start(),
                        shifted(self.fonts, &style, start, inner),
                    );
                    pending.open.push(start);
                    self.depth += 1;
                    self.inline_contents(origin, &style, blocks, pending, inner);
                    self.depth -= 1;
                    if let Some(open) = pending.open.pop() {
                        let end = pending.push(origin, &style, ItemKind::End(open));
                        pending
                            .paragraph
                            .push_spacer(frame.end(), shifted(self.fonts, &style, end, inner));
                    }
                } else {
                    self.depth += 1;
                    self.inline_contents(origin, &style, blocks, pending, inner);
                    self.depth -= 1;
                }
            }
            display => {
                let rtl = pending.paragraph.is_rtl();
                let open: Vec<(Origin, ComputedStyle)> = pending
                    .open
                    .iter()
                    .filter_map(|&tag| {
                        let item = pending.items.get(tag as usize)?;
                        Some((
                            Origin {
                                node: item.node,
                                pseudo: item.pseudo,
                            },
                            item.style.clone(),
                        ))
                    })
                    .collect();
                let taken = std::mem::replace(pending, Pending::new(rtl));
                self.flush(blocks, taken, parent);
                for (origin, style) in open {
                    let tag = pending.push(origin, &style, ItemKind::Resume);
                    pending.open.push(tag);
                }
                let item_ordinal = ordinal.filter(|_| display == Display::ListItem);
                self.depth += 1;
                blocks.push(self.block(origin, style, item_ordinal));
                self.depth -= 1;
            }
        }
    }

    fn inline_contents(
        &mut self,
        origin: Origin,
        style: &ComputedStyle,
        blocks: &mut Vec<BlockBox>,
        pending: &mut Pending,
        shift: Shift,
    ) {
        if origin.pseudo.is_some() {
            self.generated_parts(origin, style, pending, shift);
        } else {
            self.children(origin.node, blocks, pending, shift);
        }
    }

    fn push_text(
        &mut self,
        origin: Origin,
        text: &str,
        style: &ComputedStyle,
        pending: &mut Pending,
        shift: Shift,
    ) {
        let text = transformed(text, style, pending.paragraph.last_char());
        let collapsible = style.inherited.white_space.collapses_spaces();
        let blank = text
            .chars()
            .all(|ch| matches!(ch, ' ' | '\t' | '\n' | '\r' | '\x0c'));
        if self.first_letter.is_some() && !(blank && collapsible) {
            let holder = self.first_letter.take();
            if let (Some((holder, letter_style)), Some(range)) = (holder, first_letter(&text)) {
                self.push_run(origin, text.get(..range.start), style, pending, shift);
                let letter = text.get(range.clone()).unwrap_or_default();
                let letter_origin = Origin {
                    node: holder,
                    pseudo: Some(Pseudo::FirstLetter),
                };
                self.push_first_letter(letter_origin, letter, letter_style, style, pending, shift);
                self.push_run(origin, text.get(range.end..), style, pending, shift);
                return;
            }
        }
        self.push_run(origin, Some(&text), style, pending, shift);
    }

    fn push_run(
        &mut self,
        origin: Origin,
        text: Option<&str>,
        style: &ComputedStyle,
        pending: &mut Pending,
        shift: Shift,
    ) {
        let Some(text) = text.filter(|text| !text.is_empty()) else {
            return;
        };
        let tag = pending.push(origin, style, ItemKind::Text);
        pending
            .paragraph
            .push_text(text, shifted(self.fonts, style, tag, shift));
    }

    fn push_first_letter(
        &mut self,
        letter_origin: Origin,
        letter: &str,
        letter_style: ComputedStyle,
        parent: &ComputedStyle,
        pending: &mut Pending,
        shift: Shift,
    ) {
        if letter_style.reset.float != web_style::Float::None {
            let mut paragraph = Pending::new(is_rtl(&letter_style));
            let tag = paragraph.push(letter_origin, &letter_style, ItemKind::Text);
            paragraph
                .paragraph
                .push_text(letter, run_style(self.fonts, &letter_style, tag, 0.0));
            let contents = Contents::Inline(Box::new(self.finish(paragraph)));
            let index = u16::try_from(pending.out_of_flow.len()).unwrap_or(u16::MAX);
            if index < u16::MAX {
                pending.out_of_flow.push(BlockBox {
                    node: Some(letter_origin.node),
                    pseudo: letter_origin.pseudo,
                    style: letter_style.clone(),
                    contents,
                    marker: None,
                });
                let tag = pending.push(letter_origin, &letter_style, ItemKind::Anchor(index));
                let run = run_style(self.fonts, parent, tag, 0.0);
                pending.paragraph.push_anchor(index, run);
            }
            return;
        }
        let inner = resolve_shift(
            self.fonts,
            parent,
            &letter_style,
            shift,
            false,
            letter_origin.node.index(),
        );
        let frame = InlineFrame::of(&letter_style);
        let start = pending.push(letter_origin, &letter_style, ItemKind::Start);
        pending.paragraph.push_spacer(
            frame.start(),
            shifted(self.fonts, &letter_style, start, inner),
        );
        pending.open.push(start);
        self.push_run(letter_origin, Some(letter), &letter_style, pending, inner);
        pending.open.pop();
        let end = pending.push(letter_origin, &letter_style, ItemKind::End(start));
        pending
            .paragraph
            .push_spacer(frame.end(), shifted(self.fonts, &letter_style, end, inner));
    }

    fn counter(&self, name: &str) -> Option<usize> {
        self.counters
            .iter()
            .rposition(|counter| &*counter.name == name)
    }

    fn apply_counters(&mut self, origin: Origin, style: &ComputedStyle) -> Option<i32> {
        let element = origin.node;
        let changes = style.reset.counters.clone();
        let named = |list: Option<&[(Arc<str>, i32)]>, name: &str| {
            list.is_some_and(|list| list.iter().any(|(known, _)| &**known == name))
        };
        let html = if origin.pseudo.is_some() {
            None
        } else {
            self.document.html_element_name(element)
        };
        if matches!(html, Some(Atom::OL | Atom::UL | Atom::MENU | Atom::DIR))
            && !named(
                changes.as_ref().map(|changes| &changes.reset[..]),
                LIST_ITEM,
            )
        {
            let reversed = html == Some(Atom::OL)
                && self.document.attribute(element, Atom::REVERSED).is_some();
            let start = self
                .document
                .attribute(element, Atom::START)
                .and_then(|start| start.trim().parse::<i32>().ok());
            let (value, step) = if reversed {
                let items = self
                    .document
                    .children(element)
                    .filter(|&child| {
                        self.styles
                            .get(child)
                            .is_some_and(|style| style.display() == Display::ListItem)
                    })
                    .count() as i32;
                (start.unwrap_or(items).saturating_add(1), -1)
            } else {
                (start.unwrap_or(1).saturating_sub(1), 1)
            };
            self.counters.push(Counter {
                name: Arc::from(LIST_ITEM),
                value,
                step,
            });
        }
        if let Some(changes) = &changes {
            for (name, value) in &changes.reset {
                self.counters.push(Counter {
                    name: name.clone(),
                    value: *value,
                    step: 1,
                });
            }
            for (name, value) in &changes.set {
                match self
                    .counter(name)
                    .and_then(|index| self.counters.get_mut(index))
                {
                    Some(counter) => counter.value = *value,
                    None => self.counters.push(Counter {
                        name: name.clone(),
                        value: *value,
                        step: 1,
                    }),
                }
            }
            for (name, value) in &changes.increment {
                match self
                    .counter(name)
                    .and_then(|index| self.counters.get_mut(index))
                {
                    Some(counter) => counter.value = counter.value.saturating_add(*value),
                    None => self.counters.push(Counter {
                        name: name.clone(),
                        value: *value,
                        step: 1,
                    }),
                }
            }
        }
        if style.display() != Display::ListItem {
            return None;
        }
        let explicit = named(
            changes.as_ref().map(|changes| &changes.increment[..]),
            LIST_ITEM,
        );
        let item_value = self
            .document
            .attribute(element, Atom::VALUE)
            .and_then(|value| value.trim().parse::<i32>().ok());
        let index = match self.counter(LIST_ITEM) {
            Some(index) => index,
            None => {
                self.counters.push(Counter {
                    name: Arc::from(LIST_ITEM),
                    value: 0,
                    step: 1,
                });
                self.counters.len() - 1
            }
        };
        let counter = self.counters.get_mut(index)?;
        match item_value {
            Some(value) if html == Some(Atom::LI) => counter.value = value,
            _ if !explicit => counter.value = counter.value.saturating_add(counter.step),
            _ => {}
        }
        Some(counter.value)
    }

    fn generated_parts(
        &mut self,
        origin: Origin,
        style: &ComputedStyle,
        pending: &mut Pending,
        shift: Shift,
    ) {
        let Some(parts) = style.reset.content.clone() else {
            return;
        };
        let mut text = String::new();
        for part in parts.iter() {
            match part {
                ContentPart::Text(part) => text.push_str(part),
                ContentPart::Image(url) => {
                    self.push_run(origin, Some(&text), style, pending, shift);
                    text.clear();
                    let image = self.styles.anonymous_box(style, Display::InlineBlock);
                    let index = u16::try_from(pending.atomics.len()).unwrap_or(u16::MAX);
                    if index < u16::MAX {
                        pending.atomics.push(BlockBox {
                            node: None,
                            pseudo: None,
                            style: image.clone(),
                            contents: Contents::Replaced(Some(url.clone())),
                            marker: None,
                        });
                        let tag = pending.push(origin, &image, ItemKind::Atomic(index));
                        pending
                            .paragraph
                            .push_atomic(index, shifted(self.fonts, &image, tag, shift));
                    }
                }
                ContentPart::Quote { open, visible } => {
                    let quotes = &style.inherited.quotes;
                    let depth = if *open {
                        let depth = self.quote_depth;
                        self.quote_depth = depth.saturating_add(1);
                        depth
                    } else {
                        self.quote_depth = self.quote_depth.saturating_sub(1).max(0);
                        self.quote_depth
                    };
                    if *visible {
                        let last = quotes.len().saturating_sub(1);
                        if let Some(pair) = quotes.get((depth.max(0) as usize).min(last)) {
                            text.push_str(if *open { &pair.0 } else { &pair.1 });
                        }
                    }
                }
                ContentPart::Counter(name, kind) => {
                    let value = self
                        .counter(name)
                        .and_then(|index| self.counters.get(index))
                        .map_or(0, |counter| counter.value);
                    text.push_str(&counter_text(value, *kind));
                }
                ContentPart::Counters(name, separator, kind) => {
                    let values: Vec<String> = self
                        .counters
                        .iter()
                        .filter(|counter| counter.name == *name)
                        .map(|counter| counter_text(counter.value, *kind))
                        .collect();
                    text.push_str(&if values.is_empty() {
                        counter_text(0, *kind)
                    } else {
                        values.join(separator)
                    });
                }
            }
        }
        if !text.is_empty() {
            self.push_text(origin, &text, style, pending, shift);
        }
    }

    fn table_box(
        &mut self,
        origin: Option<Origin>,
        mut style: ComputedStyle,
        items: Vec<Item>,
    ) -> BlockBox {
        let mut parts = Parts::default();
        let mut table = self.table(&style, items, &mut parts);
        if style.inherited.border_collapse {
            let outer = collapse(&mut table, &style, &parts);
            let reset = Arc::make_mut(&mut style.reset);
            reset.border = outer;
            reset.border_color = web_style::Sides::all(web_style::Color::TRANSPARENT);
            reset.padding = web_style::Sides::all(web_style::LengthPercentage::ZERO);
        }
        table.parts = parts;
        BlockBox {
            node: origin.map(|origin| origin.node),
            pseudo: origin.and_then(|origin| origin.pseudo),
            style,
            contents: Contents::Table(Box::new(table)),
            marker: None,
        }
    }

    fn table(&mut self, style: &ComputedStyle, items: Vec<Item>, parts: &mut Parts) -> Table {
        let mut table = Table {
            captions: Vec::new(),
            rows: Vec::new(),
            parts: Parts::default(),
            collapsed: None,
        };
        let mut headers = Vec::new();
        let mut footers = Vec::new();
        self.depth += 1;
        let mut loose: Vec<Item> = Vec::new();
        let mut rows = Vec::new();
        for item in items {
            let Some(child_style) = self.item_style(item).cloned() else {
                continue;
            };
            let Some(origin) = self.element_origin(item) else {
                if !self.is_blank_text(item) {
                    loose.push(item);
                }
                continue;
            };
            let display = child_style.display();
            let proper = matches!(
                display,
                Display::TableCaption
                    | Display::TableHeaderGroup
                    | Display::TableFooterGroup
                    | Display::TableRowGroup
                    | Display::TableRow
            ) || (display == Display::None);
            if !proper {
                loose.push(item);
                continue;
            }
            if display == Display::None && child_style.reset.table_column.is_none() {
                continue;
            }
            self.flush_loose_row(style, &mut loose, &mut rows, None);
            match display {
                Display::TableCaption => {
                    table.captions.push(self.block(origin, child_style, None));
                }
                Display::TableHeaderGroup => {
                    self.row_group(origin, child_style, &mut headers, parts)
                }
                Display::TableFooterGroup => {
                    self.row_group(origin, child_style, &mut footers, parts)
                }
                Display::TableRowGroup => self.row_group(origin, child_style, &mut rows, parts),
                Display::TableRow => rows.push(self.row(origin, child_style)),
                _ => self.column(origin.node, child_style, parts),
            }
        }
        self.flush_loose_row(style, &mut loose, &mut rows, None);
        self.depth -= 1;
        headers.append(&mut rows);
        headers.append(&mut footers);
        table.rows = headers;
        table
    }

    fn element_origin(&self, item: Item) -> Option<Origin> {
        match item {
            Item::Node(node) => (self.document.kind(node) == Some(NodeKind::Element))
                .then(|| Origin::element(node)),
            Item::Generated(node, pseudo) => Some(Origin {
                node,
                pseudo: Some(pseudo),
            }),
        }
    }

    fn flush_loose_row(
        &mut self,
        parent: &ComputedStyle,
        loose: &mut Vec<Item>,
        rows: &mut Vec<TableRow>,
        group: Option<u32>,
    ) {
        if loose.is_empty() {
            return;
        }
        let row_style = self.styles.anonymous_box(parent, Display::TableRow);
        let cells = self.cells_of(&row_style, std::mem::take(loose));
        rows.push(TableRow {
            node: None,
            style: row_style,
            cells,
            group,
        });
    }

    fn cells_of(&mut self, row_style: &ComputedStyle, items: Vec<Item>) -> Vec<TableCell> {
        let mut cells = Vec::new();
        let mut loose: Vec<Item> = Vec::new();
        for item in items {
            let Some(style) = self.item_style(item).cloned() else {
                continue;
            };
            if let Some(origin) = self.element_origin(item)
                && style.display() == Display::TableCell
                && !style.is_out_of_flow()
                && style.reset.float == web_style::Float::None
            {
                self.flush_loose_cell(row_style, &mut loose, &mut cells);
                cells.push(self.cell(origin, style));
            } else if style.display() == Display::None {
            } else if !self.is_blank_text(item) || !loose.is_empty() {
                loose.push(item);
            }
        }
        self.flush_loose_cell(row_style, &mut loose, &mut cells);
        cells
    }

    fn flush_loose_cell(
        &mut self,
        row_style: &ComputedStyle,
        loose: &mut Vec<Item>,
        cells: &mut Vec<TableCell>,
    ) {
        while loose.last().is_some_and(|&item| self.is_blank_text(item)) {
            loose.pop();
        }
        if loose.is_empty() {
            return;
        }
        let style = self.styles.anonymous_box(row_style, Display::TableCell);
        self.depth += 1;
        let block = self.container(None, style, Some(std::mem::take(loose)), None);
        self.depth -= 1;
        cells.push(TableCell {
            block,
            columns: 1,
            rows: 1,
        });
    }

    fn row_group(
        &mut self,
        group: Origin,
        group_style: ComputedStyle,
        rows: &mut Vec<TableRow>,
        parts: &mut Parts,
    ) {
        let id = parts.groups.len() as u32;
        parts.groups.push(group_style.clone());
        let mut loose: Vec<Item> = Vec::new();
        for item in self.items_of(group) {
            let Some(style) = self.item_style(item).cloned() else {
                continue;
            };
            if let Some(origin) = self.element_origin(item)
                && style.display() == Display::TableRow
            {
                self.flush_loose_row(&group_style, &mut loose, rows, Some(id));
                let mut row = self.row(origin, style);
                row.group = Some(id);
                rows.push(row);
            } else if style.display() != Display::None && !self.is_blank_text(item) {
                loose.push(item);
            }
        }
        self.flush_loose_row(&group_style, &mut loose, rows, Some(id));
    }

    fn column(&self, node: NodeId, style: ComputedStyle, parts: &mut Parts) {
        let span = || {
            self.document
                .attribute(node, Atom::SPAN)
                .and_then(|span| span.trim().parse::<usize>().ok())
                .map_or(1, |span| span.clamp(1, 1000))
        };
        match style.reset.table_column {
            Some(TableColumn::Column) => {
                for _ in 0..span() {
                    parts.columns.push((style.clone(), None));
                }
            }
            Some(TableColumn::Group) => {
                let group = parts.column_groups.len();
                let start = parts.columns.len();
                for child in self.document.children(node) {
                    if let Some(child_style) = self.styles.get(child)
                        && self.document.kind(child) == Some(NodeKind::Element)
                        && child_style.reset.table_column == Some(TableColumn::Column)
                    {
                        let count = self
                            .document
                            .attribute(child, Atom::SPAN)
                            .and_then(|span| span.trim().parse::<usize>().ok())
                            .map_or(1, |span| span.clamp(1, 1000));
                        for _ in 0..count {
                            parts.columns.push((child_style.clone(), Some(group)));
                        }
                    }
                }
                if parts.columns.len() == start {
                    for _ in 0..span() {
                        parts
                            .columns
                            .push((self.styles.anonymous(&style), Some(group)));
                    }
                }
                parts.column_groups.push(style);
            }
            None => {}
        }
    }

    fn row(&mut self, row: Origin, style: ComputedStyle) -> TableRow {
        let items = self.items_of(row);
        let cells = self.cells_of(&style, items);
        TableRow {
            node: Some(row.node),
            style,
            cells,
            group: None,
        }
    }

    fn cell(&mut self, cell: Origin, style: ComputedStyle) -> TableCell {
        let attribute = |name: Atom, default: u32, max: u32| {
            if cell.pseudo.is_some() {
                return default as u16;
            }
            self.document
                .attribute(cell.node, name)
                .and_then(|value| value.trim().parse::<u32>().ok())
                .map_or(default, |value| value.clamp(1, max)) as u16
        };
        let columns = attribute(Atom::COLSPAN, 1, 1000);
        let rows = attribute(Atom::ROWSPAN, 1, 65534);
        self.depth += 1;
        let block = self.block(cell, style, None);
        self.depth -= 1;
        TableCell {
            block,
            columns,
            rows,
        }
    }

    fn flush(&mut self, blocks: &mut Vec<BlockBox>, mut pending: Pending, style: &ComputedStyle) {
        if pending.paragraph.is_blank() && !pending.paragraph.has_atomics() {
            let mut anchored = std::mem::take(&mut pending.out_of_flow);
            blocks.append(&mut anchored);
            return;
        }
        let anonymous = self.styles.anonymous(style);
        blocks.push(BlockBox {
            node: None,
            pseudo: None,
            style: anonymous,
            contents: Contents::Inline(Box::new(self.finish(pending))),
            marker: None,
        });
    }

    fn finish(&mut self, mut pending: Pending) -> InlineContent {
        pending.items.shrink_to_fit();
        pending.atomics.shrink_to_fit();
        pending.out_of_flow.shrink_to_fit();
        InlineContent {
            shaped: pending.paragraph.shape(self.fonts),
            items: pending.items,
            atomics: pending.atomics,
            out_of_flow: pending.out_of_flow,
        }
    }

    fn marker(&mut self, style: &ComputedStyle, ordinal: i32) -> Option<Box<InlineContent>> {
        let text = marker_text(style.inherited.list_style_type, ordinal)?;
        let mut paragraph = Paragraph::new(is_rtl(style));
        let mut run = run_style(self.fonts, style, 0, 0.0);
        run.white_space = web_style::WhiteSpace::Pre;
        paragraph.push_text(&text, run);
        Some(Box::new(InlineContent {
            shaped: paragraph.shape(self.fonts),
            items: Vec::new(),
            atomics: Vec::new(),
            out_of_flow: Vec::new(),
        }))
    }
}

const LIST_ITEM: &str = "list-item";

fn transformed<'t>(
    text: &'t str,
    style: &ComputedStyle,
    previous: Option<char>,
) -> std::borrow::Cow<'t, str> {
    use std::borrow::Cow;
    match style.inherited.text_transform {
        TextTransform::None => Cow::Borrowed(text),
        TextTransform::Uppercase => Cow::Owned(text.to_uppercase()),
        TextTransform::Lowercase => Cow::Owned(text.to_lowercase()),
        TextTransform::Capitalize => {
            let mut word_start = previous.is_none_or(|ch| !is_word_char(ch));
            let mut result = String::with_capacity(text.len());
            for ch in text.chars() {
                if word_start && is_word_char(ch) {
                    result.extend(ch.to_uppercase());
                } else {
                    result.push(ch);
                }
                word_start = !is_word_char(ch);
            }
            Cow::Owned(result)
        }
        TextTransform::FullWidth => Cow::Owned(text.chars().map(full_width).collect()),
        TextTransform::FullSizeKana => Cow::Owned(text.chars().map(full_size_kana).collect()),
    }
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '\'' || ch == '\u{2019}'
}

fn full_width(ch: char) -> char {
    match ch {
        ' ' => '\u{3000}',
        '!'..='~' => char::from_u32(ch as u32 - 0x21 + 0xFF01).unwrap_or(ch),
        _ => ch,
    }
}

fn full_size_kana(ch: char) -> char {
    const SMALL: &str = "ぁぃぅぇぉゕゖっゃゅょゎァィゥェォヵㇰヶㇱㇲッㇳㇴㇵㇶㇷㇸㇹㇺャュョㇻㇼㇽㇾㇿヮｧｨｩｪｫｯｬｭｮ";
    const FULL: &str = "あいうえおかけつやゆよわアイウエオカクケシスツトヌハヒフヘホムヤユヨラリルレロワｱｲｳｴｵﾂﾔﾕﾖ";
    SMALL
        .chars()
        .position(|small| small == ch)
        .and_then(|index| FULL.chars().nth(index))
        .unwrap_or(ch)
}

struct Counter {
    name: Arc<str>,
    value: i32,
    step: i32,
}

fn counter_text(value: i32, kind: ListStyleType) -> String {
    match kind {
        ListStyleType::None => String::new(),
        ListStyleType::Disc => "•".to_owned(),
        ListStyleType::Circle => "◦".to_owned(),
        ListStyleType::Square => "▪".to_owned(),
        ListStyleType::Decimal => value.to_string(),
        ListStyleType::LowerAlpha => alphabetic(value, b'a'),
        ListStyleType::UpperAlpha => alphabetic(value, b'A'),
        ListStyleType::LowerRoman => roman(value).to_ascii_lowercase(),
        ListStyleType::UpperRoman => roman(value),
    }
}

fn marker_text(kind: ListStyleType, ordinal: i32) -> Option<String> {
    match kind {
        ListStyleType::None => None,
        ListStyleType::Disc | ListStyleType::Circle | ListStyleType::Square => {
            Some(counter_text(ordinal, kind))
        }
        _ => Some(format!("{}.", counter_text(ordinal, kind))),
    }
}

fn alphabetic(ordinal: i32, first: u8) -> String {
    if ordinal < 1 {
        return ordinal.to_string();
    }
    let mut value = ordinal as u32;
    let mut letters = Vec::new();
    while value > 0 {
        value -= 1;
        letters.push(char::from(first + (value % 26) as u8));
        value /= 26;
    }
    letters.iter().rev().collect()
}

fn roman(ordinal: i32) -> String {
    if !(1..4000).contains(&ordinal) {
        return ordinal.to_string();
    }
    const NUMERALS: [(i32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut value = ordinal;
    let mut text = String::new();
    for (amount, numeral) in NUMERALS {
        while value >= amount {
            text.push_str(numeral);
            value -= amount;
        }
    }
    text
}
