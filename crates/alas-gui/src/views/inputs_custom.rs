// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Custom geometry editor shown on the Inputs page.
//!
//! Custom airports are not here: entering one is a form, so it lives in the
//! detached editor reached from the Route selectors
//! ([`crate::views::airport_window`]) rather than in a card the page carries
//! for every design.

use alas_config::{DesignVector, FuselageSection, WingSection};
use egui::{CollapsingHeader, DragValue, RichText, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::views::tr;

fn card(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) -> egui::Response {
    crate::theme::card_frame(ui)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            CollapsingHeader::new(
                RichText::new(tr(title))
                    .strong()
                    .size(16.0)
                    .color(ui.visuals().hyperlink_color),
            )
            .default_open(true)
            .show(ui, body);
        })
        .response
}

/// Render editable arbitrary wing and fuselage loft stations.
pub(crate) fn show_custom_geometry_card(state: &mut AppState, ui: &mut Ui) {
    let _ = card(ui, "Custom geometry", |ui| {
        let locked = !state.active_preset.is_empty();
        ui.label(
            RichText::new(tr(
                "Add arbitrary wing and fuselage stations to a clean-sheet/custom geometry. Preset geometry is locked; selecting a preset never silently applies these edits.",
            ))
            .weak()
            .small(),
        );
        if locked {
            ui.label(
                RichText::new(tr(
                    "Custom geometry editing is disabled while a registered preset is active.",
                ))
                .color(ui.visuals().warn_fg_color),
            );
            return;
        }
        show_custom_wing_sections(state, ui);
        ui.separator();
        show_custom_fuselage_sections(state, ui);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("Custom airfoil .dat path"));
            ui.text_edit_singleline(&mut state.custom_airfoil_file_path);
            if ui.button(tr("Import airfoil .dat")).clicked() {
                state.import_custom_airfoil();
            }
        });
        if let Some(status) = &state.custom_airfoil_status {
            ui.label(RichText::new(status).weak().small());
        }
        let names = alas_geom::airfoil_library::AirfoilLibrary::get_available_airfoils();
        ui.label(format!("{}: {}", tr("Available airfoils"), names.len()));
    });
}

fn show_custom_wing_sections(state: &mut AppState, ui: &mut Ui) {
    ui.label(RichText::new(tr("Wing stations")).strong());
    let mut sections = state
        .config_values
        .get("geometry")
        .and_then(|value| value.get("wing"))
        .and_then(|value| value.get("custom_sections"))
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<WingSection>>(value).ok())
        .unwrap_or_default();
    let mut changed = false;
    let mut remove = None;
    for (index, section) in sections.iter_mut().enumerate() {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{} {}", tr("Section"), index + 1));
            changed |= ui
                .add(
                    DragValue::new(&mut section.span_fraction)
                        .range(0.001..=0.999)
                        .speed(0.01),
                )
                .changed();
            changed |= ui
                .add(DragValue::new(&mut section.leading_edge_x_m).speed(0.05))
                .changed();
            changed |= ui
                .add(
                    DragValue::new(&mut section.chord_m)
                        .range(0.01..=100.0)
                        .speed(0.05),
                )
                .changed();
            changed |= ui
                .add(DragValue::new(&mut section.z_m).speed(0.05))
                .changed();
            changed |= ui
                .add(DragValue::new(&mut section.twist_deg).speed(0.1))
                .changed();
            changed |= ui.text_edit_singleline(&mut section.airfoil).changed();
            if ui.small_button(tr("Remove")).clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        sections.remove(index);
        changed = true;
    }
    if ui.small_button(tr("Add wing section")).clicked() {
        if let Some(span_fraction) = next_wing_span(&sections) {
            sections.push(default_wing_section(state, span_fraction));
            changed = true;
        }
    }
    if changed {
        set_custom_sections(
            state,
            "wing",
            serde_json::to_value(sections).unwrap_or(Value::Array(Vec::new())),
        );
        state.on_config_modified();
    }
    ui.label(
        RichText::new(tr(
            "Columns: span fraction, leading-edge X [m], chord [m], Z [m], twist [deg], airfoil name.",
        ))
        .weak()
        .small(),
    );
}

