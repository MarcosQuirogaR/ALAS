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

/// How closely an [`ExternalPolar`] must have been evaluated at a
/// candidate's own cruise condition to be flown by it.
///
/// The defaults are numerical-identity tolerances, not modelling slack: the
/// external run is expected to have been commanded at exactly the condition
/// being sized, so anything larger than solver round-tripping noise means a
/// different operating point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolarConditionTolerance {
    /// Absolute Mach tolerance, dimensionless. `1e-9` admits only the
    /// text round-trip of a commanded Mach through a solver input deck.
    pub mach: f64,
    /// Absolute altitude tolerance, m. One metre changes ISA density by
    /// about `1e-4` relative at cruise, already below the drag model's
    /// fidelity, and no solver deck carries sub-metre altitude.
    pub altitude_m: f64,
    /// Relative reference-area tolerance, dimensionless. The geometry
    /// builder's own rounding moves the built area by about `7e-6` relative
    /// at the default design (see [`NUMERICAL_SLACK`]), so `1e-5` accepts a
    /// rebuild of the same design and rejects a different wing.
    pub relative_area: f64,
}

impl Default for PolarConditionTolerance {
    fn default() -> Self {
        Self {
            mach: 1.0e-9,
            altitude_m: 1.0,
            relative_area: 1.0e-5,
        }
    }
}

/// A cruise drag polar supplied by an external aerodynamic solver, so the
/// sizing loop can close a candidate around aerodynamics it did not trim
/// itself. `induced_factor_k` is the parabolic-polar factor
/// `(cd - cd0) / cl^2` at the cruise lift coefficient.
///
/// The coefficients are meaningless without the state they were solved at,
/// so the evaluation condition (`mach`, `altitude_m`, `reference_area_m2`,
/// `target_cl`) and the solver identity travel with them: a polar solved for
/// one wing area cannot be non-dimensionally reused on another.
///
/// Units: areas m^2, altitudes m (ISA geometric), angles degrees, stations m
/// in geometry axes (x positive aft); Mach and every coefficient are
/// dimensionless.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExternalPolar {
    /// Zero-lift drag coefficient at the cruise point.
    pub cd0: f64,
    /// Induced-drag factor `k`, positive.
    pub induced_factor_k: f64,
    /// Wave-drag coefficient at the external solver's design Mach and lift.
    /// It remains separate from `induced_factor_k` so the shared mission
    /// model can omit transonic wave drag during takeoff, climb, descent and
    /// landing phases.
    pub wave_drag_cd: f64,
    /// Lift-to-drag ratio at the required cruise lift.
    pub lift_to_drag: f64,
    /// Angle of attack at that point, degrees, for the history.
    pub alpha_deg: f64,
    /// Stabilizer incidence the point was evaluated at, degrees.
    pub incidence_deg: f64,
    /// Neutral-point station in geometry axes, m, for the balance family.
    pub x_np: f64,
    /// Free-stream Mach the external solver evaluated the point at.
    /// Subsonic: the linear panel methods this admits are invalid at `M >= 1`.
    pub mach: f64,
    /// ISA geometric altitude the point is referred to, m. A panel solver
    /// carries no atmosphere itself, so this is the altitude the required
    /// lift coefficient and the parasite build-up were computed at.
    pub altitude_m: f64,
    /// Wing reference area the coefficients are non-dimensionalized by, m^2.
    pub reference_area_m2: f64,
    /// Lift coefficient the polar was evaluated/interpolated at, positive.
    pub target_cl: f64,
    /// Identity of the solver that produced the point, e.g. `"avl"`.
    pub source: &'static str,
    /// Whether `target_cl` fell inside the solved polar's lift range, so the
    /// point is an interpolation rather than an extrapolation.
    pub bracketed: bool,
}

impl ExternalPolar {
    /// Whether every term is finite, physically usable, and carries a
    /// complete, non-extrapolated evaluation identity.
    pub fn is_valid(&self) -> bool {
        self.cd0.is_finite()
            && self.cd0 > 0.0
            && self.induced_factor_k.is_finite()
            && self.induced_factor_k > 0.0
            && self.wave_drag_cd.is_finite()
            && self.wave_drag_cd >= 0.0
            && self.lift_to_drag.is_finite()
            && self.lift_to_drag > 0.0
            && self.alpha_deg.is_finite()
            && self.incidence_deg.is_finite()
            && self.x_np.is_finite()
            && self.mach.is_finite()
            && self.mach > 0.0
            && self.mach < 1.0
            && self.altitude_m.is_finite()
            && self.reference_area_m2.is_finite()
            && self.reference_area_m2 > 0.0
            && self.target_cl.is_finite()
            && self.target_cl > 0.0
            && !self.source.is_empty()
            && self.bracketed
    }

