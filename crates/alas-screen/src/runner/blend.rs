// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Weighted composite score across the screening stages.

use crate::types::AirfoilCandidateResult;

/// Assign min-max normalized weighted composite score to each candidate.
pub fn blend_scores(
    candidates: &mut [AirfoilCandidateResult],
    ld_weight: f64,
    fuel_weight: f64,
    robustness_weight: f64,
    key: impl Fn(&AirfoilCandidateResult) -> f64,
    is_3d: bool,
) {
    if candidates.is_empty() {
        return;
    }

    let normed = |values: &[f64]| -> Vec<f64> {
        let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let span = if (hi - lo).abs() > 1e-12 {
            hi - lo
        } else {
            1.0
        };
        values.iter().map(|&v| (v - lo) / span).collect()
    };

    let ld_vals: Vec<f64> = candidates.iter().map(&key).collect();
    let fuel_vals: Vec<f64> = candidates
        .iter()
        .map(|r| r.tank_capacity_kg.unwrap_or(0.0))
        .collect();

    let norm_ld = normed(&ld_vals);
    let norm_fuel = normed(&fuel_vals);

    let use_robust = robustness_weight > 0.0 && candidates.iter().any(|r| r.robustness.is_some());
    let norm_robust = if use_robust {
        let rob_vals: Vec<f64> = candidates
            .iter()
            .map(|r| r.robustness.unwrap_or(0.0))
            .collect();
        normed(&rob_vals)
    } else {
        vec![0.0; candidates.len()]
    };

    for (i, r) in candidates.iter_mut().enumerate() {
        let mut score = ld_weight * norm_ld[i] + fuel_weight * norm_fuel[i];
        if use_robust {
            score += robustness_weight * norm_robust[i];
        }
        if is_3d {
            r.score_3d = Some(score);
        } else {
            r.score = Some(score);
        }
    }
}
