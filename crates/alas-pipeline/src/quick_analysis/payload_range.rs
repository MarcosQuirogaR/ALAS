// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Achievable payload and payload-range corners for the Quick Analysis.
//!
//! The corner convention matches the report's payload-range figure: point A
//! carries the maximum payload with fuel to the declared MTOW, point B
//! carries the maximum fuel with the payload traded down to MTOW, point C is
//! the ferry case. Each range is the largest distance whose fuel plan,
//! reserves included, fits the corner's fuel (see `corners`); a corner the
//! fuel model cannot price leaves the diagram unsupported.
//!
//! The maximum payload is an *estimated achievable* capacity of the fixed
//! aircraft: the declared structural cap bounded by the preset's
//! MZFW-derived limit when the full analysis recorded one and by the mass
//! budget `MTOW - OEW`. Every corner therefore respects the declared MTOW,
//! and an aircraft whose empty mass reaches its MTOW gets no corners at all
//! instead of a curve that lifts negative fuel.

use alas_config::{presets, AlasConfig};
use alas_mass::payload_range::{max_range_with_reserves, RangeStatus};
use serde::{Deserialize, Serialize};

use super::corners::{corner_ranges, CornerMasses, RangeBasis, CORNER_TOLERANCE_M};
use crate::feasibility::{assess_fuel_capacity, FuelCapacityEvidence};
use crate::fuel_model::candidate_mission_model;
use crate::full_analysis::{effective_structural_payload_limit_kg, AnalysisReport};
use alas_opt::mdo::PlannedTrips;

/// The estimated achievable payload of the fixed aircraft and the bound
/// that sets it.
#[derive(Debug, Clone, PartialEq)]
pub struct PayloadCapacityEstimate {
    /// Achievable payload, kg; zero when the empty mass already reaches MTOW.
    pub capacity_kg: f64,
    /// Which bound is active.
    pub basis: &'static str,
}

/// The achievable payload of an aircraft with the given declared cap,
/// optional MZFW-derived limit, carried payload, declared MTOW and modeled
/// OEW.
///
/// The declared cap is the user's structural payload requirement; it is not
/// achievable when the mass budget `MTOW - OEW` or the preset's MZFW-derived
/// limit is smaller. A non-finite or non-positive declared cap contributes
/// no bound, and when no MZFW-derived limit exists either, the structural
/// maximum is unknown: the corner then carries the analysed payload, as the
/// report's payload-range figure does, still bounded by the mass budget.
pub fn payload_capacity_estimate(
    declared_cap_kg: f64,
    mzfw_limit_kg: Option<f64>,
    carried_payload_kg: f64,
    mtow_kg: f64,
    oew_kg: f64,
) -> PayloadCapacityEstimate {
    let positive = |value: f64| value.is_finite() && value > 0.0;
    let budget_kg = (mtow_kg - oew_kg).max(0.0);
    let mut estimate = PayloadCapacityEstimate {
        capacity_kg: budget_kg,
        basis: "MTOW less operating empty mass budget",
    };
    let mzfw_limit_kg = mzfw_limit_kg.filter(|value| positive(*value));
    if let Some(limit_kg) = mzfw_limit_kg {
        if limit_kg < estimate.capacity_kg {
            estimate = PayloadCapacityEstimate {
                capacity_kg: limit_kg,
                basis: "preset MZFW less modeled operating empty mass",
            };
        }
    }
    if positive(declared_cap_kg) {
        if declared_cap_kg < estimate.capacity_kg {
            estimate = PayloadCapacityEstimate {
                capacity_kg: declared_cap_kg,
                basis: "declared structural payload cap",
            };
        }
    } else if mzfw_limit_kg.is_none() && carried_payload_kg.max(0.0) < estimate.capacity_kg {
        estimate = PayloadCapacityEstimate {
            capacity_kg: carried_payload_kg.max(0.0),
            basis: "analysed carried payload (no structural payload cap declared)",
        };
    }
    estimate
}

/// Short English label of the evidence behind a usable fuel capacity.
pub fn fuel_capacity_basis(evidence: FuelCapacityEvidence) -> &'static str {
    match evidence {
        FuelCapacityEvidence::PublishedPreset => "published usable capacity",
        FuelCapacityEvidence::GeometryEstimate => "geometry-estimated wing tank capacity",
        FuelCapacityEvidence::Unavailable => "unavailable",
    }
}

