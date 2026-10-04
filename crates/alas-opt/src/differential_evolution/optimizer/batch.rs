// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The batch evaluator every search stage calls: the design-vector pre-gate,
//! exact reuse of repeated designs, then one batch through the objective.
//!
//! # Pre-gate
//!
//! [`pre_gate_violation`] runs design-vector checks before any analysis, in
//! microseconds: the search box (whose span window is already clamped to the
//! aerodrome code limit), then the main-wing planform the geometry builder
//! constructs with the same call (`WingConfig::transport_planform`), so a
//! chord that grows outboard, such as a pinned side-of-body chord below the
//! kink chord, is rejected exactly as the full build would reject it; then,
//! the hard exposed trailing-edge angle, the span code limit and the
//! declared maximum wing area. The planform and Geometry checks apply to the native
//! model only: a delegated evaluator owns its constraint set. A
//! rejected candidate is scored in the [`Tier::PreGateFailed`] tier with its
//! normalized violation, never counts against the stage's evaluation budget,
//! counts against its rejection cap instead and is reported by the first
//! check it failed ([`crate::PreGateReasons`]). Every check a design vector
//! alone decides belongs in that one function.

use super::*;
use crate::evaluation_trace::analysed_record;
use crate::mdo::TE_ANGLE_LIMIT_DEG;
use crate::search_methods::Tier;
use crate::{EvaluationTrace, TraceClass, TraceStage};
use crate::{PreGateReasons, SizingWorkSummary, StageRejections, StageSummary};

/// The first design-vector check a candidate failed, in the gate's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    DesignBox,
    Planform,
    TrailingEdgeAngle,
    SpanCode,
    WingArea,
}

impl Reason {
    pub(crate) fn count(self, reasons: &mut PreGateReasons) {
        *match self {
            Self::DesignBox => &mut reasons.design_box,
            Self::Planform => &mut reasons.planform,
            Self::TrailingEdgeAngle => &mut reasons.trailing_edge_angle,
            Self::SpanCode => &mut reasons.span_code,
            Self::WingArea => &mut reasons.wing_area,
        } += 1;
    }
}

/// The first failed check and the normalized violation of the design-vector
/// checks, `None` when all pass: the box violation
/// `sum(max(0, lo - x, x - hi) / width)` (a fixed coordinate counting its
/// absolute miss), else the planform's, else the Geometry residuals'.
pub(crate) fn pre_gate_violation(
    values: &[f64],
    bounds: &[(f64, f64)],
    config: Option<&AlasConfig>,
) -> Option<(Reason, f64)> {
    let mut violation = 0.0;
    for (&value, &(lower, upper)) in values.iter().zip(bounds) {
        if !value.is_finite() {
            return Some((Reason::DesignBox, f64::INFINITY));
        }
        let miss = (lower - value).max(value - upper).max(0.0);
        violation += miss / (upper - lower).max(1.0);
    }
    if violation > 0.0 || values.len() != bounds.len() {
        return Some((Reason::DesignBox, violation.max(f64::MIN_POSITIVE)));
    }
    let design = DesignVector::from_array(values).ok()?;
    let config = config?;
    planform_violation(config, &design)
        .map(|violation| (Reason::Planform, violation))
        .or_else(|| hard_geometry_violation(config, &design))
}

