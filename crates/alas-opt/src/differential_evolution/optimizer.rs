// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Differential-evolution orchestration for the optimizer boundary.

use std::sync::atomic::AtomicBool;

use super::*;
use crate::cancellation::{CancelPhase, CancelScope};
use crate::search_methods::product_de;

mod evaluation_cache;
mod feasibility_restoration;
mod native_pool;
mod product_search;
mod scan;
mod scipy_search;

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
        Self {
            config,
            reference_mass_coordinates: false,
        }
    }

    /// Construct an optimizer that replays the reference-compatible mass
    /// coordinate.
    ///
    /// The `scipy_legacy` solver profile also uses this evaluator. This
    /// explicit constructor replays the recorded parity fixtures regardless
    /// of the selected profile.
    pub fn new_reference_compatibility(mut config: AlasConfig) -> Self {
        // This constructor is the explicit comparison boundary.  Make the
        // selected architecture agree with the replay flag so the
        // compatibility optimizer cannot accidentally ask the pure FLOPS mass evaluator for
        // a reference-coordinate run.
        config.mass_model.mass_architecture =
            alas_config::MassArchitecture::LegacyReferenceCompatibleComparison;
        config.mass_model.apply_architecture();
        Self {
            config,
            reference_mass_coordinates: true,
        }
    }

    fn invalid_solver_setting_reason(&self) -> Option<String> {
        let solver = &self.config.optimizer.solver;
        if !alas_config::SolverSettings::is_supported_method(&solver.method) {
            return Some(format!("unknown optimizer method {:?}", solver.method));
        }
        if !alas_config::SolverSettings::is_supported_strategy(&solver.strategy) {
            return Some(format!("unknown optimizer strategy {:?}", solver.strategy));
        }
        None
    }

    /// Whether this run uses the reference-compatible objective, mass
    /// coordinates and SciPy-style driver.
    fn uses_scipy_compatible_profile(&self) -> bool {
        self.reference_mass_coordinates
            || self.config.optimizer.solver.method == alas_config::optimizer::SCIPY_LEGACY_METHOD
    }

    fn validate_request(&self, bounds: Option<&[(f64, f64)]>) -> Result<(), OptimizationError> {
        if let Some(reason) = self.invalid_solver_setting_reason() {
            return Err(OptimizationError::InvalidConfiguration(reason));
        }
        if !self.uses_scipy_compatible_profile() {
            if !self.config.mass_model.mass_architecture.is_production() {
                return Err(OptimizationError::InvalidConfiguration(
                    "the production optimizer requires pure_flops_transport_v1; use the explicit reference-compatibility constructor for the reference-compatible comparison"
                        .to_owned(),
                ));
            }
            self.config
                .optimizer
                .design_space
                .validate()
                .map_err(OptimizationError::InvalidConfiguration)?;
        }
        let default_bounds;
        let bounds = match bounds {
            Some(bounds) => bounds,
            None => {
                default_bounds = DesignVector::bounds();
                &default_bounds
            }
        };
        validate_bounds(bounds)
    }

    fn effective_bounds(
        &self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
    ) -> Result<Vec<(f64, f64)>, OptimizationError> {
        let default_bounds = DesignVector::bounds();
        let requested = bounds.unwrap_or(&default_bounds);
        validate_bounds(requested)?;
        if self.uses_scipy_compatible_profile() {
            return Ok(requested.to_vec());
        }
        let nominal = self.nominal_design(initial_design)?;
        let design_space = &self.config.optimizer.design_space;
        let declared = design_space.envelope(&nominal);
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

    fn nominal_design(
        &self,
        initial_design: Option<&DesignVector>,
    ) -> Result<DesignVector, OptimizationError> {
        let nominal = initial_design.copied().unwrap_or_else(|| {
            self.config
                .preset
                .is_empty()
                .then_some(DesignVector::default())
                .or_else(|| {
                    alas_config::presets::get(&self.config.preset)
                        .ok()
                        .map(|preset| preset.design_vector)
                })
                .unwrap_or_default()
        });
        if self.uses_scipy_compatible_profile() {
            return Ok(nominal);
        }
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
    /// `cancel` is checked between serial evaluations, or after a deferred
    /// generation batch, in both search profiles. A cancelled run returns
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
        let result = self.run_native_search(bounds, initial_design, progress_callback, cancel)?;
        // A cancelled run did not finish asking whether the design space has
        // a feasible aircraft. Preserve its explicit cancellation verdict.
        if result.was_cancelled() {
            return Ok(result);
        }
        if self.uses_scipy_compatible_profile() {
            // SciPy v1.1.0 always returned its lowest scalar-cost member. The
            // pipeline can still filter that history at reporting fidelity.
            return Ok(result);
        }
        let result = ensure_feasible(result)?;
        restore_winning_payload_load_case(&mut self.config, &result.best_design);
        Ok(result)
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
        let scipy_compatible = self.uses_scipy_compatible_profile();
        let mut objective = if scipy_compatible {
            DesignObjective::new_reference_compatibility(self.config.clone())
        } else {
            DesignObjective::new_with_nominal_and_fuselage_policy(
                self.config.clone(),
                nominal,
                preserve_explicit_fuselage_length,
            )
        };

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
        let result = if scipy_compatible {
            if cancel.is_some() {
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
                self.run_search(
                    Some(&effective_bounds),
                    initial_design,
                    &mut parallel,
                    progress_callback,
                    cancel,
                )
            } else {
                self.run_search(
                    Some(&effective_bounds),
                    initial_design,
                    &mut objective,
                    progress_callback,
                    cancel,
                )
            }
        } else {
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
            let compute_pool = Some(parallel.shared_pool());
            self.run_product_search(
                Some(&effective_bounds),
                search_initial,
                &mut parallel,
                progress_callback,
                cancel,
                compute_pool,
            )
        };

        Ok(result)
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
        let result = if self.uses_scipy_compatible_profile() {
            self.run_search(
                Some(&effective_bounds),
                initial_design,
                &mut objective,
                progress_callback,
                cancel,
            )
        } else {
            self.run_product_search(
                Some(&effective_bounds),
                search_initial,
                &mut objective,
                progress_callback,
                cancel,
                None,
            )
        };

        // Same cancellation contract as `run_cancellable`.
        if result.was_cancelled() {
            return Ok(result);
        }
        if self.uses_scipy_compatible_profile() {
            return Ok(result);
        }
        let result = ensure_feasible(result)?;

        restore_winning_payload_load_case(&mut self.config, &result.best_design);
        Ok(result)
    }
}
