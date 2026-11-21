// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lateral static stability of a sized candidate: the dihedral effect.
//!
//! # Rule
//!
//! The rolling moment due to sideslip must be stabilizing, `Cl_beta < 0`
//! (Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed., sec. 16.4,
//! p. 611: "a negative value of the rolling-moment derivative with respect
//! to beta is stabilizing (dihedral effect)"). This is the requirement the
//! geometric dihedral serves; the dihedral angle itself is only reported
//! against Raymer's initial-estimate guidelines (`residuals_layout`).
//!
//! # Estimate
//!
//! Raymer sec. 16.4.4, pp. 617-618, Eq. (16.47), builds the wing dihedral
//! effect from a sweep term, a geometric-dihedral term (Eq. 16.45) and a
//! wing vertical-position term (Eq. 16.46). Here the first two, and the
//! tail surfaces' own contribution, come from the candidate's vortex
//! lattice: the trimmed airframe the closure flew, solved at its trimmed
//! cruise attitude at `beta = +-1 deg` (a central difference), which carries
//! sweep, dihedral and fin height directly. The lattice has no fuselage, so
//! the wing-fuselage term is added from Eq. (16.46):
//!
//! `Cl_beta_wf = -1.2 sqrt(A) Z_wf (D_f + W_f) / b^2`, per radian,
//!
//! with `Z_wf` the height of the wing root above the fuselage centreline
//! (negative for a low wing, which makes the term positive, destabilizing),
//! `D_f` and `W_f` the fuselage depth and width (both the configured body
//! diameter), `A` the aspect ratio and `b` the span of the reference wing.
//!
//! # Scale
//!
//! The violation is normalized by one degree of effective dihedral,
//! `0.0002 per deg = 0.0115 per rad` (Raymer p. 617), so a residual of 1 is
//! one degree of effective dihedral short of neutral.

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_config::AlasConfig;

use super::sizing::SizingOutcome;
use super::types::ConstraintFamily::Performance;
use super::types::ConstraintResidual;
use super::ResidualRole;

/// One degree of effective dihedral, per radian (Raymer p. 617).
pub(crate) const EFFECTIVE_DIHEDRAL_DEGREE_PER_RAD: f64 = 0.0115;

/// Sideslip step of the central difference, degrees.
const SIDESLIP_STEP_DEG: f64 = 1.0;

/// Raymer Eq. (16.46): the wing vertical-position contribution to the
/// dihedral effect, per radian. `z_wf_m` is positive for a wing above the
/// fuselage centreline.
pub(crate) fn wing_fuselage_dihedral_effect_per_rad(
    aspect_ratio: f64,
    z_wf_m: f64,
    fuselage_depth_m: f64,
    fuselage_width_m: f64,
    span_m: f64,
) -> f64 {
    -1.2 * aspect_ratio.sqrt() * z_wf_m * (fuselage_depth_m + fuselage_width_m) / (span_m * span_m)
}

/// The dihedral-effect residual: `Cl_beta < 0`, scaled by one degree of
/// effective dihedral.
pub(crate) fn dihedral_effect_residual(
    cl_beta_per_rad: f64,
    detail: String,
    role: ResidualRole,
) -> ConstraintResidual {
    ConstraintResidual::direct(
        "lateral_dihedral_effect",
        Performance,
        cl_beta_per_rad,
        0.0,
        "1/rad",
        cl_beta_per_rad,
        (cl_beta_per_rad / EFFECTIVE_DIHEDRAL_DEGREE_PER_RAD).max(0.0),
        role,
    )
    .with_detail(detail)
}

