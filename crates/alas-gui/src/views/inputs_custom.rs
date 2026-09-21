// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Custom airfoil import and sandbox-only custom geometry editors.
//!
//! Custom airports are not here: entering one is a form, so it lives in the
//! detached editor reached from the Route selectors
//! ([`crate::views::airport_window`]) rather than in a card the page carries
//! for every design.

use alas_config::{DesignVector, FuselageSection, MainWingStationKind, WingSection};
use egui::{DragValue, RichText, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::views::tr;

/// Render the wing station editor inside a Sandbox Discipline Window.
pub(crate) fn show_custom_wing_sections(state: &mut AppState, ui: &mut Ui) {
    ui.label(RichText::new(tr("Wing stations")).strong());
    ui.label(
        RichText::new(tr(
            "NAME | SPAN (%) | LE X [m] | CHORD [m] | Z [m] | TWIST [deg] | AIRFOIL",
        ))
        .weak()
        .small(),
    );
    let mut sections = state
        .config_values
        .get("geometry")
        .and_then(|value| value.get("wing"))
        .and_then(|value| value.get("custom_sections"))
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<WingSection>>(value).ok())
        .unwrap_or_default();
    // Presets carry the root, optional side-of-body, kink and tip as the
    // transport planform itself.  They used to disappear from this editor,
    // which made a new station hard to place coherently and left the user
    // editing only the extra `custom_sections` list.  Keep the generated
    // stations visible and route the editable cells back to their owning
    // scalar/design values below.
    let mut generated = generated_wing_sections(state);
    let generated_before = generated.clone();
    for index in 0..generated.len() {
        let (span_min, span_max) = generated_wing_span_bounds(&generated, index);
        let (kind, section) = &mut generated[index];
        show_generated_wing_section(ui, *kind, section, span_min, span_max);
        if matches!(
            kind,
            MainWingStationKind::SideOfBody | MainWingStationKind::Kink
        ) {
            section.span_fraction = section.span_fraction.clamp(span_min, span_max);
        }
    }
    if !generated.is_empty() {
        ui.separator();
        ui.label(
            RichText::new(tr("Additional sandbox sections"))
                .strong()
                .small(),
        );
    }
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
        });
        if ui.small_button(tr("Remove this wing section")).clicked() {
            remove = Some(index);
        }
    }
    if let Some(index) = remove {
        sections.remove(index);
        changed = true;
    }
    if changed {
        sections.sort_by(|left, right| left.span_fraction.total_cmp(&right.span_fraction));
        set_custom_sections(
            state,
            "wing",
            serde_json::to_value(&sections).unwrap_or(Value::Array(Vec::new())),
        );
        state.on_config_modified();
    }
    if generated != generated_before {
        apply_generated_wing_sections(state, &generated_before, &generated);
        state.on_config_modified();
    }
    ui.horizontal(|ui| {
        if ui
            .add_sized(
                [ui.available_width() * 0.5 - 3.0, 24.0],
                egui::Button::new(tr("Create wing section")),
            )
            .clicked()
        {
            if let Some(span_fraction) = next_wing_span_available(state, &sections) {
                let section = interpolated_wing_section(state, span_fraction, &sections)
                    .unwrap_or_else(|| default_wing_section(state, span_fraction));
                sections.push(section);
                sections.sort_by(|left, right| left.span_fraction.total_cmp(&right.span_fraction));
                set_custom_sections(
                    state,
                    "wing",
                    serde_json::to_value(&sections).unwrap_or(Value::Array(Vec::new())),
                );
                state.on_config_modified();
            }
        }
        if ui
            .add_sized(
                [ui.available_width(), 24.0],
                egui::Button::new(tr("Remove last wing section")),
            )
            .clicked()
        {
            if sections.pop().is_some() {
                set_custom_sections(
                    state,
                    "wing",
                    serde_json::to_value(&sections).unwrap_or(Value::Array(Vec::new())),
                );
                state.on_config_modified();
            }
        }
    });
}

