// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The AVL optimization branch and its objective.

use super::*;

pub(super) fn run_avl_optimizer(
    config: AlasConfig,
    seed: Option<u64>,
    environment: RunEnvironment,
    nominal: DesignVector,
    bounds: Option<&[(f64, f64)]>,
    output_dir: Option<PathBuf>,
    cancel: Option<&AtomicBool>,
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
    // Every AVL evaluation writes its geometry and session files below this
    // root; without it each candidate would fail later with a less direct
    // error, so the branch fails here with the cause.
    if let Err(error) = std::fs::create_dir_all(&output_root) {
        return SolverOptimizationResult::failed(
            SolverKind::Avl,
            output_dir,
            format!(
                "AVL optimization output directory {} could not be created: {error}",
                output_root.display()
            ),
        );
    }
    let effective_config = match seeded_config(&config, seed) {
        Ok(config) => config,
        Err(error) => return SolverOptimizationResult::failed(SolverKind::Avl, output_dir, error),
    };
    let mut objective = AvlObjective::new(
        config.clone(),
        executable.clone(),
        output_root.join("evaluations"),
        cancel,
    );
    let mut optimizer = DesignOptimizer::new(effective_config);
    let optimization = match optimizer.run_with_evaluator_cancellable(
        bounds,
        Some(&nominal),
        &mut objective,
        None,
        cancel,
    ) {
        Ok(result) => result,
        Err(error) => {
            return SolverOptimizationResult::failed(
                SolverKind::Avl,
                output_dir,
                format!("AVL optimization failed: {error}"),
            )
        }
    };
    if optimization.was_cancelled() {
        write_cancelled_search_record(output_dir.as_deref(), &optimization, cancel);
        return SolverOptimizationResult::failed(
            SolverKind::Avl,
            output_dir,
            CANCELLED_DURING_OPTIMIZATION,
        );
    }
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
    let avl_result = crate::avl::run_avl_analysis_cancellable(
        &report,
        &config,
        &final_dir,
        Some(&executable),
        config.analysis.avl_timeout_s,
        cancel,
    );
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
    let status = SolverOptimizationStatus::for_delivered(&optimization);
    SolverOptimizationResult {
        solver: SolverKind::Avl,
        status,
        design: Some(design),
        optimization: Some(optimization),
        report: Some(report),
        avl_result: Some(avl_result),
        output_dir,
        error: None,
    }
}

/// The AVL-backed objective: one external solver process per uncached
/// candidate.
///
/// This is the only optimizer evaluation path in the product that spawns a
/// process, so it is the only one whose cancellation bound is set by
/// something other than an internal analysis. `alas-exec` owns the child - it
/// polls it every 25 ms and force-kills the whole process tree on its
/// deadline - and exposes no cancellation flag, so this objective bounds the
/// drain the two ways available to a caller: it refuses to *start* a sweep
/// once cancellation has been requested, and it runs each sweep under
/// [`AVL_EVALUATION_TIMEOUT_S`] rather than the comparison deadline.
struct AvlObjective<'a> {
    config: AlasConfig,
    objective: DesignObjective,
    executable: PathBuf,
    output_root: PathBuf,
    cache: BTreeMap<Vec<u64>, ObjectiveEvaluation>,
    scope: alas_opt::CancelScope<'a>,
}

impl<'a> AvlObjective<'a> {
    fn new(
        config: AlasConfig,
        executable: PathBuf,
        output_root: PathBuf,
        cancel: Option<&'a AtomicBool>,
    ) -> Self {
        Self {
            objective: DesignObjective::new(config.clone()),
            config,
            executable,
            output_root,
            cache: BTreeMap::new(),
            scope: alas_opt::CancelScope::attach(cancel),
        }
    }

    /// Short, stable directory name for one design's AVL evidence (64-bit
    /// FNV-1a over the coordinate bits).
    fn evaluation_dir_name(bits: &[u64]) -> String {
        let mut hash = 0xcbf29ce484222325_u64;
        for value in bits {
            for byte in value.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3_u64);
            }
        }
        format!("{hash:016x}")
    }
}

impl ObjectiveEvaluator for AvlObjective<'_> {
    fn evaluate(&mut self, design: &DesignVector) -> ObjectiveEvaluation {
        // Keyed by the exact coordinate bits, so two designs can never share
        // an evaluation; only the directory name is hashed.
        let bits: Vec<u64> = design.to_array().into_iter().map(f64::to_bits).collect();
        if let Some(cached) = self.cache.get(&bits) {
            return cached.clone();
        }
        let evaluation = self.evaluate_uncached(design, &Self::evaluation_dir_name(&bits));
        self.cache.insert(bits, evaluation.clone());
        evaluation
    }
}

