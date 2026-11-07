use web_style::{Color, VerticalAlign};

use crate::box_tree::{BlockBox, Contents, Table, TableCell};
use crate::collapse::Collapsed;
use crate::flow::{Bfc, Flow, Sizing};
use crate::fragment::{BoxFragment, Extra, Fragment, Layer, Rect};

pub(crate) struct Slot<'b> {
    pub(crate) row: usize,
    pub(crate) column: usize,
    pub(crate) rows: usize,
    pub(crate) columns: usize,
    pub(crate) cell: &'b TableCell,
}

pub(crate) fn grid(table: &Table) -> (Vec<Slot<'_>>, usize) {
    let row_count = table.rows.len();
    let mut occupied: Vec<Vec<bool>> = vec![Vec::new(); row_count];
    let mut slots = Vec::new();
    let mut columns = 0;
    for (row_index, row) in table.rows.iter().enumerate() {
        let mut column = 0;
        for cell in &row.cells {
            while occupied
                .get(row_index)
                .and_then(|row| row.get(column))
                .copied()
                .unwrap_or(false)
            {
                column += 1;
            }
            let rows = usize::from(cell.rows).min(row_count - row_index).max(1);
            let span = usize::from(cell.columns).max(1);
            for taken in occupied.iter_mut().skip(row_index).take(rows) {
                if taken.len() < column + span {
                    taken.resize(column + span, false);
                }
                for slot in taken.iter_mut().skip(column).take(span) {
                    *slot = true;
                }
            }
            slots.push(Slot {
                row: row_index,
                column,
                rows,
                columns: span,
                cell,
            });
            column += span;
            columns = columns.max(column);
        }
    }
    (slots, columns)
}

fn spacing(block: &BlockBox) -> (f32, f32) {
    let inherited = &block.style.inherited;
    if inherited.border_collapse {
        (0.0, 0.0)
    } else {
        inherited.border_spacing
    }
}

#[derive(Clone, Copy, Default)]
struct Column {
    min: f32,
    max: f32,
    percent: Option<f32>,
    fixed: bool,
}

fn is_empty_cell(block: &BlockBox) -> bool {
    match &block.contents {
        Contents::Inline(inline) => inline.shaped.is_empty() && inline.out_of_flow.is_empty(),
        Contents::Blocks(children) => children.is_empty(),
        _ => false,
    }
}

fn distribute(columns: &[Column], total: f32) -> Vec<f32> {
    let preferred: Vec<f32> = columns
        .iter()
        .map(|column| match column.percent {
            Some(percent) => column.min.max(percent / 100.0 * total),
            None => column.max.max(column.min),
        })
        .collect();
    let sum: f32 = preferred.iter().sum();
    let mut widths = preferred.clone();
    let tiers = [
        (|column: &Column| column.percent.is_none() && !column.fixed) as fn(&Column) -> bool,
        |column: &Column| column.percent.is_none() && column.fixed,
        |column: &Column| column.percent.is_some(),
    ];
    if sum < total {
        let extra = total - sum;
        for tier in tiers {
            let weight: f32 = columns
                .iter()
                .zip(&preferred)
                .filter(|(column, _)| tier(column))
                .map(|(_, width)| *width)
                .sum();
            let members = columns.iter().filter(|column| tier(column)).count();
            if members == 0 {
                continue;
            }
            for ((column, width), base) in columns.iter().zip(&mut widths).zip(&preferred) {
                if tier(column) {
                    *width += if weight > 0.0 {
                        extra * base / weight
                    } else {
                        extra / members as f32
                    };
                }
            }
            break;
        }
    } else {
        let mut deficit = sum - total;
        for tier in tiers {
            let slack: f32 = columns
                .iter()
                .zip(&preferred)
                .filter(|(column, _)| tier(column))
                .map(|(column, width)| width - column.min)
                .sum();
            if slack <= 0.0 {
                continue;
            }
            let ratio = (deficit / slack).min(1.0);
            for ((column, width), base) in columns.iter().zip(&mut widths).zip(&preferred) {
                if tier(column) {
                    *width = base - (base - column.min) * ratio;
                }
            }
            deficit -= slack * ratio;
            if deficit <= 0.0 {
                break;
            }
        }
    }
    widths
}

