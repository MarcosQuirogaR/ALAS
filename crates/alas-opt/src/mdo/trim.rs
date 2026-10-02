// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Trimming one candidate at its cruise lift coefficient and reading the
//! trimmed drag polar the mission model flies on.
//!
//! Extracted from `mdo::build` so the trim acceptance rules live in one place
//! and can be tightened without touching geometry or mass code.
//!
//! # Acceptance is strict, and nothing is repaired
//!
//! A drag polar reaches the mission model only when the trim solve reported
//! `converged`, every returned component is finite and physically signed, the
//! achieved lift coefficient agrees with the requested one within
//! [`CL_TARGET_RELATIVE_TOLERANCE`], and the pitching-moment residual is
//! within [`CM_RESIDUAL_TOLERANCE`]. There is no floor, clamp or fallback on
//! any term: a degenerate induced-drag factor is a failed analysis, not a
//! candidate with a repaired polar.
//!
//! # Convergence is read from the solver, not inferred
//!
//! `alas_stab::trim::StabilityTrimResult` exposes an explicit `converged`
//! flag. It is `true` only when `alas_stab`'s Newton refinement drove
//! `max(|CL - CL_target|, |Cm|)` below its own `1.0e-7` residual tolerance
//! within eight iterations, and `false` for a singular trim Jacobian, a
//! non-finite update, an exhausted iteration budget, or geometry with no
//! horizontal stabilizer to trim with. The residual checks here are therefore
//! a second, independent gate: they are evaluated on
//! `AeroAnalysis::trimmed_performance`'s own re-solve of the trimmed state,
//! which catches a converged trim that the drag build-up then disagrees with.
//!
//! # Units and frames
//!
//! Masses kg, areas m^2, altitudes m (ISA geometric), angles degrees,
//! stations m in geometry axes (x positive aft). Mach and every coefficient
//! are dimensionless; coefficients are referred to `Airplane::s_ref`.

use std::sync::Arc;

use alas_aero::analysis::{AeroAnalysis, TrimPoint, TrimmedPerformance};
use alas_atmo::Atmosphere;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_stab::trim::stability_and_trim;

use super::drag_table::{DesignTrim, TrimmedDragTable};
use super::mission_model::ParabolicPolar;
use super::types::{CandidateDrag, CandidateFailure, ExternalPolar};

/// History label for a trimmed cruise point whose drag table could not be
/// built: an untrimmed induced node, a fourth trimmed point that disagrees
/// with the induced quadratic, or a grid that misses its error bound
/// (`drag_table::DragTableError`). The candidate is rejected, never flown on
/// a partial table.
pub(crate) const DRAG_TABLE_FAILURE: &str = "drag_table";

/// History label for a trim that failed numerically or did not converge.
///
/// Shared with the weighted-penalty objective's evaluation-failure vocabulary so
/// `OptimizationHistory::reject_reason_counts` groups both paths together.
pub(crate) const TRIM_SOLVE_FAILURE: &str = "trim_solve";

/// History label for a cruise point the candidate cannot physically fly: the
/// lift coefficient the requested mass, speed and altitude demand is above
/// `DesignRequirements::max_cruise_cl`, i.e. inside the stall margin.
///
/// This is a signed infeasibility of the requested operating point, not a
/// solver failure, and is deliberately spelled differently from
/// [`TRIM_SOLVE_FAILURE`] so the two cannot be confused in a history.
pub(crate) const TRIM_CRUISE_CL_EXCEEDS_MAX: &str = "trim_cruise_cl_exceeds_max";

/// Relative tolerance on the trimmed lift coefficient against `cl_target`.
///
/// `alas_stab`'s Newton refinement converges on `|CL - CL_target| <= 1e-7`
/// absolute, measured on the same mesh (`AnalysisConfig::spanwise_resolution`
/// and `chordwise_resolution`) that `AeroAnalysis::trimmed_performance` then
/// re-solves the trimmed state on, so the re-solve reproduces that residual
/// to floating-point noise. `1e-3` relative is four orders looser than the
/// solver's own tolerance: it absorbs the re-solve's rounding while still
/// guaranteeing the mission model flies within 0.1 % of the lift the
/// candidate was sized for, an order of magnitude below the drag model's own
/// fidelity.
///
/// Measured margin at the default `AlasConfig` and `DesignVector`:
/// `CL = 0.676474279595`, `CL_target = 0.676474279634`, relative `5.7e-11`:
/// eight orders inside this tolerance, so it rejects a genuinely
/// untrimmed point rather than trading against solver noise.
const CL_TARGET_RELATIVE_TOLERANCE: f64 = 1.0e-3;

