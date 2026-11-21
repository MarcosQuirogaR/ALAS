// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the plan cases share: configurations, runs, stage builders, and the
//! checks every flown case passes (completion, closed fuel, mass ledger,
//! plausibility-tier mission rules, fuel flow against the typical SFC bands).

use alas_acceptance::plausibility::mission::check_mission;
use alas_acceptance::plausibility::mission_facts::MissionFacts;
use alas_acceptance::plausibility::{Findings, Severity};
use alas_config::airports::get as get_airport;
use alas_config::{AlasConfig, CruiseExtent, MissionPlan, PlanStage, StageKind};
use alas_exec::RunEnvironment;
use alas_pipeline::{
    DesignPipeline, LoadCaseSelection, MissionHistory, PipelineOptions, PipelineResult,
    PlanClosure, PlanLoadCase,
};
use alas_route::route::{Route, RouteSource};

/// The trim-load defect is open: the trimmed induced
/// drag still RISES as the centre of gravity moves aft and a trim tank holding
/// an aft band costs fuel (C6). Rerun (tail at its ratio 0.78
/// in the lattice, A1G table): held 90,304 kg against free 90,076 kg of flight
/// burn (+228 kg, +0.25 %); the drag-against-CG curve is flat (+-0.6 counts over
/// +-0.4 m of the reference station) with its minimum 0.3 m aft of the
/// reference station (about 1 m ahead of it before): the wing-body moment error
/// still sets the sign. Set this to `false`
/// when the defect closes, which makes C6 require the saving (`held_burn < free_burn`).
pub const TRIM_LOAD_OPEN: bool = true;

pub fn config_for_preset(name: &str) -> AlasConfig {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    config
}

pub fn run(config: AlasConfig) -> PipelineResult {
    run_with_payload(config, None)
}

/// [`run`] without panicking on a plan the engine refuses: its message.
pub fn try_run(config: AlasConfig) -> Result<PipelineResult, String> {
    try_run_with_payload(config, None)
}

/// [`run`] boarding `payload_kg` instead of the cabin's capacity (the operator's
/// payload-limited dispatch).
pub fn run_with_payload(config: AlasConfig, payload_kg: Option<f64>) -> PipelineResult {
    try_run_with_payload(config, payload_kg).unwrap_or_else(|error| panic!("pipeline: {error}"))
}

fn try_run_with_payload(
    config: AlasConfig,
    payload_kg: Option<f64>,
) -> Result<PipelineResult, String> {
    let origin = get_airport(&config.departure_airport).unwrap();
    let destination = get_airport(&config.arrival_airport).unwrap();
    let mut route = Route::great_circle(
        origin,
        destination,
        config.mission.great_circle_points as usize,
    );
    route.source = RouteSource::SimbriefApi;
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
    let pipeline = match payload_kg {
        Some(kg) => DesignPipeline::new(config).with_mission_payload_override_kg(kg),
        None => DesignPipeline::new(config),
    };
    pipeline
        .run_with_environment_and_route(&options, &RunEnvironment::default(), Some(route))
        .map_err(|error| error.to_string())
}

pub fn stage(id: &str, kind: StageKind) -> PlanStage {
    PlanStage::new(id, kind)
}

pub fn cruise_km(km: f64) -> StageKind {
    StageKind::Cruise {
        extent: CruiseExtent::DistanceKm(km),
        altitude_ft: None,
        agl: false,
        speed_m_s: None,
    }
}

pub fn loiter_min(minutes: f64, altitude_ft: Option<f64>) -> StageKind {
    StageKind::Loiter {
        time_min: minutes,
        altitude_ft,
        agl: altitude_ft.is_some(),
        speed_m_s: None,
    }
}

pub fn climb(id: &str, altitude_ft: f64) -> PlanStage {
    stage(
        id,
        StageKind::Climb {
            altitude_ft,
            agl: false,
            rate_m_s: None,
        },
    )
}

pub fn descent(id: &str, altitude_ft: f64, agl: bool) -> PlanStage {
    stage(id, StageKind::Descent { altitude_ft, agl })
}

pub fn landing() -> PlanStage {
    stage(
        "landing",
        StageKind::Landing {
            field_elevation_ft: None,
        },
    )
}

