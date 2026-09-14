// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fixed-wing UAV catalogue, sizing, layout, and shared-core verification page.
//!
//! The page is a guided workflow rather than one uninterrupted form. Inputs
//! stay editable in their own section, while evidence boundaries and the
//! status of the selected design convention remain visible before a run.

use alas_uav::optimizer::{GeneratedGeometry, OptimizedUav, RejectedUav};
use alas_uav::{optimization_catalog, optimized_aircraft_bom, Severity, UavDesign};
use egui::{Color32, RichText, ScrollArea, Sense, Stroke, Ui, Vec2};

use crate::state::AppState;
use crate::theme::success_color;
use crate::uav::{UavSection, UavWorkflowOutcome, UavWorkflowState};

use super::{tr, tr_fields};

mod airframe;
mod finding;
mod presentation;
mod results;
mod sections;
mod uav_fields;
mod workflow;

use airframe::{airframe_inputs, shared_core_inputs};
use finding::finding_label;
use presentation::card;
use results::{aircraft_bom, electrical_mission};
use sections::{component_inputs, mission_inputs, propulsion_inputs};
use uav_fields::{result_measure, result_value};
use workflow::{
    action_bar, execution_status, outcome_is_verified, release_blocker, section_intro,
    topology_selector, workflow_header, UAV_RELEASE_BLOCKED,
};

/// Render the complete fixed-wing UAV workflow.
pub fn show_uav_view(state: &mut AppState, ui: &mut Ui) {
    if UAV_RELEASE_BLOCKED {
        release_blocker(ui);
        return;
    }
    ScrollArea::vertical()
        .id_salt("uav_workflow_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            workflow_header(&mut state.uav, ui);
            ui.add_space(8.0);

            let active_section = state.uav.active_section;
            section_intro(active_section, ui);
            ui.add_space(8.0);
            let running = state.uav.is_running();
            match active_section {
                UavSection::Mission => {
                    ui.add_enabled_ui(!running, |ui| mission_inputs(&mut state.uav, ui));
                }
                UavSection::Hardware => {
                    ui.add_enabled_ui(!running, |ui| component_inputs(&mut state.uav, ui));
                }
                UavSection::Propulsion => {
                    ui.add_enabled_ui(!running, |ui| propulsion_inputs(&mut state.uav, ui));
                }
                UavSection::Airframe => {
                    ui.add_enabled_ui(!running, |ui| {
                        topology_selector(&mut state.uav, ui);
                        ui.add_space(6.0);
                        airframe_inputs(&mut state.uav, ui);
                    });
                }
                UavSection::Verification => {
                    ui.add_enabled_ui(!running, |ui| shared_core_inputs(&mut state.uav, ui));
                }
                UavSection::Results => outcome(&state.uav, ui),
            }
            ui.add_space(8.0);
            action_bar(&mut state.uav, ui);
            ui.add_space(8.0);
            execution_status(&state.uav, ui);
        });
}

