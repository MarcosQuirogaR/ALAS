// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_atmo::Atmosphere;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    fn flat_rectangular_wing(symmetric: bool) -> Wing {
        Wing::new(
            "Flat",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 1.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 5.0, 0.0], 1.0, 0.0, naca("naca0012")),
            ],
            symmetric,
        )
    }

    fn single_wing_airplane(symmetric: bool) -> Airplane {
        let wing = flat_rectangular_wing(symmetric);
        // The probe represents a product/reference aircraft, so its
        // coefficient scales must use the projected XY reference plane.
        let s_ref = wing.reference_area();
        let b_ref = wing.reference_span();
        let c_ref = wing.mean_aerodynamic_chord();
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [0.25, 0.0, 0.0],
            wings: vec![wing],
            fuselages: Vec::new(),
            s_ref,
            c_ref,
            b_ref,
        }
    }

    fn level_flight_point(alpha: f64) -> OperatingPoint {
        OperatingPoint::new(Atmosphere::new(0.0), 50.0, alpha, 0.0, 0.0, 0.0, 0.0)
    }

    #[test]
    fn a_symmetric_flat_plate_at_zero_alpha_produces_no_lift() {
        let airplane = single_wing_airplane(true);
        let op_point = level_flight_point(0.0);
        let result = run(&airplane, &op_point, 1, 4).expect("well-posed solve");
        assert!(
            result.lift.abs() < 1.0,
            "an uncambered symmetric wing at zero incidence should carry ~no lift, got {}",
            result.lift
        );
    }

    #[test]
    fn positive_angle_of_attack_produces_positive_lift() {
        let airplane = single_wing_airplane(true);
        let op_point = level_flight_point(5.0);
        let result = run(&airplane, &op_point, 1, 4).expect("well-posed solve");
        assert!(result.lift > 0.0, "lift={}", result.lift);
        assert!(result.cl_lift > 0.0);
    }

    #[test]
    fn a_symmetric_wing_at_zero_beta_produces_no_side_force_or_yaw_or_roll() {
        let airplane = single_wing_airplane(true);
        let op_point = level_flight_point(4.0);
        let result = run(&airplane, &op_point, 1, 4).expect("well-posed solve");
        assert!(result.side_force.abs() < 1e-6, "Y={}", result.side_force);
        assert!(result.yaw_moment.abs() < 1e-6, "n_b={}", result.yaw_moment);
        assert!(
            result.roll_moment.abs() < 1e-6,
            "l_b={}",
            result.roll_moment
        );
    }

    #[test]
    fn vortex_strengths_has_one_entry_per_panel() {
        let airplane = single_wing_airplane(false);
        let op_point = level_flight_point(3.0);
        let result = run(&airplane, &op_point, 1, 4).expect("well-posed solve");
        // 1 spanwise interval * 4 chordwise panels, unmirrored.
        assert_eq!(result.vortex_strengths.len(), 4);
    }

    #[test]
    fn spanwise_resolution_above_one_multiplies_the_panel_count() {
        let airplane = single_wing_airplane(false);
        let op_point = level_flight_point(3.0);
        let coarse = run(&airplane, &op_point, 1, 2).expect("well-posed solve");
        let fine = run(&airplane, &op_point, 3, 2).expect("well-posed solve");
        assert_eq!(coarse.vortex_strengths.len(), 2);
        assert_eq!(fine.vortex_strengths.len(), 3 * 2);
    }

    #[test]
    fn doubling_the_freestream_velocity_leaves_the_lift_coefficient_unchanged() {
        // CL depends on alpha, not on the airspeed itself, for an inviscid
        // linear solve -- doubling V quadruples both L and q, so CL should be
        // invariant.
        let airplane = single_wing_airplane(true);
        let slow = OperatingPoint::new(Atmosphere::new(0.0), 40.0, 4.0, 0.0, 0.0, 0.0, 0.0);
        let fast = OperatingPoint::new(Atmosphere::new(0.0), 80.0, 4.0, 0.0, 0.0, 0.0, 0.0);
        let slow_result = run(&airplane, &slow, 1, 4).expect("well-posed solve");
        let fast_result = run(&airplane, &fast, 1, 4).expect("well-posed solve");
        assert!(
            (slow_result.cl_lift - fast_result.cl_lift).abs() < 1e-9,
            "slow CL={} fast CL={}",
            slow_result.cl_lift,
            fast_result.cl_lift
        );
    }

    #[test]
    fn one_assembled_system_solves_exactly_like_one_run_per_point() {
        let airplane = single_wing_airplane(true);
        let system = VlmSystem::assemble(&airplane, 2, 3).expect("well-posed mesh");
        assert_eq!(system.panel_count(), 2 * 3 * 2);
        for alpha in [-2.0, 0.0, 3.0, 7.5] {
            let op_point = level_flight_point(alpha);
            let shared = system.solve(&op_point).expect("well-posed solve");
            let alone = run(&airplane, &op_point, 2, 3).expect("well-posed solve");
            assert_eq!(shared, alone, "alpha={alpha}");
        }
    }

    #[test]
    fn rate_derivatives_are_invariant_to_a_rigid_translation_about_xyz_ref() {
        let airplane = single_wing_airplane(true);
        // A symmetric wing is mirrored about the global XZ plane, so keep the
        // translation in the aircraft's symmetry-preserving x/z directions.
        let translation = [37.0, 0.0, 4.5];
        let mut translated = airplane.clone();
        translated.xyz_ref = [
            airplane.xyz_ref[0] + translation[0],
            airplane.xyz_ref[1] + translation[1],
            airplane.xyz_ref[2] + translation[2],
        ];
        translated.wings = airplane
            .wings
            .iter()
            .map(|wing| {
                let xsecs = wing
                    .xsecs
                    .iter()
                    .map(|xsec| xsec.translate(translation))
                    .collect();
                Wing::new(wing.name.clone(), xsecs, wing.symmetric)
            })
            .collect();

        let op_point =
            OperatingPoint::new(Atmosphere::new(0.0), 50.0, 4.0, 1.0, 0.003, 0.004, 0.005);
        let original = run_with_stability_derivatives(&airplane, &op_point, 2, 3)
            .expect("original solve should be well posed");
        let shifted = run_with_stability_derivatives(&translated, &op_point, 2, 3)
            .expect("translated solve should be well posed");

        let differences = [
            ("Clp", original.d_p.cl_lift, shifted.d_p.cl_lift),
            ("Cmp", original.d_p.cm_pitch, shifted.d_p.cm_pitch),
            ("CLq", original.d_q.cl_lift, shifted.d_q.cl_lift),
            ("Cmq", original.d_q.cm_pitch, shifted.d_q.cm_pitch),
            ("CYr", original.d_r.cy_side, shifted.d_r.cy_side),
            ("Cnr", original.d_r.cn_yaw, shifted.d_r.cn_yaw),
        ];
        for (name, before, after) in differences {
            assert!(
                (before - after).abs() < 1e-8,
                "{name}: {before} changed to {after}"
            );
        }
    }

    #[test]
    fn a_zero_area_panel_is_rejected_before_normalization() {
        let result = system::Panel::from_quad(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            false,
            3,
        );
        assert!(matches!(
            result,
            Err(VlmError::DegeneratePanel { wing_index: 3 })
        ));
    }

    #[test]
    fn nonpositive_or_nonfinite_derivative_steps_are_rejected() {
        let airplane = single_wing_airplane(true);
        let op_point = level_flight_point(4.0);
        for (angle_step, rate_step) in [(0.0, 0.001), (-0.001, 0.001), (f64::NAN, 0.001)] {
            assert_eq!(
                run_with_stability_derivatives_with_steps(
                    &airplane, &op_point, 2, 3, angle_step, rate_step,
                ),
                Err(VlmError::InvalidDerivativeStep)
            );
        }
    }
}
