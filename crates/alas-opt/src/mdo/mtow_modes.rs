// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What a takeoff-mass sizing plan (`alas_config::MtowPlan`) changes in one
//! candidate's closure: which mission the sizing loop flies, and which
//! design weights the closed ledger reports.
//!
//! The MTOW band mode closes the takeoff mass on the design mission (design
//! range at design payload, flown at the sizing cruise altitude), and the
//! payload-adjusted mode does so on the design range when one is set; every
//! other mode closes on the selected route. The design weights of the two
//! design modes are those of a structure designed at the closure; the three
//! original modes keep the rules they had.

use alas_config::{AlasConfig, DesignPayloadSource, MassSizingBasis, MtowPlan, StructuralBasis};
use alas_mass::dispatch::{DispatchSolution, DispatchStatus};
use alas_mass::fuel_plan::FuelBurnModel;
use alas_mass::payload_range::{max_range_with_reserves, RangeStatus};
use alas_units::NAUTICAL_MILE;

use super::mission_model::SegmentMissionModel;
use super::sizing::planned_mission::PlannedTrips;
use super::types::CandidateFailure;

/// Failure label of a payload-defined design mission whose range the model
/// cannot solve at the certified takeoff mass: the fuel left after the empty
/// mass and the design payload does not cover the reserves, or the burn model
/// failed.
pub(crate) const DESIGN_RANGE_UNSOLVED: &str = "design_range_unsolved";

/// Bisection tolerance of the payload-defined design range, m. Over it the
/// trip fuel moves by a fraction of a kilogram, below the sizing tolerance.
const DESIGN_RANGE_TOLERANCE_M: f64 = 50.0;

/// What the takeoff-mass sizing plan adds to a closed candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct MtowPlanOutcome {
    /// `declared_cap` or `closure_mass` (`alas_config::StructuralBasis`).
    pub structural_basis: &'static str,
    /// Payload the takeoff-mass closure flew, kg: the design payload of a
    /// design mission, otherwise the laid-out load case.
    pub design_payload_kg: f64,
    /// Derived design maximum zero-fuel mass, kg: operating empty mass plus
    /// [`Self::design_payload_kg`]. Derived, not a declared weight.
    pub derived_design_mzfw_kg: f64,
    /// The selected route flown off-design at the closed mass, when the
    /// takeoff mass was closed on a design mission and the route is known.
    pub offdesign: Option<super::offdesign::OffDesignFlight>,
}

/// The mission a design-mission plan closes the takeoff mass on.
pub(crate) struct ClosureMission {
    /// Mission model at the sizing cruise altitude.
    pub model: SegmentMissionModel,
    /// Still-air range, m. Zero when [`Self::range_at_mass_kg`] is set: that
    /// range is the closure's output, solved on every pass.
    pub range_m: f64,
    /// The takeoff mass, kg, at which the closure solves the range itself
    /// ([`range_at_certified_mass`]) for a payload-defined design mission
    /// (`DesignRange::AtCertifiedMtow`); `None` when the range is declared or
    /// the route's.
    pub range_at_mass_kg: Option<f64>,
    /// Payload flown instead of the laid-out load case, kg, when the plan
    /// declares one.
    pub payload_kg: Option<f64>,
    /// Whether the range is declared or the route distance is known.
    pub distance_known: bool,
}

