// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Sibling cards of a page: all on the one page grid, whose column count comes
//! from the available width only (never from a card's field count), so value
//! boxes have one width in every card and line up vertically.

use std::hash::Hash;

use alas_config::{Entry, Field};
use egui::{Id, Ui};
use serde_json::Value;

use super::layout::is_plain_bool;
use super::visibility::is_visible;

fn row_id() -> Id {
    Id::new("alas_form_pack_row")
}

/// Longest run of fields one grid chunk lays out (checkbox pairs are chunks
/// of their own, as in `render_leaf_run`).
fn longest_chunk(fields: &[&Field]) -> usize {
    let (mut start, mut longest) = (0, 0);
    while start < fields.len() {
        let bools = fields[start..]
            .iter()
            .take_while(|field| is_plain_bool(field))
            .count();
        let end = if bools >= 2 {
            start + bools
        } else {
            let first = fields[start..]
                .iter()
                .position(is_plain_bool)
                .map_or(fields.len(), |at| start + at);
            let mut end = first.max(start + 1);
            while end < fields.len() {
                if is_plain_bool(&fields[end]) && fields.get(end + 1).is_some_and(is_plain_bool) {
                    break;
                }
                end += 1;
            }
            end
        };
        longest = longest.max(end - start);
        start = end;
    }
    longest
}

/// Columns a card of `fields` needs, or 0 when it holds nested groups (and so
/// is laid out on the page grid). A card that is one group is its children.
pub(crate) fn leaf_columns(fields: &[Field], values: &Value) -> usize {
    let null = Value::Null;
    let (list, scope): (&[Field], &Value) = match fields {
        [only] => match &only.entry {
            Entry::Node(node) => (&node.fields, values.get(only.name).unwrap_or(&null)),
            Entry::Leaf(_) => (fields, values),
        },
        _ => (fields, values),
    };
    if list.iter().any(|f| matches!(f.entry, Entry::Node(_))) {
        return 0;
    }
    let shown: Vec<&Field> = list
        .iter()
        .filter(|field| is_visible(field, scope, &[values]))
        .collect();
    longest_chunk(&shown)
}

/// [`leaf_columns`] of one group field among a form's fields.
pub(super) fn node_columns(field: &Field, values: &Value) -> usize {
    let Entry::Node(node) = &field.entry else {
        return 0;
    };
    let null = Value::Null;
    let scope = values.get(field.name).unwrap_or(&null);
    if node
        .fields
        .iter()
        .any(|f| matches!(f.entry, Entry::Node(_)))
    {
        return 0;
    }
    let shown: Vec<&Field> = node
        .fields
        .iter()
        .filter(|f| !f.advanced && is_visible(f, scope, &[values]))
        .collect();
    longest_chunk(&shown)
}

/// Draw the cards `0..cols.len()` through `draw`, one under another. Every card
/// is laid out on the page grid (its column count comes from the width only),
/// so sibling cards share one column width and their boxes line up; a short
/// card leaves its trailing cells empty.
pub(crate) fn flow_cards(
    ui: &mut Ui,
    _id: impl Hash,
    cols: &[usize],
    mut draw: impl FnMut(&mut Ui, usize),
) {
    for k in 0..cols.len() {
        draw(ui, k);
    }
}

/// Called at the end of a card's content: in a flowed row, pad it to the
/// height of the tallest card of the row (as measured in the last frame).
pub(crate) fn equalise(ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let Some(row) = ctx.data(|data| data.get_temp::<Id>(row_id())) else {
        return;
    };
    let height = ui.min_rect().height();
    let target = ctx.data_mut(|data| {
        let cur = data.get_temp_mut_or_insert_with(row.with("cur"), || 0.0_f32);
        *cur = cur.max(height);
        data.get_temp::<f32>(row.with("target")).unwrap_or(0.0)
    });
    if target > height + 0.5 {
        // Reach the row's height exactly: spacing the cursor adds would not count.
        ui.expand_to_include_y(ui.min_rect().top() + target);
    }
}

/// Draw `body`, in a card of its own when `boxed`: the loose fields of a page
/// whose other fields are cards.
pub(super) fn boxed_run<R>(ui: &mut Ui, boxed: bool, body: impl FnOnce(&mut Ui) -> R) -> R {
    if !boxed {
        return body(ui);
    }
    crate::theme::card_frame(ui)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let title = crate::theme::card_title(ui, crate::views::tr("Additional settings"));
            crate::theme::header_label(ui, title);
            body(ui)
        })
        .inner
}
