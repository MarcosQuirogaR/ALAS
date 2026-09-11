// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Regression tying the optimizer's search-time feasibility gate to the
//! pipeline's own physical-feasibility assessment for one identical design
//! vector at one identical closed takeoff mass.
//!
//! `alas_opt::assess_product_candidate` (via `mdo::sizing` +
//! `mdo::residuals::build`) is the typed assessment MADS's progressive-barrier
//! search uses to decide whether a candidate is its `best_feasible` incumbent,
//! and `pipeline.rs` re-runs it and hard-errors the whole run if it disagrees
//! at that boundary ("optimized finalist is not hard-feasible on replay").

// This file is a test binary: a failed expect is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! But the pipeline's later physical-feasibility stage
//! (`assess_physical_feasibility_with_load_case`, which produces the
//! `model_cg` finding surfaced in the printed report and JSON export) is a
//! *third*, independently invoked assessment of the same design.
//!
//! Those two used to balance different aircraft. The search placed the lumped
//! mass groups with the frozen reference-compatibility fractions and built its
//! candidate without nacelle bodies; the report placed them on the geometric
//! stations of an aircraft built with engines. For the r5 nominal run's own
//! finalist that was worth 0.56 m of centre of gravity -- 7.2 percent of the
//! mean aerodynamic chord -- which is how a candidate the search accepted as
//! hard-feasible could print as physically INFEASIBLE in its own final report.
//!
//! Both paths now resolve their stations through
//! `alas_mass::product_stations::product_mass_coordinates` and build the same
//! aircraft, so these tests assert the ledger itself agrees item by item, not
//! merely that two pipelines produced the same scalar takeoff mass.
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_pipeline::FullAnalysis;

/// Exact `design_vector` block from the r5 nominal run's own
/// `design_database.json` (`.agent/bench/nominal-r5-claude-20260909/`), not a
/// hand-picked or simplified fixture.
fn r5_finalist_design() -> DesignVector {
    DesignVector {
        span_m: 64.25,
        root_chord_m: 12.125,
        break_chord_m: 7.3,
        tip_chord_m: 1.0,
        sweep_deg: 34.0,
        tip_twist_deg: 1.0,
        wing_x_shift_m: -1.6250000000000004,
        tail_scale: 1.0,
        fuselage_length_m: 76.25000000029104,
        tail_x_shift_m: -2.0,
        airfoil_thickness_scale: 0.8,
        airfoil_camber_scale: 1.2625,
        bump_upper_front: -0.002375,
        bump_upper_rear: -0.0006249999999999997,
        bump_lower_mid: 0.002,
        bump_lower_rear: -0.004,
    }
}

/// Every mass group whose station both paths derive from the built geometry,
/// and which must therefore be identical to round-off. The payload is
/// deliberately absent: its station is the detailed cabin layout's own centre,
/// and the two paths still lay that out against slightly different operating
/// empty masses (see `payload_station_divergence_stays_bounded`).
const GEOMETRY_PLACED_GROUPS: [&str; 9] = [
    "Wing",
    "H-Stab",
    "V-Stab",
    "Fuselage",
    "Gear",
    "Propulsion",
    "Systems",
    "Furnishings",
    "Fuel",
];

struct Replay {
    assessment: alas_opt::CandidateAssessment,
    report: alas_pipeline::AnalysisReport,
}

/// Replay one design vector through both assessors, exactly as `pipeline.rs`
/// does: the search-time gate first, then the finalist report bound to the
/// closed takeoff mass that gate produced.
fn replay(config: &AlasConfig, design: &DesignVector) -> Replay {
    let assessment = alas_opt::assess_product_candidate(config, design)
        .expect("finalist re-evaluates under the same mission-sized objective");
    let report = FullAnalysis::new(config.clone())
        .run_at_sized_takeoff_mass(design, true, assessment.sized.takeoff_mass_kg)
        .expect("finalist report binds to the same closed takeoff mass");
    Replay { assessment, report }
}

