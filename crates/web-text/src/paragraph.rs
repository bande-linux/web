use std::ops::Range;

use icu_segmenter::LineSegmenter;
use icu_segmenter::options::{LineBreakOptions, LineBreakWordOption};
use icu_properties::CodePointMapData;
use icu_properties::props::GeneralCategory;
use unicode_bidi::{BidiInfo, Level};
use web_style::{WhiteSpace, WordBreak};

use crate::fonts::{FontId, FontKey, Fonts, is_inherited};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RunStyle {
    pub font: FontKey,
    pub size: f32,
    pub line_height: Option<f32>,
    pub baseline_shift: f32,
    pub align: LineAlign,
    pub white_space: WhiteSpace,
    pub tag: u32,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    pub tab_columns: u32,
    pub word_break: WordBreak,
    pub overflow_wrap: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct LineAlign(u8);

const TOP: u8 = 4;
const BOTTOM: u8 = 5;

impl LineAlign {
    pub const BASELINE: Self = Self(0);
    pub const MIDDLE: Self = Self(1);
    pub const TEXT_TOP: Self = Self(2);
    pub const TEXT_BOTTOM: Self = Self(3);

    pub fn top(group: usize) -> Self {
        Self(TOP | ((group as u8) << 3))
    }

    pub fn bottom(group: usize) -> Self {
        Self(BOTTOM | ((group as u8) << 3))
    }

    pub fn is_edge(self) -> bool {
        matches!(self.0 & 7, TOP | BOTTOM)
    }

    fn is_top(self) -> bool {
        self.0 & 7 == TOP
    }

    fn shift(self, ascent: f32, descent: f32, baseline_shift: f32) -> f32 {
        match self {
            Self::MIDDLE => baseline_shift - (ascent - descent) / 2.0,
            Self::TEXT_TOP => baseline_shift - ascent,
            Self::TEXT_BOTTOM => baseline_shift + descent,
            _ => baseline_shift,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Glyph {
    pub id: u16,
    y_offset: i16,
    pub x: f32,
}

const Y_UNITS: f32 = 16.0;

impl Glyph {
    pub fn new(id: u16, x: f32, y: f32) -> Self {
        Glyph {
            id,
            y_offset: (y * Y_UNITS).round() as i16,
            x,
        }
    }

    pub fn y(self) -> f32 {
        f32::from(self.y_offset) / Y_UNITS
    }

    pub fn translated(self, x: f32, y: f32) -> (u32, f32, f32) {
        (u32::from(self.id), self.x + x, self.y() + y)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct LineItem {
    pub tag: u32,
    pub font: FontId,
    pub size: f32,
    pub x: f32,
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
    pub baseline_shift: f32,
    pub glyphs: Range<usize>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Line {
    pub left: f32,
    pub available: f32,
    pub top: f32,
    pub baseline: f32,
    pub height: f32,
    pub width: f32,
    pub items: Range<usize>,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Lines {
    pub lines: Vec<Line>,
    pub items: Vec<LineItem>,
    pub glyphs: Vec<Glyph>,
    pub height: f32,
    pub empty_anchors: Vec<(u32, f32)>,
}

impl Lines {
    pub fn heap_bytes(&self) -> usize {
        self.lines.capacity() * size_of::<Line>()
            + self.items.capacity() * size_of::<LineItem>()
            + self.glyphs.capacity() * size_of::<Glyph>()
    }
}

#[derive(Default)]
pub struct Paragraph {
    rtl: bool,
    nowrap: bool,
    atomics: u16,
    text: String,
    runs: Vec<(u32, RunStyle, Piece)>,
    column: u32,
    trimmable: usize,
    collapsing: bool,
    has_spacer: bool,
}

impl Paragraph {
    pub fn new(rtl: bool) -> Self {
        Self {
            rtl,
            collapsing: true,
            ..Self::default()
        }
    }

    pub fn set_nowrap(&mut self, nowrap: bool) {
        self.nowrap = nowrap;
    }

    pub fn is_rtl(&self) -> bool {
        self.rtl
    }

    pub fn is_blank(&self) -> bool {
        self.text.is_empty() && !self.has_spacer
    }

    pub fn last_char(&self) -> Option<char> {
        self.text.chars().next_back()
    }

    pub fn push_text(&mut self, text: &str, style: RunStyle) {
        let white_space = style.white_space;
        for ch in text.chars() {
            match ch {
                '\r' => {}
                '\n' if white_space.preserves_newlines() => {
                    if white_space.collapses_spaces() {
                        self.text.truncate(self.text.len() - self.trimmable);
                    }
                    self.text.push('\n');
                    self.column = 0;
                    self.trimmable = 0;
                    self.collapsing = white_space.collapses_spaces();
                }
                ' ' | '\t' | '\n' | '\x0c' if white_space.collapses_spaces() => {
                    if !self.collapsing {
                        self.text.push(' ');
                        self.trimmable = 1;
                        self.collapsing = true;
                    }
                }
                '\t' => {
                    let tab = style.tab_columns.max(1);
                    let spaces = tab - self.column % tab;
                    self.text.extend(std::iter::repeat_n(' ', spaces as usize));
                    self.column += spaces;
                    self.trimmable = 0;
                    self.collapsing = false;
                }
                _ => {
                    self.text.push(ch);
                    self.column += 1;
                    self.trimmable = 0;
                    self.collapsing = false;
                }
            }
        }
        self.end_run(style);
    }

    pub fn push_break(&mut self, style: RunStyle) {
        self.text.truncate(self.text.len() - self.trimmable);
        self.text.push('\n');
        self.column = 0;
        self.trimmable = 0;
        self.collapsing = true;
        self.end_run(style);
    }

    fn end_run(&mut self, style: RunStyle) {
        let end = self.text.len() as u32;
        match self.runs.last_mut() {
            Some((last_end, last, Piece::Text)) if *last == style => *last_end = end,
            Some((last_end, _, Piece::Text)) if *last_end == end => {}
            _ => self.runs.push((end, style, Piece::Text)),
        }
    }

    pub fn push_spacer(&mut self, width: f32, style: RunStyle) {
        if width != 0.0 {
            self.has_spacer = true;
            self.runs
                .push((self.text.len() as u32, style, Piece::Spacer(width)));
        }
    }

    pub fn push_atomic(&mut self, index: u16, style: RunStyle) {
        self.runs
            .push((self.text.len() as u32, style, Piece::Atomic(index)));
        self.atomics = self.atomics.max(index + 1);
        self.trimmable = 0;
        self.collapsing = false;
    }

    pub fn push_anchor(&mut self, index: u16, style: RunStyle) {
        self.runs
            .push((self.text.len() as u32, style, Piece::Anchor(index)));
    }

    pub fn has_atomics(&self) -> bool {
        self.atomics > 0
    }

    fn run_ranges(&self) -> impl Iterator<Item = (Range<usize>, RunStyle, Piece)> + '_ {
        let mut start = 0;
        self.runs.iter().map(move |&(end, style, spacer)| {
            let range = start..end as usize;
            start = end as usize;
            (range, style, spacer)
        })
    }

    fn levels(&self) -> Vec<Level> {
        if !self.rtl && self.text.chars().all(|ch| ch < '\u{590}') {
            return Vec::new();
        }
        let base = if self.rtl { Level::rtl() } else { Level::ltr() };
        BidiInfo::new(&self.text, Some(base)).levels
    }

    pub fn shape(self, fonts: &mut Fonts) -> ShapedParagraph {
        let levels = self.levels();
        let base = if self.rtl { 1 } else { 0 };
        let level_at = |position: usize| {
            levels
                .get(position)
                .map_or(base, |level: &Level| level.number())
        };
        let mut shaped = ShapedParagraph {
            glyphs: Vec::new(),
            offsets: Vec::new(),
            spans: Vec::new(),
            styles: self.runs.iter().map(|&(_, style, _)| style).collect(),
        };
        let mut clusters = Vec::new();
        for (run_index, (range, style, piece)) in self.run_ranges().enumerate() {
            if piece != Piece::Text {
                let (flags, id, advance) = match piece {
                    Piece::Atomic(index) => (ATOMIC, index, 0.0),
                    Piece::Anchor(index) => (ANCHOR, index, 0.0),
                    Piece::Spacer(width) => (SPACER, 0, width),
                    Piece::Text => (0, 0, 0.0),
                };
                if let Some(font) = fonts.primary(style.font) {
                    shaped.spans.push(Span {
                        font,
                        run: run_index as u32,
                        level: level_at(range.start.saturating_sub(1)),
                    });
                    shaped.glyphs.push(ShapedGlyph {
                        id,
                        flags,
                        span: (shaped.spans.len() - 1) as u32,
                        advance,
                    });
                    clusters.push(range.start as u32);
                }
                continue;
            }
            let Some(run_text) = self.text.get(range.clone()) else {
                continue;
            };
            let mut group: Option<(Span, usize)> = None;
            let mut previous_font = None;
            let mut ascii = [None; 128];
            for (offset, ch) in run_text.char_indices() {
                let position = range.start + offset;
                let font = match ascii.get_mut(ch as usize) {
                    _ if ch == '\n' => None,
                    Some(Some(font)) => *font,
                    Some(slot) => *slot.insert(fonts.for_char(style.font, ch)),
                    None if is_inherited(ch) && previous_font.is_some() => previous_font,
                    None => fonts.for_char(style.font, ch),
                };
                previous_font = font.or(previous_font);
                let span = font.map(|font| Span {
                    font,
                    run: run_index as u32,
                    level: level_at(position),
                });
                if group.map(|(group_span, _)| group_span) != span {
                    if let Some((group_span, start)) = group.take() {
                        self.shape_group(
                            fonts,
                            group_span,
                            start..position,
                            style,
                            &mut shaped,
                            &mut clusters,
                        );
                    }
                    group = span.map(|span| (span, position));
                }
                if ch == '\n'
                    && let Some(font) = previous_font.or_else(|| fonts.primary(style.font))
                {
                    shaped.spans.push(Span {
                        font,
                        run: run_index as u32,
                        level: base,
                    });
                    shaped.glyphs.push(ShapedGlyph {
                        id: 0,
                        flags: NEWLINE | MANDATORY,
                        span: (shaped.spans.len() - 1) as u32,
                        advance: 0.0,
                    });
                    clusters.push(position as u32);
                }
            }
            if let Some((group_span, start)) = group {
                self.shape_group(
                    fonts,
                    group_span,
                    start..range.end,
                    style,
                    &mut shaped,
                    &mut clusters,
                );
            }
        }
        self.mark_breaks(&mut shaped.glyphs, &clusters);
        shaped.glyphs.shrink_to_fit();
        shaped.offsets.shrink_to_fit();
        shaped.spans.shrink_to_fit();
        shaped
    }

    fn mark_breaks(&self, glyphs: &mut [ShapedGlyph], clusters: &[u32]) {
        self.mark_text_breaks(glyphs, clusters);
        for index in 0..glyphs.len() {
            let atomic = glyphs
                .get(index)
                .is_some_and(|glyph| glyph.flags & ANCHOR == ATOMIC);
            if atomic {
                if let Some(glyph) = glyphs.get_mut(index) {
                    glyph.flags |= BREAK;
                }
                if let Some(previous) = index
                    .checked_sub(1)
                    .and_then(|before| glyphs.get_mut(before))
                {
                    previous.flags |= BREAK;
                }
            }
        }
    }

    fn mark_text_breaks(&self, glyphs: &mut [ShapedGlyph], clusters: &[u32]) {
        let text = self.text.as_str();
        let bytes = text.as_bytes();
        for (glyph, &cluster) in glyphs.iter_mut().zip(clusters) {
            if glyph.flags & (SPACER | ATOMIC) == 0 && bytes.get(cluster as usize) == Some(&b' ') {
                glyph.flags |= SPACE;
                let run = self
                    .runs
                    .iter()
                    .find(|&&(end, _, piece)| end > cluster && piece == Piece::Text);
                if run.is_none_or(|(_, style, _)| style.white_space != WhiteSpace::Pre) {
                    glyph.flags |= HANGS;
                }
            }
        }
        let special = self.runs.iter().any(|(_, style, piece)| {
            *piece == Piece::Text && style.word_break != WordBreak::Normal
        });
        let special_breaks = |option: LineBreakWordOption| -> Vec<usize> {
            let mut options = LineBreakOptions::default();
            options.word_option = Some(option);
            LineSegmenter::new_auto(options).segment_str(text).collect()
        };
        let (break_all, keep_all) = if special {
            (
                special_breaks(LineBreakWordOption::BreakAll),
                special_breaks(LineBreakWordOption::KeepAll),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        let wraps_before = |position: usize| {
            let run = self
                .runs
                .iter()
                .position(|&(end, _, piece)| end as usize >= position && piece == Piece::Text);
            let boundary = run
                .and_then(|run| self.runs.get(run))
                .is_some_and(|&(end, _, _)| end as usize == position);
            if boundary {
                return !self.nowrap;
            }
            run.and_then(|run| self.runs.get(run))
                .is_none_or(|(_, style, _)| style.white_space.wraps())
        };
        let word_break_at = |position: usize| {
            self.runs
                .iter()
                .find(|&&(end, _, piece)| end as usize >= position && piece == Piece::Text)
                .map_or(WordBreak::Normal, |(_, style, _)| style.word_break)
        };
        let normal: Vec<usize> = LineSegmenter::new_auto(LineBreakOptions::default())
            .segment_str(text)
            .collect();
        let mut positions: Vec<usize> = if special {
            let mut merged: Vec<usize> = normal
                .iter()
                .copied()
                .filter(|&position| word_break_at(position) == WordBreak::Normal)
                .chain(
                    break_all
                        .iter()
                        .copied()
                        .filter(|&position| word_break_at(position) == WordBreak::BreakAll),
                )
                .chain(
                    keep_all
                        .iter()
                        .copied()
                        .filter(|&position| word_break_at(position) == WordBreak::KeepAll),
                )
                .collect();
            merged.sort_unstable();
            merged.dedup();
            merged
        } else {
            normal
        };
        positions.retain(|&position| {
            position != 0 && (bytes.get(position - 1) == Some(&b'\n') || wraps_before(position))
        });
        let mut glyph = 0;
        for position in positions {
            while clusters
                .get(glyph + 1)
                .is_some_and(|&cluster| (cluster as usize) < position)
            {
                glyph += 1;
            }
            if let Some(shaped) = glyphs.get_mut(glyph)
                && clusters
                    .get(glyph)
                    .is_some_and(|&cluster| (cluster as usize) < position)
            {
                shaped.flags |= BREAK;
            }
        }
    }

    fn shape_group(
        &self,
        fonts: &mut Fonts,
        span: Span,
        range: Range<usize>,
        style: RunStyle,
        shaped: &mut ShapedParagraph,
        clusters: &mut Vec<u32>,
    ) {
        let Some(text) = self.text.get(range.clone()) else {
            return;
        };
        let rtl = span.level % 2 == 1;
        let span_index = shaped.spans.len() as u32;
        shaped.spans.push(span);
        let spaced = style.letter_spacing != 0.0 || style.word_spacing != 0.0;
        let mut offset = range.start as u32;
        for word in text.split_inclusive(' ') {
            let first = shaped.glyphs.len();
            fonts.shape_word(
                span.font,
                style.size,
                rtl,
                style.letter_spacing != 0.0,
                word,
                &mut |glyph, (x, y)| {
                    let moved = x != 0.0 || y != 0.0;
                    if moved {
                        shaped.offsets.push((shaped.glyphs.len() as u32, x, y));
                    }
                    shaped.glyphs.push(ShapedGlyph {
                        id: glyph.id,
                        flags: if moved { OFFSET } else { 0 },
                        span: span_index,
                        advance: glyph.advance,
                    });
                    clusters.push(offset + glyph.cluster);
                },
            );
            if spaced {
                apply_spacing(
                    word,
                    style,
                    shaped.glyphs.get_mut(first..).unwrap_or_default(),
                    clusters.get(first..).unwrap_or_default(),
                    offset,
                );
            }
            offset += word.len() as u32;
        }
    }
}

fn apply_spacing(
    word: &str,
    style: RunStyle,
    glyphs: &mut [ShapedGlyph],
    clusters: &[u32],
    offset: u32,
) {
    let bytes = word.as_bytes();
    let mut index = 0;
    while index < glyphs.len() {
        let cluster = clusters.get(index).copied().unwrap_or(offset);
        let mut last = index;
        while clusters.get(last + 1) == Some(&cluster) {
            last += 1;
        }
        if let Some(glyph) = glyphs.get_mut(last) {
            glyph.advance += style.letter_spacing;
            let at = cluster.saturating_sub(offset) as usize;
            let spacing = bytes.get(at) == Some(&b' ')
                || word.get(at..).is_some_and(|rest| rest.starts_with('\u{a0}'));
            if spacing {
                glyph.advance += style.word_spacing;
            }
        }
        index = last + 1;
    }
}

pub fn first_letter(text: &str) -> Option<Range<usize>> {
    let maps = CodePointMapData::<GeneralCategory>::new();
    let is_punctuation = |ch: char| {
        matches!(
            maps.get(ch),
            GeneralCategory::OpenPunctuation
                | GeneralCategory::ClosePunctuation
                | GeneralCategory::InitialPunctuation
                | GeneralCategory::FinalPunctuation
                | GeneralCategory::OtherPunctuation
        )
    };
    let is_mark = |ch: char| {
        matches!(
            maps.get(ch),
            GeneralCategory::NonspacingMark
                | GeneralCategory::EnclosingMark
                | GeneralCategory::SpacingMark
        )
    };
    let is_space = |ch: char| maps.get(ch) == GeneralCategory::SpaceSeparator;
    let start = text.len() - text.trim_start_matches(['\n', '\t', '\r', '\x0c', ' ']).len();
    let rest = text.get(start..)?;
    let mut letter = None;
    for (at, ch) in rest.char_indices() {
        if is_punctuation(ch) || is_space(ch) {
            continue;
        }
        letter = Some((at, ch));
        break;
    }
    let (at, ch) = letter?;
    if ch.is_control() {
        return None;
    }
    let mut end = at + ch.len_utf8();
    let mut marks = true;
    let mut pending = end;
    for (offset, next) in rest.get(end..)?.char_indices() {
        let position = end + offset;
        if marks && is_mark(next) {
            pending = position + next.len_utf8();
        } else if is_punctuation(next) {
            marks = false;
            pending = position + next.len_utf8();
        } else if is_space(next) && next != ' ' {
            marks = false;
        } else {
            break;
        }
    }
    end = pending;
    Some(start..start + end)
}

const SPACE: u8 = 1;
const BREAK: u8 = 2;
const MANDATORY: u8 = 4;
const NEWLINE: u8 = 8;
const SPACER: u8 = 16;
const ATOMIC: u8 = 32;
const OFFSET: u8 = 64;
const HANGS: u8 = 128;
const ANCHOR: u8 = ATOMIC | SPACER;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Piece {
    Text,
    Spacer(f32),
    Atomic(u16),
    Anchor(u16),
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct AtomicSize {
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Span {
    font: FontId,
    run: u32,
    level: u8,
}

#[derive(Clone, Copy, Debug)]
struct ShapedGlyph {
    id: u16,
    flags: u8,
    span: u32,
    advance: f32,
}

impl ShapedGlyph {
    fn hangs(&self) -> bool {
        self.flags & (HANGS | NEWLINE) != 0
    }

    fn is_anchor(&self) -> bool {
        self.flags & ANCHOR == ANCHOR
    }
}

pub struct ShapedParagraph {
    glyphs: Vec<ShapedGlyph>,
    offsets: Vec<(u32, f32, f32)>,
    spans: Vec<Span>,
    styles: Vec<RunStyle>,
}

struct Segment {
    glyphs: Range<usize>,
    width: f32,
    trailing: f32,
    mandatory: bool,
    visible: bool,
}

struct LineSpace {
    left: f32,
    width: f32,
}

pub trait LinePlacement {
    fn space(&mut self, top: f32) -> (f32, f32);
    fn is_float(&self, anchor: u16) -> bool;
    fn place(&mut self, anchor: u16, top: f32, used: f32, has_content: bool) -> Option<(f32, f32)>;
    fn place_below(&mut self, anchor: u16, top: f32);
}

impl ShapedParagraph {
    pub fn heap_bytes(&self) -> usize {
        self.glyphs.capacity() * size_of::<ShapedGlyph>()
            + self.offsets.capacity() * size_of::<(u32, f32, f32)>()
            + self.spans.capacity() * size_of::<Span>()
            + self.styles.capacity() * size_of::<RunStyle>()
    }

    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }

    fn offset(&self, index: usize) -> (f32, f32) {
        self.offsets
            .binary_search_by_key(&(index as u32), |&(at, _, _)| at)
            .ok()
            .and_then(|found| self.offsets.get(found))
            .map_or((0.0, 0.0), |&(_, x, y)| (x, y))
    }

    pub fn intrinsic(&self, atomics: &[AtomicSize]) -> (f32, f32) {
        let mut minimum: f32 = 0.0;
        let mut maximum: f32 = 0.0;
        let mut line = 0.0;
        for segment in self.segments(atomics) {
            minimum = minimum.max(segment.width - segment.trailing);
            line += segment.width;
            if segment.mandatory {
                maximum = maximum.max(line - segment.trailing);
                line = 0.0;
            }
        }
        (minimum, maximum.max(line))
    }

    pub fn lines(
        &self,
        fonts: &mut Fonts,
        placement: &mut dyn LinePlacement,
        strut: RunStyle,
        justify: bool,
        indent: f32,
        atomics: &[AtomicSize],
    ) -> Lines {
        let mut lines = Lines::default();
        let mut start = 0;
        let mut line_width = 0.0;
        let mut has_content = false;
        let (left, width) = placement.space(0.0);
        let mut line = LineSpace {
            left: left + indent,
            width: width - indent,
        };
        let mut deferred: Vec<u16> = Vec::new();
        let mut segments = self.segments(atomics);
        let mut split: Vec<Segment> = Vec::new();
        while let Some(segment) = split.pop().or_else(|| segments.next()) {
            if segment.width - segment.trailing > line.width
                && segment.glyphs.len() > 1
                && self.wraps_anywhere(&segment)
            {
                split.extend(segment.glyphs.clone().rev().filter_map(|index| {
                    let glyph = self.glyphs.get(index)?;
                    let width = Self::advance(glyph, atomics);
                    let last = index + 1 == segment.glyphs.end;
                    Some(Segment {
                        glyphs: index..index + 1,
                        width,
                        trailing: if glyph.hangs() { width } else { 0.0 },
                        mandatory: last && segment.mandatory,
                        visible: !glyph.is_anchor(),
                    })
                }));
                continue;
            }
            if has_content && line_width + segment.width - segment.trailing > line.width {
                let justify_to = justify.then_some(line.width);
                self.push_line(
                    &mut lines,
                    fonts,
                    start..segment.glyphs.start,
                    strut,
                    justify_to,
                    (line.left, line.width),
                    atomics,
                );
                for anchor in deferred.drain(..) {
                    placement.place_below(anchor, lines.height);
                }
                (line.left, line.width) = placement.space(lines.height);
                start = segment.glyphs.start;
                line_width = 0.0;
            }
            for glyph in self.glyphs.get(segment.glyphs.clone()).unwrap_or_default() {
                if glyph.is_anchor() && placement.is_float(glyph.id) {
                    match placement.place(glyph.id, lines.height, line_width, has_content) {
                        Some((left, width)) => {
                            let first = if lines.lines.is_empty() { indent } else { 0.0 };
                            line.left = left + first;
                            line.width = width - first;
                        }
                        None => deferred.push(glyph.id),
                    }
                }
            }
            line_width += segment.width;
            has_content = has_content || segment.visible;
            if segment.mandatory {
                self.push_line(
                    &mut lines,
                    fonts,
                    start..segment.glyphs.end,
                    strut,
                    None,
                    (line.left, line.width),
                    atomics,
                );
                for anchor in deferred.drain(..) {
                    placement.place_below(anchor, lines.height);
                }
                (line.left, line.width) = placement.space(lines.height);
                start = segment.glyphs.end;
                line_width = 0.0;
                has_content = false;
            }
        }
        if has_content && start < self.glyphs.len() {
            self.push_line(
                &mut lines,
                fonts,
                start..self.glyphs.len(),
                strut,
                None,
                (line.left, line.width),
                atomics,
            );
        } else if start < self.glyphs.len() {
            self.push_empty_anchors(&mut lines, start..self.glyphs.len());
        }
        for anchor in deferred {
            placement.place_below(anchor, lines.height);
        }
        lines
    }

    fn push_empty_anchors(&self, lines: &mut Lines, range: Range<usize>) {
        for glyph in self.glyphs.get(range).unwrap_or_default() {
            if glyph.is_anchor()
                && let Some(span) = self.spans.get(glyph.span as usize)
                && let Some(style) = self.styles.get(span.run as usize)
            {
                lines.empty_anchors.push((style.tag, lines.height));
            }
        }
    }

    fn wraps_anywhere(&self, segment: &Segment) -> bool {
        self.glyphs
            .get(segment.glyphs.start)
            .and_then(|glyph| self.spans.get(glyph.span as usize))
            .and_then(|span| self.styles.get(span.run as usize))
            .is_some_and(|style| style.overflow_wrap)
    }

    fn advance(glyph: &ShapedGlyph, atomics: &[AtomicSize]) -> f32 {
        if glyph.flags & ANCHOR == ATOMIC {
            atomics
                .get(usize::from(glyph.id))
                .map_or(0.0, |atomic| atomic.width)
        } else {
            glyph.advance
        }
    }

    fn segments<'a>(&'a self, atomics: &'a [AtomicSize]) -> impl Iterator<Item = Segment> + 'a {
        let mut start = 0;
        std::iter::from_fn(move || {
            let first = start;
            let mut width = 0.0;
            let mut trailing = 0.0;
            let mut visible = false;
            while let Some(glyph) = self.glyphs.get(start) {
                start += 1;
                let advance = Self::advance(glyph, atomics);
                width += advance;
                visible = visible || !glyph.is_anchor();
                if glyph.hangs() {
                    trailing += advance;
                } else {
                    trailing = 0.0;
                }
                if glyph.flags & BREAK != 0 {
                    return Some(Segment {
                        glyphs: first..start,
                        width,
                        trailing,
                        mandatory: glyph.flags & MANDATORY != 0,
                        visible,
                    });
                }
            }
            (start > first).then_some(Segment {
                glyphs: first..start,
                width,
                trailing,
                mandatory: false,
                visible,
            })
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn push_line(
        &self,
        lines: &mut Lines,
        fonts: &mut Fonts,
        range: Range<usize>,
        strut: RunStyle,
        justify_to: Option<f32>,
        left: (f32, f32),
        atomics: &[AtomicSize],
    ) {
        let (mut above, mut below) = vertical_extent(fonts, None, strut);
        let first_item = lines.items.len();
        let mut edges: Vec<(usize, usize, f32)> = Vec::new();
        let mut groups: Vec<(LineAlign, f32, f32)> = Vec::new();
        let base = range.start;
        let shaped = self.glyphs.get(range).unwrap_or_default();
        let hanging = shaped
            .iter()
            .rev()
            .take_while(|glyph| glyph.hangs())
            .count();
        let shaped = shaped.get(..shaped.len() - hanging).unwrap_or_default();
        let order = visual_order(shaped, |glyph| {
            self.spans
                .get(glyph.span as usize)
                .map_or(0, |span| span.level)
        });
        let natural: f32 = shaped
            .iter()
            .map(|glyph| Self::advance(glyph, atomics))
            .sum();
        let spaces = shaped
            .iter()
            .filter(|glyph| glyph.flags & SPACE != 0)
            .count();
        let expansion = match justify_to {
            Some(width) if spaces > 0 && width > natural => (width - natural) / spaces as f32,
            _ => 0.0,
        };
        let mut pen = 0.0;
        let visual: Vec<(usize, &ShapedGlyph)> = order
            .iter()
            .filter_map(|&index| Some((base + index, shaped.get(index)?)))
            .filter(|(_, glyph)| glyph.flags & NEWLINE == 0)
            .collect();
        for group in visual.chunk_by(|a, b| {
            a.1.span == b.1.span && !a.1.is_anchor() && !b.1.is_anchor()
        }) {
            let Some((span, style)) = group.first().map(|first| first.1).and_then(|first| {
                let span = *self.spans.get(first.span as usize)?;
                Some((span, *self.styles.get(span.run as usize)?))
            }) else {
                continue;
            };
            let atomic = group
                .first()
                .map(|first| first.1)
                .filter(|glyph| glyph.flags & ANCHOR == ATOMIC)
                .and_then(|glyph| atomics.get(usize::from(glyph.id)));
            let anchor_only = group.iter().all(|(_, glyph)| glyph.is_anchor());
            let (raw_above, raw_below) = match atomic {
                Some(atomic) => (atomic.ascent, atomic.descent),
                None if anchor_only => (0.0, 0.0),
                None => vertical_extent(
                    fonts,
                    style.line_height.is_none().then_some(span.font),
                    RunStyle {
                        baseline_shift: 0.0,
                        ..style
                    },
                ),
            };
            let shift = style
                .align
                .shift(raw_above, raw_below, style.baseline_shift);
            match style.align {
                align if align.is_edge() => {
                    match groups.iter_mut().find(|group| group.0 == align) {
                        Some(group) => {
                            group.1 = group.1.max(raw_above + shift);
                            group.2 = group.2.max(raw_below - shift);
                        }
                        None => groups.push((align, raw_above + shift, raw_below - shift)),
                    }
                    let group = groups
                        .iter()
                        .position(|group| group.0 == align)
                        .unwrap_or_default();
                    edges.push((lines.items.len(), group, shift));
                }
                _ if anchor_only => {}
                _ => {
                    above = f32::max(above, raw_above + shift);
                    below = f32::max(below, raw_below - shift);
                }
            }
            let metrics = fonts.metrics(span.font);
            let scale = style.size / metrics.units_per_em;
            let start = pen;
            let glyph_start = lines.glyphs.len();
            for &(index, glyph) in group {
                if glyph.flags & (SPACER | ATOMIC) == 0 {
                    let (x_offset, y_offset) = if glyph.flags & OFFSET != 0 {
                        self.offset(index)
                    } else {
                        (0.0, 0.0)
                    };
                    lines
                        .glyphs
                        .push(Glyph::new(glyph.id, pen - start + x_offset, -y_offset));
                }
                pen += Self::advance(glyph, atomics);
                if glyph.flags & SPACE != 0 {
                    pen += expansion;
                }
            }
            lines.items.push(LineItem {
                tag: style.tag,
                font: span.font,
                size: style.size,
                x: start,
                width: pen - start,
                ascent: atomic.map_or(metrics.ascent * scale, |atomic| atomic.ascent),
                descent: atomic.map_or(metrics.descent * scale, |atomic| atomic.descent),
                baseline_shift: shift,
                glyphs: glyph_start..lines.glyphs.len(),
            });
        }
        for &(align, group_above, group_below) in &groups {
            let grow = group_above + group_below - (above + below);
            if grow > 0.0 {
                if align.is_top() {
                    below += grow;
                } else {
                    above += grow;
                }
            }
        }
        for (index, group, shift) in edges {
            let Some(&(align, group_above, group_below)) = groups.get(group) else {
                continue;
            };
            if let Some(item) = lines.items.get_mut(index) {
                item.baseline_shift = shift
                    + if align.is_top() {
                        above - group_above
                    } else {
                        group_below - below
                    };
            }
        }
        let top = lines.height;
        lines.lines.push(Line {
            left: left.0,
            available: left.1,
            top,
            baseline: top + above,
            height: above + below,
            width: pen,
            items: first_item..lines.items.len(),
        });
        lines.height = top + above + below;
    }
}

fn visual_order(glyphs: &[ShapedGlyph], level: impl Fn(&ShapedGlyph) -> u8) -> Vec<usize> {
    let mut order: Vec<usize> = (0..glyphs.len()).collect();
    let levels: Vec<u8> = glyphs.iter().map(&level).collect();
    let highest = levels.iter().copied().max().unwrap_or(0);
    let Some(lowest_odd) = levels.iter().copied().filter(|level| level % 2 == 1).min() else {
        return order;
    };
    for threshold in (lowest_odd..=highest).rev() {
        let mut index = 0;
        while index < order.len() {
            let at = |index: usize| {
                order
                    .get(index)
                    .and_then(|&glyph| levels.get(glyph))
                    .copied()
                    .unwrap_or(0)
            };
            if at(index) >= threshold {
                let mut end = index;
                while end < order.len() && at(end) >= threshold {
                    end += 1;
                }
                if let Some(run) = order.get_mut(index..end) {
                    run.reverse();
                }
                index = end;
            } else {
                index += 1;
            }
        }
    }
    order
}

fn vertical_extent(fonts: &mut Fonts, font: Option<FontId>, style: RunStyle) -> (f32, f32) {
    let Some(font) = font.or_else(|| fonts.primary(style.font)) else {
        let height = style.line_height.unwrap_or(style.size * 1.2);
        return (height * 0.8, height * 0.2);
    };
    let metrics = fonts.metrics(font);
    let scale = style.size / metrics.units_per_em;
    let ascent = metrics.ascent * scale;
    let descent = metrics.descent * scale;
    let line_height = style
        .line_height
        .unwrap_or(ascent + descent + metrics.line_gap * scale);
    let half_leading = (line_height - ascent - descent) / 2.0;
    (
        ascent + half_leading + style.baseline_shift,
        descent + half_leading - style.baseline_shift,
    )
}

pub fn inline_extent(fonts: &mut Fonts, style: RunStyle) -> (f32, f32) {
    vertical_extent(
        fonts,
        None,
        RunStyle {
            baseline_shift: 0.0,
            ..style
        },
    )
}

pub fn content_metrics(fonts: &mut Fonts, style: RunStyle) -> (f32, f32, f32) {
    let Some(font) = fonts.primary(style.font) else {
        return (style.size * 0.8, style.size * 0.2, style.size * 0.5);
    };
    let metrics = fonts.metrics(font);
    let scale = style.size / metrics.units_per_em;
    (
        metrics.ascent * scale,
        metrics.descent * scale,
        metrics.x_height * scale,
    )
}
