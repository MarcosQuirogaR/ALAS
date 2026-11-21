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

pub mod mtow_line;

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
    corner_ranges_at(config, report, masses, None)
}

/// The report's fuel model flown as a range search, with the design cruise altitude at
/// `cruise_level_m` when a chart states the level its range is quoted at.
///
/// The stated level takes the place of the design cruise altitude, so under
/// the `OptimumStep` policy it is the ceiling of the step ladder
/// ([`alas_opt::SegmentMissionModel::for_range_search`]): the aircraft climbs
/// to it when it can and flies the highest level it can climb to otherwise.
/// A stated level below the level the model is built at leaves that level
/// (the range search never lowers it).
pub(crate) fn range_search_model(
    config: &AlasConfig,
    report: &AnalysisReport,
    cruise_level_m: Option<f64>,
) -> Result<alas_opt::SegmentMissionModel, String> {
    let model = report_mission_model(config, report)?;
    let model = match cruise_level_m.filter(|h| h.is_finite() && *h > 0.0) {
        Some(level_m) => model.with_design_cruise_altitude_m(level_m),
        None => model,
    };
    Ok(model.for_range_search())
}

/// [`corner_ranges`] with the cruise level a chart states (`None`: the
/// configured schedule).
///
/// # Errors
///
/// As [`corner_ranges`].
pub fn corner_ranges_at(
    config: &AlasConfig,
    report: &AnalysisReport,
    masses: &CornerMasses,
    cruise_level_m: Option<f64>,
) -> Result<CornerRanges, String> {
    let model = range_search_model(config, report, cruise_level_m)?;
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
/// The
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
/// ([`ChartReserveRule`]). This is the path of the matrix validation block and
/// of the payload-range corner probes.
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

/// The masses a report's payload-range chart is built on, with what limits
/// them: one derivation for the report figure and the design database.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartCornerMasses {
    /// The corner masses.
    pub masses: CornerMasses,
    /// What limited the maximum payload.
    pub payload_basis: &'static str,
    /// Evidence class of the usable tank capacity.
    pub tank_evidence: crate::feasibility::FuelCapacityEvidence,
}

/// The payload-range chart masses of `report` under `config`, or `None` when
/// the report has no main wing or no typed usable-fuel capacity.
///
/// Takeoff-mass limit: the declared MTOW under hard-MTOW sizing (one
/// volume-limited load case does not lower it), the sized takeoff mass
/// otherwise. Maximum payload: the declared cap, the effective or
/// MZFW-derived limit and the MTOW less OEW budget, resolved by
/// [`crate::quick_analysis::payload_capacity_estimate`], all against the
/// *modelled* OEW.
pub fn chart_corner_masses(
    report: &AnalysisReport,
    config: &AlasConfig,
) -> Option<ChartCornerMasses> {
    report.airplane.wings.first()?;
    let masses = &report.component_masses;
    let oew_kg: f64 = alas_mass::breakdown::OEW_KEYS
        .iter()
        .map(|key| masses.get(*key).copied().unwrap_or(0.0))
        .sum();
    let analyzed_payload_kg = masses.get("Payload").copied().unwrap_or(0.0);
    let mtow_kg = if config
        .optimizer
        .objective
        .mtow_sizing
        .sizes_at_declared_mass()
    {
        report.aircraft_mtow_limit_kg(config.requirements.mtow_kg)
    } else {
        report.analysis_takeoff_mass_kg(config.requirements.mtow_kg)
    };
    let effective_payload_limit = report
        .geometry_summary
        .get("effective_structural_payload_limit_kg")
        .copied();
    let published_mzfw_payload_limit =
        alas_config::presets::published_mzfw_kg(config).map(|mzfw_kg| mzfw_kg - oew_kg);
    let mzfw_limit_kg = [effective_payload_limit, published_mzfw_payload_limit]
        .into_iter()
        .flatten()
        .filter(|value| value.is_finite() && *value > 0.0)
        .reduce(f64::min);
    let capacity = crate::quick_analysis::payload_capacity_estimate(
        config.requirements.max_structural_payload_kg,
        mzfw_limit_kg,
        analyzed_payload_kg,
        mtow_kg,
        oew_kg,
    );
    let fuel_capacity = crate::feasibility::assess_fuel_capacity(config, &report.design, report);
    let tank_capacity_kg = fuel_capacity
        .capacity_kg
        .filter(|value| value.is_finite())?;
    Some(ChartCornerMasses {
        masses: CornerMasses {
            mtow_kg,
            oew_kg,
            max_payload_kg: capacity.capacity_kg,
            tank_capacity_kg,
        },
        payload_basis: capacity.basis,
        tank_evidence: fuel_capacity.evidence,
    })
}

