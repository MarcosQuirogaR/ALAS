// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The plan engine itself: Raymer's profiles, the field check, agreement with
//! the standard mission, the reports of a closure that does not settle, of a
//! stage that cannot fit and of a payload balance that runs out of range.

use alas_config::{
    EventAction, EventTime, MassCategory, MissionPlan, PointMassConfig, StageEvent, StageKind,
};
use alas_pipeline::FindingCode;

use super::support::*;

/// Raymer's four example sizing profiles (Fig. 3.2) fly end to end on an
/// airliner airframe. The two profiles with a weight drop carry a 1,500 kg
/// store hung on at the start of the take-off, which the drop releases.
#[test]
fn the_four_raymer_profiles_fly_their_stages() {
    for profile in alas_config::RaymerProfile::ALL {
        let mut config = config_for_preset("A320-200");
        let mut inputs = alas_config::TemplateInputs::from_policy(&config.fuel_policy, 30_000.0);
        inputs.return_km = 50.0;
        // A transport cannot sustain a fighter's load factor: the study input is its own.
        inputs.combat_load_factor = 1.3;
        let mut plan = MissionPlan::raymer(profile, &inputs);
        if matches!(
            profile,
            alas_config::RaymerProfile::LowLevelStrike | alas_config::RaymerProfile::AirSuperiority
        ) {
            plan.stages[1].events.push(StageEvent {
                at: EventTime::Start,
                action: EventAction::StoreAdd {
                    id: "store".to_owned(),
                    mass_kg: 1_500.0,
                    x_m: 15.0,
                    y_m: 0.0,
                    z_m: -1.0,
                    drag_area_m2: Some(0.2),
                },
            });
        }
        config.mission.plan = plan;
        let design_mach = config.requirements.cruise_mach;
        let result = run(config);
        println!("== {}", profile.title());
        print_tables(&result);
        assert_complete_and_closed(&result);
        assert_plausible(profile.title(), &result, design_mach, &[]);
        let history = history_of(&result);
        assert_eq!(
            history.stages.len(),
            result.config.mission.plan.stages.len()
        );
        assert!(history.stages.iter().all(|stage| stage.fuel_kg >= 0.0));
    }
}

/// A critical take-off at a named field is judged at the mass the stage starts
/// with, and the stage reports the distance it needs and the distance it has:
/// a runway that is too short is a finding, a long one is not.
#[test]
fn a_takeoff_stage_with_a_field_is_checked_against_its_distance_available() {
    let flown = |toda_m: f64| {
        let mut config = config_for_preset("A220-300");
        config.mission.plan = MissionPlan {
            stages: vec![
                stage(
                    "takeoff",
                    StageKind::Takeoff {
                        field: Some(alas_config::TakeoffField {
                            name: "Short strip".to_owned(),
                            elevation_ft: 20.0,
                            toda_m,
                            isa_deviation_k: 0.0,
                        }),
                    },
                ),
                climb("climb", 15_000.0),
                stage("cruise", cruise_km(200.0)),
                descent("descent", 3_000.0, false),
                landing(),
            ],
            ..MissionPlan::default()
        };
        run(config)
    };
    let short = flown(500.0);
    let long = flown(4_500.0);
    let violation = |result: &alas_pipeline::PipelineResult| {
        result.feasibility.findings.iter().any(|finding| {
            finding.code == FindingCode::FieldTakeoffDistanceViolation
                && finding.message.contains("Short strip")
        })
    };
    assert!(violation(&short), "500 m is too short for an A220-300");
    assert!(!violation(&long));
    for (result, toda_m) in [(&short, 500.0), (&long, 4_500.0)] {
        let check = history_of(result).stages[0].field.as_ref().unwrap();
        println!(
            "take-off at {:.0} kg: needs {:.0} m, {:.0} m available",
            check.mass_kg, check.required_m, check.available_m
        );
        assert_eq!(check.available_m, toda_m);
        assert_eq!(
            check.feasible,
            result.config.mission.plan.stages.len() == 5 && !violation(result)
        );
    }
    // The same aircraft at the same mass needs the same distance on both strips.
    let (a, b) = (
        history_of(&short).stages[0].field.as_ref().unwrap(),
        history_of(&long).stages[0].field.as_ref().unwrap(),
    );
    assert!((a.required_m - b.required_m).abs() < 1.0e-6 * a.required_m);
}

