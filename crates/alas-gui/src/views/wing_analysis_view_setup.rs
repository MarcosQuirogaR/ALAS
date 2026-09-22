// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Setup tab: the analysed configuration, the live preview, and every input a
//! run reads.
//!
//! Each control invalidates the installed result as soon as it changes, so the
//! Results tab can never describe a condition the user has already edited.

use egui::{vec2, ComboBox, DragValue, Grid, RichText, ScrollArea, Ui};

use alas_aero::wing_analysis::{
    AttitudeInput, SpeedInput, SurfaceSet, ALPHA_LIMIT_DEG, OMITTED_COMPONENTS,
};
use alas_viz::SceneView;

use super::wing_analysis::{with_window, WingAnalysisState, WING_ANALYSIS_VIEW_KEY};
use crate::state::AppState;
use crate::views::{tr, tr_fields};

/// Width from which the condition and reference cards share a row.
const TWO_COLUMN_WIDTH: f32 = 900.0;

/// Preview height bounds, in points.
const PREVIEW_MIN_HEIGHT: f32 = 200.0;
const PREVIEW_MAX_HEIGHT: f32 = 420.0;

pub(crate) fn show_setup_tab(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .id_salt("wing_analysis_setup_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            with_window(|window| show_configuration_card(window, ui));
            ui.add_space(8.0);
            show_preview_card(state, ui);
            ui.add_space(8.0);
            with_window(|window| {
                if ui.available_width() >= TWO_COLUMN_WIDTH {
                    ui.columns(2, |columns| {
                        show_condition_card(window, &mut columns[0]);
                        show_reference_card(window, &mut columns[1]);
                    });
                } else {
                    show_condition_card(window, ui);
                    ui.add_space(8.0);
                    show_reference_card(window, ui);
                }
                ui.add_space(8.0);
                show_lattice_card(window, ui);
            });
        });
}

/// What is modelled, what is not, and the empennage option.
fn show_configuration_card(window: &mut WingAnalysisState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Analysed configuration")).strong().size(15.0));
        let surfaces = window.surface_names();
        let names = if surfaces.is_empty() {
            tr("No surfaces are available.")
        } else {
            surfaces.join(", ")
        };
        ui.label(tr_fields(
            "Modelled surfaces: {names}",
            &[("names", names)],
        ));
        if !window.source_name.is_empty() {
            ui.label(
                RichText::new(tr_fields(
                    "Wing taken from the current configuration: {source}",
                    &[("source", window.source_name.clone())],
                ))
                .weak()
                .small(),
            );
        }
        let mut empennage = window.includes_empennage();
        if ui
            .checkbox(&mut empennage, tr("Include empennage"))
            .on_hover_text(tr(
                "Add the horizontal and vertical tail as lofted, and report the static stability of that configuration about the stated moment reference.",
            ))
            .changed()
        {
            window.set_surfaces(if empennage {
                SurfaceSet::WingAndEmpennage
            } else {
                SurfaceSet::WingOnly
            });
        }
        ui.label(
            RichText::new(tr_fields(
                "This configuration omits: {components}.",
                &[("components", OMITTED_COMPONENTS.join(", "))],
            ))
            .weak()
            .small(),
        );
        ui.label(
            RichText::new(tr(
                "The model is an incompressible, inviscid vortex lattice: its drag is induced drag only, and Mach number sets the speed without correcting any coefficient.",
            ))
            .weak()
            .small(),
        );
    });
}

/// The live preview of exactly the surfaces the run models.
fn show_preview_card(state: &mut AppState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("Live preview")).strong().size(15.0));
            ui.label(
                RichText::new(tr("Updates with every geometry or empennage change."))
                    .weak()
                    .small(),
            );
        });
        let scene = with_window(|window| window.preview.clone());
        let Some(scene) = scene else {
            ui.label(tr("The analysed wing is not available to preview."));
            return;
        };
        let width = ui.available_width().max(200.0);
        let height = (width * 0.42).clamp(PREVIEW_MIN_HEIGHT, PREVIEW_MAX_HEIGHT);
        let revision = with_window(|window| window.geometry_revision);
        let response = ui.add(
            SceneView::new(&scene, state.view_state_mut(WING_ANALYSIS_VIEW_KEY))
                .desired_size(vec2(width, height))
                .orbit_only()
                .show_toolbar(false)
                .cache_key(WING_ANALYSIS_VIEW_KEY)
                .cache_revision(revision),
        );
        let response = response.on_hover_text(tr("Drag to orbit the camera; scroll to zoom"));
        let theme = state.theme.figure_theme_name().to_owned();
        with_window(|window| {
            if orbit_preview(window, &response) {
                window.rebuild_preview(&theme);
                response.ctx.request_repaint();
            }
            show_camera_buttons(window, ui, &theme);
        });
    });
}

