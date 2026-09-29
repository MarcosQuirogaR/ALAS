// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Metric rows for the aircraft, mission, mass, trim and payload tiles.

use alas_payload::layout::{LayoutSummary, PayloadLayout};
use alas_pipeline::feasibility::{FuelCapacityEvidence, MissionFuelStatus};
use alas_pipeline::AnalysisReport;

use crate::state::AppState;
use crate::views::{tr, tr_fields};

use super::super::format_cg_pct_mac;
use super::widgets::*;

pub(super) fn static_margin_rows(
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

pub(super) fn aircraft_metrics(
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

pub(super) fn mission_metrics(
    result: &alas_pipeline::PipelineResult,
) -> Vec<(&'static str, String)> {
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
pub(super) fn fuel_margin_rows(
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

pub(super) fn mass_metrics(result: &alas_pipeline::PipelineResult) -> Vec<(&'static str, String)> {
    let fuel = &result.feasibility.fuel_loading;
    let mut metrics = Vec::new();
    if let Some(report) = selected_analysis(result) {
        if let Some((oew_kg, tow_kg, mtow_kg)) = mass_triplet_kg(
            &report.component_masses,
            fuel.analyzed_takeoff_mass_kg,
            result.config.requirements.mtow_kg,
        ) {
            metrics.extend(takeoff_mass_rows(oew_kg, tow_kg, mtow_kg));
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

/// The three headline mass rows. The takeoff mass is the analysed (sized)
/// result of the run; the declared MTOW is the configured input limit and is
/// labelled as such so the two are never read as one quantity.
pub(super) fn takeoff_mass_rows(
    oew_kg: f64,
    tow_kg: f64,
    mtow_kg: f64,
) -> [(&'static str, String); 3] {
    [
        ("Operating empty mass", format!("{:.1} t", oew_kg / 1_000.0)),
        (
            "Takeoff mass (sized result)",
            format!("{:.1} t", tow_kg / 1_000.0),
        ),
        ("MTOW limit (input)", format!("{:.1} t", mtow_kg / 1_000.0)),
    ]
}

pub(super) fn takeoff_mass_margin(mtow_shortfall_kg: f64) -> String {
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

pub(super) fn trim_metrics(result: &alas_pipeline::PipelineResult) -> Vec<(&'static str, String)> {
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

pub(super) fn selected_analysis(result: &alas_pipeline::PipelineResult) -> Option<&AnalysisReport> {
    result
        .optimized_report
        .as_ref()
        .or(result.baseline_analysis.as_ref())
}

pub(super) fn mass_triplet_kg(
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

pub(super) fn result_payload_layout(
    result: &alas_pipeline::PipelineResult,
) -> Option<&PayloadLayout> {
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

pub(super) fn tonnes(value_t: f64) -> String {
    format!("{value_t:.1} t")
}

/// One labelled row per value; no slash-joined composites.
pub(super) fn payload_summary_metrics(layout: &PayloadLayout) -> Vec<(&'static str, String)> {
    match &layout.summary {
        LayoutSummary::Passenger(summary) => {
            let classes = summary
                .classes
                .iter()
                .filter(|(_, seats)| *seats > 0)
                .map(|(class, seats)| format!("{} {seats}", tr(class)))
                .collect::<Vec<_>>()
                .join(", ");
            vec![
                ("Seated passengers", summary.seated_pax.to_string()),
                ("Requested passengers", summary.total_pax.to_string()),
                ("Unseated passengers", summary.unseated_pax.to_string()),
                ("Cabin class mix", classes),
                ("Passenger payload", tonnes(summary.payload_t)),
                ("Hold load", tonnes(summary.hold_used_t)),
                ("Hold capacity", tonnes(summary.hold_capacity_t)),
                ("Hold ULDs", summary.hold_ulds.to_string()),
                ("Checked bags", tonnes(summary.bag_mass_t)),
                ("Belly freight", tonnes(summary.belly_cargo_t)),
                ("Seats abreast", summary.max_abreast.to_string()),
                ("Aisles", summary.n_aisles.to_string()),
                (
                    "Decks",
                    tr(if summary.double_deck {
                        "Double deck"
                    } else {
                        "Single deck"
                    }),
                ),
                ("Galleys", summary.galleys.to_string()),
                ("Lavatories", summary.lavatories.to_string()),
                ("Exit pairs", summary.exit_pairs.to_string()),
                (
                    "Accessible lavatories",
                    summary.accessible_lavatories.to_string(),
                ),
                (
                    "Wheelchair stowages",
                    summary.wheelchair_stowages.to_string(),
                ),
                ("Payload CG", format!("{:.1}% MAC", summary.cg_pct_mac)),
            ]
        }
        LayoutSummary::Cargo(summary) => vec![
            ("Cargo payload", tonnes(summary.payload_t)),
            ("Net cargo loaded", tonnes(summary.loaded_net_payload_t)),
            (
                "Net cargo requested",
                tonnes(summary.requested_net_payload_t),
            ),
            ("ULD tare", tonnes(summary.tare_mass_t)),
            ("ULDs loaded", summary.n_ulds.to_string()),
            ("ULD positions", summary.n_slots.to_string()),
            ("Cargo capacity", tonnes(summary.capacity_t)),
            ("Cargo capacity used", format!("{:.1} %", summary.fill_pct)),
            ("Cargo volume", format!("{:.1} m^3", summary.volume_m3)),
            ("Main deck ULDs", summary.n_main_deck.to_string()),
            ("Lower deck ULDs", summary.n_lower_deck.to_string()),
            (
                "Payload CG",
                format!("{:.1}% MAC", summary.achieved_cg_pct_mac),
            ),
            ("Loading strategy", tr(&summary.strategy)),
        ],
    }
}