/// Render the fuselage station editor inside a Sandbox Discipline Window.
pub(crate) fn show_custom_fuselage_sections(state: &mut AppState, ui: &mut Ui) {
    ui.label(RichText::new(tr("Fuselage stations")).strong());
    ui.label(
        RichText::new(tr("NAME | X (%) | WIDTH [m] | HEIGHT [m] | Z [m] | SHAPE"))
            .weak()
            .small(),
    );
    let mut sections = state
        .config_values
        .get("geometry")
        .and_then(|value| value.get("fuselage"))
        .and_then(|value| value.get("custom_sections"))
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<FuselageSection>>(value).ok())
        .unwrap_or_default();
    // Show the complete generated nose/cabin/tail station set before the
    // user-added stations.  The generated values follow the same equations
    // as the builder; their owning body parameters remain editable in the
    // fields and the fuselage preview handles.
    let mut generated = generated_fuselage_sections(state);
    let generated_before = generated.clone();
    if !generated.is_empty() {
        ui.label(
            RichText::new(tr(
                "Preset/generated stations (edit Body parameters or drag the preview points)",
            ))
            .weak()
            .small(),
        );
        for (index, section) in generated.iter_mut().enumerate() {
            show_generated_fuselage_section(ui, index, section);
        }
        ui.separator();
        ui.label(
            RichText::new(tr("Additional sandbox sections"))
                .strong()
                .small(),
        );
    }
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
        });
        if ui
            .small_button(tr("Remove this fuselage section"))
            .clicked()
        {
            remove = Some(index);
        }
    }
    if let Some(index) = remove {
        sections.remove(index);
        changed = true;
    }
    if changed {
        sections.sort_by(|left, right| left.x_fraction.total_cmp(&right.x_fraction));
        set_custom_sections(
            state,
            "fuselage",
            serde_json::to_value(&sections).unwrap_or(Value::Array(Vec::new())),
        );
        state.on_config_modified();
    }
    if generated != generated_before {
        set_generated_fuselage_sections(state, &generated);
        state.on_config_modified();
    }
    ui.horizontal(|ui| {
        if ui
            .add_sized(
                [ui.available_width() * 0.5 - 3.0, 24.0],
                egui::Button::new(tr("Create fuselage section")),
            )
            .clicked()
        {
            if let Some(x_fraction) = next_fuselage_fraction_available(state, &sections) {
                sections.push(
                    interpolated_fuselage_section(state, x_fraction, &sections).unwrap_or(
                        FuselageSection {
                            x_fraction,
                            width_m: 6.0,
                            height_m: 6.0,
                            z_m: 0.0,
                            shape: 2.0,
                        },
                    ),
                );
                sections.sort_by(|left, right| left.x_fraction.total_cmp(&right.x_fraction));
                set_custom_sections(
                    state,
                    "fuselage",
                    serde_json::to_value(&sections).unwrap_or(Value::Array(Vec::new())),
                );
                state.on_config_modified();
            }
        }
        if ui
            .add_sized(
                [ui.available_width(), 24.0],
                egui::Button::new(tr("Remove last fuselage section")),
            )
            .clicked()
        {
            if sections.pop().is_some() {
                set_custom_sections(
                    state,
                    "fuselage",
                    serde_json::to_value(&sections).unwrap_or(Value::Array(Vec::new())),
                );
                state.on_config_modified();
            }
        }
    });
}

/// Convert the resolved transport planform into the rows shown for the
/// preset's defining wing sections.  These values deliberately match the
/// aircraft builder's coordinates, so a new row can be compared directly to
/// the existing root/side-of-body/kink/tip geometry.
fn generated_wing_sections(state: &AppState) -> Vec<(MainWingStationKind, WingSection)> {
    let Some(config) = state.typed_config() else {
        return Vec::new();
    };
    let Some(design) = state.current_design() else {
        return Vec::new();
    };
    let Ok(planform) = config.geometry.wing.transport_planform(&design) else {
        return Vec::new();
    };
    let wing = &config.geometry.wing;
    planform
        .stations()
        .into_iter()
        .map(|station| {
            let (z_m, twist_deg, airfoil) = match station.kind {
                MainWingStationKind::Root => (
                    wing.root_z_m,
                    wing.root_twist_deg,
                    wing.root_airfoil.clone(),
                ),
                MainWingStationKind::SideOfBody => {
                    let t = station.span_fraction / planform.kink.span_fraction.max(1e-9);
                    (
                        lerp(wing.root_z_m, wing.break_z_m, t),
                        lerp(wing.root_twist_deg, wing.break_twist_deg, t),
                        wing.root_airfoil.clone(),
                    )
                }
                MainWingStationKind::Kink => (
                    wing.break_z_m,
                    wing.break_twist_deg,
                    wing.root_airfoil.clone(),
                ),
                MainWingStationKind::Tip => {
                    (wing.tip_z_m, design.tip_twist_deg, wing.tip_airfoil.clone())
                }
            };
            (
                station.kind,
                WingSection {
                    span_fraction: station.span_fraction,
                    leading_edge_x_m: station.leading_edge_x_m,
                    chord_m: station.chord_m,
                    z_m,
                    twist_deg,
                    airfoil,
                },
            )
        })
        .collect()
}

fn generated_wing_span_bounds(
    generated: &[(MainWingStationKind, WingSection)],
    index: usize,
) -> (f64, f64) {
    let span_min = if index == 0 {
        0.0
    } else {
        generated[index - 1].1.span_fraction + 0.001
    };
    let span_max = if index + 1 == generated.len() {
        1.0
    } else {
        generated[index + 1].1.span_fraction - 0.001
    };
    if span_min <= span_max {
        (span_min, span_max)
    } else {
        let midpoint = (span_min + span_max) * 0.5;
        (midpoint, midpoint)
    }
}

