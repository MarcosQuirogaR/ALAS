// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Aerodynamics/Common/Fidelity_Zero/Lift/VLM.py
// and the four modules it drives.
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! mission analysis model's VORLAX-derived vortex lattice method: the lift the mission flies
//! on.
//!
//! **This is not [`crate::vlm`].** That row is native aerodynamic model's vortex
//! lattice, reached from `alas/physics/aerodynamics.py`, and it answers the
//! design tool's own questions about an native aerodynamic model airplane. This one is
//! mission analysis model's, descended from the 1977 VORLAX code, and the only thing in the
//! whole reference that reaches it is the mission runner attaching
//! `mission analysis model.Analyses.Aerodynamics.Fidelity_Zero()` to an assembled vehicle.
//! The two are deliberately not unified, exactly as
//! `alas-prop::mission_turbofan` is not unified with `alas-prop::cycle`: they
//! panel differently, they impose the boundary condition differently, and
//! comparing their answers is a result the program is entitled to report.
//!
//! Nothing in the mission calls this directly either. `Fidelity_Zero` owns a
//! `Vortex_Lattice` sub-analysis whose `sample_training` runs [`run`] exactly
//! once, on a grid of ten angles of attack against eight Mach numbers, and
//! every lift and induced-drag number the mission then sees is a spline
//! through that grid. `alas-aero::lift_surrogate` is that spline; this row is
//! what it is a spline *of*.
//!
//! # How a solve goes
//!
//! [`distribution::generate`] panels the vehicle: each lifting surface into
//! spanwise strips and each strip into chordwise panels, with a bound vortex
//! a quarter of the way back and a control point three quarters of the way
//! back in every panel. [`induced::compute`] builds the influence of every
//! horseshoe on every control point. [`rhs::build`] states what the onset
//! flow does through each panel. Those two make a dense linear system whose
//! solution is the circulation on every panel, and [`forces::integrate`]
//! turns that field into eight coefficients.
//!
//! # Precision
//!
//! The panelization is stored in `f32` and the influence kernel runs
//! *entirely* in `f32`, because upstream's `settings.floating_point_precision`
//! is `np.float32` and `compute_wing_induced_velocity` casts to it
//! explicitly. The influence matrix and the circulation solve are `f64` over
//! those `f32` entries, which is upstream's arrangement too -- `np.linalg.solve`
//! receives a double array because the direction cosines it is multiplied by
//! are double. Every rounding point is marked where it happens. That is what
//! puts this row at `alas-testkit`'s `f32` tier, and the ledger records that
//! an all-`f64` path would be more accurate and is a P14 study rather than a
//! translation decision.
//!
//! # Scope
//!
//! Reached, and translated: the wing panelizer, the subsonic horseshoe
//! kernel, the mission analysis model boundary condition, and the whole of `PRESS` and `AERO`.
//!
//! Left untranslated, each unreachable from this program's inputs and each
//! recorded in the module that would have held it: the supersonic horseshoe
//! kernel and its sonic-vortex handling ([`induced`]); control-surface and
//! all-moving-surface panelization, with the span-break merge and the
//! quaternion hinge rotation it needs ([`wings`], [`distribution`]);
//! fuselage and nacelle lifting surfaces ([`distribution`]); the propeller
//! wake ([`rhs`]); and VORLAX's own `ALOC` boundary condition ([`rhs`]).
//! `gen_aero_vorlax.py` refuses to write a fixture in which any of the
//! settings that make those unreachable has moved.

mod distribution;
mod forces;
mod induced;
mod rhs;
mod types;
mod wings;

pub use types::{
    PanelCoordinates, VlmCaseResult, VlmCondition, VlmError, VlmGeometry, VlmResults, VlmSettings,
    VlmWing, VortexDistribution,
};
pub use wings::{convert_sweep_segments, span_breaks, SpanBreak, SweepSection};

/// What upstream substitutes for a zero freestream speed.
///
/// The rate terms are divided by the speed, so a literal zero is not usable.
/// `VLM` raises on one unless `use_surrogate` is set, and substitutes this
/// when it is -- which is the branch the training grid takes, because
/// `sample_training` builds its conditions with the velocity left at zero.
/// The substituted value is small enough that the rate terms stay zero to
/// machine precision when the rates themselves are zero, which they are on
/// every training point.
const ZERO_VELOCITY_SUBSTITUTE: f64 = 1e-6;

