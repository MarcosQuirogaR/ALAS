// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The typed residual, sized-candidate and assessment types the mission-sized
//! objective is built from.
//!
//! A weighted penalty cannot say whether a candidate is infeasible or merely
//! expensive, so every requirement here is kept as a [`ConstraintResidual`]
//! with its own physical units, and [`CandidateAssessment`] carries the whole
//! table rather than only the scalar it folds into.

use std::sync::Arc;

use alas_config::design_variables::DesignVector;
use alas_config::{ConstraintPolicy, TailSizing};
use alas_mass::breakdown::{MassBreakdown, MassCoordinates};
use alas_mass::dispatch::DispatchSolution;

/// Which requirement family a [`ConstraintResidual`] belongs to.
///
/// Ordered so that the relaxation policy can count *distinct* violated
/// discipline groups deterministically. The order is
/// the declaration order and carries no severity meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConstraintFamily {
    /// Fuel capacity, the takeoff-mass ceiling and the sizing closure.
    Mass,
    /// Centre-of-gravity envelope and landing-gear reactions.
    Balance,
    /// Airworthiness field-length, climb-gradient and speed requirements.
    Performance,
    /// Planform, wing-loading and accommodation requirements.
    Geometry,
    /// Strength, stiffness and the validity of the structural response model.
    Structure,
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

mod external;
mod fuel;
mod residual;

pub use external::{ExternalPolar, PolarConditionTolerance};
pub use fuel::{CandidateDrag, CandidateFuelArtifacts, DeckKey, SizingControls, SizingWork};

/// One design candidate closed against the sizing mission.
#[derive(Debug, Clone, PartialEq)]
pub struct SizedCandidate {
    /// Analysis takeoff mass, kg: the mission-closed dispatch mass in every
    /// mode but `FixedRequirement`, where it is the declared MTOW; the
    /// dispatch plan keeps the mission-required mass for the ceiling check.
    pub takeoff_mass_kg: f64,
    /// `fixed_aircraft` or `coupled` (`alas_config::MassSizingBasis`).
    pub sizing_basis: &'static str,
    /// FLOPS design gross mass `DG` the components were evaluated at, kg.
    pub design_gross_mass_kg: f64,
    /// Design landing mass `WLDG` the gear was evaluated at, kg.
    pub design_landing_mass_kg: f64,
    /// What the takeoff-mass sizing plan adds to the closure.
    pub mtow: super::mtow_modes::MtowPlanOutcome,
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
    /// Usable fuel capacity the dispatch is bounded by, kg
    /// ([`super::usable_fuel_capacity`]: published for an unchanged preset,
    /// else the resolved tank layout), or `NaN` when the configured tank
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
    /// Trimmed lift-to-drag ratio at the sizing cruise Mach and altitude and
    /// the lift coefficient of [`Self::takeoff_mass_kg`], read from the
    /// candidate's cruise drag ([`CandidateDrag::cd`]).
    pub lift_to_drag: f64,
    /// Reserve-inclusive takeoff fuel of the mission the takeoff mass was
    /// closed on (the design mission when the plan declares one, otherwise
    /// the route), kg: the plan's `takeoff_fuel_kg`.
    pub design_mission_fuel_kg: f64,
    /// Trip fuel of that mission, kg.
    pub design_mission_trip_fuel_kg: f64,
    /// The drag, deck, frozen plan and tail sizing the closure flew.
    pub fuel_artifacts: Arc<CandidateFuelArtifacts>,
    /// Work the closure spent.
    pub work: SizingWork,
    /// The dispatch closure this candidate was sized by.
    pub dispatch: DispatchSolution,
    /// Outer sizing passes taken (fixed-point iterations of empty mass, fuel
    /// and takeoff mass; always `1` under `MtowSizing::FixedRequirement`,
    /// up to the configured iteration limit under every mission-closed mode).
    pub sizing_iterations: usize,
    /// Whether the outer sizing loop closed within its iteration budget.
    pub sizing_closed: bool,
    /// Whether the outer takeoff-mass iteration settled within tolerance,
    /// whatever limit the dispatch met; [`Self::sizing_closed`] also
    /// requires a converged dispatch.
    pub takeoff_mass_settled: bool,
    /// Re-trims after the first, each triggered by a CG shift beyond the
    /// configured re-trim tolerance (a mass change alone never re-trims:
    /// the drag table spans the lift range).
    pub retrim_count: usize,
    /// CG shift, percent MAC, between the last trim and the converged state.
    pub cg_shift_pct_mac: f64,
    /// Whether the wing total includes a complete, declared primary and
    /// secondary inventory (false while a clean-sheet inventory is partial).
    pub structural_inventory_complete: bool,
    /// Strength-sized primary wingbox mass in the complete wing, kg, kept
    /// so a structural change is observable beside the empirical total.
    pub structural_primary_mass_kg: f64,
    /// Reconciled non-box wing inventory represented in the complete wing,
    /// kg.
    pub structural_secondary_mass_kg: f64,
}