impl AvlObjective<'_> {
    /// Score one candidate with the mission-sized objective around AVL's
    /// aerodynamics: AVL supplies the induced drag at the required cruise
    /// lift, while the parasite build-up, trim incidence and neutral point
    /// stay the native report's, and the sizing loop closes mass, fuel and
    /// takeoff mass around that fixed polar.
    ///
    /// Cancellation is read twice here: once before the native analysis and
    /// once immediately before the external sweep is launched. A candidate
    /// refused at either point is rejected with a reason that names
    /// cancellation, so a stopped search cannot be read as a design space
    /// where AVL failed.
    fn evaluate_uncached(&self, design: &DesignVector, key: &str) -> ObjectiveEvaluation {
        if self.scope.requested() {
            self.scope
                .work_skipped("AVL candidate not started on the cancellation request");
            return ObjectiveEvaluation::rejected(self.failure_cost(), "cancelled");
        }
        let Ok((sized_config, sized_design)) = sized_candidate(
            &self.config,
            design,
            self.objective.preserves_explicit_fuselage_length(),
        ) else {
            return ObjectiveEvaluation::rejected(self.failure_cost(), "geometry_build");
        };
        let analysis = FullAnalysis::new(sized_config);
        let report = match analysis.run(&sized_design, true) {
            Ok(report) => report,
            Err(_) => return ObjectiveEvaluation::rejected(self.failure_cost(), "full_analysis"),
        };
        if self.scope.requested() {
            // The deck is not written and no process is spawned: the drain
            // stops here instead of waiting out a sweep nobody will read.
            self.scope
                .external_skipped("AVL sweep not launched on the cancellation request");
            return ObjectiveEvaluation::rejected(self.failure_cost(), "cancelled");
        }
        let evaluation_dir = self.output_root.join(key);
        self.scope
            .enter(alas_opt::CancelPhase::ExternalSolverCall, 0);
        self.scope
            .external_started(format!("AVL sweep for candidate {key}"));
        let avl = run_avl_analysis(
            &report,
            &self.config,
            &evaluation_dir,
            Some(&self.executable),
            AVL_EVALUATION_TIMEOUT_S,
        );
        if avl.status == AvlAnalysisStatus::TimedOut {
            self.scope.external_terminated(format!(
                "AVL sweep for candidate {key} exceeded {AVL_EVALUATION_TIMEOUT_S:.0} s and its \
                 process tree was killed"
            ));
            return ObjectiveEvaluation::rejected(self.failure_cost(), "avl_timed_out");
        }
        let Some(polar) = avl.comparable_polar() else {
            return ObjectiveEvaluation::rejected(self.failure_cost(), "avl_unavailable");
        };
        let required_cl = analysis.cruise_cl(&report.airplane);
        let Some(point) = interpolate_avl_at_lift(polar, required_cl) else {
            return ObjectiveEvaluation::rejected(
                self.failure_cost(),
                "avl_required_lift_out_of_range",
            );
        };
        if !point.induced_drag_coefficient.is_finite() || point.induced_drag_coefficient <= 0.0 {
            return ObjectiveEvaluation::rejected(self.failure_cost(), "avl_induced_drag");
        }
        let cd0 = report.polar_fit.cd0;
        let wave_drag_cd = report
            .polar
            .cl
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                (**left - required_cl)
                    .abs()
                    .total_cmp(&(**right - required_cl).abs())
            })
            .and_then(|(index, _)| report.polar.cd_wave.get(index).copied())
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(0.0);
        let polar = ExternalPolar {
            cd0,
            induced_factor_k: point.induced_drag_coefficient / (required_cl * required_cl),
            wave_drag_cd,
            lift_to_drag: required_cl / (cd0 + wave_drag_cd + point.induced_drag_coefficient),
            alpha_deg: point.alpha_deg,
            incidence_deg: report
                .trimmed_design_point
                .map_or(0.0, |trim| trim.trim_ih_deg),
            x_np: report.x_neutral_point,
            // Evaluation identity, so this polar cannot be flown at another
            // state: AVL's own run Mach (a mis-commanded run is caught, not
            // relabelled), the requirement altitude (AVL has no atmosphere),
            // and the area the coefficients are referred to.
            mach: point.mach,
            altitude_m: self.config.requirements.cruise_altitude_m,
            reference_area_m2: report.airplane.s_ref,
            target_cl: required_cl,
            source: "avl",
            bracketed: true,
        };
        match alas_opt::assess_candidate_with_polar_cancellable(
            &self.objective,
            &design.to_array(),
            &polar,
            self.scope.flag(),
        ) {
            Ok(assessment) => ObjectiveEvaluation {
                cost: assessment.cost,
                valid: assessment.hard_feasible,
                l_over_d: polar.lift_to_drag,
                span_m: report.airplane.b_ref,
                alpha_deg: point.alpha_deg,
                area_m2: report.airplane.s_ref,
                trim_ih_deg: polar.incidence_deg,
                reject_reason: assessment.violated_hard_ids().join("+"),
            },
            Err(reason) => ObjectiveEvaluation::rejected(self.failure_cost(), reason),
        }
    }

    fn failure_cost(&self) -> f64 {
        self.config.optimizer.weights.failure_cost
    }
}