/// Render one generated wing row.  Cells whose geometry is derived from the
/// common transport planform stay visibly read-only; the remaining cells map
/// one-to-one to a preset scalar or design variable in
/// [`apply_generated_wing_sections`].
fn show_generated_wing_section(
    ui: &mut Ui,
    kind: MainWingStationKind,
    section: &mut WingSection,
    span_min: f64,
    span_max: f64,
) {
    let label = match kind {
        MainWingStationKind::Root => tr("Root (preset)"),
        MainWingStationKind::SideOfBody => tr("Side of body (preset)"),
        MainWingStationKind::Kink => tr("Kink (preset)"),
        MainWingStationKind::Tip => tr("Tip (preset)"),
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        match kind {
            MainWingStationKind::Root | MainWingStationKind::Tip => {
                ui.label(format!("{:.3}", section.span_fraction));
            }
            MainWingStationKind::SideOfBody | MainWingStationKind::Kink => {
                ui.add(
                    DragValue::new(&mut section.span_fraction)
                        .range(span_min..=span_max)
                        .speed(0.01),
                );
            }
        }
        if kind == MainWingStationKind::Kink {
            ui.add(
                DragValue::new(&mut section.leading_edge_x_m)
                    .range(-100.0..=100.0)
                    .speed(0.05),
            );
        } else {
            ui.label(format!("{:.3}", section.leading_edge_x_m));
        }
        ui.add(
            DragValue::new(&mut section.chord_m)
                .range(0.01..=100.0)
                .speed(0.05),
        );
        let derived_from_endpoints = kind == MainWingStationKind::SideOfBody;
        ui.add_enabled(
            !derived_from_endpoints,
            DragValue::new(&mut section.z_m)
                .range(-50.0..=50.0)
                .speed(0.05),
        );
        ui.add_enabled(
            !derived_from_endpoints,
            DragValue::new(&mut section.twist_deg)
                .range(-30.0..=30.0)
                .speed(0.1),
        );
        let editable_airfoil = matches!(kind, MainWingStationKind::Root | MainWingStationKind::Tip);
        if editable_airfoil {
            ui.add(egui::TextEdit::singleline(&mut section.airfoil).desired_width(110.0));
        } else {
            ui.label(format!("{} (shared)", section.airfoil));
        }
    });
}

/// Write generated wing row edits back to the scalar/design values that own
/// the transport planform.  The generated rows are a projection of those
/// values, so rebuilding the row after each committed edit keeps every preset
/// and every added station coherent.
fn apply_generated_wing_sections(
    state: &mut AppState,
    before: &[(MainWingStationKind, WingSection)],
    after: &[(MainWingStationKind, WingSection)],
) {
    let Some(design) = state.current_design() else {
        return;
    };
    // Compute this before taking any mutable state borrow.  Calling
    // `typed_config()` while a closure has captured `state.config_values`
    // mutably triggers a borrow conflict and, more importantly, would make
    // the sweep conversion depend on a partially-updated configuration.
    let current_kink_y = state
        .typed_config()
        .and_then(|config| config.geometry.wing.transport_planform(&design).ok())
        .map(|planform| planform.kink.y_m);
    for ((kind, old), (_, edited)) in before.iter().zip(after) {
        if old == edited {
            continue;
        }
        match kind {
            MainWingStationKind::Root => {
                set_generated_design_value(state, "root_chord_m", edited.chord_m);
                set_generated_config_value(
                    state,
                    "/geometry/wing/root_z_m",
                    Value::from(edited.z_m),
                );
                set_generated_config_value(
                    state,
                    "/geometry/wing/root_twist_deg",
                    Value::from(edited.twist_deg),
                );
                set_generated_config_value(
                    state,
                    "/geometry/wing/root_airfoil",
                    Value::String(edited.airfoil.clone()),
                );
            }
            MainWingStationKind::SideOfBody => {
                set_generated_config_value(
                    state,
                    "/geometry/wing/side_of_body_span_fraction",
                    Value::from(edited.span_fraction),
                );
                if design.root_chord_m > 1e-9 {
                    set_generated_config_value(
                        state,
                        "/geometry/wing/side_of_body_chord_ratio",
                        Value::from(edited.chord_m / design.root_chord_m),
                    );
                }
            }
            MainWingStationKind::Kink => {
                set_generated_config_value(
                    state,
                    "/geometry/wing/kink_span_fraction",
                    Value::from(edited.span_fraction),
                );
                set_generated_design_value(state, "break_chord_m", edited.chord_m);
                set_generated_config_value(
                    state,
                    "/geometry/wing/break_z_m",
                    Value::from(edited.z_m),
                );
                set_generated_config_value(
                    state,
                    "/geometry/wing/break_twist_deg",
                    Value::from(edited.twist_deg),
                );
                if let Some(kink_y) = current_kink_y.filter(|y| y.abs() > 1e-9) {
                    set_generated_design_value(
                        state,
                        "sweep_deg",
                        edited.leading_edge_x_m.atan2(kink_y).to_degrees(),
                    );
                }
            }
            MainWingStationKind::Tip => {
                set_generated_design_value(state, "tip_chord_m", edited.chord_m);
                set_generated_config_value(
                    state,
                    "/geometry/wing/tip_z_m",
                    Value::from(edited.z_m),
                );
                set_generated_design_value(state, "tip_twist_deg", edited.twist_deg);
                set_generated_config_value(
                    state,
                    "/geometry/wing/tip_airfoil",
                    Value::String(edited.airfoil.clone()),
                );
            }
        }
    }
}

