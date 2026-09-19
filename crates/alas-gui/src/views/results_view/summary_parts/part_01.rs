// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_payload::layout::{LayoutSummary, PayloadLayout};
use alas_pipeline::feasibility::{FindingSeverity, FuelCapacityEvidence, MissionFuelStatus};
use alas_pipeline::AnalysisReport;
use egui::{Frame, Margin, RichText, Rounding, Stroke, Ui};

use crate::state::AppState;
use crate::views::{tr, tr_fields};

use super::external::show_tool_status_cards;
use super::format_cg_pct_mac;

fn stat_tile(ui: &mut Ui, label: &str, value: String) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.vertical(|ui| {
            ui.label(RichText::new(tr(label)).small());
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
    let counts = tr_fields(
        "{errors} blocking finding(s), {warnings} warning(s).",
        &[
            ("errors", errors.to_string()),
            ("warnings", warnings.to_string()),
        ],
    );
    let (color, title, detail) = if errors > 0 {
        (
            ui.visuals().error_fg_color,
            tr("Infeasible under implemented checks"),
            format!("{counts} {}", mission_status_label(result)),
        )
    } else if warnings > 0 {
        (
            ui.visuals().warn_fg_color,
            tr("Feasible with engineering warnings"),
            format!("{counts} {}", mission_status_label(result)),
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
            ui.label(RichText::new(title).strong().size(19.0).color(color))
                .on_hover_text(tr(
                    "This verdict covers only the physical checks implemented by this run; it is not a certification finding.",
                ));
            ui.label(RichText::new(detail));
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

fn summary_column_count(available_width: f32) -> usize {
    ((available_width / 245.0).floor() as usize).clamp(1, 4)
}

fn show_stat_tiles<L: AsRef<str>>(ui: &mut Ui, metrics: &[(L, String)]) {
    if metrics.is_empty() {
        return;
    }
    let columns = summary_column_count(ui.available_width()).min(metrics.len());
    for row in metrics.chunks(columns) {
        ui.columns(columns, |columns| {
            for (index, (label, value)) in row.iter().enumerate() {
                stat_tile(&mut columns[index], label.as_ref(), value.clone());
            }
        });
        ui.add_space(8.0);
    }
}

fn section_title(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(tr(title)).strong().size(18.0));
    ui.add_space(5.0);
}

pub(super) fn show_summary(state: &AppState, ui: &mut Ui, result: &alas_pipeline::PipelineResult) {
    show_status_banner(ui, result);
    ui.add_space(10.0);
    findings_card::show_findings_card(ui, &result.feasibility.findings);

    section_title(ui, "External analyses");
    show_tool_status_cards(ui, result);

    section_title(ui, "Aircraft and mission");
    show_stat_tiles(ui, &aircraft_metrics(state, result));

    section_title(ui, "Mission");
    show_stat_tiles(ui, &mission_metrics(result));

    section_title(ui, "Mass and balance");
    show_stat_tiles(ui, &mass_metrics(result));

    section_title(ui, "Aerodynamics and trim");
    show_stat_tiles(ui, &trim_metrics(result));

    if let Some(layout) = result_payload_layout(result) {
        section_title(ui, "Payload and cabin");
        show_stat_tiles(ui, &payload_summary_metrics(layout));
    }

    section_title(ui, "Propulsion cycle details");
    show_propulsion_cycle_summary(ui, &result.config);
    ui.add_space(8.0);
}

/// Baseline and optimized stability values side by side, in the same units.
fn static_margin_rows(
    baseline: Option<f64>,
    optimized: Option<f64>,
) -> Vec<(&'static str, String)> {
    let margin = |value: f64| format!("{:.1} % MAC", value * 100.0);
    vec![
        (
            "Static margin (baseline)",
            baseline.map_or_else(|| tr("Not evaluated"), margin),
        ),
        (
            "Static margin (optimized)",
            optimized.map_or_else(|| tr("No optimized design in this run"), margin),
        ),
    ]
}

fn aircraft_metrics(
    state: &AppState,
    result: &alas_pipeline::PipelineResult,
) -> Vec<(&'static str, String)> {
    let mut metrics = vec![("Preset", state.active_preset.clone())];
    metrics.extend(static_margin_rows(
        result
            .baseline_report
            .as_ref()
            .map(|report| report.static_margin),
        result
            .optimized_report
            .as_ref()
            .map(|report| report.static_margin),
    ));
    metrics.push((
        "CG (baseline)",
        result.baseline_report.as_ref().map_or_else(
            || tr("Not evaluated"),
            |report| format_cg_pct_mac(report.cg_pct_mac),
        ),
    ));
    metrics.push((
        "CG envelope (optimized)",
        result.optimized_report.as_ref().map_or_else(
            || tr("No optimized design in this run"),
            |report| match report.cg_envelope_ok {
                Some(true) => "OK".to_owned(),
                Some(false) => tr("Violation"),
                None => tr("Not evaluated"),
            },
        ),
    ));
    if let Some(report) = selected_analysis(result) {
        if report.airplane.b_ref.is_finite()
            && report.airplane.b_ref > 0.0
            && report.airplane.s_ref.is_finite()
            && report.airplane.s_ref > 0.0
        {
            metrics.push(("Wing span", format!("{:.1} m", report.airplane.b_ref)));
            metrics.push(("Wing area", format!("{:.1} m^2", report.airplane.s_ref)));
        }
    }
    metrics
}

fn mission_metrics(result: &alas_pipeline::PipelineResult) -> Vec<(&'static str, String)> {
    let fuel = &result.feasibility.fuel_loading;
    let mut metrics = vec![("Mission status", mission_status_label(result))];
    metrics.push((
        "Maximum mission range",
        maximum_mission_range_km(result).map_or_else(
            || tr("Not established"),
            |range_km| format!("{range_km:.0} km"),
        ),
    ));
    let burned = fuel
        .mission
        .burned_fuel_kg
        .filter(|burned| burned.is_finite());
    metrics.push((
        "Mission fuel burn",
        burned.map_or_else(
            || tr("Not established"),
            |burned_kg| format!("{:.2} t", burned_kg / 1_000.0),
        ),
    ));
    metrics.extend(fuel_margin_rows(
        fuel.mission.status,
        fuel.analyzed_carried_fuel_kg,
        burned,
    ));
    if fuel.analyzed_carried_fuel_kg.is_finite() && fuel.analyzed_carried_fuel_kg >= 0.0 {
        metrics.push((
            "Fuel carried",
            format!("{:.1} t", fuel.analyzed_carried_fuel_kg / 1_000.0),
        ));
        let (capacity, evidence) = match fuel.usable_capacity.capacity_kg {
            Some(capacity_kg) if capacity_kg.is_finite() && capacity_kg >= 0.0 => (
                format!("{:.1} t", capacity_kg / 1_000.0),
                tr(match fuel.usable_capacity.evidence {
                    FuelCapacityEvidence::PublishedPreset => "Published preset value",
                    FuelCapacityEvidence::GeometryEstimate => "Geometry estimate",
                    FuelCapacityEvidence::Unavailable => "Unavailable",
                }),
            ),
            _ => (tr("Unavailable"), tr("Unavailable")),
        };
        metrics.push(("Usable fuel capacity", capacity));
        metrics.push(("Fuel capacity evidence", evidence));
    }
    metrics
}

/// Trip fuel margin as two rows: mass and share of the carried fuel.
fn fuel_margin_rows(
    status: MissionFuelStatus,
    carried_kg: f64,
    burned_kg: Option<f64>,
) -> Vec<(&'static str, String)> {
    let label = match status {
        MissionFuelStatus::Exhausted => "Fuel margin at stop",
        _ => "Fuel margin at destination",
    };
    match status {
        MissionFuelStatus::Completed | MissionFuelStatus::Exhausted
            if carried_kg.is_finite() && burned_kg.is_some() =>
        {
            let margin_kg = carried_kg - burned_kg.unwrap_or_default();
            let mut rows = vec![(label, format!("{:+.2} t", margin_kg / 1_000.0))];
            if carried_kg > 0.0 {
                rows.push((
                    "Fuel margin, share of carried fuel",
                    format!("{:+.1} %", 100.0 * margin_kg / carried_kg),
                ));
            }
            rows
        }
        MissionFuelStatus::NotConverged => {
            vec![(label, tr("Not established (partial telemetry)"))]
        }
        MissionFuelStatus::Unavailable => {
            vec![(label, tr("Not established (mission unavailable)"))]
        }
        MissionFuelStatus::NotRequested => vec![(label, tr("Not evaluated"))],
        MissionFuelStatus::Completed | MissionFuelStatus::Exhausted => {
            vec![(label, tr("Not established"))]
        }
    }
}

fn mass_metrics(result: &alas_pipeline::PipelineResult) -> Vec<(&'static str, String)> {
    let fuel = &result.feasibility.fuel_loading;
    let mut metrics = Vec::new();
    if let Some(report) = selected_analysis(result) {
        if let Some((oew_kg, tow_kg, mtow_kg)) = mass_triplet_kg(
            &report.component_masses,
            fuel.analyzed_takeoff_mass_kg,
            result.config.requirements.mtow_kg,
        ) {
            metrics.push(("Operating empty mass", format!("{:.1} t", oew_kg / 1_000.0)));
            metrics.push(("Takeoff mass", format!("{:.1} t", tow_kg / 1_000.0)));
            metrics.push((
                "Maximum takeoff mass",
                format!("{:.1} t", mtow_kg / 1_000.0),
            ));
        }
    }
    metrics.push((
        "Takeoff mass margin",
        takeoff_mass_margin(fuel.mtow_shortfall_kg),
    ));
    if fuel.zero_fuel_mass_kg.is_finite() && fuel.zero_fuel_mass_kg >= 0.0 {
        metrics.push((
            "Zero-fuel mass",
            format!("{:.1} t", fuel.zero_fuel_mass_kg / 1_000.0),
        ));
        metrics.push((
            "Fuel budget up to MTOW",
            format!("{:.1} t", fuel.mtow_closure_fuel_kg / 1_000.0),
        ));
    }
    metrics
}

fn takeoff_mass_margin(mtow_shortfall_kg: f64) -> String {
    if !mtow_shortfall_kg.is_finite() {
        tr("Not established")
    } else if mtow_shortfall_kg.abs() < 0.5 {
        tr("At MTOW")
    } else {
        tr_fields(
            "{margin} t below MTOW",
            &[("margin", format!("{:.2}", mtow_shortfall_kg / 1_000.0))],
        )
    }
}

fn trim_metrics(result: &alas_pipeline::PipelineResult) -> Vec<(&'static str, String)> {
    let Some(report) = selected_analysis(result) else {
        return vec![("Cruise trim", tr("Not evaluated"))];
    };
    let mut metrics = Vec::new();
    let (l_over_d, provenance) = report
        .trimmed_design_point
        .as_ref()
        .map(|point| (point.l_over_d, "Trimmed"))
        .unwrap_or((report.design_point.l_over_d, "Untrimmed"));
    if l_over_d.is_finite() && l_over_d > 0.0 {
        metrics.push(("Cruise L/D", format!("{l_over_d:.1}")));
        metrics.push(("Cruise L/D basis", tr(provenance)));
    }
    match report.trimmed_design_point.as_ref() {
        Some(point) => {
            metrics.push(("Cruise trim", tr("Solved")));
            metrics.push((
                "Trim angle of attack",
                format!("{:.2} deg", point.geometric_body_alpha_deg),
            ));
            metrics.push((
                "Stabilizer incidence",
                format!("{:.2} deg", point.trim_ih_deg),
            ));
            metrics.push((
                "Residual pitching moment |Cm|",
                format!("{:.2e}", point.cm_residual.abs()),
            ));
        }
        None => metrics.push(("Cruise trim", tr("Not demonstrated"))),
    }
    metrics
}

fn show_propulsion_cycle_summary(ui: &mut Ui, config: &alas_config::AlasConfig) {
    let lines = alas_report::families::propulsion::propulsion_cycle_summary(config);
    ui.label(RichText::new(tr("Propulsion cycle summary")).strong());
    ui.add_space(4.0);
    let entries = propulsion_summary_entries(&lines);
    if entries.is_empty() {
        return;
    }
    let columns = summary_column_count(ui.available_width()).min(entries.len());
    for row in entries.chunks(columns) {
        ui.columns(columns, |columns| {
            for (index, (label, value)) in row.iter().enumerate() {
                propulsion_metric_card(&mut columns[index], label, value);
            }
        });
        ui.add_space(8.0);
    }
}

/// Split the shared propulsion summary into independently readable cards.
///
/// The cycle producer keeps its report-oriented lines (including the compact
/// BPR/OPR/FPR/TIT row) as the source of truth. This adapter only changes the
/// presentation shape; it does not recalculate or round any physical value.
fn propulsion_summary_entries(lines: &[String]) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    for line in lines
        .iter()
        .map(String::as_str)
        .filter(|line| !line.trim().is_empty())
    {
        if line.trim_start().starts_with("BPR =") {
            entries.extend(
                line.split("    ")
                    .filter(|metric| !metric.trim().is_empty())
                    .map(propulsion_summary_entry),
            );
        } else {
            entries.push(propulsion_summary_entry(line));
        }
    }
    entries
}

fn propulsion_metric_card(ui: &mut Ui, label: &str, value: &str) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        if !label.is_empty() {
            ui.add(
                egui::Label::new(math_rich_text(&localized_propulsion_label(label)).small()).wrap(),
            );
        }
        ui.add(egui::Label::new(math_rich_text(value).strong().size(14.0)).wrap());
    });
}

