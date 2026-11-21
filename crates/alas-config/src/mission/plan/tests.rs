// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{MassCategory, PayloadItemsConfig, PointMassConfig};

fn plan() -> MissionPlan {
    MissionPlan::from_profile(&crate::MissionProfileConfig::default(), 35_000.0)
}

fn pallet(id: &str, releasable: bool) -> PointMassConfig {
    PointMassConfig {
        id: id.to_owned(),
        name: id.to_owned(),
        x_m: 10.0,
        y_m: 0.0,
        z_m: 0.0,
        mass_kg: 1_000.0,
        category: MassCategory::Payload,
        inertia_kgm2: None,
        releasable,
        drag_area_m2: None,
        movable: false,
        x_range_m: None,
    }
}

#[test]
fn the_default_plan_is_the_standard_mission_and_serialises_to_nothing() {
    let plan = MissionPlan::default();
    assert!(plan.is_empty() && !plan.has_stages());
    assert!(plan.validate().is_ok());
    assert_eq!(serde_json::to_value(&plan).unwrap(), serde_json::json!({}));
    let config = crate::MissionConfig::default();
    let value = serde_json::to_value(&config).unwrap();
    assert!(value.get("plan").is_none(), "a default file is unchanged");
}

#[test]
fn the_standard_sequence_is_a_valid_plan_that_round_trips() {
    let plan = plan();
    plan.validate().unwrap();
    let text = serde_json::to_string(&plan).unwrap();
    let loaded: MissionPlan = serde_json::from_str(&text).unwrap();
    assert_eq!(loaded, plan);
    let kinds: Vec<_> = plan.stages.iter().map(|s| s.kind.label()).collect();
    assert_eq!(kinds, ["Takeoff", "Climb", "Cruise", "Descent", "Landing"]);
}

#[test]
fn every_raymer_profile_is_a_valid_plan_with_the_segments_of_the_figure() {
    let inputs = TemplateInputs::from_policy(&crate::FuelPolicyConfig::default(), 30_000.0);
    let expected: [(RaymerProfile, &[&str]); 4] = [
        (
            RaymerProfile::SimpleCruise,
            &["Takeoff", "Climb", "Cruise", "Descent", "Loiter", "Landing"],
        ),
        (
            RaymerProfile::CommercialTransport,
            &[
                "Takeoff",
                "Climb",
                "Cruise",
                "Descent",
                "Loiter",
                "Approach",
                "Go Around",
                "Diversion",
                "Loiter",
                "Landing",
            ],
        ),
        (
            RaymerProfile::LowLevelStrike,
            &[
                "Takeoff", "Climb", "Cruise", "Loiter", "Cruise", "Climb", "Cruise", "Loiter",
                "Landing",
            ],
        ),
        (
            RaymerProfile::AirSuperiority,
            &[
                "Takeoff", "Climb", "Cruise", "Combat", "Cruise", "Loiter", "Landing",
            ],
        ),
    ];
    for (profile, labels) in expected {
        let plan = MissionPlan::raymer(profile, &inputs);
        plan.validate()
            .unwrap_or_else(|e| panic!("{}: {e}", profile.title()));
        let kinds: Vec<_> = plan.stages.iter().map(|s| s.kind.label()).collect();
        assert_eq!(kinds, labels, "{}", profile.title());
    }
    // The weight drop is a release event of the stage before the climb.
    let strike = MissionPlan::raymer(RaymerProfile::LowLevelStrike, &inputs);
    let drop = strike.position("weight-drop").unwrap();
    assert!(matches!(
        strike.stages[drop].events[0].action,
        EventAction::PayloadRelease { .. }
    ));
}

#[test]
fn takeoff_must_come_first_and_landing_last() {
    let mut plan = plan();
    // Take-off after the climb: refused, plan unchanged.
    let before = plan.clone();
    assert!(matches!(
        plan.move_stage(0, 1),
        Err(PlanError::TakeoffMustComeFirst(_))
    ));
    assert_eq!(plan, before);
    // Landing before the descent: the plan would end airborne.
    assert!(matches!(
        plan.move_stage(4, 3),
        Err(PlanError::LandingMustBeLast(_))
    ));
    assert_eq!(plan, before);
    // Swapping the climb and cruise keeps the order legal.
    plan.move_stage(1, 2).unwrap();
    assert_eq!(plan.stages[1].id, "cruise");
}

#[test]
fn events_move_with_their_stage() {
    let mut plan = plan();
    plan.stages[2] = plan.stages[2]
        .clone()
        .then(EventAction::AerialRefuel { mass_kg: 5_000.0 });
    plan.move_stage(2, 1).unwrap();
    assert_eq!(plan.stages[1].id, "cruise");
    assert_eq!(plan.stages[1].events.len(), 1);
}

