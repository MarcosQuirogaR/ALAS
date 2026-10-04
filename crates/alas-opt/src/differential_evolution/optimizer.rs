// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Differential-evolution orchestration for the optimizer boundary.

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::*;
use crate::cancellation::CancelScope;
use crate::search_methods::product_de;

mod batch;
mod evaluation_cache;
mod feasibility_restoration;
mod native_pool;
mod product_search;

/// Whether the caller supplied a single literal fuselage-length bound.
///
/// The GUI uses fixed bounds for a reference/full-analysis review.  In that
/// case the design vector is authoritative and the cabin-derived clean-sheet
/// sizing solve must not silently replace it.  A non-degenerate bound keeps
/// the normal derived-coordinate behavior.
fn explicit_fuselage_length_bound(bounds: Option<&[(f64, f64)]>) -> bool {
    let Some(bounds) = bounds else {
        return false;
    };
    let Some(index) = alas_config::design_variables::SPECS
        .iter()
        .position(|spec| spec.name == "fuselage_length_m")
    else {
        return false;
    };
    bounds
        .get(index)
        .is_some_and(|&(lower, upper)| lower == upper)
}

impl DesignOptimizer {
    /// Construct a new design optimizer with `config`.
    pub fn new(config: AlasConfig) -> Self {
        Self { config }
    }

    fn invalid_solver_setting_reason(&self) -> Option<String> {
        let solver = &self.config.optimizer.solver;
        if !alas_config::SolverSettings::is_supported_method(&solver.method) {
            return Some(format!("unknown optimizer method {:?}", solver.method));
        }
        solver.validate_budgets().err()
    }

    fn validate_request(&self, bounds: Option<&[(f64, f64)]>) -> Result<(), OptimizationError> {
        if let Some(reason) = self.invalid_solver_setting_reason() {
            return Err(OptimizationError::InvalidConfiguration(reason));
        }
        if !self.config.mass_model.mass_architecture.is_production() {
            return Err(OptimizationError::InvalidConfiguration(
                "the optimizer requires the pure_flops_transport_v1 mass architecture".to_owned(),
            ));
        }
        self.config
            .optimizer
            .design_space
            .validate()
            .map_err(OptimizationError::InvalidConfiguration)?;
        bounds.map_or(Ok(()), validate_bounds)
    }

    /// The search box [`Self::run`] uses for `bounds` and `initial_design`.
    ///
    /// This is the one place the preset-anchored envelope is derived from the
    /// configuration and the nominal design: `None` yields the configured
    /// design-space envelope around the nominal, and an explicit box is
    /// intersected with it. A front end that wants to show or pre-validate the
    /// box reads it here rather than rebuilding the envelope itself.
    ///
    /// # Errors
    ///
    /// Returns [`OptimizationError::InvalidConfiguration`] or
    /// [`OptimizationError::InvalidBounds`] exactly as [`Self::run`] would
    /// before its first evaluation.
    pub fn resolved_bounds(
        &self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
    ) -> Result<Vec<(f64, f64)>, OptimizationError> {
        self.validate_request(bounds)?;
        self.effective_bounds(bounds, initial_design)
    }

    fn effective_bounds(
        &self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
    ) -> Result<Vec<(f64, f64)>, OptimizationError> {
        let default_bounds = DesignVector::bounds();
        let requested = bounds.unwrap_or(&default_bounds);
        validate_bounds(requested)?;
        let nominal = self.nominal_design(initial_design)?;
        let design_space = &self.config.optimizer.design_space;
        let declared = self.config.design_envelope(&nominal);
        // A caller that supplies no bounds is asking for the design space it
        // configured, and that space is already a complete, anchored envelope:
        // the global box widened to contain the start for a clean sheet, the
        // +/-10 % window around the registered reference for an adaptation, the
        // reference itself for a baseline sandbox.  Intersecting it with the
        // global box as well would be intersecting it with AVE's family, and
        // the registered types do not live there.  Every other type in the
        // catalogue has a span, a chord or a body length outside at least one
        // of those global limits, an A320 by more than twenty metres of
        // fuselage, so the intersection is empty and the search is rejected
        // before it evaluates anything.  The envelope is validated on its own
        // terms below and the evaluator enforces the same one, so nothing is
        // widened by this: what changes is that a registered aircraft can be
        // optimized inside its own declared window without the caller having
        // to restate it.
        if bounds.is_none() {
            let envelope: Vec<(f64, f64)> = declared
                .iter()
                .map(|variable| (variable.lower, variable.upper))
                .collect();
            validate_bounds(&envelope)?;
            return Ok(envelope);
        }
        let mut effective = Vec::with_capacity(requested.len());
        for (index, (&(requested_lower, requested_upper), variable)) in
            requested.iter().zip(&declared).enumerate()
        {
            // The clean-sheet fuselage length is a derived coordinate solved
            // from the requested cabin load case, not a caller-chosen search
            // freedom (see the matching skip in
            // `DesignObjective::validate_design_space`). A caller replaying an
            // explicit design (for example a fixed-design finalist review)
            // legitimately pins this coordinate at the design's own literal
            // length; only the global spec bounds validated above apply.
            if design_space.sizes_fuselage_from_cabin() && variable.name == "fuselage_length_m" {
                // The evaluator derives this coordinate from the cabin load
                // case.  When the caller uses the optimizer's ordinary
                // global bounds (or those bounds contain the derived value),
                // remove the redundant search dimension and keep the winner
                // self-consistent.  An explicitly incompatible fixed range
                // remains caller-owned for compatibility with review tools
                // that deliberately replay a literal design vector.
                if bounds.is_none()
                    || (requested_lower <= nominal.fuselage_length_m
                        && nominal.fuselage_length_m <= requested_upper)
                {
                    effective.push((nominal.fuselage_length_m, nominal.fuselage_length_m));
                } else {
                    effective.push((requested_lower, requested_upper));
                }
                continue;
            }
            let (declared_lower, declared_upper) = (variable.lower, variable.upper);
            let lower = requested_lower.max(declared_lower);
            let upper = requested_upper.min(declared_upper);
            if lower > upper {
                return Err(OptimizationError::InvalidBounds(format!(
                    "bound {index} [{requested_lower}, {requested_upper}] does not intersect the design-mode envelope [{declared_lower}, {declared_upper}]"
                )));
            }
            effective.push((lower, upper));
        }
        Ok(effective)
    }