/// The plan engine flying the sequence of the standard mission (take-off, the
/// climb to the cruise altitude, a cruise that closes the route, the descent to
/// the profile's last rung, the landing) against the standard mission on the same
/// aircraft and route. The standard mission is set to its single-level form (one
/// climb to the cruise level and one cruise leg: the form the ATR 72-600 preset
/// flies, `fly_single_assigned_level`), the plan is given the level the standard
/// mission flies, and its fuel is stated so that it leaves the ground at the mass
/// the standard mission does: the two then have the same structure and the same
/// mass. The trip fuel, the block time and the distance differ only by what the
/// plan engine does differently: the centre-of-gravity trim drag and its own leg
/// building and guidance.
#[test]
fn the_plan_engine_flying_the_standard_sequence_matches_the_standard_mission() {
    let mut standard_config = config_for_preset("A320-200");
    standard_config
        .mission
        .profile
        .initial_climb_altitude_fraction = 0.999;
    standard_config.mission.profile.cruise_2_distance_fraction = 0.0;
    standard_config.mission.profile.cruise_3_distance_fraction = 0.0;
    let profile = standard_config.mission.profile.clone();
    let standard = run(standard_config.clone());
    let std_load_case = standard.mission_load_case.as_ref().unwrap();
    // The standard mission flies the level its priced route model picks (below the
    // design level on this short route); the plan flies the level it is told.
    let cruise_m = standard
        .mission_result
        .as_ref()
        .unwrap()
        .segments
        .iter()
        .find(|segment| segment.spec.tag == "cruise_step_1")
        .and_then(|segment| segment.conditions.altitude_m.first().copied())
        .unwrap();
    println!("standard cruise level {:.0} ft", cruise_m / 0.3048);
    let mut plan_config = standard_config;
    plan_config.mission.plan = MissionPlan::from_profile(&profile, cruise_m / 0.3048);
    // The taxi fuel the plan closes on, then the fuel that puts the aircraft at the
    // standard mission's mass at brake release.
    let taxi_kg = case_of(&run(plan_config.clone())).taxi_before_takeoff_kg;
    plan_config.mission.plan.takeoff_fuel_kg =
        Some(std_load_case.takeoff_mass_kg - std_load_case.zero_fuel_mass_kg + taxi_kg);
    let planned = run(plan_config);
    assert!(planned
        .mission_result
        .as_ref()
        .unwrap()
        .completed_summary()
        .is_some());
    assert!(
        (case_of(&planned).takeoff_mass_kg - std_load_case.takeoff_mass_kg).abs() < 1.0,
        "the same mass at brake release"
    );

    let range = |m: &alas_mission::MissionResult| {
        m.segments
            .last()
            .and_then(|s| s.conditions.aircraft_range_m.last().copied())
            .unwrap()
    };
    let (std_mission, plan_mission) = (
        standard.mission_result.as_ref().unwrap(),
        planned.mission_result.as_ref().unwrap(),
    );
    let (std_fuel, plan_fuel) = (std_mission.fuel_burned_kg(), plan_mission.fuel_burned_kg());
    let (std_time, plan_time) = (std_mission.block_time_s(), plan_mission.block_time_s());
    let (std_range, plan_range) = (range(std_mission), range(plan_mission));
    println!(
        "standard: fuel {std_fuel:.0} kg, time {std_time:.0} s, range {:.1} km; plan: fuel {plan_fuel:.0} kg, time {plan_time:.0} s, range {:.1} km",
        std_range / 1000.0,
        plan_range / 1000.0
    );
    println!(
        "plan / standard: fuel {:.4}, time {:.4}, range {:.5}",
        plan_fuel / std_fuel,
        plan_time / std_time,
        plan_range / std_range
    );
    // The route is the same route: closed to a metre in a thousand.
    assert!((plan_range - std_range).abs() < 0.001 * std_range);
    // Within half a percent (the two agree to 0.1 %): what is left is the trim drag of
    // the centre of gravity (a fraction of a drag count of about 270) and the legs
    // the plan builds itself.
    assert!(
        (plan_fuel / std_fuel - 1.0).abs() < 0.005,
        "{plan_fuel} against {std_fuel}"
    );
    assert!(
        (plan_time / std_time - 1.0).abs() < 0.005,
        "{plan_time} against {std_time}"
    );
}

