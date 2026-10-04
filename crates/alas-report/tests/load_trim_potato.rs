// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The load-and-trim sheet of registered presets: boarding potato, worked
//! case and the phase limit sets, from a full baseline pipeline run.

// A test asserts on values it constructed, so a failed unwrap is the
// assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{AlasConfig, DesignMode};
use alas_payload::layout::ItemKind;
use alas_pipeline::{DesignPipeline, PipelineOptions, PipelineResult, RunEnvironment};
use alas_report::families::mass_balance::load_trim::data::load_trim_data_from_pipeline;
use alas_report::families::mass_balance::load_trim::{
    figure_load_trim_sheet, loading_envelope_polygon, LoadTrimSheetData,
};
use alas_report::scene::SceneElement;
use alas_report::theme::PALETTE_LIGHT;

fn run(name: &str) -> PipelineResult {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config.mission.enabled = false;
    config.mses.enabled = false;
    config.structures.enabled = false;
    let options = PipelineOptions {
        optimize: false,
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
        .run(&options, &RunEnvironment::default())
        .unwrap()
}

/// Potato CG range (%MAC) at `mass_kg`, linear between levels.
fn potato_at(data: &LoadTrimSheetData, mass_kg: f64) -> Option<(f64, f64)> {
    data.potato.windows(2).find_map(|w| {
        (mass_kg >= w[0].mass_kg - 1e-6 && mass_kg <= w[1].mass_kg + 1e-6).then(|| {
            let t = if w[1].mass_kg > w[0].mass_kg {
                ((mass_kg - w[0].mass_kg) / (w[1].mass_kg - w[0].mass_kg)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (
                w[0].fwd_pct_mac + t * (w[1].fwd_pct_mac - w[0].fwd_pct_mac),
                w[0].aft_pct_mac + t * (w[1].aft_pct_mac - w[0].aft_pct_mac),
            )
        })
    })
}

fn dashed(scene: &alas_report::scene::Scene, dash: &[f64]) -> usize {
    scene
        .elements
        .iter()
        .filter(|e| {
            matches!(e, SceneElement::Polyline { stroke, .. }
                if stroke.dash_array.as_deref() == Some(dash) && stroke.width > 1.5)
        })
        .count()
}

#[test]
fn the_a320_potato_holds_the_worked_case_lies_inside_the_reorder_polygon_and_reports_the_ground_check(
) {
    let result = run("A320-200");
    let data = load_trim_data_from_pipeline(&result).expect("load and trim data");
    assert!(data.sequences.len() >= 12, "{}", data.sequences.len());
    assert!(data.potato.len() >= 30);

    // Worked-case points lie inside the potato.
    for step in &data.steps {
        if let Some((lo, hi)) = potato_at(&data, step.mass_kg) {
            assert!(
                step.pct_mac >= lo - 0.1 && step.pct_mac <= hi + 0.1,
                "{} at {:.0} kg: {:.2} outside [{lo:.2}, {hi:.2}]",
                step.item,
                step.mass_kg,
                step.pct_mac
            );
        }
    }
    let inside = data
        .steps
        .iter()
        .filter(|s| potato_at(&data, s.mass_kg).is_some())
        .count();
    assert!(inside >= 4, "steps covered by the potato: {inside}");

    // Regression: inside the old reorder polygon (every order of every item).
    let report = result
        .optimized_report
        .as_ref()
        .or(result.baseline_analysis.as_ref())
        .unwrap();
    let layout = report.payload_layout.as_ref().unwrap();
    let items: Vec<(f64, f64)> = layout
        .items
        .iter()
        .filter(|i| {
            i.mass > 0.0 && matches!(i.kind, ItemKind::SeatRow | ItemKind::Bag | ItemKind::Uld)
        })
        .map(|i| (i.mass, i.x))
        .collect();
    let dow = &data.steps[0];
    let dow_x = data.x_lemac_m + dow.pct_mac / 100.0 * data.mac_m;
    let polygon = loading_envelope_polygon(dow.mass_kg, dow_x, &items, data.x_lemac_m, data.mac_m);
    let n = items.len();
    let fwd = &polygon[..=n];
    let mut aft: Vec<(f64, f64)> = polygon[n..].to_vec();
    aft.reverse();
    let chain = |pts: &[(f64, f64)], m: f64| {
        pts.windows(2).find_map(|w| {
            (m >= w[0].0 - 1e-6 && m <= w[1].0 + 1e-6 && w[1].0 > w[0].0)
                .then(|| w[0].1 + (m - w[0].0) / (w[1].0 - w[0].0) * (w[1].1 - w[0].1))
        })
    };
    let zfw = polygon.iter().map(|p| p.0).fold(0.0, f64::max);
    let mut checked = 0;
    for level in data.potato.iter().filter(|l| l.mass_kg <= zfw) {
        let (Some(lo), Some(hi)) = (chain(fwd, level.mass_kg), chain(&aft, level.mass_kg)) else {
            continue;
        };
        checked += 1;
        assert!(
            level.fwd_pct_mac >= lo - 0.1 && level.aft_pct_mac <= hi + 0.1,
            "potato at {:.0} kg outside the reorder polygon",
            level.mass_kg
        );
    }
    assert!(checked >= 15, "{checked}");

    assert!(data.potato_ground_exceedance_pct_mac().is_finite());
}

#[test]
fn separate_ground_takeoff_flight_and_landing_sets_are_present_and_drawn() {
    let data = load_trim_data_from_pipeline(&run("A320-200")).expect("load and trim data");
    for (name, set) in [
        ("ground", &data.ground_limits),
        ("takeoff", &data.takeoff_limits),
        ("flight", &data.flight_limits),
        ("landing", &data.landing_limits),
    ] {
        assert!(!set.is_empty(), "{name} set");
    }
    // Fewer mechanisms cannot be more restrictive: at the takeoff mass the
    // ground band contains the takeoff band.
    let (g, t) = (
        data.ground_limits.last().unwrap(),
        data.takeoff_limits.last().unwrap(),
    );
    assert!(g.fwd_pct_mac <= t.fwd_pct_mac + 1e-9 && g.aft_pct_mac >= t.aft_pct_mac - 1e-9);
    // The landing band ends at the design landing mass.
    let landing_mass = data
        .weight(alas_report::families::mass_balance::load_trim::MassRole::DesignLanding)
        .unwrap();
    assert!((data.landing_limits.last().unwrap().mass_kg - landing_mass).abs() < 1.5);

    let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
    assert_eq!(dashed(&scene, &[7.0, 4.0]), 2, "ground line dashed");
    assert_eq!(dashed(&scene, &[1.5, 3.5]), 2, "flight line dotted");
    assert_eq!(dashed(&scene, &[8.0, 3.0, 1.5, 3.0]), 2, "landing dash-dot");
}

#[test]
fn the_atr_sheet_builds_with_per_hold_cargo_and_finite_ground_check() {
    let data = load_trim_data_from_pipeline(&run("ATR72-600")).expect("load and trim data");
    assert!(!data.potato.is_empty());
    assert!(data.potato_ground_exceedance_pct_mac().is_finite());
    assert!(!data.ground_limits.is_empty());
}