fn outcome(state: &UavWorkflowState, ui: &mut Ui) {
    if let Some(topology) = state.last_completed_topology {
        card(ui, "Design convention", None, |ui| {
            ui.label(tr_fields(
                "Design convention used by this result: {topology}",
                &[("topology", tr(topology.label()))],
            ));
            if topology != state.topology {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr("The selected convention differs from this result; run again to update it."),
                );
            }
        });
        ui.add_space(8.0);
    }
    if let Some(mission) = &state.last_electrical_mission {
        electrical_mission(mission, ui);
        ui.add_space(8.0);
    }
    match &state.outcome {
        UavWorkflowOutcome::NotRun => {
            card(ui, "UAV result", None, |ui| {
                ui.weak(tr("No UAV search has been run."));
            });
        }
        UavWorkflowOutcome::NoFeasibleDesign(summary) => {
            card(ui, "UAV result", None, |ui| {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr_fields(
                        "No verified design after {count} candidates.",
                        &[("count", summary.evaluated_candidates.to_string())],
                    ),
                );
                ui.add_space(4.0);
                for rejection in &summary.rejections {
                    ui.label(format!(
                        "{}: {}",
                        tr(finding_label(rejection.kind)),
                        rejection.candidates
                    ));
                }
                if !summary.examples.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(tr("Representative causes")).strong());
                    for example in &summary.examples {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!(
                                "{} ({}): {}",
                                tr(finding_label(example.kind)),
                                example.subject,
                                example.message
                            ),
                        );
                    }
                }
                ui.add_space(4.0);
                ui.weak(tr(
                    "A missing-data rejection is an unverified constraint, not proof that the hardware is physically impossible.",
                ));
            });
            if let Some(rejected) = &summary.best_evaluated {
                ui.add_space(8.0);
                card(
                    ui,
                    "Best evaluated review artifact",
                    Some(
                        "This geometry was generated and evaluated but did not pass verification. It is shown for diagnosis only, not as a design recommendation.",
                    ),
                    |ui| {
                        if !rejected.report.findings.is_empty() {
                            ui.label(RichText::new(tr("Blocking findings")).strong());
                            for finding in &rejected.report.findings {
                                let color = if finding.severity == Severity::Failure {
                                    ui.visuals().error_fg_color
                                } else {
                                    ui.visuals().warn_fg_color
                                };
                                ui.colored_label(color, &finding.message);
                            }
                        }
                    },
                );
                ui.add_space(8.0);
                rejected_metrics(rejected, ui);
                ui.add_space(8.0);
                layout_plot_rejected(rejected, ui);
            } else {
                ui.add_space(8.0);
                card(ui, "Generated planform and installed layout", None, |ui| {
                    ui.weak(tr(
                        "No airframe figure is shown because every candidate stopped before a physically complete geometry could be generated; the electrical mission above remains the valid result artifact.",
                    ));
                });
            }
        }
        UavWorkflowOutcome::PreliminaryFeasible {
            optimized,
            shared_core,
        } => {
            let production_verified = outcome_is_verified(&state.outcome);
            card(ui, "UAV result", None, |ui| {
                ui.colored_label(
                    if production_verified {
                        success_color(ui.visuals())
                    } else {
                        ui.visuals().warn_fg_color
                    },
                    if production_verified {
                        tr("Preliminary optimizer and production-core verification passed")
                    } else {
                        tr("Preliminary geometry generated; production-core verification did not pass")
                    },
                );
            });
            ui.add_space(8.0);
            metrics(optimized, ui);
            ui.add_space(8.0);
            layout_plot(optimized, ui);
            ui.add_space(8.0);
            component_provenance(optimized, ui);
            ui.add_space(8.0);
            aircraft_bom(optimized, ui);
            ui.add_space(8.0);
            physical_findings(optimized, ui);
            ui.add_space(8.0);
            shared_core_comparison(shared_core, ui);
        }
    }
}

fn component_provenance(optimized: &OptimizedUav, ui: &mut Ui) {
    card(ui, "Selected components and provenance", None, |ui| {
        let bom = optimized_aircraft_bom(&optimized.components);
        for line in &bom.lines {
            if let Some(record) = optimization_catalog()
                .ok()
                .and_then(|catalog| catalog.get(&line.selection.component_id))
            {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!(
                        "{} x{}: {}",
                        tr(line.category.label()),
                        line.selection.quantity,
                        record.model
                    ));
                    ui.hyperlink_to(tr("Open evidence"), &record.provenance.source_url);
                });
            } else {
                ui.label(format!(
                    "{} x{}: {}",
                    tr(line.category.label()),
                    line.selection.quantity,
                    line.selection.component_id
                ));
            }
        }
    });
}

fn physical_findings(optimized: &OptimizedUav, ui: &mut Ui) {
    card(ui, "Physical findings", None, |ui| {
        if optimized.report.findings.is_empty() {
            ui.label(tr("All requested preliminary constraints passed."));
            return;
        }
        for finding in &optimized.report.findings {
            let color = if finding.severity == Severity::Failure {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().warn_fg_color
            };
            ui.colored_label(color, &finding.message);
        }
    });
}

