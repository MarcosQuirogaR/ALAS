// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The stage list of the Mission page's editor: the rows with their drag
//! handles and dashed event rows, the drag of a stage to a new slot, and the
//! editor of the selected stage and its events.
//!
//! A drag floats a copy of the stage's card under the cursor on the tooltip
//! layer, dims the card's own slot with a dashed outline, and draws an
//! insertion line at the slot the stage will take on release. The line is
//! drawn in the warning colour when the plan refuses that order.

use alas_config::{EventAction, MissionPlan};
use egui::{
    pos2, vec2, Align, Align2, CursorIcon, FontId, Id, Key, LayerId, Layout, Order, Rect, RichText,
    Rounding, Sense, Stroke, Ui,
};

use super::fields::ReleaseContext;
use super::plot_axis::phase_name;
use super::plot_hover;
use super::release::{plan_stores, released_mass_kg, PayloadItem};
use super::{fields, model, say, EditorUi};
use crate::views::{tr_fields, tr_label};

/// The text of a stage's row.
pub(super) fn row_text(plan: &MissionPlan, index: usize) -> String {
    let stage = &plan.stages[index];
    let summary = model::summary(&stage.kind);
    // A cruise flown to the end of the route reads "Cruise, rest of route",
    // not a bare "Cruise  Route".
    let summary = if matches!(
        stage.kind,
        alas_config::StageKind::Cruise {
            extent: alas_config::CruiseExtent::Remainder,
            ..
        }
    ) {
        crate::views::tr("rest of route")
    } else {
        summary
    };
    let text = if summary.trim().is_empty() {
        format!("{}. {}", index + 1, tr_label(stage.kind.label()))
    } else {
        format!(
            "{}. {} - {}",
            index + 1,
            tr_label(stage.kind.label()),
            summary
        )
    };
    if stage.reserve {
        format!("{text}  ({})", tr_label("Reserve"))
    } else {
        text
    }
}

/// Width one column of the standard mission's numbered list needs, pt.
const COLUMN_WIDTH: f32 = 280.0;
/// Space between two rows of the legend, pt.
const ROW_GAP: f32 = 2.0;

/// Pitch of one row of the legend, pt: the height a selectable row takes, plus the gap.
pub(super) fn row_pitch(ui: &Ui) -> f32 {
    ui.spacing().interact_size.y + ROW_GAP
}

/// How many columns the standard mission's numbered list takes in `width` pt: as many as fit,
/// at most four, so the rows split evenly instead of one long column beside empty space.
fn column_count(width: f32, rows: usize) -> usize {
    ((width / COLUMN_WIDTH).floor() as usize)
        .clamp(1, 4)
        .min(rows.max(1))
}

/// The rows of the longest column of the list of `rows` entries in `width` pt.
fn column_rows(width: f32, rows: usize) -> usize {
    rows.div_ceil(column_count(width, rows))
}

/// Height the stage list takes under the plot: the balanced columns of the standard mission, or
/// the one editable column of a plan with stages.
pub(super) fn rows_height(plan: &MissionPlan, rows: usize, width: f32, pitch: f32) -> f32 {
    let lines = if plan.has_stages() {
        rows
    } else {
        column_rows(width, rows)
    };
    lines as f32 * pitch + 10.0
}

/// An entry of the numbered list under the plot (the legend of its markers): "k -> k+1  name".
pub(super) fn legend_text(row: &str, index: usize) -> String {
    format!(
        "{} \u{2192} {}  {}",
        index + 1,
        index + 2,
        phase_name(row, index)
    )
}

/// The stage list beside the plot. A plan with stages shows the editable list;
/// the standard mission shows the stages it flies in balanced columns, and a
/// click on one opens that phase's editor (returned as the stage's index).
pub(super) fn stages_column(
    ui: &mut Ui,
    plan: &mut MissionPlan,
    flown: &[String],
    tips: &[String],
    editor: &mut EditorUi,
) -> Option<usize> {
    if plan.has_stages() {
        stage_list(ui, plan, editor);
        return None;
    }
    let mut clicked = None;
    let count = column_count(ui.available_width(), flown.len());
    let (base, extra) = (flown.len() / count, flown.len() % count);
    ui.columns(count, |columns| {
        for (column, ui) in columns.iter_mut().enumerate() {
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            // One line per entry: a long name is cut with an ellipsis (tooltip has it).
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            // Balanced: the first `extra` columns take one row more (13 rows in four columns: 4, 3, 3, 3).
            let first = column * base + column.min(extra);
            let last = first + base + usize::from(column < extra);
            for (index, entry) in flown.iter().enumerate().take(last).skip(first) {
                let response = ui.selectable_label(false, legend_text(entry, index));
                plot_hover::set_list(ui.ctx(), index, response.hovered());
                if plot_hover::hovered(ui.ctx()) == Some(index) {
                    ui.painter().rect_stroke(
                        response.rect,
                        Rounding::same(3.0),
                        Stroke::new(1.5_f32, ui.visuals().hyperlink_color),
                    );
                }
                let response = match tips.get(index) {
                    Some(tip) => response.on_hover_text(tip),
                    None => response.on_hover_text(legend_text(entry, index)),
                };
                if response.clicked() {
                    clicked = Some(index);
                }
            }
        }
    });
    clicked
}

