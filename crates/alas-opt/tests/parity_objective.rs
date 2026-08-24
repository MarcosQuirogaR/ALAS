// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parity test for `alas-opt::objective`, `envelope`, and `history`.

// A test asserts on values it constructed or loaded from a fixture it controls, so a failed unwrap is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::run_mass_analysis;
use alas_opt::envelope::check_cg_envelope;
use alas_opt::objective::{wing_fuel_volume_m3, DesignObjective};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct FuelVolumeCase {
    usable_fraction: f64,
    volume_m3: f64,
}

#[derive(Debug, Deserialize)]
struct CgEnvelopeCase {
    cg_x: f64,
    x_np: f64,
    mac: f64,
    violation: bool,
    worst_exceedance: f64,
}

#[derive(Debug, Deserialize)]
struct EvalCase {
    label: String,
    vector: Vec<f64>,
    cost: f64,
    valid: bool,
    l_over_d: f64,
    span_m: f64,
    alpha_deg: f64,
    area_m2: f64,
    trim_ih_deg: f64,
    reject_reason: String,
}

#[derive(Debug, Deserialize)]
struct HistorySummary {
    n_evaluations: usize,
    n_valid: usize,
    reject_reason_counts: HashMap<String, usize>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    fuel_volume_cases: Vec<FuelVolumeCase>,
    cg_envelope_cases: Vec<CgEnvelopeCase>,
    evaluation_cases: Vec<EvalCase>,
    history_summary: HistorySummary,
}

#[test]
fn parity_objective() {
    let fixture: Fixture = alas_testkit::load("opt", "objective");
    // The fixture was generated with the historical three-station wing.
    // Obtain its configuration through the compatibility constructor so the
    // standalone fuel-volume and CG checks use the same geometry as the
    // objective replay below.
    let config = DesignObjective::new_reference_compatibility(AlasConfig::default()).config;
    let dv_default = DesignVector::default();
    let builder = AircraftBuilder::new(Some(config.geometry.clone()));
    let plane = builder
        .build(Some(&dv_default), false)
        .expect("build plane");
    let wing = &plane.wings[0];

    // 1. Fuel volume tests (Closed tier)
    let mut comp_vol = Comparison::new("wing_fuel_volume_m3", Tier::Closed);
    for case in &fixture.fuel_volume_cases {
        let actual = wing_fuel_volume_m3(wing, case.usable_fraction);
        comp_vol.scalar(
            &format!("usable_{}", case.usable_fraction),
            actual,
            case.volume_m3,
        );
    }
    comp_vol.finish();

    // 2. CG envelope tests (Closed tier)
    let (masses, coords, _) = run_mass_analysis(
        &plane,
        &config.requirements,
        &config.geometry,
        Some(&config.mass_model),
        None,
    );

    let mut comp_env = Comparison::new("check_cg_envelope", Tier::Closed);
    for case in &fixture.cg_envelope_cases {
        let res = check_cg_envelope(
            &plane, &masses, &coords, case.cg_x, case.x_np, case.mac, &config,
        );
        comp_env.exact(
            &format!("cg_x_{}_violation", case.cg_x),
            &res.violation,
            &case.violation,
        );
        comp_env.scalar(
            &format!("cg_x_{}_exceedance", case.cg_x),
            res.worst_exceedance,
            case.worst_exceedance,
        );
    }
    comp_env.finish();

    // 3. DesignObjective evaluation cases (Closed tier)
    let mut obj = DesignObjective::new_reference_compatibility(config);
    let mut comp_cost = Comparison::new("DesignObjective::evaluate", Tier::Closed);

    for case in &fixture.evaluation_cases {
        let cost = obj.evaluate(&case.vector);
        let h = &obj.history;
        let last_idx = h.cost.len() - 1;

        comp_cost.scalar(&format!("{}_cost", case.label), cost, case.cost);
        comp_cost.exact(
            &format!("{}_valid", case.label),
            &h.valid[last_idx],
            &case.valid,
        );
        comp_cost.scalar(
            &format!("{}_l_over_d", case.label),
            h.l_over_d[last_idx],
            case.l_over_d,
        );
        comp_cost.scalar(
            &format!("{}_span_m", case.label),
            h.span_m[last_idx],
            case.span_m,
        );
        comp_cost.scalar(
            &format!("{}_alpha_deg", case.label),
            h.alpha_deg[last_idx],
            case.alpha_deg,
        );
        comp_cost.scalar(
            &format!("{}_area_m2", case.label),
            h.area_m2[last_idx],
            case.area_m2,
        );
        comp_cost.scalar(
            &format!("{}_trim_ih_deg", case.label),
            h.trim_ih_deg[last_idx],
            case.trim_ih_deg,
        );
        comp_cost.exact(
            &format!("{}_reject_reason", case.label),
            &h.reject_reason[last_idx],
            &case.reject_reason,
        );
    }
    comp_cost.finish();

    // 4. History diagnostics
    assert_eq!(
        obj.history.n_evaluations(),
        fixture.history_summary.n_evaluations
    );
    assert_eq!(obj.history.n_valid(), fixture.history_summary.n_valid);
    assert_eq!(
        obj.history.reject_reason_counts(),
        fixture.history_summary.reject_reason_counts
    );
}

#[test]
fn seeded_python_winner_keeps_the_reference_objective_value() {
    let vector = [
        77.26310883297792,
        16.033742604077958,
        7.894322283595826,
        1.6464001591609407,
        34.72425730322664,
        0.8729953757071804,
        -2.1679461811881957,
        1.065889432108684,
        79.30530367149531,
        0.20106266931417716,
        0.8996440767000335,
        0.9630845474896375,
        0.0002894595161818379,
        0.0006704778573582777,
        -0.000594121555662916,
        0.00010574272521884703,
    ];
    let mut objective = DesignObjective::new_reference_compatibility(AlasConfig::default());
    let cost = objective.evaluate(&vector);
    let last = objective.history.cost.len() - 1;

    assert!(alas_testkit::agrees(
        cost,
        -22.435780026498687,
        Tier::Closed
    ));
    assert!(alas_testkit::agrees(
        objective.history.l_over_d[last],
        20.20142609929575,
        Tier::Closed
    ));
    assert!(objective.history.valid[last]);
    assert_eq!(objective.history.reject_reason[last], "");
}