/// The design mission of `plan`, or `None` when the takeoff mass is closed
/// on the route.
///
/// `route_model` is the candidate's route model; the design mission flies
/// the same aircraft, profile, aerodromes and ambient at the sizing cruise
/// altitude `requirements.cruise_altitude_m`. `route_distance_m` is the
/// great-circle distance between the selected aerodromes (zero when it is
/// not known), used when no design range is declared.
///
/// A charted design payload is flown as that mass; a configured payload
/// limited to the charted one as the laid-out load case, capped there.
///
/// A planning-seat design payload is taken as the laid-out payload
/// `laid_out_payload_kg` with its `seated` passengers replaced by the
/// planning seats at `requirements.passenger_mass_kg` each, so everything
/// else the layout loads (revenue freight, container tare) is carried by both
/// and equal seat counts give equal payloads. Any other design payload is
/// the laid-out load case itself.
///
/// # Errors
///
/// `trim_solve` when the design-mission model is not valid at the sizing
/// cruise point, the bucket the route model uses for the same condition.
pub(crate) fn closure_mission(
    plan: &MtowPlan,
    config: &AlasConfig,
    route_model: &SegmentMissionModel,
    route_distance_m: f64,
    (laid_out_payload_kg, seated): (f64, i64),
) -> Result<Option<ClosureMission>, CandidateFailure> {
    let Some(mission) = plan.design_mission else {
        return Ok(None);
    };
    let payload_kg = match mission.payload_source {
        DesignPayloadSource::PlanningSeats(seats) if mission.payload_kg.is_some() => Some(
            laid_out_payload_kg + (seats - seated) as f64 * config.requirements.passenger_mass_kg,
        ),
        DesignPayloadSource::ChartedPoint => mission.payload_kg,
        DesignPayloadSource::ConfiguredWithinChart => mission
            .payload_kg
            .map(|charted_kg| laid_out_payload_kg.min(charted_kg)),
        _ => None,
    };
    let declared_m = mission.range.declared_nmi().map(|nmi| nmi * NAUTICAL_MILE);
    let range_at_mass_kg = mission
        .range
        .is_model_defined()
        .then_some(plan.declared_mtow_kg);
    let invalid = |_| CandidateFailure::new("trim_solve");
    let mut model = route_model.clone();
    let altitude_m = config.requirements.cruise_altitude_m;
    model.cruise_altitude_m = altitude_m;
    model.design_cruise_altitude_m = altitude_m;
    // M a(T) from the US Standard Atmosphere 1976 on the flown day, as the
    // route model derives it.
    model.cruise_tas_m_s = model
        .cruise_true_airspeed_m_s(altitude_m)
        .map_err(invalid)?;
    // The profile's cruise speeds are true airspeeds declared at the level
    // the model references (`cruise_tas_at` reads them as a Mach there).
    // Where they are the requirement's Mach, the route model derived them at
    // the route's level (`route_mission_model`); moving the reference to the
    // design altitude derives them again there, by the same rule. Kept
    // literal, the A220-300's FL250 speed read at FL370 flew its design
    // mission at Mach 0.819 against the 0.78 requirement (A320-200, FL280:
    // 0.808), lowering every level the mission plans.
    if config.cruise_profile_follows_requirement() {
        let tas_m_s = config.requirements.cruise_mach
            * alas_atmo::us1976_compute_values(altitude_m, 0.0).speed_of_sound_m_s;
        let profile = &mut model.profile;
        profile.cruise_1_air_speed_m_s = tas_m_s;
        profile.cruise_2_air_speed_m_s = tas_m_s;
        profile.cruise_3_air_speed_m_s = tas_m_s;
    }
    model.validate().map_err(invalid)?;
    Ok(Some(ClosureMission {
        model,
        range_m: if range_at_mass_kg.is_some() {
            0.0
        } else {
            declared_m.unwrap_or(route_distance_m)
        },
        range_at_mass_kg,
        payload_kg,
        distance_known: declared_m.is_some()
            || range_at_mass_kg.is_some()
            || route_distance_m > 0.0,
    }))
}

/// The still-air range of a payload-defined design mission, m: the largest
/// range whose full reserve fuel plan (`policy`) fits the fuel the aircraft
/// carries at `takeoff_mass_kg`.
///
/// The fuel is `takeoff_mass_kg` less `zero_fuel_mass_kg` (the closure pass's
/// operating empty mass plus the design payload), capped by the usable tank
/// capacity less the taxi fuel, which is loaded but burned before brake
/// release (the cap the dispatch applies). Every probed trip is flown on the
/// plan frozen for its own mass and range ([`PlannedTrips`]).
///
/// # Errors
///
/// [`DESIGN_RANGE_UNSOLVED`] when no fuel is left for the mission, the taxi
/// fuel flow is unavailable, or the reserves alone exceed the fuel.
pub(crate) fn range_at_certified_mass(
    model: &SegmentMissionModel,
    policy: &alas_config::FuelPolicyConfig,
    takeoff_mass_kg: f64,
    zero_fuel_mass_kg: f64,
    usable_capacity_kg: Option<f64>,
) -> Result<f64, CandidateFailure> {
    let unsolved = |detail: String| CandidateFailure::with_detail(DESIGN_RANGE_UNSOLVED, detail);
    let taxi_kg = model
        .taxi_fuel_flow_kg_s()
        .map_err(|error| unsolved(format!("taxi fuel flow is unavailable: {error}")))?
        * policy.taxi_time_min
        * 60.0;
    let mut fuel_kg = takeoff_mass_kg - zero_fuel_mass_kg;
    if let Some(capacity_kg) = usable_capacity_kg {
        fuel_kg = fuel_kg.min(capacity_kg - taxi_kg);
    }
    if !fuel_kg.is_finite() || fuel_kg <= 0.0 {
        return Err(unsolved(format!(
            "no fuel is left for the mission: {takeoff_mass_kg:.1} kg takeoff mass against a {zero_fuel_mass_kg:.1} kg zero-fuel mass"
        )));
    }
    let corner = max_range_with_reserves(
        zero_fuel_mass_kg + fuel_kg,
        fuel_kg,
        policy,
        &PlannedTrips(model),
        DESIGN_RANGE_TOLERANCE_M,
    );
    match corner.status {
        RangeStatus::Solved if corner.range_m.is_finite() && corner.range_m > 0.0 => {
            Ok(corner.range_m)
        }
        RangeStatus::ModelFailed(reason) => Err(unsolved(reason)),
        _ => Err(unsolved(format!(
            "{fuel_kg:.1} kg of fuel does not cover the reserves and a flyable trip"
        ))),
    }
}

