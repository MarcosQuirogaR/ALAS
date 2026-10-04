// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Material-volume conservation and closed-form cantilever verification.

use super::*;
use alas_config::materials::MaterialSpec;

fn assert_roundoff_equal(actual: f64, expected: f64) {
    // These sums contain at most a few hundred positive terms. This bound
    // permits roundoff in the independent summation, not modelling error.
    assert!(
        (actual - expected).abs() <= 1.0e-12 * expected.abs().max(1.0),
        "actual={actual}, volume-derived={expected}"
    );
}

pub(super) fn assert_material_volumes(
    geometry: &WingStructureGeometry,
    sizing: &WingboxSizing,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) {
    let mut cap_volume = 0.0;
    let mut web_volume = 0.0;
    let mut cover_volume = 0.0;
    let (front, rear) = box_chord_band(geometry);
    for station in 0..sizing.y_stations.len() - 1 {
        let left = sizing.y_stations[station];
        let right = sizing.y_stations[station + 1];
        let span = right - left;
        cover_volume += (rear - front)
            * (sizing.chord[station] + sizing.chord[station + 1])
            * sizing.t_skin
            * span;
        for spar in &sizing.spars {
            let mut centre_lines = [[0.0; 3]; 2];
            for (end, &position) in [left, right].iter().enumerate() {
                let eta = position / geometry.semi_span;
                let chord = geometry.local_chord(eta);
                let (upper, lower) = geometry.airfoil_zu_zl(eta, spar.chord_fraction);
                let origin = geometry.z_le(eta);
                centre_lines[end] = [
                    geometry.x_le(eta) + spar.chord_fraction * chord,
                    origin + upper * chord - 0.5 * spar.t_cap[station + end],
                    origin + lower * chord + 0.5 * spar.t_cap[station + end],
                ];
            }
            let delta_x = centre_lines[1][0] - centre_lines[0][0];
            let upper_length = span
                .hypot(delta_x)
                .hypot(centre_lines[1][1] - centre_lines[0][1]);
            let lower_length = span
                .hypot(delta_x)
                .hypot(centre_lines[1][2] - centre_lines[0][2]);
            let middle_length = span.hypot(delta_x).hypot(
                0.5 * (centre_lines[1][1] + centre_lines[1][2]
                    - centre_lines[0][1]
                    - centre_lines[0][2]),
            );
            // Piecewise-linear section areas integrated along actual 3-D
            // material paths: V = sum((A_left+A_right)/2 * ds).
            cap_volume += 0.5
                * (spar.a_cap[station] + spar.a_cap[station + 1])
                * (upper_length + lower_length);
            web_volume +=
                0.5 * (spar.h[station] + spar.h[station + 1]) * spar.t_web * middle_length;
        }
    }
    assert_roundoff_equal(
        sizing.mass_breakdown_kg.spar_caps,
        cap.rho_kg_m3 * cap_volume,
    );
    assert_roundoff_equal(
        sizing.mass_breakdown_kg.spar_webs,
        web.rho_kg_m3 * web_volume,
    );
    assert_roundoff_equal(sizing.mass_breakdown_kg.skin, skin.rho_kg_m3 * cover_volume);
    assert_roundoff_equal(
        sizing.total_mass_kg,
        cap.rho_kg_m3 * cap_volume
            + web.rho_kg_m3 * web_volume
            + skin.rho_kg_m3 * cover_volume
            + sizing.mass_breakdown_kg.ribs,
    );
}

