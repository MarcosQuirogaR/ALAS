// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The route's trip against the fuel its load case carries.
//!
//! The route is judged on the unified segment mission model, on the plan
//! frozen for it by the dispatch (`crate::mission_stage::dispatch`). The
//! native pseudospectral mission flown beside it is telemetry and never
//! decides the route.

use alas_mission::MissionResult;

use super::{DispatchAssessment, DispatchOutcome};

/// Outcome of flying a mission against the fuel its load case carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MissionFuelStatus {
    /// Mission analysis was disabled for this run.
    #[default]
    NotRequested,
    /// Mission analysis was enabled but produced no result.
    Unavailable,
    /// The mission was evaluated, but its solve did not converge.
    NotConverged,
    /// The trip was flown on the fuel carried.
    Completed,
    /// The trip needs more fuel than the load case carries.
    Exhausted,
}

/// The route's trip on the unified segment mission model, with the native
/// mission beside it as telemetry.
///
/// `status`, `burned_fuel_kg` and `required_trip_fuel_kg` describe the route
/// as the dispatch flew it on the plan frozen for it
/// (`crate::mission_stage::dispatch`): the model the sizing closure and the
/// reserve check use. The native pseudospectral mission is a different
/// aerodynamic and profile model, retired as a fuel source; it is reported
/// in [`NativeMissionTelemetry`] and never gates feasibility.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MissionFuelAssessment {
    /// Whether the route's trip was flown on the fuel carried.
    pub status: MissionFuelStatus,
    /// Route trip fuel on the unified model, in kilograms; for an exhausted
    /// route, the loadable fuel the trip would have consumed.
    pub burned_fuel_kg: Option<f64>,
    /// Fuel required for the route's trip, excluding reserves, in
    /// kilograms. Available only when the trip was flown on a converged plan.
    pub required_trip_fuel_kg: Option<f64>,
    /// The native pseudospectral mission flown at the selected mass.
    pub native: NativeMissionTelemetry,
}

/// The native pseudospectral mission's outcome: telemetry, never a gate.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NativeMissionTelemetry {
    /// Completeness of the native trajectory.
    pub status: MissionFuelStatus,
    /// Fuel the native trajectory burned, in kilograms (to its stopping
    /// point when it did not complete).
    pub burned_fuel_kg: Option<f64>,
    /// Whether any native segment reached the full-throttle boundary before
    /// its force balance converged.
    pub throttle_limited: bool,
}

/// The route's trip on the unified model, and the native mission telemetry.
///
/// The route's status follows its dispatch: a priced, converged plan whose
/// trip fits in the loadable fuel (the plan's takeoff fuel less any
/// shortfall against the takeoff-mass or tank limit) is completed, even when
/// its reserves are short (that is [`FindingCode::ReserveFuelShortfall`]); a
/// trip above the loadable fuel is exhausted. A deliberate or failed
/// maximum-available-fuel case has no unified route result.
pub(crate) fn assess_mission_fuel(
    mission_requested: bool,
    dispatch: Option<&DispatchAssessment>,
    native: Option<&MissionResult>,
) -> MissionFuelAssessment {
    if !mission_requested {
        return MissionFuelAssessment::default();
    }
    let native = native_telemetry(native);
    let Some((dispatch, plan)) = dispatch.and_then(|d| d.plan.map(|plan| (d, plan))) else {
        return MissionFuelAssessment {
            status: MissionFuelStatus::Unavailable,
            native,
            ..MissionFuelAssessment::default()
        };
    };
    let trip_kg = plan.trip.kg;
    if dispatch.outcome == DispatchOutcome::NotConverged {
        return MissionFuelAssessment {
            status: MissionFuelStatus::NotConverged,
            burned_fuel_kg: Some(trip_kg),
            required_trip_fuel_kg: None,
            native,
        };
    }
    let loadable_kg = plan.takeoff_fuel_kg() - dispatch.shortfall_kg;
    if trip_kg > loadable_kg {
        return MissionFuelAssessment {
            status: MissionFuelStatus::Exhausted,
            burned_fuel_kg: Some(loadable_kg),
            required_trip_fuel_kg: None,
            native,
        };
    }
    MissionFuelAssessment {
        status: MissionFuelStatus::Completed,
        burned_fuel_kg: Some(trip_kg),
        required_trip_fuel_kg: Some(trip_kg),
        native,
    }
}

