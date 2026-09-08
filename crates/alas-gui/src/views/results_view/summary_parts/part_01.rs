// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_payload::layout::{LayoutSummary, PayloadLayout};
use alas_pipeline::feasibility::{
    FindingCode, FindingSeverity, FuelCapacityEvidence, MissionFuelStatus, PhysicalFinding,
};
use alas_pipeline::AnalysisReport;
use egui::{Frame, Margin, RichText, Rounding, Stroke, Ui};

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

fn semantic_frame(ui: &Ui, color: egui::Color32) -> Frame {
    Frame::group(ui.style())
        .fill(ui.visuals().window_fill())
        .stroke(Stroke::new(1.5_f32, color))
        .inner_margin(Margin::symmetric(12.0, 8.0))
        .rounding(Rounding::same(12.0))
}

fn status_frame(ui: &Ui, severity: FindingSeverity) -> Frame {
    semantic_frame(
        ui,
        match severity {
            FindingSeverity::Error => ui.visuals().error_fg_color,
            FindingSeverity::Warning => ui.visuals().warn_fg_color,
        },
    )
}

fn show_status_banner(ui: &mut Ui, result: &alas_pipeline::PipelineResult) {
    let errors = result
        .feasibility
        .findings
        .iter()
        .filter(|finding| finding.severity == FindingSeverity::Error)
        .count();
    let warnings = result.feasibility.findings.len().saturating_sub(errors);
    let (color, title, detail) = if errors > 0 {
        (
            ui.visuals().error_fg_color,
            tr("Infeasible under implemented checks"),
            format!(
                "{errors} blocking finding(s) | {warnings} warning(s) | {}",
                mission_status_label(result)
            ),
        )
    } else if warnings > 0 {
        (
            ui.visuals().warn_fg_color,
            tr("Feasible with engineering warnings"),
            format!("{warnings} warning(s) | {}", mission_status_label(result)),
        )
    } else {
        (
            crate::theme::success_color(ui.visuals()),
            tr("Feasible under implemented checks"),
            mission_status_label(result),
        )
    };

    semantic_frame(ui, color).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(title).strong().size(19.0).color(color));
            ui.label(RichText::new(detail).weak());
            ui.label(RichText::new("?").strong().color(color))
                .on_hover_text(tr(
                    "This verdict covers only the physical checks implemented by this run; it is not a certification finding.",
                ));
        });
    });
}

fn mission_status_label(result: &alas_pipeline::PipelineResult) -> String {
    let status = match result.feasibility.fuel_loading.mission.status {
        MissionFuelStatus::NotRequested => "Mission not requested",
        MissionFuelStatus::Unavailable => "Mission unavailable",
        MissionFuelStatus::NotConverged => "Mission did not converge",
        MissionFuelStatus::Completed => "Mission completed",
        MissionFuelStatus::Exhausted => "Mission stopped: fuel exhausted",
    };
    tr(status)
}

fn maximum_mission_range_km(result: &alas_pipeline::PipelineResult) -> Option<f64> {
    result.mission_result.as_ref().and_then(|mission| {
        mission
            .segments
            .iter()
            .flat_map(|segment| segment.conditions.aircraft_range_m.iter().copied())
            .filter(|range| range.is_finite())
            .reduce(f64::max)
            .map(|range_m| range_m / 1_000.0)
    })
}

