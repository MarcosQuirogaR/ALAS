// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! C5: a wide-body tanker (A340-300, the registered four-engine wide-body with a
//! trim tank: the A330 MRTT class has no preset) offloading fuel on a track,
//! and the receiver variant.

use alas_config::{AlasConfig, EventAction, MissionPlan, StageKind};
use alas_pipeline::FindingCode;

use super::support::*;

const PRESET: &str = "A340-300";

/// Each offload is a contact of `contact_min` of loiter on the track; with none, a
/// plain orbit of that time (none when it is zero).
fn tanker_plan(config: &AlasConfig, offloads: &[f64], contact_min: f64) -> MissionPlan {
    let mut stages = vec![
        stage("taxi", StageKind::Taxi { time_min: 15.0 }),
        takeoff(),
        climb("climb", 30_000.0),
        stage("cruise-to-track", cruise_km(1_000.0)),
    ];
    for (index, kg) in offloads.iter().enumerate() {
        stages.push(
            stage(
                &format!("contact-{}", index + 1),
                loiter_min(contact_min, None),
            )
            .then(EventAction::AerialRefuel { mass_kg: -kg }),
        );
    }
    if offloads.is_empty() && contact_min > 0.0 {
        stages.push(stage("station", loiter_min(contact_min, None)));
    }
    stages.extend([
        stage("cruise-back", cruise_km(1_000.0)),
        descent("descent", 3_000.0, false),
        landing(),
    ]);
    with_reserves(
        MissionPlan {
            name: "C5 tanker".to_owned(),
            stages,
            ..MissionPlan::default()
        },
        config,
        25_000.0,
    )
}

/// C5: offloading 40,000 kg in four contacts of 10,000 kg during a two-hour
/// orbit, 1,000 km out and 1,000 km back.
#[test]
fn c5_tanker_offloads_forty_tonnes_in_four_contacts() {
    let mut config = config_for_preset(PRESET);
    config.mission.plan = tanker_plan(&config, &[10_000.0; 4], 30.0);
    let design_mach = config.requirements.cruise_mach;
    let result = run(config);
    print_tables(&result);
    print_findings(&result);
    assert_complete_and_closed(&result);
    assert_plausible("C5 A340-300", &result, design_mach, &[]);
    assert_level_stage_sfc(&result, false, &[]);
    let history = history_of(&result);
    let offloads: Vec<_> = history
        .events
        .iter()
        .filter(|event| event.description.starts_with("offload"))
        .collect();
    assert_eq!(offloads.len(), 4);
    for event in &offloads {
        assert!((event.mass_before_kg - event.mass_after_kg - 10_000.0).abs() < 1.0e-6);
    }
    // The plan loaded the 40 t it gives away.
    let case = case_of(&result);
    assert!((case.net_offload_kg - 40_000.0).abs() < 1.0e-9);
    // The centre of gravity stays in the envelope through every contact.
    assert!(history.points.iter().all(|point| point.within_envelope));
    // Tank by tank: the quantities add up to the fuel on board at every point.
    for point in &history.points {
        let tanks: f64 = point.tank_fuel_kg.iter().map(|(_, kg)| kg).sum();
        assert!(
            (tanks - point.fuel_kg).abs() < 1.0e-6,
            "{}: tanks {tanks} against fuel {}",
            point.label,
            point.fuel_kg
        );
    }
    for point in history.points.iter().step_by(12) {
        let tanks: Vec<String> = point
            .tank_fuel_kg
            .iter()
            .map(|(id, kg)| format!("{id} {kg:.0}"))
            .collect();
        println!(
            "{:<28} fuel {:>7.0} kg  {}",
            point.label,
            point.fuel_kg,
            tanks.join(", ")
        );
    }
}

