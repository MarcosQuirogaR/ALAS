// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the optimizer integration tests share: a preset's registered design,
//! the clean-sheet configuration of an MTOW sizing mode, and the violated
//! hard residual test.

// Each test binary compiles its own copy of this module and uses part of it.
#![allow(dead_code)]
// A test binary's failed expect is the assertion failing.
#![allow(clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, MtowSizing};

mod assessment_view;
// Each test binary imports the part of the shared-assessment API it uses.
#[allow(unused_imports)]
pub use assessment_view::{assess_view, Assessed, AssessmentView};

/// The registered design vector of `preset`.
pub fn nominal(preset: &str) -> DesignVector {
    alas_config::presets::get(preset)
        .expect("registered preset")
        .design_vector
}

/// The default clean-sheet aircraft sized in `sizing`, with the transport
/// planform preferences disabled.
pub fn clean_sheet(sizing: MtowSizing) -> AlasConfig {
    let mut config = AlasConfig::default();
    let optimizer = &mut config.optimizer;
    optimizer.weights.transport_planform_constraints_enabled = false;
    optimizer.objective.mtow_sizing = sizing;
    config
}

/// Whether `id` appears among the assessment's violated hard residuals.
pub fn violates(assessment: &impl Assessed, id: &str) -> bool {
    assessment
        .violated_hard_ids()
        .into_iter()
        .any(|name| name == id)
}

/// A configuration for `preset` in reference-adaptation mode, with a small
/// but honest search budget so a test measures behaviour rather than
/// hardware: the seed is fixed and the stage budgets are evaluation counts.
pub fn reference_search_config(
    preset: &str,
    workers: i64,
    screening_evaluations: i64,
    refinement_evaluations: i64,
) -> AlasConfig {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    config.optimizer.design_space.mode = alas_config::DesignMode::ReferenceAdaptation;
    config.optimizer.solver.seed = Some(7);
    config.optimizer.solver.workers = workers;
    config.optimizer.solver.screening.max_evaluations = screening_evaluations;
    config.optimizer.solver.refinement.max_evaluations = refinement_evaluations;
    config
}

/// What a search must not change when it is spread over more worker threads.
///
/// Every batch is built from the seed before it is evaluated and every score
/// is stored at its own index, so the worker count may change the wall time
/// and nothing else: same winner bits, same history, same per-stage
/// accounting.
pub fn assert_same_search_outcome(
    serial: &alas_opt::OptimizationResult,
    parallel: &alas_opt::OptimizationResult,
    workers: i64,
) {
    assert_eq!(serial.best_design, parallel.best_design, "{workers}");
    assert_eq!(serial.best_cost.to_bits(), parallel.best_cost.to_bits());
    assert_eq!(serial.termination, parallel.termination);
    assert_eq!(
        serial.history.design_vectors,
        parallel.history.design_vectors
    );
    let (a, b) = (
        serial.search_diagnostics.clone().expect("diagnostics"),
        parallel.search_diagnostics.clone().expect("diagnostics"),
    );
    assert_eq!(a.analysis_evaluations, b.analysis_evaluations);
    assert_eq!(a.poll_iterations, b.poll_iterations);
    assert_eq!(
        a.poll_block_size, b.poll_block_size,
        "N_init follows the budget"
    );
    let counts = |d: &alas_opt::SearchDiagnostics| {
        d.stages
            .iter()
            .map(|s| {
                (
                    s.evaluations,
                    s.elite_size,
                    s.termination.clone(),
                    s.sizing_work,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(counts(&a), counts(&b));
    assert_eq!(b.workers, workers as usize);
}

/// ICAO code of the hand-entered aerodrome [`open_ground_departure`] registers.
pub const OPEN_GROUND_ICAO: &str = "ZOGD";

/// Departs `config` from a hand-entered aerodrome whose entered survey states open
/// ground: EDDF's position, elevation, declared distances and runway 25C heading, the
/// terrain flat at the field elevation with no obstacle, the symmetric survey
/// uncertainty of a lidar surface (0.45 m) and the 14 CFR 77.9(c) mobile-object
/// allowance (7.01 m), and no completeness allowance.
///
/// It is a TEST FIXTURE, not physical evidence about EDDF: no curated survey can certify a take-off obstacle clearance, so no
/// registered preset at its own departure has a feasible design. The tests that use
/// it are about the optimizer's search (budgets, replay, determinism, cost ordering),
/// which needs a feasible design; the obstacle check itself is tested in alas-perf and
/// in `mission_sized` on the curated surveys.
pub fn open_ground_departure(config: &mut AlasConfig) {
    use alas_config::airport_io::{
        find_custom, register_custom_airport, AirportProvenance, AirportProvenanceKind,
        CustomAirport,
    };
    use alas_config::airport_obstacles::{departure_surveys, DepartureSurvey};
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(|| {
        if find_custom(OPEN_GROUND_ICAO).is_some() {
            return;
        }
        let eddf = alas_config::airports::get("EDDF").expect("EDDF is curated");
        let heading_deg = departure_surveys("EDDF")
            .expect("EDDF surveys")
            .iter()
            .find(|survey| survey.runway == "25C")
            .expect("EDDF 25C")
            .heading_deg;
        let lateral_centres_m: Vec<f64> = (0..41).map(|i| -600.0 + 30.0 * f64::from(i)).collect();
        let rows = ((40_000.0 + 2.0 * 250.0) / 250.0) as usize;
        let survey = DepartureSurvey {
            runway: "25C".to_owned(),
            heading_deg,
            reference_elevation_m: Some(eddf.elevation_m),
            terrain_source: "test fixture: open ground entered by hand".to_owned(),
            vertical_uncertainty_m: Some(0.45),
            one_sided_bias_m: Some(23.0 * 0.3048),
            completeness_allowance_m: None,
            reference_minus_aerodrome_elevation_m: Some(0.0),
            terrain_s0_m: eddf.toda_m,
            terrain_max_elevation_m: vec![vec![eddf.elevation_m; lateral_centres_m.len()]; rows],
            cell_along_m: 250.0,
            lateral_centres_m,
            cell_lateral_m: 30.0,
            obstacles: Vec::new(),
            provenance: "test fixture".to_owned(),
        };
        register_custom_airport(CustomAirport {
            icao: OPEN_GROUND_ICAO.to_owned(),
            name: "Open-ground test fixture".to_owned(),
            latitude_deg: eddf.latitude_deg,
            longitude_deg: eddf.longitude_deg,
            isa_delta_c: eddf.isa_deviation_c,
            altitude_m: eddf.elevation_m,
            runway_lengths_m: vec![eddf.toda_m],
            declared_toda_m: Some(eddf.toda_m),
            declared_lda_m: Some(eddf.lda_m),
            departure_surveys: vec![survey],
            provenance: AirportProvenance {
                kind: AirportProvenanceKind::UserEntered,
                source: Some("ALAS optimizer test fixture".to_owned()),
                note: None,
            },
        })
        .expect("the fixture aerodrome registers");
    });
    config.departure_airport = OPEN_GROUND_ICAO.to_owned();
}