/// The landing mass a sizing closure has to be able to land at, kg, or
/// `None` when the dispatch produced no finite value.
///
/// For a settled dispatch it is the plan's own destination landing mass
/// (takeoff mass less trip fuel at the closure's last iterate), which is the
/// field the `landing_mass` residual compares and the one the report's
/// design-mission arrival check reads, so a limit that the floor sets is met
/// exactly instead of being exceeded by the dispatch's settling residual (3 g
/// on a 150-seat clean-sheet brief). A dispatch that did not
/// settle (MTOW- or tank-limited, not converged) has a plan at a clamped mass,
/// so the floor stays the mass the mission has to land at: the zero-fuel mass
/// plus every fuel the plan carries past the destination (contingency,
/// alternate, final reserve, additional, extra and the taxi-in budget). The
/// floor of `AlasConfig::design_landing_mass_with_reserve_floor`.
pub(crate) fn landing_floor_kg(dispatch: &DispatchSolution) -> Option<f64> {
    let plan = &dispatch.plan;
    let floor_kg = if matches!(dispatch.status, DispatchStatus::Converged) {
        dispatch.destination_landing_mass_kg
    } else {
        dispatch.zero_fuel_mass_kg
            + plan.contingency.kg
            + plan.alternate.kg
            + plan.final_reserve.kg
            + plan.additional.kg
            + plan.extra.kg
            + plan.taxi_in_fuel_kg()
    };
    (floor_kg.is_finite() && floor_kg > 0.0).then_some(floor_kg)
}

/// Sizing-basis label, `DG` and `WLDG`, kg, of a candidate whose analysis
/// takeoff mass is `analysis_takeoff_mass_kg`.
///
/// `declared_mtow_kg` is `requirements.mtow_kg` of the candidate and
/// `landing_floor_kg` the [`landing_floor_kg`] of its sizing closure, which
/// the two design modes raise `WLDG` to.
pub(crate) fn design_weights(
    config: &AlasConfig,
    plan: &MtowPlan,
    analysis_takeoff_mass_kg: f64,
    declared_mtow_kg: f64,
    landing_floor_kg: Option<f64>,
) -> (&'static str, f64, f64) {
    if plan.mode.sizes_at_declared_mass() {
        let basis = config.mass_sizing_basis();
        return match basis {
            MassSizingBasis::FixedAircraft {
                design_gross_mass_kg,
                design_landing_mass_kg,
            } => (basis.as_str(), design_gross_mass_kg, design_landing_mass_kg),
            MassSizingBasis::Coupled => (
                basis.as_str(),
                declared_mtow_kg,
                config.design_landing_mass_for(declared_mtow_kg),
            ),
        };
    }
    if plan.requires_mission_sized_evaluation() {
        // The two design modes design the structure at the closure (unless
        // an explicit design gross mass pins it), so the label says the
        // components followed the closed mass.
        let (design_gross_mass_kg, _) = config.sized_design_weights_kg(analysis_takeoff_mass_kg);
        let design_landing_mass_kg = match plan.structural_basis {
            StructuralBasis::ClosureMass => config
                .design_landing_mass_with_reserve_floor(analysis_takeoff_mass_kg, landing_floor_kg),
            StructuralBasis::DeclaredCap => {
                config.sized_design_weights_kg(analysis_takeoff_mass_kg).1
            }
        };
        let label = match plan.structural_basis {
            StructuralBasis::ClosureMass => MassSizingBasis::Coupled.as_str(),
            StructuralBasis::DeclaredCap => config.mass_sizing_basis().as_str(),
        };
        return (label, design_gross_mass_kg, design_landing_mass_kg);
    }
    let basis = config.mass_sizing_basis();
    match basis {
        MassSizingBasis::FixedAircraft {
            design_gross_mass_kg,
            design_landing_mass_kg,
        } => (basis.as_str(), design_gross_mass_kg, design_landing_mass_kg),
        MassSizingBasis::Coupled => {
            // The component ledger is closed on this candidate's own
            // dispatched mass, which is what "coupled" means. The landing
            // mass is one quantity for the three places that read it: the gear is designed at it
            // (`at_sized_closure_mass_with_landing_floor`), the dispatch
            // limits the landing mass to it (`mda::converge`) and the
            // `landing_mass` residual compares the mission with it. It is
            // `WLDG = max(mlw_fraction x closure, ZFW + reserves)`: the
            // fraction of this candidate's own closure raised to the mass the
            // mission itself has to land at. A bare fraction of the closure
            // is unsatisfiable on a short sector (measured on the shipped
            // clean-sheet path: 434 of 462 A320-200 candidates rejected,
            // 438 of 460 A220-300, 603 of 605 A340-300), which is why this
            // limit was once taken against the declared MTOW while the gear
            // was built at the closure fraction: a 20-28 % mismatch between
            // the gear's design landing mass and the limit it was checked
            // against. The floor removes the unsatisfiability without
            // separating the two bases.
            (
                basis.as_str(),
                analysis_takeoff_mass_kg,
                config.design_landing_mass_with_reserve_floor(
                    analysis_takeoff_mass_kg,
                    landing_floor_kg,
                ),
            )
        }
    }
}