#[test]
fn the_search_and_the_report_place_every_geometric_mass_group_identically() {
    let config = AlasConfig::default();
    let design = r5_finalist_design();
    let Replay { assessment, report } = replay(&config, &design);

    assert_eq!(
        assessment.resolved.provenance.id(),
        "mdo::mda::converge",
        "the resolved state must be the search's own converged closure, not a rebuild"
    );
    assert_eq!(
        assessment.resolved.takeoff_mass_kg,
        assessment.sized.takeoff_mass_kg
    );
    assert!(
        (assessment.resolved.mac_m - report.airplane.c_ref).abs() < 1.0e-9,
        "search mac {} vs report mac {}",
        assessment.resolved.mac_m,
        report.airplane.c_ref
    );

    for ((name, _), (_, search_station)) in assessment
        .resolved
        .masses
        .as_pairs()
        .iter()
        .zip(assessment.resolved.coords.as_pairs().iter())
    {
        if !GEOMETRY_PLACED_GROUPS.contains(name) {
            continue;
        }
        let report_station = report
            .mass_coordinates
            .get(*name)
            .copied()
            .unwrap_or_else(|| panic!("report carries a {name} station"));
        for axis in 0..3 {
            assert!(
                (search_station[axis] - report_station[axis]).abs() < 1.0e-6,
                "{name} axis {axis}: the search places it at {} m and the report at {} m -- \
                 both must resolve through \
                 `alas_mass::product_stations::product_mass_coordinates` on the same built \
                 aircraft",
                search_station[axis],
                report_station[axis],
            );
        }
    }
}

#[test]
fn the_search_and_the_report_agree_on_where_the_finalist_balances() {
    let config = AlasConfig::default();
    let design = r5_finalist_design();
    let Replay { assessment, report } = replay(&config, &design);

    // Every item mass, not just the total. The total agrees by construction
    // (the report is bound to the search's own closed takeoff mass), so a
    // scalar check would pass while a heavier wing and a lighter fuel load
    // cancelled inside it -- which is exactly what happened while the two
    // paths published different wing groups.
    //
    // The bound is relative because the fuel item is a closure remainder --
    // `takeoff_mass - operating_empty - payload` -- and the two paths reach
    // that subtraction by summing the same nine items in different orders, so
    // it carries f64 accumulation error proportional to the takeoff mass and
    // nothing else. 1e-7 of the takeoff mass is ~0.02 kg here. For scale, the
    // wing-group defect this assertion was written to catch was 1.2e3 kg:
    // five orders of magnitude above the bound, so this cannot absorb a
    // modelling difference.
    let mass_round_off_kg = 1.0e-7 * assessment.resolved.takeoff_mass_kg;
    for (name, search_mass) in assessment.resolved.masses.as_pairs() {
        let report_mass = report
            .component_masses
            .get(name)
            .copied()
            .unwrap_or_else(|| panic!("report carries a {name} mass"));
        assert!(
            (search_mass - report_mass).abs() <= mass_round_off_kg,
            "{name}: the search sizes {search_mass} kg and the report publishes {report_mass} kg \
             for one identical design vector at one identical closed takeoff mass. Both must \
             build the wing group through `alas_mass::wing_reconciliation` and close the fuel \
             remainder against it.",
        );
    }

    // With the ledger identical item by item, the only input the two still
    // derive separately is the payload station, and its effect on the whole
    // aircraft's centre of gravity is bounded by
    // `payload_mass / takeoff_mass * payload station difference`. That is the
    // bound asserted here -- computed from this run's own numbers rather than
    // a fixed percentage, so it tightens automatically when the payload
    // divergence is closed and cannot hide a new one.
    let payload_station_difference_m =
        (assessment.resolved.coords.payload[0] - report.mass_coordinates["Payload"][0]).abs();
    let payload_cg_budget_m = assessment.resolved.masses.payload
        / assessment.resolved.takeoff_mass_kg
        * payload_station_difference_m;
    let cg_difference_m = (assessment.resolved.cg_x_m - report.physical_cg[0]).abs();
    assert!(
        cg_difference_m <= payload_cg_budget_m + 1.0e-6,
        "search CG {} m and report CG {} m differ by {cg_difference_m} m, more than the \
         {payload_cg_budget_m} m the one remaining separately derived input (the payload \
         station, {payload_station_difference_m} m apart) can account for. Something else in \
         the two ledgers has diverged.",
        assessment.resolved.cg_x_m,
        report.physical_cg[0],
    );
}

