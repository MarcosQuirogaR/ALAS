// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Layout-derived aircraft metrics shown above the result figures.

use alas_payload::layout::{LayoutSummary, PayloadLayout};
use alas_pipeline::feasibility::{FuelCapacityEvidence, MissionFuelStatus};
use alas_pipeline::AnalysisReport;
use egui::{RichText, Ui};

use crate::state::AppState;
use crate::views::{tr, tr_fields};

use super::format_cg_pct_mac;

fn stat_tile(ui: &mut Ui, label: &str, value: String) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.vertical(|ui| {
            ui.label(RichText::new(tr(label)).weak().small());
            ui.add(egui::Label::new(RichText::new(value).strong().size(16.0)).wrap());
        });
    });
}

fn summary_column_count(available_width: f32) -> usize {
    ((available_width / 260.0).floor() as usize).clamp(1, 3)
}

fn show_stat_tiles(ui: &mut Ui, metrics: &[(&str, String)]) {
    if metrics.is_empty() {
        return;
    }
    let columns = summary_column_count(ui.available_width()).min(metrics.len());
    for row in metrics.chunks(columns) {
        ui.columns(columns, |columns| {
            for (index, (label, value)) in row.iter().enumerate() {
                stat_tile(&mut columns[index], label, value.clone());
            }
        });
        ui.add_space(8.0);
    }
}

pub(super) fn show_summary(state: &AppState, ui: &mut Ui, result: &alas_pipeline::PipelineResult) {
    let mut metrics = vec![
        ("Preset", state.active_preset.clone()),
        (
            "Physical status",
            if result.feasibility.is_feasible() {
                tr("Feasible under implemented checks")
            } else {
                tr_fields(
                    "{count} finding(s)",
                    &[("count", result.feasibility.findings.len().to_string())],
                )
            },
        ),
    ];
    if let Some(baseline) = &result.baseline_report {
        metrics.push((
            "Baseline static margin",
            format!("{:.1}%", baseline.static_margin * 100.0),
        ));
        metrics.push(("Baseline CG", format_cg_pct_mac(baseline.cg_pct_mac)));
    }
    if let Some(optimized) = &result.optimized_report {
        metrics.push((
            "Optimized static margin",
            format!("{:.1}%", optimized.static_margin * 100.0),
        ));
        let envelope = match optimized.cg_envelope_ok {
            Some(true) => "OK".to_owned(),
            Some(false) => tr("Violation"),
            None => "-".to_owned(),
        };
        metrics.push(("CG envelope", envelope));
    }
    metrics.extend(aircraft_summary_metrics(result));
    if let Some(layout) = result_payload_layout(result) {
        metrics.extend(payload_summary_metrics(layout));
    }
    show_stat_tiles(ui, &metrics);

    show_propulsion_cycle_summary(ui, &result.config);
    ui.add_space(8.0);
    if !result.feasibility.findings.is_empty() {
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.label(RichText::new(tr("Physical findings")).strong());
            for finding in &result.feasibility.findings {
                let detail = match (finding.actual, finding.limit) {
                    (Some(actual), Some(limit)) if !finding.unit.is_empty() => tr_fields(
                        "{message} (actual {actual} {unit}, limit {limit} {unit})",
                        &[
                            ("message", tr(&finding.message)),
                            ("actual", format!("{actual:.3}")),
                            ("limit", format!("{limit:.3}")),
                            ("unit", finding.unit.to_owned()),
                        ],
                    ),
                    _ => tr(&finding.message),
                };
                ui.colored_label(ui.visuals().error_fg_color, format!("- {detail}"));
            }
        });
        ui.add_space(8.0);
    }
    ui.add(
        egui::Label::new(
            RichText::new(tr(
                "Open a discipline tab above for its figures. Slots that read \"Not available for \
             this run\" need data this run did not produce.",
            ))
            .weak(),
        )
        .wrap(),
    );
}

