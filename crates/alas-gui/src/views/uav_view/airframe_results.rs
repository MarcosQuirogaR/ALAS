// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Result cards for the M4 airframe analysis (static margin, trim, spar) and
//! the M5 optimizer score of the preliminary UAV result. Every value carries
//! its unit; the analysis is labelled as preliminary.

use alas_uav::airframe::analysis::{AirframeAnalysis, MarginVerdict};
use alas_uav::UavReport;
use egui::{Grid, Ui};

use super::super::tr;
use super::presentation::card;
use super::uav_fields::{result_measure, result_value};

/// Render the stability, trim and spar cards of the selected result.
pub(super) fn airframe_results(report: &UavReport, ui: &mut Ui) {
    match &report.airframe {
        Some(analysis) => {
            stability_card(analysis, ui);
            ui.add_space(8.0);
            trim_card(analysis, ui);
            ui.add_space(8.0);
            spar_card(analysis, ui);
        }
        None => {
            card(ui, "Airframe analysis", None, |ui| {
                ui.weak(tr("No airframe analysis for this result."));
            });
        }
    }
}

fn verdict_text(verdict: MarginVerdict) -> &'static str {
    match verdict {
        MarginVerdict::Below => "Below the 5-15 % screening band",
        MarginVerdict::Within => "Inside the 5-15 % screening band",
        MarginVerdict::Above => "Above the 5-15 % screening band",
    }
}

fn stability_card(analysis: &AirframeAnalysis, ui: &mut Ui) {
    card(
        ui,
        "Static stability (preliminary)",
        Some("Neutral point from the inviscid lattice; the margin is taken at the loaded centre of gravity."),
        |ui| {
            Grid::new("uav_stability_grid")
                .num_columns(2)
                .spacing([20.0, 8.0])
                .show(ui, |ui| {
                    result_measure(
                        ui,
                        "Static margin (loaded CG)",
                        analysis.static_margin * 100.0,
                        "% MAC",
                    );
                    ui.label(tr(verdict_text(analysis.static_margin_verdict)));
                    ui.end_row();
                    result_measure(ui, "Loaded CG x (body axes)", analysis.cg_x_m, "m");
                    result_measure(ui, "Neutral point x", analysis.neutral_point_x_m, "m");
                    result_measure(ui, "Mean aerodynamic chord", analysis.mac_m, "m");
                    result_measure(
                        ui,
                        "Lift-curve slope",
                        analysis.cl_alpha_per_rad,
                        "1/rad",
                    );
                    if let Some(volume) = analysis.tail_volume_horizontal {
                        result_measure(ui, "Horizontal-tail volume", volume, "");
                    }
                    if let Some(volume) = analysis.tail_volume_vertical {
                        result_measure(ui, "Vertical-tail volume", volume, "");
                    }
                });
            if analysis.estimate {
                ui.weak(tr("Estimates, not calibrated against flight data"));
            }
        },
    );
}

fn trim_card(analysis: &AirframeAnalysis, ui: &mut Ui) {
    card(ui, "Cruise trim (preliminary)", None, |ui| {
        let Some(trim) = &analysis.cruise_trim else {
            ui.weak(tr("No cruise trim: no horizontal tail or trim setup."));
            return;
        };
        Grid::new("uav_trim_grid")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                result_measure(
                    ui,
                    "Elevator deflection",
                    trim.trim.delta_rad.to_degrees(),
                    "deg",
                );
                result_measure(ui, "Tail lift", trim.tail_lift_n, "N");
                result_measure(ui, "Required lift coefficient", trim.cl_required, "");
                result_measure(ui, "Trim drag", trim.trim_drag_n, "N");
                ui.label(tr("Elevator within its stops"));
                ui.label(tr(if trim.trim.within_stops { "Yes" } else { "No" }));
                ui.end_row();
            });
    });
}

fn spar_card(analysis: &AirframeAnalysis, ui: &mut Ui) {
    card(
        ui,
        "Wing spar at limit load (preliminary)",
        Some("Margins of safety are against the allowables with the safety factor applied to the limit loads; no buckling, joint or wingtip test is included."),
        |ui| {
            let Some(spar) = &analysis.spar else {
                ui.weak(tr("No spar analysis for this result."));
                return;
            };
            Grid::new("uav_spar_grid")
                .num_columns(2)
                .spacing([20.0, 8.0])
                .show(ui, |ui| {
                    result_measure(ui, "Limit lift", spar.limit_lift_n, "N");
                    result_measure(ui, "Root bending moment", spar.root_moment_limit_nm, "N m");
                    result_measure(ui, "Tip deflection", spar.tip_deflection_m * 1000.0, "mm");
                    result_value(ui, "Tip deflection over semispan", spar.tip_deflection_over_semispan);
                    result_measure(ui, "Spar mass (both semispans)", spar.spar_mass_kg, "kg");
                    result_value(ui, "Bending margin of safety", spar.bending_margin_of_safety);
                    result_value(ui, "Shear margin of safety", spar.shear_margin_of_safety);
                });
            if spar.estimate {
                ui.weak(tr("Spar estimate: placeholder laminate data, not a design allowable"));
            }
        },
    );
}
