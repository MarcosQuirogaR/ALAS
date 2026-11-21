// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The properties of the picked item (right column of the payload page).
//!
//! A zone's kind is a preset: picking one fills density, occupants and
//! length, and a value edited away from it carries a "(modified)" note. A
//! zone or point mass can be flagged Movable with the stations the payload
//! auto-balance may use (`x_range_m`).

use alas_config::{FloorZoneConfig, MassCategory, PayloadItemsConfig, PointMassConfig, ZoneKind};
use egui::{DragValue, Grid, RichText, Ui};

use super::canvas::Target;
use super::placement;
use super::side_view::SideView;
use super::zones;
use crate::views::{tr, tr_fields, tr_label};

const ZONE_KINDS: [(ZoneKind, &str); 8] = [
    (ZoneKind::Seats, "Seats"),
    (ZoneKind::Office, "Office"),
    (ZoneKind::Bedroom, "Bedroom"),
    (ZoneKind::Ops, "Operations"),
    (ZoneKind::Galley, "Galley"),
    (ZoneKind::Lav, "Lavatory"),
    (ZoneKind::Cargo, "Cargo"),
    (ZoneKind::Custom, "Custom"),
];

fn kind_name(kind: ZoneKind) -> &'static str {
    ZONE_KINDS
        .iter()
        .find(|(candidate, _)| *candidate == kind)
        .map_or("Custom", |(_, name)| name)
}

fn modified_note(ui: &mut Ui, shown: bool) {
    if shown {
        ui.label(RichText::new(tr("(modified)")).small().weak().italics());
    }
}

/// Draw the properties of `selected`.
pub(super) fn show(
    ui: &mut Ui,
    items: &mut PayloadItemsConfig,
    view: Option<&SideView>,
    selected: Option<&Target>,
    locked: bool,
) {
    ui.add_enabled_ui(!locked, |ui| match selected {
        Some(Target::Zone(index)) if *index < items.zones.len() => zone(ui, items, *index, view),
        Some(Target::PointMass(index)) if *index < items.point_masses.len() => {
            point_mass(ui, &mut items.point_masses[*index], view);
        }
        _ => {
            ui.label(RichText::new(tr_label("Select An Item To Edit")).weak());
        }
    });
}

/// A label line over a value box; the "(modified)" note of a value edited away
/// from its preset sits at the label's end.
fn label_line(ui: &mut Ui, key: &str, modified: bool) {
    crate::views::form::label_row(ui, tr_label(key), |ui| modified_note(ui, modified));
}

fn metres(ui: &mut Ui, value: &mut f64, speed: f64) -> egui::Response {
    let width = ui.available_width();
    crate::views::form::drag_box(ui, width, DragValue::new(value).speed(speed).suffix(" m"))
}

fn zone(ui: &mut Ui, items: &mut PayloadItemsConfig, index: usize, view: Option<&SideView>) {
    let region = view.map_or([f64::MIN, f64::MAX], |view| view.cabin_x);
    let mut new_kind = None;
    let modified = zones::modified(&items.zones[index]);
    let can_move = placement::zone_can_move(&items.zones[index]);
    let zone = &mut items.zones[index];
    ui.label(RichText::new(&zone.name).strong());
    // The same column grid as the form's fields: equal cells, label over box.
    let mut grid =
        crate::views::form::CellGrid::with_min_column(ui, 150.0, ("payload_zone_props", index));
    grid.cell(ui, |ui, width| {
        crate::views::form::label_row(ui, tr_label("Kind"), |_| {});
        egui::ComboBox::from_id_salt(("zone_kind", index))
            .icon(crate::combo_icon::down_arrow)
            .width(width)
            .selected_text(tr_label(kind_name(zone.kind)))
            .show_ui(ui, |ui| {
                for (kind, name) in ZONE_KINDS {
                    if ui
                        .selectable_label(zone.kind == kind, tr_label(name))
                        .clicked()
                        && zone.kind != kind
                    {
                        new_kind = Some(kind);
                    }
                }
            });
    });
    grid.cell(ui, |ui, _| {
        label_line(ui, "Start", false);
        let mut start = zone.x_start_m;
        if metres(ui, &mut start, 0.1).changed() {
            zones::set_start(zone, start);
        }
    });
    grid.cell(ui, |ui, _| {
        label_line(ui, "End", false);
        metres(ui, &mut zone.x_end_m, 0.1);
    });
    grid.cell(ui, |ui, _| {
        label_line(ui, "Length", modified.length);
        ui.label(format!("{:.1} m", zone.length_m()));
    });
    grid.cell(ui, |ui, _| {
        label_line(ui, "Occupants", modified.occupants);
        let limit = zones::preset(zone.kind)
            .map_or(i64::MAX, |preset| preset.occupant_limit)
            .max(zone.occupants);
        let width = ui.available_width();
        crate::views::form::drag_box(
            ui,
            width,
            DragValue::new(&mut zone.occupants).range(0..=limit),
        );
        if let Some(view) = view.filter(|view| zone.occupants > 0 && view.occupant_kg > 0.0) {
            ui.label(
                RichText::new(tr_fields(
                    "{n} occupants = {kg} kg",
                    &[
                        ("n", zone.occupants.to_string()),
                        (
                            "kg",
                            format!("{:.0}", zone.occupants as f64 * view.occupant_kg),
                        ),
                    ],
                ))
                .small()
                .weak(),
            );
        }
    });
    grid.cell(ui, |ui, _| {
        label_line(ui, "Furnishing", modified.density);
        let width = ui.available_width();
        crate::views::form::drag_box(
            ui,
            width,
            DragValue::new(&mut zone.furnishing_kg_per_m2)
                .speed(0.5)
                .range(0.0..=f64::MAX)
                .suffix(" kg/m\u{b2}"),
        );
        if let Some(view) = view {
            let total = zones::furnishing_kg(view, zone);
            ui.label(
                RichText::new(tr_fields(
                    "= {kg} kg total",
                    &[("kg", format!("{total:.0}"))],
                ))
                .small()
                .weak(),
            );
        }
    });
    grid.cell(ui, |ui, _| {
        label_line(ui, "Lateral Position", false);
        metres(ui, &mut zone.y_center_m, 0.05);
    });
    grid.finish(ui);
    if let Some(kind) = new_kind {
        zones::apply_preset(&mut items.zones, index, kind, region);
    }
    movable_zone(ui, &mut items.zones[index], view, can_move);
}