impl<'a, 'b> Flow<'a, 'b> {
    fn columns(&mut self, block: &'b BlockBox, table: &'b Table, width: f32) -> Vec<Column> {
        let (slots, count) = grid(table);
        let (horizontal, _) = spacing(block);
        let count = count.max(table.parts.columns.len());
        let mut columns = vec![Column::default(); count];
        for (column, (style, _)) in columns.iter_mut().zip(&table.parts.columns) {
            match style.reset.width {
                Some(value) if value.percent > 0.0 => column.percent = Some(value.percent),
                Some(value) => {
                    column.min = value.px;
                    column.max = value.px;
                    column.fixed = true;
                }
                None => {}
            }
        }
        let mut spanning = Vec::new();
        for slot in &slots {
            let cell = &slot.cell.block;
            let (min, max) = self.intrinsic_content(cell, (width, None));
            let cell_frame = crate::flow::frame(&cell.style, width).0;
            let (mut min, mut max) = (min + cell_frame, max + cell_frame);
            let mut percent = None;
            let mut fixed = false;
            match cell.style.reset.width {
                Some(value) if value.percent > 0.0 => percent = Some(value.percent),
                Some(value) => {
                    let specified = if cell.style.reset.border_box {
                        value.px
                    } else {
                        value.px + cell_frame
                    };
                    min = min.max(cell.style.reset.min_width.px);
                    max = specified.max(min);
                    fixed = true;
                }
                None => {}
            }
            if slot.columns == 1 {
                if let Some(column) = columns.get_mut(slot.column) {
                    column.min = column.min.max(min);
                    if fixed && !column.fixed {
                        column.max = max;
                    } else {
                        column.max = column.max.max(max);
                    }
                    column.max = column.max.max(column.min);
                    column.fixed = column.fixed || fixed;
                    if let Some(percent) = percent {
                        column.percent = Some(column.percent.map_or(percent, |p| p.max(percent)));
                    }
                }
            } else {
                spanning.push((slot.column, slot.columns, min, max, percent));
            }
        }
        spanning.sort_by_key(|span| span.1);
        for (start, span, min, max, percent) in spanning {
            let gaps = horizontal * (span - 1) as f32;
            let range = start..(start + span).min(count);
            let members = columns.get_mut(range).unwrap_or_default();
            let current_min: f32 = members.iter().map(|c| c.min).sum();
            let current_max: f32 = members.iter().map(|c| c.max).sum();
            let extra_min = (min - gaps - current_min).max(0.0);
            let extra_max = (max - gaps - current_max).max(0.0);
            let share = members.len().max(1) as f32;
            let free_percent = percent.map(|percent| {
                let taken: f32 = members.iter().filter_map(|c| c.percent).sum();
                (percent - taken).max(0.0)
            });
            let plain = members.iter().filter(|c| c.percent.is_none()).count();
            for column in members {
                let weight = if current_max > 0.0 {
                    column.max / current_max
                } else {
                    1.0 / share
                };
                column.min += extra_min * weight;
                column.max += extra_max * weight;
                column.max = column.max.max(column.min);
                if let Some(free) = free_percent
                    && column.percent.is_none()
                    && plain > 0
                {
                    column.percent = Some(free / plain as f32);
                }
            }
        }
        columns
    }

    pub(crate) fn table_intrinsic(
        &mut self,
        block: &'b BlockBox,
        table: &'b Table,
        width: f32,
    ) -> (f32, f32) {
        let columns = self.columns(block, table, width);
        let gaps = if columns.is_empty() {
            0.0
        } else {
            spacing(block).0 * (columns.len() + 1) as f32
        };
        let min: f32 = columns.iter().map(|column| column.min).sum::<f32>() + gaps;
        let sum: f32 = columns.iter().map(|column| column.max).sum();
        let percents: f32 = columns.iter().filter_map(|column| column.percent).sum();
        let plain: f32 = columns
            .iter()
            .filter(|column| column.percent.is_none())
            .map(|column| column.max)
            .sum();
        let from_percent = columns
            .iter()
            .filter_map(|column| {
                let percent = column.percent.filter(|percent| *percent > 0.0)?;
                Some(column.max * 100.0 / percent)
            })
            .fold(0.0, f32::max);
        let from_plain = if percents < 100.0 {
            plain * 100.0 / (100.0 - percents)
        } else if plain > 0.0 {
            f32::MAX / 4.0
        } else {
            0.0
        };
        let max = sum.max(from_percent).max(from_plain) + gaps;
        let captions = table
            .captions
            .iter()
            .map(|caption| self.outer_intrinsic(caption, (width, None)).0)
            .fold(0.0, f32::max);
        (min.max(captions), max.max(min).max(captions))
    }