/// Why the payload-range corners could not be produced.
#[derive(Debug, Clone, PartialEq)]
pub enum PayloadRangeUnavailable {
    /// The corners cannot be priced for this configuration (no fuel
    /// capacity, or no fuel model for the aircraft or one of its corners).
    Unsupported(String),
    /// The fixed aircraft cannot lift any payload or fuel at its MTOW.
    Infeasible(String),
}

/// The payload-range corners of the Quick Analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickPayloadRange {
    /// `(range_m, payload_kg)` from the max-payload corner to the ferry corner.
    pub points: Vec<(f64, f64)>,
    /// Operating empty mass used for the corners.
    pub oew_kg: f64,
    /// The declared MTOW the corners are bounded by.
    pub mtow_kg: f64,
    /// Achievable maximum payload of point A, kg (bounded by `MTOW - OEW`).
    pub max_payload_kg: f64,
    /// Which bound set the maximum payload.
    pub payload_basis: String,
    /// Usable fuel capacity after the MTOW budget.
    pub fuel_capacity_kg: f64,
    /// Which evidence set the capacity.
    pub fuel_capacity_basis: String,
    /// Reserve fuel the plan holds back at each point, kg, aligned with
    /// `points`.
    #[serde(default)]
    pub reserve_fuel_kg: Vec<f64>,
    /// What the ranges include.
    #[serde(default)]
    pub range_basis: RangeBasis,
    /// The assumptions behind the ranges.
    pub note: String,
}

/// Payload-range corners from an analysis report.
pub fn payload_range_corners(
    config: &AlasConfig,
    report: &AnalysisReport,
) -> Result<QuickPayloadRange, PayloadRangeUnavailable> {
    if report.airplane.wings.is_empty() {
        return Err(PayloadRangeUnavailable::Unsupported(
            "the analysed aircraft has no wing".to_owned(),
        ));
    }
    let oew_kg = super::report_oew_kg(report);
    let mtow_kg = config.requirements.mtow_kg;
    if !oew_kg.is_finite() || !mtow_kg.is_finite() || oew_kg >= mtow_kg {
        return Err(PayloadRangeUnavailable::Infeasible(format!(
            "operating empty mass {oew_kg:.0} kg is not below the declared MTOW {mtow_kg:.0} kg, so the fixed aircraft can lift neither payload nor fuel"
        )));
    }
    let effective_limit_kg = report
        .geometry_summary
        .get("effective_structural_payload_limit_kg")
        .copied()
        .or_else(|| effective_structural_payload_limit_kg(config, &report.design, oew_kg));
    // The published MZFW less the modeled OEW bounds the payload as well, the
    // same basis the report's payload-range chart uses.
    let published_limit_kg = presets::get(&config.preset)
        .ok()
        .and_then(|preset| preset.reference.mzfw_kg)
        .map(|mzfw_kg| mzfw_kg - oew_kg);
    let mzfw_limit_kg = [effective_limit_kg, published_limit_kg]
        .into_iter()
        .flatten()
        .filter(|value| value.is_finite() && *value > 0.0)
        .reduce(f64::min);
    let carried_payload_kg = report
        .component_masses
        .get("Payload")
        .copied()
        .unwrap_or(0.0);
    let payload = payload_capacity_estimate(
        config.requirements.max_structural_payload_kg,
        mzfw_limit_kg,
        carried_payload_kg,
        mtow_kg,
        oew_kg,
    );
    let max_payload_kg = payload.capacity_kg;
    let capacity = assess_fuel_capacity(config, &report.design, report);
    let Some(capacity_kg) = capacity.capacity_kg.filter(|value| value.is_finite()) else {
        return Err(PayloadRangeUnavailable::Unsupported(
            "fuel capacity unavailable for this design".to_owned(),
        ));
    };
    let mut basis = fuel_capacity_basis(capacity.evidence).to_owned();
    let structural_capacity_kg = mtow_kg - oew_kg;
    let fuel_capacity_kg = if capacity_kg <= structural_capacity_kg {
        capacity_kg
    } else {
        basis = "MTOW budget".to_owned();
        structural_capacity_kg
    };
    let masses = CornerMasses {
        mtow_kg,
        oew_kg,
        max_payload_kg,
        tank_capacity_kg: capacity_kg,
    };
    let ranges =
        corner_ranges(config, report, &masses).map_err(PayloadRangeUnavailable::Unsupported)?;
    let points = (0..4)
        .map(|index| (ranges.range_m[index], ranges.payload_kg[index]))
        .collect();
    Ok(QuickPayloadRange {
        points,
        oew_kg,
        mtow_kg,
        max_payload_kg,
        payload_basis: payload.basis.to_owned(),
        fuel_capacity_kg,
        fuel_capacity_basis: basis,
        reserve_fuel_kg: ranges.reserve_fuel_kg.to_vec(),
        range_basis: ranges.basis,
        note: ranges.note,
    })
}

