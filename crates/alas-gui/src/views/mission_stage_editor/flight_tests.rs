// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A plan built through the editors' own steps flies end to end on a
//! medium-haul route. Heavy (a full pipeline run), so the name ends in
//! `_nightly`: the `nightly` nextest profile selects it
//! (`cargo nextest run -p alas-gui --profile nightly -E 'test(_nightly)'`),
//! and CI runs it with `cargo xtask gate --nightly`. It is outside the
//! `default`, `merge` and `full` profiles.

use alas_acceptance::plausibility::mission::check_mission;
use alas_acceptance::plausibility::mission_facts::MissionFacts;
use alas_acceptance::plausibility::{Findings, Severity};
use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, FindingCode, LoadCaseSelection, PipelineOptions, PlanClosure};
use alas_route::route::{Route, RouteSource};

use super::*;
use crate::views::payload_editor;

#[test]
fn a_gui_built_plan_flies_every_stage_and_reports_them_nightly() {
    let mut state = AppState::default();
    state.load_preset("A320-200");
    // The editors' steps: a template, an added stage, a releasable point mass
    // placed on the side view and released by an event on the cruise.
    let mut plan = template_plan(&state, Template::Raymer(RaymerProfile::CommercialTransport));
    let mut editor = EditorUi::default();
    // Added before the reserve landing, so it inherits the reserve flag.
    add_stage(&mut plan, &mut editor, "Loiter", 35_000.0);
    let added = plan.position(&editor.selected.clone().unwrap()).unwrap();
    assert!(plan.stages[added].reserve);
    let items = payload_editor::test_support::releasable_store_in_the_hold(&state);
    let id = items.point_masses[0].id.clone();
    let cruise = plan.position("cruise-out").unwrap();
    plan.stages[cruise].events.push(alas_config::StageEvent {
        at: alas_config::EventTime::End,
        action: alas_config::EventAction::PayloadRelease {
            items: vec![id.clone()],
        },
    });
    assert!(plan.validate().is_ok());
    model::write_plan(&mut state, &plan);
    state.config_values["cabin"]["payload_items"] = serde_json::to_value(&items).unwrap();
    let plan = model::read_plan(&state);

    let mut config = state.typed_config().expect("typed config");
    let design_mach = config.requirements.cruise_mach;
    assert!(plan.validate_items(&config.cabin.payload_items).is_ok());
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    // A medium-haul route (LEMD-EDDF, about 1,420 km; the preset is not
    // changed): the template's trip closes it and its reserves are not part
    // of it. On the preset's own 546 km sector the trip ends 37 km short of
    // the route (509 of 546 km): a plan-closure matter, not a GUI one.
    config.departure_airport = "LEMD".to_owned();
    config.arrival_airport = "EDDF".to_owned();
    let origin = alas_config::airports::get(&config.departure_airport).unwrap();
    let destination = alas_config::airports::get(&config.arrival_airport).unwrap();
    let mut route = Route::great_circle(
        origin,
        destination,
        config.mission.great_circle_points as usize,
    );
    route.source = RouteSource::SimbriefApi;
    let route_m = route.total_distance_m();
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    };
    let result = DesignPipeline::new(config)
        .run_with_environment_and_route(&options, &RunEnvironment::default(), Some(route))
        .expect("the plan flies");

    // Completed and closed.
    let mission = result.mission_result.as_ref().expect("a mission result");
    assert!(
        mission.completed_summary().is_some(),
        "{:?}",
        mission.completion_refusal()
    );
    let load_case = result.mission_load_case.as_ref().expect("a load case");
    let LoadCaseSelection::Plan(case) = &load_case.selection else {
        panic!("a plan load case");
    };
    assert!(
        matches!(case.closure, PlanClosure::Closed | PlanClosure::Stated),
        "{:?}",
        case.closure
    );
    assert!(
        load_case.native_error.is_none(),
        "{:?}",
        load_case.native_error
    );
    assert!(result
        .feasibility
        .findings
        .iter()
        .all(|finding| finding.code != FindingCode::MissionPlanOperationFailed));

    // Every stage reported, in order, with its fuel, and the release event.
    let history = result
        .feasibility
        .mass_balance
        .as_ref()
        .and_then(|balance| balance.history.as_ref())
        .expect("a per-stage history");
    let flown: Vec<&str> = history
        .stages
        .iter()
        .map(|stage| stage.id.as_str())
        .collect();
    let planned: Vec<&str> = plan.stages.iter().map(|stage| stage.id.as_str()).collect();
    assert_eq!(flown, planned, "every stage of the plan is reported");
    assert!(history.stages.last().unwrap().mass_end_kg < history.stages[0].mass_start_kg);
    assert!(history
        .events
        .iter()
        .any(|event| event.mass_after_kg < event.mass_before_kg));
    // The trip is the route down to the decision height of the attempt to land
    // (the hold between the descent and the approach is a reserve level leg,
    // not route); the missed approach, diversion and landing are reserves.
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

    // The mission-tier plausibility rules on the flown plan, judged as
    // `assert_plausible` judges the acceptance plan cases: no High finding
    // (the owned-defect ratchet is empty here: no open model defect is known
    // for this template plan).
    // MISSION-DESCENT-RATE is judged without the final approach legs, as in
    // `assert_plausible` (the 3 degree glide path is not a cruise descent).
    let facts = MissionFacts::from_result(mission, design_mach);
    let mut cruise_to_approach = MissionFacts::from_result(mission, design_mach);
    cruise_to_approach.segments.retain(|segment| {
        !["descent_4", "final_landing", "approach"]
            .iter()
            .any(|prefix| segment.tag.starts_with(prefix))
    });
    cruise_to_approach.scheduled = cruise_to_approach.segments.len();
    let mut all_rules = Findings::new("gui-built plan");
    check_mission(&mut all_rules, &facts);
    let mut descent_rate = Findings::new("gui-built plan");
    check_mission(&mut descent_rate, &cruise_to_approach);
    let high = all_rules
        .items()
        .iter()
        .filter(|f| f.rule != "MISSION-DESCENT-RATE")
        .chain(
            descent_rate
                .items()
                .iter()
                .filter(|f| f.rule == "MISSION-DESCENT-RATE"),
        )
        .find(|f| f.severity == Severity::High);
    if let Some(finding) = high {
        panic!("plausibility: {finding}");
    }

    // The side view of the same form reads this run's numbers: the layout
    // balance of the analysis, not the pre-run estimate.
    let ctx = egui::Context::default();
    assert!(payload_editor::test_support::shown_view_is_estimate(
        &mut state, &ctx
    ));
    let analysed = result
        .baseline_report
        .as_ref()
        .and_then(|report| report.payload_layout.clone())
        .or_else(|| {
            result
                .optimized_report
                .as_ref()
                .and_then(|report| report.payload_layout.clone())
        })
        .expect("the run's payload layout");
    // The completion path of a finished run: the form and design values it
    // was built from are recorded there, not by the test.
    state.set_completed_pipeline_result(result);
    assert!(
        !payload_editor::test_support::shown_view_is_estimate(&mut state, &ctx),
        "the page's cached view switches to the run's numbers"
    );
    let (estimate, cg_x) = payload_editor::test_support::view_source(&state);
    assert!(!estimate, "the run's numbers");
    assert_eq!(cg_x, analysed.cg_x);
    // The marker is the analysis's zero-fuel CG: its own balance outcome (the
    // movable store was balanced against the cabin-synchronized empty mass).
    let reached = analysed
        .balance
        .as_ref()
        .expect("the movable store balanced")
        .reached_x_m;
    let marker = payload_editor::test_support::view_zero_fuel_cg(&state).expect("a marker");
    assert!(
        (marker - reached).abs() < 0.1,
        "marker {marker:.3} m, analysis {reached:.3} m"
    );
}
