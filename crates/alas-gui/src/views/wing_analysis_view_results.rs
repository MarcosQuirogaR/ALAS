// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Results tab: the solved point, the spanwise lift distribution, the swept
//! curves, the static stability of the modelled configuration, and the solver
//! evidence.
//!
//! Nothing here is computed: every number is read from the outcome the worker
//! returned, and the tab refuses to show one whose revision has been
//! superseded by an input edit.

use egui::{vec2, Color32, Grid, RichText, ScrollArea, Ui};

use alas_aero::wing_analysis::{WingAnalysisOutcome, OMITTED_COMPONENTS};

use super::draw::{line_chart, Series};
use super::wing_analysis::with_window;
use crate::views::{tr, tr_fields};

/// Curve colours, chosen for the three themes' canvases alike.
const LIFT_COLOR: Color32 = Color32::from_rgb(37, 99, 235);
const REFERENCE_COLOR: Color32 = Color32::from_rgb(148, 163, 184);
const MOMENT_COLOR: Color32 = Color32::from_rgb(217, 119, 6);

/// Chart height bounds, in points.
const CHART_MIN_HEIGHT: f32 = 170.0;
const CHART_MAX_HEIGHT: f32 = 280.0;

pub(crate) fn show_results_tab(ui: &mut Ui) {
    let (outcome, current, configuration) = with_window(|window| {
        (
            window.result.clone(),
            window.result_is_current(),
            super::configuration_label(window),
        )
    });
    let Some(outcome) = outcome else {
        ui.label(tr(
            "No wing analysis result yet. Set the condition in Setup and run the analysis.",
        ));
        return;
    };
    ScrollArea::vertical()
        .id_salt("wing_analysis_results_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if !current {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr("These numbers belong to earlier inputs; run the analysis again."),
                );
            }
            show_point_card(&outcome, &configuration, ui);
            ui.add_space(8.0);
            show_span_load_card(&outcome, ui);
            ui.add_space(8.0);
            show_sweep_card(&outcome, ui);
            ui.add_space(8.0);
            show_stability_card(&outcome, &configuration, ui);
            ui.add_space(8.0);
            show_evidence_card(&outcome, ui);
        });
}

/// The solved point: condition, forces and coefficients.
fn show_point_card(outcome: &WingAnalysisOutcome, configuration: &str, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            RichText::new(tr_fields(
                "Solved point of the modelled {configuration}",
                &[("configuration", configuration.to_lowercase())],
            ))
            .strong()
            .size(15.0),
        );
        if outcome.alpha_from_lift_target {
            ui.label(
                RichText::new(tr(
                    "The angle of attack was solved from the lift-coefficient target.",
                ))
                .weak()
                .small(),
            );
        }
        let condition = &outcome.condition;
        let rows: [(&str, String); 12] = [
            (
                "Angle of attack (deg)",
                format!("{:.3}", condition.alpha_deg),
            ),
            ("Lift coefficient CL", format!("{:.4}", outcome.cl)),
            (
                "Induced drag coefficient CDi",
                format!("{:.5}", outcome.cd_induced),
            ),
            (
                "Pitching-moment coefficient Cm",
                format!("{:.5}", outcome.cm_pitch),
            ),
            ("Lift (N)", format!("{:.1}", outcome.lift_n)),
            ("Induced drag (N)", format!("{:.1}", outcome.induced_drag_n)),
            (
                "Pitching moment (N m)",
                format!("{:.1}", outcome.pitch_moment_n_m),
            ),
            (
                "Span efficiency e",
                outcome.span_efficiency.map_or_else(
                    || tr("not resolvable at this lift"),
                    |value| format!("{value:.4}"),
                ),
            ),
            (
                "True airspeed (m/s)",
                format!("{:.2}", condition.true_airspeed_m_s),
            ),
            ("Mach", format!("{:.4}", condition.mach)),
            (
                "Dynamic pressure (Pa)",
                format!("{:.1}", condition.dynamic_pressure_pa),
            ),
            (
                "Reynolds number on the MAC",
                format!("{:.3e}", condition.reynolds_chord),
            ),
        ];
        let columns = if ui.available_width() >= 760.0 { 2 } else { 1 };
        Grid::new("wing_analysis_point")
            .num_columns(2 * columns)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                for (index, (label, value)) in rows.iter().enumerate() {
                    ui.label(tr(label));
                    ui.label(RichText::new(value).strong());
                    if (index + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
        ui.label(
            RichText::new(tr_fields(
                "Induced drag only; this configuration omits {components}.",
                &[("components", OMITTED_COMPONENTS.join(", "))],
            ))
            .weak()
            .small(),
        );
    });
}