#[test]
fn a_go_around_needs_an_approach_and_taxi_needs_the_ground() {
    let mut plan = plan();
    plan.stages.insert(
        3,
        PlanStage::new(
            "ga",
            StageKind::GoAround {
                altitude_ft: 3_000.0,
                agl: true,
            },
        ),
    );
    assert!(matches!(
        plan.validate(),
        Err(PlanError::GoAroundNeedsApproach(_))
    ));
    let mut taxi_in_air = self::plan();
    taxi_in_air
        .stages
        .insert(2, PlanStage::new("t", StageKind::Taxi { time_min: 3.0 }));
    assert!(matches!(
        taxi_in_air.validate(),
        Err(PlanError::NeedsGround(_))
    ));
    // Taxi out before the take-off and taxi in after the landing is legal.
    let mut ground = self::plan();
    ground
        .stages
        .insert(0, PlanStage::new("out", StageKind::Taxi { time_min: 8.0 }));
    ground
        .stages
        .push(PlanStage::new("in", StageKind::Taxi { time_min: 5.0 }));
    ground.validate().unwrap();
}

#[test]
fn ids_and_numbers_are_checked() {
    let mut plan = plan();
    plan.stages[1].id = "takeoff".to_owned();
    assert!(matches!(plan.validate(), Err(PlanError::DuplicateId(_))));
    let mut plan = self::plan();
    plan.stages[1].kind = StageKind::Climb {
        altitude_ft: f64::NAN,
        agl: false,
        rate_m_s: None,
    };
    assert!(matches!(plan.validate(), Err(PlanError::Parameter { .. })));
    let mut plan = self::plan();
    plan.stages[2].kind = StageKind::Combat {
        time_min: 5.0,
        load_factor: 0.5,
        altitude_ft: None,
        agl: false,
        speed_m_s: None,
    };
    assert!(plan.validate().is_err());
}

#[test]
fn a_release_needs_a_carried_releasable_item_and_happens_once() {
    let items = PayloadItemsConfig {
        zones: Vec::new(),
        point_masses: vec![pallet("a", true), pallet("b", true), pallet("fixed", false)],
        ..PayloadItemsConfig::default()
    };
    let release = |ids: &[&str]| EventAction::PayloadRelease {
        items: ids.iter().map(|s| (*s).to_owned()).collect(),
    };
    let mut plan = plan();
    plan.stages[2] = plan.stages[2].clone().then(release(&["a"]));
    plan.validate_items(&items).unwrap();
    plan.stages[3] = plan.stages[3].clone().then(release(&["a"]));
    assert!(plan.validate_items(&items).is_err(), "twice");
    plan.stages[3].events.clear();
    plan.stages[3] = plan.stages[3].clone().then(release(&["fixed"]));
    assert!(plan.validate_items(&items).is_err(), "not releasable");
    plan.stages[3].events.clear();
    plan.stages[3] = plan.stages[3].clone().then(release(&["nope"]));
    assert!(plan.validate_items(&items).is_err(), "unknown");
    // Releasing nothing by name releases what can be released.
    plan.stages[3].events.clear();
    plan.stages[3] = plan.stages[3].clone().then(release(&[]));
    plan.stages[4] = plan.stages[4].clone().then(release(&["b"]));
    assert!(plan.validate_items(&items).is_err(), "b went with the rest");
}

#[test]
fn a_store_added_in_flight_can_be_released_later() {
    let items = PayloadItemsConfig::default();
    let mut plan = plan();
    plan.stages[1] = plan.stages[1].clone().then(EventAction::StoreAdd {
        id: "pod".to_owned(),
        mass_kg: 300.0,
        x_m: 20.0,
        y_m: 0.0,
        z_m: -1.0,
        drag_area_m2: Some(0.1),
    });
    plan.stages[3] = plan.stages[3].clone().then(EventAction::PayloadRelease {
        items: vec!["pod".to_owned()],
    });
    plan.validate_items(&items).unwrap();
}

#[test]
fn the_reserves_of_a_policy_become_stages_before_the_landing() {
    let policy = crate::FuelPolicyConfig::default();
    let plan = plan().with_policy_reserves(&policy, 25_000.0, 3_000.0);
    plan.validate().unwrap();
    let kinds: Vec<_> = plan.stages.iter().map(|s| s.kind.label()).collect();
    assert_eq!(
        kinds,
        [
            "Takeoff",
            "Climb",
            "Cruise",
            "Descent",
            "Approach",
            "Go Around",
            "Diversion",
            "Loiter",
            "Landing"
        ]
    );
    assert!(plan.stages[4..].iter().all(|s| s.reserve));
    assert_eq!(plan.contingency_fraction, policy.contingency_trip_fraction);
}

#[test]
fn the_documented_example_plan_parses_and_validates() {
    let text = include_str!("../../../../../docs/schemas/mission-plan.example.json");
    let plan: MissionPlan = serde_json::from_str(text).unwrap();
    plan.validate().unwrap();
    assert_eq!(plan.stages.len(), 8);
    assert!(matches!(
        plan.stages[1].kind,
        StageKind::Takeoff { field: Some(_) }
    ));
}

