// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The item list of the payload page: the declared zones and point masses,
//! two lines each (name and delete; mass, position and state chips), to pick
//! the item whose properties the editor beside it shows. With no items the
//! list is one hint line. A last line says what the cabin layout fixes.

use alas_config::PayloadItemsConfig;
use egui::{RichText, Rounding, ScrollArea, Stroke, Ui};

use super::canvas::Target;
use crate::views::{tr_fields, tr_label};

const LIST_HEIGHT: f32 = 320.0;

/// What the list asked for this frame.
#[derive(Debug, Default)]
pub(super) struct Out {
    /// The row whose delete button was pressed.
    pub remove: Option<Target>,
}

/// Draw the list. `locked` freezes the delete buttons (Lock Layout).
pub(super) fn show(
    ui: &mut Ui,
    items: &PayloadItemsConfig,
    selected: &mut Option<Target>,
    released_at: &std::collections::BTreeMap<String, usize>,
    locked: bool,
) -> Out {
    let mut out = Out::default();
    if items.zones.is_empty() && items.point_masses.is_empty() {
        // One place below the canvas, wrapping within the card.
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr_label("No items yet. Add a zone or a point mass.")).weak());
            fixed_row(ui);
        });
        return out;
    }
    ScrollArea::vertical()
        .id_salt("payload_item_list")
        .max_height(LIST_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if !items.zones.is_empty() {
                ui.label(RichText::new(tr_label("Zones")).strong());
                for (index, zone) in items.zones.iter().enumerate() {
                    let detail = tr_fields(
                        "{from} to {to} m",
                        &[
                            ("from", format!("{:.1}", zone.x_start_m)),
                            ("to", format!("{:.1}", zone.x_end_m)),
                        ],
                    );
                    let chips = [chip_state(zone.movable)];
                    row(
                        ui,
                        selected,
                        &mut out.remove,
                        Target::Zone(index),
                        &zone.name,
                        &detail,
                        &chips,
                        locked,
                    );
                }
            }
            if !items.point_masses.is_empty() {
                ui.label(RichText::new(tr_label("Point Masses")).strong());
                for (index, mass) in items.point_masses.iter().enumerate() {
                    let detail = format!(
                        "{:.0} kg \u{b7} x {:.1} y {:.1} z {:.1} m",
                        mass.mass_kg, mass.x_m, mass.y_m, mass.z_m
                    );
                    let mut chips = vec![chip_state(mass.movable)];
                    // A Payload Release stage of the mission drops this item.
                    if let Some(stage) = released_at.get(&mass.id) {
                        chips.push(tr_fields(
                            "Released at stage {n}",
                            &[("n", stage.to_string())],
                        ));
                    }
                    row(
                        ui,
                        selected,
                        &mut out.remove,
                        Target::PointMass(index),
                        &mass.name,
                        &detail,
                        &chips,
                        locked,
                    );
                }
            }
        });
    fixed_row(ui);
    out
}

fn chip_state(movable: bool) -> String {
    tr_label(if movable { "Movable" } else { "Locked" })
}

// reason: the arguments are the independent inputs of one draw call; a struct would only rename them.
#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut Ui,
    selected: &mut Option<Target>,
    remove: &mut Option<Target>,
    target: Target,
    name: &str,
    detail: &str,
    chips: &[String],
    locked: bool,
) {
    let picked = selected.as_ref() == Some(&target);
    let frame = egui::Frame::none()
        .rounding(Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(6.0, 3.0))
        .fill(if picked {
            ui.visuals().selection.bg_fill.gamma_multiply(0.35)
        } else {
            egui::Color32::TRANSPARENT
        });
    frame.show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            if ui
                .selectable_label(picked, RichText::new(name).strong())
                .clicked()
            {
                *selected = Some(target.clone());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(!locked, |ui| {
                    if crate::theme::close_icon_button(ui, tr_label("Delete Item")).clicked() {
                        *remove = Some(target.clone());
                    }
                });
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(detail).weak().small());
            for chip in chips {
                egui::Frame::none()
                    .stroke(Stroke::new(1.0_f32, ui.visuals().weak_text_color()))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::symmetric(6.0, 0.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new(chip).small());
                    });
            }
        });
    });
}

/// What the cabin layout fixes: seats and fixed masses are drawn on the view
/// but are not items of this list.
fn fixed_row(ui: &mut Ui) {
    // A margin of the card's padding, so the wrapped note never reaches its border.
    let room = (ui
        .available_width()
        .min(ui.clip_rect().right() - ui.cursor().left())
        - 24.0)
        .max(120.0);
    ui.scope(|ui| {
        ui.set_max_width(room);
        ui.horizontal_wrapped(|ui| {
            crate::lock_icon::lock_icon(
                ui,
                &tr_label("Seats and fixed masses come from the cabin layout"),
            );
        });
    });
}