pub(super) fn stage_list(ui: &mut Ui, plan: &mut MissionPlan, editor: &mut EditorUi) {
    let mut rects: Vec<Rect> = Vec::with_capacity(plan.stages.len());
    // Vertical extent of each stage with its event rows: where an insertion
    // line goes.
    let mut blocks: Vec<(f32, f32)> = Vec::with_capacity(plan.stages.len());
    let gap = ui.spacing().item_spacing.y;
    let mut remove = None;
    let mut moved: Option<(usize, usize)> = None;
    let mut dropped: Option<(usize, f32)> = None;
    for index in 0..plan.stages.len() {
        let selected = editor.selected.as_deref() == Some(plan.stages[index].id.as_str());
        let top = ui.cursor().top();
        let row = ui.horizontal(|ui| {
            let handle = handle(ui);
            if ui
                .selectable_label(selected, row_text(plan, index))
                .clicked()
            {
                editor.selected = Some(plan.stages[index].id.clone());
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if crate::theme::close_icon_button(ui, tr_label("Delete Stage")).clicked() {
                    remove = Some(index);
                }
            });
            handle
        });
        let handle = row.inner;
        rects.push(row.response.rect);
        if handle.drag_started() {
            editor.dragging = Some(index);
            editor.drag_grab = ui
                .input(|input| input.pointer.interact_pos())
                .map_or(0.0, |pointer| pointer.y - row.response.rect.top());
        }
        if handle.has_focus() {
            let (up, down) = ui.input(|input| {
                (
                    input.key_pressed(Key::ArrowUp),
                    input.key_pressed(Key::ArrowDown),
                )
            });
            if up && index > 0 {
                moved = Some((index, index - 1));
            } else if down && index + 1 < plan.stages.len() {
                moved = Some((index, index + 1));
            }
        }
        if handle.drag_stopped() {
            if let (Some(from), Some(pointer)) = (
                editor.dragging.take(),
                ui.input(|input| input.pointer.interact_pos()),
            ) {
                dropped = Some((from, pointer.y));
            }
        }
        for event in &plan.stages[index].events {
            event_row(ui, model::event_label(&event.action));
        }
        blocks.push((top, (ui.cursor().top() - gap).max(top)));
    }
    if let Some(from) = editor.dragging {
        if ui.input(|input| input.key_pressed(Key::Escape)) {
            editor.dragging = None;
        } else {
            drag_overlay(ui, plan, editor, from, &rects, &blocks, gap);
        }
    }
    if let Some((from, pointer_y)) = dropped {
        moved = Some((from, drop_target(&rects, from, pointer_y)));
    }
    if let Some(index) = remove {
        let id = plan.stages.remove(index).id;
        if editor.selected.as_deref() == Some(id.as_str()) {
            editor.selected = None;
        }
    }
    if let Some((from, to)) = moved.filter(|(from, to)| from != to) {
        if let Err(error) = plan.move_stage(from, to) {
            say(editor, ui, error.to_string(), false);
        }
    }
}

