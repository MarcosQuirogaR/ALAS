// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the native flight adds to the trip: the ground phases and the reserve program as flown.

use alas_config::AlasConfig;
use alas_mass::fuel_plan::TAXI_IN_TIME_MIN;
use alas_mission::segments::SegmentSpec;
use alas_mission::MissionRequest;

use super::{NativeSupplement, ReachCause, ReachNote};
use crate::full_analysis::AnalysisReport;
use crate::mission_stage::reserve::{self, ReserveProgram};
use crate::mission_stage::schedule::configuration::apply_configuration_drag;
use crate::mission_stage::schedule::Reach;
use crate::mission_stage::sequence::{Phase, PhaseKind};
use crate::mission_stage::{cruise_level, plan};

/// The ground phases and the reserve program flown from the landing mass.
#[allow(clippy::too_many_arguments)]
pub(super) fn supplement(
    config: &AlasConfig,
    report: &AnalysisReport,
    analyses: &mut alas_mission::segments::MissionAnalyses,
    priced: &cruise_level::PricedCruise,
    taxi_flow_kg_s: f64,
    reach: Option<&Reach>,
    reserve: Result<Option<ReserveProgram>, String>,
    landing_mass_kg: f64,
    request: &MissionRequest,
    trip_phases: Vec<Phase>,
) -> NativeSupplement {
    let flow_kg_s = taxi_flow_kg_s;
    let ground = |kind, elevation_m, duration_s| Phase {
        kind,
        start_altitude_m: elevation_m,
        end_altitude_m: elevation_m,
        distance_m: 0.0,
        duration_s,
        fuel_kg: Some(flow_kg_s * duration_s),
    };
    let performance = &config.performance;
    let area_m2 = analyses.reference_area_m2;
    let finish = |schedule: &mut Vec<SegmentSpec>| {
        apply_configuration_drag(schedule, performance);
        plan::add_carried_drag(schedule, report, area_m2);
    };
    let turboprop = matches!(
        config.geometry.engine.active_model(),
        Ok(alas_config::ActiveEngineModel::Turboprop(_))
    );
    let isa_deviation_k = request.departure_isa_deviation_c;
    let hold_speed =
        |analyses: &alas_mission::segments::MissionAnalyses, altitude_m: f64, mass_kg: f64| {
            plan::loiter::endurance_speed_m_s(
                analyses,
                altitude_m,
                mass_kg,
                isa_deviation_k,
                turboprop,
                performance.cl_max_clean,
            )
        };
    let mut reserves = None;
    let mut reserve_error = None;
    let context = reserve::FlyContext {
        finish: &finish,
        hold_speed_m_s: &hold_speed,
        isa_deviation_k,
    };
    match reserve {
        Ok(Some(program)) => {
            // The diversion is flown at the profile's top level. An aircraft that cannot hold
            // the speed changes at that level at the mass it lands with flies it lower: the
            // levels are retried at a share of the cruise altitude, and the level flown is
            // reported (`FlownReserves::level_scale`), as the priced model lowers a level it
            // cannot hold.
            let mut program = Some(program);
            let mut scales = DIVERSION_LEVEL_SCALES.iter();
            let mut scale = 1.0;
            while let Some(attempt) = program.take() {
                match reserve::fly(&attempt, analyses, landing_mass_kg, &context) {
                    Ok(mut flown) => {
                        flown.level_scale = scale;
                        reserves = Some(flown);
                    }
                    Err(error) => {
                        reserve_error = Some(error);
                        if let Some(next) = scales.next() {
                            scale = *next;
                            let lowered = MissionRequest {
                                cruise_altitude_m: request.cruise_altitude_m * scale,
                                ..request.clone()
                            };
                            program =
                                reserve::build(&lowered, &config.fuel_policy, priced.rule.clone())
                                    .ok()
                                    .flatten();
                        }
                    }
                }
            }
            if reserves.is_some() {
                reserve_error = None;
            }
        }
        Ok(None) => {}
        Err(error) => reserve_error = Some(error),
    }
    // The level the flight cruised at, against the configured one: lowered because the aircraft
    // cannot hold it at this mass (the ladder was refitted) or because the guidance fitted the
    // altitudes under the footprint of the climbs and descents the engine allows. Either way it is
    // said, never left to differ silently from the configured top level.
    let flown_top_m = trip_phases
        .iter()
        .filter(|phase| phase.kind == PhaseKind::Cruise)
        .map(|phase| phase.end_altitude_m)
        .fold(f64::NEG_INFINITY, f64::max);
    // Only a drop of a flight level or more is a lowering; a smaller one is the guidance fit and
    // both levels read alike.
    let reach_note = (flown_top_m.is_finite()
        && request.cruise_altitude_m - flown_top_m >= REPORTED_LOWERING_M)
        .then_some(ReachNote {
            configured_m: request.cruise_altitude_m,
            flown_m: flown_top_m,
            cause: if reach.is_some_and(|reach| reach.top_m < request.cruise_altitude_m - 1.0) {
                ReachCause::Mass
            } else {
                ReachCause::RouteFit
            },
        });
    NativeSupplement {
        taxi_out: ground(
            PhaseKind::TaxiOut,
            request.departure_elevation_m,
            config.fuel_policy.taxi_time_min * 60.0,
        ),
        taxi_in: ground(
            PhaseKind::TaxiIn,
            request.arrival_elevation_m,
            TAXI_IN_TIME_MIN * 60.0,
        ),
        reach_note,
        trip_phases,
        reserves,
        reserve_error,
        sizing: None,
    }
}

/// Smallest drop of the flown top level that is reported: one flight level, 100 ft.
const REPORTED_LOWERING_M: f64 = 100.0 * 0.3048;

/// Shares of the cruise altitude a diversion that cannot be flown at the profile's top level
/// is retried at, highest first.
const DIVERSION_LEVEL_SCALES: [f64; 3] = [0.8, 0.6, 0.4];