/// The three hard Geometry residuals a design vector alone decides: the
/// exposed trailing-edge angle
/// ([`crate::transport_planform::exposed_te_angle_deg`]), the aerodrome
/// code span limit ([`AlasConfig::max_design_span_m`]) and the declared
/// maximum wing area (`requirements.max_wing_area_m2`), each normalized by
/// its limit as the residual table does. The reason is the first that fails
/// in that order.
///
/// The area is the planform's own trapezoid sum
/// ([`alas_config::TransportPlanform::reference_area_m2`]). The built wing's
/// `s_ref` reproduces it to summation round-off and adds the projection of
/// any non-planar tip device, so it is never smaller; the gate rejects only
/// an excess beyond [`AREA_ROUNDOFF`], so every design it rejects is over
/// the limit in the residual table too, and the borderline ones and those a
/// tip device carries over are left to it.
fn hard_geometry_violation(config: &AlasConfig, design: &DesignVector) -> Option<(Reason, f64)> {
    let angle = crate::transport_planform::exposed_te_angle_deg(&config.geometry.wing, design)?;
    let excess_angle = (angle - TE_ANGLE_LIMIT_DEG) / TE_ANGLE_LIMIT_DEG;
    let excess_span = config
        .max_design_span_m()
        .map_or(0.0, |limit| (design.span_m - limit) / limit);
    let limit_m2 = config.requirements.max_wing_area_m2;
    let excess_area = match config.geometry.wing.transport_planform(design) {
        Ok(planform) if limit_m2.is_finite() && limit_m2 > 0.0 => {
            let excess = (planform.reference_area_m2() - limit_m2) / limit_m2;
            if excess > AREA_ROUNDOFF {
                excess
            } else {
                0.0
            }
        }
        _ => 0.0,
    };
    let violation = excess_angle.max(0.0) + excess_span.max(0.0) + excess_area;
    let reason = if excess_angle > 0.0 {
        Reason::TrailingEdgeAngle
    } else if excess_span > 0.0 {
        Reason::SpanCode
    } else {
        Reason::WingArea
    };
    (violation > 0.0).then_some((reason, violation))
}

/// Relative excess of the planform area over the declared maximum below
/// which the pre-gate defers to the full residual table: far above the
/// summation round-off between the planform and the lofted wing's `s_ref`
/// (`the_wing_area_pre_gate_agrees_with_the_built_reference_area`) and far
/// below any physical difference.
const AREA_ROUNDOFF: f64 = 1.0e-9;

/// A planform the builder cannot construct: a chord growing outboard by the
/// relative excess `(outboard - inboard) / outboard`, any other defect 1.
fn planform_violation(config: &AlasConfig, design: &DesignVector) -> Option<f64> {
    match config.geometry.wing.transport_planform(design) {
        Ok(_) => None,
        Err(alas_config::TransportPlanformError::NonMonotoneChord { inboard, outboard }) => {
            Some(((outboard - inboard) / outboard.abs().max(1.0e-9)).max(f64::MIN_POSITIVE))
        }
        Err(_) => Some(1.0),
    }
}

/// Evaluates one stage's batches through `objective`; every point is scored
/// in the input order whatever the worker count.
pub(super) struct BatchEvaluator<'a, E: SearchObjective + ?Sized> {
    pub(super) objective: &'a mut E,
    pub(super) workers: usize,
    pub(super) bounds: &'a [(f64, f64)],
    /// The configuration the design-vector pre-gate checks the planform of.
    pub(super) config: Option<&'a AlasConfig>,
    /// The baseline design, always analysed so the run has a same-model
    /// baseline to compare the winner with, whatever the pre-gate says.
    pub(super) baseline: Option<&'a [f64]>,
    pub(super) scope: CancelScope<'a>,
    pub(super) cache: evaluation_cache::EvaluationCache,
    pub(super) pre_gate_rejects: usize,
    /// The rejections by the first check each failed.
    pub(super) reasons: PreGateReasons,
    /// Distinct analysed designs that were strictly feasible.
    pub(super) feasible: usize,
    /// Lane wall time spent analysing candidates, summed over lanes.
    pub(super) busy: Duration,
    /// History rows of requested candidates no lane started before a
    /// cancellation ([`CANCELLED_UNSTARTED`]).
    pub(super) cancelled_unstarted: usize,
    /// The stage the next batches are recorded under in [`Self::trace`].
    pub(super) stage: TraceStage,
    /// Every requested candidate this evaluator rejected or analysed, for
    /// display; the search never reads it.
    pub(super) trace: EvaluationTrace,
}

/// The history reason of a requested candidate a cancellation stopped before
/// any analysis started. It keeps one history row per requested design, and
/// it is never an analysis.
pub(super) const CANCELLED_UNSTARTED: &str = crate::evaluation_trace::CANCELLED_UNSTARTED;

impl<'a, E: SearchObjective + ?Sized> BatchEvaluator<'a, E> {
    pub(super) fn new(
        objective: &'a mut E,
        workers: usize,
        bounds: &'a [(f64, f64)],
        config: Option<&'a AlasConfig>,
        cancel: Option<&'a AtomicBool>,
    ) -> Self {
        Self {
            objective,
            workers,
            bounds,
            config,
            baseline: None,
            scope: CancelScope::attach(cancel),
            cache: evaluation_cache::EvaluationCache::new(None),
            pre_gate_rejects: 0,
            reasons: PreGateReasons::default(),
            feasible: 0,
            busy: Duration::ZERO,
            cancelled_unstarted: 0,
            stage: TraceStage::Refinement,
            trace: EvaluationTrace::default(),
        }
    }

