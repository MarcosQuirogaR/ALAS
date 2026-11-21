// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! C4: a drone mothership (A220-300 airframe). C3: a humanitarian airdrop
//! (A400M, nine 1,000 kg pallets released four, then five).

use alas_config::{
    AlasConfig, CruiseExtent, EventAction, MassCategory, MissionPlan, PointMassConfig, StageKind,
};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::product_cabin_geometry;

use super::support::*;

/// A cruise of `km` at Mach 0.70 at the 25,000 ft of the case: a case choice of
/// the study (a relay loiter aircraft does not need the cruise Mach of the
/// profile), not a workaround. The profile's default is Mach 0.78. Probe
/// (`propulsion_mission_dump A220-300`, release build, one run): the
/// steady cruise at M 0.768 (23,983 ft) needs throttle 0.874-0.875 of the
/// maximum-climb rating, so thrust does not limit the cruise. The throttle
/// reaches 0.993-1.000 only in the level acceleration from M 0.641 to 0.768
/// (`speed_change_to_cruise_step_1`, 3.2 min), i.e. the acceleration demand, not a
/// steady-state thrust ceiling. The C4 claim of a thrust limit at M 0.78 is not
/// supported by this run. Whether M 0.78 sits above the VMO of the A220-300 is
/// not checked here (M 0.78 at FL250 is about 329 KCAS); that comparison uses the
/// sourced VMO and stays open.
fn mach_070_cruise_km(km: f64) -> StageKind {
    // Speed of sound at 25,000 ft in the standard atmosphere: 309.7 m/s.
    StageKind::Cruise {
        extent: CruiseExtent::DistanceKm(km),
        altitude_ft: None,
        agl: false,
        speed_m_s: Some(0.70 * 309.7),
    }
}

/// C4: a 1,000 kg drone carried at 0.55 L below the keel (drag area 0.3 m2
/// while carried), released after the first 600 km; a three-hour relay loiter
/// at FL250; the return; the reserves.
#[test]
fn c4_drone_mothership_releases_the_drone_and_loiters_three_hours() {
    let mut config = config_for_preset("A220-300");
    let preset = alas_config::presets::get("A220-300").unwrap();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .unwrap();
    let g = product_cabin_geometry(&plane, &config).unwrap();
    let x = 0.55 * g.fus_len;
    let keel = g.zc_at(x) - 0.5 * g.height_at(x);
    config.cabin.payload_items.point_masses = vec![PointMassConfig {
        id: "drone".to_owned(),
        name: "Drone".to_owned(),
        x_m: x,
        y_m: 0.0,
        z_m: keel - 0.25,
        mass_kg: 1_000.0,
        category: MassCategory::Payload,
        inertia_kgm2: None,
        releasable: true,
        drag_area_m2: Some(0.3),
        movable: false,
        x_range_m: None,
    }];
    let plan = MissionPlan {
        name: "C4 drone mothership".to_owned(),
        stages: vec![
            stage("taxi", StageKind::Taxi { time_min: 10.0 }),
            takeoff(),
            climb("climb", 25_000.0),
            stage("cruise-out", mach_070_cruise_km(600.0)).then(EventAction::PayloadRelease {
                items: vec!["drone".to_owned()],
            }),
            stage("relay", loiter_min(180.0, None)),
            stage("cruise-back", mach_070_cruise_km(600.0)),
            descent("descent", 3_000.0, false),
            landing(),
        ],
        ..MissionPlan::default()
    };
    config.mission.plan = with_reserves(plan, &config, 25_000.0);
    let design_mach = config.requirements.cruise_mach;
    let result = run(config);
    print_tables(&result);
    print_findings(&result);
    assert_complete_and_closed(&result);
    assert_plausible("C4 A220-300", &result, design_mach, &[]);
    assert_level_stage_sfc(&result, false, &[]);
    let history = history_of(&result);

    // The release: the drone's mass leaves at the end of the first cruise and
    // the centre of gravity jumps by exactly the first moment of the drone.
    let release = history
        .events
        .iter()
        .find(|event| event.description.starts_with("release"))
        .expect("the release event");
    assert!((release.mass_before_kg - release.mass_after_kg - 1_000.0).abs() < 1.0e-6);
    let (before, after) = around_event(history, "release");
    let expected = cg_after_release(
        history.points[before].mass_kg,
        history.points[before].cg_m[0],
        1_000.0,
        x,
    );
    assert!((history.points[after].cg_m[0] - expected).abs() < 1.0e-9);
    assert!(
        release.cg_after_pct_mac < release.cg_before_pct_mac,
        "the drone hangs aft of the centre of gravity: its release moves it forward"
    );
    println!(
        "C4 CG jump at release: {:.2} -> {:.2} % MAC",
        release.cg_before_pct_mac, release.cg_after_pct_mac
    );

    assert_trim_bounded(&result);
    // The drone's drag area is carried to the release and not after it.
    let carried = added_area_of_stage_m2(&result, 3);
    let after_release = added_area_of_stage_m2(&result, 5);
    println!(
        "C4 added drag area: carried {carried:.3} m2, after the release {after_release:.3} m2"
    );
    assert!(
        (carried - after_release - 0.3).abs() < 0.01,
        "the drone's 0.3 m2 leaves with it: {carried} then {after_release}"
    );

    // Three hours of loiter, flown at the aircraft's own speed of minimum drag.
    let mission = result.mission_result.as_ref().unwrap();
    let relay = mission
        .segments
        .iter()
        .find(|segment| segment.spec.tag == "loiter@04")
        .expect("the relay loiter");
    let c = &relay.conditions;
    assert!((c.time_s.last().unwrap() - c.time_s.first().unwrap() - 10_800.0).abs() < 1.0);
    let relay_stage = &history.stages[4];
    println!(
        "C4 relay: {:.0} kg of fuel in {:.0} s, {:.0} kg/h",
        relay_stage.fuel_kg,
        relay_stage.time_s,
        relay_stage.fuel_kg / relay_stage.time_s * 3600.0
    );
    assert!(history.points.iter().all(|point| point.within_envelope));
}

