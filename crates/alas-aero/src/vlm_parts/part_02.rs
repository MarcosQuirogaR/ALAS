// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn run_with_rotation_reference(
    airplane: &Airplane,
    op_point: &OperatingPoint,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
    rotation_reference: [f64; 3],
) -> Result<VlmResult, VlmError> {
    let panels = mesh_panels(airplane, spanwise_resolution, chordwise_resolution)?;
    let n = panels.len();

    let steady_freestream_velocity = op_point.freestream_velocity_geometry_axes();

    let collocation_points: Vec<[f64; 3]> = panels.iter().map(|p| p.collocation_point).collect();
    let rotation_at_collocation =
        op_point.rotation_velocity_geometry_axes_about(&collocation_points, rotation_reference);

    let freestream_influences: Vec<f64> = panels
        .iter()
        .zip(&rotation_at_collocation)
        .map(|(panel, &rotation_velocity)| {
            let freestream_velocity = add3(steady_freestream_velocity, rotation_velocity);
            dot3(freestream_velocity, panel.normal_direction)
        })
        .collect();

    // AIC[i][j]: the velocity horseshoe j (unit strength) induces at
    // collocation point i, dotted with that collocation panel's own normal.
    let mut aic = vec![vec![0.0; n]; n];
    for (i, collocation_panel) in panels.iter().enumerate() {
        for (j, source_panel) in panels.iter().enumerate() {
            let induced = calculate_induced_velocity_horseshoe(
                collocation_panel.collocation_point,
                source_panel.left_vortex_vertex,
                source_panel.right_vortex_vertex,
                TRAILING_VORTEX_DIRECTION,
                1.0,
                VORTEX_CORE_RADIUS,
            );
            aic[i][j] = dot3(induced, collocation_panel.normal_direction);
        }
    }

    let rhs: Vec<Vec<f64>> = freestream_influences
        .iter()
        .map(|&value| vec![-value])
        .collect();
    let (solved, solve_diagnostics) =
        linalg::solve_with_diagnostics(&aic, &rhs).map_err(VlmError::SingularAic)?;
    if !solve_diagnostics.residual_norm.is_finite()
        || !solve_diagnostics.normalized_residual.is_finite()
        || !solve_diagnostics.pivot_ratio.is_finite()
        || !solve_diagnostics.minimum_pivot.is_finite()
    {
        return Err(VlmError::NonFiniteResult);
    }
    let vortex_strengths: Vec<f64> = solved.into_iter().map(|row| row[0]).collect();

    let vortex_centers: Vec<[f64; 3]> = panels.iter().map(|p| p.vortex_center).collect();
    let v_centers = velocity_at_points(
        &vortex_centers,
        &panels,
        &vortex_strengths,
        op_point,
        steady_freestream_velocity,
        rotation_reference,
    );

    let density = op_point.atmosphere.density();
    let mut force_geometry = [0.0; 3];
    let mut moment_geometry = [0.0; 3];
    // Recorded per panel as the loop goes, alongside the running totals it
    // has always computed -- same operations in the same order, so the totals
    // above are unaffected; this is only an additional read-out.
    let mut panel_forces_geometry = Vec::with_capacity(n);
    for ((panel, &gamma), &v_center) in panels.iter().zip(&vortex_strengths).zip(&v_centers) {
        let vi_cross_li = cross3(v_center, panel.vortex_bound_leg);
        let force_panel = scale3(vi_cross_li, density * gamma);
        let moment_panel = cross3(sub3(panel.vortex_center, airplane.xyz_ref), force_panel);
        force_geometry = add3(force_geometry, force_panel);
        moment_geometry = add3(moment_geometry, moment_panel);
        panel_forces_geometry.push(force_panel);
    }

    let (fbx, fby, fbz) = op_point.convert_axes(
        force_geometry[0],
        force_geometry[1],
        force_geometry[2],
        AxisFrame::Geometry,
        AxisFrame::Body,
    );
    let force_body = [fbx, fby, fbz];
    let (fwx, fwy, fwz) = op_point.convert_axes(
        force_body[0],
        force_body[1],
        force_body[2],
        AxisFrame::Body,
        AxisFrame::Wind,
    );
    let force_wind = [fwx, fwy, fwz];

    let (mbx, mby, mbz) = op_point.convert_axes(
        moment_geometry[0],
        moment_geometry[1],
        moment_geometry[2],
        AxisFrame::Geometry,
        AxisFrame::Body,
    );
    let moment_body = [mbx, mby, mbz];
    let (mwx, mwy, mwz) = op_point.convert_axes(
        moment_body[0],
        moment_body[1],
        moment_body[2],
        AxisFrame::Body,
        AxisFrame::Wind,
    );
    let moment_wind = [mwx, mwy, mwz];

    let lift = -force_wind[2];
    let drag = -force_wind[0];
    let side_force = force_wind[1];
    let roll_moment = moment_body[0];
    let pitch_moment = moment_body[1];
    let yaw_moment = moment_body[2];

    let q = op_point.dynamic_pressure();
    let s_ref = airplane.s_ref;
    let b_ref = airplane.b_ref;
    let c_ref = airplane.c_ref;
    let cl_lift = lift / q / s_ref;
    let cd_drag = drag / q / s_ref;
    let cy_side = side_force / q / s_ref;
    let cl_roll = roll_moment / q / s_ref / b_ref;
    let cm_pitch = pitch_moment / q / s_ref / c_ref;
    let cn_yaw = yaw_moment / q / s_ref / b_ref;
    let coefficient_values = [
        lift,
        drag,
        side_force,
        roll_moment,
        pitch_moment,
        yaw_moment,
        cl_lift,
        cd_drag,
        cy_side,
        cl_roll,
        cm_pitch,
        cn_yaw,
    ];
    if !force_geometry
        .iter()
        .chain(force_body.iter())
        .chain(force_wind.iter())
        .chain(moment_geometry.iter())
        .chain(moment_body.iter())
        .chain(moment_wind.iter())
        .chain(coefficient_values.iter())
        .all(|value| value.is_finite())
    {
        return Err(VlmError::NonFiniteResult);
    }

    Ok(VlmResult {
        force_geometry,
        force_body,
        force_wind,
        moment_geometry,
        moment_body,
        moment_wind,
        lift,
        drag,
        side_force,
        roll_moment,
        pitch_moment,
        yaw_moment,
        cl_lift,
        cd_drag,
        cy_side,
        cl_roll,
        cm_pitch,
        cn_yaw,
        vortex_strengths,
        panels: panels
            .iter()
            .map(|p| streamlines::PanelSample {
                front_left: p.front_left,
                back_left: p.back_left,
                back_right: p.back_right,
                front_right: p.front_right,
                left_vortex_vertex: p.left_vortex_vertex,
                right_vortex_vertex: p.right_vortex_vertex,
                vortex_center: p.vortex_center,
                is_trailing_edge: p.is_trailing_edge,
                wing_index: p.wing_index,
            })
            .collect(),
        panel_forces_geometry,
        solve_diagnostics,
    })
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_atmo::Atmosphere;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::WingXSec;

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
        let result = Panel::from_quad(
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