    /// Whether this polar was evaluated at the condition a candidate is being
    /// sized at.
    ///
    /// # Errors
    ///
    /// A human-readable description naming the mismatched quantity, both
    /// values and the tolerance, when the Mach, altitude or reference area
    /// differs by more than `tolerance`. A polar reused across conditions
    /// silently rescales every coefficient, so this is a rejection rather
    /// than a warning.
    pub fn matches_condition(
        &self,
        mach: f64,
        altitude_m: f64,
        reference_area_m2: f64,
        tolerance: PolarConditionTolerance,
    ) -> Result<(), String> {
        if !mach.is_finite() || !altitude_m.is_finite() || !reference_area_m2.is_finite() {
            return Err(format!(
                "candidate condition is not finite: mach={mach}, altitude_m={altitude_m}, reference_area_m2={reference_area_m2}"
            ));
        }
        if (self.mach - mach).abs() > tolerance.mach {
            return Err(format!(
                "polar mach {} does not match candidate mach {mach} within {}",
                self.mach, tolerance.mach
            ));
        }
        if (self.altitude_m - altitude_m).abs() > tolerance.altitude_m {
            return Err(format!(
                "polar altitude {} m does not match candidate altitude {altitude_m} m within {} m",
                self.altitude_m, tolerance.altitude_m
            ));
        }
        let area_scale = reference_area_m2.abs().max(1e-9);
        if (self.reference_area_m2 - reference_area_m2).abs() / area_scale > tolerance.relative_area
        {
            return Err(format!(
                "polar reference area {} m2 does not match candidate area {reference_area_m2} m2 within {} relative",
                self.reference_area_m2, tolerance.relative_area
            ));
        }
        Ok(())
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
    /// Seats the candidate shell can certify under its selected cabin layout.
    pub passenger_capacity: i64,
    /// Seats actually occupied by the requested load case.
    pub carried_passengers: i64,
    /// Net cargo capacity of the candidate hold, kg.
    pub cargo_capacity_kg: f64,
    /// Net cargo actually loaded, kg.
    pub carried_cargo_payload_kg: f64,
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
    /// Whether the route distance was explicit or computed from two finite
    /// source-resolved airport coordinates.
    pub mission_distance_known: bool,
    /// Whether both configured airport records resolved, independently of
    /// whether their runway distances are suitable for field performance.
    pub airport_records_resolved: bool,
    /// Whether both airports supplied declared operational runway distances.
    pub declared_airport_data_complete: bool,
    /// Horizontal distance consumed by non-cruise climb/descent profile
    /// phases, m. A shorter requested mission is infeasible for this model.
    pub minimum_profile_range_m: f64,
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
    /// Whether the wing total includes a complete, declared primary and
    /// secondary inventory. Clean-sheet Torenbeek movable terms are partial
    /// by design and therefore remain false until the omitted inventory is
    /// explicitly supplied.
    pub structural_inventory_complete: bool,
    /// Strength-sized primary wingbox mass represented in the complete wing,
    /// kg. This is retained for the result/report seam so a structural mass
    /// change is observable rather than hidden behind the empirical total.
    pub structural_primary_mass_kg: f64,
    /// Reconciled non-box wing inventory represented in the complete wing,
    /// kg.
    pub structural_secondary_mass_kg: f64,
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

/// Capacity and carried-load facts returned by the detailed payload layout.
/// Passenger counts are seats; cargo masses are net revenue cargo, excluding
/// ULD tare. Keeping both capacity and carried values prevents a geometry
/// auto-sizer from silently replacing the required load case.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PayloadCapacity {
    pub passenger_capacity: i64,
    pub carried_passengers: i64,
    pub cargo_capacity_kg: f64,
    pub carried_cargo_payload_kg: f64,
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

// The tests construct every polar they assert on, so a failed `expect` is
// the assertion failing rather than a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    /// A transport-like cruise polar at M 0.78 / 11 000 m over 120 m^2, with
    /// every validity gate satisfied, for the rejection tests to perturb one
    /// field of at a time.
    fn polar() -> ExternalPolar {
        ExternalPolar {
            cd0: 0.020,
            induced_factor_k: 0.045,
            wave_drag_cd: 0.0012,
            lift_to_drag: 17.5,
            alpha_deg: 2.4,
            incidence_deg: -1.2,
            x_np: 20.5,
            mach: 0.78,
            altitude_m: 11_000.0,
            reference_area_m2: 120.0,
            target_cl: 0.52,
            source: "avl",
            bracketed: true,
        }
    }

    #[test]
    fn a_complete_transport_polar_is_valid() {
        assert!(polar().is_valid());
    }

    #[test]
    fn every_non_finite_component_is_rejected() {
        let setters: [fn(&mut ExternalPolar, f64); 10] = [
            |p, v| p.cd0 = v,
            |p, v| p.induced_factor_k = v,
            |p, v| p.wave_drag_cd = v,
            |p, v| p.lift_to_drag = v,
            |p, v| p.alpha_deg = v,
            |p, v| p.incidence_deg = v,
            |p, v| p.x_np = v,
            |p, v| p.mach = v,
            |p, v| p.reference_area_m2 = v,
            |p, v| p.target_cl = v,
        ];
        for (index, set) in setters.iter().enumerate() {
            for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let mut candidate = polar();
                set(&mut candidate, value);
                assert!(!candidate.is_valid(), "field {index} accepted {value}");
            }
        }
    }