/// A single payload point priced like a corner: brake release at `tow_limit_kg`
/// less whatever the tank cannot hold, under `policy`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PayloadPointRange {
    /// Takeoff mass at brake release, kg.
    pub tow_kg: f64,
    /// Takeoff fuel (tank less taxi, or the MTOW budget), kg.
    pub fuel_kg: f64,
    /// Whether the takeoff fuel is the tank's (`true`) or the MTOW budget's.
    pub tank_limited: bool,
    /// Taxi fuel burned before brake release, kg.
    pub taxi_kg: f64,
    /// Still-air range, m.
    pub range_m: f64,
    /// Reserve fuel held back, kg.
    pub reserve_fuel_kg: f64,
    /// The fuel plan at `range_m`, term by term (`None` when it cannot be
    /// priced again).
    pub plan: Option<PlanBreakdown>,
    /// The flight of the priced trip (brake release at `tow_kg` over
    /// `range_m`, the range search's own plan): its initial and final cruise
    /// levels, m, and its time from brake release to the top of climb, s.
    /// `None` when the trip cannot be re-flown.
    pub flight: Option<PointFlight>,
}

/// The terms of a priced point's fuel plan, kg ([`alas_mass::fuel_plan::FuelPlan`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanBreakdown {
    /// Trip fuel, brake release to landing at the destination.
    pub trip_kg: f64,
    /// Contingency.
    pub contingency_kg: f64,
    /// Diversion to the alternate.
    pub alternate_kg: f64,
    /// Final reserve hold.
    pub final_reserve_kg: f64,
    /// Taxi-in budget carried through the flight and burned after landing
    /// (`FuelPlan::taxi_in_fuel_kg`), part of the takeoff fuel but not of
    /// the reserves.
    pub taxi_in_kg: f64,
}

/// The cruise levels and climb time of a priced payload point's trip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointFlight {
    /// Initial cruise level, m.
    pub initial_cruise_level_m: f64,
    /// Cruise level at the top of descent, m: the trip re-flown on the plan
    /// the range search priced it on (`PlannedTrips`), step climbs included.
    pub final_cruise_level_m: f64,
    /// Time from brake release to the top of the climb ladder, s.
    pub climb_time_s: f64,
    /// Fuel from brake release to the top of the climb ladder, kg.
    pub climb_fuel_kg: f64,
    /// Initial cruise altitude capability of the trip, m: the highest level,
    /// up to the `ceiling_m` the caller searched to, at which maximum-climb
    /// thrust leaves 300 ft/min at the top-of-climb mass the trip's own climb
    /// reaches ([`alas_opt::SegmentMissionModel::initial_cruise_capability_m`]);
    /// `None` when it cannot be evaluated or no level qualifies.
    pub initial_cruise_capability_m: Option<f64>,
}

/// The reserve-inclusive range of `payload_kg` on the corner masses `masses`
/// under `policy` (the corner rule of [`corner_ranges`] applied to one
/// payload; the policy may differ from the configured one, as a chart's
/// printed reserve rule does), flown at the stated `cruise_level_m` when the
/// source states one (`None`: the configured schedule). With
/// `capability_ceiling_m` the trip's initial cruise altitude capability is
/// searched up to it ([`PointFlight::initial_cruise_capability_m`]).
///
/// # Errors
///
/// Returns the reason when the fuel model cannot be built, the payload
/// leaves no fuel, or the point cannot be priced.
pub fn payload_point_range(
    config: &AlasConfig,
    report: &AnalysisReport,
    masses: &CornerMasses,
    payload_kg: f64,
    policy: &FuelPolicyConfig,
    cruise_level_m: Option<f64>,
    capability_ceiling_m: Option<f64>,
) -> Result<PayloadPointRange, String> {
    let model = range_search_model(config, report, cruise_level_m)?;
    let taxi_kg = model
        .taxi_fuel_flow_kg_s()
        .map_err(|error| format!("taxi fuel flow is unavailable: {error}"))?
        * policy.taxi_time_min
        * SECONDS_PER_MINUTE;
    let tank_kg = (masses.tank_capacity_kg - taxi_kg).max(0.0);
    let budget_kg = masses.mtow_kg - masses.oew_kg - payload_kg;
    if budget_kg.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return Err(format!(
            "payload {payload_kg:.0} kg leaves no fuel under the {:.0} kg takeoff limit",
            masses.mtow_kg
        ));
    }
    let tank_limited = tank_kg <= budget_kg;
    let fuel_kg = tank_kg.min(budget_kg);
    let tow_kg = masses.oew_kg + payload_kg + fuel_kg;
    let corner = max_range_with_reserves(
        tow_kg,
        fuel_kg,
        policy,
        &PlannedTrips(&model),
        CORNER_TOLERANCE_M,
    );
    if let RangeStatus::ModelFailed(message) = corner.status {
        return Err(message);
    }
    let flight = alas_opt::mdo::plan_for_mission(&model, tow_kg, corner.range_m)
        .ok()
        .and_then(|planned| planned.fly_trip(tow_kg, corner.range_m).ok())
        .map(|flown| PointFlight {
            initial_cruise_level_m: flown.cruise_altitude_m,
            final_cruise_level_m: flown.final_cruise_altitude_m,
            climb_time_s: flown.climb_time_s,
            climb_fuel_kg: flown.climb_fuel_kg,
            initial_cruise_capability_m: capability_ceiling_m
                .and_then(|ceiling_m| {
                    model
                        .initial_cruise_capability_m(tow_kg, corner.range_m, ceiling_m)
                        .ok()
                })
                .flatten(),
        });
    let plan =
        alas_mass::fuel_policy::plan_fuel(policy, &PlannedTrips(&model), tow_kg, corner.range_m)
            .ok()
            .map(|plan| PlanBreakdown {
                trip_kg: plan.trip.kg,
                contingency_kg: plan.contingency.kg,
                alternate_kg: plan.alternate.kg,
                final_reserve_kg: plan.final_reserve.kg,
                taxi_in_kg: plan.taxi_in_fuel_kg(),
            });
    Ok(PayloadPointRange {
        plan,
        tow_kg,
        fuel_kg,
        tank_limited,
        taxi_kg,
        range_m: corner.range_m,
        reserve_fuel_kg: corner.reserve_fuel_kg,
        flight,
    })
}