fn set_generated_config_value(state: &mut AppState, pointer: &str, value: Value) {
    if let Some(slot) = state.config_values.pointer_mut(pointer) {
        *slot = value;
    } else if let Some((parent, key)) = pointer.rsplit_once('/') {
        if let Some(map) = state
            .config_values
            .pointer_mut(parent)
            .and_then(Value::as_object_mut)
        {
            map.insert(key.to_owned(), value);
        }
    }
}

fn set_generated_design_value(state: &mut AppState, name: &str, value: f64) {
    state.design_values.insert(name.to_owned(), value);
}

fn generated_fuselage_sections(state: &AppState) -> Vec<FuselageSection> {
    let Some(config) = state.typed_config() else {
        return Vec::new();
    };
    let Some(design) = state.current_design() else {
        return Vec::new();
    };
    let fuselage = &config.geometry.fuselage;
    let length_m = design.fuselage_length_m;
    if !(length_m > 0.0) {
        return Vec::new();
    }
    let cabin_start_m = fuselage.cabin_start_x_m;
    let cabin_end_m = length_m - fuselage.tailcone_length_m;
    let radius_m = fuselage.diameter_m / 2.0;
    let mut sections = Vec::new();
    for xi in sinspace(0.0, 1.0, 10).into_iter().take(9) {
        sections.push(generated_fuselage_section(
            fuselage,
            length_m,
            cabin_start_m,
            cabin_end_m,
            radius_m,
            xi * cabin_start_m,
        ));
    }
    sections.push(generated_fuselage_section(
        fuselage,
        length_m,
        cabin_start_m,
        cabin_end_m,
        radius_m,
        cabin_start_m,
    ));
    sections.push(generated_fuselage_section(
        fuselage,
        length_m,
        cabin_start_m,
        cabin_end_m,
        radius_m,
        cabin_end_m,
    ));
    for xi in linspace(0.0, 1.0, 10).into_iter().skip(1) {
        sections.push(generated_fuselage_section(
            fuselage,
            length_m,
            cabin_start_m,
            cabin_end_m,
            radius_m,
            cabin_end_m + xi * fuselage.tailcone_length_m,
        ));
    }
    for (index, override_section) in fuselage.generated_sections.iter().enumerate() {
        let Some(base) = sections.get_mut(index) else {
            break;
        };
        // X is generated by the body station equations and stays tied to the
        // body parameters; the sandbox override owns the local cross-section.
        base.width_m = override_section.width_m;
        base.height_m = override_section.height_m;
        base.z_m = override_section.z_m;
        base.shape = override_section.shape;
    }
    sections
}

fn show_generated_fuselage_section(ui: &mut Ui, index: usize, section: &mut FuselageSection) {
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("{} {}", tr("Preset station"), index + 1));
        ui.label(format!("{:.3}", section.x_fraction));
        ui.add(
            DragValue::new(&mut section.width_m)
                .range(0.01..=100.0)
                .speed(0.05),
        );
        ui.add(
            DragValue::new(&mut section.height_m)
                .range(0.01..=100.0)
                .speed(0.05),
        );
        ui.add(
            DragValue::new(&mut section.z_m)
                .range(-50.0..=50.0)
                .speed(0.05),
        );
        ui.add(
            DragValue::new(&mut section.shape)
                .range(1.0..=50.0)
                .speed(0.1),
        );
    });
}

fn set_generated_fuselage_sections(state: &mut AppState, sections: &[FuselageSection]) {
    if let Some(fuselage) = state
        .config_values
        .pointer_mut("/geometry/fuselage")
        .and_then(Value::as_object_mut)
    {
        fuselage.insert(
            "generated_sections".to_owned(),
            serde_json::to_value(sections).unwrap_or(Value::Array(Vec::new())),
        );
    }
}

fn set_custom_sections(state: &mut AppState, surface: &str, value: Value) {
    if let Some(geometry) = state.config_values.get_mut("geometry") {
        if let Some(surface) = geometry.get_mut(surface) {
            surface["custom_sections"] = value;
        }
    }
}

#[cfg(test)]
fn next_wing_span(sections: &[WingSection]) -> Option<f64> {
    [0.20, 0.40, 0.60, 0.80, 0.90]
        .into_iter()
        .find(|candidate| {
            sections
                .iter()
                .all(|section| (*candidate - section.span_fraction).abs() > 1.0e-9)
        })
}