/// The Movable flag of a zone and the stations the auto-balance may slide it
/// over. A furnished or seat zone is fixed: only occupants move with the
/// balance.
fn movable_zone(ui: &mut Ui, zone: &mut FloorZoneConfig, view: Option<&SideView>, can_move: bool) {
    ui.horizontal(|ui| {
        if can_move {
            let mut movable = zone.movable;
            if crate::checkbox::checkbox(ui, &mut movable, tr_label("Movable")).changed() {
                if let Some(view) = view {
                    placement::set_zone_movable(zone, view, movable);
                } else {
                    zone.movable = movable;
                }
            }
        } else {
            let reason = if zone.kind == ZoneKind::Seats {
                "Seats Are Fixed"
            } else {
                "Furnishing Is Fixed"
            };
            ui.label(tr_label("Movable"));
            crate::lock_icon::lock_icon(ui, &tr_label(reason));
        }
    });
    if zone.movable {
        range_row(ui, &mut zone.x_range_m);
    }
}

fn range_row(ui: &mut Ui, range: &mut Option<[f64; 2]>) {
    let [mut low, mut high] = range.unwrap_or([0.0, 0.0]);
    ui.horizontal(|ui| {
        ui.label(tr_label("Range"));
        let from = ui.add(DragValue::new(&mut low).speed(0.1).suffix(" m"));
        ui.label("\u{2013}");
        let to = ui.add(DragValue::new(&mut high).speed(0.1).suffix(" m"));
        if from.changed() || to.changed() {
            *range = Some([low, high.max(low)]);
        }
    });
}

fn point_mass(ui: &mut Ui, mass: &mut PointMassConfig, view: Option<&SideView>) {
    ui.label(RichText::new(&mass.name).strong());
    Grid::new("payload_point_mass_props")
        .num_columns(2)
        .spacing(crate::theme::space::GRID_SPACING)
        .show(ui, |ui| {
            for (label, value, speed) in [
                ("Station", &mut mass.x_m, 0.1),
                ("Lateral Position", &mut mass.y_m, 0.05),
                ("Height", &mut mass.z_m, 0.05),
            ] {
                ui.label(tr_label(label));
                ui.add(DragValue::new(value).speed(speed).suffix(" m"));
                ui.end_row();
            }
            ui.label(tr_label("Item Mass"));
            ui.add(
                DragValue::new(&mut mass.mass_kg)
                    .speed(10.0)
                    .range(0.0..=f64::MAX)
                    .suffix(" kg"),
            );
            ui.end_row();
            ui.label(tr_label("Category"));
            let equipment = mass.category == MassCategory::Equipment;
            if ui
                .button(tr_label(if equipment { "Equipment" } else { "Payload" }))
                .clicked()
            {
                mass.category = if equipment {
                    MassCategory::Payload
                } else {
                    MassCategory::Equipment
                };
            }
            ui.end_row();
            ui.label(tr_label("Releasable"));
            crate::checkbox::checkbox(ui, &mut mass.releasable, "");
            ui.end_row();
        });
    ui.horizontal(|ui| {
        if placement::point_mass_can_move(mass) {
            let mut movable = mass.movable;
            if crate::checkbox::checkbox(ui, &mut movable, tr_label("Movable")).changed() {
                if let Some(view) = view {
                    placement::set_point_mass_movable(mass, view, movable);
                } else {
                    mass.movable = movable;
                }
            }
        } else {
            ui.label(tr_label("Movable"));
            crate::lock_icon::lock_icon(ui, &tr_label("Equipment Is Fixed"));
        }
    });
    if mass.movable {
        range_row(ui, &mut mass.x_range_m);
    }
}