/// Apply this frame's orbit and zoom gestures to the preview camera.
fn orbit_preview(window: &mut WingAnalysisState, response: &egui::Response) -> bool {
    let mut changed = false;
    if response.dragged_by(egui::PointerButton::Primary)
        || response.dragged_by(egui::PointerButton::Middle)
    {
        let delta = response.drag_motion();
        window.camera.apply_orbit_motion(delta);
        changed = delta.is_finite() && delta != egui::Vec2::ZERO;
    }
    if response.hovered() {
        let scroll = response.ctx.input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > f32::EPSILON {
            window
                .camera
                .apply_zoom_factor(f64::from((1.0 + scroll * 0.0015).clamp(0.5, 1.5)));
            changed = true;
        }
    }
    changed
}

/// Standard views and a fit action for the preview camera.
fn show_camera_buttons(window: &mut WingAnalysisState, ui: &mut Ui, theme: &str) {
    ui.horizontal_wrapped(|ui| {
        for (label, camera) in [
            ("Isometric", crate::viewport::PreviewCamera::isometric()),
            ("Top", crate::viewport::PreviewCamera::top()),
            ("Front", crate::viewport::PreviewCamera::front()),
            ("Side", crate::viewport::PreviewCamera::side()),
        ] {
            if ui.button(tr(label)).clicked() {
                window.camera = camera;
                window.rebuild_preview(theme);
            }
        }
        if ui.button(tr("Fit")).clicked() {
            window.camera.fit();
            window.rebuild_preview(theme);
        }
    });
}

/// The explicit flight condition, with the state it resolves to.
fn show_condition_card(window: &mut WingAnalysisState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Flight condition")).strong().size(15.0));
        let mut changed = false;
        Grid::new("wing_analysis_condition")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label(tr("Altitude (m)"));
                changed |= ui
                    .add(
                        DragValue::new(&mut window.inputs.condition.altitude_m)
                            .speed(50.0)
                            .range(-500.0..=25_000.0),
                    )
                    .on_hover_text(tr(
                        "Geopotential altitude of the standard atmosphere the density and speed of sound come from.",
                    ))
                    .changed();
                ui.end_row();

                let mut by_mach = matches!(window.inputs.condition.speed, SpeedInput::Mach(_));
                ui.label(tr("Speed stated as"));
                ui.horizontal(|ui| {
                    ComboBox::from_id_salt("wing_analysis_speed_kind")
                        .width(120.0)
                        .selected_text(if by_mach { tr("Mach") } else { tr("True airspeed") })
                        .show_ui(ui, |ui| {
                            changed |= ui.selectable_value(&mut by_mach, true, tr("Mach")).changed();
                            changed |= ui
                                .selectable_value(&mut by_mach, false, tr("True airspeed"))
                                .changed();
                        });
                    match (by_mach, window.inputs.condition.speed) {
                        (true, SpeedInput::TrueAirspeed(_)) => {
                            window.inputs.condition.speed = SpeedInput::Mach(0.5);
                        }
                        (false, SpeedInput::Mach(_)) => {
                            window.inputs.condition.speed = SpeedInput::TrueAirspeed(150.0);
                        }
                        _ => {}
                    }
                    match &mut window.inputs.condition.speed {
                        SpeedInput::Mach(value) => {
                            changed |= ui
                                .add(DragValue::new(value).speed(0.01).range(0.01..=0.95))
                                .changed();
                            ui.label(RichText::new(tr("dimensionless")).weak());
                        }
                        SpeedInput::TrueAirspeed(value) => {
                            changed |= ui
                                .add(DragValue::new(value).speed(1.0).range(1.0..=400.0))
                                .changed();
                            ui.label(RichText::new(tr("m/s")).weak());
                        }
                    }
                });
                ui.end_row();

                let mut by_alpha =
                    matches!(window.inputs.condition.attitude, AttitudeInput::AngleOfAttack(_));
                ui.label(tr("Attitude stated as"));
                ui.horizontal(|ui| {
                    ComboBox::from_id_salt("wing_analysis_attitude_kind")
                        .width(120.0)
                        .selected_text(if by_alpha {
                            tr("Angle of attack")
                        } else {
                            tr("Lift coefficient")
                        })
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(&mut by_alpha, true, tr("Angle of attack"))
                                .changed();
                            changed |= ui
                                .selectable_value(&mut by_alpha, false, tr("Lift coefficient"))
                                .changed();
                        });
                    match (by_alpha, window.inputs.condition.attitude) {
                        (true, AttitudeInput::LiftCoefficient(_)) => {
                            window.inputs.condition.attitude = AttitudeInput::AngleOfAttack(2.0);
                        }
                        (false, AttitudeInput::AngleOfAttack(_)) => {
                            window.inputs.condition.attitude = AttitudeInput::LiftCoefficient(0.5);
                        }
                        _ => {}
                    }
                    match &mut window.inputs.condition.attitude {
                        AttitudeInput::AngleOfAttack(value) => {
                            changed |= ui
                                .add(
                                    DragValue::new(value)
                                        .speed(0.1)
                                        .range(-ALPHA_LIMIT_DEG..=ALPHA_LIMIT_DEG),
                                )
                                .changed();
                            ui.label(RichText::new(tr("deg, positive nose-up")).weak());
                        }
                        AttitudeInput::LiftCoefficient(value) => {
                            changed |= ui
                                .add(DragValue::new(value).speed(0.01).range(-1.5..=2.5))
                                .changed();
                            ui.label(RichText::new(tr("on the reference area")).weak());
                        }
                    }
                });
                ui.end_row();
            });
        if changed {
            window.invalidate_inputs();
        }
    });
}