/// A trip loiter below the cruise (Raymer p. 32: 20-30 min at 10,000 ft before
/// the attempt to land) inserts the descent from the cruise level, and the
/// deceleration to the holding speed, into its own stage. Those legs fly along
/// the route and the route cruise is closed around them; the hold itself
/// returns to its fix. Before LOITER-DIST the cruise also took their distance
/// and the A320 overshot its 546 km sector by 150 km (07b: 137 km on its own
/// sector); counting only the descent left the 21 km level deceleration.
#[test]
fn a_trip_loiter_below_the_cruise_keeps_the_flown_route_at_the_sector() {
    const HOLD_SPEED_M_S: f64 = 110.0;
    let mut config = config_for_preset("A320-200");
    config.mission.plan = MissionPlan {
        stages: vec![
            takeoff(),
            climb("climb", 30_000.0),
            stage(
                "cruise",
                StageKind::Cruise {
                    extent: alas_config::CruiseExtent::Remainder,
                    altitude_ft: None,
                    agl: false,
                    speed_m_s: None,
                },
            ),
            stage(
                "loiter",
                StageKind::Loiter {
                    time_min: 30.0,
                    altitude_ft: Some(10_000.0),
                    agl: false,
                    speed_m_s: Some(HOLD_SPEED_M_S),
                },
            ),
            landing(),
        ],
        ..MissionPlan::default()
    };
    let origin = alas_config::airports::get(&config.departure_airport).unwrap();
    let destination = alas_config::airports::get(&config.arrival_airport).unwrap();
    let sector_m = alas_route::route::Route::great_circle(
        origin,
        destination,
        config.mission.great_circle_points as usize,
    )
    .total_distance_m();
    let result = run(config);
    print_tables(&result);
    assert_complete_and_closed(&result);
    let mission = result.mission_result.as_ref().unwrap();
    let range = |segment: &alas_mission::Segment| {
        let c = &segment.conditions;
        c.aircraft_range_m.last().unwrap() - c.aircraft_range_m.first().unwrap()
    };
    let hold = mission
        .segments
        .iter()
        .find(|segment| segment.spec.tag == "loiter@03")
        .expect("the hold");
    // The stage of a leg is the number after its `@`, as the plan reads it
    // (`stage_of_tag`): a leg the schedule cuts at a junction keeps its
    // stage and carries a suffix (`descent_1@03_path_entry`).
    let stage_of = |tag: &str| -> Option<usize> {
        let (_, tail) = tag.rsplit_once('@')?;
        let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    };
    let ramp_m: f64 = mission
        .segments
        .iter()
        .filter(|segment| stage_of(&segment.spec.tag) == Some(3) && segment.spec.tag != "loiter@03")
        .map(range)
        .sum();
    let flown_m: f64 = mission.segments.iter().map(range).sum::<f64>() - range(hold);
    println!(
        "sector {:.1} km, route flown {:.1} km, descent into the loiter {:.1} km, hold {:.1} km",
        sector_m / 1000.0,
        flown_m / 1000.0,
        ramp_m / 1000.0,
        range(hold) / 1000.0
    );
    assert!(ramp_m > 50.0e3, "the loiter stage descends from the cruise");
    assert!((range(hold) - HOLD_SPEED_M_S * 1800.0).abs() < 1.0);
    // Closed to a metre in a thousand, as the standard sequence is.
    assert!(
        (flown_m - sector_m).abs() < 0.001 * sector_m,
        "the route flown is {flown_m} m for a sector of {sector_m} m"
    );
}

/// A plan the closure cannot settle (a cruise no mass of fuel can fly) reports
/// it, with the numbers of the flight that was flown: not an un-flown guess.
#[test]
fn a_fuel_closure_that_does_not_settle_is_a_finding_with_consistent_numbers() {
    let mut config = config_for_preset("A220-300");
    config.mission.plan = MissionPlan {
        stages: vec![
            takeoff(),
            climb("climb", 30_000.0),
            stage("cruise", cruise_km(100_000.0)),
            descent("descent", 3_000.0, false),
            landing(),
        ],
        ..MissionPlan::default()
    };
    let result = run(config);
    let case = case_of(&result);
    print_findings(&result);
    assert_eq!(case.closure, alas_pipeline::PlanClosure::NotClosed);
    assert!(has_finding(&result, FindingCode::DispatchNotConverged));
    assert!(!result.feasibility.is_feasible());
    // The reported fuel is the fuel of the reported flight.
    let load_case = result.mission_load_case.as_ref().unwrap();
    assert!(
        (load_case.takeoff_mass_kg
            - (load_case.zero_fuel_mass_kg + case.ramp_fuel_kg - case.taxi_before_takeoff_kg))
            .abs()
            < 1.0e-6
    );
    let mission = result.mission_result.as_ref().unwrap();
    assert!((mission.initial_mass_kg() - load_case.takeoff_mass_kg).abs() < 1.0);
}