/// Where a [`ResolvedProductState`] came from, so a consumer can say which
/// physical evaluation it is quoting instead of assuming every mass/CG state
/// in the run is the same one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductStateProvenance {
    /// The converged mass/CG/trim fixed point of `mdo::mda::converge`, i.e.
    /// the state the search's own hard-feasibility gate was evaluated on.
    MissionSizedClosure,
}

impl ProductStateProvenance {
    /// Stable identifier for reports, manifests and JSON exports.
    pub fn id(self) -> &'static str {
        match self {
            Self::MissionSizedClosure => "mdo::mda::converge",
        }
    }
}

/// The converged physical state a candidate's feasibility was actually
/// decided on.
///
/// A downstream report reuses the balance state the search accepted rather
/// than rebuilding its own. Two independent rebuilds of "the same" aircraft can
/// disagree about where it balances while agreeing on its takeoff mass, which
/// is exactly how a hard-feasible finalist could be printed as physically
/// infeasible. Carrying the state itself makes the two comparable, and the
/// provenance keeps it explicit that these numbers are the search's, not a
/// second opinion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedProductState {
    /// The design vector this state was actually evaluated on.
    ///
    /// **Not necessarily the vector the caller passed.** A clean-sheet design
    /// space derives `fuselage_length_m` from the cabin load case
    /// (`AlasConfig::optimizer.design_space.sizes_fuselage_from_cabin`), so
    /// the evaluator replaces the caller's coordinate before it builds
    /// anything. The derivation has fixed points -- an optimizer finalist is
    /// one, which is why the ordinary search path sees no difference -- but a
    /// vector pinned by hand, imported, or recorded by an earlier build of
    /// the cabin need not be one.
    ///
    /// A report bound to this assessment must build on **this** vector.
    /// Building on the caller's instead describes a different aeroplane from
    /// the one the feasibility gate passed: on the r5 fixture at
    /// `AlasConfig::default()` the bodies differ by 4.25 m (2 406 kg, 1.237 m).
    pub design: DesignVector,
    /// Solved tail sizing; a replay applies it with [`TailSizing::apply_to`].
    pub tail_sizing: TailSizing,
    /// Component masses at the closed takeoff mass, kg.
    pub masses: MassBreakdown,
    /// Component centroids the balance was evaluated at, m.
    pub coords: MassCoordinates,
    /// Physical centre of gravity of `masses` at `coords`, m.
    pub cg_x_m: f64,
    /// Neutral point the static margin was measured against, m.
    pub x_neutral_point_m: f64,
    /// Mean aerodynamic chord the percentages are expressed in, m.
    pub mac_m: f64,
    /// Closed takeoff mass this state belongs to, kg.
    pub takeoff_mass_kg: f64,
    /// Which evaluation produced it.
    pub provenance: ProductStateProvenance,
}

