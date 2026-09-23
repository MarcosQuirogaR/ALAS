// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parity tests comparing `alas-pipeline::full_analysis` (`FullAnalysis`)
//! against `golden/pipeline/full_analysis.json`.
//!
//! # Wave-drag re-pin (physics review v1.2, finding A3)
//!
//! `alas-aero::analysis::AeroAnalysis::wave_drag` now applies the published
//! Lock/Korn law (`CD_w = 20 (M - M_crit)^4`) instead of substituting the
//! drag-divergence Mach `M_dd` for the critical Mach `M_crit`. This raises
//! cruise CD and lowers L/D for every jet case whose cruise point sits above
//! `M_crit`, so `design_point.{cd,l_over_d}`, `polar_fit.{cd0,k,oswald_e}`
//! and (for the `default` case, the only one of the three whose trim solve
//! converges) `trimmed_design_point.*` were re-pinned to the corrected
//! values for all three fixture cases (`default`, `narrowbody`,
//! `high_aspect_ratio`), verified by evaluating the corrected build and
//! reverting only the wave-drag fix to confirm the old fixture values
//! reproduce exactly under the superseded law. `high_aspect_ratio` and
//! `narrowbody`'s `trimmed_design_point` fixture entries are stale
//! (unrelated pre-existing degenerate/unconverged trims -- confirmed
//! unaffected by this change under both the old and new law) and were left
//! untouched; the Rust side already returns `None` for both regardless, so
//! this test's `if let (Some, Some)` guard never compares them.
//!
//! # Gravity re-pin (physics review v1.2, finding F5)
//!
//! `alas_config::requirements::DesignRequirements::gravity_m_s2` also moved
//! from the frozen two-decimal `9.81` to `alas_units::STANDARD_GRAVITY`
//! (9.80665), a 0.035% decrease that reaches the FLOPS mass model's own
//! weight-from-mass terms independently of the wave-drag correction above.
//! `physical_cg` and `component_masses.{Fuel,Propulsion}` shifted by the
//! same fraction in all three fixture cases (confirmed by reverting only
//! the gravity default and reproducing the old fixture values exactly), so
//! those five fields were re-pinned too, in every case, alongside the
//! aero-driven ones. Every other component mass, and every geometry field,
//! is unaffected and was left as-is.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_pipeline::full_analysis::FullAnalysis;
use alas_testkit::{load, Comparison, Tier};
use serde::Deserialize;

const TIER_NUMERIC: Tier = Tier::Iter { relative: 1e-4 };
const TIER_MASS: Tier = Tier::Closed;

#[derive(Debug, Deserialize)]
struct FixturePoint {
    alpha_deg: f64,
    cl: f64,
    cd: f64,
    l_over_d: f64,
}

#[derive(Debug, Deserialize)]
struct FixturePolarFit {
    cd0: f64,
    k: f64,
    oswald_e: f64,
    aspect_ratio: f64,
}

#[derive(Debug, Deserialize)]
struct FixtureTrimPoint {
    alpha_deg: f64,
    trim_ih_deg: f64,
    cl: f64,
    cd: f64,
    l_over_d: f64,
    cm_residual: f64,
}

// Allow unused fields in deserialize fixtures for schema completeness
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct FixtureCase {
    design: DesignVector,
    design_point: FixturePoint,
    polar_fit: FixturePolarFit,
    static_margin: f64,
    x_neutral_point: f64,
    physical_cg: [f64; 3],
    geometry_summary: HashMap<String, f64>,
    component_masses: HashMap<String, f64>,
    mass_coordinates: HashMap<String, [f64; 3]>,
    cg_envelope_ok: Option<bool>,
    trimmed_design_point: Option<FixtureTrimPoint>,
}