/// A stage shorter than its own climb and descent is an error of the plan, not
/// a silent zero-length cruise.
#[test]
fn a_stage_shorter_than_its_own_climb_and_descent_is_an_error() {
    let mut config = config_for_preset("A220-300");
    let plan = MissionPlan {
        stages: vec![
            takeoff(),
            climb("climb", 30_000.0),
            stage("cruise", cruise_km(600.0)),
            descent("descent", 3_000.0, false),
            stage(
                "divert",
                StageKind::Diversion {
                    distance_km: 5.0,
                    cruise_altitude_ft: 25_000.0,
                    field_elevation_ft: None,
                },
            )
            .as_reserve(),
            landing(),
        ],
        ..MissionPlan::default()
    };
    config.mission.plan = plan;
    let refusal = try_run(config).expect_err("the plan is refused");
    println!("refused: {refusal}");
    assert!(
        refusal.contains("'divert'") && refusal.contains("less than"),
        "{refusal}"
    );
}

/// A movable payload item that cannot reach the balance target within its range
/// is reported, not only logged.
#[test]
fn a_payload_balance_that_runs_out_of_range_is_a_finding() {
    let mut config = config_for_preset("A220-300");
    config.cabin.payload_items.point_masses = vec![PointMassConfig {
        id: "crate".to_owned(),
        name: "Crate".to_owned(),
        x_m: 20.0,
        y_m: 0.0,
        z_m: -0.5,
        mass_kg: 800.0,
        category: MassCategory::Payload,
        inertia_kgm2: None,
        releasable: false,
        drag_area_m2: None,
        movable: true,
        x_range_m: Some([19.9, 20.1]),
    }];
    // The balance aims the zero-fuel centre of gravity at a target no 800 kg
    // crate can reach within 0.2 m.
    config.mission.plan.balance_target_pct_mac = Some(5.0);
    let result = run(config);
    print_findings(&result);
    assert!(has_finding(
        &result,
        FindingCode::PayloadAutoBalanceIncomplete
    ));
}

/// The commercial transport template flies on the preset's own route with the
/// fuel policy's alternate and the preset's cruise level. The trip is the route
/// down to the decision height of the attempt to land: the ground distance from
/// take-off to the end of `attempt-to-land` is the great circle, and the
/// missed approach, the diversion and its landing are reserve stages flown
/// beyond it. (An earlier template ended the trip at the loiter level and
/// billed the whole approach as reserve, so the flight overshot the destination.)
#[test]
fn the_commercial_template_closes_the_route_to_the_decision_height_on_the_preset_route() {
    let mut config = config_for_preset("A320-200");
    let origin = alas_config::airports::get(&config.departure_airport).unwrap();
    let destination = alas_config::airports::get(&config.arrival_airport).unwrap();
    let cruise_ft = alas_mission::route_cruise_altitude_m(&config, origin, destination) / 0.3048;
    let inputs = alas_config::TemplateInputs::from_policy(&config.fuel_policy, cruise_ft)
        .with_profile(&config.mission.profile);
    let plan = MissionPlan::raymer(alas_config::RaymerProfile::CommercialTransport, &inputs);
    config.mission.plan = plan;
    let design_mach = config.requirements.cruise_mach;
    let route = alas_route::route::Route::great_circle(
        origin,
        destination,
        config.mission.great_circle_points as usize,
    );
    let route_m = route.total_distance_m();
    let result = run(config);
    print_tables(&result);
    assert_complete_and_closed(&result);
    assert_plausible("Commercial Transport", &result, design_mach, &[]);
    let history = history_of(&result);
    let plan = &result.config.mission.plan;
    let attempt = plan.position("attempt-to-land").unwrap();
    let to_decision_height_m: f64 = history.stages[..=attempt]
        .iter()
        .map(|stage| stage.distance_m)
        .sum();
    let hold_m = history.stages[plan.position("loiter").unwrap()].distance_m;
    assert!(
        (to_decision_height_m - hold_m - route_m).abs() < 0.02 * route_m,
        "trip {to_decision_height_m:.0} m less the hold {hold_m:.0} m, route {route_m:.0} m"
    );
    for stage in &history.stages[attempt + 1..] {
        assert!(plan.stages[plan.position(&stage.id).unwrap()].reserve);
    }
}