/// Reference quantities, axes and the moment reference every moment uses.
fn show_reference_card(window: &mut WingAnalysisState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            RichText::new(tr("Reference quantities, axes and moment reference"))
                .strong()
                .size(15.0),
        );
        match window.reference() {
            Some(reference) => {
                Grid::new("wing_analysis_reference")
                    .num_columns(2)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        for (label, value) in [
                            ("Reference area S (m2)", reference.area_m2),
                            ("Reference span b (m)", reference.span_m),
                            ("Reference chord MAC (m)", reference.chord_m),
                            ("Aspect ratio b2/S", reference.aspect_ratio()),
                        ] {
                            ui.label(tr(label));
                            ui.label(RichText::new(format!("{value:.4}")).strong());
                            ui.end_row();
                        }
                    });
            }
            None => {
                ui.label(tr("Reference quantities need an available wing."));
            }
        }
        ui.label(
            RichText::new(tr(
                "Geometry axes: x aft from the aircraft datum, y to starboard, z up. Lift and drag are wind-axis; the pitching moment is positive nose-up about the point below.",
            ))
            .weak()
            .small(),
        );
        let mut changed = false;
        Grid::new("wing_analysis_moment_reference")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                for (index, label) in ["Moment reference x (m)", "Moment reference y (m)", "Moment reference z (m)"]
                    .into_iter()
                    .enumerate()
                {
                    ui.label(tr(label));
                    changed |= ui
                        .add(
                            DragValue::new(&mut window.inputs.moment_reference_m[index])
                                .speed(0.05)
                                .range(-200.0..=200.0),
                        )
                        .changed();
                    ui.end_row();
                }
            });
        if changed {
            window.moment_reference_manual = true;
            window.invalidate_inputs();
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(tr("Use quarter-MAC"))
                .on_hover_text(tr(
                    "Return the moment reference to the wing's quarter mean-aerodynamic chord. This is a geometric point, not a mass property: no mass model is read.",
                ))
                .clicked()
            {
                window.reset_moment_reference();
            }
            let source = if window.moment_reference_manual {
                tr("Stated by you.")
            } else {
                tr("Derived from the wing geometry.")
            };
            ui.label(RichText::new(source).weak().small());
        });
    });
}

/// Lattice resolution and the swept angle range.
fn show_lattice_card(window: &mut WingAnalysisState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Lattice and sweep")).strong().size(15.0));
        let mut changed = false;
        Grid::new("wing_analysis_lattice")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label(tr("Spanwise panel multiplier"));
                changed |= ui
                    .add(DragValue::new(&mut window.inputs.spanwise_resolution).range(1..=2))
                    .on_hover_text(tr(
                        "Multiplier on each surface's own spanwise subdivision. Above 2 the lattice stops converging in induced drag.",
                    ))
                    .changed();
                ui.end_row();
                ui.label(tr("Chordwise panels per strip"));
                changed |= ui
                    .add(DragValue::new(&mut window.inputs.chordwise_resolution).range(1..=16))
                    .on_hover_text(tr(
                        "At one panel the camber line is sampled only at the edges, which turns every section into a flat plate.",
                    ))
                    .changed();
                ui.end_row();
                ui.label(tr("Sweep alpha minimum (deg)"));
                changed |= ui
                    .add(
                        DragValue::new(&mut window.inputs.sweep.min_deg)
                            .speed(0.5)
                            .range(-ALPHA_LIMIT_DEG..=ALPHA_LIMIT_DEG),
                    )
                    .changed();
                ui.end_row();
                ui.label(tr("Sweep alpha maximum (deg)"));
                changed |= ui
                    .add(
                        DragValue::new(&mut window.inputs.sweep.max_deg)
                            .speed(0.5)
                            .range(-ALPHA_LIMIT_DEG..=ALPHA_LIMIT_DEG),
                    )
                    .changed();
                ui.end_row();
                ui.label(tr("Sweep points"));
                changed |= ui
                    .add(DragValue::new(&mut window.inputs.sweep.points).range(2..=41))
                    .changed();
                ui.end_row();
            });
        if changed {
            window.invalidate_inputs();
        }
        let findings = window.inputs.validate();
        for finding in findings {
            ui.colored_label(ui.visuals().warn_fg_color, tr(&finding));
        }
    });
}