/// C5, take-off at MTOW: the fuel that is left to give away falls with the time
/// on station, and offloading it still lets the aircraft come home.
#[test]
fn c5_offload_available_at_mtow_falls_with_time_on_station() {
    let base = config_for_preset(PRESET);
    // The zero-fuel mass and the taxi fuel of this aircraft and plan.
    let mut probe = base.clone();
    probe.mission.plan = tanker_plan(&probe, &[], 30.0);
    let probe_result = run(probe);
    let zero_fuel_mass_kg = probe_result
        .mission_load_case
        .as_ref()
        .unwrap()
        .zero_fuel_mass_kg;
    let taxi_kg = case_of(&probe_result).taxi_fuel_kg;
    let mtow_ramp_fuel_kg = base.requirements.mtow_kg - zero_fuel_mass_kg + taxi_kg;
    println!(
        "MTOW {:.0} kg, zero fuel {zero_fuel_mass_kg:.0} kg",
        base.requirements.mtow_kg
    );

    let flown = |orbit_min: f64, offload_kg: f64| {
        let mut config = base.clone();
        let offloads: &[f64] = if offload_kg > 0.0 { &[offload_kg] } else { &[] };
        config.mission.plan = tanker_plan(&config, offloads, orbit_min);
        config.mission.plan.takeoff_fuel_kg = Some(mtow_ramp_fuel_kg);
        run(config)
    };
    let mut available = Vec::new();
    for orbit_min in [60.0, 120.0, 180.0] {
        // A plan with no contact: the orbit is one stage `contact-1`.
        let mut config = base.clone();
        config.mission.plan = tanker_plan(&config, &[], orbit_min);
        config.mission.plan.takeoff_fuel_kg = Some(mtow_ramp_fuel_kg);
        let result = run(config);
        let case = case_of(&result);
        assert!(
            result
                .mission_result
                .as_ref()
                .unwrap()
                .completed_summary()
                .is_some(),
            "{:?}",
            result.mission_result.as_ref().unwrap().completion_refusal()
        );
        // What is left at the end of the plan, less the contingency the plan keeps
        // on board by policy: that part is not available to give away.
        let on_board = case.final_fuel_kg() - case.contingency_kg;
        println!(
            "orbit {orbit_min:.0} min: {:.0} kg left at the end, {:.0} kg of it contingency: {on_board:.0} kg available",
            case.final_fuel_kg(),
            case.contingency_kg
        );
        available.push((orbit_min, on_board));
        assert!(
            (case.takeoff_mass_kg - base.requirements.mtow_kg).abs() < 1.0,
            "taking off at {} kg",
            case.takeoff_mass_kg
        );
    }
    assert!(
        available[0].1 > available[1].1 && available[1].1 > available[2].1,
        "{available:?}"
    );
    assert!(available[1].1 > 0.0);
    // Giving away what the two-hour orbit leaves: the aircraft, lighter after
    // the offload, still flies the rest of the plan and lands with its reserves.
    let verify = flown(120.0, available[1].1);
    let case = case_of(&verify);
    assert!(
        verify
            .mission_result
            .as_ref()
            .unwrap()
            .completed_summary()
            .is_some(),
        "{:?}",
        verify.mission_result.as_ref().unwrap().completion_refusal()
    );
    let margin_kg = case.final_fuel_kg() - case.contingency_kg;
    assert!(margin_kg >= -1.0, "{margin_kg}");
    assert!(!has_finding(&verify, FindingCode::MissionFuelShortfall));
}

/// C5, second variant: a receiver aircraft taking on 20,000 kg mid-route.
#[test]
fn c5_receiver_variant_takes_on_twenty_tonnes_mid_route() {
    let mut config = config_for_preset(PRESET);
    let mut plan = tanker_plan(&config, &[], 0.0);
    plan.stages[3] = plan.stages[3]
        .clone()
        .then(EventAction::AerialRefuel { mass_kg: 20_000.0 });
    config.mission.plan = plan;
    let result = run(config);
    print_tables(&result);
    assert_complete_and_closed(&result);
    let history = history_of(&result);
    let refuel = history
        .events
        .iter()
        .find(|event| event.description.starts_with("refuel"))
        .expect("the refuelling");
    assert!((refuel.mass_after_kg - refuel.mass_before_kg - 20_000.0).abs() < 1.0e-6);
    assert!(
        case_of(&result).net_offload_kg < 0.0,
        "fuel received reduces the fuel loaded"
    );
    assert!(!has_finding(
        &result,
        FindingCode::MissionPlanMassLimitViolation
    ));
}

/// Fuel taken on in flight that the tanks have no room for is a plan the engine
/// refuses, with the room there was, not a credit in the flown mass.
#[test]
fn fuel_received_beyond_the_room_in_the_tanks_is_refused() {
    let mut config = config_for_preset(PRESET);
    let mut plan = tanker_plan(&config, &[], 0.0);
    plan.stages[3] = plan.stages[3]
        .clone()
        .then(EventAction::AerialRefuel { mass_kg: 200_000.0 });
    plan.takeoff_fuel_kg = Some(60_000.0);
    config.mission.plan = plan;
    let error = try_run(config).expect_err("the plan is refused");
    println!("refused: {error}");
    assert!(
        error.contains("cruise-to-track") && error.contains("room for"),
        "{error}"
    );
}

/// Fuel received that fits the tanks but carries the aircraft past its maximum
/// mass is a finding.
#[test]
fn fuel_received_beyond_the_maximum_mass_is_reported() {
    let mut config = config_for_preset(PRESET);
    let mut plan = tanker_plan(&config, &[], 0.0);
    plan.stages[3] = plan.stages[3]
        .clone()
        .then(EventAction::AerialRefuel { mass_kg: 50_000.0 });
    plan.takeoff_fuel_kg = Some(60_000.0);
    config.mission.plan = plan;
    let result = run(config);
    print_findings(&result);
    assert!(has_finding(
        &result,
        FindingCode::MissionPlanMassLimitViolation
    ));
    assert!(!has_finding(
        &result,
        FindingCode::MissionPlanOperationFailed
    ));
    assert!(!result.feasibility.is_feasible());
}