pub fn takeoff() -> PlanStage {
    stage("takeoff", StageKind::Takeoff { field: None })
}

/// The reserves of the fuel policy as stages, the go-around at the profile's
/// approach height.
pub fn with_reserves(plan: MissionPlan, config: &AlasConfig, alternate_ft: f64) -> MissionPlan {
    plan.with_policy_reserves(
        &config.fuel_policy,
        alternate_ft,
        config.mission.profile.descent_4_altitude_ft,
    )
}

pub fn case_of(result: &PipelineResult) -> &PlanLoadCase {
    let load_case = result.mission_load_case.as_ref().unwrap();
    let LoadCaseSelection::Plan(case) = &load_case.selection else {
        panic!("a plan load case");
    };
    case
}

pub fn history_of(result: &PipelineResult) -> &MissionHistory {
    result
        .feasibility
        .mass_balance
        .as_ref()
        .and_then(|balance| balance.history.as_ref())
        .unwrap_or_else(|| {
            for finding in result.feasibility.findings.iter().take(12) {
                println!("FINDING {:?} {}", finding.code, finding.message);
            }
            let Some(mission) = result.mission_result.as_ref() else {
                panic!("no mission result at all");
            };
            panic!(
                "no mass history: mission {:?}; native error {:?}",
                mission.completion_refusal(),
                result
                    .mission_load_case
                    .as_ref()
                    .and_then(|l| l.native_error.clone()),
            )
        })
}

/// The stage and event tables of a flown plan, as text.
pub fn print_tables(result: &PipelineResult) {
    let case = case_of(result);
    let history = history_of(result);
    for stage in &history.stages {
        println!(
            "{:<18} {:<10} fuel {:>7.0} kg  t {:>7.0} s  d {:>8.1} km  m {:>8.0} -> {:>8.0}  cg end {:>6.2} % MAC [{:>6.2}, {:>6.2}]  in envelope {}",
            stage.id, stage.label, stage.fuel_kg, stage.time_s, stage.distance_m / 1000.0,
            stage.mass_start_kg, stage.mass_end_kg, stage.cg_end_pct_mac,
            stage.cg_range_pct_mac[0], stage.cg_range_pct_mac[1], stage.within_envelope
        );
    }
    for event in &history.events {
        println!(
            "event in stage {} ({:?}): {}  m {:.0} -> {:.0} kg  cg {:.2} -> {:.2} % MAC  in envelope {}",
            event.stage, event.at, event.description, event.mass_before_kg, event.mass_after_kg,
            event.cg_before_pct_mac, event.cg_after_pct_mac, event.within_envelope
        );
    }
    for note in &history.notes {
        println!("note: {note}");
    }
    println!(
        "ramp fuel {:.0} kg ({:.0} kg at brake release), burned in flight {:.0} kg, taxi {:.0} kg, contingency {:.0} kg: {:?} in {} flights; trim drag {:?} (max {:.1} counts)",
        case.ramp_fuel_kg, case.brake_release_fuel_kg(), case.flight_burn_kg, case.taxi_fuel_kg,
        case.contingency_kg, case.closure, case.iterations, case.trim_drag.status,
        1.0e4 * case.trim_drag.max_increment_cd
    );
    if !case.trim_drag.curve.is_empty() {
        let curve: Vec<String> = case
            .trim_drag
            .curve
            .iter()
            .map(|[x, cd]| format!("x {x:.2} m {:+.2} counts", 1.0e4 * cd))
            .collect();
        println!(
            "trim drag against the centre of gravity (reference x {:.2} m): {}",
            case.trim_drag.reference_cg_x_m,
            curve.join(", ")
        );
    }
}

/// The envelope verdict of the flight, as the findings state it.
pub fn print_findings(result: &PipelineResult) {
    use alas_pipeline::FindingCode as Code;
    for finding in result.feasibility.findings.iter().filter(|finding| {
        matches!(
            finding.code,
            Code::CgEnvelopeViolation
                | Code::MissionPlanOperationFailed
                | Code::MissionPlanMassLimitViolation
                | Code::DispatchNotConverged
                | Code::TrimUnavailable
                | Code::MissionPlanTrimUnsettled
                | Code::PayloadAutoBalanceIncomplete
                | Code::FieldTakeoffDistanceViolation
        )
    }) {
        println!("FINDING {:?} {}", finding.code, finding.message);
    }
}

