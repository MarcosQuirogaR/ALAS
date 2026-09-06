// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The typed residual, sized-candidate and assessment types the mission-sized
//! objective is built from.
//!
//! A weighted penalty cannot say whether a candidate is infeasible or merely
//! expensive, so every requirement here is kept as a [`ConstraintResidual`]
//! with its own physical units, and [`CandidateAssessment`] carries the whole
//! table rather than only the scalar it folds into.

use alas_config::design_variables::DesignVector;
use alas_config::ConstraintPolicy;
use alas_mass::dispatch::DispatchSolution;

/// Which requirement family a [`ConstraintResidual`] belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintFamily {
    /// Fuel capacity, the takeoff-mass ceiling and the sizing closure.
    Mass,
    /// Centre-of-gravity envelope and landing-gear reactions.
    Balance,
    /// Airworthiness field-length, climb-gradient and speed requirements.
    Performance,
    /// Planform, wing-loading and accommodation requirements.
    Geometry,
}

/// One requirement, evaluated as a typed residual rather than folded into a
/// weighted penalty.
///
/// `raw_residual` is positive when the requirement is violated, in the same
/// physical units as `actual` and `limit`. `normalized_violation` is what
/// `mdo::cost` actually sums: `max(raw_residual, 0) / scale`, dimensionless,
/// so a mass residual and an angle residual can be added together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConstraintResidual {
    /// Stable identifier, joined with `+` in a rejected candidate's reason.
    pub id: &'static str,
    /// Requirement family this residual belongs to.
    pub family: ConstraintFamily,
    /// Value the candidate achieves.
    pub actual: f64,
    /// Governing threshold.
    pub limit: f64,
    /// Unit shared by `actual` and `limit`.
    pub unit: &'static str,
    /// Signed physical residual; positive means a violation.
    pub raw_residual: f64,
    /// `max(raw_residual, 0) / scale`, dimensionless.
    pub normalized_violation: f64,
    /// How this family takes part in the ranking.
    pub policy: ConstraintPolicy,
}

/// Relative violation below which a scaled residual counts as met.
///
/// The built reference area differs from the design vector's area by the
/// geometry builder's own rounding (about 7e-6 relative at the default
/// design), and every other quantity in the table carries at least that
/// much numerical noise; a candidate is not infeasible for a violation an
/// order of magnitude below any model's fidelity.
const NUMERICAL_SLACK: f64 = 1.0e-5;

impl ConstraintResidual {
    /// Build a residual whose normalized violation is `raw_residual` scaled
    /// by the magnitude of `limit`: the convention every scaled residual in
    /// `mdo::residuals` uses, so a limit near zero cannot divide the
    /// violation toward infinity. Violations below [`NUMERICAL_SLACK`] of
    /// the limit are reported in `raw_residual` but not counted.
    pub(crate) fn scaled(
        id: &'static str,
        family: ConstraintFamily,
        actual: f64,
        limit: f64,
        unit: &'static str,
        raw_residual: f64,
        policy: ConstraintPolicy,
    ) -> Self {
        let scale = limit.abs().max(1e-9);
        let normalized_violation = (raw_residual / scale - NUMERICAL_SLACK).max(0.0);
        Self {
            id,
            family,
            actual,
            limit,
            unit,
            raw_residual,
            normalized_violation,
            policy,
        }
    }

    /// Build a residual whose normalized violation is supplied directly, for
    /// a source (the CG envelope, an evaluation failure) that already
    /// carries its own normalization.
    #[allow(clippy::too_many_arguments)] // one named field per physical quantity of the residual; a struct would only rename them once
    pub(crate) fn direct(
        id: &'static str,
        family: ConstraintFamily,
        actual: f64,
        limit: f64,
        unit: &'static str,
        raw_residual: f64,
        normalized_violation: f64,
        policy: ConstraintPolicy,
    ) -> Self {
        Self {
            id,
            family,
            actual,
            limit,
            unit,
            raw_residual,
            normalized_violation,
            policy,
        }
    }

    /// Whether this residual is on the infeasible side of its limit.
    fn violated(&self) -> bool {
        self.normalized_violation > 0.0
    }

    /// The signed, dimensionless constraint value a gradient-based driver
    /// works with: the counted violation when the requirement is missed,
    /// and the negative margin over the limit's magnitude when it is met,
    /// so the value crosses zero exactly at the limit.
    pub fn signed_normalized(&self) -> f64 {
        if self.normalized_violation > 0.0 {
            self.normalized_violation
        } else {
            (self.raw_residual / self.limit.abs().max(1e-9)).min(0.0)
        }
    }
}

/// A cruise drag polar supplied by an external aerodynamic solver, so the
/// sizing loop can close a candidate around aerodynamics it did not trim
/// itself. `induced_factor_k` is the parabolic-polar factor
/// `(cd - cd0) / cl^2` at the cruise lift coefficient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExternalPolar {
    /// Zero-lift drag coefficient at the cruise point.
    pub cd0: f64,
    /// Induced-drag factor `k`, positive.
    pub induced_factor_k: f64,
    /// Lift-to-drag ratio at the required cruise lift.
    pub lift_to_drag: f64,
    /// Angle of attack at that point, degrees, for the history.
    pub alpha_deg: f64,
    /// Stabilizer incidence the point was evaluated at, degrees.
    pub incidence_deg: f64,
    /// Neutral-point station in geometry axes, m, for the balance family.
    pub x_np: f64,
}