fn next_wing_span_available(state: &AppState, sections: &[WingSection]) -> Option<f64> {
    let generated = generated_wing_sections(state);
    [0.20, 0.40, 0.60, 0.80, 0.90]
        .into_iter()
        .find(|candidate| {
            sections
                .iter()
                .chain(generated.iter().map(|(_, section)| section))
                .all(|section| (*candidate - section.span_fraction).abs() > 1.0e-6)
        })
}

#[cfg(test)]
fn next_fuselage_fraction(sections: &[FuselageSection]) -> Option<f64> {
    [0.10, 0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80, 0.90]
        .into_iter()
        .find(|candidate| {
            sections
                .iter()
                .all(|section| (*candidate - section.x_fraction).abs() > 1.0e-9)
        })
}

fn next_fuselage_fraction_available(state: &AppState, sections: &[FuselageSection]) -> Option<f64> {
    let generated = generated_fuselage_sections(state);
    [0.10, 0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80, 0.90]
        .into_iter()
        .find(|candidate| {
            sections
                .iter()
                .chain(generated.iter())
                .all(|section| (*candidate - section.x_fraction).abs() > 1.0e-6)
        })
}

/// Build a wing section by interpolating the generated planform and any
/// existing custom stations. New stations therefore inherit the actual local
/// leading-edge offset, chord, dihedral, twist and airfoil transition instead
/// of starting from an unrelated NACA/default section.
fn interpolated_wing_section(
    state: &AppState,
    span_fraction: f64,
    existing: &[WingSection],
) -> Option<WingSection> {
    let config = state.typed_config()?;
    let design = state.current_design()?;
    let planform = config.geometry.wing.transport_planform(&design).ok()?;
    let wing = &config.geometry.wing;
    let mut anchors = planform
        .stations()
        .into_iter()
        .map(|station| {
            let (z_m, twist_deg, airfoil) = match station.kind {
                MainWingStationKind::Root => (
                    wing.root_z_m,
                    wing.root_twist_deg,
                    wing.root_airfoil.clone(),
                ),
                MainWingStationKind::SideOfBody => {
                    let t = station.span_fraction / planform.kink.span_fraction;
                    (
                        lerp(wing.root_z_m, wing.break_z_m, t),
                        lerp(wing.root_twist_deg, wing.break_twist_deg, t),
                        wing.root_airfoil.clone(),
                    )
                }
                MainWingStationKind::Kink => (
                    wing.break_z_m,
                    wing.break_twist_deg,
                    wing.root_airfoil.clone(),
                ),
                MainWingStationKind::Tip => {
                    (wing.tip_z_m, design.tip_twist_deg, wing.tip_airfoil.clone())
                }
            };
            WingSection {
                span_fraction: station.span_fraction,
                leading_edge_x_m: station.leading_edge_x_m,
                chord_m: station.chord_m,
                z_m,
                twist_deg,
                airfoil,
            }
        })
        .collect::<Vec<_>>();
    anchors.extend(existing.iter().cloned());
    interpolate_wing_anchor(span_fraction, &mut anchors)
}

fn interpolate_wing_anchor(span_fraction: f64, anchors: &mut [WingSection]) -> Option<WingSection> {
    anchors.sort_by(|left, right| left.span_fraction.total_cmp(&right.span_fraction));
    let exact = anchors
        .iter()
        .find(|section| (section.span_fraction - span_fraction).abs() <= 1.0e-9)
        .cloned();
    if exact.is_some() {
        return exact;
    }
    let pair = anchors.windows(2).find(|pair| {
        pair[0].span_fraction < span_fraction && span_fraction < pair[1].span_fraction
    })?;
    let t =
        (span_fraction - pair[0].span_fraction) / (pair[1].span_fraction - pair[0].span_fraction);
    Some(WingSection {
        span_fraction,
        leading_edge_x_m: lerp(pair[0].leading_edge_x_m, pair[1].leading_edge_x_m, t),
        chord_m: lerp(pair[0].chord_m, pair[1].chord_m, t),
        z_m: lerp(pair[0].z_m, pair[1].z_m, t),
        twist_deg: lerp(pair[0].twist_deg, pair[1].twist_deg, t),
        airfoil: if t < 0.5 {
            pair[0].airfoil.clone()
        } else {
            pair[1].airfoil.clone()
        },
    })
}