fn show_propulsion_cycle_summary(ui: &mut Ui, config: &alas_config::AlasConfig) {
    let lines = alas_report::families::propulsion::propulsion_cycle_summary(config);
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Propulsion cycle summary")).strong());
        ui.add_space(4.0);
        let entries: Vec<(String, String)> = lines
            .into_iter()
            .filter(|line| !line.is_empty())
            .map(|line| propulsion_summary_entry(&line))
            .collect();
        let columns = usize::from(ui.available_width() >= 620.0).max(1);
        for row in entries.chunks(columns) {
            ui.columns(columns, |columns| {
                for (index, (label, value)) in row.iter().enumerate() {
                    columns[index].horizontal_wrapped(|ui| {
                        if !label.is_empty() {
                            ui.label(RichText::new(label).weak().small());
                        }
                        ui.label(RichText::new(value).strong());
                    });
                }
            });
            ui.add_space(4.0);
        }
    });
}

fn propulsion_summary_entry(line: &str) -> (String, String) {
    line.split_once(':')
        .or_else(|| line.split_once('='))
        .map(|(label, value)| (label.trim().to_owned(), value.trim().to_owned()))
        .unwrap_or_else(|| (String::new(), line.trim().to_owned()))
}

fn selected_analysis(result: &alas_pipeline::PipelineResult) -> Option<&AnalysisReport> {
    result
        .optimized_report
        .as_ref()
        .or(result.baseline_analysis.as_ref())
}

fn aircraft_summary_metrics(result: &alas_pipeline::PipelineResult) -> Vec<(&'static str, String)> {
    let mut metrics = Vec::new();
    let fuel = &result.feasibility.fuel_loading;
    if let Some(report) = selected_analysis(result) {
        if let Some((oew_kg, tow_kg, mtow_kg)) = mass_triplet_kg(
            &report.component_masses,
            fuel.analyzed_takeoff_mass_kg,
            result.config.requirements.mtow_kg,
        ) {
            metrics.push((
                "Masses (OEW / TOW / MTOW)",
                format!(
                    "{:.1} / {:.1} / {:.1} t",
                    oew_kg / 1_000.0,
                    tow_kg / 1_000.0,
                    mtow_kg / 1_000.0
                ),
            ));
        }
        if report.airplane.b_ref.is_finite()
            && report.airplane.b_ref > 0.0
            && report.airplane.s_ref.is_finite()
            && report.airplane.s_ref > 0.0
        {
            metrics.push((
                "Wing geometry",
                tr_fields(
                    "{span} m span | {area} m^2 area",
                    &[
                        ("span", format!("{:.1}", report.airplane.b_ref)),
                        ("area", format!("{:.1}", report.airplane.s_ref)),
                    ],
                ),
            ));
        }
        let (l_over_d, provenance) = report
            .trimmed_design_point
            .as_ref()
            .map(|point| (point.l_over_d, "trimmed"))
            .unwrap_or((report.design_point.l_over_d, "untrimmed"));
        if l_over_d.is_finite() && l_over_d > 0.0 {
            metrics.push(("Cruise L/D", format!("{l_over_d:.1} ({})", tr(provenance))));
        }
    }

    if fuel.analyzed_carried_fuel_kg.is_finite() && fuel.analyzed_carried_fuel_kg >= 0.0 {
        let capacity = match fuel.usable_capacity.capacity_kg {
            Some(capacity_kg) if capacity_kg.is_finite() && capacity_kg >= 0.0 => tr_fields(
                "{capacity} t ({evidence})",
                &[
                    ("capacity", format!("{:.1}", capacity_kg / 1_000.0)),
                    (
                        "evidence",
                        tr(match fuel.usable_capacity.evidence {
                            FuelCapacityEvidence::PublishedPreset => "published",
                            FuelCapacityEvidence::GeometryEstimate => "geometry estimate",
                            FuelCapacityEvidence::Unavailable => "unavailable",
                        }),
                    ),
                ],
            ),
            _ => tr("unavailable"),
        };
        metrics.push((
            "Fuel carried / usable capacity",
            tr_fields(
                "{carried} t / {capacity}",
                &[
                    (
                        "carried",
                        format!("{:.1}", fuel.analyzed_carried_fuel_kg / 1_000.0),
                    ),
                    ("capacity", capacity),
                ],
            ),
        ));
    }

    let mission_status = tr(match fuel.mission.status {
        MissionFuelStatus::NotRequested => "not requested",
        MissionFuelStatus::Unavailable => "unavailable",
        MissionFuelStatus::NotConverged => "not converged",
        MissionFuelStatus::Completed => "completed",
        MissionFuelStatus::Exhausted => "fuel exhausted",
    });
    let mission_value = result.mission_result.as_ref().and_then(|mission| {
        mission
            .segments
            .iter()
            .flat_map(|segment| segment.conditions.aircraft_range_m.iter().copied())
            .filter(|range| range.is_finite())
            .reduce(f64::max)
            .map(|range_m| {
                tr_fields(
                    "{range} km | {status}",
                    &[
                        ("range", format!("{:.0}", range_m / 1_000.0)),
                        ("status", mission_status.clone()),
                    ],
                )
            })
    });
    metrics.push((
        "Mission range / status",
        mission_value.unwrap_or(mission_status),
    ));
    metrics
}

