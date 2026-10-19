// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Model outputs the validation references ask for that the base dump did
//! not export: the service ceiling, the CG envelope limits in % MAC, and the
//! block fuel of the stated reference missions.
//!
//! Each value is the one the report already computes (the same routines the
//! matrix `validation` block calls in `alas-acceptance`). Nothing here is a
//! new model. A quantity the model cannot produce is `{"unavailable": reason}`.

use alas_config::AlasConfig;
use alas_pipeline::quick_analysis::{CruiseSolve, SERVICE_CEILING_CLIMB_RATE_M_S};
use alas_pipeline::reference_mission::reference_route_fuel;
use alas_pipeline::{AnalysisReport, FeasibilityReport};
use serde_json::{json, Value};

/// Metres per international nautical mile.
const M_PER_NMI: f64 = 1852.0;

/// Stated block-fuel missions with a manufacturer definition (ATR 72-600
/// brochure 2026: max payload 7 250 kg, 95 kg per passenger, EASA reserves,
/// 100 NM alternate, 10 min taxi, ISA sea level). Same list as
/// `optimization_preset_audit/validation.rs::REFERENCE_MISSIONS`; the
/// 500 and 1000 NM entries have no published value.
const STATED_BLOCK_MISSIONS: &[(&str, &str, f64)] = &[
    ("ATR72-600", "Q5.block200", 200.0),
    ("ATR72-600", "Q5.block300", 300.0),
    ("ATR72-600", "Q5.block400", 400.0),
    ("ATR72-600", "Q5.block500", 500.0),
    ("ATR72-600", "Q5.block1000", 1000.0),
];

/// Service ceiling (m) at the analysed take-off mass, standard day, at the
/// maximum-climb rating, with the report's drag table and propulsion deck.
pub(super) fn service_ceiling(config: &AlasConfig, report: &AnalysisReport) -> Value {
    let mass_kg = report.analysis_takeoff_mass_kg(config.requirements.mtow_kg);
    let criterion = SERVICE_CEILING_CLIMB_RATE_M_S;
    match CruiseSolve::new(config, report, mass_kg).and_then(|solve| solve.service_ceiling_m()) {
        Ok(height_m) => json!({
            "service_ceiling_m": height_m,
            "mass_kg": mass_kg,
            "climb_rate_criterion_m_s": criterion,
            "basis": "highest altitude where the best excess-power climb rate over the deck Mach domain reaches the criterion; standard day; max-climb rating; analysed take-off mass (conservative for end of cruise)",
        }),
        Err(reason) => json!({
            "unavailable": reason,
            "mass_kg": mass_kg,
            "climb_rate_criterion_m_s": criterion,
        }),
    }
}

/// CG envelope limits in % MAC: the published planning curve, the loading
/// envelope (min / max over every sampled boarding, cargo and fuel-burn
/// sequence) and the model's own hard limits.
pub(super) fn cg_limits(feasibility: &FeasibilityReport) -> Value {
    let planning = &feasibility.cg_envelope;
    let loading = feasibility.operational_envelope.as_ref().map(|envelope| {
        let finite: Vec<&(f64, f64, f64)> = envelope
            .potato_pct_mac
            .iter()
            .filter(|p| p.1.is_finite() && p.2.is_finite())
            .collect();
        let forward = finite.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let aft = finite.iter().map(|p| p.2).fold(f64::NEG_INFINITY, f64::max);
        let have = !finite.is_empty();
        json!({
            "forward_pct_mac": have.then_some(forward),
            "aft_pct_mac": have.then_some(aft),
            "range_pct_mac": have.then_some(aft - forward),
            "zfw_operational_limits_pct_mac": envelope
                .zfw_operational_limits_pct_mac
                .map(|(lo, hi)| [lo, hi]),
        })
    });
    let model = feasibility.model_cg.as_ref();
    json!({
        "coordinate_frame": "% MAC of the manufacturer planning frame (published MAC leading edge and chord)",
        "planning_status": format!("{:?}", planning.planning_status),
        "planning_cg_pct_mac": planning.cg_pct_mac,
        "planning_forward_limit_pct_mac": planning.forward_limit_pct_mac,
        "planning_aft_limit_pct_mac": planning.aft_limit_pct_mac,
        "loading_envelope": loading.unwrap_or_else(|| json!({"unavailable": "no operational loading envelope evaluated"})),
        "model_forward_limit_pct_mac": model.map(|m| m.configured_forward_limit_pct_mac),
        "model_aft_limit_pct_mac": model.map(|m| m.worst_aft_limit_pct_mac),
        "model_aerodynamic_aft_limit_pct_mac": model.map(|m| m.aerodynamic_aft_limit_pct_mac),
        "model_limit_basis": "model_forward_limit: configured forward limit; model_aft_limit: worst (governing) aft limit of the model's hard constraints; the model's own MAC frame, not the manufacturer's",
    })
}

/// Block fuel of each stated reference mission this preset has, priced by
/// the report's route pricing. The payload is the one the report carries
/// (see `zero_fuel_mass_kg`), which is compared with the brochure payload
/// before any verdict is drawn.
pub(super) fn reference_block_fuel(config: &AlasConfig, report: &AnalysisReport) -> Value {
    let missions: Vec<(&str, f64)> = STATED_BLOCK_MISSIONS
        .iter()
        .filter(|(preset, _, _)| *preset == config.preset)
        .map(|(_, id, nmi)| (*id, *nmi))
        .collect();
    if missions.is_empty() {
        return json!({"note": "no stated reference block-fuel mission for this preset"});
    }
    let mut out = serde_json::Map::new();
    for (id, range_nmi) in missions {
        let value = match reference_route_fuel(config, report, range_nmi * M_PER_NMI) {
            Ok(fuel) => json!({
                "range_nmi": range_nmi,
                "block_fuel_kg": fuel.block_fuel_kg,
                "trip_fuel_kg": fuel.trip_fuel_kg,
                "taxi_out_fuel_kg": fuel.taxi_out_fuel_kg,
                "reserve_fuel_kg": fuel.reserve_fuel_kg,
                "takeoff_mass_kg": fuel.takeoff_mass_kg,
                "zero_fuel_mass_kg": fuel.zero_fuel_mass_kg,
                "converged": fuel.converged,
                "dispatch_status": fuel.status,
                "shortfall_kg": fuel.shortfall_kg,
            }),
            Err(reason) => json!({ "unavailable": reason, "range_nmi": range_nmi }),
        };
        out.insert(id.to_owned(), value);
    }
    Value::Object(out)
}
