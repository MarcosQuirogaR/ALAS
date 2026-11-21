// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! SUAVE mission provenance at the public pipeline boundary.
//!
//! The lower-level mission parity tests prove the native solver against a
//! SUAVE fixture. This test proves that the public pipeline reaches that same
//! native solver while isolating the declared structural-wingbox CG correction
//! from the unchanged force, atmosphere, aerodynamic, and mass-flow outputs.

use alas_config::airports::get as get_airport;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, FullAnalysis, PipelineOptions};
use alas_route::route::{Route, RouteSource, Waypoint, EARTH_RADIUS_M};
use serde::Deserialize;

/// Tag suffix Product mission guidance gives the upper piece of a climb split
/// at its saturation altitude (`mission_stage::guidance`).
/// Suffixes of the pieces the product guidance cuts a scheduled leg into: the upper
/// piece of a climb that saturates, the lower piece of a descent that reaches idle,
/// the piece that flies the exit speed change of a leg.
const PIECE_TAG_SUFFIXES: [&str; 4] = ["_upper", "_lower", "_path_exit", "_piece"];
/// Suffix of the piece that flies the entry speed change of a leg (it precedes the leg).
const PATH_ENTRY_TAG_SUFFIX: &str = "_path_entry";

#[derive(Debug, Deserialize)]
struct Fixture {
    call_graph: CallGraph,
    solver: Solver,
    inputs: Inputs,
    geometry: Geometry,
    segments: Vec<FixtureSegment>,
}

#[derive(Debug, Deserialize)]
struct FixtureSegment {
    tag: String,
    selected_points: std::collections::BTreeMap<String, FixturePoint>,
}

#[derive(Debug, Deserialize)]
struct FixturePoint {
    altitude_m: f64,
}

#[derive(Debug, Deserialize)]
struct CallGraph {
    reference: Vec<String>,
    native: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Solver {
    reference: String,
    native: String,
    segment_count: usize,
    control_points: Vec<usize>,
}

#[derive(Debug, Deserialize)]
struct Inputs {
    mission_tag: String,
    route_distance_m: f64,
}

#[derive(Debug, Deserialize)]
struct Geometry {
    reference_area_m2: f64,
    reference_span_m: f64,
}

fn fixture() -> Fixture {
    alas_testkit::load("mission", "w64_provenance")
}

fn route(config: &AlasConfig, distance_m: f64) -> Route {
    let origin = get_airport(&config.departure_airport)
        .unwrap_or_else(|error| panic!("configured origin: {error}"));
    let destination = get_airport(&config.arrival_airport)
        .unwrap_or_else(|error| panic!("configured destination: {error}"));
    let latitude = origin.latitude_deg.to_radians();
    let central_angle = distance_m / EARTH_RADIUS_M;
    let longitude_delta = 2.0
        * ((central_angle / 2.0).sin() / latitude.cos())
            .asin()
            .to_degrees();
    Route {
        waypoints: vec![
            Waypoint::named(
                origin.latitude_deg,
                origin.longitude_deg,
                origin.icao.clone(),
            ),
            Waypoint::named(
                origin.latitude_deg,
                origin.longitude_deg + longitude_delta,
                "SUAVE_FIXTURE",
            ),
        ],
        source: RouteSource::SimbriefApi,
        origin_airport: Some(origin.clone()),
        dest_airport: Some(destination.clone()),
    }
}

fn options() -> PipelineOptions {
    PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    }
}