fn headline_metrics(result: &alas_pipeline::PipelineResult) -> Vec<(&'static str, String)> {
    let fuel = &result.feasibility.fuel_loading;
    let mission = maximum_mission_range_km(result).map_or_else(
        || mission_status_label(result),
        |range_km| format!("{range_km:.0} km | {}", mission_status_label(result)),
    );
    let fuel_margin = match fuel.mission.status {
        MissionFuelStatus::Completed | MissionFuelStatus::Exhausted
            if fuel.analyzed_carried_fuel_kg.is_finite()
                && fuel
                    .mission
                    .burned_fuel_kg
                    .is_some_and(|burned| burned.is_finite()) =>
        {
            let burned = fuel.mission.burned_fuel_kg.unwrap_or_default();
            let margin_kg = fuel.analyzed_carried_fuel_kg - burned;
            let qualifier = if fuel.mission.status == MissionFuelStatus::Exhausted {
                tr("observed at stop")
            } else {
                tr("trip margin")
            };
            if fuel.analyzed_carried_fuel_kg > 0.0 {
                let percent = 100.0 * margin_kg / fuel.analyzed_carried_fuel_kg;
                format!(
                    "{:+.2} t | {:+.1}% | {qualifier}",
                    margin_kg / 1_000.0,
                    percent
                )
            } else {
                format!("{:+.2} t | {qualifier}", margin_kg / 1_000.0)
            }
        }
        MissionFuelStatus::NotConverged => tr("Not established | partial telemetry"),
        MissionFuelStatus::Unavailable => tr("Not established | mission unavailable"),
        MissionFuelStatus::NotRequested => tr("Not evaluated"),
        MissionFuelStatus::Completed | MissionFuelStatus::Exhausted => tr("Not established"),
    };
    let mass_margin = if fuel.mtow_shortfall_kg.is_finite() {
        if fuel.mtow_shortfall_kg.abs() < 0.5 {
            tr("At MTOW")
        } else {
            format!("{:.2} t below MTOW", fuel.mtow_shortfall_kg / 1_000.0)
        }
    } else {
        tr("Not established")
    };
    let trim = selected_analysis(result).map_or_else(
        || tr("Not evaluated"),
        |report| {
            report.trimmed_design_point.as_ref().map_or_else(
                || tr("Not demonstrated"),
                |point| {
                    format!(
                        "Solved | alpha {:.2} deg | iH {:.2} deg | |Cm| {:.2e}",
                        point.geometric_body_alpha_deg,
                        point.trim_ih_deg,
                        point.cm_residual.abs()
                    )
                },
            )
        },
    );
    vec![
        ("Mission progress", mission),
        ("Fuel margin", fuel_margin),
        ("Takeoff mass margin", mass_margin),
        ("Cruise trim", trim),
    ]
}

fn summary_column_count(available_width: f32) -> usize {
    ((available_width / 245.0).floor() as usize).clamp(1, 4)
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
    show_status_banner(ui, result);
    ui.add_space(10.0);
    show_stat_tiles(ui, &headline_metrics(result));

    if !result.feasibility.findings.is_empty() {
        show_findings(ui, &result.feasibility.findings);
        ui.add_space(10.0);
    }

    ui.label(
        RichText::new(tr("Aircraft and mission"))
            .strong()
            .size(18.0),
    );
    ui.label(
        RichText::new(tr(
            "Selected design values, limits, and evidence used by this run.",
        ))
        .weak()
        .small(),
    );
    ui.add_space(5.0);
    let mut metrics = vec![("Preset", state.active_preset.clone())];
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
    metrics.extend(fuel_detail_metrics(result));
    show_stat_tiles(ui, &metrics);

    if let Some(layout) = result_payload_layout(result) {
        ui.label(RichText::new(tr("Payload and cabin")).strong().size(18.0));
        ui.label(
            RichText::new(tr(
                "Delivered payload, accommodation, and loading arrangement.",
            ))
            .weak()
            .small(),
        );
        ui.add_space(5.0);
        show_stat_tiles(ui, &payload_summary_metrics(layout));
    }

    egui::CollapsingHeader::new(RichText::new(tr("Propulsion cycle details")).strong())
        .default_open(false)
        .show(ui, |ui| show_propulsion_cycle_summary(ui, &result.config));
    ui.add_space(8.0);
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

fn show_findings(ui: &mut Ui, findings: &[PhysicalFinding]) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr("Findings requiring attention"))
                .strong()
                .size(18.0),
        );
        ui.label(RichText::new("?").strong())
            .on_hover_ui(show_finding_catalog);
    });
    ui.add_space(5.0);
    let columns = if ui.available_width() >= 820.0 { 2 } else { 1 };
    for row in findings.chunks(columns) {
        ui.columns(columns, |column_uis| {
            for (index, finding) in row.iter().enumerate() {
                show_finding_card(&mut column_uis[index], finding);
            }
        });
        ui.add_space(6.0);
    }
}