/// Interpolate the AVL polar at the required cruise lift coefficient.
///
/// The AVL branch is an induced-drag objective at a prescribed lift state; a
/// nearest-alpha lookup changes that state whenever the alpha grid or design
/// lift curve moves. Only a bracketed finite pair is admitted, so an AVL run
/// that does not cover the required lift is rejected instead of extrapolated.
pub(super) fn interpolate_avl_at_lift(polar: &AvlPolar, target_cl: f64) -> Option<AvlPolarPoint> {
    if !target_cl.is_finite() {
        return None;
    }
    for point in &polar.points {
        if point.lift_coefficient == target_cl && finite_avl_objective_point(point) {
            return Some(*point);
        }
    }
    for pair in polar.points.windows(2) {
        let [left, right] = pair else {
            continue;
        };
        let delta_cl = right.lift_coefficient - left.lift_coefficient;
        if !finite_avl_objective_point(left)
            || !finite_avl_objective_point(right)
            || !delta_cl.is_finite()
            || delta_cl == 0.0
            || (target_cl - left.lift_coefficient) * (target_cl - right.lift_coefficient) > 0.0
        {
            continue;
        }
        let fraction = (target_cl - left.lift_coefficient) / delta_cl;
        let lerp = |a: f64, b: f64| a + fraction * (b - a);
        let point = AvlPolarPoint {
            alpha_deg: lerp(left.alpha_deg, right.alpha_deg),
            beta_deg: lerp(left.beta_deg, right.beta_deg),
            mach: lerp(left.mach, right.mach),
            lift_coefficient: target_cl,
            total_drag_coefficient: lerp(left.total_drag_coefficient, right.total_drag_coefficient),
            induced_drag_coefficient: lerp(
                left.induced_drag_coefficient,
                right.induced_drag_coefficient,
            ),
            pitching_moment_coefficient: lerp(
                left.pitching_moment_coefficient,
                right.pitching_moment_coefficient,
            ),
            span_efficiency: match (left.span_efficiency, right.span_efficiency) {
                (Some(a), Some(b)) if a.is_finite() && b.is_finite() => Some(lerp(a, b)),
                _ => None,
            },
        };
        return finite_avl_objective_point(&point).then_some(point);
    }
    None
}

fn finite_avl_objective_point(point: &AvlPolarPoint) -> bool {
    [
        point.alpha_deg,
        point.beta_deg,
        point.mach,
        point.lift_coefficient,
        point.total_drag_coefficient,
        point.induced_drag_coefficient,
        point.pitching_moment_coefficient,
    ]
    .iter()
    .all(|value| value.is_finite())
}

/// The configuration and design vector an AVL candidate is drawn with: the
/// tail the coupled assessment sizes, masses and trims, resolved before the
/// geometry is built so the external aerodynamics and the mass model
/// describe the same aircraft.
pub(super) fn sized_candidate(
    config: &AlasConfig,
    design: &DesignVector,
    preserve_explicit_fuselage_length: bool,
) -> Result<(AlasConfig, DesignVector), String> {
    let (mut sized_design, tail_sizing) =
        alas_opt::resolve_tail_sizing(config, design, preserve_explicit_fuselage_length)?;
    let mut sized_config = config.clone();
    tail_sizing.apply_to(&mut sized_config.geometry.empennage, &mut sized_design);
    Ok((sized_config, sized_design))
}
