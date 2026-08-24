// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Independent VLM and AVL optimization branches.
//!
//! The native VLM objective remains the reference product path. The AVL
//! branch uses the same design vector, geometry builder, and hard feasibility
//! checks, but scores the admitted AVL Trefftz induced drag at the required
//! cruise lift. Keeping the branches in separate output namespaces means a
//! slow or unavailable external executable cannot corrupt the VLM result.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_opt::{DesignOptimizer, ObjectiveEvaluation, ObjectiveEvaluator, OptimizationResult};

use crate::avl::{run_avl_analysis, AvlAnalysisResult, AvlAnalysisStatus};
use crate::full_analysis::{AnalysisReport, FullAnalysis};
use crate::solver_mode::{OptimizationSolverMode, SolverKind};

/// Lifecycle state of one requested optimization branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolverOptimizationStatus {
    /// This backend was not selected for the run.
    NotRequested,
    /// The backend produced a design and a full report.
    Completed,
    /// The selected backend could not produce a usable result.
    Failed,
}

impl SolverOptimizationStatus {
    /// Stable spelling for run manifests and UI diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotRequested => "not_requested",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// One independently optimized aircraft solution.
#[derive(Debug, Clone, PartialEq)]
pub struct SolverOptimizationResult {
    /// Concrete backend that produced this solution.
    pub solver: SolverKind,
    /// Explicit lifecycle state.
    pub status: SolverOptimizationStatus,
    /// Best design vector, when the branch completed.
    pub design: Option<DesignVector>,
    /// Optimizer trajectory, when the branch completed.
    pub optimization: Option<OptimizationResult>,
    /// Full report for this branch's best design.
    pub report: Option<AnalysisReport>,
    /// Retained AVL run for the AVL branch's best design.
    pub avl_result: Option<AvlAnalysisResult>,
    /// Branch-local output directory, when one is configured.
    pub output_dir: Option<PathBuf>,
    /// Actionable failure detail, present only for a failed branch.
    pub error: Option<String>,
}

impl SolverOptimizationResult {
    fn not_requested(solver: SolverKind) -> Self {
        Self {
            solver,
            status: SolverOptimizationStatus::NotRequested,
            design: None,
            optimization: None,
            report: None,
            avl_result: None,
            output_dir: None,
            error: None,
        }
    }

    fn failed(solver: SolverKind, output_dir: Option<PathBuf>, error: impl Into<String>) -> Self {
        Self {
            solver,
            status: SolverOptimizationStatus::Failed,
            design: None,
            optimization: None,
            report: None,
            avl_result: None,
            output_dir,
            error: Some(error.into()),
        }
    }
}

/// Run-scoped pair of independently optimized aircraft solutions.
#[derive(Debug, Clone, PartialEq)]
pub struct SolverOptimizationSet {
    /// Native VLM optimization branch.
    pub vlm: SolverOptimizationResult,
    /// External AVL optimization branch.
    pub avl: SolverOptimizationResult,
}

impl SolverOptimizationSet {
    /// Select the requested primary solution, without silently falling back
    /// from an explicitly AVL-only request.
    pub fn selected(
        &self,
        mode: OptimizationSolverMode,
    ) -> Result<&SolverOptimizationResult, String> {
        match mode {
            OptimizationSolverMode::Vlm => self.completed_or_error(&self.vlm),
            OptimizationSolverMode::Avl => self.completed_or_error(&self.avl),
            OptimizationSolverMode::Both => {
                if self.vlm.status == SolverOptimizationStatus::Completed {
                    Ok(&self.vlm)
                } else {
                    self.completed_or_error(&self.avl)
                }
            }
        }
    }

    fn completed_or_error<'a>(
        &self,
        result: &'a SolverOptimizationResult,
    ) -> Result<&'a SolverOptimizationResult, String> {
        if result.status == SolverOptimizationStatus::Completed {
            Ok(result)
        } else {
            Err(result.error.clone().unwrap_or_else(|| {
                format!(
                    "{} optimization did not produce a usable result",
                    result.solver.as_str()
                )
            }))
        }
    }
}

