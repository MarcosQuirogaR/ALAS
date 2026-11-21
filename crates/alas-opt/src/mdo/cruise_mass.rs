// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mid-cruise mass the in-loop cruise checks are evaluated at.
//!
//! # Definition
//!
//! The mid-cruise state is that of the design mission flown from the
//! candidate's own takeoff loading: the aircraft leaves at the takeoff mass
//! `TOW` carrying `F` kg of fuel at brake release, lands with the fuel its
//! dispatch plan keeps beyond the trip (`R`: contingency, alternate, final
//! reserve, additional and extra fuel and the taxi-in budget,
//! [`FuelPlan::destination_landing_fuel_kg`]), and so burns `F - R` in the
//! trip. Mid-cruise is the mean of the Breguet endpoints:
//!
//! `m_mid = TOW - (F - R) / 2`.
//!
//! Under every mission-closed MTOW mode the loading is the dispatch itself
//! (`F` is its takeoff fuel), so `F - R` is the dispatch's trip fuel and the
//! mass is the closed mission's own mid-cruise mass. Under Hard MTOW the
//! loading is the maximum the weight and volume budgets admit
//! (`alas_mass::loading::MtowFuelLoading`), and the mass is the mid-cruise
//! state of the longest mission that loading flies, a real flight state of
//! the aircraft at that takeoff mass, rather than the takeoff mass of one
//! mission combined with the trip of another.
//!
//! # Reserves of the longer mission
//!
//! A contingency priced as a share `f` of the trip
//! ([`FuelRule::TripFraction`]) grows with the trip, so the trip of the
//! loading is solved from `T + f T + R_other = F`, `R_other` being every
//! other quantity left on landing. Every other reserve (holds, diversion,
//! final reserve, flight-time shares) is kept at the dispatch plan's value:
//! those quantities are priced at the landing mass and flight time of the
//! plan's own mission and are not re-flown here.
//!
//! # A loading no closure flew
//!
//! A report analysed at a declared takeoff mass (a baseline at the certified
//! MTOW with the fuel that mass admits) has no closure of its own. Its
//! cruise point is the same state ([`cruise_mass_of_loading`]): the analysed
//! loading flies its own fuel down to reserves, and the mid-cruise mass is
//! the takeoff mass less half that mission's trip fuel. It is the one
//! mission the loading physically flies. The reserves are those of the
//! design mission of `AlasConfig::design_mission` priced from the takeoff
//! mass (the reserve rule its source quotes), with a trip-share contingency
//! rescaled as above. The design range itself is not the mission: a charted
//! range at maximum payload flown from a lighter-payload loading would land
//! with fuel unburned, which describes another loading.
//!
//! Units: kilograms.

use alas_mass::fuel_plan::{FuelPlan, FuelRule};

/// Trip fuel, kg, of the mission flown from `loaded_fuel_kg` of fuel at
/// brake release with the reserves of `plan`, never negative.
///
/// `NaN` when an input is not finite.
#[must_use]
pub fn trip_fuel_of_loading_kg(loaded_fuel_kg: f64, plan: &FuelPlan) -> f64 {
    let landing_fuel_kg = plan.destination_landing_fuel_kg();
    if !(loaded_fuel_kg.is_finite() && landing_fuel_kg.is_finite()) {
        return f64::NAN;
    }
    let trip_kg = match plan.contingency.rule {
        FuelRule::TripFraction { fraction } if fraction.is_finite() && fraction >= 0.0 => {
            let other_kg = landing_fuel_kg - plan.contingency.kg;
            (loaded_fuel_kg - other_kg) / (1.0 + fraction)
        }
        _ => loaded_fuel_kg - landing_fuel_kg,
    };
    trip_kg.max(0.0)
}

/// Mid-cruise mass, kg, of the mission flown from a takeoff loading of
/// `takeoff_mass_kg` carrying `loaded_fuel_kg` of fuel at brake release,
/// with the reserves of `plan` (see the module documentation).
///
/// The burned fuel is bounded to `[0, loaded_fuel_kg]`; `NaN` when an input
/// is not finite.
#[must_use]
pub fn mid_cruise_mass_of_loading_kg(
    takeoff_mass_kg: f64,
    loaded_fuel_kg: f64,
    plan: &FuelPlan,
) -> f64 {
    if !takeoff_mass_kg.is_finite() {
        return f64::NAN;
    }
    let trip_kg = trip_fuel_of_loading_kg(loaded_fuel_kg, plan);
    if trip_kg.is_nan() {
        return f64::NAN;
    }
    takeoff_mass_kg - 0.5 * trip_kg.min(loaded_fuel_kg.max(0.0))
}

