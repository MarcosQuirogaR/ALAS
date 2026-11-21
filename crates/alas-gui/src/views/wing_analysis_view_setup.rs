// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Setup tab: the analysed configuration, the live preview, and every input a
//! run reads.
//!
//! Each control invalidates the installed result as soon as it changes, so the
//! Results tab can never describe a condition the user has already edited.

use egui::{vec2, ComboBox, DragValue, RichText, ScrollArea, Ui};

use alas_aero::wing_analysis::{AttitudeInput, SpeedInput, SurfaceSet, ALPHA_LIMIT_DEG};
use alas_viz::SceneView;

use super::wing_analysis::{with_window, WingAnalysisState, WING_ANALYSIS_VIEW_KEY};
use crate::state::AppState;
use crate::views::form::CellGrid;
use crate::views::{tr, tr_fields};

/// Width from which the condition and reference cards share a row.
const TWO_COLUMN_WIDTH: f32 = 900.0;

/// Preview height bounds, in points.
const PREVIEW_MIN_HEIGHT: f32 = 150.0;
const PREVIEW_MAX_HEIGHT: f32 = 420.0;

pub(crate) fn show_setup_tab(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .id_salt("wing_analysis_setup_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // The preview comes first and is sized to the window, so the
            // aircraft is seen whole without scrolling; the cards follow.
            show_preview_card(state, ui);
            ui.add_space(8.0);
            with_window(|window| show_configuration_card(window, ui));
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
        let title = crate::theme::card_title(ui, tr("Analysed configuration"));
        crate::theme::header_label(ui, title)
            .on_hover_text(tr(
                "The model is an incompressible, inviscid vortex lattice: its drag is induced drag only, and Mach number sets the speed without correcting any coefficient.",
            ));
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
        if crate::checkbox::checkbox(ui, &mut empennage, tr("Include empennage"))
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
    });
}

/// The live preview of exactly the surfaces the run models.
fn show_preview_card(state: &mut AppState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let title = crate::theme::card_title(ui, tr("Live preview"));
        crate::theme::header_label(ui, title)
            .on_hover_text(tr("Updates with every geometry or empennage change."));
        // The scene is drawn for the pane it is shown in, so its fit follows a
        // resize and the aircraft fills the card at true aspect.
        let width = ui.available_width().max(200.0);
        // The panel fits the window: what is left below the card heading, less
        // the camera buttons and the card frame, down to a usable minimum.
        let room = ui.ctx().screen_rect().bottom() - ui.cursor().top() - 78.0;
        let height = (width * 0.42)
            .min(room)
            .clamp(PREVIEW_MIN_HEIGHT, PREVIEW_MAX_HEIGHT);
        let theme = state.theme.figure_theme_name().to_owned();
        with_window(|window| {
            let canvas = (f64::from(width.round()), f64::from(height.round()));
            if window.preview_canvas != canvas {
                window.preview_canvas = canvas;
                window.rebuild_preview(&theme);
            }
        });
        let scene = with_window(|window| window.preview.clone());
        let Some(scene) = scene else {
            ui.label(tr("The analysed wing is not available to preview."));
            return;
        };
        let revision = with_window(|window| window.preview_revision);
        let response = ui.add(
            SceneView::new(&scene, state.view_state_mut(WING_ANALYSIS_VIEW_KEY))
                .desired_size(vec2(width, height))
                .orbit_only()
                .vector_overlay(true)
                .show_toolbar(false)
                .cache_key(WING_ANALYSIS_VIEW_KEY)
                .cache_revision(revision),
        );
        let view = state.view_state_mut(WING_ANALYSIS_VIEW_KEY);
        with_window(|window| {
            if orbit_preview(window, view, &response) {
                window.rebuild_preview(&theme);
                response.ctx.request_repaint();
            }
            show_camera_buttons(window, view, ui, &theme);
        });
    });
}

