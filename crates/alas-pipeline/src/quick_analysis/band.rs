// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Whether a design mission is reachable inside a takeoff-mass band.
//!
//! This is a quick estimate: it holds the operating empty mass at the
//! report's value at both ends of the band, although a heavier takeoff mass
//! would grow the airframe. At each band end the aircraft carries the design
//! payload and the most fuel the mass and tanks allow, and the reserve
//! inclusive range comes from [`alas_mass::payload_range`].

use alas_config::AlasConfig;
use alas_mass::fuel_plan::FuelBurnModel;
use alas_mass::payload_range::{max_range_with_reserves, RangeStatus};

use super::corners::CORNER_TOLERANCE_M;
use crate::feasibility::assess_fuel_capacity;
use crate::fuel_model::segment_model_from_report;
use crate::full_analysis::AnalysisReport;

/// Where the design mission sits relative to the band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandStatus {
    /// Out of reach at the lower band mass and within reach at the upper.
    Inside,
    /// Already reachable at the lower band mass: every mass in the band is
    /// heavier than the mission needs.
    BandTooHeavy,
    /// Out of reach even at the upper band mass.
    BandTooLight,
    /// The estimate could not be made; see the note.
    Unavailable,
}

/// The quick reachability estimate of a design mission over a mass band.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignMissionBand {
    /// Reserve-inclusive range at the lower band mass, m.
    pub range_at_lo_m: f64,
    /// Reserve-inclusive range at the upper band mass, m.
    pub range_at_hi_m: f64,
    /// The verdict.
    pub status: BandStatus,
    /// Why the status is `Unavailable`, or the basis of the estimate.
    pub note: String,
}

impl DesignMissionBand {
    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            range_at_lo_m: 0.0,
            range_at_hi_m: 0.0,
            status: BandStatus::Unavailable,
            note: reason.into(),
        }
    }
}

/// The verdict for ranges at the two band ends against the design range.
pub fn classify_band(range_at_lo_m: f64, range_at_hi_m: f64, design_range_m: f64) -> BandStatus {
    if range_at_lo_m >= design_range_m {
        BandStatus::BandTooHeavy
    } else if range_at_hi_m < design_range_m {
        BandStatus::BandTooLight
    } else {
        BandStatus::Inside
    }
}

/// Quick estimate of whether the design mission (`design_payload_kg` over
/// `design_range_m`, still air) is reachable for takeoff masses in
/// `[band_lo_kg, band_hi_kg]`.
///
/// Operating empty mass is held at the report's value, so the estimate is
/// optimistic at the heavy end and pessimistic at the light end relative to
/// a resized airframe. The fuel at each end is the smaller of the tank
/// capacity less taxi fuel and the mass left after empty mass and payload.
pub fn design_mission_band_check(
    config: &AlasConfig,
    report: &AnalysisReport,
    design_payload_kg: f64,
    design_range_m: f64,
    band_lo_kg: f64,
    band_hi_kg: f64,
) -> DesignMissionBand {
    let finite = [design_payload_kg, design_range_m, band_lo_kg, band_hi_kg]
        .iter()
        .all(|value| value.is_finite());
    if !finite || design_payload_kg < 0.0 || design_range_m <= 0.0 || band_lo_kg > band_hi_kg {
        return DesignMissionBand::unavailable("the band, payload or design range is not valid");
    }
    let Some(tank_kg) = assess_fuel_capacity(config, &report.design, report)
        .capacity_kg
        .filter(|value| value.is_finite())
    else {
        return DesignMissionBand::unavailable("usable fuel capacity is unavailable");
    };
    let model = match segment_model_from_report(config, report) {
        Ok(model) => model,
        Err(reason) => return DesignMissionBand::unavailable(reason),
    };
    let taxi_kg = match model.taxi_fuel_flow_kg_s() {
        Ok(flow) => flow * config.fuel_policy.taxi_time_min * 60.0,
        Err(error) => return DesignMissionBand::unavailable(error.to_string()),
    };
    let oew_kg = super::report_oew_kg(report);
    let range_at = |tow_max_kg: f64| -> Result<f64, String> {
        let fuel_kg = (tank_kg - taxi_kg)
            .min(tow_max_kg - oew_kg - design_payload_kg)
            .max(0.0);
        let corner = max_range_with_reserves(
            oew_kg + design_payload_kg + fuel_kg,
            fuel_kg,
            &config.fuel_policy,
            &model,
            CORNER_TOLERANCE_M,
        );
        match corner.status {
            RangeStatus::ModelFailed(message) => Err(message),
            RangeStatus::Solved | RangeStatus::ZeroRange => Ok(corner.range_m),
        }
    };
    let (range_at_lo_m, range_at_hi_m) = match (range_at(band_lo_kg), range_at(band_hi_kg)) {
        (Ok(lo), Ok(hi)) => (lo, hi),
        (Err(reason), _) | (_, Err(reason)) => return DesignMissionBand::unavailable(reason),
    };
    DesignMissionBand {
        range_at_lo_m,
        range_at_hi_m,
        status: classify_band(range_at_lo_m, range_at_hi_m, design_range_m),
        note: format!(
            "quick estimate with the operating empty mass held at {oew_kg:.0} kg, scheme {}",
            config.fuel_policy.scheme.as_str()
        ),
    }
}