/// The mid-cruise state of the mission a takeoff loading flies on all of its
/// fuel but the reserves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadingCruiseMass {
    /// Mass at brake release, kg.
    pub takeoff_mass_kg: f64,
    /// Trip fuel the mission burns, kg.
    pub trip_fuel_kg: f64,
    /// Fuel on board at the destination, kg: the reserves.
    pub landing_fuel_kg: f64,
    /// `takeoff_mass_kg - trip_fuel_kg / 2`, kg.
    pub mid_cruise_mass_kg: f64,
    /// Still-air range of the design mission whose plan prices the
    /// reserves, m. Context only: the loading's own mission is longer or
    /// shorter as its fuel allows.
    pub reserve_plan_range_m: f64,
}

/// The mid-cruise state of the mission flown from `takeoff_mass_kg` on
/// `loaded_fuel_kg` of fuel at brake release down to the reserves of
/// `reserve_plan`, priced over `reserve_plan_range_m` (m); see the module
/// documentation.
///
/// `None` when an input is not finite or the takeoff mass is not positive.
#[must_use]
pub fn cruise_mass_of_loading_with_plan(
    takeoff_mass_kg: f64,
    loaded_fuel_kg: f64,
    reserve_plan_range_m: f64,
    reserve_plan: &FuelPlan,
) -> Option<LoadingCruiseMass> {
    let mid_cruise_mass_kg =
        mid_cruise_mass_of_loading_kg(takeoff_mass_kg, loaded_fuel_kg, reserve_plan);
    if !(mid_cruise_mass_kg.is_finite() && takeoff_mass_kg > 0.0) {
        return None;
    }
    let trip_fuel_kg = 2.0 * (takeoff_mass_kg - mid_cruise_mass_kg);
    Some(LoadingCruiseMass {
        takeoff_mass_kg,
        trip_fuel_kg,
        landing_fuel_kg: loaded_fuel_kg - trip_fuel_kg,
        mid_cruise_mass_kg,
        reserve_plan_range_m,
    })
}

/// The mid-cruise state of the aircraft `artifacts` describes flown from
/// `takeoff_mass_kg` on `loaded_fuel_kg` of fuel at brake release down to
/// the reserves of the design mission of `config` (see the module
/// documentation).
///
/// # Errors
///
/// Why the design mission's reserves could not be priced at that mass.
pub fn cruise_mass_of_loading(
    config: &alas_config::AlasConfig,
    artifacts: &super::types::CandidateFuelArtifacts,
    takeoff_mass_kg: f64,
    loaded_fuel_kg: f64,
) -> Result<LoadingCruiseMass, String> {
    let (model, range_m, policy) =
        super::sizing::candidate_model::design_mission_model(config, artifacts)?;
    let plan = alas_mass::fuel_policy::plan_fuel(&policy, &model, takeoff_mass_kg, range_m)
        .map_err(|error| {
            format!(
                "the reserves of the design mission of {:.0} km cannot be priced from {takeoff_mass_kg:.0} kg: {error}",
                range_m / 1_000.0
            )
        })?;
    cruise_mass_of_loading_with_plan(takeoff_mass_kg, loaded_fuel_kg, range_m, &plan).ok_or_else(
        || {
            format!(
                "the loading of {takeoff_mass_kg:.0} kg with {loaded_fuel_kg:.0} kg of fuel has no finite mid-cruise mass"
            )
        },
    )
}

/// Resolution of the range search that finds the mission a takeoff loading
/// flies, m: the payload-range corners' own (`CORNER_TOLERANCE_M` of the
/// pipeline), far below the distance over which a step position moves.
const LOADING_RANGE_TOLERANCE_M: f64 = 500.0;

