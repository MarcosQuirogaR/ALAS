// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The range of each payload-range corner, shared by the report chart and the
//! sandbox Quick Analysis so both publish the same numbers for one report.
//!
//! A corner fixes the payload and the fuel at brake release. Its range is the
//! largest still-air distance whose full fuel plan (trip, contingency,
//! alternate, final reserve; [`alas_mass::payload_range`]) fits that fuel,
//! priced by the report's fuel model
//! ([`crate::fuel_model::report_mission_model`], flown as a range search:
//! [`alas_opt::SegmentMissionModel::for_range_search`], so a corner several
//! times longer than the declared sector is not capped at that sector's
//! level) under the configuration's fuel policy, every probed trip flown on
//! the plan frozen for its own takeoff mass and range
//! ([`alas_opt::mdo::PlannedTrips`]), step climbs included, as the sizing
//! closure flies its own. Tank fuel is loaded at the ramp and taxi fuel is
//! burned before brake release, so a tank-limited corner carries the tank
//! capacity less taxi fuel at takeoff.
//!
//! A corner the model cannot price has no range: the chart is unavailable,
//! never drawn on another basis.

use alas_config::{AlasConfig, FuelPolicyConfig, FuelScheme};
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
    let model = report_mission_model(config, report)?.for_range_search();
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

/// The reserve rule a published chart prints in its own notes. Every field
/// left `None` is not printed on that chart, and the configured fuel policy
/// supplies it (EASA basic by default), so a chart is flown under its own
/// stated convention wherever it states one and under the configured one
/// where it does not: the same rule for every chart.
///
/// Post-diagnosis definition fix (2026-10-13, owner decision 2): the
/// DC-10 and 747-400 charts print FAR reserves (14 CFR 121.645: ten percent of
/// the trip time), which the corners used to price as EASA five percent.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChartReserveRule {
    /// Operating rule printed on the chart.
    pub scheme: Option<FuelScheme>,
    /// Destination-alternate distance printed on the chart, nmi.
    pub alternate_distance_nmi: Option<f64>,
    /// Final-reserve hold printed on the chart, min.
    pub final_reserve_hold_min: Option<f64>,
}

impl ChartReserveRule {
    /// The rule printed on a chart that prints none: the configured policy.
    pub const NOT_PRINTED: Self = Self {
        scheme: None,
        alternate_distance_nmi: None,
        final_reserve_hold_min: None,
    };

    /// Read the optional `reserve_rule` object of a chart-mass entry; extra
    /// keys (source, page, note) are provenance and are ignored.
    ///
    /// # Errors
    ///
    /// Returns the reason when the object does not parse.
    pub fn from_json(value: Option<&serde_json::Value>) -> Result<Self, String> {
        match value {
            None | Some(serde_json::Value::Null) => Ok(Self::NOT_PRINTED),
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|error| format!("reserve_rule: {error}")),
        }
    }

    /// The fuel policy the chart is flown under: `base` with the printed
    /// fields replaced.
    pub fn apply(&self, base: &FuelPolicyConfig) -> FuelPolicyConfig {
        let mut policy = base.clone();
        if let Some(scheme) = self.scheme {
            policy.scheme = scheme;
        }
        if let Some(nmi) = self.alternate_distance_nmi {
            policy.alternate_distance_nmi = nmi;
        }
        if let Some(min) = self.final_reserve_hold_min {
            policy.final_reserve_hold_min = min;
        }
        policy
    }
}

/// The masses of a published payload-range chart, kg: the inputs of a
/// chart-mass corner flight. They are chart quantities (takeoff-mass line,
/// usable fuel of the chart's document, payload or zero-fuel-mass difference
/// between corners B and C), never a modelled mass and never a chart range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartMasses {
    /// Takeoff mass of the chart's line, kg.
    pub tow_kg: f64,
    /// Usable fuel capacity at the ramp, kg.
    pub tank_kg: f64,
    /// Payload (or zero-fuel mass) at corner B less that at corner C, kg.
    pub delta_payload_kg: f64,
    /// Reserve rule the chart prints ([`ChartReserveRule::NOT_PRINTED`] when
    /// it prints none).
    pub reserve_rule: ChartReserveRule,
    /// Cruise Mach the chart prints (the 787-9 box "MACH 0.85 CRUISE", the
    /// 747-400 title "0.85 Mach Cruise", the A340 "M = 0.82"); `None` when
    /// it prints none and the configured requirement is flown.
    pub cruise_mach: Option<f64>,
}