/// Build the generated fuselage stations with the same nose/cabin/tail
/// equations as the production loft, then interpolate a new station between
/// its true neighbours and any custom stations already present.
fn interpolated_fuselage_section(
    state: &AppState,
    x_fraction: f64,
    existing: &[FuselageSection],
) -> Option<FuselageSection> {
    let config = state.typed_config()?;
    let design = state.current_design()?;
    let fuselage = &config.geometry.fuselage;
    let length_m = design.fuselage_length_m;
    if !(length_m > 0.0) {
        return None;
    }
    let cabin_start_m = fuselage.cabin_start_x_m;
    let cabin_end_m = length_m - fuselage.tailcone_length_m;
    let radius_m = fuselage.diameter_m / 2.0;
    let mut anchors = Vec::new();
    for xi in sinspace(0.0, 1.0, 10).into_iter().take(9) {
        anchors.push(generated_fuselage_section(
            fuselage,
            length_m,
            cabin_start_m,
            cabin_end_m,
            radius_m,
            xi * cabin_start_m,
        ));
    }
    anchors.push(generated_fuselage_section(
        fuselage,
        length_m,
        cabin_start_m,
        cabin_end_m,
        radius_m,
        cabin_start_m,
    ));
    anchors.push(generated_fuselage_section(
        fuselage,
        length_m,
        cabin_start_m,
        cabin_end_m,
        radius_m,
        cabin_end_m,
    ));
    for xi in linspace(0.0, 1.0, 10).into_iter().skip(1) {
        anchors.push(generated_fuselage_section(
            fuselage,
            length_m,
            cabin_start_m,
            cabin_end_m,
            radius_m,
            cabin_end_m + xi * fuselage.tailcone_length_m,
        ));
    }
    anchors.extend(existing.iter().cloned());
    interpolate_fuselage_anchor(x_fraction, &mut anchors)
}

fn generated_fuselage_section(
    fuselage: &alas_config::FuselageConfig,
    length_m: f64,
    cabin_start_m: f64,
    cabin_end_m: f64,
    radius_m: f64,
    x_m: f64,
) -> FuselageSection {
    let (z_m, radius) = if x_m <= cabin_start_m && cabin_start_m > 0.0 {
        let xi = (x_m / cabin_start_m).clamp(0.0, 1.0);
        (
            fuselage.cabin_z_m + (fuselage.nose_z_m - fuselage.cabin_z_m) * (1.0 - xi).powi(2),
            radius_m * (1.0 - (1.0 - xi).powi(2)).max(0.0).sqrt(),
        )
    } else if x_m >= cabin_end_m && fuselage.tailcone_length_m > 0.0 {
        let xi = ((x_m - cabin_end_m) / fuselage.tailcone_length_m).clamp(0.0, 1.0);
        (
            fuselage.cabin_z_m + (fuselage.tail_z_m - fuselage.cabin_z_m) * xi.powf(1.5),
            radius_m * (1.0 - xi.powf(1.5)).max(0.0),
        )
    } else {
        (fuselage.cabin_z_m, radius_m)
    };
    let width_m = radius * 2.0;
    let height_m = fuselage
        .height_m
        .map(|height| width_m * height / fuselage.diameter_m.max(f64::MIN_POSITIVE))
        .unwrap_or(width_m);
    FuselageSection {
        x_fraction: (x_m / length_m).clamp(0.0, 1.0),
        width_m,
        height_m,
        z_m,
        shape: 2.0,
    }
}

fn interpolate_fuselage_anchor(
    x_fraction: f64,
    anchors: &mut [FuselageSection],
) -> Option<FuselageSection> {
    anchors.sort_by(|left, right| left.x_fraction.total_cmp(&right.x_fraction));
    if let Some(exact) = anchors
        .iter()
        .find(|section| (section.x_fraction - x_fraction).abs() <= 1.0e-9)
        .copied()
    {
        return Some(exact);
    }
    let pair = anchors
        .windows(2)
        .find(|pair| pair[0].x_fraction < x_fraction && x_fraction < pair[1].x_fraction)?;
    let t = (x_fraction - pair[0].x_fraction) / (pair[1].x_fraction - pair[0].x_fraction);
    Some(FuselageSection {
        x_fraction,
        width_m: lerp(pair[0].width_m, pair[1].width_m, t),
        height_m: lerp(pair[0].height_m, pair[1].height_m, t),
        z_m: lerp(pair[0].z_m, pair[1].z_m, t),
        shape: lerp(pair[0].shape, pair[1].shape, t),
    })
}

fn lerp(left: f64, right: f64, t: f64) -> f64 {
    left + t.clamp(0.0, 1.0) * (right - left)
}

fn linspace(start: f64, stop: f64, count: usize) -> Vec<f64> {
    if count < 2 {
        return vec![start];
    }
    (0..count)
        .map(|index| start + (stop - start) * index as f64 / (count - 1) as f64)
        .collect()
}