/// What a drag in progress shows: the dimmed, dashed slot of the stage, the
/// insertion line at the slot it will take, and its card under the cursor.
fn drag_overlay(
    ui: &Ui,
    plan: &MissionPlan,
    editor: &EditorUi,
    from: usize,
    rects: &[Rect],
    blocks: &[(f32, f32)],
    gap: f32,
) {
    let (Some(pointer), Some(row), Some(&(top, bottom))) = (
        ui.input(|input| input.pointer.latest_pos()),
        rects.get(from),
        blocks.get(from),
    ) else {
        return;
    };
    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    let accent = ui.visuals().hyperlink_color;
    let slot = Rect::from_x_y_ranges(row.x_range(), top..=bottom).expand2(vec2(2.0, 1.0));
    let painter = ui.painter();

    // The original slot: dimmed, with a dashed outline.
    painter.rect_filled(
        slot,
        Rounding::same(3.0),
        ui.visuals().panel_fill.gamma_multiply(0.75),
    );
    painter.add(egui::Shape::dashed_line(
        &[
            slot.left_top(),
            slot.right_top(),
            slot.right_bottom(),
            slot.left_bottom(),
            slot.left_top(),
        ],
        Stroke::new(1.0_f32, ui.visuals().weak_text_color()),
        4.0,
        3.0,
    ));

    // The insertion line at the slot the stage takes on release.
    let to = drop_target(rects, from, pointer.y);
    if to != from {
        let y = if to < from {
            blocks[to].0 - gap / 2.0
        } else {
            blocks[to].1 + gap / 2.0
        };
        let refused = plan.clone().move_stage(from, to).is_err();
        let color = if refused {
            ui.visuals().warn_fg_color
        } else {
            accent
        };
        painter.line_segment(
            [pos2(row.left() - 2.0, y), pos2(row.right() + 2.0, y)],
            Stroke::new(2.5_f32, color),
        );
        painter.circle_filled(pos2(row.left() - 2.0, y), 3.5, color);
    }

    // The card under the cursor, on the top layer, kept inside the list.
    let list_top = blocks.first().map_or(row.top(), |block| block.0);
    let list_bottom = blocks.last().map_or(row.bottom(), |block| block.1);
    let height = row.height() + 6.0;
    let y =
        (pointer.y - editor.drag_grab - 3.0).clamp(list_top, (list_bottom - height).max(list_top));
    let card = Rect::from_min_size(pos2(row.left() - 3.0, y), vec2(row.width() + 6.0, height));
    let layer = ui.ctx().layer_painter(LayerId::new(
        Order::Tooltip,
        Id::new("mission_stage_drag_card"),
    ));
    layer.rect(
        card.translate(vec2(2.0, 3.0)),
        Rounding::same(4.0),
        egui::Color32::from_black_alpha(70),
        Stroke::NONE,
    );
    layer.rect(
        card,
        Rounding::same(4.0),
        ui.visuals().window_fill,
        Stroke::new(1.5_f32, accent),
    );
    layer.text(
        pos2(card.left() + 24.0, card.center().y),
        Align2::LEFT_CENTER,
        row_text(plan, from),
        FontId::proportional(14.0),
        ui.visuals().text_color(),
    );
    for offset in [-4.0, 0.0, 4.0] {
        layer.line_segment(
            [
                pos2(card.left() + 6.0, card.center().y + offset),
                pos2(card.left() + 18.0, card.center().y + offset),
            ],
            Stroke::new(1.5_f32, ui.visuals().text_color()),
        );
    }
}

/// The index stage `from` takes when released at height `pointer_y` over
/// the stage rows `rects`: the number of rows whose centre is above the
/// pointer, less one when the stage itself is among them.
pub(super) fn drop_target(rects: &[Rect], from: usize, pointer_y: f32) -> usize {
    let above = rects
        .iter()
        .filter(|rect| rect.center().y < pointer_y)
        .count();
    let slot = if above > from { above - 1 } else { above };
    slot.min(rects.len().saturating_sub(1))
}

/// The drag handle: three bars, focusable for Up and Down.
fn handle(ui: &mut Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(16.0, 18.0), Sense::click_and_drag());
    let color = if response.hovered() || response.has_focus() {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    for offset in [-4.0, 0.0, 4.0] {
        let y = rect.center().y + offset;
        ui.painter().line_segment(
            [
                egui::pos2(rect.left() + 3.0, y),
                egui::pos2(rect.right() - 3.0, y),
            ],
            Stroke::new(1.5_f32, color),
        );
    }
    if response.clicked() {
        response.request_focus();
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }
    response.on_hover_text(tr_label("Drag To Reorder"))
}

/// A dashed row for an event, under the stage that owns it.
fn event_row(ui: &mut Ui, label: &str) {
    let response = ui.horizontal(|ui| {
        ui.add_space(34.0);
        ui.label(
            RichText::new(format!("\u{25c7} {}", tr_label(label)))
                .small()
                .color(ui.visuals().weak_text_color()),
        );
    });
    let rect = response.response.rect.expand2(vec2(4.0, 0.0));
    let stroke = Stroke::new(1.0_f32, ui.visuals().weak_text_color());
    ui.painter().add(egui::Shape::dashed_line(
        &[
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
            rect.left_top(),
        ],
        stroke,
        3.0,
        3.0,
    ));
}