#[test]
fn public_native_mission_matches_pinned_provenance_checkpoints() {
    let fixture = fixture();
    assert_eq!(
        fixture.call_graph.reference,
        vec![
            "alas.pipeline -> alas.integration.suave_mission.build_mission_request",
            "external tools/suave_runner/vehicle_builder.build_vehicle",
            "external tools/suave_runner/mission_builder.mission_setup",
            "SUAVE.Analyses.Mission.Sequential_Segments.evaluate",
            "SUAVE.Methods.Missions.Segments.converge_root",
        ]
    );
    assert_eq!(
        fixture.call_graph.native,
        vec![
            "DesignPipeline.run_with_environment_and_route",
            "alas_pipeline::mission_stage::evaluate",
            "alas_pipeline::mission_stage::build_analyses",
            "alas_mission::Mission::evaluate",
            "alas_mission::converge_root",
        ]
    );
    assert_eq!(
        fixture.solver.reference,
        "scipy.optimize.fsolve / MINPACK hybrd"
    );
    assert_eq!(fixture.solver.native, "alas_math::hybrd");

    let mut config = AlasConfig {
        departure_airport: "LEMD".to_owned(),
        arrival_airport: "HKJK".to_owned(),
        ..AlasConfig::default()
    };
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    // This fixture predates the product transport-planform stations. Keep its
    // frozen three-station geometry so the test continues to isolate the
    // declared structural-wingbox CG correction instead of mixing a geometry
    // model change into the SUAVE mission-solver parity evidence.
    config.geometry.wing.side_of_body_span_fraction = None;
    config.geometry.wing.side_of_body_chord_ratio = None;
    config.geometry.wing.kink_span_fraction = None;
    config.geometry.wing.outboard_le_sweep_deg = None;
    // Likewise the fixture's mission profile: takeoff configuration to
    // 3,048 m above the field and the landing configuration from 6,500 ft,
    // the defaults it was generated with (its `initial_climb` starts at
    // 3,658 m and its `final_landing` at 1,981.2 m). The product defaults
    // since moved to 1,500 ft (NADP) and 3,000 ft (glide-path interception),
    // which drops the 3,000 ft rung below the 1,624 m Nairobi field.
    config.mission.profile.takeoff_altitude_gain_m = 3048.0;
    config.mission.profile.descent_4_altitude_ft = 6500.0;
    let active_route = route(&config, fixture.inputs.route_distance_m);
    let result = DesignPipeline::new(config.clone())
        .run_with_environment_and_route(
            &options(),
            &RunEnvironment::default(),
            Some(active_route.clone()),
        )
        .unwrap_or_else(|error| panic!("public native mission path: {error}"));
    assert_eq!(result.route, Some(active_route));

    let report = result
        .optimized_report
        .as_ref()
        .unwrap_or_else(|| panic!("public path did not publish its analysis report"));
    let mission = result.mission_result.as_ref().unwrap_or_else(|| {
        panic!(
            "public path did not publish mission telemetry: {:?}",
            result.feasibility.native_mission_error
        )
    });
    let reference_report = FullAnalysis::new_reference_compatibility(config.clone())
        .run(&DesignVector::default(), true)
        .unwrap_or_else(|error| panic!("reference-compatible report: {error}"));
    let cg_shift_m = report
        .physical_cg
        .iter()
        .zip(reference_report.physical_cg)
        .map(|(&product, reference)| (product - reference).powi(2))
        .sum::<f64>()
        .sqrt();
    assert!(
        cg_shift_m > 0.1,
        "the public path must retain the structural-wingbox correction: product={:?}, reference={:?}",
        report.physical_cg,
        reference_report.physical_cg
    );

    // Product reference propagation: the published summary and every public
    // mission consumer must use projected XY area/lateral span.  The frozen
    // geometry fixture intentionally remains the unfolded compatibility
    // evidence and is checked through the explicit reference report below.
    let main_wing = match report.airplane.wings.first() {
        Some(main_wing) => main_wing,
        None => panic!("product report has a main wing"),
    };
    assert!((report.airplane.s_ref - main_wing.reference_area()).abs() < 1.0e-9);
    assert!((report.airplane.b_ref - main_wing.reference_span()).abs() < 1.0e-9);
    assert!((report.geometry_summary["wing_area_m2"] - report.airplane.s_ref).abs() < 1.0e-9);
    assert!((report.geometry_summary["span_m"] - report.airplane.b_ref).abs() < 1.0e-9);
    assert!(
        (report.geometry_summary["aspect_ratio"]
            - report.airplane.b_ref * report.airplane.b_ref / report.airplane.s_ref)
            .abs()
            < 1.0e-9
    );
    assert!((report.airplane.s_ref - main_wing.unfolded_area()).abs() > 1.0e-6);

    assert!((reference_report.airplane.s_ref - fixture.geometry.reference_area_m2).abs() < 1.0e-6);
    assert!((reference_report.airplane.b_ref - fixture.geometry.reference_span_m).abs() < 1.0e-6);
    assert!(
        (reference_report.geometry_summary["wing_area_m2"] - fixture.geometry.reference_area_m2)
            .abs()
            < 1.0e-6
    );
    assert_eq!(fixture.inputs.mission_tag, "LEMD_to_HKJK");

    // The installed GE9X rating, not the historical cruise-required target,
    // sizes product turbofan flow.  This is 2 * 467 kN = 934,000 N.
    let rated_total_thrust_n = config.geometry.engine.thrust_kn()
        * config.geometry.engine.spanwise_positions_m.len() as f64
        * 1000.0;
    assert!((rated_total_thrust_n - 934_000.0).abs() < 1.0e-9);

    // The fixture records the SUAVE reference schedule. Product guidance may
    // split a climb that saturates the throttle part way up into a lower piece
    // that keeps the scheduled tag and an upper piece tagged `<tag>_upper`
    // (repeated splits append the suffix again). Compare like with like: merge
    // every upper piece back into the scheduled segment it came from, and
    // require every extra flown segment to be exactly such a split.
    assert_eq!(mission.scheduled_segment_count, mission.segments.len());
    assert_eq!(mission.solutions.len(), mission.segments.len());
    // The flight opens with the climb at V2 the reference does not have: from the field to the
    // acceleration height, at one speed (the legs are skipped from the reference comparison below).
    {
        let field_m = get_airport("LEMD")
            .unwrap_or_else(|error| panic!("LEMD: {error}"))
            .elevation_m;
        let climb: Vec<_> = mission
            .segments
            .iter()
            .take_while(|segment| segment.spec.tag.starts_with("takeoff_climb"))
            .collect();
        let (Some(first), Some(last)) = (climb.first(), climb.last()) else {
            panic!("the flight opens with takeoff_climb");
        };
        let altitudes = &last.conditions.altitude_m;
        assert!((first.conditions.altitude_m[0] - field_m).abs() < 1.0);
        assert!(
            (altitudes[altitudes.len() - 1]
                - (field_m + alas_config::mission::TAKEOFF_ACCELERATION_HEIGHT_M))
                .abs()
                < 1.0
        );
        let speeds = &first.conditions.velocity_m_s;
        assert!(
            (speeds[0] - speeds[speeds.len() - 1]).abs() < 0.5,
            "one speed: V2"
        );
    }
    let mut scheduled_tags: Vec<&str> = Vec::new();
    let mut scheduled_start_m: Vec<f64> = Vec::new();
    // The start of an entry piece (`<tag>_path_entry`), which belongs to the
    // scheduled leg that follows it.
    let mut entry_start_m: Option<f64> = None;
    for segment in &mission.segments {
        let tag = segment.spec.tag.as_str();
        let first_altitude_m = segment
            .conditions
            .altitude_m
            .first()
            .copied()
            .unwrap_or(f64::NAN);
        // The level speed-change legs the schedule inserts between legs are not
        // part of the SUAVE reference schedule.
        if tag.starts_with("speed_change_to_") || tag == "takeoff_climb" {
            continue;
        }
        if tag.ends_with(PATH_ENTRY_TAG_SUFFIX) {
            entry_start_m = Some(first_altitude_m);
            continue;
        }
        // A split of a piece that already exists is numbered (`<tag>_upper_2`).
        let mut parent = tag;
        loop {
            let numbered = parent.trim_end_matches(|c: char| c.is_ascii_digit());
            let stem = match numbered.strip_suffix('_') {
                Some(stem) if PIECE_TAG_SUFFIXES.iter().any(|s| stem.ends_with(s)) => stem,
                _ => parent,
            };
            let stripped = PIECE_TAG_SUFFIXES
                .iter()
                .find_map(|suffix| stem.strip_suffix(suffix))
                .unwrap_or(stem);
            if stripped.len() == parent.len() {
                break;
            }
            parent = stripped;
        }
        if parent.len() == tag.len() {
            scheduled_tags.push(tag);
            scheduled_start_m.push(entry_start_m.take().unwrap_or(first_altitude_m));
            continue;
        }
        assert_eq!(
            scheduled_tags.last().copied(),
            Some(parent),
            "piece {tag} must directly follow the pieces of its scheduled leg"
        );
    }
    assert_eq!(fixture.segments.len(), fixture.solver.segment_count);
    // The levels the cruise is flown at: the start of the first cruise leg, and of the last one
    // (the top of the ladder, the level the priced route reaches).
    let cruise_start_m = |leg: &str| {
        scheduled_tags
            .iter()
            .position(|tag| *tag == leg)
            .map(|index| scheduled_start_m[index])
            .unwrap_or(f64::NAN)
    };
    let first_level_m = cruise_start_m("cruise_step_1");
    let top_level_m = cruise_start_m("cruise_step_3");
    // The reference ladder (FL390) is not a level this vehicle can hold at mission weight:
    // with the policy `Declared` the priced sizing closure did not converge
    // (`NotConverged { last_change_kg: 17409 }`; a declared level
    // the aircraft cannot reach is flown at the reachable level and reported), so the product
    // mission keeps the default policy and the native ladder is scaled to the level the
    // priced route reaches. The unscaled replay of the reference ladder is the frozen
    // compatibility path (`suave_boundary`), which never applies this cap.
    // Pinned by probe: the priced route's level for this mission under the 300 ft/min
    // residual-climb criterion, read at the top-of-climb mass (0.985 x the take-off mass,
    // `TOC_MASS_FRACTION`). The level is a continuous root interpolated from a 1 m scan
    // (`LEVEL_RESOLUTION_M`), so a drift of about 1 mm (1e-7 of the level) is mass
    // bookkeeping, not a model change; the tolerance is 5 cm, far below the resolution of the
    // level and far above that drift. The criterion binds on the 300 ft/min residual at the
    // cruise speed and T - D is nearly flat in altitude near 9 km, so the level is sensitive
    // to the trimmed drag and to the take-off mass: it moves with the viscous margin 1.035
    // (Raymer Table 12.8), the stretched trim nodes (trimmed induced drag raised by the
    // compressible tail download), the tail dynamic-pressure ratio 0.78 in the lattice, the
    // product parasite items (pylons, wing-body fairing, upsweep, base; Raymer 6th ed.
    // Eqs. 12.36-12.38, Torenbeek 2013 App. A.4-A.5), the sourced Jenkinson net idle thrust
    // and idle-chart fuel floor, the priced level deceleration at the 10,000 ft restriction,
    // the class cabin section of a strict single-deck body (payload and take-off mass through
    // the container count of the hold) and the container volume limit (an AKE holds 4.5 m3 at
    // the ICAO 161 kg/m3, 724.5 kg net, so the 8,400 kg of checked bags ride in 12 containers
    // with 984 kg of tare; `hold_demand_conserved.rs`). The level is PROVISIONAL: a GE9X
    // widebody at Mach 0.84 is not plausibly limited to this level (reference 11,887 m, real
    // initial cruise about FL310-350); it follows the rigid neutral point about 17 points too
    // far aft and the trimmed drag that implies.
    //
    // The ceiling is the RANGE-STEP VC = VD / 1.25 (this vehicle declares no VMO, so the VD
    // VMO branch is not involved). The approach rung does not move this mission (its arrival
    // field, 1,624 m, has the 6,500 ft step).
    //
    // Mission legs: N cruise legs are N-1 step climbs of 2,000 ft = 609.6 m (`schedule::Reach`
    // lowered to real steps). The priced route's level is the TOP of the ladder, flown by the
    // last cruise leg (`cruise_step_3`), and the first cruise leg flies two steps below it. So
    // the first-leg level is not a ceiling: the vehicle holds the top level on the 300 ft/min
    // criterion (itself PROVISIONAL, see above) and the configured three legs step up to it.
    // The three legs do not merge into one run: their levels differ, so each is its own run
    // with its step climb between (`standard_mission_phases.rs` pins the ladder generically).
    //
    // Legal flight levels: the priced route plans its level at the ICAO cruising level at or
    // below its capability (`flight_levels::legal_level_at_or_below_m`, ICAO Annex 2, App. 3,
    // table a), so a continuous capability near 7,075 m (FL232) becomes FL230, 7,010.4 m, and
    // the ladder below it 6,400.8 / 5,791.2 m. The capability moves by metres with the mass the
    // lower level burns, far from the FL230/FL240 boundaries (7,010.4 / 7,315.2 m).
    assert!(
        (top_level_m - 7_010.4_f64).abs() < 0.05,
        "the top cruise level is {top_level_m} m"
    );
    // Real steps of 2,000 ft below the top, one per step climb (`schedule/reach.rs`).
    const STEP_M: f64 = 2000.0 * 0.3048;
    assert!(
        (first_level_m - (top_level_m - 2.0 * STEP_M)).abs() < 1.0e-6,
        "the first cruise level is {first_level_m} m, two steps below {top_level_m} m"
    );
    // The two step-climb rungs and the three cruise legs follow the configured ladder under the
    // reachable top (`schedule/reach.rs::levels`): cruise leg, step climb, cruise leg, step climb,
    // cruise leg. Each step climb starts at the level of the leg before it. The start altitudes
    // are continuous in the level, hence the 5 cm tolerance of the level pin.
    const PRICED_PLAN: [(&str, f64); 5] = [
        ("cruise_step_1", 5_791.2),
        ("step_climb_1", 5_791.2),
        ("cruise_step_2", 6_400.8),
        ("step_climb_2", 6_400.8),
        ("cruise_step_3", 7_010.4),
    ];
    let is_priced_plan = |tag: &str| PRICED_PLAN.iter().any(|(name, _)| *name == tag);
    // A descent rung whose floor altitude lies at or above the level the vehicle flies has no
    // descent to fly: the reference rung (descent 1 ends at 30,000 ft) is omitted when the priced
    // route's level is below it. Only such rungs may be missing from the flown ladder.
    let rung_floor_m = |tag: &str| -> Option<f64> {
        let profile = &config.mission.profile;
        let floor_ft = match tag {
            "descent_1" => profile.descent_1_altitude_ft,
            "descent_2" => profile.descent_2_altitude_ft,
            "descent_3" => profile.descent_3_altitude_ft,
            "descent_4" => profile.descent_4_altitude_ft,
            _ => return None,
        };
        Some(floor_ft * 0.3048)
    };
    // The omission rule reads the profile floors, so they must be the fixture's own rung starts:
    // rung n+1 starts where rung n ends.
    for (rung, floor_tag) in [
        ("descent_2", "descent_1"),
        ("descent_3", "descent_2"),
        ("descent_4", "descent_3"),
    ] {
        let start_m = fixture
            .segments
            .iter()
            .find(|segment| segment.tag == rung)
            .map(|segment| segment.selected_points["0"].altitude_m);
        assert!(
            start_m
                .zip(rung_floor_m(floor_tag))
                .is_some_and(|(a, b)| (a - b).abs() < 1.0e-3),
            "{rung} must start at the floor of {floor_tag}"
        );
    }
    // A descent rung whose floor lies at or above the top level has no descent to fly.
    let omitted = |tag: &str| rung_floor_m(tag).is_some_and(|floor_m| floor_m >= top_level_m);
    assert_eq!(
        scheduled_tags.len(),
        fixture
            .segments
            .iter()
            .filter(|segment| !omitted(&segment.tag))
            .count(),
        "{scheduled_tags:?}"
    );
    let flown_plan: Vec<&str> = scheduled_tags
        .iter()
        .copied()
        .filter(|tag| is_priced_plan(tag))
        .collect();
    let pinned_plan: Vec<&str> = PRICED_PLAN.iter().map(|(name, _)| *name).collect();
    assert_eq!(flown_plan, pinned_plan, "priced-plan leg order");
    // Every other leg keeps its place in the reference order.
    let flown_rest: Vec<&str> = scheduled_tags
        .iter()
        .copied()
        .filter(|tag| !is_priced_plan(tag))
        .collect();
    let reference_rest: Vec<&str> = fixture
        .segments
        .iter()
        .map(|s| s.tag.as_str())
        .filter(|tag| !is_priced_plan(tag) && !omitted(tag))
        .collect();
    assert_eq!(flown_rest, reference_rest, "scheduled segment order");
    // And the two sequences interleave as probed: the initial climb to the first level, then a
    // cruise leg and a step climb alternating up the ladder, then the descent.
    let interleaved: Vec<&str> = [
        "takeoff",
        "initial_climb",
        "cruise_step_1",
        "step_climb_1",
        "cruise_step_2",
        "step_climb_2",
        "cruise_step_3",
        "descent_1",
        "descent_2",
        "descent_3",
        "descent_4",
        "final_landing",
    ]
    .into_iter()
    .filter(|tag| !omitted(tag))
    .collect();
    assert_eq!(scheduled_tags, interleaved, "interleaved leg order");
    let level_scale = top_level_m / 11_887.2;
    for (tag, start_m) in scheduled_tags.iter().zip(&scheduled_start_m) {
        if let Some((_, pinned_m)) = PRICED_PLAN.iter().find(|(name, _)| name == tag) {
            assert!(
                (start_m - pinned_m).abs() < 0.05,
                "{tag} starts at {start_m} m, pinned {pinned_m} m"
            );
            continue;
        }
        let reference = fixture
            .segments
            .iter()
            .find(|reference| reference.tag == *tag)
            .unwrap_or_else(|| panic!("{tag} is a reference leg"));
        let reference_start_m = reference.selected_points["0"].altitude_m;
        // The native flight files the level the priced route reaches with a residual
        // maximum-climb rate of 300 ft/min at top-of-climb mass (Jenkinson, Civil Jet
        // Aircraft Design, Ch. 10 pp. 238-239), not the SUAVE reference level: the
        // heavy long-range mission cannot hold FL390 on that criterion. The first
        // descent rung starts on that level, so it scales with it; the others (take-off,
        // initial climb, the lower descent rungs) keep the reference altitudes.
        // The SUAVE reference departs Madrid at 610 m; the airport table carries the
        // AIP Spain aerodrome elevation, 609 m (AD 2.3, AIRAC 09/26).
        let elevation_m = get_airport("LEMD")
            .unwrap_or_else(|error| panic!("LEMD: {error}"))
            .elevation_m;
        let expected_start_m = if *tag == "descent_1" {
            reference_start_m * level_scale
        } else if *tag == "takeoff" {
            // the take-off band opens at the acceleration height: the climb at V2 (leg
            // `takeoff_climb`, not in the reference) flew the 1,000 ft below it
            elevation_m + alas_config::mission::TAKEOFF_ACCELERATION_HEIGHT_M
        } else if [610.0, 3658.0]
            .iter()
            .any(|&level| (reference_start_m - level).abs() < 1.0e-9)
        {
            // the initial climb, which starts 3,048 m (10,000 ft) above the field (the take-off
            // is handled above): the SUAVE reference departs Madrid at 610 m, the table carries
            // the AIP value
            reference_start_m - (610.0 - elevation_m)
        } else {
            // A rung that starts above the flown level starts at that level.
            reference_start_m.min(if rung_floor_m(tag).is_some() {
                top_level_m
            } else {
                f64::INFINITY
            })
        };
        assert!(
            (start_m - expected_start_m).abs() < 1.0e-3,
            "{tag} starts at {start_m} m, the SUAVE reference at {reference_start_m} m"
        );
    }
    assert!(mission.solutions.iter().all(|solution| solution.converged));
    assert_eq!(
        mission.segments[0].conditions.len(),
        fixture.solver.control_points[0]
    );
    // A flown command lies between the engine's flight-idle net force (a negative fraction of
    // the maximum-climb force where the idle net force is a drag) and the ceiling of one (the
    // maximum-climb force, or on the take-off acceleration the take-off rating at the point).
    assert!(mission.segments.iter().all(|segment| {
        let ceiling = 1.0 + 1.0e-9;
        segment
            .throttle
            .iter()
            .zip(&segment.conditions.available_throttle_floor)
            .all(|(throttle, floor)| {
                throttle.is_finite() && *throttle >= floor.min(0.0) - 1.0e-9 && *throttle <= ceiling
            })
    }));
    assert!(mission.initial_mass_kg().is_finite());
    assert!(mission.final_mass_kg().is_finite());
    assert!(mission.fuel_burned_kg().is_finite() && mission.fuel_burned_kg() > 0.0);
    assert!(mission.final_mass_kg() < mission.initial_mass_kg());
    let final_range_m = mission
        .segments
        .last()
        .and_then(|segment| segment.conditions.aircraft_range_m.last())
        .copied()
        .unwrap_or(f64::NAN);
    assert!(
        (final_range_m - fixture.inputs.route_distance_m).abs() < 1.0e-3,
        "mission route closure: final range {final_range_m}, requested {}",
        fixture.inputs.route_distance_m
    );
}