/// Apply this frame's orbit and zoom gestures to the preview camera.
fn orbit_preview(
    window: &mut WingAnalysisState,
    view: &mut alas_viz::SceneViewState,
    response: &egui::Response,
) -> bool {
    let mut changed = false;
    // Shared mapping (see `nav_input`); the pan and the re-centre move the canvas.
    if crate::nav_input::pivot_clicked(response) {
        crate::nav_input::recentre_canvas(view, response);
    }
    match crate::nav_input::read(response) {
        crate::nav_input::NavAction::Rotate(delta) => {
            window.camera.apply_orbit_motion(delta);
            changed = delta.is_finite() && delta != egui::Vec2::ZERO;
        }
        crate::nav_input::NavAction::Pan(delta) => crate::nav_input::pan_canvas(view, delta),
        crate::nav_input::NavAction::None => {}
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
fn show_camera_buttons(
    window: &mut WingAnalysisState,
    view: &mut alas_viz::SceneViewState,
    ui: &mut Ui,
    theme: &str,
) {
    ui.horizontal_wrapped(|ui| {
        for (label, camera) in [
            ("Isometric", crate::viewport::PreviewCamera::isometric()),
            ("Top", crate::viewport::PreviewCamera::top()),
            ("Front", crate::viewport::PreviewCamera::front()),
            ("Side", crate::viewport::PreviewCamera::side()),
        ] {
            if ui.button(tr(label)).clicked() {
                window.camera = camera;
                window.preview_fit = None;
                view.reset();
                window.rebuild_preview(theme);
            }
        }
        if ui.button(tr("Fit")).clicked() {
            window.camera.fit();
            window.preview_fit = None;
            view.reset();
            window.rebuild_preview(theme);
        }
        ui.label(
            RichText::new(tr("Left drag: orbit; right, middle or Shift + left drag: pan; wheel: zoom; middle or double click: pivot"))
                .weak()
                .small(),
        );
    });
}

/// A card heading, in the header style of the card's depth.
fn card_title(ui: &mut Ui, text: &str) {
    let title = crate::theme::card_title(ui, tr(text));
    crate::theme::header_label(ui, title);
}

/// A numeric box as wide as its cell, under its label.
fn number_cell(
    ui: &mut Ui,
    width: f32,
    label: &str,
    hover: Option<&str>,
    drag: DragValue<'_>,
) -> bool {
    let response = crate::views::form::label_row(ui, crate::notation::rich(ui, &tr(label)), |_| {});
    if let Some(hover) = hover {
        response.on_hover_text(tr(hover));
    }
    crate::views::form::drag_box(ui, width, drag).changed()
}

/// The explicit flight condition, with the state it resolves to.
fn show_condition_card(window: &mut WingAnalysisState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        card_title(ui, "Flight condition");
        let mut changed = false;
        let mut grid = CellGrid::with_min_column(ui, 200.0, "wing_condition");
        grid.cell(ui, |ui, width| {
            changed |= number_cell(
                ui,
                width,
                "Altitude (m)",
                Some("Pressure altitude: the geopotential altitude of the standard atmosphere the density and speed of sound come from. A flight level in metres is entered as it stands."),
                DragValue::new(&mut window.inputs.condition.altitude_m)
                    .speed(50.0)
                    .range(-500.0..=25_000.0),
            );
        });
        let mut by_mach = matches!(window.inputs.condition.speed, SpeedInput::Mach(_));
        grid.cell(ui, |ui, width| {
            crate::views::form::label_row(ui, tr("Speed stated as"), |_| {});
            ComboBox::from_id_salt("wing_analysis_speed_kind")
                .icon(crate::combo_icon::down_arrow)
                .width(width)
                .selected_text(if by_mach { tr("Mach") } else { tr("True airspeed") })
                .show_ui(ui, |ui| {
                    changed |= ui.selectable_value(&mut by_mach, true, tr("Mach")).changed();
                    changed |= ui
                        .selectable_value(&mut by_mach, false, tr("True airspeed"))
                        .changed();
                });
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
        grid.cell(ui, |ui, width| match &mut window.inputs.condition.speed {
            SpeedInput::Mach(value) => {
                changed |= number_cell(
                    ui,
                    width,
                    "Mach",
                    None,
                    DragValue::new(value).speed(0.01).range(0.01..=0.95),
                );
            }
            SpeedInput::TrueAirspeed(value) => {
                changed |= number_cell(
                    ui,
                    width,
                    "True airspeed",
                    None,
                    DragValue::new(value).speed(1.0).range(1.0..=400.0).suffix(" m/s"),
                );
            }
        });
        let mut by_alpha =
            matches!(window.inputs.condition.attitude, AttitudeInput::AngleOfAttack(_));
        grid.cell(ui, |ui, width| {
            crate::views::form::label_row(ui, tr("Attitude stated as"), |_| {});
            ComboBox::from_id_salt("wing_analysis_attitude_kind")
                .icon(crate::combo_icon::down_arrow)
                .width(width)
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
        grid.cell(ui, |ui, width| match &mut window.inputs.condition.attitude {
            AttitudeInput::AngleOfAttack(value) => {
                changed |= number_cell(
                    ui,
                    width,
                    "Angle of attack",
                    None,
                    DragValue::new(value)
                        .speed(0.1)
                        .range(-ALPHA_LIMIT_DEG..=ALPHA_LIMIT_DEG)
                        .suffix(" deg"),
                );
            }
            AttitudeInput::LiftCoefficient(value) => {
                changed |= number_cell(
                    ui,
                    width,
                    "Lift coefficient",
                    None,
                    DragValue::new(value).speed(0.01).range(-1.5..=2.5),
                );
            }
        });
        grid.finish(ui);
        if changed {
            window.invalidate_inputs();
        }
    });
}

/// Reference quantities, axes and the moment reference every moment uses.
fn show_reference_card(window: &mut WingAnalysisState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let title = crate::theme::card_title(ui, tr("Reference quantities, axes and moment reference"));
        crate::theme::header_label(ui, title).on_hover_text(tr(
            "Geometry axes: x aft from the aircraft datum, y to starboard, z up. Lift and drag are wind-axis; the pitching moment is positive nose-up about the point below.",
        ));
        match window.reference() {
            Some(reference) => {
                let items = [
                    ("Reference area S_ref (m^2)", reference.area_m2),
                    ("Reference span b (m)", reference.span_m),
                    ("Reference chord MAC (m)", reference.chord_m),
                    ("Aspect ratio b^2/S_ref", reference.aspect_ratio()),
                ];
                let mut grid = CellGrid::with_min_column(ui, 200.0, "wing_reference");
                for (label, value) in items {
                    grid.cell(ui, |ui, _| {
                        crate::views::form::label_row(ui, crate::notation::rich(ui, &tr(label)), |_| {});
                        ui.label(RichText::new(format!("{value:.4}")).strong());
                    });
                }
                grid.finish(ui);
            }
            None => {
                ui.label(tr("Reference quantities need an available wing."));
            }
        }
        ui.add_space(4.0);
        let mut changed = false;
        let mut grid = CellGrid::with_min_column(ui, 200.0, "wing_moment_reference");
        for (index, label) in [
            "Moment reference x (m)",
            "Moment reference y (m)",
            "Moment reference z (m)",
        ]
        .into_iter()
        .enumerate()
        {
            grid.cell(ui, |ui, width| {
                changed |= number_cell(
                    ui,
                    width,
                    label,
                    None,
                    DragValue::new(&mut window.inputs.moment_reference_m[index])
                        .speed(0.05)
                        .range(-200.0..=200.0),
                );
            });
        }
        grid.finish(ui);
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
        card_title(ui, "Lattice and sweep");
        let mut changed = false;
        let mut grid = CellGrid::with_min_column(ui, 200.0, "wing_lattice");
        grid.cell(ui, |ui, width| {
            changed |= number_cell(
                ui,
                width,
                "Spanwise panel multiplier",
                Some("Multiplier on each surface's own spanwise subdivision. Above 2 the lattice stops converging in induced drag."),
                DragValue::new(&mut window.inputs.spanwise_resolution).range(1..=2),
            );
        });
        grid.cell(ui, |ui, width| {
            changed |= number_cell(
                ui,
                width,
                "Chordwise panels per strip",
                Some("At one panel the camber line is sampled only at the edges, which turns every section into a flat plate."),
                DragValue::new(&mut window.inputs.chordwise_resolution).range(1..=16),
            );
        });
        grid.cell(ui, |ui, width| {
            changed |= number_cell(
                ui,
                width,
                "Sweep alpha minimum (deg)",
                None,
                DragValue::new(&mut window.inputs.sweep.min_deg)
                    .speed(0.5)
                    .range(-ALPHA_LIMIT_DEG..=ALPHA_LIMIT_DEG),
            );
        });
        grid.cell(ui, |ui, width| {
            changed |= number_cell(
                ui,
                width,
                "Sweep alpha maximum (deg)",
                None,
                DragValue::new(&mut window.inputs.sweep.max_deg)
                    .speed(0.5)
                    .range(-ALPHA_LIMIT_DEG..=ALPHA_LIMIT_DEG),
            );
        });
        grid.cell(ui, |ui, width| {
            changed |= number_cell(
                ui,
                width,
                "Sweep points",
                None,
                DragValue::new(&mut window.inputs.sweep.points).range(2..=41),
            );
        });
        grid.finish(ui);
        if changed {
            window.invalidate_inputs();
        }
        let findings = window.inputs.validate();
        for finding in findings {
            ui.colored_label(ui.visuals().warn_fg_color, tr(&finding));
        }
    });
}
