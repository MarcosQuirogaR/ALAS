// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Downstream consumers of an optimized run read the pipeline's one sized
//! takeoff mass, never the preset's declared MTOW.
//!
//! One small, seeded product-DE run of the A220-300 preset (the smallest budget
//! that still delivers a hard-feasible finalist within seconds) supplies
//! the report bound to `assessment.sized.takeoff_mass_kg`. Every figure that
//! depends on the takeoff mass (not `config.requirements.mtow_kg`) is checked against a
//! reference drawn at that sized mass explicitly, and against the same figure
//! drawn at the declared MTOW, which must differ when the two masses differ.

// A test asserts on values it constructed, so a failed unwrap is the
// assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_pipeline::{DesignPipeline, PipelineOptions, PipelineResult, RunEnvironment};
use alas_report::families::mass_balance::load_trim::data::load_trim_data_from_pipeline;
use alas_report::families::performance;
use alas_report::svg::render_svg;

const PRESET: &str = "A220-300";

fn run_seeded_a220() -> PipelineResult {
    let mut config =
        AlasConfig::from_value(&serde_json::json!({ "preset": PRESET })).expect("A220 preset");
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    config.optimizer.solver.refinement.max_evaluations = 128;
    config.optimizer.solver.screening.max_evaluations = 8;
    config.optimizer.solver.workers = std::thread::available_parallelism()
        .map_or(1, |n| n.get().min(8))
        .try_into()
        .unwrap_or(1);
    config.structures.enabled = false;
    // The checks are about numbers flowing consistently between stages, not
    // about any aerodynamic value, so the run uses the draft analysis
    // resolution (only its five resolution fields, leaving tuned assumptions
    // alone) and a worker pool; the seeded search is independent of the
    // worker count.
    let draft = &alas_config::fidelity_presets::get("draft")
        .expect("draft fidelity preset")
        .analysis;
    let analysis = &mut config.analysis;
    analysis.sweep_n_points = draft.sweep_n_points;
    analysis.spanwise_resolution = draft.spanwise_resolution;
    analysis.chordwise_resolution = draft.chordwise_resolution;
    analysis.fine_spanwise_resolution = draft.fine_spanwise_resolution;
    analysis.fine_chordwise_resolution = draft.fine_chordwise_resolution;
    let design = alas_config::presets::get(PRESET)
        .expect("A220 preset")
        .design_vector;
    let envelope = config.optimizer.design_space.envelope(&design);
    let bounds: Vec<(f64, f64)> = alas_config::DESIGN_VARIABLE_SPECS
        .iter()
        .map(|spec| {
            let variable = envelope
                .iter()
                .find(|v| v.name == spec.name)
                .expect("every registered design variable has an envelope");
            (variable.lower, variable.upper)
        })
        .collect();
    let options = PipelineOptions {
        optimize: true,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    DesignPipeline::new(config)
        .run_with_design_space(&options, &RunEnvironment::default(), &design, &bounds)
        .expect("the seeded A220-300 finalist is delivered")
}

#[test]
fn figures_and_load_trim_use_the_sized_takeoff_mass_not_the_preset_mtow() {
    let result = run_seeded_a220();
    let report = result.optimized_report.as_ref().expect("optimized report");
    let declared_mtow_kg = result.config.requirements.mtow_kg;
    let sized_kg = report
        .sized_takeoff_mass_kg()
        .expect("a product run binds its report to the sized takeoff mass");
    assert!(sized_kg.is_finite() && sized_kg > 0.0);
    assert!(
        sized_kg <= declared_mtow_kg + 1e-6,
        "the sized mass {sized_kg} kg cannot exceed the declared limit {declared_mtow_kg} kg"
    );
    assert!(
        (declared_mtow_kg - sized_kg).abs() > 1.0,
        "the fixture must size below the preset MTOW ({sized_kg} vs {declared_mtow_kg} kg) or the \
         test cannot tell the two masses apart"
    );

    // The pipeline's other mass result is the same number.
    let analyzed_kg = result.feasibility.fuel_loading.analyzed_takeoff_mass_kg;
    assert!(
        (analyzed_kg - sized_kg).abs() < 0.005 * sized_kg,
        "analysed takeoff mass {analyzed_kg} kg vs sized {sized_kg} kg"
    );

    // Payload-range: the takeoff mass the curve is built on.
    let range = performance::payload_range_data(report, &result.config).expect("payload range");
    assert_eq!(range.mtow_kg, sized_kg);
    assert!(range.mass_is_sized);
    assert_ne!(range.mtow_kg, declared_mtow_kg);

    // Matching chart, arrival/departure field performance: identical to the
    // same figure on a configuration whose ceiling *is* the sized mass, and
    // different from the figure drawn at the declared ceiling.
    let sized_config = result.config.at_closure_mass(sized_kg);
    let mut declared_report = report.clone();
    declared_report
        .geometry_summary
        .remove("analysis_mass_basis_is_sized");
    for (name, sized_svg, config_svg, declared_svg) in [
        (
            "matching",
            render_svg(&performance::figure_matching_chart(
                report,
                &result.config,
                None,
            )),
            render_svg(&performance::figure_matching_chart(
                report,
                &sized_config,
                None,
            )),
            render_svg(&performance::figure_matching_chart(
                &declared_report,
                &result.config,
                None,
            )),
        ),
        (
            "lto_departure",
            render_svg(&performance::figure_lto_departure(
                report,
                &result.config,
                None,
            )),
            render_svg(&performance::figure_lto_departure(
                report,
                &sized_config,
                None,
            )),
            render_svg(&performance::figure_lto_departure(
                &declared_report,
                &result.config,
                None,
            )),
        ),
        (
            "lto_arrival",
            render_svg(&performance::figure_lto_arrival(
                report,
                &result.config,
                None,
            )),
            render_svg(&performance::figure_lto_arrival(
                report,
                &sized_config,
                None,
            )),
            render_svg(&performance::figure_lto_arrival(
                &declared_report,
                &result.config,
                None,
            )),
        ),
    ] {
        assert_eq!(
            sized_svg, config_svg,
            "{name}: must be drawn at the sized takeoff mass"
        );
        assert_ne!(
            sized_svg, declared_svg,
            "{name}: must differ from the figure drawn at the declared MTOW"
        );
    }
    let payload_svg = render_svg(&performance::figure_payload_range(
        report,
        &result.config,
        None,
    ));
    assert!(payload_svg.contains("SIZED TOW") || payload_svg.contains("Sized TOW"));

    // Load-and-trim sheet: model weights, not the published reference.
    let sheet = load_trim_data_from_pipeline(&result).expect("load and trim data");
    let mtow_line = sheet.weight("MTOW").expect("MTOW line");
    assert_eq!(mtow_line, sized_kg);
    let published = alas_config::presets::get(PRESET)
        .expect("A220 preset")
        .reference
        .clone();
    if let Some(published_mtow_kg) = published.mtow_kg {
        assert_ne!(mtow_line, published_mtow_kg);
    }
    assert!(sheet
        .notes
        .iter()
        .any(|note| note.contains("Published reference")));
}