pub(super) fn assert_rib_material_volume(
    geometry: &WingStructureGeometry,
    sizing: &WingboxSizing,
    thickness: f64,
    density: f64,
    airfoils: [&Airfoil; 2],
) {
    let (front, rear) = box_chord_band(geometry);
    let mut knots = vec![front, rear];
    knots.extend(
        airfoils
            .into_iter()
            .flat_map(|airfoil| &airfoil.coordinates)
            .map(|&(x, _)| x)
            .filter(|&x| x > front && x < rear),
    );
    knots.sort_by(f64::total_cmp);
    knots.dedup();
    let mut volume = 0.0;
    let mut volume_error_bound = 0.0;
    assert!(sizing.num_ribs >= 2);
    for rib in 0..sizing.num_ribs {
        let eta = rib as f64 / (sizing.num_ribs - 1) as f64;
        let chord = geometry.local_chord(eta);
        let heights: Vec<_> = knots
            .iter()
            .map(|&x| geometry.spar_height(eta, x))
            .collect();
        let positions: Vec<_> = knots.iter().map(|x| x * chord).collect();
        // Airfoil surfaces and their spanwise blend are piecewise linear.
        // Integrating over the complete root/tip vertex union is exact;
        // it is independent of the production 60-node chord quadrature.
        let area: f64 = positions
            .windows(2)
            .zip(heights.windows(2))
            .map(|(x, h)| 0.5 * (x[1] - x[0]) * (h[0] + h[1]))
            .sum();
        let slopes: Vec<_> = positions
            .windows(2)
            .zip(heights.windows(2))
            .map(|(x, h)| (h[1] - h[0]) / (x[1] - x[0]))
            .collect();
        let slope_variation: f64 = slopes.windows(2).map(|s| (s[1] - s[0]).abs()).sum();
        // For a linear hinge inside [a,b], trapezoid minus exact area is
        // 0.5*DeltaSlope*(x-a)*(b-x). Thus every hinge contributes at most
        // |DeltaSlope|*dx^2/8. Summing absolute slope jumps proves a bound
        // for the production 60-node rule, without an empirical tolerance.
        let panel_width = (rear - front) * chord / 59.0;
        volume += area * thickness;
        volume_error_bound += panel_width.powi(2) * slope_variation * thickness / 8.0;
    }
    let exact_mass = density * volume;
    let mass_error_bound = density * volume_error_bound;
    assert!(
        (sizing.mass_breakdown_kg.ribs - exact_mass).abs()
            <= mass_error_bound + 1.0e-12 * exact_mass,
        "rib mass={}, exact volume mass={exact_mass}, quadrature bound={mass_error_bound}",
        sizing.mass_breakdown_kg.ribs
    );
}

fn rectangular_geometry() -> WingStructureGeometry {
    let design = DesignVector {
        span_m: 20.0,
        root_chord_m: 4.0,
        break_chord_m: 4.0,
        tip_chord_m: 4.0,
        sweep_deg: 0.0,
        ..DesignVector::default()
    };
    let airfoil = Airfoil::from_coordinates(
        "rectangular cover verification section",
        vec![
            (1.0, 0.0),
            (0.9, 0.06),
            (0.1, 0.06),
            (0.0, 0.0),
            (0.1, -0.06),
            (0.9, -0.06),
            (1.0, 0.0),
        ],
    );
    WingStructureGeometry::new(
        &design,
        &WingConfig::default(),
        &airfoil,
        &airfoil,
        &[0.25, 0.70],
        None,
    )
    .unwrap()
}

#[test]
fn rectangular_box_ribs_equal_their_closed_form_material_volume() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset":"A320-200"})).unwrap();
    let geometry = rectangular_geometry();
    let cfg = &config.structures;
    let material = materials::get("Al 7075-T6").unwrap();
    let dry = vec![0.0; sizing_stations(&geometry, cfg).len()];
    let sizing = size_wingbox_with_wing_carried_mass(
        &geometry,
        cfg,
        &config.requirements,
        material,
        material,
        material,
        material,
        Some(&dry),
        &[],
    );
    let box_width = (0.70 - 0.25) * geometry.c_root;
    let box_depth = sizing.spars[0].h[0];
    // A constant-depth rectangular rib has V = count * width * height * t.
    let rib_volume = sizing.num_ribs as f64 * box_width * box_depth * cfg.t_rib_m;
    assert_roundoff_equal(
        sizing.mass_breakdown_kg.ribs,
        material.rho_kg_m3 * rib_volume,
    );
}