/// The still-air range of the closure's carried fuel, m, and the reserve
/// fuel held back from it, kg: the largest distance whose reserve-inclusive
/// plan fits the fuel the closure loads, `min(TOW - ZFW, usable capacity)`,
/// at the closure's takeoff mass, on the closure's mission model.
pub(super) fn quick_range(
    config: &AlasConfig,
    sized: &alas_opt::SizedCandidate,
) -> Result<(f64, f64), String> {
    let model = candidate_mission_model(config, &sized.fuel_artifacts)?;
    let fuel_kg = (sized.takeoff_mass_kg - sized.zero_fuel_mass_kg)
        .min(sized.usable_capacity_kg)
        .max(0.0);
    let range = max_range_with_reserves(
        sized.takeoff_mass_kg,
        fuel_kg,
        &config.fuel_policy,
        &PlannedTrips(&model),
        CORNER_TOLERANCE_M,
    );
    match range.status {
        RangeStatus::ModelFailed(reason) => Err(reason),
        RangeStatus::Solved | RangeStatus::ZeroRange => Ok((range.range_m, range.reserve_fuel_kg)),
    }
}

#[cfg(test)]
mod tests {
    // Tests assert on values they construct here, so a failed expect is the
    // assertion failing, not a library invariant being broken.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn the_achievable_payload_is_the_tightest_of_cap_mzfw_and_mass_budget() {
        let cap = payload_capacity_estimate(30.0, None, 10.0, 100.0, 60.0);
        assert_eq!(cap.capacity_kg, 30.0);
        assert_eq!(cap.basis, "declared structural payload cap");
        let budget = payload_capacity_estimate(30.0, None, 10.0, 100.0, 80.0);
        assert_eq!(budget.capacity_kg, 20.0);
        assert_eq!(budget.basis, "MTOW less operating empty mass budget");
        let mzfw = payload_capacity_estimate(30.0, Some(25.0), 10.0, 100.0, 60.0);
        assert_eq!(mzfw.capacity_kg, 25.0);
        assert_eq!(mzfw.basis, "preset MZFW less modeled operating empty mass");
        let impossible = payload_capacity_estimate(30.0, Some(25.0), 10.0, 100.0, 120.0);
        assert_eq!(impossible.capacity_kg, 0.0);
        assert_eq!(impossible.basis, "MTOW less operating empty mass budget");
        // No cap and no MZFW limit: the structural maximum is unknown, so the
        // analysed payload stands in, bounded by the budget.
        let no_cap = payload_capacity_estimate(f64::NAN, None, 10.0, 100.0, 60.0);
        assert_eq!(no_cap.capacity_kg, 10.0);
        assert!(no_cap.basis.starts_with("analysed carried payload"));
        let heavy_load = payload_capacity_estimate(0.0, None, 55.0, 100.0, 60.0);
        assert_eq!(heavy_load.capacity_kg, 40.0);
        assert_eq!(heavy_load.basis, "MTOW less operating empty mass budget");
        // A non-finite MZFW limit is no limit, but a zero cap with a known
        // MZFW limit keeps the MZFW bound rather than the carried payload.
        let zero_cap = payload_capacity_estimate(0.0, Some(f64::INFINITY), 10.0, 100.0, 60.0);
        assert_eq!(zero_cap.capacity_kg, 10.0);
        let mzfw_only = payload_capacity_estimate(0.0, Some(35.0), 10.0, 100.0, 60.0);
        assert_eq!(mzfw_only.capacity_kg, 35.0);
    }
}