/// Absolute tolerance on the trimmed pitching-moment coefficient.
///
/// `AeroAnalysis::trimmed_performance` documents `cm_residual` as purely
/// diagnostic; "it should be near zero, and nothing penalizes it if it is
/// not", so this is the only place an untrimmed aircraft is caught. The
/// solver's own converged residual is `1e-7`; `1e-3` is the largest moment
/// coefficient still negligible against a transport's trimmed tail load
/// (0.1 % of `q S c_bar`, inside the linear VLM's own error), and anything
/// above it means the returned angle/incidence pair is not a trim state.
///
/// Measured margin at the default `AlasConfig` and `DesignVector`:
/// `Cm = -2.5e-11`, eight orders inside this tolerance.
const CM_RESIDUAL_TOLERANCE: f64 = 1.0e-3;

/// The trimmed cruise point: the drag the missions are flown on, the
/// history-facing angle labels and the condition the point is only valid at.
///
/// Every field is an accepted measurement: the constructors reject rather
/// than repair, so no consumer needs to re-check finiteness or sign.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrimmedPolar {
    /// Clean cruise drag at this trim: the native `CD(CL, M, h)` table, or
    /// the external solver's polar.
    pub drag: CandidateDrag,
    /// The cruise trim Jacobian, per degree, that moves the attitude with
    /// the lift coefficient at fixed centre of gravity; `None` for an
    /// external polar, whose attitude is held as supplied.
    pub attitude: Option<TrimAttitude>,
    /// Trimmed lift-to-drag ratio, positive.
    pub lift_to_drag: f64,
    /// Compressibility-corrected reporting angle of attack, degrees.
    pub alpha_deg: f64,
    /// Uncorrected geometric aircraft-body angle used by the VLM trim solve,
    /// in degrees.  This is kept separately from [`Self::alpha_deg`]: the
    /// latter is a display quantity after the transonic compressibility
    /// correction, while sizing constraints and the 2-D section mapping need
    /// the actual body attitude that generated the solved forces.
    pub geometric_body_alpha_deg: f64,
    /// Trimmed horizontal-stabilizer incidence, degrees.
    pub incidence_deg: f64,
    /// Neutral-point station in geometry axes, m.
    pub x_np: f64,
    /// Lift coefficient actually achieved at the trimmed state, positive and
    /// within [`CL_TARGET_RELATIVE_TOLERANCE`] of the requested cruise lift.
    pub cl_trim: f64,
    /// Pitching-moment coefficient left at the trimmed state, or `None` when
    /// the supplying solver reports no moment residual (external polars).
    /// When present it is within [`CM_RESIDUAL_TOLERANCE`] of zero.
    pub cm_residual: Option<f64>,
    /// Whether the state was produced by a solve that reported convergence.
    /// Always `true` on an accepted polar; carried so a consumer that stores
    /// or serializes the polar keeps the provenance.
    pub converged: bool,
    /// Reference area the coefficients are non-dimensionalized by, m^2.
    pub reference_area_m2: f64,
    /// Free-stream Mach the point was evaluated at, dimensionless.
    pub mach: f64,
    /// ISA geometric altitude the point was evaluated at, m.
    pub altitude_m: f64,
}

/// The cruise trim Jacobian of `alas_stab::trim::StabilityTrimResult`, per
/// degree, and the compressibility factor of the reported angle of attack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TrimAttitude {
    /// `dCL/dalpha`, `dCL/dih`, `dCm/dalpha`, `dCm/dih`, per degree.
    pub cl_alpha: f64,
    pub cl_ih: f64,
    pub cm_alpha: f64,
    pub cm_ih: f64,
    /// `beta` of `alas_aero::analysis::compressible_report_alpha`, recovered
    /// from the trimmed point: `alpha_report = alpha + (beta - 1) CL / CL_alpha`.
    pub report_beta: f64,
}