/// Corners B and C flown at chart masses, in still air under the chart's
/// printed reserve rule (the configured fuel policy where it prints none),
/// priced by the report's segment mission model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartCorners {
    /// Fuel scheme the corners were flown under (the chart's own where it
    /// prints one).
    pub scheme: FuelScheme,
    /// Destination-alternate distance of that policy, nmi.
    pub alternate_distance_nmi: f64,
    /// Final-reserve hold of that policy, min.
    pub final_reserve_hold_min: f64,
    /// Taxi fuel burned before brake release, kg.
    pub taxi_kg: f64,
    /// Takeoff fuel of corner B: tank less taxi less the payload difference, kg.
    pub fuel_b_kg: f64,
    /// Takeoff fuel of corner C: tank less taxi, kg.
    pub fuel_c_kg: f64,
    /// Range of corner B, m.
    pub range_b_m: f64,
    /// Range of corner C, m.
    pub range_c_m: f64,
    /// Reserve fuel held back at corner B, kg.
    pub reserve_b_kg: f64,
    /// Reserve fuel held back at corner C, kg.
    pub reserve_c_kg: f64,
}

/// Price corners B and C at the masses of a published chart.
///
/// Both corners lie on the chart's takeoff-mass line `tow_kg`: corner C
/// carries the tank less taxi fuel, corner B carries that less the payload
/// difference. The ranges therefore do not depend on the modelled empty
/// mass; the report's drag, propulsion, mission schedule and fuel policy are
/// what is tested. Flown as a range search
/// ([`alas_opt::SegmentMissionModel::for_range_search`]), as
/// [`corner_ranges`]. The reserve rule is the one printed on the chart
/// ([`ChartReserveRule`]). This is the path of the kappa calibration probe and
/// of the matrix validation block.
///
/// # Errors
///
/// Returns the reason when the fuel model cannot be built or a corner cannot
/// be priced on it.
pub fn chart_mass_corners(
    config: &AlasConfig,
    report: &AnalysisReport,
    masses: &ChartMasses,
) -> Result<ChartCorners, String> {
    let config = &config.with_cruise_mach(masses.cruise_mach);
    let model = report_mission_model(config, report)?.for_range_search();
    let taxi_kg = model
        .taxi_fuel_flow_kg_s()
        .map_err(|error| format!("taxi fuel flow: {error}"))?
        * config.fuel_policy.taxi_time_min
        * SECONDS_PER_MINUTE;
    let policy = masses.reserve_rule.apply(&config.fuel_policy);
    let fuel_c_kg = masses.tank_kg - taxi_kg;
    let fuel_b_kg = fuel_c_kg - masses.delta_payload_kg;
    let price = |fuel_kg: f64| -> Result<(f64, f64), String> {
        let corner = max_range_with_reserves(
            masses.tow_kg,
            fuel_kg,
            &policy,
            &PlannedTrips(&model),
            CORNER_TOLERANCE_M,
        );
        match corner.status {
            RangeStatus::ModelFailed(reason) => Err(reason),
            _ => Ok((corner.range_m, corner.reserve_fuel_kg)),
        }
    };
    let (range_b_m, reserve_b_kg) = price(fuel_b_kg)?;
    let (range_c_m, reserve_c_kg) = price(fuel_c_kg)?;
    Ok(ChartCorners {
        scheme: policy.scheme,
        alternate_distance_nmi: policy.alternate_distance_nmi,
        final_reserve_hold_min: policy.final_reserve_hold_min,
        taxi_kg,
        fuel_b_kg,
        fuel_c_kg,
        range_b_m,
        range_c_m,
        reserve_b_kg,
        reserve_c_kg,
    })
}

// Tests construct every fixture they assert on, so a failed unwrap is the
// assertion failing rather than a library invariant breaking.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chart_that_prints_no_rule_flies_the_configured_policy() {
        let base = FuelPolicyConfig::default();
        assert_eq!(ChartReserveRule::NOT_PRINTED.apply(&base), base);
        assert_eq!(
            ChartReserveRule::from_json(None).unwrap(),
            ChartReserveRule::NOT_PRINTED
        );
        // Provenance keys beside the rule are ignored.
        let rule = ChartReserveRule::from_json(Some(&serde_json::json!({"source": "AC p1"})));
        assert_eq!(rule.unwrap(), ChartReserveRule::NOT_PRINTED);
    }

    #[test]
    fn printed_fields_replace_the_policy_and_unprinted_ones_stay() {
        let base = FuelPolicyConfig::default();
        let rule = ChartReserveRule::from_json(Some(&serde_json::json!({
            "scheme": "faa_flag_supplemental",
            "alternate_distance_nmi": 100.0,
            "source": "chart note",
        })))
        .unwrap();
        let policy = rule.apply(&base);
        assert_eq!(policy.scheme, FuelScheme::FaaFlagSupplemental);
        assert_eq!(policy.alternate_distance_nmi, 100.0);
        assert_eq!(policy.final_reserve_hold_min, base.final_reserve_hold_min);
        assert_eq!(policy.taxi_time_min, base.taxi_time_min);
        assert!(
            ChartReserveRule::from_json(Some(&serde_json::json!({"scheme": "bogus"}))).is_err()
        );
    }
}