fn show_finding_card(ui: &mut Ui, finding: &PhysicalFinding) {
    let (symbol, color) = match finding.severity {
        FindingSeverity::Error => ("x", ui.visuals().error_fg_color),
        FindingSeverity::Warning => ("!", ui.visuals().warn_fg_color),
    };
    status_frame(ui, finding.severity).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(symbol).strong().color(color));
            ui.label(RichText::new(finding_title(finding.code)).strong());
            ui.label(RichText::new("?").strong().color(color))
                .on_hover_ui(|ui| show_finding_help(ui, finding));
        });
        ui.label(
            RichText::new(affected_disciplines(finding.code))
                .small()
                .weak(),
        );
        if let (Some(actual), Some(limit)) = (finding.actual, finding.limit) {
            if !finding.unit.is_empty() {
                let margin = finding_margin(finding.code, actual, limit);
                ui.label(
                    RichText::new(format!("{} {margin:+.3} {}", tr("Margin"), finding.unit))
                        .strong()
                        .color(color),
                );
            }
        }
    });
}

fn show_finding_help(ui: &mut Ui, finding: &PhysicalFinding) {
    ui.set_max_width(430.0);
    ui.label(RichText::new(finding_title(finding.code)).strong());
    ui.add(egui::Label::new(finding_meaning(finding.code)).wrap());
    ui.separator();
    ui.label(RichText::new(tr("Solver output")).strong());
    ui.add(egui::Label::new(tr(&finding.message)).wrap());
    if let (Some(actual), Some(limit)) = (finding.actual, finding.limit) {
        if !finding.unit.is_empty() {
            ui.label(format!(
                "{}: {actual:.3} {} | {}: {limit:.3} {}",
                actual_label(finding.code),
                finding.unit,
                limit_label(finding.code),
                finding.unit
            ));
        }
    }
    ui.label(
        RichText::new(format!(
            "{}: {}",
            tr("Inspect next"),
            finding_next_step(finding.code)
        ))
        .small()
        .weak(),
    );
    if finding.code == FindingCode::TrimUnavailable {
        ui.separator();
        ui.label(
            RichText::new(tr("Possible causes hidden by the current solver output:")).strong(),
        );
        ui.label(tr(
            "Aerodynamic probe failure; singular or nearly singular lift/moment response; non-finite angle or stabilizer incidence; a non-converged coupled solve; trimmed-performance evaluation failure; or a pitching-moment residual above |Cm| = 0.001. The Summary tab cannot distinguish these without a future model-output change.",
        ));
    }
}

fn show_finding_catalog(ui: &mut Ui) {
    ui.set_max_width(520.0);
    ui.label(RichText::new(tr("Finding guide")).strong());
    ui.label(tr(
        "ALAS may report failures in these groups. Hover the question mark on a specific finding for its exact meaning.",
    ));
    ui.separator();
    for (group, text) in [
        ("Aerodynamics and trim", "invalid cruise aerodynamics; cruise trim unavailable; non-finite cruise force balance"),
        ("Fuel and mission", "non-positive fuel; tank-limited takeoff mass; unknown tank capacity; unavailable or non-converged mission; invalid burn; fuel shortfall; throttle above the modeled envelope"),
        ("Mass, CG, and stability", "model CG unavailable or outside its range; public planning-envelope violation; nose/main gear strength or minimum nose-load violation; insufficient static margin"),
        ("Geometry and payload", "wing-area limit; passenger seating shortfall; cargo capacity shortfall"),
        ("Field performance", "airport/input unavailable; takeoff or landing distance violation; maximum landing mass exceeded; insufficient thrust margin"),
    ] {
        ui.label(RichText::new(tr(group)).strong());
        ui.add(egui::Label::new(tr(text)).wrap());
    }
    ui.separator();
    ui.label(
        RichText::new(tr(
            "A finding means an implemented preliminary-design check failed or could not be demonstrated. It is not by itself a certification determination.",
        ))
        .small()
        .weak(),
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