fn mass_triplet_kg(
    component_masses: &std::collections::HashMap<String, f64>,
    tow_kg: f64,
    mtow_kg: f64,
) -> Option<(f64, f64, f64)> {
    let total_kg: f64 = component_masses.values().copied().sum();
    let payload_kg = component_masses.get("Payload").copied().unwrap_or(0.0);
    let fuel_kg = component_masses.get("Fuel").copied().unwrap_or(0.0);
    let oew_kg = total_kg - payload_kg - fuel_kg;
    (oew_kg.is_finite()
        && oew_kg > 0.0
        && tow_kg.is_finite()
        && tow_kg > 0.0
        && mtow_kg.is_finite()
        && mtow_kg > 0.0)
        .then_some((oew_kg, tow_kg, mtow_kg))
}

fn result_payload_layout(result: &alas_pipeline::PipelineResult) -> Option<&PayloadLayout> {
    result
        .optimized_report
        .as_ref()
        .and_then(|report| report.payload_layout.as_ref())
        .or_else(|| {
            result
                .baseline_analysis
                .as_ref()
                .and_then(|report| report.payload_layout.as_ref())
        })
        .or_else(|| {
            result
                .baseline_report
                .as_ref()
                .and_then(|report| report.payload_layout.as_ref())
        })
}