/// Run the requested optimizer branches, optionally in parallel.
// Each argument is an independent runtime control or typed input boundary;
// keeping them explicit makes the pipeline call site auditable and avoids a
// configuration object that could silently mix the two solver namespaces.
#[allow(clippy::too_many_arguments)]
pub fn run_solver_optimizations(
    config: &AlasConfig,
    mode: OptimizationSolverMode,
    parallel: bool,
    seed: Option<u64>,
    environment: &RunEnvironment,
    nominal_design: &DesignVector,
    bounds: Option<&[(f64, f64)]>,
    output_dir: Option<&Path>,
) -> SolverOptimizationSet {
    let want_vlm = matches!(
        mode,
        OptimizationSolverMode::Vlm | OptimizationSolverMode::Both
    );
    let want_avl = matches!(
        mode,
        OptimizationSolverMode::Avl | OptimizationSolverMode::Both
    );
    let bounds = bounds.map(<[(f64, f64)]>::to_vec);
    let nominal = *nominal_design;
    let vlm_config = config.clone();
    let avl_config = config.clone();
    let vlm_environment = environment.clone();
    let avl_environment = environment.clone();
    let vlm_output = output_dir.map(|path| path.join("solvers/vlm"));
    let avl_output = output_dir.map(|path| path.join("solvers/avl"));

    let run_vlm = || {
        if want_vlm {
            run_vlm_optimizer(
                vlm_config,
                seed,
                vlm_environment,
                nominal,
                bounds.as_deref(),
                vlm_output,
            )
        } else {
            SolverOptimizationResult::not_requested(SolverKind::Vlm)
        }
    };
    let run_avl = || {
        if want_avl {
            run_avl_optimizer(
                avl_config,
                seed,
                avl_environment,
                nominal,
                bounds.as_deref(),
                avl_output,
            )
        } else {
            SolverOptimizationResult::not_requested(SolverKind::Avl)
        }
    };

    if parallel && want_vlm && want_avl {
        std::thread::scope(|scope| {
            let vlm = scope.spawn(run_vlm);
            let avl = scope.spawn(run_avl);
            SolverOptimizationSet {
                vlm: vlm.join().unwrap_or_else(|_| {
                    SolverOptimizationResult::failed(
                        SolverKind::Vlm,
                        None,
                        "VLM optimizer worker panicked",
                    )
                }),
                avl: avl.join().unwrap_or_else(|_| {
                    SolverOptimizationResult::failed(
                        SolverKind::Avl,
                        None,
                        "AVL optimizer worker panicked",
                    )
                }),
            }
        })
    } else {
        SolverOptimizationSet {
            vlm: run_vlm(),
            avl: run_avl(),
        }
    }
}

fn run_vlm_optimizer(
    config: AlasConfig,
    seed: Option<u64>,
    _environment: RunEnvironment,
    nominal: DesignVector,
    bounds: Option<&[(f64, f64)]>,
    output_dir: Option<PathBuf>,
) -> SolverOptimizationResult {
    let output_dir = create_branch_directory(output_dir);
    let effective_config = match seeded_config(&config, seed) {
        Ok(config) => config,
        Err(error) => return SolverOptimizationResult::failed(SolverKind::Vlm, output_dir, error),
    };
    let mut optimizer = DesignOptimizer::new(effective_config.clone());
    let optimization = optimizer.run(bounds, Some(&nominal), None);
    let design = optimization.best_design;
    let report = match FullAnalysis::new(config).run(&design, true) {
        Ok(report) => report,
        Err(error) => {
            return SolverOptimizationResult::failed(
                SolverKind::Vlm,
                output_dir,
                format!("VLM best-design analysis failed: {error}"),
            )
        }
    };
    SolverOptimizationResult {
        solver: SolverKind::Vlm,
        status: SolverOptimizationStatus::Completed,
        design: Some(design),
        optimization: Some(optimization),
        report: Some(report),
        avl_result: None,
        output_dir,
        error: None,
    }
}