/// Use a monospace face for cycle symbols (η, subscripts and compact metric
/// names) so they remain legible in all three desktop themes and at narrow
/// widths. The values themselves remain the producer's unit-bearing strings.
fn math_rich_text(text: &str) -> RichText {
    RichText::new(text.to_owned()).font(egui::FontId::monospace(13.0))
}

fn localized_propulsion_label(label: &str) -> String {
    let compact = label.split_whitespace().collect::<Vec<_>>().join(" ");
    match compact.as_str() {
        "Engine" => tr("Engine:").trim().to_owned(),
        "Design point" => tr("Design point: ").trim().to_owned(),
        "Specific thrust SFn" => format!("{}  SFn", tr("Specific thrust")),
        "Fuel-air ratio f" => format!("{}  f", tr("Fuel-air ratio")),
        "TSFC (computed)" => tr("TSFC (computed)"),
        "TSFC (reference)" => tr("TSFC (reference)"),
        "Thermal efficiency (eta_t)" => format!("{} (ηₜ)", tr("Thermal efficiency")),
        "Propulsive efficiency (eta_p)" => format!("{} (ηₚ)", tr("Propulsive efficiency")),
        "Overall efficiency (eta_o)" => format!("{} (ηₒ)", tr("Overall efficiency")),
        "Per-engine thrust, static (rated)" => tr("Per-engine thrust, static (rated)"),
        "Per-engine thrust, this cruise pt" => tr("Per-engine thrust, this cruise pt"),
        "Cycle infeasible at this design point" => tr("Cycle infeasible at this design point:")
            .trim_end_matches(':')
            .to_owned(),
        _ if compact.starts_with("Total installed thrust") => format!(
            "{} {}",
            tr("Total installed thrust"),
            compact.trim_start_matches("Total installed thrust").trim()
        ),
        _ => label.trim().to_owned(),
    }
}

fn propulsion_summary_entry(line: &str) -> (String, String) {
    line.split_once(':')
        .or_else(|| line.split_once('='))
        .map(|(label, value)| (label.trim().to_owned(), value.trim().to_owned()))
        .unwrap_or_else(|| (String::new(), line.trim().to_owned()))
}