fn shared_core_comparison(
    shared_core: &Result<Box<alas_uav::SharedCoreVerification>, alas_uav::SharedCoreFailure>,
    ui: &mut Ui,
) {
    card(ui, "Shared-core comparison", None, |ui| match shared_core {
        Ok(verification) => {
            let assessment = &verification.assessment;
            match verification.lift_verdict {
                alas_uav::SharedCoreLiftVerdict::Passed => {
                    ui.colored_label(
                        success_color(ui.visuals()),
                        tr("Shared-core lift verdict: passed"),
                    );
                }
                alas_uav::SharedCoreLiftVerdict::InsufficientLift => {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        tr("Shared-core lift verdict: insufficient lift"),
                    );
                }
            }
            egui::Grid::new("uav_shared_result")
                .num_columns(2)
                .spacing([20.0, 8.0])
                .show(ui, |ui| {
                    result_value(
                        ui,
                        "Required one-g CL",
                        assessment.required_lift_coefficient,
                    );
                    result_value(ui, "VLM CL", assessment.vlm.cl_lift);
                    result_value(ui, "VLM induced CD", assessment.vlm.cd_drag);
                    result_value(
                        ui,
                        "Preliminary induced CD at VLM CL",
                        assessment.preliminary_induced_drag_coefficient,
                    );
                    result_value(
                        ui,
                        "Induced-CD discrepancy",
                        assessment.induced_drag_coefficient_delta,
                    );
                    result_measure(ui, "Lift margin", assessment.lift_margin_n, "N");
                    result_value(
                        ui,
                        "Preliminary parasite CD0 (not in VLM)",
                        assessment.preliminary_zero_lift_drag_coefficient,
                    );
                });
        }
        Err(error) => {
            ui.colored_label(ui.visuals().error_fg_color, error.to_string());
        }
    });
}

fn metrics(optimized: &OptimizedUav, ui: &mut Ui) {
    let geometry = optimized.geometry;
    card(ui, "Preliminary design metrics", None, |ui| {
        egui::Grid::new("uav_metrics_grid")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                result_measure(ui, "Takeoff mass", optimized.metrics.takeoff_mass_kg, "kg");
                result_measure(
                    ui,
                    "Mission energy",
                    optimized.metrics.mission_energy_wh,
                    "Wh",
                );
                result_value(
                    ui,
                    "Propulsive efficiency",
                    optimized.metrics.propulsive_efficiency,
                );
                result_measure(ui, "Wing area", geometry.wing.area_m2, "m2");
                result_measure(ui, "Wing span", geometry.wing.span_m, "m");
                result_measure(ui, "Fuselage length", geometry.fuselage.length_m, "m");
                result_measure(
                    ui,
                    "Horizontal-tail area",
                    geometry.empennage.horizontal_area_m2,
                    "m2",
                );
                result_measure(
                    ui,
                    "Vertical-tail area",
                    geometry.empennage.vertical_area_m2,
                    "m2",
                );
                result_measure(ui, "Landing-gear track", geometry.landing_gear.track_m, "m");
                result_measure(
                    ui,
                    "Landing-gear wheelbase",
                    geometry.landing_gear.wheelbase_m,
                    "m",
                );
                result_measure(
                    ui,
                    "Minimum gear-leg length",
                    geometry.landing_gear.minimum_leg_length_m,
                    "m",
                );
            });
    });
}

fn layout_plot(optimized: &OptimizedUav, ui: &mut Ui) {
    layout_plot_geometry(optimized.geometry, &optimized.design, ui);
}

fn layout_plot_rejected(rejected: &RejectedUav, ui: &mut Ui) {
    layout_plot_geometry(rejected.geometry, &rejected.design, ui);
}