/// The neutral point is the one quantity the two deliberately compute at
/// different fidelities, and it is *not* a mass/CG consistency defect.
///
/// The search trims at `analysis.spanwise_resolution`/`chordwise_resolution`
/// once per closed-mass pass, because it does that for every candidate the
/// optimizer touches; the report re-solves `neutral_point` once at
/// `fine_spanwise_resolution`/`fine_chordwise_resolution`. This is a cost
/// trade in the aerodynamic solve with no mass or payload content, so it is
/// pinned separately rather than folded into a blanket CG allowance that
/// could absorb an unclosed mass defect. The report's fine value is the
/// authoritative one; the search's coarse value is what its own gate uses,
/// and this test states how far apart they are allowed to drift before that
/// trade stops being safe.
#[test]
fn the_neutral_point_resolution_difference_is_pinned_and_purely_aerodynamic() {
    let config = AlasConfig::default();
    let design = r5_finalist_design();
    let Replay { assessment, report } = replay(&config, &design);

    let np_difference_pct_mac = 100.0
        * (assessment.resolved.x_neutral_point_m - report.x_neutral_point).abs()
        / assessment.resolved.mac_m;
    assert!(
        np_difference_pct_mac < 1.0,
        "search neutral point {} m (search-resolution trim) and report neutral point {} m \
         (fine-resolution solve) differ by {np_difference_pct_mac}% MAC",
        assessment.resolved.x_neutral_point_m,
        report.x_neutral_point,
    );

    // The forward CG limit is `neutral_point - min_static_margin -
    // cg_range`, so the neutral-point difference passes through to the limit
    // one-for-one and nothing else does. Asserting that equality is what
    // makes this a *resolution* difference rather than an unexplained one:
    // if some other term crept into either limit, this fails even though the
    // bound above still passed.
    let feasibility = alas_pipeline::assess_physical_feasibility(&config, &design, &report, None);
    let model_cg = feasibility
        .model_cg
        .as_ref()
        .expect("model CG assessment is available for the finalist report");
    let search_forward_limit = assessment
        .residuals
        .iter()
        .find(|residual| residual.id == "forward_cg_range")
        .map(|residual| residual.limit)
        .expect("the balance family evaluates the forward CG range");
    let limit_difference_pct_mac =
        (model_cg.configured_forward_limit_pct_mac - search_forward_limit).abs();
    assert!(
        (limit_difference_pct_mac - np_difference_pct_mac).abs() < 1.0e-6,
        "the forward CG limits differ by {limit_difference_pct_mac}% MAC but the neutral points \
         differ by {np_difference_pct_mac}% MAC; the limit difference is no longer explained by \
         the neutral-point resolution alone",
    );
}

#[test]
fn a_hard_feasible_finalist_is_never_reported_physically_infeasible() {
    let config = AlasConfig::default();
    let design = r5_finalist_design();
    let Replay { assessment, report } = replay(&config, &design);

    let feasibility = alas_pipeline::assess_physical_feasibility(&config, &design, &report, None);
    let model_cg = feasibility
        .model_cg
        .as_ref()
        .expect("model CG assessment is available for the finalist report");

    // The production invariant `pipeline.rs` depends on: whatever the search
    // hands over as its winner, the report's own physical-feasibility stage
    // must not contradict on the balance constraints they both evaluate.
    if assessment.hard_feasible {
        assert!(
            model_cg.hard_constraints_pass(),
            "the search accepted this candidate as hard-feasible but the finalist report's own \
             model-CG assessment marks it INFEASIBLE"
        );
    }

    // Non-vacuous in both directions: the two must reach the same verdict on
    // the forward CG range for this vector. The r5 finalist was accepted by
    // the search only because the search balanced a differently placed
    // aircraft; on the unified placement both assessors reject it, which is
    // the correct physical answer and is what this asserts.
    let search_forward_violated = assessment
        .residuals
        .iter()
        .find(|residual| residual.id == "forward_cg_range")
        .map(|residual| residual.normalized_violation > 0.0)
        .expect("the balance family evaluates the forward CG range");
    let report_forward_violated = model_cg.loading_states.iter().any(|state| {
        state.constraints.iter().any(|constraint| {
            constraint.constraint == alas_opt::ModelCgConstraint::ConfiguredForwardCgRange
                && constraint.violated
        })
    });
    assert_eq!(
        search_forward_violated, report_forward_violated,
        "search says forward CG violated = {search_forward_violated}, report says \
         {report_forward_violated} for the same design at the same closed takeoff mass"
    );
}

#[test]
fn payload_station_divergence_stays_bounded() {
    let config = AlasConfig::default();
    let design = r5_finalist_design();
    let Replay { assessment, report } = replay(&config, &design);

    // Known, bounded and deliberately pinned: the search lays the cabin out
    // once, in its first mass pass at the takeoff-mass ceiling, and freezes
    // that summary through the sizing closure, while the report lays it out
    // against the sized operating empty mass. Both seat the same payload; the
    // centres differ by a fraction of a seat pitch. This is the remaining
    // input divergence between the two ledgers and must not grow.
    let search_payload = assessment.resolved.coords.payload;
    let report_payload = report
        .mass_coordinates
        .get("Payload")
        .copied()
        .expect("report carries a payload station");
    assert!(
        (search_payload[0] - report_payload[0]).abs() < 0.25,
        "payload station: search {} m, report {} m",
        search_payload[0],
        report_payload[0],
    );
    assert!(
        (assessment.resolved.masses.payload - report.component_masses["Payload"]).abs() < 1.0e-6,
        "both paths must seat the same payload mass"
    );
}