/// The long-range cruise a payload point is flown at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LongRangeCruisePoint {
    /// Long-range cruise true airspeed, m/s (99 % of the maximum specific
    /// range, high-speed side).
    pub tas_m_s: f64,
    /// True airspeed of maximum specific range at the same state, m/s.
    pub max_range_tas_m_s: f64,
    /// Long-range cruise Mach at `altitude_m`.
    pub mach: f64,
    /// Mean cruise mass the speed was found at, kg.
    pub mass_kg: f64,
    /// Mean cruise level the speed was found at, m.
    pub altitude_m: f64,
    /// Specific range at the long-range cruise speed over the maximum.
    pub specific_range_ratio: f64,
    /// Passes of the speed-mass fixed point.
    pub passes: usize,
}

/// Passes of the long-range cruise speed against the mean cruise mass and
/// level of the trip it produces (the speed moves the fuel, the fuel moves
/// the mass, the mass moves the speed; each pass moves it far less).
const LONG_RANGE_PASSES: usize = 4;

/// Relative change of the long-range cruise speed that ends the passes.
const LONG_RANGE_SPEED_TOLERANCE: f64 = 1.0e-3;

/// Keep the profile's upper descent speeds at or below the cruise speed they join.
///
/// A long-range cruise below the schedule's upper descent rungs would make the aircraft accelerate through a descent to reach them,
/// a transition the mission model refuses ("speed schedule not attained"):
/// the schedule's rungs are operating-practice limits of the configured
/// speed (a descent at 220 KCAS), and a slower cruise descends at its own
/// speed. The climb rungs are left alone: the climb is flown at the
/// aircraft's own climb speed, below the cruise, and slowing it breaks the
/// lift-coefficient limit at heavy mass. A calibrated-airspeed rung is capped at the
/// cruise's equivalent airspeed at `altitude_m` (incompressible, `sqrt(sigma)`:
/// below 1 % in the low Mach number of a turboprop).
fn cap_schedule_at_cruise(
    profile: &mut alas_config::MissionProfileConfig,
    cruise_tas_m_s: f64,
    altitude_m: f64,
    isa_deviation_c: f64,
) -> Result<(), String> {
    use alas_config::SpeedReference;
    let cap = match profile.climb_descent_speed_reference {
        SpeedReference::TrueAirspeed => cruise_tas_m_s,
        SpeedReference::CalibratedAirspeed => {
            let density = |h: f64| {
                alas_atmo::us1976_try_compute_values(h, isa_deviation_c)
                    .map(|values| values.density_kg_m3)
                    .map_err(|error| format!("atmosphere at {h:.0} m: {error:?}"))
            };
            cruise_tas_m_s * (density(altitude_m)? / density(0.0)?).sqrt()
        }
    };
    for speed in [
        &mut profile.descent_1_air_speed_m_s,
        &mut profile.descent_2_air_speed_m_s,
        &mut profile.descent_3_air_speed_m_s,
    ] {
        *speed = speed.min(cap);
    }
    Ok(())
}