fn sinspace(start: f64, stop: f64, count: usize) -> Vec<f64> {
    linspace(0.0, std::f64::consts::FRAC_PI_2, count)
        .into_iter()
        .map(|angle| start + (stop - start) * (1.0 - angle.cos()))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::{
        apply_generated_wing_sections, generated_wing_sections, generated_wing_span_bounds,
        interpolate_fuselage_anchor, interpolate_wing_anchor, interpolated_fuselage_section,
        interpolated_wing_section, next_fuselage_fraction, next_fuselage_fraction_available,
        next_wing_span, next_wing_span_available,
    };
    use alas_config::{FuselageSection, MainWingStationKind, WingSection};
    use serde_json::Value;

    #[test]
    fn wing_add_station_uses_the_first_free_coordinate_even_when_rows_are_unsorted() {
        let sections = vec![
            WingSection {
                span_fraction: 0.80,
                leading_edge_x_m: 0.0,
                chord_m: 2.0,
                z_m: 0.0,
                twist_deg: 0.0,
                airfoil: "naca2410".to_owned(),
            },
            WingSection {
                span_fraction: 0.20,
                leading_edge_x_m: 0.0,
                chord_m: 2.0,
                z_m: 0.0,
                twist_deg: 0.0,
                airfoil: "naca2410".to_owned(),
            },
        ];
        assert_eq!(next_wing_span(&sections), Some(0.40));
    }

    #[test]
    fn fuselage_add_station_never_reuses_an_existing_coordinate() {
        let sections = vec![FuselageSection {
            x_fraction: 0.10,
            width_m: 2.0,
            height_m: 2.0,
            z_m: 0.0,
            shape: 2.0,
        }];
        assert_eq!(next_fuselage_fraction(&sections), Some(0.20));
    }

    #[test]
    fn wing_station_interpolation_preserves_the_existing_loft_trend() {
        let mut anchors = vec![
            WingSection {
                span_fraction: 0.10,
                leading_edge_x_m: 2.0,
                chord_m: 10.0,
                z_m: -1.0,
                twist_deg: 4.0,
                airfoil: "root".to_owned(),
            },
            WingSection {
                span_fraction: 0.50,
                leading_edge_x_m: 6.0,
                chord_m: 5.0,
                z_m: 1.0,
                twist_deg: 1.0,
                airfoil: "kink".to_owned(),
            },
        ];
        let section = interpolate_wing_anchor(0.30, &mut anchors).expect("bracketed station");
        assert!((section.leading_edge_x_m - 4.0).abs() < 1.0e-12);
        assert!((section.chord_m - 7.5).abs() < 1.0e-12);
        assert!((section.z_m - 0.0).abs() < 1.0e-12);
        assert!((section.twist_deg - 2.5).abs() < 1.0e-12);
        assert_eq!(section.airfoil, "root");
    }

    #[test]
    fn fuselage_station_interpolation_preserves_all_section_dimensions() {
        let mut anchors = vec![
            FuselageSection {
                x_fraction: 0.20,
                width_m: 4.0,
                height_m: 3.0,
                z_m: -0.5,
                shape: 1.5,
            },
            FuselageSection {
                x_fraction: 0.60,
                width_m: 8.0,
                height_m: 7.0,
                z_m: 0.5,
                shape: 3.5,
            },
        ];
        let section = interpolate_fuselage_anchor(0.40, &mut anchors).expect("bracketed station");
        assert!((section.width_m - 6.0).abs() < 1.0e-12);
        assert!((section.height_m - 5.0).abs() < 1.0e-12);
        assert!((section.z_m - 0.0).abs() < 1.0e-12);
        assert!((section.shape - 2.5).abs() < 1.0e-12);
    }

    #[test]
    fn generated_wing_and_fuselage_additions_are_physical() {
        let state = crate::state::AppState::default();
        let wing = interpolated_wing_section(&state, 0.20, &[]).expect("wing planform");
        assert!(wing.leading_edge_x_m.is_finite());
        assert!(wing.chord_m > 0.0);
        assert!(wing.z_m.is_finite() && wing.twist_deg.is_finite());
        assert!(!wing.airfoil.is_empty());

        let fuselage = interpolated_fuselage_section(&state, 0.20, &[]).expect("fuselage loft");
        assert!(fuselage.width_m > 0.0 && fuselage.height_m > 0.0);
        assert!(fuselage.z_m.is_finite() && (1.0..=50.0).contains(&fuselage.shape));
    }

    #[test]
    fn generated_wing_rows_round_trip_to_each_transport_owner() {
        let mut state = crate::state::AppState::default();
        let before = generated_wing_sections(&state);
        assert_eq!(
            before.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
            vec![
                MainWingStationKind::Root,
                MainWingStationKind::SideOfBody,
                MainWingStationKind::Kink,
                MainWingStationKind::Tip,
            ]
        );

        let mut root_edit = before.clone();
        root_edit[0].1.chord_m += 0.2;
        root_edit[0].1.z_m += 0.15;
        root_edit[0].1.twist_deg += 0.3;
        root_edit[0].1.airfoil = "naca0012".to_owned();
        apply_generated_wing_sections(&mut state, &before, &root_edit);
        let root = generated_wing_sections(&state)[0].1.clone();
        assert!((root.chord_m - root_edit[0].1.chord_m).abs() < 1.0e-12);
        assert!((root.z_m - root_edit[0].1.z_m).abs() < 1.0e-12);
        assert_eq!(root.airfoil, "naca0012");

        let before_side = generated_wing_sections(&state);
        let mut side_edit = before_side.clone();
        side_edit[1].1.span_fraction += 0.01;
        side_edit[1].1.chord_m -= 0.1;
        apply_generated_wing_sections(&mut state, &before_side, &side_edit);
        let side = generated_wing_sections(&state)[1].1.clone();
        assert!((side.span_fraction - side_edit[1].1.span_fraction).abs() < 1.0e-12);
        assert!((side.chord_m - side_edit[1].1.chord_m).abs() < 1.0e-12);
        assert!(state
            .config_values
            .pointer("/geometry/wing/side_of_body_chord_ratio")
            .is_some());

        let before_kink = generated_wing_sections(&state);
        let mut kink_edit = before_kink.clone();
        kink_edit[2].1.leading_edge_x_m += 0.25;
        kink_edit[2].1.chord_m -= 0.15;
        kink_edit[2].1.z_m += 0.1;
        kink_edit[2].1.twist_deg -= 0.2;
        apply_generated_wing_sections(&mut state, &before_kink, &kink_edit);
        let kink = generated_wing_sections(&state)[2].1.clone();
        assert!((kink.leading_edge_x_m - kink_edit[2].1.leading_edge_x_m).abs() < 1.0e-10);
        assert!((kink.chord_m - kink_edit[2].1.chord_m).abs() < 1.0e-12);
        assert!((kink.z_m - kink_edit[2].1.z_m).abs() < 1.0e-12);

        let before_tip = generated_wing_sections(&state);
        let mut tip_edit = before_tip.clone();
        tip_edit[3].1.chord_m -= 0.2;
        tip_edit[3].1.z_m += 0.2;
        tip_edit[3].1.twist_deg -= 0.4;
        tip_edit[3].1.airfoil = "naca0015".to_owned();
        apply_generated_wing_sections(&mut state, &before_tip, &tip_edit);
        let tip = generated_wing_sections(&state)[3].1.clone();
        assert!((tip.chord_m - tip_edit[3].1.chord_m).abs() < 1.0e-12);
        assert!((tip.z_m - tip_edit[3].1.z_m).abs() < 1.0e-12);
        assert_eq!(tip.airfoil, "naca0015");
    }

    #[test]
    fn generated_rows_cover_legacy_wings_without_a_side_of_body_station() {
        let mut state = crate::state::AppState::default();
        let mut config = state.typed_config().expect("config");
        config.geometry.wing.side_of_body_span_fraction = None;
        config.geometry.wing.kink_span_fraction = None;
        state.config_values = serde_json::to_value(config).expect("config JSON");
        let rows = generated_wing_sections(&state);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].0, MainWingStationKind::Root);
        assert_eq!(rows[1].0, MainWingStationKind::Kink);
        assert_eq!(rows[2].0, MainWingStationKind::Tip);
    }

    #[test]
    fn new_section_candidates_skip_preset_station_coordinates() {
        let mut state = crate::state::AppState::default();
        state.config_values["geometry"]["wing"]["kink_span_fraction"] = Value::from(0.20);
        assert_eq!(next_wing_span_available(&state, &[]), Some(0.40));

        state.config_values["geometry"]["fuselage"]["cabin_start_x_m"] = Value::from(6.0);
        state
            .design_values
            .insert("fuselage_length_m".to_owned(), 60.0);
        assert_eq!(next_fuselage_fraction_available(&state, &[]), Some(0.20));
    }

    #[test]
    fn generated_span_editor_bounds_keep_side_and_kink_ordered() {
        let generated = vec![
            (
                MainWingStationKind::Root,
                WingSection {
                    span_fraction: 0.0,
                    leading_edge_x_m: 0.0,
                    chord_m: 10.0,
                    z_m: 0.0,
                    twist_deg: 0.0,
                    airfoil: "root".to_owned(),
                },
            ),
            (
                MainWingStationKind::SideOfBody,
                WingSection {
                    span_fraction: 0.90,
                    leading_edge_x_m: 0.0,
                    chord_m: 5.0,
                    z_m: 0.0,
                    twist_deg: 0.0,
                    airfoil: "root".to_owned(),
                },
            ),
            (
                MainWingStationKind::Kink,
                WingSection {
                    span_fraction: 0.20,
                    leading_edge_x_m: 0.0,
                    chord_m: 4.0,
                    z_m: 0.0,
                    twist_deg: 0.0,
                    airfoil: "root".to_owned(),
                },
            ),
            (
                MainWingStationKind::Tip,
                WingSection {
                    span_fraction: 1.0,
                    leading_edge_x_m: 0.0,
                    chord_m: 2.0,
                    z_m: 0.0,
                    twist_deg: 0.0,
                    airfoil: "tip".to_owned(),
                },
            ),
        ];
        let side = generated_wing_span_bounds(&generated, 1);
        let kink = generated_wing_span_bounds(&generated, 2);
        assert!(side.0 <= side.1);
        assert!(kink.0 <= kink.1);
        assert!(side.1 < 0.20);
        assert!(kink.0 > 0.90);
    }
}
