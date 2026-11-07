use web_layout::{BoxFragment, Fragment, FragmentTree, Rect, TextFragment};

const MAX_MATCHES: usize = 10_000;
const NO_BREAK_SPACE: char = '\u{a0}';
const SOFT_HYPHEN: char = '\u{ad}';

struct Placed {
    cluster: u32,
    run: u32,
    rect: Rect,
}

pub struct TextIndex {
    folded: Vec<char>,
    offsets: Vec<u32>,
    length: u32,
    placed: Vec<Placed>,
}

fn fold(text: &str) -> (Vec<char>, Vec<u32>) {
    let mut folded = Vec::with_capacity(text.len());
    let mut offsets = Vec::with_capacity(text.len());
    for (offset, character) in text.char_indices() {
        match character {
            SOFT_HYPHEN => {}
            NO_BREAK_SPACE => {
                folded.push(' ');
                offsets.push(offset as u32);
            }
            character => {
                for lower in character.to_lowercase() {
                    folded.push(lower);
                    offsets.push(offset as u32);
                }
            }
        }
    }
    (folded, offsets)
}

fn union(area: Rect, rect: Rect) -> Rect {
    let x = area.x.min(rect.x);
    let y = area.y.min(rect.y);
    Rect {
        x,
        y,
        width: (area.x + area.width).max(rect.x + rect.width) - x,
        height: (area.y + area.height).max(rect.y + rect.height) - y,
    }
}

impl TextIndex {
    pub(crate) fn new(tree: &FragmentTree) -> Self {
        let (folded, offsets) = fold(&tree.text);
        let mut index = Self {
            folded,
            offsets,
            length: tree.text.len() as u32,
            placed: Vec::new(),
        };
        let mut run = 0;
        index.visit(tree, &tree.root, 0.0, 0.0, &mut run);
        index.placed.sort_by_key(|placed| placed.cluster);
        index
    }

    fn visit(
        &mut self,
        tree: &FragmentTree,
        fragment: &BoxFragment,
        x: f32,
        y: f32,
        run: &mut u32,
    ) {
        let (x, y) = (x + fragment.rect.x, y + fragment.rect.y);
        for child in &fragment.children {
            match child {
                Fragment::Box(child) => self.visit(tree, child, x, y, run),
                Fragment::Text(text) => {
                    self.place(tree, text, x, y, *run);
                    *run += 1;
                }
            }
        }
    }

    fn place(&mut self, tree: &FragmentTree, text: &TextFragment, x: f32, y: f32, run: u32) {
        let range = text.glyphs.start as usize..text.glyphs.end as usize;
        let glyphs = tree.glyphs.get(range.clone()).unwrap_or_default();
        let clusters = tree.clusters.get(range).unwrap_or_default();
        let left = x + text.rect.x;
        for (position, (glyph, cluster)) in glyphs.iter().zip(clusters).enumerate() {
            if *cluster == web_text::NO_TEXT {
                continue;
            }
            let right = glyphs
                .get(position + 1)
                .map_or(text.rect.width, |next| next.x);
            self.placed.push(Placed {
                cluster: *cluster,
                run,
                rect: Rect {
                    x: left + glyph.x,
                    y: y + text.rect.y,
                    width: (right - glyph.x).max(0.0),
                    height: text.rect.height,
                },
            });
        }
    }

    fn rects(&self, start: u32, end: u32) -> Vec<Rect> {
        let first = self.placed.partition_point(|placed| placed.cluster < start);
        let last = self.placed.partition_point(|placed| placed.cluster < end);
        let mut runs: Vec<(u32, Rect)> = Vec::new();
        for placed in self.placed.get(first..last).unwrap_or_default() {
            match runs.iter_mut().find(|(run, _)| *run == placed.run) {
                Some((_, area)) => *area = union(*area, placed.rect),
                None => runs.push((placed.run, placed.rect)),
            }
        }
        runs.sort_by_key(|(run, _)| *run);
        runs.into_iter().map(|(_, rect)| rect).collect()
    }

    pub fn find(&self, query: &str) -> Vec<Vec<Rect>> {
        let (query, _) = fold(query.trim());
        let mut found = Vec::new();
        if query.is_empty() {
            return found;
        }
        let mut position = 0;
        while found.len() < MAX_MATCHES {
            let Some(window) = self.folded.get(position..position + query.len()) else {
                break;
            };
            if window != query.as_slice() {
                position += 1;
                continue;
            }
            let start = self.offsets.get(position).copied().unwrap_or(self.length);
            let after = position + query.len();
            let end = self.offsets.get(after).copied().unwrap_or(self.length);
            let rects = self.rects(start, end);
            if !rects.is_empty() {
                found.push(rects);
            }
            position = after;
        }
        found
    }
}
