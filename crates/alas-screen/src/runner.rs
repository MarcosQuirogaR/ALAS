// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/analysis/airfoil_screening.py
// Reference: alas @ rust-port-baseline.

//! Screening runner orchestrating Stage 1 (2-D), Stage 2 (3-D), and Stage 3 (MSES).

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
use crate::score::{cruise_condition, score_candidate};
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
    should_cancel: Option<&dyn Fn() -> bool>,
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
    should_cancel: Option<&dyn Fn() -> bool>,
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

// The private dispatcher carries the explicit parity/product mode seam.
#[allow(clippy::too_many_arguments)]
fn run_airfoil_screening_with_mass_model(
    config: &AlasConfig,
    dv: Option<&DesignVector>,
    options: &AirfoilScreeningOptions,
    mses_dir: Option<&Path>,
    mut progress_callback: Option<&mut dyn FnMut(&str)>,
    should_cancel: Option<&dyn Fn() -> bool>,
    mass_model: ScreeningMassModel,
) -> Result<AirfoilScreeningResult, String> {
    let dv_val = dv.copied().unwrap_or_default();
    let (mach, reynolds, level_flight_cl, altitude) = cruise_condition(config, &dv_val)?;
    let section_mach = mach * dv_val.sweep_deg.to_radians().cos();
    let cl_target = options.target_cl.unwrap_or(level_flight_cl);
    if !cl_target.is_finite() || cl_target <= 0.0 {
        return Err("screening target CL must be finite and positive".to_owned());
    }
    let flow_regime = ScreeningFlowRegime::from_section_mach(section_mach);
    let (ld_weight, fuel_weight, robustness_weight) = options.objective.weights((
        options.ld_weight,
        options.fuel_weight,
        options.robustness_weight,
    ));

    let all_names = AirfoilLibrary::get_available_airfoils();
    let names = filter_names(&all_names, &options.name_filter);
    let n_total = names.len();

    let n_alpha = (((options.alpha_max_deg - options.alpha_min_deg)
        / options.alpha_step_deg.max(0.1))
    .round() as usize)
        + 1;
    let alphas_deg = linspace(options.alpha_min_deg, options.alpha_max_deg, n_alpha);

    let model_size = ModelSize::from_name(&options.model_size).unwrap_or(ModelSize::Large);

    let mut results: Vec<AirfoilCandidateResult> = Vec::new();
    let mut errors: Vec<HashMap<String, String>> = Vec::new();
    let mut cancelled = false;

    // Stage 1: 2-D screening
    for (i, name) in names.iter().enumerate() {
        if let Some(cancel_fn) = should_cancel {
            if cancel_fn() {
                cancelled = true;
                break;
            }
        }

        let cand = score_candidate(
            name,
            config,
            &dv_val,
            section_mach,
            reynolds,
            cl_target,
            config.mass_model.fuel_tank_usable_fraction,
            &alphas_deg,
            model_size,
            options.min_tc,
            options.max_tc,
            options.cl_band,
        );

        if cand.status == "ok" {
            results.push(cand);
        } else {
            let mut err_map = HashMap::new();
            err_map.insert("name".to_string(), name.clone());
            err_map.insert(
                "error".to_string(),
                cand.error.unwrap_or_else(|| "unknown error".to_string()),
            );
            errors.push(err_map);
        }

        if let Some(ref mut cb) = progress_callback {
            if (i + 1) % 50 == 0 || i + 1 == n_total {
                let msg = format!(
                    "Stage 1 (2-D): {}/{} evaluated -- {} ok, {} errors",
                    i + 1,
                    n_total,
                    results.len(),
                    errors.len()
                );
                cb(&msg);
            }
        }
    }

    let n_ok_stage1 = results.len();

    if !results.is_empty() {
        blend_scores(
            &mut results,
            ld_weight,
            fuel_weight,
            robustness_weight,
            |r| r.l_over_d.unwrap_or(0.0),
            false,
        );
        results.sort_by(|a, b| b.score.unwrap_or(0.0).total_cmp(&a.score.unwrap_or(0.0)));
    }

    let reference_set: HashSet<&str> = REFERENCE_AIRFOILS.iter().copied().collect();
    for r in &mut results {
        if reference_set.contains(r.name.as_str()) {
            r.is_reference = true;
        }
    }

    // Stage 2: 3-D refinement
    let mut n_refined = 0;
    if options.refine_3d && !results.is_empty() && options.refine_top_n > 0 && !cancelled {
        let top_n_limit = options.refine_top_n.min(results.len());
        let mut shortlist_indices: Vec<usize> = (0..top_n_limit).collect();
        for (idx, r) in results.iter().enumerate().skip(top_n_limit) {
            if r.is_reference && !shortlist_indices.contains(&idx) {
                shortlist_indices.push(idx);
            }
        }

        for (step, &idx) in shortlist_indices.iter().enumerate() {
            if let Some(cancel_fn) = should_cancel {
                if cancel_fn() {
                    cancelled = true;
                    break;
                }
            }

            refine_candidate_3d_with_mass_model(
                &mut results[idx],
                config,
                &dv_val,
                mach,
                altitude,
                cl_target,
                options.min_static_margin,
                mass_model,
            );

            if results[idx].refined {
                n_refined += 1;
            }

            if let Some(ref mut cb) = progress_callback {
                let msg = format!(
                    "Stage 2 (3-D wing): {}/{} re-simulated -- {} ok",
                    step + 1,
                    shortlist_indices.len(),
                    n_refined
                );
                cb(&msg);
            }
        }

        let mut refined_candidates: Vec<AirfoilCandidateResult> =
            results.iter().filter(|r| r.refined).cloned().collect();

        if !refined_candidates.is_empty() {
            blend_scores(
                &mut refined_candidates,
                ld_weight,
                fuel_weight,
                robustness_weight,
                |r| r.l_over_d_3d.unwrap_or(0.0),
                true,
            );

            let refined_map: HashMap<String, f64> = refined_candidates
                .into_iter()
                .filter_map(|r| r.score_3d.map(|s| (r.name, s)))
                .collect();

            for r in &mut results {
                if let Some(&s3d) = refined_map.get(&r.name) {
                    r.score_3d = Some(s3d);
                }
            }
        }

        results.sort_by(|a, b| {
            let key_a = (
                a.refined,
                a.score_3d.unwrap_or(-1.0),
                a.score.unwrap_or(0.0),
            );
            let key_b = (
                b.refined,
                b.score_3d.unwrap_or(-1.0),
                b.score.unwrap_or(0.0),
            );
            key_b
                .partial_cmp(&key_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Stage 3: MSES verification
    let mut n_mses_verified = 0;
    if options.verify_mses && !results.is_empty() && options.mses_top_n > 0 && !cancelled {
        if let Some(dir) = mses_dir {
            let refined_indices: Vec<usize> = results
                .iter()
                .enumerate()
                .filter(|(_, r)| r.refined)
                .map(|(i, _)| i)
                .collect();

            let mses_limit = options.mses_top_n.min(refined_indices.len());
            let mut mses_indices: Vec<usize> =
                refined_indices.iter().take(mses_limit).copied().collect();

            for &idx in &refined_indices[mses_limit..] {
                if results[idx].is_reference && !mses_indices.contains(&idx) {
                    mses_indices.push(idx);
                }
            }

            let cancellation = Arc::new(AtomicBool::new(false));
            if should_cancel.is_some_and(|cancel_fn| cancel_fn()) {
                cancellation.store(true, Ordering::Relaxed);
                cancelled = true;
            }
            let mut completed_mses = 0;
            let mut ordered_verified = vec![None; mses_indices.len()];
            let completed = run_bounded_indexed(
                mses_indices.len(),
                MAX_MSES_WORKERS,
                cancellation.clone(),
                |job_index| {
                    let idx = mses_indices[job_index];
                    let mut candidate = results[idx].clone();
                    let worker_cancellation = cancellation.clone();
                    let worker_cancel = || worker_cancellation.load(Ordering::Relaxed);
                    verify_candidate_mses(
                        &mut candidate,
                        config,
                        &dv_val,
                        mach,
                        altitude,
                        cl_target,
                        dir,
                        Some(&worker_cancel),
                    );
                    candidate
                },
                |job_index, candidate| {
                    ordered_verified[job_index] = Some(candidate.mses_verified);
                    while completed_mses < ordered_verified.len() {
                        let Some(verified) = ordered_verified[completed_mses].take() else {
                            break;
                        };
                        completed_mses += 1;
                        if verified {
                            n_mses_verified += 1;
                        }
                        if let Some(ref mut cb) = progress_callback {
                            let msg = format!(
                                "Stage 3 (MSES): {}/{} verified -- {} ok",
                                completed_mses,
                                mses_indices.len(),
                                n_mses_verified
                            );
                            cb(&msg);
                        }
                    }
                    if should_cancel.is_some_and(|cancel_fn| cancel_fn()) {
                        cancellation.store(true, Ordering::Relaxed);
                    }
                },
            );
            for (job_index, candidate) in completed {
                results[mses_indices[job_index]] = candidate;
            }
            n_mses_verified = mses_indices
                .iter()
                .filter(|&&index| results[index].mses_verified)
                .count();
            if cancellation.load(Ordering::Relaxed) {
                cancelled = true;
            }

            let mut verified: Vec<AirfoilCandidateResult> = results
                .iter()
                .filter(|r| r.mses_verified)
                .cloned()
                .collect();

            if !verified.is_empty() {
                verified.sort_by(|a, b| {
                    b.l_over_d_mses
                        .unwrap_or(0.0)
                        .total_cmp(&a.l_over_d_mses.unwrap_or(0.0))
                });
                let verified_names: HashSet<String> =
                    verified.iter().map(|r| r.name.clone()).collect();
                let rest: Vec<AirfoilCandidateResult> = results
                    .into_iter()
                    .filter(|r| !verified_names.contains(&r.name))
                    .collect();

                results = verified;
                results.extend(rest);
            }
        } else {
            mark_mses_not_configured(&mut results);
        }
    }

    let top_candidates: Vec<AirfoilCandidateResult> =
        results.into_iter().take(options.top_n).collect();

    Ok(AirfoilScreeningResult {
        baseline_airfoil: config.geometry.wing.root_airfoil.clone(),
        cruise_mach: mach,
        cruise_reynolds: reynolds,
        cruise_altitude_m: altitude,
        cl_target,
        uses_explicit_target_cl: options.target_cl.is_some(),
        section_mach,
        flow_regime,
        transonic_caveat: section_mach >= TRANSONIC_MACH_CAVEAT,
        refined_3d: options.refine_3d && n_refined > 0,
        n_total,
        n_ok: n_ok_stage1,
        n_error: errors.len(),
        n_refined,
        n_mses_verified,
        cancelled,
        candidates: top_candidates,
        errors: errors.into_iter().take(100).collect(),
    })
}

/// Run indexed jobs with a bounded worker count and return results by index.
///
/// Workers claim indices atomically and only the coordinator invokes the
/// progress callback. This keeps cancellation and callback ordering stable,
/// while sorting the collected results prevents completion timing from
/// changing the ranked candidate order.
fn run_bounded_indexed<T, F, P>(
    job_count: usize,
    worker_limit: usize,
    cancellation: Arc<AtomicBool>,
    job: F,
    mut on_result: P,
) -> Vec<(usize, T)>
where
    T: Send,
    F: Fn(usize) -> T + Sync,
    P: FnMut(usize, &T),
{
    if job_count == 0 || cancellation.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let workers = job_count.min(worker_limit.max(1));
    let next = Arc::new(AtomicUsize::new(0));
    let (sender, receiver) = channel();
    let mut output = Vec::with_capacity(job_count);
    thread::scope(|scope| {
        let job_ref = &job;
        for _ in 0..workers {
            let sender = sender.clone();
            let next = next.clone();
            let cancellation = cancellation.clone();
            scope.spawn(move || loop {
                if cancellation.load(Ordering::Relaxed) {
                    break;
                }
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= job_count {
                    break;
                }
                let value = job_ref(index);
                if sender.send((index, value)).is_err() {
                    break;
                }
            });
        }
        drop(sender);
        for (index, value) in receiver {
            on_result(index, &value);
            output.push((index, value));
        }
    });
    output.sort_by_key(|(index, _)| *index);
    output
}

/// Preserve the ranking while making a selected-but-unresolved Stage 3 visible.
fn mark_mses_not_configured(candidates: &mut [AirfoilCandidateResult]) {
    for candidate in candidates.iter_mut().filter(|candidate| candidate.refined) {
        candidate.mses_status = Some("not_configured".to_owned());
        candidate.mses_error = Some(
            "MSES verification was selected, but no MSES installation was resolved. Configure Setup > External Tools."
                .to_owned(),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use super::{mark_mses_not_configured, run_bounded_indexed};
    use crate::types::AirfoilCandidateResult;

    #[test]
    fn selected_mses_stage_reports_an_unresolved_installation_per_finalist() {
        let mut candidates = vec![
            AirfoilCandidateResult {
                name: "finalist".to_owned(),
                refined: true,
                ..AirfoilCandidateResult::default()
            },
            AirfoilCandidateResult {
                name: "proxy_only".to_owned(),
                refined: false,
                ..AirfoilCandidateResult::default()
            },
        ];

        mark_mses_not_configured(&mut candidates);

        assert_eq!(candidates[0].mses_status.as_deref(), Some("not_configured"));
        assert!(candidates[0]
            .mses_error
            .as_deref()
            .is_some_and(|message| message.contains("Setup > External Tools")));
        assert!(candidates[1].mses_status.is_none());
    }

    #[test]
    fn bounded_scheduler_returns_index_order_and_keeps_job_errors_isolated() {
        let output = run_bounded_indexed(
            8,
            3,
            Arc::new(AtomicBool::new(false)),
            |index| {
                thread::sleep(Duration::from_millis((8 - index) as u64));
                if index == 3 {
                    Err("controlled failure")
                } else {
                    Ok(index)
                }
            },
            |_, _| {},
        );
        let indices: Vec<usize> = output.iter().map(|(index, _)| *index).collect();
        assert_eq!(indices, (0..8).collect::<Vec<_>>());
        assert!(output.iter().any(|(index, result)| {
            *index == 3
                && result
                    .as_ref()
                    .is_err_and(|error| *error == "controlled failure")
        }));
        assert_eq!(
            output.iter().filter(|(_, result)| result.is_ok()).count(),
            7
        );
    }

    #[test]
    fn bounded_scheduler_caps_concurrency_and_honors_coordinator_cancellation() {
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let cancellation = Arc::new(AtomicBool::new(false));
        let mut completed = 0;
        let active_for_job = active.clone();
        let maximum_for_job = maximum.clone();
        let cancellation_for_callback = cancellation.clone();
        let output = run_bounded_indexed(
            32,
            3,
            cancellation,
            move |index| {
                let now = active_for_job.fetch_add(1, Ordering::Relaxed) + 1;
                maximum_for_job.fetch_max(now, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(2));
                active_for_job.fetch_sub(1, Ordering::Relaxed);
                index
            },
            move |_, _| {
                completed += 1;
                if completed >= 4 {
                    cancellation_for_callback.store(true, Ordering::Relaxed);
                }
            },
        );
        assert!(maximum.load(Ordering::Relaxed) <= 3);
        assert!(output.len() < 32);
        assert!(output.len() >= 4);
    }
}