    /// The search box and canonical baseline of a run given no caller
    /// bounds: the configured design space anchored at the preset.
    pub(crate) fn anchored_search_space(
        &self,
    ) -> Result<(Vec<(f64, f64)>, DesignVector), OptimizationError> {
        self.validate_request(None)?;
        Ok((
            self.effective_bounds(None, None)?,
            self.nominal_design(None)?,
        ))
    }

    fn nominal_design(
        &self,
        initial_design: Option<&DesignVector>,
    ) -> Result<DesignVector, OptimizationError> {
        let nominal = initial_design
            .copied()
            .unwrap_or_else(|| self.config.configured_nominal_design());
        crate::mdo::canonical_nominal_design(&self.config, nominal)
            .map_err(OptimizationError::InvalidConfiguration)
    }

    /// Execute the Differential Evolution optimization search.
    ///
    /// # Errors
    ///
    /// Returns [`OptimizationError::NoFeasibleDesign`] when every evaluated
    /// candidate fails the active objective/analysis policy. Invalid bounds
    /// and unknown solver tokens are reported before any objective evaluation
    /// begins.
    pub fn run(
        &mut self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        progress_callback: Option<&mut dyn FnMut(&str)>,
    ) -> Result<OptimizationResult, OptimizationError> {
        self.run_cancellable(bounds, initial_design, progress_callback, None)
    }

    /// [`Self::run`], observing an optional pipeline cancellation flag.
    ///
    /// `cancel` is checked between evaluation blocks. A cancelled run returns
    /// `Ok` with `OptimizationResult::termination` set to `"cancelled"`; its
    /// winner is the best fully scored member available when cancellation was
    /// observed.
    ///
    /// # Errors
    ///
    /// Same as [`Self::run`].
    pub fn run_cancellable(
        &mut self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        progress_callback: Option<&mut dyn FnMut(&str)>,
        cancel: Option<&AtomicBool>,
    ) -> Result<OptimizationResult, OptimizationError> {
        self.run_cancellable_with_evidence(bounds, initial_design, progress_callback, cancel, None)
    }

    /// Run the same strict search, observing completed evidence before rejection.
    ///
    /// The callback borrows the complete search once, before cancellation or
    /// feasibility determines the result and before restoring the winning load
    /// case. Observing failed candidates never makes them deliverable.
    ///
    /// # Errors
    ///
    /// Same as [`Self::run_cancellable`]; invalid requests publish no evidence.
    pub fn run_cancellable_with_evidence(
        &mut self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        progress_callback: Option<&mut dyn FnMut(&str)>,
        cancel: Option<&AtomicBool>,
        evidence_callback: Option<&mut dyn FnMut(&OptimizationResult)>,
    ) -> Result<OptimizationResult, OptimizationError> {
        let result = self.run_native_search(bounds, initial_design, progress_callback, cancel);
        complete_search(&mut self.config, result, evidence_callback)
    }