impl TrimmedPolar {
    /// This trim moved to cruise lift coefficient `cl` at the same centre of
    /// gravity, Mach and altitude, without another lattice solve.
    ///
    /// The lift-to-drag ratio is read from [`Self::drag`] at `cl`. The
    /// attitude moves by the trim Jacobian, `J [d alpha, d ih] = [d CL, 0]`:
    /// the vortex lattice is linear in circulation, so the step is exact up
    /// to the lattice's geometric nonlinearity in angle and incidence, which
    /// is second order in `d CL` (`drag_table::induced` measures it through
    /// the same Jacobian). An external polar keeps its supplied attitude.
    pub(crate) fn at_cruise_cl(&self, cl: f64) -> Self {
        let mut moved = self.clone();
        moved.lift_to_drag = cl / self.drag.cd(cl, self.mach, self.altitude_m);
        let Some(j) = self.attitude else {
            return moved;
        };
        let det = j.cl_alpha * j.cm_ih - j.cl_ih * j.cm_alpha;
        // `trim_and_polar` accepted this Jacobian, and the drag table
        // inverted it to place its induced nodes, so it is regular.
        if !det.is_finite() || det == 0.0 {
            return moved;
        }
        let d_cl = cl - self.cl_trim;
        let d_alpha = j.cm_ih * d_cl / det;
        let d_ih = -j.cm_alpha * d_cl / det;
        moved.geometric_body_alpha_deg += d_alpha;
        moved.incidence_deg += d_ih;
        moved.alpha_deg = moved.geometric_body_alpha_deg + (j.report_beta - 1.0) * cl / j.cl_alpha;
        moved.cl_trim = cl;
        moved
    }

    /// Adopt an externally solved cruise polar without re-trimming.
    ///
    /// The caller must gate on [`ExternalPolar::is_valid`] and, once the
    /// candidate's own cruise state is in hand, on
    /// [`ExternalPolar::matches_condition`]: the coefficients are
    /// non-dimensionalized by the area and evaluated at the Mach and
    /// altitude the external solver ran, so reusing them on another
    /// condition silently rescales every drag term. `converged` is `false`
    /// for a polar that fails validity, so an ungated call still cannot
    /// launder an unusable point into a plausible one.
    ///
    /// An external solver supplies no pitching-moment residual for a trimmed
    /// state, so `cm_residual` is `None` rather than a fabricated zero.
    pub(crate) fn from_external(polar: &ExternalPolar) -> Self {
        Self {
            drag: CandidateDrag::External(Arc::new(ParabolicPolar::from_external(polar))),
            attitude: None,
            lift_to_drag: polar.lift_to_drag,
            alpha_deg: polar.alpha_deg,
            // An external polar's alpha is the angle at which its solver was
            // commanded.  ExternalPolar deliberately has no separate
            // compressibility-display field, so it is the geometric input
            // available at this boundary.
            geometric_body_alpha_deg: polar.alpha_deg,
            incidence_deg: polar.incidence_deg,
            x_np: polar.x_np,
            cl_trim: polar.target_cl,
            cm_residual: None,
            converged: polar.is_valid() && polar.bracketed,
            reference_area_m2: polar.reference_area_m2,
            mach: polar.mach,
            altitude_m: polar.altitude_m,
        }
    }
}

/// A trim rejected for a numerical reason: a failed solve, a non-converged
/// solve, a non-finite or non-physical component, or a residual outside
/// tolerance.
fn trim_solve_failure() -> CandidateFailure {
    CandidateFailure {
        reason: TRIM_SOLVE_FAILURE,
    }
}