/// `(item id, stage label)` of the items that releases other than event
/// `position` of stage `index` already name.
fn releases_elsewhere(plan: &MissionPlan, index: usize, position: usize) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for (stage_index, stage) in plan.stages.iter().enumerate() {
        for (event_index, event) in stage.events.iter().enumerate() {
            if (stage_index, event_index) == (index, position) {
                continue;
            }
            if let EventAction::PayloadRelease { items } = &event.action {
                let label = tr_label(stage.kind.label());
                found.extend(items.iter().map(|id| (id.clone(), label.clone())));
            }
        }
    }
    found
}

/// The editor of the stage at `index`. `items` are the declared point masses
/// a Payload Release may name; the ones a ticked row made releasable are added
/// to `newly_releasable` for the caller to store.
pub(super) fn stage_detail(
    ui: &mut Ui,
    plan: &mut MissionPlan,
    index: usize,
    items: &[PayloadItem],
    newly_releasable: &mut Vec<String>,
    tanks: &dyn Fn() -> Vec<String>,
    reveal: bool,
) {
    let mut add_event: Option<&'static str> = None;
    let mut remove_event = None;
    let elsewhere: Vec<Vec<(String, String)>> = (0..plan.stages[index].events.len())
        .map(|position| releases_elsewhere(plan, index, position))
        .collect();
    let all_items: Vec<PayloadItem> = items.iter().cloned().chain(plan_stores(plan)).collect();
    let card = crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let stage = &mut plan.stages[index];
        let heading = crate::theme::card_title(
            ui,
            tr_fields("{stage} Stage", &[("stage", tr_label(stage.kind.label()))]),
        );
        crate::theme::header_label(ui, heading);
        fields::stage(ui, stage);
        ui.add_space(4.0);
        for (position, event) in stage.events.iter_mut().enumerate() {
            let mut title = tr_label(model::event_label(&event.action));
            if let EventAction::PayloadRelease { items: ids } = &event.action {
                let mass = released_mass_kg(&all_items, ids, &elsewhere[position]);
                title = format!("{title}: {mass:.0} kg");
            }
            let heading = crate::theme::section_title(ui, format!("\u{25c7} {title}"));
            let header = egui::CollapsingHeader::new(heading)
                .id_salt(("mission_event", index, position))
                .default_open(true);
            crate::theme::show_header(ui, header, |ui| {
                let ids = if matches!(event.action, alas_config::EventAction::FuelTransfer { .. }) {
                    tanks()
                } else {
                    Vec::new()
                };
                let mut payload = ReleaseContext {
                    items: &all_items,
                    elsewhere: &elsewhere[position],
                    newly_releasable,
                };
                fields::event(ui, event, &mut payload, &ids);
                if ui.button(tr_label("Delete Event")).clicked() {
                    remove_event = Some(position);
                }
            });
        }
        ui.menu_button(tr_label("Add Event"), |ui| {
            for kind in model::EVENT_KINDS {
                if ui.button(tr_label(kind)).clicked() {
                    add_event = Some(kind);
                    ui.close_menu();
                }
            }
        });
    });
    // A click on the profile brings the stage's editor into view: with the
    // plot filling the page the card sits below the fold, and a click that
    // seems to do nothing reads as an uneditable phase.
    if reveal {
        ui.scroll_to_rect(card.response.rect, Some(egui::Align::Center));
    }
    // The search pulses the card of the stage that flies the searched field.
    if let Some(focus) = crate::views::mission_phase_focus::focused(ui.ctx()) {
        let kind = plan.stages[index].kind.label();
        if kind == crate::views::mission_phase_focus::stage_label_of_phase(focus.phase) {
            crate::search::focus_anchor(ui, &focus.path, card.response.rect);
        }
    }
    let stage = &mut plan.stages[index];
    if let Some(kind) = add_event {
        if let Some(mut event) = model::new_event(kind) {
            if let alas_config::EventAction::FuelTransfer { from, to, .. } = &mut event.action {
                let ids = tanks();
                *from = ids.first().cloned().unwrap_or_default();
                *to = ids.get(1).cloned().unwrap_or_default();
            }
            stage.events.push(event);
        }
    }
    if let Some(position) = remove_event {
        stage.events.remove(position);
    }
}