    pub(crate) fn captions(
        &mut self,
        table: &'b Table,
        bottom: bool,
        width: f32,
        origin: (f32, f32),
    ) -> (Vec<Fragment>, f32) {
        let mut y = 0.0;
        let mut fragments = Vec::new();
        for caption in table
            .captions
            .iter()
            .filter(|caption| caption.style.inherited.caption_bottom == bottom)
        {
            let laid = self.block(
                caption,
                (width, None),
                (origin.0, origin.1 + y),
                &mut Bfc::default(),
                Sizing::default(),
            );
            let mut fragment = laid.fragment;
            fragment.rect.y = y + laid.margin.top;
            y += laid.margin.top + fragment.rect.height + laid.margin.bottom;
            fragments.push(Fragment::Box(fragment));
        }
        (fragments, y)
    }

    fn fixed_widths(&mut self, table: &'b Table, total: f32) -> Vec<f32> {
        let (slots, count) = grid(table);
        let count = count.max(table.parts.columns.len());
        let resolve = |value: web_style::LengthPercentage| value.resolve(total);
        let mut widths: Vec<Option<f32>> = table
            .parts
            .columns
            .iter()
            .map(|(style, _)| style.reset.width.map(resolve))
            .collect();
        widths.resize(count, None);
        for slot in slots.iter().filter(|slot| slot.row == 0) {
            let cell = &slot.cell.block;
            let Some(value) = cell.style.reset.width else {
                continue;
            };
            let frame = crate::flow::frame(&cell.style, total).0;
            let outer = if cell.style.reset.border_box {
                resolve(value)
            } else {
                resolve(value) + frame
            };
            let share = outer / slot.columns as f32;
            for width in widths
                .get_mut(slot.column..slot.column + slot.columns)
                .unwrap_or_default()
            {
                width.get_or_insert(share);
            }
        }
        let specified: f32 = widths.iter().flatten().sum();
        let unset = widths.iter().filter(|width| width.is_none()).count();
        if unset > 0 {
            let share = ((total - specified) / unset as f32).max(0.0);
            widths.iter().map(|width| width.unwrap_or(share)).collect()
        } else if specified > 0.0 && specified < total {
            let scale = total / specified;
            widths.iter().flatten().map(|width| width * scale).collect()
        } else {
            widths.iter().flatten().copied().collect()
        }
    }