/// Trim the candidate at the cruise lift coefficient the supplied mass
/// demands and evaluate the trimmed drag polar exactly once.
///
/// `cg_x` is written into `plane.xyz_ref[0]` (m, geometry axes) before the
/// solve, so the neutral point and static margin are measured against the
/// centre of gravity the sizing loop currently believes in.
///
/// # Errors
///
/// [`TRIM_CRUISE_CL_EXCEEDS_MAX`] when the requested cruise point needs more
/// lift than `max_cruise_cl` allows: a physical infeasibility of the
/// requested mass/speed/altitude combination, not a solver failure.
/// [`TRIM_SOLVE_FAILURE`] for every numerical failure: a degenerate cruise
/// lift target, a solver error, a solve that did not converge, a non-finite
/// or non-physically-signed component, a trimmed lift that misses the target
/// by more than [`CL_TARGET_RELATIVE_TOLERANCE`], or a moment residual larger
/// than [`CM_RESIDUAL_TOLERANCE`].
pub(crate) fn trim_and_polar(
    config: &AlasConfig,
    plane: &mut Airplane,
    cg_x: f64,
    dv: &DesignVector,
    cruise_mass_kg: f64,
) -> Result<TrimmedPolar, CandidateFailure> {
    let req = &config.requirements;
    plane.xyz_ref[0] = cg_x;

    let atmo = Atmosphere::new(req.cruise_altitude_m);
    let velocity_m_s = req.cruise_mach * atmo.speed_of_sound();
    let dynamic_pressure_pa = 0.5 * atmo.density() * velocity_m_s * velocity_m_s;
    // `W / (q S)` at the mass the loop is sizing, which equals
    // `DesignRequirements::required_cruise_cl` at the takeoff-mass ceiling.
    let cl_target = cruise_mass_kg * req.gravity_m_s2 / (dynamic_pressure_pa * plane.s_ref);

    // A non-finite or non-positive target is a degenerate state (zero area,
    // zero dynamic pressure, non-positive mass): numerically unusable.
    if !cl_target.is_finite() || cl_target <= 0.0 {
        return Err(trim_solve_failure());
    }
    // Needing more lift than the wing can usably produce is a signed physical
    // infeasibility of the requested cruise point, kept visible as its own
    // reason rather than folded into the solver-failure bucket.
    if cl_target > req.max_cruise_cl {
        return Err(CandidateFailure {
            reason: TRIM_CRUISE_CL_EXCEEDS_MAX,
        });
    }

    let trim = stability_and_trim(
        plane,
        &config.analysis,
        cl_target,
        req.cruise_mach,
        req.cruise_altitude_m,
    )
    .map_err(|_| trim_solve_failure())?;

    // `converged` is false for a singular Jacobian, an exhausted iteration
    // budget, and for geometry with no horizontal stabilizer (whose
    // `trim_ih_deg` is NaN), so the finiteness checks below never see a
    // deliberately-degraded pure-alpha result.
    if !trim.converged
        || !trim.trim_alpha_deg.is_finite()
        || !trim.trim_ih_deg.is_finite()
        || !trim.cl_alpha.is_finite()
        || !trim.x_np.is_finite()
    {
        return Err(trim_solve_failure());
    }

    let aero = AeroAnalysis::new(
        plane,
        AeroAnalysis::quarter_chord_sweep_deg(plane, dv.sweep_deg),
        Some(config.geometry.clone()),
        Some(config.drag_model.clone()),
        Some(config.analysis.clone()),
    );
    let trim_point = TrimPoint {
        trim_alpha_deg: trim.trim_alpha_deg,
        trim_ih_deg: trim.trim_ih_deg,
        cl_alpha: trim.cl_alpha,
    };
    let perf = aero
        .trimmed_performance(&trim_point, req.cruise_mach, req.cruise_altitude_m)
        .map_err(|_| trim_solve_failure())?;

    if !finite_and_physical(&perf) {
        return Err(trim_solve_failure());
    }
    if (perf.cl - cl_target).abs() > CL_TARGET_RELATIVE_TOLERANCE * cl_target {
        return Err(trim_solve_failure());
    }
    if perf.cm_residual.abs() > CM_RESIDUAL_TOLERANCE {
        return Err(trim_solve_failure());
    }

    let drag_table = TrimmedDragTable::build(
        &aero,
        &DesignTrim {
            trim,
            cl: perf.cl,
            cd_induced: perf.cd_induced,
            cm_residual: perf.cm_residual,
            mach: req.cruise_mach,
            altitude_m: req.cruise_altitude_m,
            cl_max_clean: config.performance.cl_max_clean,
            cm_tolerance: CM_RESIDUAL_TOLERANCE,
        },
    )
    .map_err(|_| CandidateFailure {
        reason: DRAG_TABLE_FAILURE,
    })?;

    // `alpha_report = alpha_0L + beta (alpha - alpha_0L)` with
    // `alpha - alpha_0L = CL / CL_alpha` (`TrimmedPerformance::alpha_deg`).
    let lift_angle_deg = perf.cl / trim.cl_alpha;
    let attitude = TrimAttitude {
        cl_alpha: trim.cl_alpha,
        cl_ih: trim.cl_ih,
        cm_alpha: trim.cm_alpha,
        cm_ih: trim.cm_ih,
        report_beta: 1.0 + (perf.alpha_deg - trim.trim_alpha_deg) / lift_angle_deg,
    };
    Ok(TrimmedPolar {
        drag: CandidateDrag::Table(Arc::new(drag_table)),
        attitude: Some(attitude),
        lift_to_drag: perf.l_over_d,
        alpha_deg: perf.alpha_deg,
        geometric_body_alpha_deg: trim.trim_alpha_deg,
        incidence_deg: perf.incidence_deg,
        x_np: trim.x_np,
        cl_trim: perf.cl,
        cm_residual: Some(perf.cm_residual),
        converged: true,
        reference_area_m2: plane.s_ref,
        mach: req.cruise_mach,
        altitude_m: req.cruise_altitude_m,
    })
}