/// The spanwise lift distribution, against the elliptic loading of the same
/// total lift for comparison.
fn show_span_load_card(outcome: &WingAnalysisOutcome, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            RichText::new(tr("Spanwise lift distribution of the main wing"))
                .strong()
                .size(15.0),
        );
        if outcome.span_load.is_empty() {
            ui.label(tr("The solve returned no spanwise stations."));
            return;
        }
        let solved: Vec<[f64; 2]> = outcome
            .span_load
            .iter()
            .map(|station| [station.y_over_semispan, station.lift_per_span_n_m])
            .collect();
        let semispan = 0.5 * outcome.reference.span_m;
        let root = if semispan > 0.0 {
            4.0 * (0.5 * outcome.lift_n) / (std::f64::consts::PI * semispan)
        } else {
            0.0
        };
        let elliptic: Vec<[f64; 2]> = solved
            .iter()
            .map(|point| [point[0], root * (1.0 - point[0] * point[0]).max(0.0).sqrt()])
            .collect();
        let width = ui.available_width().max(220.0);
        let height = (width * 0.32).clamp(CHART_MIN_HEIGHT, CHART_MAX_HEIGHT);
        line_chart(
            ui,
            vec2(width, height),
            &[
                Series {
                    label: tr("Solved lift per unit span"),
                    color: LIFT_COLOR,
                    points: &solved,
                },
                Series {
                    label: tr("Elliptic loading of the same total lift"),
                    color: REFERENCE_COLOR,
                    points: &elliptic,
                },
            ],
            &tr("Spanwise station y / (b/2), positive to starboard"),
            &tr("N/m"),
        );
        let peak = outcome
            .span_load
            .iter()
            .fold(f64::NEG_INFINITY, |peak, station| {
                peak.max(station.section_cl)
            });
        ui.label(
            RichText::new(tr_fields(
                "{count} strips; highest section lift coefficient {peak}.",
                &[
                    ("count", outcome.span_load.len().to_string()),
                    ("peak", format!("{peak:.3}")),
                ],
            ))
            .weak()
            .small(),
        );
    });
}

/// Lift and pitching moment against angle of attack.
fn show_sweep_card(outcome: &WingAnalysisOutcome, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            RichText::new(tr("Lift and pitching moment against angle of attack"))
                .strong()
                .size(15.0),
        );
        if outcome.sweep.is_empty() {
            ui.label(tr("The sweep evaluated no angles."));
            return;
        }
        let lift: Vec<[f64; 2]> = outcome
            .sweep
            .iter()
            .map(|point| [point.alpha_deg, point.cl])
            .collect();
        let moment: Vec<[f64; 2]> = outcome
            .sweep
            .iter()
            .map(|point| [point.alpha_deg, point.cm_pitch])
            .collect();
        let drag: Vec<[f64; 2]> = outcome
            .sweep
            .iter()
            .map(|point| [point.cd_induced, point.cl])
            .collect();
        let width = ui.available_width().max(220.0);
        let height = (width * 0.30).clamp(CHART_MIN_HEIGHT, CHART_MAX_HEIGHT);
        line_chart(
            ui,
            vec2(width, height),
            &[
                Series {
                    label: tr("CL"),
                    color: LIFT_COLOR,
                    points: &lift,
                },
                Series {
                    label: tr("Cm about the moment reference"),
                    color: MOMENT_COLOR,
                    points: &moment,
                },
            ],
            &tr("Angle of attack (deg)"),
            &tr("CL and Cm"),
        );
        ui.add_space(6.0);
        line_chart(
            ui,
            vec2(width, height),
            &[Series {
                label: tr("Induced-drag polar"),
                color: LIFT_COLOR,
                points: &drag,
            }],
            &tr("Induced drag coefficient CDi"),
            &tr("CL"),
        );
    });
}