    pub(crate) fn table(
        &mut self,
        block: &'b BlockBox,
        table: &'b Table,
        size: (f32, Option<f32>),
        origin: (f32, f32),
        offset: (f32, f32),
        children: &mut Vec<Fragment>,
    ) -> (f32, f32) {
        let (width, specified_height) = size;
        let (horizontal, vertical) = spacing(block);
        let (slots, grid_columns) = grid(table);
        let count = grid_columns.max(table.parts.columns.len());
        let gaps = if count == 0 {
            0.0
        } else {
            horizontal * (count + 1) as f32
        };
        let inner = (width - gaps).max(0.0);
        let fixed = block.style.reset.fixed_table && block.style.reset.width.is_some();
        let widths: Vec<f32> = if fixed {
            self.fixed_widths(table, inner)
        } else {
            let columns = self.columns(block, table, width);
            distribute(&columns, inner)
        };
        let grid_width = widths.iter().sum::<f32>() + gaps;
        let rtl = block.style.is_rtl();
        let mut lefts = vec![0.0; widths.len()];
        let mut x = if count == 0 { 0.0 } else { horizontal };
        if rtl {
            for (left, width) in lefts.iter_mut().zip(&widths).rev() {
                *left = x;
                x += width + horizontal;
            }
        } else {
            for (left, width) in lefts.iter_mut().zip(&widths) {
                *left = x;
                x += width + horizontal;
            }
        }
        let span_width = |slot: &Slot<'_>| {
            widths
                .get(slot.column..slot.column + slot.columns)
                .unwrap_or_default()
                .iter()
                .sum::<f32>()
                + horizontal * (slot.columns - 1) as f32
        };
        let span_left = |slot: &Slot<'_>| {
            let last = if rtl {
                slot.column + slot.columns - 1
            } else {
                slot.column
            };
            lefts.get(last).copied().unwrap_or(0.0)
        };
        let mut heights = vec![0.0f32; table.rows.len()];
        for (row, height) in table.rows.iter().zip(heights.iter_mut()) {
            if let Some(specified) = row.style.reset.height {
                *height = match specified_height {
                    Some(total) => specified.resolve(total),
                    None => specified.px,
                };
            }
        }
        let mut measured = Vec::with_capacity(slots.len());
        let mut above = vec![0.0f32; table.rows.len()];
        let mut below = vec![0.0f32; table.rows.len()];
        let mut baselines = Vec::with_capacity(slots.len());
        let mark = self.mark();
        for slot in &slots {
            let cell_width = span_width(slot);
            let cell = &slot.cell.block;
            let laid = self.block(
                cell,
                (cell_width, None),
                origin,
                &mut Bfc::default(),
                Sizing {
                    width: Some(cell_width),
                    auto_height: true,
                    ..Sizing::default()
                },
            );
            let content = laid.fragment.rect.height;
            let reset = &cell.style.reset;
            let cell_frame = crate::flow::frame(&cell.style, cell_width).1;
            let floor = reset
                .height
                .and_then(|height| {
                    (height.percent == 0.0).then(|| {
                        if reset.border_box {
                            height.px
                        } else {
                            height.px + cell_frame
                        }
                    })
                })
                .unwrap_or(0.0);
            let outer = content.max(floor);
            let baseline_cell = matches!(
                reset.vertical_align,
                VerticalAlign::Baseline
                    | VerticalAlign::Sub
                    | VerticalAlign::Super
                    | VerticalAlign::TextTop
                    | VerticalAlign::TextBottom
                    | VerticalAlign::Length(_)
            );
            let baseline = crate::flow::first_baseline(&laid.fragment.children).unwrap_or(content);
            baselines.push(baseline);
            if slot.rows == 1
                && let Some(row) = heights.get_mut(slot.row)
            {
                *row = row.max(outer);
                if baseline_cell {
                    if let Some(row_above) = above.get_mut(slot.row) {
                        *row_above = row_above.max(baseline);
                    }
                    if let Some(row_below) = below.get_mut(slot.row) {
                        *row_below = row_below.max(content - baseline);
                    }
                }
            }
            measured.push((outer, content));
        }
        self.rewind(mark);
        for ((height, row_above), row_below) in heights.iter_mut().zip(&above).zip(&below) {
            *height = height.max(row_above + row_below);
        }
        for (slot, &(height, _)) in slots.iter().zip(&measured) {
            if slot.rows > 1 {
                let range = slot.row..slot.row + slot.rows;
                let current: f32 = heights
                    .get(range.clone())
                    .unwrap_or_default()
                    .iter()
                    .sum::<f32>()
                    + vertical * (slot.rows - 1) as f32;
                if height > current
                    && let Some(last) = heights.get_mut(range.end - 1)
                {
                    *last += height - current;
                }
            }
        }
        let row_gaps = if heights.is_empty() {
            0.0
        } else {
            vertical * (heights.len() + 1) as f32
        };
        if let Some(total) = specified_height {
            let current = heights.iter().sum::<f32>() + row_gaps;
            if total > current && !heights.is_empty() {
                let extra = total - current;
                let sum: f32 = heights.iter().sum();
                let share = heights.len() as f32;
                for height in &mut heights {
                    *height += if sum > 0.0 {
                        extra * *height / sum
                    } else {
                        extra / share
                    };
                }
            }
        }
        let mut tops = Vec::with_capacity(heights.len() + 1);
        let mut top = if heights.is_empty() { 0.0 } else { vertical };
        for height in &heights {
            tops.push(top);
            top += height + vertical;
        }
        let grid_height = if heights.is_empty() { 0.0 } else { top };
        for (index, (slot, &(_, content))) in slots.iter().zip(&measured).enumerate() {
            let cell = &slot.cell.block;
            let cell_width = span_width(slot);
            let cell_height = heights
                .get(slot.row..slot.row + slot.rows)
                .unwrap_or_default()
                .iter()
                .sum::<f32>()
                + vertical * (slot.rows - 1) as f32;
            let hidden = !block.style.inherited.border_collapse
                && cell.style.inherited.hide_empty_cells
                && is_empty_cell(cell);
            let cell_rect = Rect {
                x: offset.0 + span_left(slot),
                y: offset.1 + tops.get(slot.row).copied().unwrap_or(0.0),
                width: cell_width,
                height: cell_height,
            };
            if !hidden {
                self.part_backgrounds(table, slot, cell_rect, children);
            }
            let laid = self.block(
                cell,
                (cell_width, None),
                origin,
                &mut Bfc::default(),
                Sizing {
                    width: Some(cell_width),
                    height: Some(cell_height),
                    ..Sizing::default()
                },
            );
            let row_baseline = above.get(slot.row).copied().unwrap_or(0.0);
            let shift = match cell.style.reset.vertical_align {
                VerticalAlign::Middle => (cell_height - content) / 2.0,
                VerticalAlign::Bottom => cell_height - content,
                VerticalAlign::Top => 0.0,
                _ if slot.rows == 1 => {
                    row_baseline - baselines.get(index).copied().unwrap_or(row_baseline)
                }
                _ => 0.0,
            }
            .max(0.0);
            let mut fragment = laid.fragment;
            if shift > 0.0 {
                for child in &mut fragment.children {
                    match child {
                        Fragment::Box(child) => child.rect.y += shift,
                        Fragment::Text(text) => {
                            text.rect.y += shift;
                            text.baseline += shift;
                        }
                    }
                }
            }
            fragment.rect.x = cell_rect.x;
            fragment.rect.y = cell_rect.y;
            if hidden {
                fragment.background = Color::TRANSPARENT;
                fragment.extra = None;
            }
            children.push(Fragment::Box(fragment));
        }
        if let Some(collapsed) = &table.collapsed {
            let mut xs = lefts.clone();
            xs.push(x);
            tops.push(top);
            edges(collapsed, &xs, &tops, offset, children);
        }
        (grid_height, grid_width)
    }

    fn part_backgrounds(
        &self,
        table: &Table,
        slot: &Slot<'_>,
        rect: Rect,
        children: &mut Vec<Fragment>,
    ) {
        let column = table.parts.columns.get(slot.column);
        let column_group = column
            .and_then(|(_, group)| *group)
            .and_then(|group| table.parts.column_groups.get(group));
        let row = table.rows.get(slot.row);
        let row_group = row
            .and_then(|row| row.group)
            .and_then(|group| table.parts.groups.get(group as usize));
        let row_style = row.filter(|row| row.node.is_some()).map(|row| &row.style);
        for style in [
            column_group,
            column.map(|(style, _)| style),
            row_group,
            row_style,
        ]
        .into_iter()
        .flatten()
        {
            let background = style.reset.background_color;
            let image = style.reset.background_image.clone();
            if background.a == 0 && image.is_none() {
                continue;
            }
            let extra = image.map(|image| {
                Box::new(Extra {
                    border: web_style::Sides::default(),
                    border_color: style.reset.border_color,
                    padding: web_style::Sides::default(),
                    background: style.reset.background,
                    layer: Layer::FLOW,
                    image: None,
                    background_image: Some(image),
                })
            });
            children.push(Fragment::Box(BoxFragment {
                node: None,
                rect,
                background,
                extra,
                children: Vec::new(),
            }));
        }
    }
}