/// Whether `findings` holds one of `code`.
pub fn has_finding(result: &PipelineResult, code: alas_pipeline::FindingCode) -> bool {
    result
        .feasibility
        .findings
        .iter()
        .any(|finding| finding.code == code)
}

/// The centre of gravity, m, after `mass_kg` at `position_m` leaves a state of
/// `mass_before_kg` at `cg_before_m`: the first moment, nothing else.
pub fn cg_after_release(
    mass_before_kg: f64,
    cg_before_m: f64,
    mass_kg: f64,
    position_m: f64,
) -> f64 {
    (mass_before_kg * cg_before_m - mass_kg * position_m) / (mass_before_kg - mass_kg)
}

/// The history point just before the one an event made, and that one.
pub fn around_event(history: &MissionHistory, label_part: &str) -> (usize, usize) {
    let after = history
        .points
        .iter()
        .position(|point| point.label.contains(label_part))
        .unwrap_or_else(|| panic!("no history point for {label_part}"));
    (after - 1, after)
}

/// The plan flew to its end, closed on its fuel, and the ledger and the flight
/// agree on mass.
pub fn assert_complete_and_closed(result: &PipelineResult) {
    assert_flown_and_closed(result);
    let case = case_of(result);
    // Fuel per stage adds up to what the closure says was burned.
    let history = history_of(result);
    let stage_fuel: f64 = history.stages.iter().map(|stage| stage.fuel_kg).sum();
    assert!(
        (stage_fuel - (case.flight_burn_kg + case.taxi_fuel_kg)).abs() < 1.0e-6,
        "stage fuel {stage_fuel} against burn {} + taxi {}",
        case.flight_burn_kg,
        case.taxi_fuel_kg
    );
    assert!(
        history.max_mass_mismatch_kg < 1.0,
        "the ledger and the flight agree on mass"
    );
    // The mass the flight starts at is the zero-fuel mass and the ramp fuel
    // less the taxi fuel (and what the plan hangs on at the start).
    let load_case = result.mission_load_case.as_ref().unwrap();
    assert!(
        (load_case.takeoff_mass_kg
            - (load_case.zero_fuel_mass_kg + case.ramp_fuel_kg - case.taxi_before_takeoff_kg))
            .abs()
            < 1.0e-6
                + history
                    .events
                    .iter()
                    .filter(|event| event.description.starts_with("store"))
                    .map(|event| event.mass_after_kg - event.mass_before_kg)
                    .sum::<f64>()
    );
    // No surfaced error finding about the plan's own operations.
    assert!(
        !has_finding(
            result,
            alas_pipeline::FindingCode::MissionPlanOperationFailed
        ),
        "{:?}",
        history.notes
    );
}

/// The plan flew to its end and closed on its fuel, which the tanks and the
/// take-off mass limit hold.
pub fn assert_flown_and_closed(result: &PipelineResult) {
    let mission = result.mission_result.as_ref().unwrap();
    assert!(
        mission.completed_summary().is_some(),
        "{:?}",
        mission.completion_refusal()
    );
    let case = case_of(result);
    assert_eq!(case.closure, PlanClosure::Closed);
    assert_eq!(
        case.shortfall_kg, 0.0,
        "the tanks and MTOW hold the plan's fuel"
    );
}

/// The mean added drag of the legs of stage `index` as a flat-plate area, m2
/// (the added drag coefficient on the reference area; zero for a stage without
/// legs), without the trim drag of the centre of gravity the legs fly at (which
/// the load case reports apart): it holds the drag of what is carried or open
/// and the configuration drag of the flap and gear legs.
pub fn added_area_of_stage_m2(result: &PipelineResult, index: usize) -> f64 {
    let s_ref = result
        .baseline_report
        .as_ref()
        .and_then(|report| report.airplane.as_ref())
        .map(|airplane| airplane.s_ref)
        .expect("the analysed aircraft");
    added_cd_of_stage(result, index) * s_ref
}