    #[test]
    fn a_non_positive_drag_lift_or_area_is_rejected() {
        // `cd0`, `k`, `L/D`, Mach, area and the target lift are all strictly
        // positive on a lifting cruise point; only wave drag may be zero.
        let setters: [fn(&mut ExternalPolar, f64); 6] = [
            |p, v| p.cd0 = v,
            |p, v| p.induced_factor_k = v,
            |p, v| p.lift_to_drag = v,
            |p, v| p.mach = v,
            |p, v| p.reference_area_m2 = v,
            |p, v| p.target_cl = v,
        ];
        for (index, set) in setters.iter().enumerate() {
            for value in [0.0, -1.0e-6, -1.0] {
                let mut candidate = polar();
                set(&mut candidate, value);
                assert!(!candidate.is_valid(), "field {index} accepted {value}");
            }
        }

        let mut zero_wave = polar();
        zero_wave.wave_drag_cd = 0.0;
        assert!(zero_wave.is_valid(), "zero wave drag is physical");
        let mut negative_wave = polar();
        negative_wave.wave_drag_cd = -1.0e-9;
        assert!(!negative_wave.is_valid(), "negative wave drag is not");
    }

    #[test]
    fn an_extrapolated_or_unattributed_polar_is_rejected() {
        let mut extrapolated = polar();
        extrapolated.bracketed = false;
        assert!(!extrapolated.is_valid());

        let mut unattributed = polar();
        unattributed.source = "";
        assert!(!unattributed.is_valid());
    }

    #[test]
    fn a_supersonic_evaluation_is_rejected() {
        // Every admitted external solver here is a linear subsonic panel
        // method; `M >= 1` is outside its validity domain, not merely noisy.
        for mach in [1.0, 1.2] {
            let mut candidate = polar();
            candidate.mach = mach;
            assert!(!candidate.is_valid(), "M={mach}");
        }
    }

    #[test]
    fn matches_condition_accepts_the_state_the_polar_was_solved_at() {
        let reference = polar();
        assert!(reference
            .matches_condition(0.78, 11_000.0, 120.0, PolarConditionTolerance::default())
            .is_ok());
        // Within tolerance: 1 m of altitude and 1e-5 relative area.
        assert!(reference
            .matches_condition(
                0.78,
                11_000.5,
                120.0 * (1.0 + 5.0e-6),
                PolarConditionTolerance::default()
            )
            .is_ok());
    }

    #[test]
    fn matches_condition_rejects_a_different_mach_altitude_or_area() {
        let reference = polar();
        let tolerance = PolarConditionTolerance::default();
        let mach = reference
            .matches_condition(0.80, 11_000.0, 120.0, tolerance)
            .expect_err("a 0.02 Mach change is a different cruise point");
        assert!(mach.contains("mach"), "{mach}");

        let altitude = reference
            .matches_condition(0.78, 10_000.0, 120.0, tolerance)
            .expect_err("1000 m is a different atmosphere");
        assert!(altitude.contains("altitude"), "{altitude}");

        let area = reference
            .matches_condition(0.78, 11_000.0, 132.0, tolerance)
            .expect_err("a 10 % area change rescales every coefficient");
        assert!(area.contains("reference area"), "{area}");
    }

    #[test]
    fn matches_condition_rejects_a_non_finite_candidate_condition() {
        let error = polar()
            .matches_condition(
                f64::NAN,
                11_000.0,
                120.0,
                PolarConditionTolerance::default(),
            )
            .expect_err("a NaN condition cannot be matched");
        assert!(error.contains("not finite"), "{error}");
    }
}