/// Panel a vehicle and solve it at every given flight condition.
///
/// The panelization depends on the geometry alone and is built once. The
/// influence matrix depends on the Mach number alone and is built once per
/// distinct one -- upstream does the same, through `np.unique`, and it is
/// what makes an eighty-point training grid affordable at eight Mach
/// numbers.
pub fn run(
    geometry: &VlmGeometry,
    settings: &VlmSettings,
    conditions: &[VlmCondition],
) -> Result<VlmResults, VlmError> {
    if let Some(mach) = conditions
        .iter()
        .map(|condition| condition.mach)
        .find(|&mach| !mach.is_finite() || !(0.0..1.0).contains(&mach))
    {
        return Err(VlmError::MachOutsideSubsonicDomain { mach });
    }
    let vd = distribution::generate(geometry, settings)?;
    let angles = rhs::TangencyAngles::compute(&vd);
    let n = vd.n_cp;

    // Group the conditions by Mach number. Two conditions with the same Mach
    // share an influence matrix *and* an assembled left-hand side, since the
    // direction cosines the matrix is contracted with come from the
    // panelization rather than from the condition -- so one elimination
    // answers every condition in the group.
    let mut groups: Vec<(f64, Vec<usize>)> = Vec::new();
    for (index, condition) in conditions.iter().enumerate() {
        match groups.iter_mut().find(|(m, _)| *m == condition.mach) {
            Some((_, members)) => members.push(index),
            None => groups.push((condition.mach, vec![index])),
        }
    }

    let mut cases: Vec<Option<VlmCaseResult>> = (0..conditions.len()).map(|_| None).collect();
    let mut solve_diagnostics = Vec::with_capacity(groups.len());

    for (mach, members) in groups {
        let (influence, bound) = induced::compute(&vd, mach);

        // The influence matrix contracted with the panel normals' direction
        // cosines. Katz and Plotkin's equation 7.42, and validated against it
        // in upstream's own comment.
        let matrix: Vec<Vec<f64>> = (0..n)
            .map(|m| {
                (0..n)
                    .map(|k| {
                        let c = influence.at(n, m, k);
                        let (sin_delta, cos_delta) = angles.delta[m].sin_cos();
                        let (sin_phi, cos_phi) = angles.phi[m].sin_cos();
                        f64::from(c[0]) * (sin_delta * cos_phi)
                            + f64::from(c[1]) * (cos_delta * sin_phi)
                            - f64::from(c[2]) * (cos_phi * cos_delta)
                    })
                    .collect()
            })
            .collect();

        let terms: Vec<rhs::RhsTerms> = members
            .iter()
            .map(|&index| {
                let condition = substitute_zero_velocity(conditions[index]);
                rhs::build(&vd, &angles, &condition, geometry.moment_reference_m)
            })
            .collect();

        let right: Vec<Vec<f64>> = (0..n)
            .map(|row| terms.iter().map(|t| t.rhs[row]).collect())
            .collect();
        let (solved, diagnostics) = alas_math::linalg::solve_with_diagnostics(&matrix, &right)
            .map_err(|step| VlmError::SingularInfluenceMatrix { step })?;
        if !diagnostics.residual_norm.is_finite()
            || !diagnostics.normalized_residual.is_finite()
            || !diagnostics.pivot_ratio.is_finite()
            || !diagnostics.minimum_pivot.is_finite()
        {
            return Err(VlmError::NonFiniteNumericalDiagnostics);
        }
        solve_diagnostics.push(diagnostics);

        for (column, (&index, term)) in members.iter().zip(&terms).enumerate() {
            let gamma: Vec<f64> = (0..n).map(|row| solved[row][column]).collect();
            let condition = substitute_zero_velocity(conditions[index]);
            cases[index] = Some(forces::integrate(&forces::LoadCase {
                vd: &vd,
                geometry,
                condition: &condition,
                phi: &angles.phi,
                gamma: &gamma,
                induced: &influence,
                semispan: &bound.semispan,
                rhs: term,
                leading_edge_suction_multiplier: settings.leading_edge_suction_multiplier,
            }));
        }
    }

    Ok(VlmResults {
        distribution: vd,
        cases: cases.into_iter().flatten().collect(),
        solve_diagnostics,
    })
}