    pub(super) fn run_native_search(
        &self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        progress_callback: Option<&mut dyn FnMut(&str)>,
        cancel: Option<&AtomicBool>,
    ) -> Result<OptimizationResult, OptimizationError> {
        self.validate_request(bounds)?;
        let effective_bounds = self.effective_bounds(bounds, initial_design)?;
        let nominal = self.nominal_design(initial_design)?;
        let preserve_explicit_fuselage_length = explicit_fuselage_length_bound(bounds);
        let mut objective = DesignObjective::new_with_nominal_and_fuselage_policy(
            self.config.clone(),
            nominal,
            preserve_explicit_fuselage_length,
        );
        // Every candidate is sized under a cap measured on the nominal, so a
        // straggler cannot stall a generation (`search::work_cap`).
        let cap = crate::search::work_cap::nominal_work_cap(&objective, &nominal);
        objective.sizing_controls.budget = cap;

        let search_initial = if bounds.is_none()
            && self
                .config
                .optimizer
                .design_space
                .sizes_fuselage_from_cabin()
        {
            Some(&nominal)
        } else {
            initial_design.or(Some(&nominal))
        };
        let mut parallel = native_pool::NativeObjective::new(
            &mut objective,
            self.config.optimizer.solver.resolved_workers(),
            cancel,
        )
        .map_err(|error| {
            OptimizationError::InvalidConfiguration(format!(
                "could not create the native compute worker pool: {error}"
            ))
        })?;
        let fidelity = crate::ScreeningFidelity::shipped();
        if fidelity == crate::ScreeningFidelity::full() {
            let result = self.run_product_search(
                &effective_bounds,
                search_initial,
                &mut parallel,
                product_search::ScreeningModel::Same,
                progress_callback,
                cancel,
            );
            return Ok(with_work_cap(result, cap));
        }
        let mut screening_model = DesignObjective::new_with_nominal_and_fuselage_policy(
            fidelity.configure(&self.config),
            nominal,
            preserve_explicit_fuselage_length,
        );
        screening_model.sizing_controls = fidelity.controls();
        if screening_model.sizing_controls.budget.is_none() {
            screening_model.sizing_controls.budget = cap;
        }
        let mut screening = parallel.sharing_lanes(&mut screening_model);
        let result = self.run_product_search(
            &effective_bounds,
            search_initial,
            &mut parallel,
            product_search::ScreeningModel::Separate(&mut screening),
            progress_callback,
            cancel,
        );
        Ok(with_work_cap(result, cap))
    }

    /// Execute the same differential-evolution search with a caller-provided
    /// aerodynamic objective.
    ///
    /// The evaluator receives a typed [`DesignVector`] and returns the same
    /// diagnostics recorded by the native VLM objective. This is the seam used
    /// by the pipeline's AVL adapter; it deliberately contains no process or
    /// CPACS dependency.
    ///
    /// # Errors
    ///
    /// Returns [`OptimizationError::NoFeasibleDesign`] when every evaluated
    /// candidate fails the delegated evaluator's active validity policy.
    pub fn run_with_evaluator<E: ObjectiveEvaluator + ?Sized>(
        &mut self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        evaluator: &mut E,
        progress_callback: Option<&mut dyn FnMut(&str)>,
    ) -> Result<OptimizationResult, OptimizationError> {
        self.run_with_evaluator_cancellable(
            bounds,
            initial_design,
            evaluator,
            progress_callback,
            None,
        )
    }

    /// [`Self::run_with_evaluator`], observing an optional pipeline
    /// cancellation flag. See [`Self::run_cancellable`] for the semantics.
    ///
    /// # Errors
    ///
    /// Same as [`Self::run_with_evaluator`].
    pub fn run_with_evaluator_cancellable<E: ObjectiveEvaluator + ?Sized>(
        &mut self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        evaluator: &mut E,
        progress_callback: Option<&mut dyn FnMut(&str)>,
        cancel: Option<&AtomicBool>,
    ) -> Result<OptimizationResult, OptimizationError> {
        self.validate_request(bounds)?;
        let effective_bounds = self.effective_bounds(bounds, initial_design)?;
        let nominal = self.nominal_design(initial_design)?;
        let search_initial = if bounds.is_none()
            && self
                .config
                .optimizer
                .design_space
                .sizes_fuselage_from_cabin()
        {
            Some(&nominal)
        } else {
            initial_design.or(Some(&nominal))
        };
        let mut objective =
            DelegatedObjective::new(evaluator, self.config.optimizer.weights.failure_cost);
        // A delegated evaluator is an external, serial model: the refinement
        // starts from the baseline without a screening stage.
        let result = self.run_product_search(
            &effective_bounds,
            search_initial,
            &mut objective,
            product_search::ScreeningModel::Skip,
            progress_callback,
            cancel,
        );

        // Same cancellation contract as `run_cancellable`.
        if result.was_cancelled() {
            return Ok(result);
        }
        let result = ensure_feasible(result)?;

        restore_winning_payload_load_case(&mut self.config, &result.best_design);
        Ok(result)
    }
}

