// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Result cards of a completed competition search: the summary, the chosen
//! components, the geometry, the hard gates that bind, and the M4 analysis.

use alas_uav::optimizer::{CompetitionCandidate, SelectedComponents};
use alas_uav::{optimization_catalog, Severity};
use egui::{Grid, RichText, Ui};

use crate::theme::success_color;
use crate::uav::CompetitionResult;

use super::super::{tr, tr_fields};
use super::airframe_results::airframe_results;
use super::layout_plot_geometry;
use super::presentation::card;
use super::results::aircraft_bom;
use super::uav_fields::{result_measure, result_value};

/// Render the competition result: summary first, then the best candidate.
pub(super) fn competition_result(result: &CompetitionResult, ui: &mut Ui) {
    summary_card(result, ui);
    ui.add_space(8.0);
    match &result.optimum.best {
        Some(best) => {
            components_card(&best.components, ui);
            ui.add_space(8.0);
            geometry_card(best, ui);
            ui.add_space(8.0);
            gates_card(best, ui);
            ui.add_space(8.0);
            airframe_results(&best.report, ui);
            ui.add_space(8.0);
            layout_plot_geometry(best.geometry, &best.design, ui);
            ui.add_space(8.0);
            aircraft_bom(&best.components, ui);
            ui.add_space(8.0);
            findings_card(best, ui);
        }
        None => {
            card(ui, "Best candidate", None, |ui| {
                ui.weak(tr("No candidate produced a complete design."));
            });
        }
    }
}

fn summary_card(result: &CompetitionResult, ui: &mut Ui) {
    let optimum = &result.optimum;
    let feasible = optimum
        .best
        .as_ref()
        .is_some_and(|best| best.evaluation.feasible);
    card(ui, "Competition search result", None, |ui| {
        if feasible {
            ui.colored_label(
                success_color(ui.visuals()),
                tr("Feasible design found: every evaluated hard gate passes."),
            );
        } else {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("No feasible design within the budget; the best candidate is shown for diagnosis only."),
            );
        }
        ui.add_space(6.0);
        Grid::new("uav_competition_summary")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                ui.label(tr("Competition preset"));
                ui.monospace(result.preset_name.as_str());
                ui.end_row();
                ui.label(tr("Search method"));
                ui.label(tr(
                    "Differential evolution over compatible propulsion bundles",
                ));
                ui.end_row();
                ui.label(tr("Random seed"));
                ui.monospace(optimum.seed.to_string());
                ui.end_row();
                result_value(ui, "Evaluations", optimum.evaluations as f64);
                result_value(
                    ui,
                    "Feasible evaluations",
                    optimum.feasible_evaluations as f64,
                );
                result_value(
                    ui,
                    "Compatible propulsion bundles",
                    optimum.compatible_bundles as f64,
                );
                result_value(
                    ui,
                    "Rejected propulsion bundles",
                    optimum.rejected_bundles as f64,
                );
                if let Some(score) = optimum.best.as_ref().and_then(|b| b.evaluation.score) {
                    result_measure(ui, "Competition score", score, "points");
                }
                result_measure(ui, "Search time", optimum.elapsed_s, "s");
                ui.end_row();
            });
        ui.weak(tr("Search time and evaluation rate depend on the machine and are not reproducible; the seed and budget are."));
    });
}

fn components_card(components: &SelectedComponents, ui: &mut Ui) {
    card(
        ui,
        "Chosen components",
        Some("Catalogue records selected by the search, with the source of each record."),
        |ui| {
            let propulsors = components.motor_count;
            let rows = [
                ("Battery", components.battery_id.as_str(), 1),
                ("Motor", components.motor_id.as_str(), propulsors),
                ("Speed controller", components.esc_id.as_str(), propulsors),
                ("Propeller", components.propeller_id.as_str(), propulsors),
                (
                    "Servo",
                    components.servo_id.as_str(),
                    components.servo_count,
                ),
                ("Structural material", components.material_id.as_str(), 1),
                ("Receiver", components.receiver_id.as_str(), 1),
                ("Mission electronics", components.electronics_id.as_str(), 1),
                ("Landing gear", components.landing_gear_id.as_str(), 1),
            ];
            for (label, id, quantity) in rows {
                let record = optimization_catalog()
                    .ok()
                    .and_then(|catalog| catalog.get(id));
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{} x{quantity}: {id}", tr(label)));
                    if let Some(record) = record {
                        ui.hyperlink_to(tr("Open evidence"), &record.provenance.source_url);
                    }
                });
            }
        },
    );
}

fn geometry_card(best: &CompetitionCandidate, ui: &mut Ui) {
    card(ui, "Geometry and sizing", None, |ui| {
        let variables = best.variables;
        Grid::new("uav_competition_geometry")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                result_measure(ui, "Wing area", variables.wing_area_m2, "m2");
                result_value(ui, "Wing aspect ratio", variables.wing_aspect_ratio);
                result_measure(ui, "Wing span", best.geometry.wing.span_m, "m");
                result_measure(ui, "Fuselage length", variables.fuselage_length_m, "m");
                result_value(
                    ui,
                    "Wing leading-edge fraction",
                    variables.wing_leading_edge_fraction,
                );
                result_value(
                    ui,
                    "Horizontal-tail volume",
                    variables.horizontal_tail_volume,
                );
                result_value(ui, "Vertical-tail volume", variables.vertical_tail_volume);
                if let Some(mass) = best.evaluation.takeoff_mass_kg {
                    result_measure(ui, "Takeoff mass", mass, "kg");
                }
                if let Some(distance) = best.evaluation.takeoff_distance_m {
                    result_measure(ui, "Take-off distance", distance, "m");
                }
            });
    });
}

fn gates_card(best: &CompetitionCandidate, ui: &mut Ui) {
    card(
        ui,
        "Hard gates",
        Some("Gates the search enforces. A failed gate contributes its normalised violation to the search ordering."),
        |ui| {
            let evaluation = &best.evaluation;
            if evaluation.failed_gates.is_empty() {
                ui.colored_label(success_color(ui.visuals()), tr("No hard gate fails."));
            } else {
                ui.label(RichText::new(tr("Gates that bind (failed)")).strong());
                for gate in &evaluation.failed_gates {
                    ui.colored_label(ui.visuals().error_fg_color, gate.as_str());
                }
            }
            if !evaluation.unevaluated_gates.is_empty() {
                ui.add_space(4.0);
                ui.label(tr("Gates not evaluated (missing inputs):"));
                for gate in &evaluation.unevaluated_gates {
                    ui.weak(gate.as_str());
                }
            }
            if let Some(note) = &evaluation.note {
                ui.add_space(4.0);
                ui.weak(note.as_str());
            }
            ui.add_space(4.0);
            result_value(ui, "Total violation", evaluation.violation);
        },
    );
}

fn findings_card(best: &CompetitionCandidate, ui: &mut Ui) {
    card(ui, "Physical findings", None, |ui| {
        if best.report.findings.is_empty() {
            ui.label(tr("All requested preliminary constraints passed."));
            return;
        }
        for finding in &best.report.findings {
            let color = if finding.severity == Severity::Failure {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().warn_fg_color
            };
            ui.colored_label(color, &finding.message);
        }
        ui.add_space(4.0);
        ui.weak(tr_fields(
            "{count} findings listed; a missing input is unverified, not proof of failure.",
            &[("count", best.report.findings.len().to_string())],
        ));
    });
}
