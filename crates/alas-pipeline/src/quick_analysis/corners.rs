// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The range of each payload-range corner, shared by the report chart and the
//! sandbox Quick Analysis so both publish the same numbers for one report.
//!
//! A corner fixes the payload and the fuel at brake release. Its range is the
//! largest still-air distance whose full fuel plan (trip, contingency,
//! alternate, final reserve; [`alas_mass::payload_range`]) fits that fuel,
//! priced by the report's fuel model
//! ([`crate::fuel_model::report_mission_model`]) under the configuration's
//! fuel policy, every probed trip flown on the plan frozen for its own
//! takeoff mass and range ([`alas_opt::mdo::PlannedTrips`]), step climbs
//! included, as the sizing closure flies its own. Tank fuel is loaded at the ramp and taxi fuel is burned
//! before brake release, so a tank-limited corner carries the tank capacity
//! less taxi fuel at takeoff.
//!
//! A corner the model cannot price has no range: the chart is unavailable,
//! never drawn on another basis.

use alas_config::AlasConfig;
use alas_mass::fuel_plan::FuelBurnModel;
use alas_mass::payload_range::{max_range_with_reserves, RangeStatus};
use serde::{Deserialize, Serialize};

use crate::fuel_model::report_mission_model;
use crate::full_analysis::AnalysisReport;
use alas_opt::mdo::PlannedTrips;

/// Resolution of the corner range search, m (engineering choice: far below
/// the model's own accuracy and the chart's reading resolution).
pub const CORNER_TOLERANCE_M: f64 = 500.0;

/// Seconds per minute for the taxi allowance.
const SECONDS_PER_MINUTE: f64 = 60.0;

/// What a corner range includes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RangeBasis {
    /// Trip plus the configured reserves, priced by the segment mission model.
    #[default]
    ReservesIncluded,
}

impl RangeBasis {
    /// Short English label of the basis.
    pub const fn label(self) -> &'static str {
        match self {
            Self::ReservesIncluded => "reserve-inclusive fuel plan",
        }
    }
}

/// The masses a corner set is built from, all kg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CornerMasses {
    /// Takeoff-mass limit the corners respect.
    pub mtow_kg: f64,
    /// Operating empty mass.
    pub oew_kg: f64,
    /// Achievable maximum payload of corner A and B.
    pub max_payload_kg: f64,
    /// Usable tank capacity as loaded at the ramp, before any MTOW budget.
    pub tank_capacity_kg: f64,
}

/// Ranges, payloads and reserves of the corners A (zero range), B, C and D.
#[derive(Debug, Clone, PartialEq)]
pub struct CornerRanges {
    /// Still-air range of each corner, m.
    pub range_m: [f64; 4],
    /// Payload at each corner, kg.
    pub payload_kg: [f64; 4],
    /// Reserve fuel the plan holds back at each corner, kg.
    pub reserve_fuel_kg: [f64; 4],
    /// What the ranges include.
    pub basis: RangeBasis,
    /// The fuel scheme and allowances the ranges were priced under.
    pub note: String,
}

/// Payload, takeoff fuel and takeoff mass of a corner.
#[derive(Debug, Clone, Copy)]
pub(super) struct CornerLoad {
    pub payload_kg: f64,
    pub fuel_kg: f64,
    pub tow_kg: f64,
}

/// The B, C and D loads with `taxi_kg` burned before brake release.
pub(super) fn corner_loads(masses: &CornerMasses, taxi_kg: f64) -> [CornerLoad; 3] {
    let budget_kg = (masses.mtow_kg - masses.oew_kg).max(0.0);
    let tank_kg = (masses.tank_capacity_kg - taxi_kg).max(0.0);
    let load = |payload_kg: f64| {
        let fuel_kg = tank_kg.min(budget_kg - payload_kg).max(0.0);
        CornerLoad {
            payload_kg,
            fuel_kg,
            tow_kg: masses.oew_kg + payload_kg + fuel_kg,
        }
    };
    let max_payload_kg = masses.max_payload_kg.max(0.0);
    [
        load(max_payload_kg),
        load((budget_kg - tank_kg).min(max_payload_kg).max(0.0)),
        load(0.0),
    ]
}

/// The reserve-inclusive corner ranges for `masses`.
///
/// # Errors
///
/// Returns the reason when the report's fuel model cannot be built or a
/// corner cannot be priced on it.
pub fn corner_ranges(
    config: &AlasConfig,
    report: &AnalysisReport,
    masses: &CornerMasses,
) -> Result<CornerRanges, String> {
    let model = report_mission_model(config, report)?;
    let policy = &config.fuel_policy;
    let taxi_kg = model
        .taxi_fuel_flow_kg_s()
        .map_err(|error| format!("taxi fuel flow is unavailable: {error}"))?
        * policy.taxi_time_min
        * SECONDS_PER_MINUTE;
    let loads = corner_loads(masses, taxi_kg);
    let mut ranges = CornerRanges {
        range_m: [0.0; 4],
        payload_kg: [masses.max_payload_kg.max(0.0), 0.0, 0.0, 0.0],
        reserve_fuel_kg: [0.0; 4],
        basis: RangeBasis::ReservesIncluded,
        note: format!(
            "reserve-inclusive fuel plan, scheme {} (alternate {:.0} nmi, taxi {taxi_kg:.0} kg burned before brake release), priced by the segment mission model in still air",
            policy.scheme.as_str(),
            policy.alternate_distance_nmi,
        ),
    };
    for (index, load) in loads.iter().enumerate() {
        let corner = max_range_with_reserves(
            load.tow_kg,
            load.fuel_kg,
            policy,
            &PlannedTrips(&model),
            CORNER_TOLERANCE_M,
        );
        if let RangeStatus::ModelFailed(message) = corner.status {
            return Err(message);
        }
        ranges.range_m[index + 1] = corner.range_m;
        ranges.payload_kg[index + 1] = load.payload_kg;
        ranges.reserve_fuel_kg[index + 1] = corner.reserve_fuel_kg;
    }
    Ok(ranges)
}