    /// `stage` completed with what this evaluator measured over the history
    /// rows from `before` in `wall_s` seconds, and the stage's rejections
    /// under the cap `max_rejects`.
    pub(super) fn close_stage(
        &self,
        stage: StageSummary,
        before: usize,
        wall_s: f64,
        max_rejects: usize,
    ) -> (StageSummary, StageRejections) {
        let analysed = self.analyses() - before;
        let busy = self.busy.as_secs_f64();
        let history = self.objective.history();
        let rows = history.reject_reason.get(before..).unwrap_or_default();
        let rejections = StageRejections::new(&stage.stage, max_rejects, self.reasons, rows);
        let summary = StageSummary {
            pre_gate_rejects: self.pre_gate_rejects,
            analysis_evaluations: analysed,
            cancelled_unstarted: self.cancelled_unstarted,
            feasible: self.feasible,
            wall_time_s: wall_s,
            candidate_time_s: if analysed > 0 {
                busy / analysed as f64
            } else {
                0.0
            },
            lane_utilization: if wall_s > 0.0 {
                busy / (self.lanes() as f64 * wall_s)
            } else {
                0.0
            },
            sizing_work: SizingWorkSummary::of_history(history, before),
            ..stage
        };
        (summary, rejections)
    }

    /// Lanes a batch runs on: the workers for a concurrent objective, else
    /// one.
    pub(super) fn lanes(&self) -> usize {
        if self.objective.runs_concurrently() {
            self.workers.max(1)
        } else {
            1
        }
    }

    /// Coupled analyses actually run so far: history rows less the
    /// candidates this evaluator's batches recorded as never started.
    pub(super) fn analyses(&self) -> usize {
        self.objective.history().n_evaluations() - self.cancelled_unstarted
    }