/// Apply the zero-speed substitution one condition at a time.
fn substitute_zero_velocity(mut condition: VlmCondition) -> VlmCondition {
    if condition.velocity_m_s == 0.0 {
        condition.velocity_m_s = ZERO_VELOCITY_SUBSTITUTE;
    }
    condition
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn probe_wing(tag: &str, symmetric: bool, vertical: bool) -> VlmWing {
        VlmWing {
            tag: tag.to_string(),
            symmetric,
            vertical,
            vortex_lift: false,
            span_projected_m: 10.0,
            chord_root_m: 2.0,
            chord_tip_m: 1.0,
            taper: 0.5,
            aspect_ratio: 10.0 * 10.0 / 7.5,
            sweep_quarter_chord_rad: 0.2,
            sweep_leading_edge_rad: None,
            twist_root_rad: 0.02,
            twist_tip_rad: -0.01,
            dihedral_rad: 0.0,
            area_reference_m2: 7.5,
            origin_m: [0.0, 0.0, 0.0],
        }
    }

    fn probe_geometry() -> VlmGeometry {
        VlmGeometry {
            reference_area_m2: 7.5,
            center_of_gravity_m: [0.0, 0.0, 0.0],
            mean_aerodynamic_chord_m: 1.6,
            reference_span_m: 10.0,
            moment_reference_m: [0.5, 0.0],
            wings: vec![probe_wing("main_wing", true, false)],
        }
    }

    fn level(alpha_deg: f64) -> VlmCondition {
        VlmCondition {
            angle_of_attack_rad: alpha_deg.to_radians(),
            mach: 0.2,
            side_slip_angle_rad: 0.0,
            pitch_rate_rad_s: 0.0,
            roll_rate_rad_s: 0.0,
            yaw_rate_rad_s: 0.0,
            velocity_m_s: 68.0,
        }
    }

    #[test]
    fn a_symmetric_wing_becomes_two_surfaces_and_an_asymmetric_one_becomes_one() {
        let settings = VlmSettings::default();
        let mut geometry = probe_geometry();
        geometry.wings.push(probe_wing("fin", false, true));
        let results = run(&geometry, &settings, &[level(2.0)]).expect("solves");
        assert_eq!(results.distribution.n_w, 3);
        assert_eq!(
            results.distribution.n_cp,
            3 * settings.number_spanwise_vortices * settings.number_chordwise_vortices
        );
    }

    #[test]
    fn lift_grows_with_angle_of_attack_and_passes_through_a_negative_value() {
        let geometry = probe_geometry();
        let settings = VlmSettings::default();
        let results = run(
            &geometry,
            &settings,
            &[level(-4.0), level(0.0), level(4.0), level(8.0)],
        )
        .expect("solves");
        assert_eq!(results.solve_diagnostics.len(), 1);
        assert!(results.solve_diagnostics[0].normalized_residual.is_finite());
        let lifts: Vec<f64> = results.cases.iter().map(|c| c.cl).collect();
        assert!(
            lifts[0] < 0.0,
            "a wing this weakly cambered stalls negative at -4 deg"
        );
        for pair in lifts.windows(2) {
            assert!(
                pair[1] > pair[0],
                "lift is monotone in angle of attack here"
            );
        }
    }

    #[test]
    fn a_symmetric_aircraft_at_zero_sideslip_carries_no_side_force_or_yawing_moment() {
        let geometry = probe_geometry();
        let results = run(&geometry, &VlmSettings::default(), &[level(3.0)]).expect("solves");
        let case = &results.cases[0];
        // Exactly zero in closed form. In floating point the two sides
        // cancel only as far as the panelization lets them, and the
        // panelization is `f32`: the side force and yawing moment come back
        // at 1e-8 and the rolling moment two decades worse, because it is
        // the one that multiplies by the single-precision strip area twice
        // over. The reference shows the same asymmetry, at the same size, on
        // the real aircraft.
        assert!(case.cytot.abs() < 1e-6);
        assert!(case.cntot.abs() < 1e-6);
        assert!(case.crtot.abs() < 1e-4);
    }

    #[test]
    fn induced_drag_is_never_negative_on_a_planar_wing() {
        let geometry = probe_geometry();
        let results = run(
            &geometry,
            &VlmSettings::default(),
            &[level(-4.0), level(0.0), level(6.0)],
        )
        .expect("solves");
        for case in &results.cases {
            assert!(case.cdi >= 0.0);
        }
    }

    #[test]
    fn conditions_sharing_a_mach_number_are_solved_on_one_influence_matrix() {
        // Two conditions at the same Mach must come back in the order they
        // were given, not in the order the grouping visited them.
        let geometry = probe_geometry();
        let mut fast = level(6.0);
        fast.mach = 0.7;
        let results = run(
            &geometry,
            &VlmSettings::default(),
            &[level(2.0), fast, level(4.0)],
        )
        .expect("solves");
        assert!(results.cases[0].cl < results.cases[2].cl);
    }

    #[test]
    fn a_vehicle_with_no_wings_is_an_error_rather_than_an_empty_solve() {
        let mut geometry = probe_geometry();
        geometry.wings.clear();
        assert_eq!(
            run(&geometry, &VlmSettings::default(), &[level(2.0)]),
            Err(VlmError::NoWings)
        );
    }

    #[test]
    fn a_zero_freestream_speed_is_substituted_rather_than_dividing_by_zero() {
        let geometry = probe_geometry();
        let mut still = level(5.0);
        still.mach = 0.0;
        still.velocity_m_s = 0.0;
        let results = run(&geometry, &VlmSettings::default(), &[still]).expect("solves");
        assert!(results.cases[0].cl.is_finite());
        assert!(results.cases[0].cl > 0.0);
    }

    #[test]
    fn nonfinite_or_supersonic_mach_is_rejected_before_the_subsonic_kernel() {
        let geometry = probe_geometry();
        let settings = VlmSettings::default();
        for mach in [1.0, -0.1, f64::NAN] {
            let condition = VlmCondition { mach, ..level(0.0) };
            assert!(matches!(
                run(&geometry, &settings, &[condition]),
                Err(VlmError::MachOutsideSubsonicDomain { .. })
            ));
        }
    }
}