fn rejected_metrics(rejected: &RejectedUav, ui: &mut Ui) {
    let Some(metrics) = rejected.metrics else {
        card(ui, "Preliminary design metrics", None, |ui| {
            ui.weak(tr(
                "Objective metrics were unavailable for this rejected candidate.",
            ));
        });
        return;
    };
    card(ui, "Preliminary design metrics", None, |ui| {
        egui::Grid::new("uav_rejected_metrics_grid")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                result_measure(ui, "Takeoff mass", metrics.takeoff_mass_kg, "kg");
                result_measure(ui, "Mission energy", metrics.mission_energy_wh, "Wh");
                result_value(ui, "Propulsive efficiency", metrics.propulsive_efficiency);
                result_value(ui, "Objective score", metrics.score);
                result_measure(ui, "Wing area", rejected.geometry.wing.area_m2, "m2");
                result_measure(ui, "Wing span", rejected.geometry.wing.span_m, "m");
                result_measure(
                    ui,
                    "Fuselage length",
                    rejected.geometry.fuselage.length_m,
                    "m",
                );
            });
    });
}

fn layout_plot_geometry(geometry: GeneratedGeometry, design: &UavDesign, ui: &mut Ui) {
    card(ui, "Generated planform and installed layout", None, |ui| {
        let available_width = ui.available_width().max(320.0);
        let desired = Vec2::new(available_width, 300.0);
        let (response, painter) = ui.allocate_painter(desired, Sense::hover());
        let rect = response.rect.shrink(16.0);
        let length = geometry.fuselage.length_m;
        let span = geometry.wing.span_m;
        let scale = (rect.width() / length as f32).min(rect.height() / span as f32);
        let center = rect.center();
        let point = |x_m: f64, y_m: f64| {
            egui::pos2(
                center.x + (x_m - 0.5 * length) as f32 * scale,
                center.y - y_m as f32 * scale,
            )
        };
        let line_color = ui.visuals().text_color();
        let accent = ui.visuals().hyperlink_color;
        let weak = ui.visuals().weak_text_color();
        let wing = geometry.wing;
        let wing_front = wing.leading_edge_x_m;
        let wing_back = wing_front + wing.mean_chord_m;
        painter.rect_stroke(
            egui::Rect::from_two_pos(point(wing_front, -0.5 * span), point(wing_back, 0.5 * span)),
            0.0,
            Stroke::new(2.0_f32, accent),
        );
        let fuselage_half = 0.5 * geometry.fuselage.diameter_m;
        painter.rect_filled(
            egui::Rect::from_two_pos(point(0.0, -fuselage_half), point(length, fuselage_half)),
            3.0,
            Color32::from_rgba_unmultiplied(weak.r(), weak.g(), weak.b(), 80),
        );
        let tail = geometry.empennage;
        if tail.horizontal_area_m2 > 0.0 && tail.horizontal_span_m > 0.0 {
            let tail_x = 0.9 * length;
            let h_chord = tail.horizontal_area_m2 / tail.horizontal_span_m;
            painter.rect_stroke(
                egui::Rect::from_two_pos(
                    point(tail_x - 0.25 * h_chord, -0.5 * tail.horizontal_span_m),
                    point(tail_x + 0.75 * h_chord, 0.5 * tail.horizontal_span_m),
                ),
                0.0,
                Stroke::new(1.5_f32, line_color),
            );
        }
        let main_x = wing.leading_edge_x_m + 0.25 * wing.mean_chord_m;
        let nose_x = main_x - geometry.landing_gear.wheelbase_m;
        for (x_m, y_m) in [
            (main_x, -0.5 * geometry.landing_gear.track_m),
            (main_x, 0.5 * geometry.landing_gear.track_m),
            (nose_x, 0.0),
        ] {
            painter.circle_filled(point(x_m, y_m), 4.0, line_color);
        }
        for placement in std::iter::once(design.battery.placement)
            .chain(std::iter::once(design.motor.placement))
            .chain(std::iter::once(design.esc.placement))
            .chain(std::iter::once(design.propeller.placement))
            .chain(design.additional_propulsors.iter().flat_map(|propulsor| {
                [
                    propulsor.motor.placement,
                    propulsor.esc.placement,
                    propulsor.propeller.placement,
                ]
            }))
            .chain(design.servos.iter().map(|item| item.placement))
            .chain(design.other_items.iter().map(|item| item.placement))
        {
            painter.circle_filled(
                point(placement.center_x_m, placement.center_y_m),
                3.0,
                ui.visuals().warn_fg_color,
            );
        }
    });
}