/// The state of the cruise segment a mission flies `mass_kg` in: of the
/// start states of its level cruise segments in flight order (`states`, the
/// initial level and each step level, from `planned_cruise_states`), the
/// last whose start mass is at or above `mass_kg`. A mass above the first
/// segment's start (a trip whose climb burns more than half its trip fuel)
/// is flown in the first segment. `None` for a plan with no cruise segment.
#[must_use]
pub fn state_flown_at_mass(
    states: &[super::mission_model::CruiseState],
    mass_kg: f64,
) -> Option<super::mission_model::CruiseState> {
    let first = states.first()?;
    let last_started = states
        .iter()
        .take_while(|state| state.mass_kg >= mass_kg)
        .last();
    Some(*last_started.unwrap_or(first))
}

/// The level of the cruise segment a mission flies `mass_kg` in, m
/// ([`state_flown_at_mass`]).
#[must_use]
pub fn level_flown_at_mass_m(
    states: &[super::mission_model::CruiseState],
    mass_kg: f64,
) -> Option<f64> {
    state_flown_at_mass(states, mass_kg).map(|state| state.altitude_m)
}

/// The flown state of a mid-cruise mass: the level, true airspeed and Mach
/// of the cruise segment its mission flies it in.
///
/// The speed is the segment's own, as the schedule commands it (a
/// turboprop's equivalent airspeed, a jet's Mach below its reference level,
/// the VMO/MMO cap) rather than the requirement's Mach at that level, so the
/// dynamic pressure, lift coefficient and drag a report evaluates there are
/// those of the flight (ATR72-600 at FL150: the requirement's Mach read at
/// that level understated its lift coefficient by about 9 %).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlownCruisePoint {
    /// Pressure altitude, m.
    pub altitude_m: f64,
    /// True airspeed, m/s.
    pub tas_m_s: f64,
    /// `tas_m_s` over the speed of sound of the mission's day at the level
    /// (US Standard Atmosphere 1976, `SegmentMissionModel::isa_deviation_c`).
    pub mach: f64,
}

impl FlownCruisePoint {
    /// The flown point of `state` on `model`'s day.
    ///
    /// # Errors
    ///
    /// A state whose level has no atmosphere or whose speed or Mach is not
    /// positive and finite, as a description.
    pub fn of_state(
        model: &super::SegmentMissionModel,
        state: super::mission_model::CruiseState,
    ) -> Result<Self, String> {
        let sound_m_s =
            alas_atmo::us1976_try_compute_values(state.altitude_m, model.isa_deviation_c)
                .map_err(|error| {
                    format!(
                        "no atmosphere at the {:.0} m cruise level: {error}",
                        state.altitude_m
                    )
                })?
                .speed_of_sound_m_s;
        let mach = state.tas_m_s / sound_m_s;
        if !(mach.is_finite() && mach > 0.0 && state.altitude_m.is_finite()) {
            return Err(format!(
                "the cruise segment at {:.0} m flies {} m/s, no usable Mach",
                state.altitude_m, state.tas_m_s
            ));
        }
        Ok(Self {
            altitude_m: state.altitude_m,
            tas_m_s: state.tas_m_s,
            mach,
        })
    }
}

/// The flown point of `sized`'s mid-cruise mass on `model`'s frozen plan,
/// flown from `sized`'s takeoff mass ([`state_flown_at_mass`]).
fn plan_mid_cruise_point(
    model: &super::SegmentMissionModel,
    sized: &super::types::SizedCandidate,
) -> Result<FlownCruisePoint, String> {
    let plan = model
        .frozen_plan()
        .ok_or_else(|| "the mission carries no frozen plan".to_owned())?;
    if !plan.holds_a_cruise_level() {
        return Err(format!(
            "the design mission's plan holds no cruise level (it sits on the {:.0} m floor of \
             the climb geometry)",
            plan.cruise_floor_m
        ));
    }
    let states = model.planned_cruise_states(sized.takeoff_mass_kg)?;
    let state = state_flown_at_mass(&states, sized.mid_cruise_mass_kg())
        .ok_or_else(|| "the design mission's plan flies no cruise segment".to_owned())?;
    FlownCruisePoint::of_state(model, state)
}

