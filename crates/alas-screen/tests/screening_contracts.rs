// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Screening failure boundaries and unit-independent ranking contracts.

use alas_config::{design_variables::DesignVector, AlasConfig};
use alas_screen::{
    runner::{blend_scores, run_airfoil_screening_product},
    score::cruise_condition,
    types::{AirfoilCandidateResult, AirfoilScreeningOptions},
};

#[test]
fn ranking_is_invariant_to_positive_mass_unit_scaling() {
    let mut candidates = vec![
        AirfoilCandidateResult {
            l_over_d: Some(10.0),
            tank_capacity_kg: Some(100.0),
            ..Default::default()
        },
        AirfoilCandidateResult {
            l_over_d: Some(20.0),
            tank_capacity_kg: Some(50.0),
            ..Default::default()
        },
    ];
    blend_scores(
        &mut candidates,
        0.7,
        0.3,
        0.0,
        |r| r.l_over_d.unwrap_or(0.0),
        false,
    );
    let original: Vec<_> = candidates.iter().map(|r| r.score).collect();
    for candidate in &mut candidates {
        candidate.tank_capacity_kg = candidate.tank_capacity_kg.map(|v| v * 1000.0);
    }
    blend_scores(
        &mut candidates,
        0.7,
        0.3,
        0.0,
        |r| r.l_over_d.unwrap_or(0.0),
        false,
    );
    assert_eq!(
        original,
        candidates.iter().map(|r| r.score).collect::<Vec<_>>()
    );
    assert_eq!(original, vec![Some(0.3), Some(0.7)]);
}

#[test]
fn tied_candidate_scores_are_finite_in_both_stages() {
    let mut candidates = vec![AirfoilCandidateResult::default(); 2];
    for is_3d in [false, true] {
        blend_scores(&mut candidates, 0.7, 0.3, 0.2, |_| 1.0, is_3d);
        for candidate in &candidates {
            assert_eq!(
                if is_3d {
                    candidate.score_3d
                } else {
                    candidate.score
                },
                Some(0.0)
            );
        }
    }
}

#[test]
fn invalid_target_cl_is_reported_before_screening() {
    for target in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let options = AirfoilScreeningOptions {
            target_cl: Some(target),
            ..Default::default()
        };
        let result =
            run_airfoil_screening_product(&AlasConfig::default(), None, &options, None, None, None);
        assert!(matches!(result, Err(message) if message.contains("target CL")));
    }
}

#[test]
fn cruise_lift_coefficient_scales_with_weight_at_fixed_geometry() -> Result<(), String> {
    let mut config = AlasConfig::default();
    let dv = DesignVector::default();
    let (mach, re, cl, altitude_m) = cruise_condition(&config, &dv)?;
    config.requirements.mtow_kg *= 1.2;
    let (new_mach, new_re, new_cl, new_altitude_m) = cruise_condition(&config, &dv)?;
    assert!((new_cl / cl - 1.2).abs() < 1e-12);
    assert_eq!((mach, re, altitude_m), (new_mach, new_re, new_altitude_m));
    Ok(())
}

#[test]
fn malformed_angle_sweep_returns_an_error_without_allocating_samples() {
    for (min, max, step) in [
        (10.0, -4.0, 0.5),
        (-4.0, 14.0, 0.0),
        (-4.0, 14.0, -0.5),
        (f64::NAN, 14.0, 0.5),
        (-4.0, f64::INFINITY, 0.5),
    ] {
        let options = AirfoilScreeningOptions {
            alpha_min_deg: min,
            alpha_max_deg: max,
            alpha_step_deg: step,
            ..Default::default()
        };
        let result =
            run_airfoil_screening_product(&AlasConfig::default(), None, &options, None, None, None);
        assert!(matches!(result, Err(message) if message.contains("alpha sweep")));
    }
}