/// `config` flown at the long-range cruise speed of the payload point
/// `payload_kg` on `masses` under `policy`, and the speed.
///
/// A range quoted "at long-range cruise" is flown at the speed of 99 % of the
/// maximum specific range on the high-speed side
/// ([`alas_opt::SegmentMissionModel::long_range_cruise`]), found at the mean
/// cruise mass and level of the trip it produces: the point is priced at the
/// configured speed first, its mean cruise state read, the speed moved to
/// that state's long-range cruise, and so on until the speed settles. One
/// speed is flown for the whole cruise (the profile carries one true airspeed
/// or Mach per cruise rung), so a trip whose mass changes a great deal flies
/// the speed of its mean state, not a speed per instant.
///
/// A turboprop's profile speed is a true airspeed declared for the configured
/// cruise level (the speed found is converted to it); a turbofan's is the Mach
/// the configuration derives it from ([`AlasConfig::with_cruise_mach`]).
///
/// # Errors
///
/// The reason when the point cannot be priced or no speed qualifies.
pub fn long_range_cruise_config(
    config: &AlasConfig,
    report: &AnalysisReport,
    masses: &CornerMasses,
    payload_kg: f64,
    policy: &FuelPolicyConfig,
    cruise_level_m: Option<f64>,
) -> Result<(AlasConfig, LongRangeCruisePoint), String> {
    use alas_opt::mdo::propulsion::DeckKind;
    let mut flown = config.clone();
    let mut passes = 0;
    let mut previous_tas = f64::NAN;
    loop {
        let model = range_search_model(&flown, report, cruise_level_m)?;
        let point = payload_point_range(
            &flown,
            report,
            masses,
            payload_kg,
            policy,
            cruise_level_m,
            None,
        )
        .map_err(|reason| {
            format!(
                "pass {passes} flown at {:.1} m/s: {reason}",
                flown.mission.profile.cruise_2_air_speed_m_s
            )
        })?;
        let flight = point
            .flight
            .ok_or_else(|| "the point's trip cannot be re-flown".to_owned())?;
        let altitude_m = 0.5 * (flight.initial_cruise_level_m + flight.final_cruise_level_m);
        // Mean cruise mass: brake release less the climb fuel and half the
        // remaining trip fuel (descent and the landing allowance are the
        // small remainder of the trip).
        let trip_kg = point
            .plan
            .map_or(point.fuel_kg - point.reserve_fuel_kg, |plan| plan.trip_kg);
        let mass_kg = point.tow_kg - flight.climb_fuel_kg - 0.5 * (trip_kg - flight.climb_fuel_kg);
        let turbofan = model.propulsion.kind() == DeckKind::Turbofan;
        let reference_tas = if turbofan {
            model.cruise_true_airspeed_m_s(altitude_m)?
        } else {
            model.profile.cruise_2_air_speed_m_s
        };
        let found = model.long_range_cruise(mass_kg, altitude_m, reference_tas)?;
        passes += 1;
        let sound_m_s = alas_atmo::us1976_try_compute_values(altitude_m, model.isa_deviation_c)
            .map_err(|error| format!("atmosphere at {altitude_m:.0} m: {error:?}"))?
            .speed_of_sound_m_s;
        let mach = found.tas_m_s / sound_m_s;
        let settled = previous_tas.is_finite()
            && ((found.tas_m_s - previous_tas) / previous_tas).abs() < LONG_RANGE_SPEED_TOLERANCE;
        previous_tas = found.tas_m_s;
        if turbofan {
            flown = config.with_cruise_mach(Some(mach));
        } else {
            // The profile field is a true airspeed declared for the configured
            // cruise level and flown at the same equivalent airspeed at any
            // other level (`SegmentMissionModel::cruise_speed_at_level`); the
            // speed was found at the trip's own level, so it is written as the
            // declared speed that flies it there. Written unconverted, the
            // A400M flew its 187.5 m/s found at 8,926 m as 160 m/s there,
            // under the 1.3 g buffet floor at every level, and the next pass
            // planned its cruise at the 609 m floor.
            let declared_m_s = model.declared_cruise_speed_for(altitude_m, found.tas_m_s)?;
            flown = config.clone();
            let profile = &mut flown.mission.profile;
            profile.cruise_1_air_speed_m_s = declared_m_s;
            profile.cruise_2_air_speed_m_s = declared_m_s;
            profile.cruise_3_air_speed_m_s = declared_m_s;
            cap_schedule_at_cruise(profile, found.tas_m_s, altitude_m, model.isa_deviation_c)?;
        }
        if settled || passes >= LONG_RANGE_PASSES {
            return Ok((
                flown,
                LongRangeCruisePoint {
                    tas_m_s: found.tas_m_s,
                    max_range_tas_m_s: found.max_range_tas_m_s,
                    mach,
                    mass_kg,
                    altitude_m,
                    specific_range_ratio: found.specific_range_m_kg / found.max_specific_range_m_kg,
                    passes,
                },
            ));
        }
    }
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