/// Whether every trimmed component is finite and carries a physically
/// possible sign: positive parasite and induced drag, non-negative wave
/// drag, and a positive lift coefficient and lift-to-drag ratio.
fn finite_and_physical(perf: &TrimmedPerformance) -> bool {
    [
        perf.alpha_deg,
        perf.incidence_deg,
        perf.cm_residual,
        perf.cd,
    ]
    .iter()
    .all(|value| value.is_finite())
        && perf.cd_parasite.is_finite()
        && perf.cd_parasite > 0.0
        && perf.cd_induced.is_finite()
        && perf.cd_induced > 0.0
        && perf.cd_wave.is_finite()
        && perf.cd_wave >= 0.0
        && perf.cl.is_finite()
        && perf.cl > 0.0
        && perf.l_over_d.is_finite()
        && perf.l_over_d > 0.0
}

// The tests build their own configurations and assert on values they
// constructed, so a failed `expect` is the assertion failing rather than a
// library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdo::build::{build_geometry, first_mass_pass};
    use crate::mdo::types::PolarConditionTolerance;

    /// The default configuration, its default design vector, the geometry it
    /// builds and the first-pass centre of gravity the sizing loop trims at.
    fn default_candidate() -> (AlasConfig, DesignVector, Airplane, f64) {
        let config = AlasConfig::default();
        let x = DesignVector::default().to_array();
        let (candidate_config, dv, plane) =
            build_geometry(&config, &x).unwrap_or_else(|f| panic!("{}", f.reason));
        let (_, _, cg0, _, _, _) = first_mass_pass(&candidate_config, &dv, &plane)
            .unwrap_or_else(|f| panic!("{}", f.reason));
        (candidate_config, dv, plane, cg0[0])
    }

    /// A polar that passes every validity gate, at the default cruise
    /// condition, for the rejection tests to perturb one field of.
    fn valid_external(mach: f64, altitude_m: f64, area_m2: f64) -> ExternalPolar {
        ExternalPolar {
            cd0: 0.02,
            induced_factor_k: 0.045,
            wave_drag_cd: 0.001,
            lift_to_drag: 17.0,
            alpha_deg: 2.0,
            incidence_deg: -1.0,
            x_np: 20.0,
            mach,
            altitude_m,
            reference_area_m2: area_m2,
            target_cl: 0.5,
            source: "avl",
            bracketed: true,
        }
    }

    #[test]
    fn the_default_design_trims_to_a_finite_positive_converged_polar() {
        let (config, dv, mut plane, cg_x) = default_candidate();
        let mass = config.requirements.mtow_kg;
        let polar = trim_and_polar(&config, &mut plane, cg_x, &dv, mass)
            .unwrap_or_else(|f| panic!("{}", f.reason));

        assert!(polar.converged);
        assert!(
            polar.drag.table().is_some(),
            "a native trim carries its table"
        );
        assert!(polar.lift_to_drag > 0.0, "l/d={}", polar.lift_to_drag);
        assert!(polar.cl_trim > 0.0, "cl={}", polar.cl_trim);
        for value in [
            polar.alpha_deg,
            polar.incidence_deg,
            polar.x_np,
            polar.reference_area_m2,
            polar.mach,
            polar.altitude_m,
        ] {
            assert!(value.is_finite(), "{value}");
        }

        // The condition identity is the one that was actually flown.
        assert_eq!(polar.mach, config.requirements.cruise_mach);
        assert_eq!(polar.altitude_m, config.requirements.cruise_altitude_m);
        assert_eq!(polar.reference_area_m2, plane.s_ref);

        // The accepted point really is the requested cruise lift, trimmed.
        let atmo = Atmosphere::new(config.requirements.cruise_altitude_m);
        let velocity = config.requirements.cruise_mach * atmo.speed_of_sound();
        let q = 0.5 * atmo.density() * velocity * velocity;
        let cl_target = mass * config.requirements.gravity_m_s2 / (q * plane.s_ref);
        let relative = (polar.cl_trim - cl_target).abs() / cl_target;
        assert!(
            relative <= CL_TARGET_RELATIVE_TOLERANCE,
            "cl={} target={cl_target} relative={relative}",
            polar.cl_trim
        );
        let cm = polar.cm_residual.expect("a native trim reports a residual");
        assert!(cm.abs() <= CM_RESIDUAL_TOLERANCE, "cm={cm}");
    }

    /// The table the missions fly reproduces the trimmed solve's own total
    /// drag at the trim point, within the table's interpolation bound.
    #[test]
    fn the_table_reproduces_the_direct_trimmed_drag_at_the_trim_point() {
        let (config, dv, mut plane, cg_x) = default_candidate();
        let polar = trim_and_polar(&config, &mut plane, cg_x, &dv, config.requirements.mtow_kg)
            .unwrap_or_else(|f| panic!("{}", f.reason));
        // `lift_to_drag` is the trimmed re-solve's `CL / CD`.
        let direct_cd = polar.cl_trim / polar.lift_to_drag;
        let table_cd = polar.drag.cd(polar.cl_trim, polar.mach, polar.altitude_m);
        assert!(
            (table_cd - direct_cd).abs() <= crate::mdo::drag_table::CD_ERROR_BOUND,
            "table {table_cd:.7} direct {direct_cd:.7}"
        );
        // Moving the trim to its own lift coefficient changes nothing.
        let same = polar.at_cruise_cl(polar.cl_trim);
        assert_eq!(
            same.geometric_body_alpha_deg,
            polar.geometric_body_alpha_deg
        );
        assert_eq!(same.incidence_deg, polar.incidence_deg);
        assert!((same.alpha_deg - polar.alpha_deg).abs() < 1.0e-9);
    }

    /// A mass change no longer re-trims: the attitude the trim Jacobian
    /// moves to and the table's lift-to-drag ratio at the new lift agree with
    /// a direct trim there. The Jacobian step's error is second order in the
    /// lift change, so a 10 % mass change must stay inside the trim's own
    /// acceptance: 0.1 % of the lift-to-drag ratio (the lift tolerance
    /// `CL_TARGET_RELATIVE_TOLERANCE`) and a hundredth of a degree.
    #[test]
    fn a_mass_change_moves_the_trim_by_its_jacobian_without_a_resolve() {
        let (config, dv, mut plane, cg_x) = default_candidate();
        let mass = config.requirements.mtow_kg;
        let heavy = trim_and_polar(&config, &mut plane, cg_x, &dv, mass)
            .unwrap_or_else(|f| panic!("{}", f.reason));
        let light = trim_and_polar(&config, &mut plane, cg_x, &dv, 0.9 * mass)
            .unwrap_or_else(|f| panic!("{}", f.reason));
        let moved = heavy.at_cruise_cl(light.cl_trim);
        let ld_error = (moved.lift_to_drag - light.lift_to_drag).abs() / light.lift_to_drag;
        assert!(
            ld_error <= CL_TARGET_RELATIVE_TOLERANCE,
            "L/D {ld_error:.3e}"
        );
        for (name, jacobian, direct) in [
            (
                "alpha",
                moved.geometric_body_alpha_deg,
                light.geometric_body_alpha_deg,
            ),
            ("ih", moved.incidence_deg, light.incidence_deg),
            ("report alpha", moved.alpha_deg, light.alpha_deg),
        ] {
            assert!(
                (jacobian - direct).abs() <= 1.0e-2,
                "{name}: Jacobian {jacobian:.5} direct {direct:.5}"
            );
        }
    }

    #[test]
    fn a_cruise_lift_above_the_stall_margin_is_its_own_reason() {
        let (config, dv, mut plane, cg_x) = default_candidate();
        // Ten times the certified takeoff mass at the same speed and area
        // demands roughly ten times the cruise lift coefficient, far above
        // `max_cruise_cl`, without touching the solver's conditioning.
        let mass = config.requirements.mtow_kg * 10.0;
        let failure = trim_and_polar(&config, &mut plane, cg_x, &dv, mass)
            .expect_err("the requested cruise point is inside the stall margin");
        assert_eq!(failure.reason, TRIM_CRUISE_CL_EXCEEDS_MAX);
        assert_ne!(failure.reason, TRIM_SOLVE_FAILURE);
    }

    #[test]
    fn a_degenerate_cruise_lift_target_is_a_solver_failure_not_a_stall_margin() {
        let (config, dv, mut plane, cg_x) = default_candidate();
        let failure = trim_and_polar(&config, &mut plane, cg_x, &dv, 0.0)
            .expect_err("zero mass has no cruise lift coefficient");
        assert_eq!(failure.reason, TRIM_SOLVE_FAILURE);
    }

    #[test]
    fn an_external_polar_at_the_candidate_condition_is_adopted_unchanged() {
        let (config, _, plane, _) = default_candidate();
        let req = &config.requirements;
        let external = valid_external(req.cruise_mach, req.cruise_altitude_m, plane.s_ref);
        assert!(external.is_valid());
        assert!(external
            .matches_condition(
                req.cruise_mach,
                req.cruise_altitude_m,
                plane.s_ref,
                PolarConditionTolerance::default(),
            )
            .is_ok());
        let polar = TrimmedPolar::from_external(&external);
        let cd = external.cd0
            + external.induced_factor_k * external.target_cl * external.target_cl
            + external.wave_drag_cd;
        assert_eq!(
            polar
                .drag
                .cd(external.target_cl, external.mach, external.altitude_m),
            cd
        );
        assert_eq!(polar.cl_trim, external.target_cl);
        assert_eq!(polar.mach, external.mach);
        assert_eq!(polar.reference_area_m2, plane.s_ref);
        assert!(polar.converged);
        // An external solver reports no trimmed moment residual; the field
        // says so instead of fabricating a zero.
        assert_eq!(polar.cm_residual, None);
    }

    #[test]
    fn an_external_polar_from_another_condition_is_rejected() {
        let (config, _, plane, _) = default_candidate();
        let req = &config.requirements;
        for external in [
            valid_external(req.cruise_mach + 0.05, req.cruise_altitude_m, plane.s_ref),
            valid_external(req.cruise_mach, req.cruise_altitude_m + 1000.0, plane.s_ref),
            valid_external(req.cruise_mach, req.cruise_altitude_m, plane.s_ref * 1.1),
        ] {
            // The polar itself is usable; only the state it was solved at
            // disagrees with the candidate being sized.
            assert!(external.is_valid());
            let error = external
                .matches_condition(
                    req.cruise_mach,
                    req.cruise_altitude_m,
                    plane.s_ref,
                    PolarConditionTolerance::default(),
                )
                .expect_err("a polar from another condition cannot be reused");
            assert!(error.contains("does not match"), "{error}");
        }
    }

    #[test]
    fn an_external_polar_with_a_degenerate_induced_factor_is_rejected_not_floored() {
        let (config, _, plane, _) = default_candidate();
        let req = &config.requirements;
        for k in [0.0, -0.1, f64::NAN] {
            let mut external = valid_external(req.cruise_mach, req.cruise_altitude_m, plane.s_ref);
            external.induced_factor_k = k;
            // No floor exists to fall back on: the polar is simply invalid,
            // and `from_external` refuses to report it as converged.
            assert!(!external.is_valid(), "k={k}");
            assert!(!TrimmedPolar::from_external(&external).converged, "k={k}");
        }
    }
}