fn native_telemetry(native: Option<&MissionResult>) -> NativeMissionTelemetry {
    let Some(result) = native else {
        return NativeMissionTelemetry {
            status: MissionFuelStatus::Unavailable,
            ..NativeMissionTelemetry::default()
        };
    };
    let throttle_limited = result
        .solutions
        .iter()
        .any(|solution| solution.throttle_limited);
    if let Some(exhaustion) = &result.fuel_exhaustion {
        return NativeMissionTelemetry {
            status: MissionFuelStatus::Exhausted,
            burned_fuel_kg: Some(exhaustion.burned_fuel_kg),
            throttle_limited,
        };
    }
    match result.completed_summary() {
        Some(summary) => NativeMissionTelemetry {
            status: MissionFuelStatus::Completed,
            burned_fuel_kg: Some(summary.trip_fuel_kg),
            throttle_limited,
        },
        None => {
            let burned_fuel_kg = result.fuel_burned_kg();
            NativeMissionTelemetry {
                status: MissionFuelStatus::NotConverged,
                burned_fuel_kg: burned_fuel_kg.is_finite().then_some(burned_fuel_kg),
                throttle_limited,
            }
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// is the assertion failing.
#[cfg(test)]
mod tests {
    use super::*;

    /// A route plan with the given trip and reserve, priced at the given
    /// shortfall against the loadable fuel.
    fn route_dispatch(trip_kg: f64, reserve_kg: f64, shortfall_kg: f64) -> DispatchAssessment {
        use alas_mass::fuel_plan::{FuelPlan, FuelQuantity};
        let quantity = |kg: f64| FuelQuantity {
            kg,
            ..FuelQuantity::NONE
        };
        let plan = FuelPlan {
            scheme: alas_config::FuelScheme::TripFuelOnly,
            taxi: FuelQuantity::NONE,
            trip: quantity(trip_kg),
            contingency: FuelQuantity::NONE,
            alternate: FuelQuantity::NONE,
            final_reserve: quantity(reserve_kg),
            additional: FuelQuantity::NONE,
            extra: FuelQuantity::NONE,
            trip_time_s: 3_600.0,
            destination_landing_mass_kg: 0.0,
            reserve_landing_mass_kg: 0.0,
        };
        DispatchAssessment {
            outcome: if shortfall_kg > 0.0 {
                DispatchOutcome::MtowLimited
            } else {
                DispatchOutcome::Converged
            },
            plan: Some(plan),
            takeoff_mass_kg: 100_000.0,
            shortfall_kg,
            route_distance_m: Some(1.0e6),
            reserve_margin_kg: Some(-shortfall_kg),
            design_mission: None,
            native_check: None,
        }
    }

    /// The native trajectory is telemetry: an exhausted native flight leaves
    /// a route the unified model flies with its reserves completed, and its
    /// own status is still reported.
    #[test]
    fn an_exhausted_native_flight_does_not_decide_the_route() {
        let native = MissionResult {
            segments: Vec::new(),
            solutions: Vec::new(),
            scheduled_segment_count: 1,
            fuel_exhaustion: Some(alas_mission::FuelExhaustion {
                segment_index: 0,
                segment_tag: "descent".to_owned(),
                available_fuel_kg: 10_000.0,
                burned_fuel_kg: 10_000.0,
                minimum_mass_kg: 90_000.0,
            }),
        };
        let dispatch = route_dispatch(8_000.0, 2_000.0, 0.0);

        let assessment = assess_mission_fuel(true, Some(&dispatch), Some(&native));

        assert_eq!(assessment.status, MissionFuelStatus::Completed);
        assert_eq!(assessment.required_trip_fuel_kg, Some(8_000.0));
        assert_eq!(assessment.native.status, MissionFuelStatus::Exhausted);
        assert_eq!(assessment.native.burned_fuel_kg, Some(10_000.0));
    }

    /// Fuel conservation on the route: the trip is flown while it fits in the
    /// loadable fuel (reserves short is a separate finding) and exhausted once
    /// it exceeds it.
    #[test]
    fn the_route_is_exhausted_only_when_its_trip_exceeds_the_loadable_fuel() {
        // Loadable 10,000 - 1,500 = 8,500 kg >= 8,000 kg trip: reserves short.
        let reserves_short = route_dispatch(8_000.0, 2_000.0, 1_500.0);
        let flown = assess_mission_fuel(true, Some(&reserves_short), None);
        assert_eq!(flown.status, MissionFuelStatus::Completed);
        assert_eq!(flown.native.status, MissionFuelStatus::Unavailable);

        // Loadable 10,000 - 2,500 = 7,500 kg < 8,000 kg trip.
        let trip_short = route_dispatch(8_000.0, 2_000.0, 2_500.0);
        let exhausted = assess_mission_fuel(true, Some(&trip_short), None);
        assert_eq!(exhausted.status, MissionFuelStatus::Exhausted);
        assert_eq!(exhausted.burned_fuel_kg, Some(7_500.0));
        assert_eq!(exhausted.required_trip_fuel_kg, None);

        let unpriced = assess_mission_fuel(true, None, None);
        assert_eq!(unpriced.status, MissionFuelStatus::Unavailable);
    }
}
