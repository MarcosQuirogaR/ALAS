// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parity test for `alas-screen::runner`, `score`, `refine`, and `blend_scores`.

// A test asserts on values it constructed or loaded from a fixture it controls, so a failed unwrap is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_aero::neuralfoil::ModelSize;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::airfoil_library::AirfoilLibrary;
use alas_geom::asb::spacing::linspace;
use alas_screen::refine::refine_candidate_3d;
use alas_screen::runner::{blend_scores, filter_names, run_airfoil_screening};
use alas_screen::score::{cruise_condition, score_candidate};
use alas_screen::types::{AirfoilCandidateResult, AirfoilScreeningOptions};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CruiseConditionFixture {
    mach: f64,
    reynolds: f64,
    cl_target: f64,
    altitude_m: f64,
}

#[derive(Debug, Deserialize)]
struct FilterCase {
    pattern: String,
    expected_count: Option<usize>,
    expected_names: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct Stage1Fixture {
    name: String,
    status: String,
    l_over_d: Option<f64>,
    cd: Option<f64>,
    alpha_deg: Option<f64>,
    max_thickness_frac: Option<f64>,
    tank_volume_m3: Option<f64>,
    tank_capacity_kg: Option<f64>,
    robustness: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Stage2Fixture {
    name: String,
    refined: bool,
    l_over_d_3d: Option<f64>,
    cd_3d: Option<f64>,
    alpha_3d_deg: Option<f64>,
    cm_residual_3d: Option<f64>,
    static_margin_3d: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct BlendedStage1Fixture {
    name: String,
    score: f64,
}

#[derive(Debug, Deserialize)]
struct BlendedStage2Fixture {
    name: String,
    score_3d: f64,
}

#[derive(Debug, Deserialize)]
struct CandidateSweepFixture {
    name: String,
    l_over_d: Option<f64>,
    score: Option<f64>,
    refined: bool,
    l_over_d_3d: Option<f64>,
    score_3d: Option<f64>,
    is_reference: bool,
}

#[derive(Debug, Deserialize)]
struct FullSweepFixture {
    baseline_airfoil: String,
    n_total: usize,
    n_ok: usize,
    n_refined: usize,
    candidates: Vec<CandidateSweepFixture>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    cruise_condition: CruiseConditionFixture,
    filter_cases: Vec<FilterCase>,
    stage1_candidates: Vec<Stage1Fixture>,
    stage2_candidates: Vec<Stage2Fixture>,
    blended_stage1: Vec<BlendedStage1Fixture>,
    blended_stage2: Vec<BlendedStage2Fixture>,
    full_sweep: FullSweepFixture,
}

#[test]
fn parity_screening() {
    let fixture: Fixture = alas_testkit::load("screen", "screening");
    let config = AlasConfig::default();
    let dv = DesignVector::default();

    // 1. Cruise condition (Closed tier)
    let (mach, reynolds, cl_target, altitude) =
        cruise_condition(&config, &dv).expect("cruise condition");
    let mut comp_cruise = Comparison::new("cruise_condition", Tier::Closed);
    comp_cruise.scalar("mach", mach, fixture.cruise_condition.mach);
    comp_cruise.scalar("reynolds", reynolds, fixture.cruise_condition.reynolds);
    comp_cruise.scalar("cl_target", cl_target, fixture.cruise_condition.cl_target);
    comp_cruise.scalar("altitude_m", altitude, fixture.cruise_condition.altitude_m);
    comp_cruise.finish();

    // 2. Name filtering (Exact tier)
    let all_names = AirfoilLibrary::get_available_airfoils();
    for case in &fixture.filter_cases {
        let filtered = filter_names(&all_names, &case.pattern);
        if let Some(cnt) = case.expected_count {
            assert_eq!(filtered.len(), cnt);
        }
        if let Some(ref names) = case.expected_names {
            assert_eq!(&filtered, names);
        }
    }

    // 3. Stage 1 (2-D) scoring (Closed tier)
    let section_mach = mach * dv.sweep_deg.to_radians().cos();
    let n_alpha = (((14.0 - (-4.0)) / 0.5f64).round() as usize) + 1;
    let alphas_deg = linspace(-4.0, 14.0, n_alpha);

    let mut scored_candidates: Vec<AirfoilCandidateResult> = Vec::new();
    let mut comp_s1 = Comparison::new("score_candidate", Tier::Closed);

    for expected in &fixture.stage1_candidates {
        let actual = score_candidate(
            &expected.name,
            &config,
            &dv,
            section_mach,
            reynolds,
            cl_target,
            config.mass_model.fuel_tank_usable_fraction,
            &alphas_deg,
            ModelSize::Large,
            0.005,
            0.25,
            0.05,
        );

        comp_s1.exact(
            &format!("{}_status", expected.name),
            &actual.status,
            &expected.status,
        );
        if let (Some(act_ld), Some(exp_ld)) = (actual.l_over_d, expected.l_over_d) {
            comp_s1.scalar(&format!("{}_l_over_d", expected.name), act_ld, exp_ld);
        }
        if let (Some(act_cd), Some(exp_cd)) = (actual.cd, expected.cd) {
            comp_s1.scalar(&format!("{}_cd", expected.name), act_cd, exp_cd);
        }
        if let (Some(act_alpha), Some(exp_alpha)) = (actual.alpha_deg, expected.alpha_deg) {
            comp_s1.scalar(
                &format!("{}_alpha_deg", expected.name),
                act_alpha,
                exp_alpha,
            );
        }
        if let (Some(act_t), Some(exp_t)) = (actual.max_thickness_frac, expected.max_thickness_frac)
        {
            comp_s1.scalar(
                &format!("{}_max_thickness_frac", expected.name),
                act_t,
                exp_t,
            );
        }
        if let (Some(act_vol), Some(exp_vol)) = (actual.tank_volume_m3, expected.tank_volume_m3) {
            comp_s1.scalar(&format!("{}_tank_vol", expected.name), act_vol, exp_vol);
        }
        if let (Some(act_cap), Some(exp_cap)) = (actual.tank_capacity_kg, expected.tank_capacity_kg)
        {
            comp_s1.scalar(&format!("{}_tank_cap", expected.name), act_cap, exp_cap);
        }
        if let (Some(act_rob), Some(exp_rob)) = (actual.robustness, expected.robustness) {
            comp_s1.scalar(&format!("{}_robustness", expected.name), act_rob, exp_rob);
        }

        scored_candidates.push(actual);
    }
    comp_s1.finish();

    // 4. Stage 2 (3-D) refinement (Closed tier)
    let mut comp_s2 = Comparison::new("refine_candidate_3d", Tier::Closed);
    for (cand, expected) in scored_candidates.iter_mut().zip(&fixture.stage2_candidates) {
        if cand.status == "ok" {
            refine_candidate_3d(cand, &config, &dv, mach, altitude, cl_target, None);
            comp_s2.exact(
                &format!("{}_refined", expected.name),
                &cand.refined,
                &expected.refined,
            );
            if let (Some(act_ld), Some(exp_ld)) = (cand.l_over_d_3d, expected.l_over_d_3d) {
                comp_s2.scalar(&format!("{}_l_over_d_3d", expected.name), act_ld, exp_ld);
            }
            if let (Some(act_cd), Some(exp_cd)) = (cand.cd_3d, expected.cd_3d) {
                comp_s2.scalar(&format!("{}_cd_3d", expected.name), act_cd, exp_cd);
            }
            if let (Some(act_alpha), Some(exp_alpha)) = (cand.alpha_3d_deg, expected.alpha_3d_deg) {
                comp_s2.scalar(
                    &format!("{}_alpha_3d_deg", expected.name),
                    act_alpha,
                    exp_alpha,
                );
            }
            if let (Some(act_cm), Some(exp_cm)) = (cand.cm_residual_3d, expected.cm_residual_3d) {
                comp_s2.scalar(&format!("{}_cm_residual_3d", expected.name), act_cm, exp_cm);
            }
            if let (Some(act_sm), Some(exp_sm)) = (cand.static_margin_3d, expected.static_margin_3d)
            {
                comp_s2.scalar(
                    &format!("{}_static_margin_3d", expected.name),
                    act_sm,
                    exp_sm,
                );
            }
        }
    }
    comp_s2.finish();

    // 5. Score blending (Closed tier)
    let mut ok_cands: Vec<AirfoilCandidateResult> = scored_candidates
        .iter()
        .filter(|c| c.status == "ok")
        .cloned()
        .collect();

    blend_scores(
        &mut ok_cands,
        0.7,
        0.3,
        0.1,
        |r| r.l_over_d.unwrap_or(0.0),
        false,
    );

    let mut comp_b1 = Comparison::new("blend_scores_stage1", Tier::Closed);
    for (actual, expected) in ok_cands.iter().zip(&fixture.blended_stage1) {
        if let Some(s) = actual.score {
            comp_b1.scalar(&format!("{}_score", expected.name), s, expected.score);
        }
    }
    comp_b1.finish();

    let mut refined_cands: Vec<AirfoilCandidateResult> =
        ok_cands.into_iter().filter(|c| c.refined).collect();

    blend_scores(
        &mut refined_cands,
        0.7,
        0.3,
        0.1,
        |r| r.l_over_d_3d.unwrap_or(0.0),
        true,
    );

    let mut comp_b2 = Comparison::new("blend_scores_stage2", Tier::Closed);
    for (actual, expected) in refined_cands.iter().zip(&fixture.blended_stage2) {
        if let Some(s3d) = actual.score_3d {
            comp_b2.scalar(
                &format!("{}_score_3d", expected.name),
                s3d,
                expected.score_3d,
            );
        }
    }
    comp_b2.finish();

    // 6. Full screening sweep (Closed tier)
    let options = AirfoilScreeningOptions {
        name_filter: "sc20714, rae2822, whitcomb".to_string(),
        refine_3d: true,
        verify_mses: false,
        top_n: 10,
        ..Default::default()
    };

    let sweep_result = run_airfoil_screening(&config, Some(&dv), &options, None, None, None)
        .expect("screening sweep");

    assert_eq!(
        sweep_result.baseline_airfoil,
        fixture.full_sweep.baseline_airfoil
    );
    assert_eq!(sweep_result.n_total, fixture.full_sweep.n_total);
    assert_eq!(sweep_result.n_ok, fixture.full_sweep.n_ok);
    assert_eq!(sweep_result.n_refined, fixture.full_sweep.n_refined);

    let mut comp_sweep = Comparison::new("full_screening_sweep", Tier::Closed);
    for (actual, expected) in sweep_result
        .candidates
        .iter()
        .zip(&fixture.full_sweep.candidates)
    {
        comp_sweep.exact(
            &format!("{}_name", expected.name),
            &actual.name,
            &expected.name,
        );
        comp_sweep.exact(
            &format!("{}_refined", expected.name),
            &actual.refined,
            &expected.refined,
        );
        comp_sweep.exact(
            &format!("{}_is_reference", expected.name),
            &actual.is_reference,
            &expected.is_reference,
        );
        if let (Some(act_ld), Some(exp_ld)) = (actual.l_over_d, expected.l_over_d) {
            comp_sweep.scalar(&format!("{}_l_over_d", expected.name), act_ld, exp_ld);
        }
        if let (Some(act_s), Some(exp_s)) = (actual.score, expected.score) {
            comp_sweep.scalar(&format!("{}_score", expected.name), act_s, exp_s);
        }
        if let (Some(act_ld3d), Some(exp_ld3d)) = (actual.l_over_d_3d, expected.l_over_d_3d) {
            comp_sweep.scalar(
                &format!("{}_l_over_d_3d", expected.name),
                act_ld3d,
                exp_ld3d,
            );
        }
        if let (Some(act_s3d), Some(exp_s3d)) = (actual.score_3d, expected.score_3d) {
            comp_sweep.scalar(&format!("{}_score_3d", expected.name), act_s3d, exp_s3d);
        }
    }
    comp_sweep.finish();
}