/// The field check of a take-off stage reads the same speed basis as the
/// feasibility gate and the ISA sea-level reference: the candidate's CLmax, the
/// minimum control speed its fin and rudder give, and for a jet the improved-climb
/// V2. At sea level, ISA, with the mass of the stage, the stage's required distance
/// is the reference's take-off field length to the last digit. (Before FIELD-OEI the
/// stage was judged with the declared VMC ratio alone, which raised the ATR 72-600's
/// V2 to 1.10 of a control speed it does not have and lengthened its distance by
/// about 7 %.)
#[test]
fn the_stage_field_check_reads_the_speed_basis_of_the_reference() {
    for name in ["ATR72-600", "A220-300"] {
        let mut config = config_for_preset(name);
        config.mission.plan = MissionPlan {
            stages: vec![
                stage(
                    "takeoff",
                    StageKind::Takeoff {
                        field: Some(alas_config::TakeoffField {
                            name: "Sea level".to_owned(),
                            elevation_ft: 0.0,
                            toda_m: 4_500.0,
                            isa_deviation_k: 0.0,
                        }),
                    },
                ),
                climb("climb", 12_000.0),
                stage("cruise", cruise_km(150.0)),
                descent("descent", 3_000.0, false),
                landing(),
            ],
            ..MissionPlan::default()
        };
        let result = run(config);
        let check = history_of(&result).stages[0].field.as_ref().unwrap();
        let report = result.optimized_report.as_ref().unwrap();
        let reference = alas_pipeline::field_reference::report_isa_sea_level_field_reference(
            &result.config,
            report,
            report.airplane.s_ref,
            check.mass_kg,
            0.95 * check.mass_kg,
        )
        .unwrap();
        println!(
            "{name}: stage {:.1} m, reference {:.1} m at {:.0} kg",
            check.required_m, reference.takeoff_field_length_m, check.mass_kg
        );
        assert!(
            (check.required_m - reference.takeoff_field_length_m).abs()
                < 1.0e-9 * reference.takeoff_field_length_m,
            "{name}"
        );
        assert!((check.v2_m_s - reference.v2_m_s).abs() < 1.0e-9, "{name}");
        // The published V2 minimum of the preset (ATR factsheet 2020 p.2: 116 KCAS at
        // 23,000 kg, ISA sea level) is flown by the reference and by the stage, whose
        // V2 goes with the square root of the mass at the same wing, density and
        // lift coefficient.
        if let Some(published) = alas_config::presets::published_takeoff_reference(name) {
            let at_published_mass = check.v2_m_s * (published.mass_kg / check.mass_kg).sqrt();
            println!(
                "{name}: V2 {:.2} m/s at {:.0} kg, {at_published_mass:.2} m/s at {:.0} kg; published {:.2} m/s",
                check.v2_m_s, check.mass_kg, published.mass_kg, published.v2_min_m_s
            );
            assert!(
                at_published_mass >= published.v2_min_m_s - published.speed_uncertainty_m_s
                    && at_published_mass
                        <= published.v2_min_m_s + 2.0 * published.speed_uncertainty_m_s,
                "{name}"
            );
        }
    }
}

/// The minimum control speed in the speed rule belongs to the field the speeds
/// are flown at, not to the configured departure: a hot, high field resolves
/// another ratio than the departure does.
#[test]
fn the_control_speed_of_a_stage_is_evaluated_at_its_own_field() {
    let config = config_for_preset("ATR72-600");
    let mut flown = config.clone();
    flown.mission.plan = MissionPlan {
        stages: vec![
            stage("takeoff", StageKind::Takeoff { field: None }),
            climb("climb", 10_000.0),
            stage("cruise", cruise_km(100.0)),
            descent("descent", 3_000.0, false),
            landing(),
        ],
        ..MissionPlan::default()
    };
    let result = run(flown);
    let report = result.optimized_report.as_ref().unwrap();
    let mtow = config.requirements.mtow_kg;
    let hot_high = alas_config::airports::Airport::custom(
        "hot and high",
        2_500.0,
        4_000.0,
        4_000.0,
        25.0,
        0.0,
        0.0,
    );
    let ratio = |airport: Option<&alas_config::airports::Airport>| {
        alas_pipeline::report_config::resolve_report_at(
            &config,
            report,
            airport,
            mtow,
            0.95 * mtow,
            report.airplane.s_ref,
        )
        .vmc
        .rule_ratio
        .expect("a computed control speed")
    };
    let (departure, field) = (ratio(None), ratio(Some(&hot_high)));
    println!("VMC/VSR at the departure {departure:.4}, at the hot and high field {field:.4}");
    // Assert the physical direction, not only a change. The engine-out thrust
    // falls with density altitude and temperature, so the control speed ratio
    // falls at the hot and high field (0.838 at the departure, 0.753 at
    // 2,500 m ISA+25 when this was written).
    assert!(
        field < departure,
        "VMC/VSR must be lower at the hot and high field: {field:.4} against {departure:.4}"
    );
    assert!(
        (departure - field).abs() > 1.0e-4,
        "the field must move the control speed"
    );
}