#[test]
fn full_analysis_matches_reference_fixtures() {
    let cases: HashMap<String, FixtureCase> = load("pipeline", "full_analysis");

    for (name, expected) in cases {
        let mut config = AlasConfig::default();
        if name == "narrowbody" {
            config.requirements.cruise_mach = 0.78;
            config.requirements.cruise_altitude_m = 10668.0;
        } else if name == "high_aspect_ratio" {
            config.requirements.cruise_mach = 0.75;
        }

        let full_analysis = FullAnalysis::new_reference_compatibility(config);
        let actual = full_analysis
            .run(&expected.design, true)
            .unwrap_or_else(|e| panic!("failed full analysis for {name}: {e}"));

        let mut cmp = Comparison::new(format!("FullAnalysis [{name}]"), TIER_NUMERIC);

        // Design point comparison
        cmp.scalar(
            "design_point.alpha_deg",
            actual.design_point.alpha_deg,
            expected.design_point.alpha_deg,
        );
        cmp.scalar(
            "design_point.cl",
            actual.design_point.cl,
            expected.design_point.cl,
        );
        cmp.scalar(
            "design_point.cd",
            actual.design_point.cd,
            expected.design_point.cd,
        );
        cmp.scalar(
            "design_point.l_over_d",
            actual.design_point.l_over_d,
            expected.design_point.l_over_d,
        );

        // Polar fit comparison
        cmp.scalar(
            "polar_fit.cd0",
            actual.polar_fit.cd0,
            expected.polar_fit.cd0,
        );
        cmp.scalar("polar_fit.k", actual.polar_fit.k, expected.polar_fit.k);
        cmp.scalar(
            "polar_fit.oswald_e",
            actual.polar_fit.oswald_e,
            expected.polar_fit.oswald_e,
        );
        cmp.scalar(
            "polar_fit.aspect_ratio",
            actual.polar_fit.aspect_ratio,
            expected.polar_fit.aspect_ratio,
        );

        // Stability comparison
        cmp.scalar(
            "static_margin",
            actual.static_margin,
            expected.static_margin,
        );
        cmp.scalar(
            "x_neutral_point",
            actual.x_neutral_point,
            expected.x_neutral_point,
        );

        // Trimmed design point comparison
        if let (Some(actual_trim), Some(expected_trim)) =
            (actual.trimmed_design_point, expected.trimmed_design_point)
        {
            cmp.scalar(
                "trimmed.alpha_deg",
                actual_trim.alpha_deg,
                expected_trim.alpha_deg,
            );
            cmp.scalar(
                "trimmed.trim_ih_deg",
                actual_trim.trim_ih_deg,
                expected_trim.trim_ih_deg,
            );
            cmp.scalar("trimmed.cl", actual_trim.cl, expected_trim.cl);
            cmp.scalar("trimmed.cd", actual_trim.cd, expected_trim.cd);
            cmp.scalar(
                "trimmed.l_over_d",
                actual_trim.l_over_d,
                expected_trim.l_over_d,
            );
            cmp.scalar(
                "trimmed.cm_residual",
                actual_trim.cm_residual,
                expected_trim.cm_residual,
            );
        }

        cmp.exact(
            "cg_envelope_ok",
            &actual.cg_envelope_ok,
            &expected.cg_envelope_ok,
        );

        cmp.finish();

        // Mass and CG comparison at Closed tier
        let mut cmp_mass = Comparison::new(format!("FullAnalysis Mass [{name}]"), TIER_MASS);
        cmp_mass.scalar(
            "physical_cg.x",
            actual.physical_cg[0],
            expected.physical_cg[0],
        );
        cmp_mass.scalar(
            "physical_cg.y",
            actual.physical_cg[1],
            expected.physical_cg[1],
        );
        cmp_mass.scalar(
            "physical_cg.z",
            actual.physical_cg[2],
            expected.physical_cg[2],
        );

        for (comp, &expected_mass) in &expected.component_masses {
            let actual_mass = actual
                .component_masses
                .get(comp)
                .copied()
                .unwrap_or_default();
            cmp_mass.scalar(&format!("mass.{comp}"), actual_mass, expected_mass);
        }

        cmp_mass.finish();
    }
}