fn complete_search(
    config: &mut AlasConfig,
    result: Result<OptimizationResult, OptimizationError>,
    evidence_callback: Option<&mut dyn FnMut(&OptimizationResult)>,
) -> Result<OptimizationResult, OptimizationError> {
    let result = result?;
    if let Some(callback) = evidence_callback {
        callback(&result);
    }
    // A cancelled search has not decided whether the design space is feasible.
    if result.was_cancelled() {
        return Ok(result);
    }
    let result = ensure_feasible(result)?;
    restore_winning_payload_load_case(config, &result.best_design);
    Ok(result)
}

/// `result` with the per-candidate work cap of its native search recorded on
/// every stage's work summary.
fn with_work_cap(
    mut result: OptimizationResult,
    cap: Option<crate::mdo::mission_model::SizingBudget>,
) -> OptimizationResult {
    let stages = result
        .search_diagnostics
        .iter_mut()
        .flat_map(|diagnostics| diagnostics.stages.iter_mut());
    for work in stages.filter_map(|stage| stage.sizing_work.as_mut()) {
        work.cap_trip_flights = cap.map(|cap| cap.max_trip_flights);
        work.cap_deck_evals = cap.map(|cap| cap.max_deck_evals);
    }
    result
}

#[cfg(test)]
// Fixture setup and expected outcomes fail at the assertion boundary.
#[allow(clippy::expect_used)]
mod evidence_tests {
    use super::*;

    fn search_result(valid: bool, termination: &str) -> OptimizationResult {
        let design = DesignVector::default();
        OptimizationResult {
            best_design: design,
            best_cost: 1.0,
            best_valid: valid,
            history: OptimizationHistory {
                design_vectors: vec![design],
                valid: vec![valid],
                cost: vec![1.0],
                objective_value: vec![2.0],
                hard_violation: vec![if valid { 0.0 } else { 1.0 }],
                reject_reason: vec![if valid { "" } else { "buffet_margin" }.into()],
                ..Default::default()
            },
            wall_time_s: 0.0,
            method: "differential_evolution".into(),
            strategy: "test".into(),
            termination: termination.into(),
            pareto_front: Vec::new(),
            search_diagnostics: None,
            delivered_acceptance: None,
        }
    }

    #[test]
    fn observing_success_preserves_result_and_payload_restoration_in_every_mass_mode() {
        for mode in alas_config::MtowSizing::ALL {
            let mut initial = AlasConfig::default();
            initial.optimizer.objective.mtow_sizing = mode;
            initial.cabin.passenger.class_mix_mode = "percent".into();
            initial.requirements.num_passengers = 1;
            let result = search_result(true, "evaluation_budget");
            let mut unobserved_config = initial.clone();
            let unobserved = complete_search(&mut unobserved_config, Ok(result.clone()), None)
                .expect("successful search");
            assert_ne!(unobserved_config, initial, "winning payload is restored");
            let mut observed_config = initial;
            let mut snapshots = Vec::new();
            let observed = complete_search(
                &mut observed_config,
                Ok(result.clone()),
                Some(&mut |snapshot| snapshots.push(snapshot.clone())),
            )
            .expect("observing success");
            assert_eq!(snapshots, vec![result]);
            assert_eq!(observed, unobserved);
            assert_eq!(observed_config, unobserved_config, "{mode:?} payload state");
        }
    }

    #[test]
    fn rejection_and_cancellation_publish_once_without_restoring_payload() {
        for termination in ["evaluation_budget", "cancelled"] {
            let mut config = AlasConfig::default();
            config.requirements.num_passengers = 1;
            let initial = config.clone();
            let result = search_result(false, termination);
            let mut snapshots = Vec::new();
            let observed = complete_search(
                &mut config,
                Ok(result.clone()),
                Some(&mut |snapshot| snapshots.push(snapshot.clone())),
            );
            assert_eq!(snapshots, vec![result.clone()]);
            assert_eq!(config, initial);
            if termination == "cancelled" {
                assert_eq!(observed.expect("cancellation has priority"), result);
            } else {
                let strict = ensure_feasible(result).expect_err("strict rejection");
                assert_eq!(observed.expect_err("observed rejection"), strict);
            }
        }
    }

    #[test]
    fn errors_before_search_publish_no_candidate_history() {
        let mut config = AlasConfig::default();
        let initial = config.clone();
        let error = OptimizationError::InvalidBounds("invalid request".into());
        let mut callbacks = 0;
        let result = complete_search(
            &mut config,
            Err(error.clone()),
            Some(&mut |_| callbacks += 1),
        );
        assert_eq!(result.expect_err("invalid request"), error);
        assert_eq!(callbacks, 0);
        assert_eq!(config, initial);
    }
}
