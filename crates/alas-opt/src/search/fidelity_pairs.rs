// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The same candidates scored by a screening model and by the full in-loop
//! model, for the rank-correlation experiment that decides whether a
//! [`ScreeningFidelity`] may ship (`alas-acceptance`,
//! `screening_rank_correlation`). This module only evaluates; the statistics
//! live with the experiment.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use alas_config::AlasConfig;
use rayon::ThreadPoolBuilder;

use super::screening::{self, ScreeningFidelity};
use crate::mdo::{
    evaluate_mission_sized_with_assessment, CandidateAssessment, ConstraintResidual, SizingWork,
};
use crate::{DesignObjective, DesignOptimizer, OptimizationError};

/// One model's verdict on one candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateScore {
    /// The scalar the search ranks by.
    pub cost: f64,
    /// Whether every hard residual was met with nothing relaxed.
    pub feasible: bool,
    /// Whether the coupled analysis produced a residual table at all.
    pub analysed: bool,
    /// Sum of normalized hard violations (dimensionless).
    pub hard_violation: f64,
    /// The configured mission objective (e.g. block fuel, kg).
    pub objective_value: f64,
    /// Closed takeoff mass, kg.
    pub takeoff_mass_kg: f64,
    /// Every residual's normalized violation, by identifier.
    pub violations: Vec<(&'static str, f64)>,
    /// Complete physical residuals, retaining units and signed margins.
    pub residuals: Vec<ConstraintResidual>,
    /// Rejection before a residual table could be assembled.
    pub failure_reason: Option<String>,
    /// Wall time of this one evaluation, s.
    pub wall_time_s: f64,
    /// Work of the sizing closure, when the candidate sized.
    pub work: Option<SizingWork>,
}

/// A candidate and its two scores.
#[derive(Debug, Clone, PartialEq)]
pub struct FidelityPair {
    /// The candidate design vector.
    pub design: Vec<f64>,
    /// Its score under the screening model.
    pub screening: CandidateScore,
    /// Its score under the full in-loop model.
    pub full: CandidateScore,
}

/// Score the first `count` points of the screening sample for `seed` (the
/// baseline first) with `fidelity` and with the full in-loop model, spread
/// over `workers` threads. The points are the ones a product run with this
/// configuration would screen.
///
/// # Errors
///
/// The optimizer's own refusal of the configuration (invalid design space or
/// solver settings), before anything is evaluated.
pub fn compare_fidelities(
    config: &AlasConfig,
    fidelity: ScreeningFidelity,
    count: usize,
    seed: u64,
    workers: usize,
) -> Result<Vec<FidelityPair>, OptimizationError> {
    let (bounds, nominal) = DesignOptimizer::new(config.clone()).anchored_search_space()?;
    let projection = super::planform_projection::PlanformProjection::new(config, &bounds);
    let points = screening::sample(
        &bounds,
        Some(&nominal.to_array()),
        count,
        seed,
        projection.as_ref(),
    );
    let mut screening_model =
        DesignObjective::new_with_nominal(fidelity.configure(config), nominal);
    screening_model.sizing_controls = fidelity.controls();
    let full_model = DesignObjective::new_with_nominal(config.clone(), nominal);
    let next = AtomicUsize::new(0);
    let results = Mutex::new(vec![None; points.len()]);
    let workers = if workers == 0 {
        config.optimizer.solver.resolved_workers()
    } else {
        workers
    };
    let lanes = (0..workers.clamp(1, points.len().max(1)))
        .map(|_| ThreadPoolBuilder::new().num_threads(1).build())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            OptimizationError::InvalidConfiguration(format!(
                "could not create fidelity-comparison compute lanes: {error}"
            ))
        })?;
    std::thread::scope(|scope| {
        for lane in &lanes {
            let (next, results, points, screening_model, full_model) =
                (&next, &results, &points, &screening_model, &full_model);
            scope.spawn(move || {
                // Match the native optimizer: nested VLM parallel loops stay
                // on the candidate's one-thread lane.
                lane.install(|| loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(design) = points.get(index) else {
                        break;
                    };
                    let pair = FidelityPair {
                        design: design.clone(),
                        screening: score(screening_model, design),
                        full: score(full_model, design),
                    };
                    if let Ok(mut results) = results.lock() {
                        results[index] = Some(pair);
                    }
                });
            });
        }
    });
    Ok(results
        .into_inner()
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .collect())
}