/// Static stability of the modelled configuration, or why there is none.
fn show_stability_card(outcome: &WingAnalysisOutcome, configuration: &str, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            RichText::new(tr("Static stability of the modelled configuration"))
                .strong()
                .size(15.0),
        );
        let Some(stability) = outcome.stability else {
            ui.label(tr(
                "Stability is reported for the wing-empennage configuration. Switch on Include empennage in Setup and run the analysis again.",
            ));
            return;
        };
        let reference = outcome.reference.moment_reference_m;
        ui.label(tr_fields(
            "About the stated moment reference at x = {x} m, y = {y} m, z = {z} m in geometry axes.",
            &[
                ("x", format!("{:.3}", reference[0])),
                ("y", format!("{:.3}", reference[1])),
                ("z", format!("{:.3}", reference[2])),
            ],
        ));
        Grid::new("wing_analysis_stability")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                for (label, value) in [
                    ("CL_alpha (1/rad)", stability.cl_alpha_per_rad),
                    ("Cm_alpha (1/rad)", stability.cm_alpha_per_rad),
                    ("CY_beta (1/rad)", stability.cy_beta_per_rad),
                    ("Cn_beta (1/rad)", stability.cn_beta_per_rad),
                    ("Cl_beta (1/rad)", stability.cl_beta_per_rad),
                    ("Cm_q (1/rad)", stability.cm_q_per_rad),
                    ("Neutral point x (m)", stability.neutral_point_x_m),
                    ("Static margin (MAC)", stability.static_margin),
                ] {
                    ui.label(tr(label));
                    ui.label(RichText::new(format!("{value:.5}")).strong());
                    ui.end_row();
                }
            });
        let longitudinal = if stability.cm_alpha_per_rad < 0.0 {
            tr("Cm_alpha is negative: this configuration is longitudinally stable about that point.")
        } else {
            tr("Cm_alpha is not negative: this configuration is longitudinally unstable about that point.")
        };
        ui.label(longitudinal);
        ui.label(
            RichText::new(tr_fields(
                "These derivatives belong to the modelled {configuration} alone. With the fuselage, nacelles and propulsion omitted they are not aircraft stability derivatives, and the directional and lateral terms in particular miss the fuselage contribution.",
                &[("configuration", configuration.to_lowercase())],
            ))
            .weak()
            .small(),
        );
    });
}

/// Per-surface lift shares and the solver evidence for this result.
fn show_evidence_card(outcome: &WingAnalysisOutcome, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Surfaces and solver evidence")).strong().size(15.0));
        Grid::new("wing_analysis_surfaces")
            .num_columns(3)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.label(RichText::new(tr("Surface")).weak().small());
                ui.label(RichText::new(tr("Lift (N)")).weak().small());
                ui.label(RichText::new(tr("Panels")).weak().small());
                ui.end_row();
                for surface in &outcome.modelled_surfaces {
                    ui.label(&surface.name);
                    ui.label(format!("{:.1}", surface.lift_n));
                    ui.label(surface.panel_count.to_string());
                    ui.end_row();
                }
            });
        let diagnostics = &outcome.diagnostics;
        ui.label(
            RichText::new(tr_fields(
                "{panels} panels, spanwise multiplier {spanwise}, {chordwise} chordwise panels per strip; normalized solve residual {residual}, pivot ratio {pivot}.",
                &[
                    ("panels", diagnostics.panel_count.to_string()),
                    ("spanwise", diagnostics.spanwise_resolution.to_string()),
                    ("chordwise", diagnostics.chordwise_resolution.to_string()),
                    ("residual", format!("{:.2e}", diagnostics.residual)),
                    ("pivot", format!("{:.2e}", diagnostics.pivot_ratio)),
                ],
            ))
            .weak()
            .small(),
        );
    });
}