/// The trim-CL defect is open: the trim increment of a leg
/// is read from the clean cruise-lattice table at the leg's own lift coefficient,
/// whatever that is, and the table was checked only from 0 to CL_max of the clean
/// wing; the pitching-moment change of the flaps on take-off and approach legs is
/// not in it. While `true` legs above the clean CL_max are allowed to carry an
/// increment; set it to `false` when the table is built or
/// bounded over the flown range, and every leg that carries an increment must then
/// be inside the range the table covers (the clean CL_max).
pub const TRIM_CL_OPEN: bool = true;

/// The limit of the increment of any leg, drag coefficient (five counts of 1e-4; a
/// count is 1e-4 of the drag coefficient): a runaway detector set from the worst
/// leg the cases measure, 3.95 counts on the A400M in C3 (descent leg
/// `descent_4_2@08` at a mean C_L of 0.97, INSIDE the clean wing's range of 1.50:
/// the increment grows with C_L squared and with the centre-of-gravity shift, and
/// the cruise-C_L curve printed beside it, 1.4 counts at the most forward station,
/// is smaller only because it is read at the cruise C_L). No source gives a
/// trim-drag share of cruise drag. C6 measures 1.3 counts, C4 0.5.
const LIMIT_CD: f64 = 5.0e-4;

/// Bound on the trim drag of the centre of gravity the plan flies at. Prints the
/// worst leg (tag, mean lift coefficient, the clean wing's CL_max, increment) and
/// the curve of the plan (increment against station at the cruise lift
/// coefficient). Fails an increment over [`LIMIT_CD`] and, once [`TRIM_CL_OPEN`]
/// is closed, any leg beyond the table's range that carries one.
pub fn assert_trim_bounded(result: &PipelineResult) {
    let trim = &case_of(result).trim_drag;
    println!(
        "max trim increment {:.2} counts; reference station {:.2} m; curve [station m, counts] {:?}",
        trim.max_increment_cd * 1.0e4,
        trim.reference_cg_x_m,
        trim.curve
            .iter()
            .map(|p| [(p[0] * 100.0).round() / 100.0, (p[1] * 1.0e5).round() / 10.0])
            .collect::<Vec<_>>()
    );
    let cl_max_clean = result.config.performance.cl_max_clean;
    let legs: Vec<(f64, f64, String)> = result
        .mission_result
        .as_ref()
        .unwrap()
        .segments
        .iter()
        .map(|s| {
            let cl = &s.conditions.lift_coefficient;
            let mean = cl.iter().sum::<f64>() / cl.len().max(1) as f64;
            (trim.increment_cd(&s.spec.tag), mean, s.spec.tag.clone())
        })
        .collect();
    if let Some((increment, mean_cl, tag)) =
        legs.iter().max_by(|a, b| a.0.abs().total_cmp(&b.0.abs()))
    {
        println!(
            "worst leg {tag}: increment {:.2} counts at mean C_L {mean_cl:.2} (clean CL_max {cl_max_clean:.2}{})",
            increment * 1.0e4,
            if *mean_cl > cl_max_clean { ", ABOVE the clean table's range" } else { "" }
        );
    }
    assert!(
        trim.max_increment_cd < LIMIT_CD,
        "trim drag increment of {:.1} counts against a limit of {:.1}",
        trim.max_increment_cd * 1.0e4,
        LIMIT_CD * 1.0e4
    );
    if !TRIM_CL_OPEN {
        for (increment, mean_cl, tag) in &legs {
            assert!(
                *increment == 0.0 || *mean_cl <= cl_max_clean,
                "{tag}: an increment of {:.2} counts at C_L {mean_cl:.2} beyond the table's range {cl_max_clean:.2}",
                increment * 1.0e4
            );
        }
    }
}

/// The mean added drag coefficient of the legs of stage `index` (zero for a
/// stage without legs), less the trim drag the legs carry: what is left is the
/// drag of what is carried, what is open and the flap and gear.
pub fn added_cd_of_stage(result: &PipelineResult, index: usize) -> f64 {
    let trim = &case_of(result).trim_drag;
    let suffix = format!("@{index:02}");
    let mission = result.mission_result.as_ref().unwrap();
    let legs: Vec<_> = mission
        .segments
        .iter()
        .filter(|segment| {
            segment.spec.tag.contains(&suffix) && !segment.spec.tag.starts_with("speed_change")
        })
        .collect();
    if legs.is_empty() {
        return 0.0;
    }
    legs.iter()
        .map(|s| s.spec.configuration_delta_cd - trim.increment_cd(&s.spec.tag))
        .sum::<f64>()
        / legs.len() as f64
}