fn score(model: &DesignObjective, design: &[f64]) -> CandidateScore {
    let mut objective = model.clone();
    let started = Instant::now();
    let (cost, assessment) = evaluate_mission_sized_with_assessment(&mut objective, design);
    let wall_time_s = started.elapsed().as_secs_f64();
    match assessment {
        Some(assessment) => from_assessment(cost, &assessment, wall_time_s),
        None => CandidateScore {
            cost,
            feasible: false,
            analysed: false,
            hard_violation: f64::INFINITY,
            objective_value: f64::NAN,
            takeoff_mass_kg: f64::NAN,
            violations: Vec::new(),
            residuals: Vec::new(),
            failure_reason: objective.history.reject_reason.last().cloned(),
            wall_time_s,
            work: None,
        },
    }
}

fn from_assessment(
    cost: f64,
    assessment: &CandidateAssessment,
    wall_time_s: f64,
) -> CandidateScore {
    CandidateScore {
        cost,
        feasible: assessment.is_strictly_feasible(),
        analysed: true,
        hard_violation: assessment.hard_violation_sum,
        objective_value: assessment.objective_value,
        takeoff_mass_kg: assessment.sized.takeoff_mass_kg,
        violations: assessment
            .residuals
            .iter()
            .map(|residual| (residual.id, residual.normalized_violation))
            .collect(),
        residuals: assessment.residuals.clone(),
        failure_reason: None,
        wall_time_s,
        work: Some(assessment.sized.work),
    }
}

/// Score `designs` with `fidelity` under `config`, spread over `workers`
/// one-thread lanes, in input order: the replay a fidelity-ladder study runs
/// on candidates a finished search recorded.
///
/// With `capped` every candidate is sized under the search's own work cap
/// (`search::work_cap`), measured once on the nominal with the full model, as
/// the product search does.
///
/// # Errors
///
/// The optimizer's own refusal of the configuration, before anything is
/// evaluated.
pub fn score_designs(
    config: &AlasConfig,
    fidelity: ScreeningFidelity,
    designs: &[Vec<f64>],
    workers: usize,
    capped: bool,
) -> Result<Vec<CandidateScore>, OptimizationError> {
    let (_, nominal) = DesignOptimizer::new(config.clone()).anchored_search_space()?;
    let mut model = DesignObjective::new_with_nominal(fidelity.configure(config), nominal);
    model.sizing_controls = fidelity.controls();
    if capped && model.sizing_controls.budget.is_none() {
        let full = DesignObjective::new_with_nominal(config.clone(), nominal);
        model.sizing_controls.budget = super::work_cap::nominal_work_cap(&full, &nominal);
    }
    let next = AtomicUsize::new(0);
    let results = Mutex::new(vec![None; designs.len()]);
    let lanes = (0..workers.clamp(1, designs.len().max(1)))
        .map(|_| ThreadPoolBuilder::new().num_threads(1).build())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            OptimizationError::InvalidConfiguration(format!(
                "could not create fidelity-replay compute lanes: {error}"
            ))
        })?;
    std::thread::scope(|scope| {
        for lane in &lanes {
            let (next, results, model) = (&next, &results, &model);
            scope.spawn(move || {
                lane.install(|| loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(design) = designs.get(index) else {
                        break;
                    };
                    let scored = score(model, design);
                    if let Ok(mut results) = results.lock() {
                        results[index] = Some(scored);
                    }
                });
            });
        }
    });
    Ok(results
        .into_inner()
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .collect())
}

/// Per-variable `(lower, upper)` bounds of a search box.
pub type SearchBox = Vec<(f64, f64)>;

/// The anchored search box and the canonical nominal design vector a product
/// run of `config` searches, for a study that proposes its own candidates.
///
/// # Errors
///
/// The optimizer's own refusal of the configuration.
pub fn anchored_box(config: &AlasConfig) -> Result<(SearchBox, Vec<f64>), OptimizationError> {
    let (bounds, nominal) = DesignOptimizer::new(config.clone()).anchored_search_space()?;
    Ok((bounds, nominal.to_array()))
}
