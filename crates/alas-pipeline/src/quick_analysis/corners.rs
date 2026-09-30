// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The range of each payload-range corner, shared by the report chart and the
//! sandbox Quick Analysis so both publish the same numbers for one report.
//!
//! A corner fixes the payload and the fuel at brake release. Its range is the
//! largest still-air distance whose full fuel plan (trip, contingency,
//! alternate, final reserve; [`alas_mass::payload_range`]) fits that fuel,
//! priced by the production burn model
//! ([`crate::fuel_model::segment_model_from_report`]) under the
//! configuration's fuel policy. Tank fuel is loaded at the ramp and taxi fuel
//! is burned before brake release, so a tank-limited corner carries the tank
//! capacity less taxi fuel at takeoff.
//!
//! Only when that model cannot be built or solved does a corner fall back to
//! the single-point Breguet range with no reserves, and the basis and note
//! say so.

use alas_config::AlasConfig;
use alas_mass::fuel_plan::FuelBurnModel;
use alas_mass::payload_range::{max_range_with_reserves, RangeStatus};
use serde::{Deserialize, Serialize};

use super::breguet::range_model;
use crate::cruise_mass::corner_l_over_d;
use crate::fuel_model::segment_model_from_report;
use crate::full_analysis::AnalysisReport;

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
    ReservesIncluded,
    /// Single-point Breguet cruise with every kilogram of fuel burned.
    #[default]
    BreguetNoReserves,
}

impl RangeBasis {
    /// Short English label of the basis.
    pub const fn label(self) -> &'static str {
        match self {
            Self::ReservesIncluded => "reserve-inclusive fuel plan",
            Self::BreguetNoReserves => "Breguet, NO RESERVES",
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
    /// Reserve fuel the plan holds back at each corner, kg (zero on the
    /// Breguet fallback).
    pub reserve_fuel_kg: [f64; 4],
    /// What the ranges include.
    pub basis: RangeBasis,
    /// The fuel scheme or the reason for the fallback.
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

fn report_l_over_d(report: &AnalysisReport) -> f64 {
    report
        .trimmed_design_point
        .as_ref()
        .map_or(report.design_point.l_over_d, |point| point.l_over_d)
}

/// The corner ranges for `masses`, reserve-inclusive when the production burn
/// model can be built and solved, else the labelled no-reserve Breguet range.
///
/// # Errors
///
/// Returns the reason when neither the burn model nor the Breguet anchor is
/// available for the configured propulsion.
pub fn corner_ranges(
    config: &AlasConfig,
    report: &AnalysisReport,
    masses: &CornerMasses,
) -> Result<CornerRanges, String> {
    match reserve_inclusive(config, report, masses) {
        Ok(ranges) => Ok(ranges),
        Err(reason) => breguet_fallback(config, report, masses, &reason),
    }
}

fn reserve_inclusive(
    config: &AlasConfig,
    report: &AnalysisReport,
    masses: &CornerMasses,
) -> Result<CornerRanges, String> {
    let model = segment_model_from_report(config, report)?;
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
            &model,
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

/// The single-point Breguet corner ranges with no reserves and no taxi, the
/// fallback basis, labelled `BreguetNoReserves`. `reason` is recorded in the
/// note.
///
/// # Errors
///
/// Returns the reason when the propulsion model has no cruise fuel-flow anchor.
pub fn breguet_fallback(
    config: &AlasConfig,
    report: &AnalysisReport,
    masses: &CornerMasses,
    reason: &str,
) -> Result<CornerRanges, String> {
    let l_over_d = report_l_over_d(report);
    let anchor = range_model(config, l_over_d).ok_or_else(|| {
        format!("no cruise fuel-flow anchor for the selected propulsion model ({reason})")
    })?;
    let loads = corner_loads(masses, 0.0);
    let mut ranges = CornerRanges {
        range_m: [0.0; 4],
        payload_kg: [masses.max_payload_kg.max(0.0), 0.0, 0.0, 0.0],
        reserve_fuel_kg: [0.0; 4],
        basis: RangeBasis::BreguetNoReserves,
        note: format!(
            "NO RESERVES: Breguet cruise at Mach {:.3}, {:.0} m, reported L/D {l_over_d:.1} moved to each corner's mid-cruise mass; {}; the reserve-inclusive fuel plan was unavailable ({reason})",
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
            anchor.note,
        ),
    };
    for (index, load) in loads.iter().enumerate() {
        let corner_l_over_d = corner_l_over_d(config, report, load.tow_kg, load.fuel_kg, l_over_d);
        ranges.range_m[index + 1] = range_model(config, corner_l_over_d).map_or(0.0, |leg| {
            leg.range_m(load.tow_kg, load.tow_kg - load.fuel_kg)
        });
        ranges.payload_kg[index + 1] = load.payload_kg;
    }
    Ok(ranges)
}