#[test]
fn the_missed_approach_climbs_to_the_interception_height_not_to_cruise_and_no_template_stage_overshoots(
) {
    // Raymer, Aircraft Design: A Conceptual Approach, 6th ed., Fig. 3.2 (p. 32,
    // PDF page 66, read from the rendered page): after "Attempt to land" the
    // commercial profile climbs back only to the diversion level; the figure
    // states no altitude, so the template uses the height the approach was
    // intercepted at (3 000 ft above the field) and the diversion cruise level
    // is an input.
    let mut inputs = TemplateInputs::from_policy(&crate::FuelPolicyConfig::default(), 35_000.0);
    let plan = MissionPlan::raymer(RaymerProfile::CommercialTransport, &inputs);
    let go_around = &plan.stages[plan.position("climb-after-go-around").unwrap()];
    assert!(matches!(
        go_around.kind,
        StageKind::GoAround { altitude_ft, agl: true } if altitude_ft == 3_000.0
    ));
    // Raymer p. 32 (section 3.4.1): the loiter before the attempt to land is
    // "typically 20-30 min [at 10,000 ft {3048 m}]", below the cruise level of
    // Fig. 3.2. The trip descends to that level and the loiter holds it.
    let loiter = &plan.stages[plan.position("loiter").unwrap()];
    assert!(matches!(
        loiter.kind,
        StageKind::Loiter { altitude_ft: Some(ft), agl: false, .. }
            if ft == 10_000.0 && ft < inputs.cruise_altitude_ft
    ));
    // The trip is the route down to the decision height of the attempt to land
    // (the descent and the approach); the hold between them is a level leg, and
    // the missed approach, the diversion and its landing are the reserve.
    let attempt = plan.position("attempt-to-land").unwrap();
    let descend = plan.position("descend").unwrap();
    assert!(plan.stages[..=attempt]
        .iter()
        .enumerate()
        .all(|(index, stage)| { stage.reserve == (index == plan.position("loiter").unwrap()) }));
    assert!(descend < attempt);
    assert!(plan.stages[attempt + 1..].iter().all(|stage| stage.reserve));
    // The inputs a study states reach the stages.
    inputs.missed_approach_ft = 2_500.0;
    inputs.alternate_cruise_ft = 24_000.0;
    let plan = MissionPlan::raymer(RaymerProfile::CommercialTransport, &inputs);
    let go_around = &plan.stages[plan.position("climb-after-go-around").unwrap()];
    assert!(matches!(
        go_around.kind,
        StageKind::GoAround { altitude_ft, agl: true } if altitude_ft == 2_500.0
    ));
    let divert = &plan.stages[plan.position("divert").unwrap()];
    assert!(matches!(
        divert.kind,
        StageKind::Diversion { cruise_altitude_ft, .. } if cruise_altitude_ft == 24_000.0
    ));
    plan.validate().unwrap();
    // The missed approach height is the profile's own approach rung.
    let profile = crate::MissionProfileConfig {
        descent_4_altitude_ft: 2_000.0,
        ..Default::default()
    };
    let with_profile = TemplateInputs::from_policy(&crate::FuelPolicyConfig::default(), 35_000.0)
        .with_profile(&profile);
    assert_eq!(with_profile.missed_approach_ft, 2_000.0);
    // Across all four templates no stage targets above the cruise altitude
    // (above mean sea level, or above the field), and the loiter and cruise
    // altitudes a template states stay at or below it.
    for profile in RaymerProfile::ALL {
        for stage in &MissionPlan::raymer(profile, &inputs).stages {
            let (altitude_ft, agl) = match stage.kind {
                StageKind::Climb {
                    altitude_ft, agl, ..
                }
                | StageKind::GoAround { altitude_ft, agl }
                | StageKind::Descent { altitude_ft, agl } => (altitude_ft, agl),
                StageKind::Loiter {
                    altitude_ft: Some(altitude_ft),
                    agl,
                    ..
                }
                | StageKind::Cruise {
                    altitude_ft: Some(altitude_ft),
                    agl,
                    ..
                } => (altitude_ft, agl),
                StageKind::Diversion {
                    cruise_altitude_ft, ..
                } => (cruise_altitude_ft, false),
                _ => continue,
            };
            assert!(
                agl || altitude_ft <= inputs.cruise_altitude_ft,
                "{}: {}",
                profile.title(),
                stage.id
            );
        }
    }
}

#[test]
fn the_missed_approach_climbs_above_the_field_it_was_attempted_at() {
    // A height above the field goes up from the decision height over a field of
    // any elevation, so the template does not need to know the field.
    let inputs = TemplateInputs::from_policy(&crate::FuelPolicyConfig::default(), 35_000.0);
    let plan = MissionPlan::raymer(RaymerProfile::CommercialTransport, &inputs);
    let go_around = &plan.stages[plan.position("climb-after-go-around").unwrap()];
    assert!(matches!(
        go_around.kind,
        StageKind::GoAround { altitude_ft, agl: true } if altitude_ft == inputs.missed_approach_ft
    ));
    assert!(inputs.missed_approach_ft > CATEGORY_I_DECISION_HEIGHT_FT);
    plan.validate().unwrap();
    // The simple-cruise descent to the holding level is above the field.
    let simple = MissionPlan::raymer(RaymerProfile::SimpleCruise, &inputs);
    assert!(matches!(
        simple.stages[simple.position("descend").unwrap()].kind,
        StageKind::Descent { agl: true, .. }
    ));
}