fn payload_summary_metrics(layout: &PayloadLayout) -> Vec<(&'static str, String)> {
    match &layout.summary {
        LayoutSummary::Passenger(summary) => {
            let classes = summary
                .classes
                .iter()
                .filter(|(_, seats)| *seats > 0)
                .map(|(class, seats)| format!("{} {seats}", tr(class)))
                .collect::<Vec<_>>()
                .join(" | ");
            vec![
                (
                    "Seating capacity",
                    tr_fields(
                        "{seated} / {requested} seats requested",
                        &[
                            ("seated", summary.seated_pax.to_string()),
                            ("requested", summary.total_pax.to_string()),
                        ],
                    ),
                ),
                ("Unseated passengers", summary.unseated_pax.to_string()),
                ("Cabin class mix", classes),
                ("Passenger payload", format!("{:.1} t", summary.payload_t)),
                (
                    "Hold loading",
                    format!(
                        "{:.1} / {:.1} t | {} ULD",
                        summary.hold_used_t, summary.hold_capacity_t, summary.hold_ulds
                    ),
                ),
                (
                    "Checked bags / belly freight",
                    format!("{:.1} / {:.1} t", summary.bag_mass_t, summary.belly_cargo_t),
                ),
                (
                    "Cabin arrangement",
                    tr_fields(
                        "{abreast} abreast | {aisles} aisle(s) | {decks}",
                        &[
                            ("abreast", summary.max_abreast.to_string()),
                            ("aisles", summary.n_aisles.to_string()),
                            (
                                "decks",
                                tr(if summary.double_deck {
                                    "double deck"
                                } else {
                                    "single deck"
                                }),
                            ),
                        ],
                    ),
                ),
                (
                    "Galleys / lavatories / exit pairs",
                    format!(
                        "{} / {} / {}",
                        summary.galleys, summary.lavatories, summary.exit_pairs
                    ),
                ),
                ("Payload CG", format!("{:.1}% MAC", summary.cg_pct_mac)),
            ]
        }
        LayoutSummary::Cargo(summary) => vec![
            ("Cargo payload", format!("{:.1} t", summary.payload_t)),
            (
                "Net cargo / requested",
                format!(
                    "{:.1} / {:.1} t",
                    summary.loaded_net_payload_t, summary.requested_net_payload_t
                ),
            ),
            ("ULD tare", format!("{:.1} t", summary.tare_mass_t)),
            (
                "ULD loading",
                tr_fields(
                    "{loaded} / {slots} positions",
                    &[
                        ("loaded", summary.n_ulds.to_string()),
                        ("slots", summary.n_slots.to_string()),
                    ],
                ),
            ),
            (
                "Cargo capacity",
                format!("{:.1} t | {:.1}%", summary.capacity_t, summary.fill_pct),
            ),
            ("Cargo volume", format!("{:.1} m^3", summary.volume_m3)),
            (
                "Main / lower deck ULDs",
                format!("{} / {}", summary.n_main_deck, summary.n_lower_deck),
            ),
            (
                "Payload CG",
                format!("{:.1}% MAC", summary.achieved_cg_pct_mac),
            ),
            ("Loading strategy", tr(&summary.strategy)),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::{mass_triplet_kg, payload_summary_metrics};
    use alas_payload::layout::{LayoutSummary, PassengerSummary, PayloadLayout};

    #[test]
    fn passenger_summary_metrics_come_from_the_built_layout() {
        let layout = PayloadLayout {
            mode: alas_payload::layout::Mode::Passenger,
            items: Vec::new(),
            total_mass: 21_000.0,
            cg_x: 10.0,
            cg_y: 0.0,
            summary: LayoutSummary::Passenger(Box::new(PassengerSummary {
                total_pax: 204,
                seated_pax: 198,
                unseated_pax: 6,
                classes: vec![("Business", 18), ("Economy", 180)],
                lavatories: 4,
                galleys: 3,
                exit_type: "A",
                exit_pairs: 4,
                exit_capacity: 220,
                max_certifiable_capacity: 220,
                payload_t: 21.0,
                seat_mass_t: 17.5,
                bag_mass_t: 2.8,
                belly_cargo_t: 0.7,
                hold_capacity_t: 8.0,
                hold_used_t: 3.5,
                hold_ulds: 5,
                aisle_width_m: 0.51,
                max_abreast: 6,
                n_aisles: 1,
                deck_utilization: vec![("main", 0.82)],
                cg_pct_mac: 25.4,
                double_deck: false,
            })),
        };
        let metrics = payload_summary_metrics(&layout);
        assert!(metrics.iter().any(|(label, value)| {
            *label == "Seating capacity" && value == "198 / 204 seats requested"
        }));
        assert!(metrics
            .iter()
            .any(|(label, value)| *label == "Cabin class mix" && value.contains("Business 18")));
        assert!(metrics
            .iter()
            .any(|(label, value)| *label == "Hold loading" && value.contains("3.5 / 8.0 t")));
    }

    #[test]
    fn aircraft_mass_summary_excludes_payload_and_fuel_from_oew() {
        let masses = std::collections::HashMap::from([
            ("Wing".to_owned(), 12_000.0),
            ("Fuselage".to_owned(), 8_000.0),
            ("Payload".to_owned(), 6_000.0),
            ("Fuel".to_owned(), 4_000.0),
        ]);
        assert_eq!(
            mass_triplet_kg(&masses, 29_000.0, 32_000.0),
            Some((20_000.0, 29_000.0, 32_000.0))
        );
    }
}