    pub(super) fn evaluate_block(&mut self, points: &[Vec<f64>]) -> Vec<ScoredPoint> {
        let mut rejected = Vec::new();
        let admitted: Vec<Vec<f64>> = points
            .iter()
            .enumerate()
            .filter(|(index, values)| {
                match (self.baseline != Some(values.as_slice()))
                    .then(|| pre_gate_violation(values, self.bounds, self.config))
                    .flatten()
                {
                    Some((reason, violation)) => {
                        rejected.push((
                            *index,
                            reason,
                            ScoredPoint {
                                values: (*values).clone(),
                                cost: f64::INFINITY,
                                tier: Tier::PreGateFailed,
                                constraint_violation: violation,
                                objectives: [f64::INFINITY; 3],
                            },
                        ));
                        false
                    }
                    None => true,
                }
            })
            .map(|(_, values)| values.clone())
            .collect();
        for (_, _, point) in &rejected {
            self.cache.insert(point.clone());
        }
        let pending = self.cache.missing(&admitted);
        let before = self.objective.history().n_evaluations();
        // A concurrent objective reports each candidate it ran; one that runs
        // the batch as a single uninterruptible block is timed as one.
        let objective = &mut *self.objective;
        let workers = self.workers;
        let scores = if objective.runs_concurrently() {
            let scores = objective.evaluate_batch(&pending, workers);
            let durations = objective.take_concurrent_telemetry().unwrap_or_default();
            self.scope.record_concurrent(&durations);
            self.busy += durations
                .iter()
                .map(|(elapsed, _)| *elapsed)
                .sum::<Duration>();
            scores
        } else {
            let started = Instant::now();
            let scores = self.scope.block(pending.len() as u64, || {
                objective.evaluate_batch(&pending, workers)
            });
            self.busy += started.elapsed();
            scores
        };
        let history = self.objective.history();
        self.cancelled_unstarted += history
            .reject_reason
            .get(before..)
            .unwrap_or_default()
            .iter()
            .filter(|reason| reason.as_str() == CANCELLED_UNSTARTED)
            .count();
        for (offset, (values, (cost, _))) in pending.iter().zip(scores).enumerate() {
            let point = scored_point_at(values, cost, history, before + offset);
            self.feasible += usize::from(point.valid());
            self.trace
                .evaluations
                .extend(analysed_record(history, before + offset, self.stage));
            self.cache.insert_analysed(point, before + offset);
        }
        let resolved = self.cache.resolve_prefix(points);
        for (index, reason, _) in rejected {
            if index < resolved.len() {
                reason.count(&mut self.reasons);
                self.pre_gate_rejects += 1;
                self.trace
                    .push(self.stage, TraceClass::Rejected, None, None);
            }
        }
        resolved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deadline_counts_only_the_resolved_prefix_including_its_pre_gate_rejects() {
        struct Prefix(OptimizationHistory);
        impl SearchObjective for Prefix {
            fn evaluate(&mut self, design: &[f64]) -> f64 {
                self.0.record(
                    DesignVector::from_array(design).unwrap(),
                    true,
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    "",
                );
                1.0
            }
            fn history(&self) -> &OptimizationHistory {
                &self.0
            }
            fn evaluate_batch(
                &mut self,
                designs: &[Vec<f64>],
                _workers: usize,
            ) -> Vec<(f64, bool)> {
                designs
                    .iter()
                    .take(1)
                    .map(|values| (self.evaluate(values), true))
                    .collect()
            }
        }
        let bounds = vec![(0.0, 1.0); DesignVector::bounds().len()];
        let points = vec![vec![0.5; 16], vec![2.0; 16], vec![0.6; 16], vec![3.0; 16]];
        let mut objective = Prefix(OptimizationHistory::new());
        let mut evaluator = BatchEvaluator::new(&mut objective, 4, &bounds, None, None);
        let scores = evaluator.evaluate_block(&points);
        assert_eq!(scores.len(), 2);
        assert_eq!(evaluator.analyses(), 1);
        assert_eq!(evaluator.feasible, 1);
        assert_eq!(evaluator.pre_gate_rejects, 1);
        assert_eq!(evaluator.reasons.design_box, 1);
        assert_eq!(scores[0].tier, Tier::Feasible);
        assert_eq!(scores[1].tier, Tier::PreGateFailed);
    }

    #[test]
    fn pre_gated_candidates_stay_out_of_the_budget_and_the_baseline_is_always_analysed() {
        struct Counting(OptimizationHistory);
        impl SearchObjective for Counting {
            fn evaluate(&mut self, design: &[f64]) -> f64 {
                let design = DesignVector::from_array(design).unwrap();
                self.0
                    .record(design, true, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, "");
                1.0
            }
            fn history(&self) -> &OptimizationHistory {
                &self.0
            }
        }
        let bounds = vec![(0.0, 1.0); DesignVector::bounds().len()];
        let (inside, baseline, outside) = (vec![0.5; 16], vec![2.0; 16], vec![3.0; 16]);
        let mut objective = Counting(OptimizationHistory::new());
        let mut evaluator = BatchEvaluator::new(&mut objective, 1, &bounds, None, None);
        evaluator.baseline = Some(&baseline);
        let scores = evaluator.evaluate_block(&[inside, baseline.clone(), outside]);
        assert_eq!(evaluator.pre_gate_rejects, 1);
        assert_eq!(evaluator.reasons.design_box, 1);
        assert_eq!(evaluator.analyses(), 2);
        assert_eq!(scores[1].tier, Tier::Feasible);
        assert_eq!(scores[2].tier, Tier::PreGateFailed);
        assert!((scores[2].constraint_violation - 32.0).abs() < 1e-12);
        // Every requested candidate is traced once; a repeat served from the
        // cache is not.
        evaluator.evaluate_block(&[vec![0.5; 16]]);
        let trace = &evaluator.trace;
        assert_eq!(trace.len(), 3);
        assert_eq!(trace.count(TraceClass::Rejected), 1);
        assert_eq!(trace.count(TraceClass::Valid), 2);
        assert!(trace
            .evaluations
            .iter()
            .all(|evaluation| evaluation.stage == TraceStage::Refinement));
    }

    #[test]
    fn the_pre_gate_admits_the_box_and_scores_a_miss_by_its_normalized_size() {
        let bounds = [(0.0, 10.0), (5.0, 5.0)];
        assert_eq!(pre_gate_violation(&[3.0, 5.0], &bounds, None), None);
        let boxed = |miss| Some((Reason::DesignBox, miss));
        assert_eq!(pre_gate_violation(&[12.0, 5.0], &bounds, None), boxed(0.2));
        assert_eq!(pre_gate_violation(&[3.0, 7.0], &bounds, None), boxed(2.0));
        assert_eq!(
            pre_gate_violation(&[f64::NAN, 5.0], &bounds, None),
            boxed(f64::INFINITY)
        );
    }

    #[test]
    fn the_planform_pre_gate_rejects_exactly_what_the_geometry_builder_rejects() {
        // Registered aircraft whose envelopes pin inboard stations, sampled
        // over their own search boxes: the pre-gate verdict must be the
        // builder's, candidate by candidate, and must reject some.
        let mut rejected = 0;
        for preset in ["ATR72-600", "A320-200"] {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset })).unwrap();
            let bounds = DesignOptimizer::new(config.clone())
                .resolved_bounds(None, None)
                .unwrap();
            let mut rng = crate::search_methods::rng::SearchRng::seed(5);
            let builder = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()));
            for values in crate::search_methods::latin_hypercube(&bounds, 200, &mut rng) {
                let design = DesignVector::from_array(&values).unwrap();
                let gate = planform_violation(&config, &design);
                let built = builder.build(Some(&design), false);
                assert_eq!(gate.is_some(), built.is_err(), "{preset} {values:?}");
                rejected += usize::from(gate.is_some());
            }
        }
        assert!(rejected > 0);
    }

    #[test]
    fn the_wing_area_pre_gate_agrees_with_the_built_reference_area() {
        // Every registered aircraft over its own box: the built wing's
        // `s_ref` is the planform's trapezoid area to round-off far below
        // the gate's margin, plus any non-planar tip device's projection
        // (the AVE's), never less, so the gate rejects only designs the
        // residual table rejects.
        let mut gated = 0;
        for preset in alas_config::presets::available() {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset })).unwrap();
            let bounds = DesignOptimizer::new(config.clone())
                .resolved_bounds(None, None)
                .unwrap();
            let limit_m2 = config.requirements.max_wing_area_m2;
            let mut rng = crate::search_methods::rng::SearchRng::seed(11);
            let builder = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()));
            for values in crate::search_methods::latin_hypercube(&bounds, 40, &mut rng) {
                let design = DesignVector::from_array(&values).unwrap();
                let Ok(planform) = config.geometry.wing.transport_planform(&design) else {
                    continue;
                };
                let Ok(plane) = builder.build(Some(&design), false) else {
                    continue;
                };
                let area_m2 = planform.reference_area_m2();
                assert!(
                    plane.s_ref >= area_m2 * (1.0 - 1.0e-12),
                    "{preset}: planform {area_m2} m2 against built {} m2",
                    plane.s_ref
                );
                let gate = hard_geometry_violation(&config, &design)
                    .is_some_and(|(reason, _)| reason == Reason::WingArea);
                if gate {
                    assert!(plane.s_ref > limit_m2, "{preset} {values:?}");
                    gated += 1;
                }
            }
        }
        assert!(gated > 0);
    }

    #[test]
    fn the_trailing_edge_pre_gate_agrees_with_the_full_residual_table() {
        // The A320-200 box: most of it violates the exposed trailing-edge
        // angle. Each sampled verdict must be the full evaluation's.
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
        let bounds = DesignOptimizer::new(config.clone())
            .resolved_bounds(None, None)
            .unwrap();
        let mut rng = crate::search_methods::rng::SearchRng::seed(3);
        let mut compared = 0;
        for values in crate::search_methods::latin_hypercube(&bounds, 6, &mut rng) {
            let design = DesignVector::from_array(&values).unwrap();
            let Ok(assessment) = crate::mdo::assess_product_candidate(&config, &design) else {
                continue;
            };
            let violated = assessment
                .violated_hard_ids()
                .contains(&"root_to_kink_te_angle");
            let gated = hard_geometry_violation(&config, &design).is_some();
            assert_eq!(gated, violated, "{values:?}");
            compared += 1;
        }
        assert!(compared >= 3, "{compared}");
    }
}