/// What the controlled-relaxation policy made of one candidate's violated
/// hard residuals.
///
/// Empty and `rejected: false` for a strictly feasible candidate, which is
/// every candidate under the shipped strict policy. A candidate with a
/// non-empty `relaxed_ids` is *relaxed*, never fully feasible, and every
/// reader that reports feasibility has to say so.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RelaxationOutcome {
    /// Limits that were missed inside their own declared tolerance.
    pub relaxed_ids: Vec<&'static str>,
    /// Distinct discipline groups carrying a relaxed miss (the policy counts
    /// groups, not limits).
    pub violated_groups: usize,
    /// Whether the candidate is rejected despite the policy: a miss outside
    /// its tolerance, a limit that is not eligible, or more violated groups
    /// than the policy allows.
    pub rejected: bool,
}

impl RelaxationOutcome {
    /// The outcome of a candidate that needed no relaxation at all.
    pub fn strict() -> Self {
        Self::default()
    }

    /// Whether this candidate was admitted only by the relaxation policy.
    pub fn is_relaxed(&self) -> bool {
        !self.relaxed_ids.is_empty() && !self.rejected
    }
}

/// The residual table and scalar cost for one evaluated candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateAssessment {
    /// The candidate the mission was sized against.
    pub sized: SizedCandidate,
    /// The converged mass/CG/neutral-point state the balance residuals above
    /// were evaluated on, for a caller that must report the same aircraft.
    pub resolved: ResolvedProductState,
    /// Every evaluated requirement, as a typed residual.
    pub residuals: Vec<ConstraintResidual>,
    /// Whether the candidate is admissible: every hard-policy residual
    /// satisfied, or every miss admitted by the controlled-relaxation policy.
    /// Check [`CandidateAssessment::is_strictly_feasible`] before reporting
    /// an aircraft as feasible.
    pub hard_feasible: bool,
    /// What the relaxation policy made of the violated hard residuals.
    /// Empty under the shipped strict policy.
    pub relaxation: RelaxationOutcome,
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
    /// Whether every hard-policy residual is met with nothing relaxed.
    ///
    /// This is the question a feasibility report has to ask. `hard_feasible`
    /// admits a relaxed candidate on purpose, so that the search can rank it;
    /// reporting one as feasible would be exactly the silent relabelling the
    /// relaxation policy forbids.
    pub fn is_strictly_feasible(&self) -> bool {
        self.hard_feasible && !self.relaxation.is_relaxed()
    }

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
/// The reason is one of the weighted-penalty objective's own evaluation-failure labels
/// (`geometry_build`, `mass_coordinates`, `payload_layout`, `trim_solve`;
/// see `crate::objective`), which is what lets
/// `OptimizationHistory::reject_reason_counts` and the differential-evolution
/// reject-reason grouping read a mission-sized failure the same way as a
/// weighted-penalty one.
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

    #[test]
    fn unavailable_constraint_measurements_never_become_satisfied() {
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for (actual, limit, raw) in [
                (invalid, 1.0, 0.0),
                (1.0, invalid, 0.0),
                (1.0, 1.0, invalid),
            ] {
                let residual = ConstraintResidual::scaled(
                    "test",
                    ConstraintFamily::Structure,
                    actual,
                    limit,
                    "m",
                    raw,
                    ConstraintPolicy::Hard,
                );
                assert!(residual.violated());
            }
            let residual = ConstraintResidual::direct(
                "test",
                ConstraintFamily::Structure,
                1.0,
                1.0,
                "m",
                0.0,
                invalid,
                ConstraintPolicy::Hard,
            );
            assert!(residual.violated());
        }
        let satisfied = ConstraintResidual::scaled(
            "test",
            ConstraintFamily::Structure,
            0.5,
            1.0,
            "m",
            -0.5,
            ConstraintPolicy::Hard,
        );
        assert!(!satisfied.violated());
    }

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
