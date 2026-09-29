// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Figures of a baseline (non-optimizing) run are drawn at the declared MTOW.
//!
//! Only a sized product-optimizer finalist is bound to a mission-sized
//! takeoff mass. The payload-range, landing and take-off, and matching
//! figures of a baseline report must use `config.requirements.mtow_kg`, and
//! the payload-range footer must say "MTOW", not "Sized TOW".

// A test asserts on values it constructed, so a failed unwrap is the
// assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{AlasConfig, DesignMode};
use alas_pipeline::{DesignPipeline, PipelineOptions, PipelineResult, RunEnvironment};
use alas_report::families::performance;
use alas_report::svg::render_svg;

const PRESETS: [&str; 2] = ["A320-200", "ATR72-600"];

fn run_baseline(name: &str) -> PipelineResult {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config.mission.enabled = false;
    config.mses.enabled = false;
    config.structures.enabled = false;
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: true,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    DesignPipeline::new(config)
        .run(&options, &RunEnvironment::default())
        .unwrap()
}

/// The three field/matching figures of `report` under `config`, rendered.
fn field_figures(
    report: &alas_pipeline::full_analysis::AnalysisReport,
    config: &AlasConfig,
) -> [String; 3] {
    [
        render_svg(&performance::figure_matching_chart(report, config, None)),
        render_svg(&performance::figure_lto_departure(report, config, None)),
        render_svg(&performance::figure_lto_arrival(report, config, None)),
    ]
}

#[test]
fn baseline_figures_are_drawn_at_the_declared_mtow() {
    for name in PRESETS {
        let result = run_baseline(name);
        let report = result
            .baseline_analysis
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: {:?}", result.baseline_analysis_error));
        let declared_mtow_kg = result.config.requirements.mtow_kg;
        assert!(declared_mtow_kg > 0.0, "{name}");
        assert_eq!(report.sized_takeoff_mass_kg(), None, "{name}");

        // Payload-range data and footer.
        let range = performance::payload_range_data(report, &result.config)
            .unwrap_or_else(|| panic!("{name}: payload-range data"));
        assert_eq!(range.mtow_kg, declared_mtow_kg, "{name}");
        assert!(!range.mass_is_sized, "{name}");
        let range_svg = render_svg(&performance::figure_payload_range(
            report,
            &result.config,
            None,
        ));
        assert!(range_svg.contains("MTOW: "), "{name}: footer label");
        assert!(!range_svg.contains("Sized TOW"), "{name}: footer label");

        // Matching and LTO figures equal those drawn from a configuration
        // whose ceiling is the declared mass explicitly...
        let declared_config = result.config.at_closure_mass(declared_mtow_kg);
        let at_declared = field_figures(report, &result.config);
        assert_eq!(
            at_declared,
            field_figures(report, &declared_config),
            "{name}"
        );

        // ...and, as a sensitivity control, differ from a lighter ceiling, so
        // the equality above is not a figure that ignores mass.
        let lighter_config = result.config.at_closure_mass(0.9 * declared_mtow_kg);
        let at_lighter = field_figures(report, &lighter_config);
        assert_ne!(
            at_declared[0], at_lighter[0],
            "{name}: the matching chart must respond to the takeoff mass"
        );
    }
}