#[test]
fn tip_point_load_recovers_closed_form_deflection_and_exact_refinement_error() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset":"A320-200"})).unwrap();
    let geometry = rectangular_geometry();
    let material = materials::get("Al 7075-T6").unwrap();
    let length = geometry.semi_span;
    let width = 0.12;
    let thickness = 0.008;
    let depth = 0.48_f64;
    let web_thickness = 0.004;
    let skin_thickness = 0.003;
    let cap_area = width * thickness;
    // Megson, 4th ed., ch. 20: compatible bending includes each material's
    // actual I. Equal-depth covers here have I = 2*b*t*(h/2)^2.
    let cap_inertia =
        4.0 * cap_area * (0.25 * (depth - thickness).powi(2) + thickness * thickness / 12.0);
    let web_inertia = 2.0 * web_thickness * depth.powi(3) / 12.0;
    let cover_inertia = 2.0 * 1.8 * skin_thickness * (0.5 * depth).powi(2);
    let stiffness = material.e_pa * (cap_inertia + web_inertia + cover_inertia);
    let mut previous_error: Option<f64> = None;
    for intervals in [20, 40, 80] {
        let count = intervals + 1;
        let mut sizing =
            dry_strength_size(&config, &presets::get("A320-200").unwrap().design_vector);
        sizing.y_stations = (0..count)
            .map(|station| length * station as f64 / intervals as f64)
            .collect();
        sizing.eta_stations = sizing.y_stations.iter().map(|y| y / length).collect();
        sizing.chord = vec![4.0; count];
        sizing.spar_fracs = vec![0.25, 0.70];
        for (spar, fraction) in sizing.spars.iter_mut().zip(&sizing.spar_fracs) {
            spar.chord_fraction = *fraction;
            spar.h = vec![depth; count];
            spar.w_cap = vec![width; count];
            spar.t_cap = vec![thickness; count];
            spar.a_cap = vec![cap_area; count];
            spar.t_web = web_thickness;
            spar.frac_moment = vec![0.5; count];
            spar.margin_of_safety = vec![0.0; count];
        }
        sizing.t_skin = skin_thickness;
        let dry = vec![0.0; count];
        let analyze = |points: &[(f64, f64)]| {
            analyze_structure_with_wing_carried_mass(
                &geometry,
                &sizing,
                &config.structures,
                &config.requirements,
                &config.geometry.engine,
                &config.mass_model,
                material,
                material,
                material,
                &dry,
                points,
            )
        };
        let baseline = analyze(&[]);
        let tip_mass = 1_000.0;
        let relieved = analyze(&[(length, tip_mass)]);
        for actual in &baseline.ei_nm2 {
            assert_roundoff_equal(*actual, stiffness);
        }
        let mut pull_up_error = 0.0;
        for (base, with_mass) in baseline.load_cases.iter().zip(&relieved.load_cases) {
            // Linear superposition isolates the tip force from aerodynamic
            // loads and uniform self-weight. Bruhn, 1973, A6: delta=F*L^3/(3EI).
            let force = base.load_factor * config.requirements.gravity_m_s2 * tip_mass;
            let expected = force * length.powi(3) / (3.0 * stiffness);
            let actual = base.tip_deflection_m - with_mass.tip_deflection_m;
            // Trapezoidal integration of (L-y)^2 adds L*h^2/6 exactly;
            // therefore its relative tip error is 1/(2*N^2), not a fit band.
            let relative_error = actual / expected - 1.0;
            let quadrature_error = 0.5 / (intervals * intervals) as f64;
            assert!((relative_error - quadrature_error).abs() < 1.0e-12);
            if base.name == "pull-up" {
                pull_up_error = relative_error;
            }
            assert_roundoff_equal(
                *base.deflection_m.last().unwrap() - with_mass.deflection_m.last().unwrap(),
                actual,
            );
        }
        if let Some(previous) = previous_error {
            // Both measured errors have the 1e-12 arithmetic budget above.
            // Its propagation into e_N/e_2N bounds deviation from four.
            assert!((previous / pull_up_error - 4.0).abs() < 5.0e-12 / pull_up_error);
        }
        previous_error = Some(pull_up_error);
    }
}