/// Whether the registry of open plausibility findings lists `rule` (any of
/// its kinds) for `aircraft`: a case that an open defect of the aircraft
/// itself keeps from a verdict.
pub fn known_defect(aircraft: &str, rule: &str) -> bool {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/data/plausibility_known.tsv");
    std::fs::read_to_string(path).is_ok_and(|text| {
        text.lines()
            .filter(|line| !line.starts_with('#'))
            .any(|line| {
                let mut fields = line.split('\t');
                fields.next() == Some(aircraft)
                    && fields.next().and_then(|key| key.split('/').next()) == Some(rule)
            })
    })
}

/// The plausibility-tier mission rules (`MISSION-*`) on the flown plan. The mass
/// of a plan changes at its events, so the continuity of mass across a
/// boundary is judged against the events instead; every other finding of
/// severity `High` fails the case.
///
/// `owned` lists rules whose `High` finding is an open defect of the aircraft's
/// own model, listed for correction: a temporary ratchet like the registry of the
/// plausibility tier, so the finding must still be there (once the fix lands the
/// case fails until the entry is removed).
pub fn assert_plausible(
    case: &str,
    result: &PipelineResult,
    design_mach: f64,
    owned: &[&'static str],
) {
    let mission = result.mission_result.as_ref().unwrap();
    let facts = MissionFacts::from_result(mission, design_mach);
    // Every rule sees every leg. Only MISSION-DESCENT-RATE is judged without the
    // final approach (the last descent rung, the approach and the landing legs):
    // those fly a 3 degree glide path at approach speed, 650-800 ft/min by
    // geometry (V sin 3 deg at 72-80 m/s), which is not one of the
    // cruise-to-approach descents that rule bands. The standard mission never
    // has them alone (they merge with the descent before); a plan that holds
    // near the field does. The path-angle rule still sees them.
    let mut cruise_to_approach = MissionFacts::from_result(mission, design_mach);
    cruise_to_approach.segments.retain(|segment| {
        !["descent_4", "final_landing", "approach"]
            .iter()
            .any(|prefix| segment.tag.starts_with(prefix))
    });
    cruise_to_approach.scheduled = cruise_to_approach.segments.len();
    let mut all_rules = Findings::new(case);
    check_mission(&mut all_rules, &facts);
    let mut descent_rate = Findings::new(case);
    check_mission(&mut descent_rate, &cruise_to_approach);
    let findings: Vec<_> = all_rules
        .items()
        .iter()
        .filter(|f| f.rule != "MISSION-DESCENT-RATE")
        .chain(
            descent_rate
                .items()
                .iter()
                .filter(|f| f.rule == "MISSION-DESCENT-RATE"),
        )
        .collect();
    for finding in &findings {
        println!("{finding}");
        let allowed = finding.severity != Severity::High || owned.contains(&finding.rule);
        if !allowed {
            print_legs(result);
        }
        assert!(allowed, "plausibility: {finding}");
    }
    for rule in owned {
        assert!(
            findings
                .iter()
                .any(|f| f.rule == *rule && f.severity == Severity::High),
            "{rule} no longer fires on {case}: remove it from the owned list"
        );
    }
}

/// Specific fuel consumption implied by each level stage (cruise or hold at
/// one load factor) of the flight, against Raymer's typical values,
/// *Aircraft Design: A Conceptual Approach*, 6th ed., Table 3.3 (p. 36, jets,
/// 1/h: turbofan 0.4-0.9 across cruise and loiter) and Table 3.4 (p. 37,
/// turboprop 0.4-0.6 lb/hp/h, i.e. 0.068-0.101 mg/(W s), propeller efficiency
/// 0.8, p. 36), each widened by 20 %.
///
/// For a jet, `C = fuel flow * (L/D) / mass` (thrust equals drag, `D = m g / (L/D)`);
/// for a propeller aircraft `C_bhp = fuel flow (L/D) eta_p / (m g V)`.
///
/// `owned` lists stages whose implied value lies outside the band by an open
/// defect of the aircraft's engine model (a temporary ratchet:
/// the stage must still be outside, so the entry goes with the fix).
pub fn assert_level_stage_sfc(result: &PipelineResult, turboprop: bool, owned: &[&str]) {
    const PROPELLER_EFFICIENCY: f64 = 0.8;
    let mission = result.mission_result.as_ref().unwrap();
    let plan = &case_of(result).plan;
    for (index, stage) in plan.stages.iter().enumerate() {
        if !matches!(
            stage.kind,
            StageKind::Cruise { .. } | StageKind::Loiter { .. }
        ) {
            continue;
        }
        let suffix = format!("@{index:02}");
        let (mut fuel, mut time, mut mass_time, mut ld_time, mut speed_time) =
            (0.0, 0.0, 0.0, 0.0, 0.0);
        for segment in mission
            .segments
            .iter()
            .filter(|s| s.spec.tag.contains(&suffix) && !s.spec.tag.starts_with("speed_change"))
        {
            let c = &segment.conditions;
            let (Some(first), Some(last)) = (c.total_mass_kg.first(), c.total_mass_kg.last())
            else {
                continue;
            };
            let dt = c.time_s.last().unwrap() - c.time_s.first().unwrap();
            let n = c.lift_coefficient.len() as f64;
            let ld = c
                .lift_coefficient
                .iter()
                .zip(&c.drag_coefficient)
                .map(|(l, d)| l / d)
                .sum::<f64>()
                / n;
            let speed = c.velocity_m_s.iter().sum::<f64>() / n;
            fuel += first - last;
            time += dt;
            mass_time += 0.5 * (first + last) * dt;
            ld_time += ld * dt;
            speed_time += speed * dt;
        }
        if time < 600.0 {
            continue;
        }
        let (mass, ld, speed) = (mass_time / time, ld_time / time, speed_time / time);
        let flow_kg_s = fuel / time;
        if turboprop {
            let c_bhp = flow_kg_s * ld * PROPELLER_EFFICIENCY / (mass * 9.806_65 * speed);
            let mg_per_ws = c_bhp * 1.0e6;
            println!("stage {}: C_bhp {mg_per_ws:.3} mg/(W s)", stage.id);
            assert_eq!(
                (0.8 * 0.068..=1.2 * 0.101).contains(&mg_per_ws),
                !owned.contains(&stage.id.as_str()),
                "stage {}: implied C_bhp {mg_per_ws:.3} mg/(W s) against 0.8 x 0.068 .. 1.2 x 0.101 (owned: {owned:?})",
                stage.id
            );
        } else {
            let c_per_hour = flow_kg_s * 3600.0 * ld / mass;
            println!("stage {}: C {c_per_hour:.3} 1/h", stage.id);
            assert_eq!(
                (0.8 * 0.4..=1.2 * 0.9).contains(&c_per_hour),
                !owned.contains(&stage.id.as_str()),
                "stage {}: implied C {c_per_hour:.3} 1/h against 0.8 x 0.4 .. 1.2 x 0.9 (owned: {owned:?})",
                stage.id
            );
        }
    }
}

/// The legs of the flight with their time, mass, altitude and speed (a
/// diagnostic for a case that fails a plausibility rule).
pub fn print_legs(result: &PipelineResult) {
    let mission = result.mission_result.as_ref().unwrap();
    for segment in &mission.segments {
        let c = &segment.conditions;
        println!(
            "LEG {:<34} t {:>7.0}..{:>7.0} alt {:>7.0}..{:>7.0} m v {:>6.1}..{:>6.1} m/s rate {:>6.0} ft/min fuel {:>6.1} kg",
            segment.spec.tag,
            c.time_s.first().unwrap(),
            c.time_s.last().unwrap(),
            c.altitude_m.first().unwrap(),
            c.altitude_m.last().unwrap(),
            c.velocity_m_s.first().unwrap(),
            c.velocity_m_s.last().unwrap(),
            (c.altitude_m.last().unwrap() - c.altitude_m.first().unwrap())
                / (c.time_s.last().unwrap() - c.time_s.first().unwrap()).max(1.0e-9)
                / 0.3048
                * 60.0,
            c.total_mass_kg.first().unwrap() - c.total_mass_kg.last().unwrap()
        );
    }
}