fn run_avl_optimizer(
    config: AlasConfig,
    seed: Option<u64>,
    environment: RunEnvironment,
    nominal: DesignVector,
    bounds: Option<&[(f64, f64)]>,
    output_dir: Option<PathBuf>,
) -> SolverOptimizationResult {
    let Some(executable) = environment.avl_exe.clone() else {
        return SolverOptimizationResult::failed(
            SolverKind::Avl,
            output_dir,
            "AVL optimization requested but no native AVL executable is configured",
        );
    };
    let Some(output_root) = output_dir.clone() else {
        return SolverOptimizationResult::failed(
            SolverKind::Avl,
            None,
            "AVL optimization requires an output directory to retain solver evidence",
        );
    };
    let _ = std::fs::create_dir_all(&output_root);
    let effective_config = match seeded_config(&config, seed) {
        Ok(config) => config,
        Err(error) => return SolverOptimizationResult::failed(SolverKind::Avl, output_dir, error),
    };
    let mut objective = AvlObjective::new(
        config.clone(),
        executable.clone(),
        output_root.join("evaluations"),
    );
    let mut optimizer = DesignOptimizer::new(effective_config);
    let optimization = optimizer.run_with_evaluator(bounds, Some(&nominal), &mut objective, None);
    let design = optimization.best_design;
    let report = match FullAnalysis::new(config.clone()).run(&design, true) {
        Ok(report) => report,
        Err(error) => {
            return SolverOptimizationResult::failed(
                SolverKind::Avl,
                output_dir,
                format!("AVL best-design analysis failed: {error}"),
            )
        }
    };
    let final_dir = output_root.join("final");
    let avl_result = run_avl_analysis(&report, &config, &final_dir, Some(&executable), 300.0);
    if avl_result.status != AvlAnalysisStatus::CompletedComparable {
        return SolverOptimizationResult::failed(
            SolverKind::Avl,
            output_dir,
            format!(
                "AVL best-design output is not comparable: {}",
                avl_result
                    .error
                    .as_deref()
                    .unwrap_or(avl_result.status.as_str())
            ),
        );
    }
    SolverOptimizationResult {
        solver: SolverKind::Avl,
        status: SolverOptimizationStatus::Completed,
        design: Some(design),
        optimization: Some(optimization),
        report: Some(report),
        avl_result: Some(avl_result),
        output_dir,
        error: None,
    }
}

fn create_branch_directory(output_dir: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = &output_dir {
        let _ = std::fs::create_dir_all(path);
    }
    output_dir
}

fn seeded_config(config: &AlasConfig, seed: Option<u64>) -> Result<AlasConfig, String> {
    let mut effective = config.clone();
    if let Some(seed) = seed {
        effective.optimizer.solver.seed = Some(
            i64::try_from(seed)
                .map_err(|_| "optimizer seed exceeds the supported integer range")?,
        );
    }
    Ok(effective)
}

struct AvlObjective {
    config: AlasConfig,
    executable: PathBuf,
    output_root: PathBuf,
    cache: BTreeMap<String, ObjectiveEvaluation>,
}

impl AvlObjective {
    fn new(config: AlasConfig, executable: PathBuf, output_root: PathBuf) -> Self {
        Self {
            config,
            executable,
            output_root,
            cache: BTreeMap::new(),
        }
    }

    fn cache_key(design: &DesignVector) -> String {
        let mut hash = 0xcbf29ce484222325_u64;
        for value in design.to_array() {
            for byte in value.to_bits().to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3_u64);
            }
        }
        format!("{hash:016x}")
    }
}

impl ObjectiveEvaluator for AvlObjective {
    fn evaluate(&mut self, design: &DesignVector) -> ObjectiveEvaluation {
        let key = Self::cache_key(design);
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        let evaluation = self.evaluate_uncached(design, &key);
        self.cache.insert(key, evaluation.clone());
        evaluation
    }
}