fn show_custom_fuselage_sections(state: &mut AppState, ui: &mut Ui) {
    ui.label(RichText::new(tr("Fuselage stations")).strong());
    let mut sections = state
        .config_values
        .get("geometry")
        .and_then(|value| value.get("fuselage"))
        .and_then(|value| value.get("custom_sections"))
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<FuselageSection>>(value).ok())
        .unwrap_or_default();
    let mut changed = false;
    let mut remove = None;
    for (index, section) in sections.iter_mut().enumerate() {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{} {}", tr("Section"), index + 1));
            changed |= ui
                .add(
                    DragValue::new(&mut section.x_fraction)
                        .range(0.001..=0.999)
                        .speed(0.01),
                )
                .changed();
            changed |= ui
                .add(
                    DragValue::new(&mut section.width_m)
                        .range(0.01..=100.0)
                        .speed(0.05),
                )
                .changed();
            changed |= ui
                .add(
                    DragValue::new(&mut section.height_m)
                        .range(0.01..=100.0)
                        .speed(0.05),
                )
                .changed();
            changed |= ui
                .add(DragValue::new(&mut section.z_m).speed(0.05))
                .changed();
            changed |= ui
                .add(
                    DragValue::new(&mut section.shape)
                        .range(1.0..=50.0)
                        .speed(0.1),
                )
                .changed();
            if ui.small_button(tr("Remove")).clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        sections.remove(index);
        changed = true;
    }
    if ui.small_button(tr("Add fuselage section")).clicked() {
        sections.push(FuselageSection {
            x_fraction: 0.50,
            width_m: 6.0,
            height_m: 6.0,
            z_m: 0.0,
            shape: 2.0,
        });
        changed = true;
    }
    if changed {
        set_custom_sections(
            state,
            "fuselage",
            serde_json::to_value(sections).unwrap_or(Value::Array(Vec::new())),
        );
        state.on_config_modified();
    }
    ui.label(
        RichText::new(tr(
            "Columns: X fraction, width [m], height [m], Z [m], superellipse shape.",
        ))
        .weak()
        .small(),
    );
}

fn set_custom_sections(state: &mut AppState, surface: &str, value: Value) {
    if let Some(geometry) = state.config_values.get_mut("geometry") {
        if let Some(surface) = geometry.get_mut(surface) {
            surface["custom_sections"] = value;
        }
    }
}

fn next_wing_span(sections: &[WingSection]) -> Option<f64> {
    let previous = sections.last().map_or(0.0, |section| section.span_fraction);
    [0.20, 0.40, 0.60, 0.80, 0.90]
        .into_iter()
        .find(|candidate| *candidate > previous + 1.0e-9)
}

fn default_wing_section(state: &AppState, span_fraction: f64) -> WingSection {
    let defaults = DesignVector::default();
    let root_chord = state
        .design_values
        .get("root_chord_m")
        .copied()
        .unwrap_or(defaults.root_chord_m);
    let break_chord = state
        .design_values
        .get("break_chord_m")
        .copied()
        .unwrap_or(defaults.break_chord_m);
    let tip_chord = state
        .design_values
        .get("tip_chord_m")
        .copied()
        .unwrap_or(defaults.tip_chord_m);
    let break_fraction = state
        .config_values
        .pointer("/geometry/wing/kink_span_fraction")
        .and_then(Value::as_f64)
        .or_else(|| {
            state
                .config_values
                .pointer("/geometry/wing/break_span_fraction")
                .and_then(Value::as_f64)
        })
        .unwrap_or(0.37)
        .clamp(0.01, 0.99);
    let chord_m = if span_fraction <= break_fraction {
        root_chord + span_fraction / break_fraction * (break_chord - root_chord)
    } else {
        break_chord
            + (span_fraction - break_fraction) / (1.0 - break_fraction) * (tip_chord - break_chord)
    };
    WingSection {
        span_fraction,
        leading_edge_x_m: 0.0,
        chord_m,
        z_m: 0.0,
        twist_deg: 0.0,
        airfoil: "naca2410".to_owned(),
    }
}