/// The C3 configuration: the A400M with nine 1,000 kg pallets on its main deck
/// (7 on the floor and 2 on the ramp), released in two low-level drop runs,
/// four then five, with `ramp_open_m2` of equivalent flat-plate area while the
/// ramp is open. The pallets leave by the ramp, so the aftmost four go first.
/// Returns the configuration and the pallet stations aft to forward.
fn c3_configuration(ramp_open_m2: f64) -> (AlasConfig, Vec<(f64, f64, f64)>) {
    let mut config = config_for_preset("A400M");
    let preset = alas_config::presets::get("A400M").unwrap();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .unwrap();
    config.requirements.aircraft_type = "cargo".to_owned();
    config.requirements.cargo_payload_kg = 36_000.0;
    let mut positions: Vec<_> = alas_payload::build_payload_layout(&plane, &config, 0.0, 0.0)
        .unwrap()
        .items
        .iter()
        .filter(|item| item.kind == alas_payload::ItemKind::Uld)
        .map(|item| (item.x, item.y, item.z))
        .collect();
    assert_eq!(positions.len(), 9, "7 floor + 2 ramp positions");
    positions.sort_by(|a, b| b.0.total_cmp(&a.0));
    config.requirements.cargo_payload_kg = 0.0;
    config.cabin.payload_items.point_masses = positions
        .iter()
        .enumerate()
        .map(|(n, &(x, y, z))| PointMassConfig {
            id: format!("pallet{}", n + 1),
            name: format!("Pallet {}", n + 1),
            x_m: x,
            y_m: y,
            z_m: z,
            mass_kg: 1_000.0,
            category: MassCategory::Payload,
            inertia_kgm2: None,
            releasable: true,
            // Cargo inside the closed hold has no external drag.
            drag_area_m2: None,
            movable: false,
            x_range_m: None,
        })
        .collect();
    let release = |ids: std::ops::Range<usize>| EventAction::PayloadRelease {
        items: ids.map(|n| format!("pallet{}", n + 1)).collect(),
    };
    // The ramp-and-door-open drag of a drop run is a study input: no public
    // measured value was found, so an equivalent flat-plate area is stated,
    // labelled as such in the plan; the test flies the plan with and without
    // it to show what it costs.
    let drop_run = |id: &str| {
        let stage = stage(
            id,
            StageKind::Cruise {
                extent: CruiseExtent::TimeMin(2.0),
                altitude_ft: None,
                agl: false,
                speed_m_s: None,
            },
        );
        if ramp_open_m2 > 0.0 {
            stage.with_drag_area(
                ramp_open_m2,
                "study input: ramp and door open, no published measurement",
            )
        } else {
            stage
        }
    };
    let plan = MissionPlan {
        name: "C3 humanitarian airdrop".to_owned(),
        stages: vec![
            stage("taxi", StageKind::Taxi { time_min: 10.0 }),
            takeoff(),
            climb("climb", 20_000.0),
            stage("cruise-out", cruise_km(1_500.0)),
            descent("descent-to-dz1", 1_000.0, true),
            drop_run("drop-1").then(release(0..4)),
            climb("climb-from-dz1", 10_000.0),
            stage("transit", cruise_km(200.0)),
            descent("descent-to-dz2", 1_000.0, true),
            drop_run("drop-2").then(release(4..9)),
            climb("climb-out", 20_000.0),
            stage("cruise-back", cruise_km(1_700.0)),
            descent("descent", 3_000.0, false),
            landing(),
        ],
        ..MissionPlan::default()
    };
    config.mission.plan = with_reserves(plan, &config, 25_000.0);
    (config, positions)
}

