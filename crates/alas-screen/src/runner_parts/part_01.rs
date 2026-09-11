// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;

use alas_aero::neuralfoil::ModelSize;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::airfoil_library::AirfoilLibrary;

use crate::refine::{refine_candidate_3d_with_mass_model, ScreeningMassModel};
use crate::score::{
    cruise_condition_with_geometry, score_candidate_with_geometry, ScreeningGeometry,
};
use crate::types::{
    AirfoilCandidateResult, AirfoilScreeningOptions, AirfoilScreeningResult, ScreeningFlowRegime,
    REFERENCE_AIRFOILS, TRANSONIC_MACH_CAVEAT,
};
use crate::verify_mses::verify_candidate_mses;

/// MSES runs are external processes; four workers keep the desktop responsive
/// without creating an unbounded process fan-out on a many-core workstation.
const MAX_MSES_WORKERS: usize = 4;

/// Simple glob / wildcard match for `*` and `?`.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut pi = 0;
    let mut ti = 0;
    let mut star_pi = None;
    let mut star_ti = 0;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_pi = Some(pi);
            pi += 1;
            star_ti = ti;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Restrict airfoil `names` to those matching `pattern` (case-insensitive substring or glob).
pub fn filter_names(names: &[&str], pattern: &str) -> Vec<String> {
    let tokens: Vec<String> = pattern
        .split(',')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();

    if tokens.is_empty() {
        return names.iter().map(|&s| s.to_string()).collect();
    }

    let mut out = Vec::new();
    for &name in names {
        let low = name.to_lowercase();
        for tok in &tokens {
            let matches = if tok.contains('*') || tok.contains('?') {
                glob_match(tok, &low)
            } else {
                low.contains(tok)
            };
            if matches {
                out.push(name.to_string());
                break;
            }
        }
    }
    out
}

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

/// Execute the full multi-stage airfoil screening sweep.
#[allow(clippy::too_many_arguments)] // mirrors the translated public screening contract
pub fn run_airfoil_screening(
    config: &AlasConfig,
    dv: Option<&DesignVector>,
    options: &AirfoilScreeningOptions,
    mses_dir: Option<&Path>,
    progress_callback: Option<&mut dyn FnMut(&str)>,
    should_cancel: Option<&(dyn Fn() -> bool + Sync)>,
) -> Result<AirfoilScreeningResult, String> {
    run_airfoil_screening_with_mass_model(
        config,
        dv,
        options,
        mses_dir,
        progress_callback,
        should_cancel,
        ScreeningMassModel::ReferenceCompatibility,
    )
}

/// Execute screening with the physical product mass-coordinate model.
///
/// The translated [`run_airfoil_screening`] entry point remains explicitly
/// reference-compatible so its fixture is not changed by a product-model
/// improvement. Desktop and other product callers use this entry point.
#[allow(clippy::too_many_arguments)] // product entry keeps the callback and cancellation contract explicit
pub fn run_airfoil_screening_product(
    config: &AlasConfig,
    dv: Option<&DesignVector>,
    options: &AirfoilScreeningOptions,
    mses_dir: Option<&Path>,
    progress_callback: Option<&mut dyn FnMut(&str)>,
    should_cancel: Option<&(dyn Fn() -> bool + Sync)>,
) -> Result<AirfoilScreeningResult, String> {
    run_airfoil_screening_with_mass_model(
        config,
        dv,
        options,
        mses_dir,
        progress_callback,
        should_cancel,
        ScreeningMassModel::StructuralWingbox,
    )
}