/// The flown point of a mission-closed candidate's mid-cruise mass: its
/// closure's own frozen plan ([`super::candidate_mission_model`]) flown from
/// the candidate's takeoff mass. Read by both the reported cruise point
/// ([`mid_cruise_point`]) and the in-loop wave-drag ceiling row
/// ([`closure_mid_cruise_level_m`]), so the two describe one state.
///
/// # Errors
///
/// A candidate analysed at a takeoff loading no closure flew (its mission is
/// not the closure's), no frozen plan, a plan on the takeoff-band floor, or
/// a model or trip that cannot be built or flown, as a description.
pub fn closure_mid_cruise_point(
    config: &alas_config::AlasConfig,
    sized: &super::types::SizedCandidate,
) -> Result<FlownCruisePoint, String> {
    if sized.takeoff_loading.is_some() {
        return Err("the candidate's loading is not the mission its closure flew".to_owned());
    }
    let model = super::candidate_mission_model(config, &sized.fuel_artifacts)?;
    plan_mid_cruise_point(&model, sized)
}

/// The level of [`closure_mid_cruise_point`], m.
///
/// # Errors
///
/// As [`closure_mid_cruise_point`].
pub fn closure_mid_cruise_level_m(
    config: &alas_config::AlasConfig,
    sized: &super::types::SizedCandidate,
) -> Result<f64, String> {
    closure_mid_cruise_point(config, sized).map(|point| point.altitude_m)
}

/// The flown state of the design mission of `sized` at its mid-cruise mass
/// ([`super::types::SizedCandidate::mid_cruise_mass_kg`]): the level, speed
/// and Mach of the mission plan's cruise segment whose start mass is at or
/// above that mass ([`state_flown_at_mass`]), so the state and the mass name
/// one flight state.
///
/// The mission plans its levels under the configured policy: under
/// `OptimumStep` the legal flight level at or below the configured level and
/// the capability at its top-of-climb mass (300 ft/min residual climb, 1.3 g
/// to buffet onset), then the step climbs it qualifies for as fuel burns off
/// (`mission_model::cruise_levels`). The configured cruise altitude is the
/// ceiling of that ladder, not the level the mid-cruise mass is flown at.
///
/// The mission is the one the cruise point's mass belongs to:
/// - a mission-closed candidate (no `takeoff_loading`) flies its closure's
///   own dispatch: [`closure_mid_cruise_point`];
/// - a candidate analysed at a takeoff loading no closure flew (Hard MTOW, a
///   baseline at its certified MTOW) flies that loading's own mission: the
///   longest still-air range its fuel covers with the design mission's
///   reserves ([`alas_mass::payload_range::max_range_with_reserves`], each
///   probed trip on a plan of its own, [`super::PlannedTrips`]), on a plan
///   frozen for that mass and range. The closure's plan belongs to another
///   mass (the route's) and is not read. The search needs the design
///   mission's model and reserve rule, not its range: a payload-defined
///   design mission (the C295's, whose range is the model's output at the
///   certified MTOW) prices it as well as a declared one.
///
/// # Errors
///
/// A plan that holds no cruise level (the trip sits on the takeoff-band
/// floor), a design mission that cannot be modelled, or a loading whose
/// mission cannot be priced or flown, as a description.
pub fn mid_cruise_point(
    config: &alas_config::AlasConfig,
    sized: &super::types::SizedCandidate,
) -> Result<FlownCruisePoint, String> {
    if sized.takeoff_loading.is_none() {
        return closure_mid_cruise_point(config, sized);
    }
    let (model, _, policy) =
        super::sizing::candidate_model::design_mission_flight(config, &sized.fuel_artifacts)?;
    let corner = alas_mass::payload_range::max_range_with_reserves(
        sized.takeoff_mass_kg,
        sized.takeoff_fuel_kg,
        &policy,
        &super::PlannedTrips(&model),
        LOADING_RANGE_TOLERANCE_M,
    );
    match corner.status {
        alas_mass::payload_range::RangeStatus::Solved => {}
        alas_mass::payload_range::RangeStatus::ZeroRange => {
            return Err(format!(
                "the loading of {:.0} kg with {:.0} kg of fuel covers no trip",
                sized.takeoff_mass_kg, sized.takeoff_fuel_kg
            ));
        }
        alas_mass::payload_range::RangeStatus::ModelFailed(reason) => return Err(reason),
    }
    let planned = super::plan_for_mission(&model, sized.takeoff_mass_kg, corner.range_m)
        .map_err(|error| error.to_string())?;
    plan_mid_cruise_point(&planned, sized)
}

#[cfg(test)]
#[path = "cruise_mass_tests.rs"]
mod tests;