/// C3: nine 1,000 kg pallets on the A400M main
/// deck, released in two low-level drop runs, four then five.
#[test]
fn c3_a400m_airdrop_releases_four_then_five_pallets() {
    let (config, positions) = c3_configuration(1.0);
    let design_mach = config.requirements.cruise_mach;
    let result = run(config);
    print_tables(&result);
    print_findings(&result);
    assert_complete_and_closed(&result);
    assert_plausible("C3 A400M", &result, design_mach, &[]);
    // The 1,500 ft final-reserve hold of the A400M implies 0.20 mg/(W s) of brake
    // power against 0.08-0.11 in its cruise stages: the turboprop deck's part-load
    // fuel flow at the minimum-power speed, an open turboprop part-load
    // hold fuel-flow defect; the entry is a ratchet that goes with the fix.
    assert_level_stage_sfc(&result, true, &["final-reserve"]);
    let history = history_of(&result);

    // Two release events, four then five pallets, 4,000 then 5,000 kg.
    let drops: Vec<_> = history
        .events
        .iter()
        .filter(|event| event.description.starts_with("release"))
        .collect();
    assert_eq!(drops.len(), 2);
    for (event, mass) in drops.iter().zip([4_000.0, 5_000.0]) {
        assert!((event.mass_before_kg - event.mass_after_kg - mass).abs() < 1.0e-6);
    }
    // The centre of gravity after each drop is the first moment of what is
    // left, from the pallets' own stations.
    for (label, ids) in [("drop-1 (", 0..4_usize), ("drop-2 (", 4..9)] {
        let moment: f64 = ids.clone().map(|n| positions[n].0 * 1_000.0).sum();
        let mass = 1_000.0 * ids.len() as f64;
        let (before, after) = around_event(history, label);
        let expected = cg_after_release(
            history.points[before].mass_kg,
            history.points[before].cg_m[0],
            mass,
            moment / mass,
        );
        assert!((history.points[after].cg_m[0] - expected).abs() < 1.0e-9);
    }
    // The first drop empties the ramp and the aft floor: the centre of gravity
    // moves forward; the second drop takes the forward pallets: it moves aft.
    assert!(drops[0].cg_after_pct_mac < drops[0].cg_before_pct_mac);
    assert!(drops[1].cg_after_pct_mac > drops[1].cg_before_pct_mac);

    // The drag: the pallets are inside the closed hold and add none; the only
    // added drag of the whole flight is the open ramp of the two drop runs.
    // (The take-off, approach, go-around, diversion and landing legs also carry
    // the flap and gear drag of their configuration, which is not what is
    // checked.)
    let stages = &result.config.mission.plan.stages;
    let configured = ["takeoff", "approach", "go-around", "divert", "landing"];
    let added: Vec<f64> = (0..stages.len())
        .map(|n| added_area_of_stage_m2(&result, n))
        .collect();
    println!("C3 added drag area by stage, m2: {added:.3?}");
    assert_trim_bounded(&result);
    for (index, stage) in stages.iter().enumerate() {
        if configured.contains(&stage.id.as_str()) {
            continue;
        }
        // The ramp area (1.0 m2) is the only added drag of the flight (the trim
        // drag of the centre of gravity is reported apart and left out here).
        let expected = if stage.id.starts_with("drop") {
            1.0
        } else {
            0.0
        };
        assert!(
            (added[index] - expected).abs() < 0.01,
            "stage {} adds {} m2 of drag against {expected}",
            stage.id,
            added[index]
        );
    }
    // The mass at landing carries no pallet.
    let landing_stage = history.stages.last().unwrap();
    assert!(landing_stage.mass_end_kg < history.points[0].mass_kg - 9_000.0);

    // The envelope, state by state. The A400M's operating empty centre of
    // gravity lies aft of the model's aft limit (registered known line
    // A400M CG-ENVELOPE): while that line stands the verdict is the
    // base aircraft's and the plan reports it at every stage; once it is fixed
    // (the line removed) the case asserts the criterion of C3, that
    // the centre of gravity stays in the envelope through each release.
    if known_defect("A400M", "CG-ENVELOPE") {
        assert!(
            has_finding(&result, alas_pipeline::FindingCode::CgEnvelopeViolation),
            "the envelope violation is reported, not hidden"
        );
    } else {
        assert!(history.points.iter().all(|point| point.within_envelope));
    }

    // What the stated ramp area costs: the same plan flown without it.
    let (without, _) = c3_configuration(0.0);
    let without = run(without);
    let (with_fuel, without_fuel) = (
        case_of(&result).flight_burn_kg,
        case_of(&without).flight_burn_kg,
    );
    println!(
        "C3 burn with the 1.0 m2 ramp-open area {with_fuel:.1} kg, without {without_fuel:.1} kg: {:.1} kg",
        with_fuel - without_fuel
    );
    assert!(with_fuel > without_fuel, "an open ramp costs fuel");
    assert!(
        with_fuel - without_fuel < 0.01 * without_fuel,
        "two two-minute drop runs cost under one percent of the burn"
    );
}
