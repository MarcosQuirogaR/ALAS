// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The configured ladder is the ladder that is flown, fitted under the level the aircraft holds.

// Tests assert on values they constructed, so a failed unwrap is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::airports::get as get_airport;
use alas_config::AlasConfig;

use super::sequence::{flown_phases, PhaseKind, Scope};
use super::{evaluate, planned_standard_mission};
use crate::full_analysis::FullAnalysis;

#[test]
fn a_ladder_above_the_ceiling_is_lowered_to_what_the_aircraft_holds_and_says_so() {
    // The default three-leg ladder tops out at the design altitude, which the default
    // aircraft cannot hold at the mass of a 5,000 km route.
    let config = AlasConfig::default();
    let origin = get_airport(&config.departure_airport).unwrap();
    let destination = get_airport(&config.arrival_airport).unwrap();
    let report = FullAnalysis::new(config.clone())
        .run(
            &alas_config::design_variables::DesignVector::default(),
            true,
        )
        .unwrap();
    let route_m = 5_000_000.0;
    let planned = planned_standard_mission(&config, origin, destination, route_m).unwrap();
    let steps = |phases: &[super::sequence::Phase]| {
        phases
            .iter()
            .filter(|phase| phase.kind == PhaseKind::StepClimb)
            .count()
    };
    // The default ladder's lower levels are its climb-schedule rungs, 10,000 ft and more under
    // the top: a 5,000 km sector cannot pay for steps that large (first-order altitude drift,
    // `alas_mission::minimum_cruise_time_for_step_s`), so the climb goes on to one level.
    let planned_steps = steps(&planned.phases);
    assert_eq!(planned_steps, 0, "no step climb on rung levels");
    let (result, load_case) = evaluate(&config, &report, origin, destination, route_m).unwrap();
    let result = result.expect("the native flight");
    assert_eq!(load_case.native_error, None);
    assert!(result.completed_summary().is_some());
    // The planned ladder is flown; the level is lowered, and the run says to what.
    let flown = flown_phases(&result, Scope::Trip);
    assert_eq!(steps(&flown), planned_steps);
    let note = load_case
        .native_supplement
        .as_ref()
        .and_then(|supplement| supplement.reach_note)
        .expect("the lowered level is reported");
    assert!(note.flown_m < note.configured_m);
}

/// A reserve leg whose path change the engine cannot fly at a meaningful rate flies it level
/// and says so, with its leg and why. The real diversion guidance of the default aircraft, from
/// its MTOW, over a 1,000 nmi alternate at 1.2 times its cruise level: the step climb's speed
/// changes are refused for thrust down to below 100 ft/min. The window is not closed there, so
/// the note gives the refused and next rates and never claims that no constant rate flies it.
#[test]
fn a_reserve_leg_path_change_flown_level_is_reported() {
    let mut config = AlasConfig::default();
    config.fuel_policy.alternate_distance_nmi = 1_000.0;
    let origin = get_airport(&config.departure_airport).unwrap();
    let destination = get_airport(&config.arrival_airport).unwrap();
    let report = FullAnalysis::new(config.clone())
        .run(
            &alas_config::design_variables::DesignVector::default(),
            true,
        )
        .unwrap();
    let mut request =
        alas_mission::build_mission_request(&config, origin, destination, 2_000_000.0);
    request.cruise_altitude_m *= 1.2;
    let program = super::reserve::build(&request, &config.fuel_policy, None)
        .unwrap()
        .unwrap();
    let mut analyses = super::build_analyses(&config, &report).unwrap();
    let performance = config.performance.clone();
    let finish = |schedule: &mut Vec<alas_mission::segments::SegmentSpec>| {
        super::schedule::configuration::apply_configuration_drag(schedule, &performance);
    };
    // The holds come after the diversion and are not what is tested: one fixed speed.
    let hold = |_: &alas_mission::segments::MissionAnalyses, _: f64, _: f64| Some(120.0);
    let context = super::reserve::FlyContext {
        finish: &finish,
        hold_speed_m_s: &hold,
        isa_deviation_k: 0.0,
    };
    let flown = super::reserve::fly(
        &program,
        &mut analyses,
        config.requirements.mtow_kg,
        &context,
    )
    .unwrap();
    assert!(!flown.flown_level.is_empty(), "a conversion on this path");
    for note in &flown.flown_level {
        assert!(note.starts_with("reserve diversion leg, "), "{note}");
        assert!(note.ends_with("; flown level"), "{note}");
        // A descent below flight idle is searched for a rate; a climb is flown at the rating and
        // ends at its ceiling.
        assert!(
            note.contains("the next rate to try")
                || note.contains("residual climb of the service ceiling"),
            "{note}"
        );
        assert!(!note.contains("no constant"), "{note}");
    }
}

/// A refused climb is worded by its own cause: a throttle limit is a refusal for thrust, an
/// unconverged solve that reached no limit is not.
#[test]
fn a_refused_climb_is_worded_by_its_cause() {
    let solution =
        |converged, throttle_limited, idle_floor_limited| alas_mission::SegmentSolution {
            converged,
            status: alas_math::hybrd::Status::MaxEvaluations,
            evaluations: 1,
            throttle_limited,
            peak_throttle: 1.04,
            minimum_throttle: 0.5,
            available_throttle_floor: 0.0,
            idle_floor_limited,
            sub_idle_points: 0,
        };
    let thrust = super::flight::climb_refusal(&solution(false, true, false));
    assert!(thrust.starts_with("climb refused for thrust"), "{thrust}");
    let unconverged = super::flight::climb_refusal(&solution(false, false, false));
    assert!(
        unconverged.starts_with("climb solve did not converge"),
        "{unconverged}"
    );
    assert!(!unconverged.contains("thrust"), "{unconverged}");
    let idle = super::flight::climb_refusal(&solution(true, false, true));
    assert!(idle.contains("flight idle"), "{idle}");
}