fn edges(
    collapsed: &Collapsed,
    xs: &[f32],
    ys: &[f32],
    origin: (f32, f32),
    children: &mut Vec<Fragment>,
) {
    let mut push = |rect: Rect, color: Color| {
        if rect.width > 0.0 && rect.height > 0.0 && color != Color::TRANSPARENT {
            children.push(Fragment::Box(BoxFragment {
                node: None,
                rect: Rect {
                    x: origin.0 + rect.x,
                    y: origin.1 + rect.y,
                    ..rect
                },
                background: color,
                extra: None,
                children: Vec::new(),
            }));
        }
    };
    let rows = ys.len().saturating_sub(1);
    let columns = collapsed.columns;
    let half = |edge: Option<crate::collapse::Edge>| edge.map_or(0.0, |edge| edge.width / 2.0);
    for row in 0..rows {
        let (Some(&top), Some(&bottom)) = (ys.get(row), ys.get(row + 1)) else {
            continue;
        };
        for column in 0..=columns {
            let (Some(edge), Some(&x)) = (collapsed.vertical(row, column), xs.get(column)) else {
                continue;
            };
            push(
                Rect {
                    x: x - edge.width / 2.0,
                    y: top,
                    width: edge.width,
                    height: bottom - top,
                },
                edge.color,
            );
        }
    }
    for row in 0..=rows {
        let Some(&y) = ys.get(row) else {
            continue;
        };
        for column in 0..columns {
            let (Some(edge), Some(&left), Some(&right)) = (
                collapsed.horizontal(row, column),
                xs.get(column),
                xs.get(column + 1),
            ) else {
                continue;
            };
            let corner = |at: usize| {
                let above = row
                    .checked_sub(1)
                    .and_then(|row| collapsed.vertical(row, at));
                let below = (row < rows).then(|| collapsed.vertical(row, at)).flatten();
                half(above).max(half(below))
            };
            let start = left - corner(column);
            let end = right + corner(column + 1);
            push(
                Rect {
                    x: start,
                    y: y - edge.width / 2.0,
                    width: end - start,
                    height: edge.width,
                },
                edge.color,
            );
        }
    }
}