/// The lateral static-stability residuals of one sized candidate. Empty for
/// a candidate trimmed by an external polar, which leaves no lattice to
/// solve in sideslip.
pub(super) fn lateral_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let Some(reuse) = outcome.trim_reuse.as_ref() else {
        return Vec::new();
    };
    let (plane, alpha_deg, cache) = reuse.trimmed_lattice();
    let failed = |reason: String| {
        vec![ConstraintResidual::direct(
            "lateral_dihedral_effect_unavailable",
            Performance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            role,
        )
        .with_detail(reason)]
    };
    let spanwise = usize::try_from(config.analysis.spanwise_resolution.max(1)).unwrap_or(1);
    let chordwise = usize::try_from(config.analysis.chordwise_resolution.max(1)).unwrap_or(1);
    let system = {
        let Ok(mut cache) = cache.lock() else {
            return failed("candidate lattice cache unavailable".to_owned());
        };
        match VlmSystem::assemble_cached(plane, spanwise, chordwise, &mut cache) {
            Ok(system) => system,
            Err(error) => return failed(format!("sideslip lattice: {error}")),
        }
    };
    let requirements = &config.requirements;
    let atmosphere = Atmosphere::new(requirements.cruise_altitude_m);
    let velocity_m_s = requirements.cruise_mach * atmosphere.speed_of_sound();
    let solve = |beta_deg: f64| {
        system.solve(&OperatingPoint::new(
            atmosphere,
            velocity_m_s,
            alpha_deg,
            beta_deg,
            0.0,
            0.0,
            0.0,
        ))
    };
    let (plus, minus) = match (solve(SIDESLIP_STEP_DEG), solve(-SIDESLIP_STEP_DEG)) {
        (Ok(plus), Ok(minus)) => (plus, minus),
        (Err(error), _) | (_, Err(error)) => return failed(format!("sideslip solve: {error}")),
    };
    let lattice_cl_beta = (plus.cl_roll - minus.cl_roll) / (2.0 * SIDESLIP_STEP_DEG.to_radians());
    let cl = 0.5 * (plus.cl_lift + minus.cl_lift);
    let span_m = plane.b_ref;
    let aspect_ratio = span_m * span_m / plane.s_ref;
    let z_wf_m = plane
        .wings
        .first()
        .map_or(f64::NAN, |wing| wing.design_root().0[2]);
    let diameter_m = config.geometry.fuselage.diameter_m;
    let body_cl_beta =
        wing_fuselage_dihedral_effect_per_rad(aspect_ratio, z_wf_m, diameter_m, diameter_m, span_m);
    let cl_beta = lattice_cl_beta + body_cl_beta;
    vec![dihedral_effect_residual(
        cl_beta,
        format!(
            "lattice {lattice_cl_beta:.4} /rad at alpha {alpha_deg:.2} deg (CL {cl:.3}) plus \
             wing-fuselage {body_cl_beta:.4} /rad (Raymer Eq. 16.46, Z_wf {z_wf_m:.2} m)"
        ),
        role,
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Eq. (16.46) is zero for a mid wing, destabilizing (positive) for a low
    /// wing and stabilizing for a high one, linear in the offset.
    #[test]
    fn the_wing_fuselage_term_follows_the_wing_height() {
        let term = |z| wing_fuselage_dihedral_effect_per_rad(9.0, z, 4.0, 4.0, 34.0);
        assert_eq!(term(0.0), 0.0);
        assert!(term(-1.0) > 0.0 && term(1.0) < 0.0);
        // -1.2 * 3 * 1 * 8 / 34^2
        assert!((term(1.0) + 1.2 * 3.0 * 8.0 / (34.0 * 34.0)).abs() < 1e-15);
    }

    /// A stabilizing dihedral effect passes; a positive one fails by its size
    /// in degrees of effective dihedral.
    #[test]
    fn a_positive_dihedral_effect_is_a_violation_in_degrees_of_effective_dihedral() {
        let stable = dihedral_effect_residual(-0.1, String::new(), ResidualRole::Constraint);
        assert!(!stable.violated());
        let unstable = dihedral_effect_residual(0.023, String::new(), ResidualRole::Constraint);
        assert!(unstable.violated());
        assert!((unstable.normalized_violation - 2.0).abs() < 1e-9);
    }
}
