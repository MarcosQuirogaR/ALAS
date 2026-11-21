// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Load & trim sheet has one answer: the desktop's pre-run draft ("Analyze
//! reference") and the sheet of a completed run of the same inputs assess the
//! same loading, so every state reads the same CG and limits and the same
//! verdict. The draft used to build its CG assessment from a lumped breakdown
//! at the closure mass, which on the AVE carried 149 t of fuel its tanks
//! cannot hold and on the ATR 72-600 placed the empty aircraft 27 %MAC aft.

// A test asserts on values it constructed, so a failed unwrap is the
// assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{AlasConfig, DesignMode};
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};
use alas_report::families::mass_balance::load_trim::data::{
    load_trim_data_from_pipeline, load_trim_data_from_report,
};
use alas_report::families::mass_balance::load_trim::LoadTrimSheetData;
use alas_report::families::mass_balance::{attach_preview_neutral_point, quick_preview_report};

/// The sheet of a completed baseline run of `name` and the sheet of the draft
/// the desktop builds before any run, from the same configuration.
fn both_sheets(name: &str) -> (LoadTrimSheetData, LoadTrimSheetData) {
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
    let result = DesignPipeline::new(config.clone())
        .run(&options, &RunEnvironment::default())
        .unwrap();
    let run = load_trim_data_from_pipeline(&result).expect("run sheet");

    let design = alas_config::presets::get(name).unwrap().design_vector;
    let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .unwrap();
    let mut draft = quick_preview_report(plane, &config, design).unwrap();
    attach_preview_neutral_point(&mut draft, &config).unwrap();
    let draft = load_trim_data_from_report(&draft, &config).expect("draft sheet");
    (run, draft)
}

fn assert_same_answer(name: &str, run: &LoadTrimSheetData, draft: &LoadTrimSheetData) {
    for state in ["DOW", "ZFW"] {
        let a = run.steps.iter().find(|s| s.state == state).unwrap();
        let b = draft.steps.iter().find(|s| s.state == state).unwrap();
        assert!(
            (a.mass_kg - b.mass_kg).abs() < 0.002 * a.mass_kg,
            "{name} {state} mass"
        );
        assert!(
            (a.pct_mac - b.pct_mac).abs() < 0.5,
            "{name} {state} CG {} vs {}",
            a.pct_mac,
            b.pct_mac
        );
        let (ga, gb) = (a.gate.as_ref().unwrap(), b.gate.as_ref().unwrap());
        assert_eq!(ga.violated, gb.violated, "{name} {state} verdict");
        assert!(
            (ga.fwd_pct_mac - gb.fwd_pct_mac).abs() < 1.0,
            "{name} {state} forward limit"
        );
        assert!(
            (ga.aft_pct_mac - gb.aft_pct_mac).abs() < 1.0,
            "{name} {state} aft limit"
        );
    }
    assert_eq!(run.envelopes().len(), draft.envelopes().len(), "{name}");
}

#[test]
fn the_ave_draft_and_the_run_assess_one_loading() {
    let (run, draft) = both_sheets("AVE");
    assert_same_answer("AVE", &run, &draft);
    // The draft's takeoff state is judged by the gate against its flown-mass
    // limits and passes, as the run's does (it read 21.5 %MAC forward, outside
    // its limits, from the lumped breakdown).
    let tow = draft.steps.iter().find(|s| s.state == "TOW").unwrap();
    assert!(!tow.gate.as_ref().unwrap().violated);
    assert!((tow.gate.as_ref().unwrap().fwd_pct_mac - 14.9).abs() < 2.0);
}

#[test]
fn the_atr_draft_and_the_run_place_the_empty_aircraft_alike_and_inside_its_limits() {
    let (run, draft) = both_sheets("ATR72-600");
    assert_same_answer("ATR72-600", &run, &draft);
    let dow = draft.steps.iter().find(|s| s.state == "DOW").unwrap();
    // The ATR 72 weight and balance manual's worked example places the
    // operating empty weight near 27 %MAC.
    assert!(
        (15.0..=35.0).contains(&dow.pct_mac),
        "DOW at {} %MAC",
        dow.pct_mac
    );
    assert!(!dow.gate.as_ref().unwrap().violated);
}