impl ExternalPolar {
    /// Whether every term is finite and physically usable.
    pub fn is_valid(&self) -> bool {
        self.cd0.is_finite()
            && self.cd0 > 0.0
            && self.induced_factor_k.is_finite()
            && self.induced_factor_k > 0.0
            && self.lift_to_drag.is_finite()
            && self.lift_to_drag > 0.0
            && self.alpha_deg.is_finite()
            && self.incidence_deg.is_finite()
            && self.x_np.is_finite()
    }
}

/// One design candidate closed against the sizing mission.
#[derive(Debug, Clone, PartialEq)]
pub struct SizedCandidate {
    /// Takeoff mass the dispatch closure settled on, kg.
    pub takeoff_mass_kg: f64,
    /// Operating empty mass at the closed takeoff mass, kg.
    pub operating_empty_mass_kg: f64,
    /// Zero-fuel mass (operating empty plus payload) at closure, kg.
    pub zero_fuel_mass_kg: f64,
    /// Payload mass carried, kg.
    pub payload_kg: f64,
    /// Taxi plus trip fuel, kg.
    pub block_fuel_kg: f64,
    /// Fuel on board at brake release, kg.
    pub takeoff_fuel_kg: f64,
    /// Fuel loaded at the ramp, kg.
    pub ramp_fuel_kg: f64,
    /// Usable fuel-tank capacity, kg, or `NaN` when the configured tank
    /// arrangement could not be resolved on the built geometry.
    pub usable_capacity_kg: f64,
    /// Still-air distance the mission was sized over, m.
    pub design_range_m: f64,
    /// Trimmed cruise lift-to-drag ratio. This is the aerodynamic operating
    /// point evaluated once for the candidate and does not vary with the
    /// sized mass.
    pub lift_to_drag: f64,
    /// The dispatch closure this candidate was sized by.
    pub dispatch: DispatchSolution,
    /// Outer sizing passes taken (fixed-point iterations of empty mass, fuel
    /// and takeoff mass; always `1` under `MtowSizing::FixedRequirement`).
    pub sizing_iterations: usize,
    /// Whether the outer sizing loop closed within its iteration budget.
    pub sizing_closed: bool,
    /// Trim and drag-polar re-evaluations the sizing loop performed after
    /// the first, each triggered by a centre-of-gravity shift beyond the
    /// configured re-trim tolerance.
    pub retrim_count: usize,
    /// Centre-of-gravity shift, percent MAC, between the last trim and the
    /// converged mass state: the residual inconsistency the loop accepted.
    pub cg_shift_pct_mac: f64,
}

/// The residual table and scalar cost for one evaluated candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateAssessment {
    /// The candidate the mission was sized against.
    pub sized: SizedCandidate,
    /// Every evaluated requirement, as a typed residual.
    pub residuals: Vec<ConstraintResidual>,
    /// Whether every hard-policy residual is satisfied.
    pub hard_feasible: bool,
    /// Sum of normalized violations across hard-policy residuals.
    pub hard_violation_sum: f64,
    /// Sum of normalized violations across soft-policy residuals.
    pub soft_violation_sum: f64,
    /// The configured mission quantity, before normalization.
    pub objective_value: f64,
    /// The scalar cost the search ranks candidates by.
    pub cost: f64,
}

impl CandidateAssessment {
    /// Identifiers of every violated hard-policy residual, in evaluation
    /// order, joined with `+` for a rejected candidate's history entry.
    pub fn violated_hard_ids(&self) -> Vec<&'static str> {
        self.residuals
            .iter()
            .filter(|residual| residual.policy == ConstraintPolicy::Hard && residual.violated())
            .map(|residual| residual.id)
            .collect()
    }
}

/// The extra fields a history entry needs that [`CandidateAssessment`] does
/// not carry, because they describe the design vector and its aerodynamic
/// operating point rather than the sizing outcome.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HistoryFields {
    pub dv: DesignVector,
    pub span_m: f64,
    pub alpha_deg: f64,
    pub area_m2: f64,
    pub trim_ih_deg: f64,
}

/// A candidate that could not be built, sized or trimmed at all.
///
/// Carries only the reason, not the design vector: `DesignVector` is large
/// enough that embedding one in every fallible pipeline stage's `Err`
/// variant would make each `Result` itself large, and every caller already
/// has `x` at hand to rebuild the vector only in the failure path that
/// actually needs it for a history entry.
///
/// The reason is one of the legacy objective's own evaluation-failure labels
/// (`geometry_build`, `mass_coordinates`, `payload_layout`, `trim_solve`;
/// see `crate::objective_evaluate`), which is what lets
/// `OptimizationHistory::reject_reason_counts` and the differential-evolution
/// reject-reason grouping read a mission-sized failure the same way as a
/// legacy one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CandidateFailure {
    pub reason: &'static str,
}