impl AvlObjective {
    fn evaluate_uncached(&self, design: &DesignVector, key: &str) -> ObjectiveEvaluation {
        let report = match FullAnalysis::new(self.config.clone()).run(design, true) {
            Ok(report) => report,
            Err(_) => return ObjectiveEvaluation::rejected(self.failure_cost(), "full_analysis"),
        };
        let projected_wing_area_m2 = report
            .airplane
            .wings
            .first()
            .map_or(f64::NAN, alas_geom::aircraft::wing::Wing::projected_area);
        let Some(area_penalty) = wing_area_excess_penalty(
            projected_wing_area_m2,
            self.config.requirements.max_wing_area_m2,
            self.config.optimizer.weights.area_penalty_scale,
        ) else {
            return ObjectiveEvaluation::rejected(self.failure_cost(), "wing_area_limit");
        };
        if report.cg_envelope_ok != Some(true) {
            return ObjectiveEvaluation::rejected(self.failure_cost(), "cg_envelope");
        }
        if report.static_margin < self.config.requirements.min_physical_static_margin {
            return ObjectiveEvaluation::rejected(self.failure_cost(), "static_margin");
        }
        let evaluation_dir = self.output_root.join(key);
        let avl = run_avl_analysis(
            &report,
            &self.config,
            &evaluation_dir,
            Some(&self.executable),
            300.0,
        );
        let Some(polar) = avl.comparable_polar() else {
            return ObjectiveEvaluation::rejected(self.failure_cost(), "avl_unavailable");
        };
        let Some(point) = polar.points.iter().min_by(|left, right| {
            (left.alpha_deg - report.design_point.alpha_deg)
                .abs()
                .total_cmp(&(right.alpha_deg - report.design_point.alpha_deg).abs())
        }) else {
            return ObjectiveEvaluation::rejected(self.failure_cost(), "avl_output");
        };
        if !point.lift_coefficient.is_finite()
            || !point.induced_drag_coefficient.is_finite()
            || point.induced_drag_coefficient <= 0.0
        {
            return ObjectiveEvaluation::rejected(self.failure_cost(), "avl_induced_drag");
        }
        let induced_l_over_d = point.lift_coefficient / point.induced_drag_coefficient;
        let lift_error = (point.lift_coefficient - report.design_point.cl).abs();
        let moment_penalty = point.pitching_moment_coefficient.abs();
        let cost = -induced_l_over_d + 100.0 * lift_error + 10.0 * moment_penalty + area_penalty;
        ObjectiveEvaluation {
            cost,
            valid: true,
            l_over_d: induced_l_over_d,
            span_m: report.airplane.b_ref,
            alpha_deg: point.alpha_deg,
            area_m2: report.airplane.s_ref,
            trim_ih_deg: report
                .trimmed_design_point
                .map_or(0.0, |trim| trim.trim_ih_deg),
            reject_reason: String::new(),
        }
    }

    fn failure_cost(&self) -> f64 {
        self.config.optimizer.weights.failure_cost
    }
}

fn wing_area_excess_penalty(
    projected_area_m2: f64,
    maximum_area_m2: f64,
    penalty_scale: f64,
) -> Option<f64> {
    if !projected_area_m2.is_finite()
        || !maximum_area_m2.is_finite()
        || maximum_area_m2 <= 0.0
        || !penalty_scale.is_finite()
    {
        return None;
    }
    let excess_fraction = ((projected_area_m2 - maximum_area_m2) / maximum_area_m2).max(0.0);
    Some(excess_fraction.powi(2) * penalty_scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avl_optimization_preserves_a_gradient_above_the_wing_area_limit() {
        assert_eq!(wing_area_excess_penalty(90.0, 100.0, 0.5), Some(0.0));
        assert_eq!(wing_area_excess_penalty(100.0, 100.0, 0.5), Some(0.0));
        let Some(slight_excess) = wing_area_excess_penalty(101.0, 100.0, 0.5) else {
            panic!("finite dimensions should produce a penalty");
        };
        let Some(larger_excess) = wing_area_excess_penalty(110.0, 100.0, 0.5) else {
            panic!("finite dimensions should produce a penalty");
        };
        assert!(slight_excess > 0.0);
        assert!(larger_excess > slight_excess);
        assert_eq!(wing_area_excess_penalty(f64::NAN, 100.0, 0.5), None);
    }

    #[test]
    fn both_mode_prefers_a_completed_vlm_branch_when_avl_is_unavailable() {
        let vlm = SolverOptimizationResult {
            solver: SolverKind::Vlm,
            status: SolverOptimizationStatus::Completed,
            design: None,
            optimization: None,
            report: None,
            avl_result: None,
            output_dir: None,
            error: None,
        };
        let avl = SolverOptimizationResult::failed(
            SolverKind::Avl,
            None,
            "AVL executable was not configured",
        );
        let set = SolverOptimizationSet { vlm, avl };

        assert_eq!(
            set.selected(OptimizationSolverMode::Both).map(|r| r.solver),
            Ok(SolverKind::Vlm)
        );
        assert!(set.selected(OptimizationSolverMode::Avl).is_err());
    }

    #[test]
    fn avl_only_mode_never_selects_a_failed_branch_as_a_vlm_fallback() {
        let set = SolverOptimizationSet {
            vlm: SolverOptimizationResult::not_requested(SolverKind::Vlm),
            avl: SolverOptimizationResult::failed(SolverKind::Avl, None, "missing executable"),
        };

        assert!(set.selected(OptimizationSolverMode::Avl).is_err());
    }
}
