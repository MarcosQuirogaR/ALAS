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

use alas_atmo::Atmosphere;
use alas_config::{
    AlasConfig, DesignPayloadSource, MassSizingBasis, MtowPlan, MtowSizing, StructuralBasis,
};
use alas_mass::dispatch::DispatchSolution;
use alas_units::NAUTICAL_MILE;

use super::mission_model::SegmentMissionModel;
use super::types::CandidateFailure;

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
    /// Still-air range, m.
    pub range_m: f64,
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
/// A charted design payload is flown as that mass.
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
        _ => None,
    };
    let declared_m = mission.range.declared_nmi().map(|nmi| nmi * NAUTICAL_MILE);
    let invalid = |_| CandidateFailure {
        reason: "trim_solve",
    };
    let mut model = route_model.clone();
    let altitude_m = config.requirements.cruise_altitude_m;
    model.cruise_altitude_m = altitude_m;
    model.design_cruise_altitude_m = altitude_m;
    model.cruise_tas_m_s = model.cruise_mach
        * Atmosphere::try_new(altitude_m)
            .map_err(|error| invalid(error.to_string()))?
            .speed_of_sound();
    model.validate().map_err(invalid)?;
    Ok(Some(ClosureMission {
        model,
        range_m: declared_m.unwrap_or(route_distance_m),
        payload_kg,
        distance_known: declared_m.is_some() || route_distance_m > 0.0,
    }))
}

/// The landing mass a sizing closure has to be able to land at, kg: its
/// zero-fuel mass plus every fuel the plan carries past the destination
/// (contingency, alternate, final reserve, additional, extra and the
/// taxi-in budget), or `None` when the dispatch produced no finite value.
///
/// This is the plan's own destination landing mass, so the `landing_mass`
/// residual compares the mission with a limit built on the same fuel basis
/// and reserve policy. The floor of
/// `AlasConfig::design_landing_mass_with_reserve_floor`.
pub(crate) fn landing_floor_kg(dispatch: &DispatchSolution) -> Option<f64> {
    let plan = &dispatch.plan;
    let floor_kg = dispatch.zero_fuel_mass_kg
        + plan.contingency.kg
        + plan.alternate.kg
        + plan.final_reserve.kg
        + plan.additional.kg
        + plan.extra.kg
        + plan.taxi_in_fuel_kg();
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
    if plan.mode == MtowSizing::FixedRequirement {
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
            // dispatched mass, which is what "coupled" means and is left
            // alone. The *landing* limit is a different quantity and must
            // not follow it.
            //
            // A maximum landing mass is a structural design weight: a
            // fraction of the design gross weight the airframe and gear are
            // built for. Referring it to the mass this particular sector
            // happens to close at makes the `landing_mass` residual say
            // "burn at least (1 - mlw_fraction) of your own take-off mass on
            // this flight", which is a statement about the mission with no
            // aircraft property in it, and it is unsatisfiable by
            // construction on a short sector: measured on the shipped
            // clean-sheet path it rejected 434 of 462 A320-200 candidates,
            // 438 of 460 A220-300 and 603 of 605 A340-300.
            //
            // Under `MtowSizing::SizedByMission` the closure is bounded above
            // by the declared MTOW, so that declared mass *is* the design
            // gross weight the structure must support and the closure is
            // only this mission's dispatch; the limit is taken against it.
            // `Unconstrained` declares no ceiling, so it keeps the closed
            // mass, which is the only design weight that mode has.
            let limit_basis_kg = if plan.mode == MtowSizing::SizedByMission
                && declared_mtow_kg.is_finite()
                && declared_mtow_kg > 0.0
            {
                declared_mtow_kg
            } else {
                analysis_takeoff_mass_kg
            };
            (
                basis.as_str(),
                analysis_takeoff_mass_kg,
                config.landing_mass_limit_kg(limit_basis_kg),
            )
        }
    }
}
