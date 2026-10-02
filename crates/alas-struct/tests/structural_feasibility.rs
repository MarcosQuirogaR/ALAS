// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Numerical verification of the linear-model domain gate and matched loads.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_struct::analytical::{
    LoadCaseResult, ModalResult, SparStressResult, StructuralAnalysisReport,
};
use alas_struct::feasibility::{assess, LinearModelLimits};
use alas_struct::sizing::{MassBreakdown, SparSizing, WingboxSizing};

/// Exact uniformly loaded cantilever; all dimensional values are SI.
fn beam(level_tip_m: f64, semispan_m: f64) -> (WingboxSizing, StructuralAnalysisReport) {
    let count = 101;
    let y: Vec<f64> = (0..count)
        .map(|i| semispan_m * i as f64 / (count - 1) as f64)
        .collect();
    let ei = 1.0e8;
    let q = level_tip_m * 8.0 * ei / semispan_m.powi(4);
    let sizing = WingboxSizing {
        y_stations: y.clone(),
        eta_stations: y.iter().map(|v| v / semispan_m).collect(),
        chord: vec![2.0; count],
        spar_fracs: vec![0.25],
        spars: vec![SparSizing {
            chord_fraction: 0.25,
            h: vec![1.0; count],
            w_cap: vec![0.1; count],
            t_cap: vec![0.01; count],
            a_cap: vec![0.001; count],
            t_web: 0.002,
            frac_moment: vec![1.0; count],
            margin_of_safety: vec![0.0; count],
        }],
        t_skin: 0.006,
        num_ribs: 101,
        rib_spacing_m: semispan_m / 100.0,
        mass_breakdown_kg: MassBreakdown {
            spar_caps: 10.0,
            spar_webs: 10.0,
            skin: 10.0,
            ribs: 10.0,
        },
        total_mass_kg: 40.0,
        sizing_load_case: "pull-up",
        composite_declaration: None,
    };
    let load_cases = [("pull-up", 3.75), ("push-down", -1.5), ("level", 1.0)]
        .into_iter()
        .map(|(name, load_factor)| LoadCaseResult {
            name,
            load_factor,
            y: y.clone(),
            q_net: vec![q * load_factor; count],
            shear_n: y
                .iter()
                .map(|v| q * load_factor * (semispan_m - v))
                .collect(),
            moment_nm: y
                .iter()
                .map(|v| q * load_factor * (semispan_m - v).powi(2) / 2.0)
                .collect(),
            deflection_m: y
                .iter()
                .map(|v| {
                    q * load_factor
                        * v.powi(2)
                        * (6.0 * semispan_m.powi(2) - 4.0 * semispan_m * v + v.powi(2))
                        / (24.0 * ei)
                })
                .collect(),
            tip_deflection_m: load_factor * level_tip_m,
            spar_stress: vec![SparStressResult {
                chord_fraction: 0.25,
                stress_pa: vec![1.0e6; count],
                margin_of_safety: vec![1.0; count],
            }],
        })
        .collect();
    (
        sizing,
        StructuralAnalysisReport {
            y,
            ei_nm2: vec![ei; count],
            load_cases,
            modal: ModalResult {
                frequencies_hz: vec![],
                mode_shapes: vec![],
            },
        },
    )
}

#[test]
fn a_small_deflection_uniform_beam_passes_with_the_expected_slope() {
    let (sizing, report) = beam(0.1, 10.0);
    let result = assess(&sizing, &report, LinearModelLimits::default());
    // Uniform cantilever: tip slope = (4/3) tip deflection / L, gated on the
    // 1 g flight shape.
    let expected_slope = (4.0 / 3.0) * 0.1 / 10.0;
    assert!(result.passes());
    assert!((result.max_abs_slope / expected_slope - 1.0).abs() < 6.0e-5);
    assert_eq!(result.governing_load_case, "level");
    assert_eq!(result.governing_load_factor, 1.0);
}

#[test]
fn a_flight_shape_outside_the_budget_fails_and_its_ultimate_error_is_reported() {
    // 5.3 m at 1 g on a 33 m semispan: a 1 g tip slope of 0.215 rad.
    let (sizing, report) = beam(20.0 / 3.75, 33.0);
    let result = assess(&sizing, &report, LinearModelLimits::default());
    assert!(result.input_valid);
    assert!(!result.passes());
    assert!(result.max_linear_curvature_relative_error > 0.05);
    assert!(result.manoeuvre_curvature_relative_error > 1.0);
    assert!((result.max_tip_deflection_ratio - 20.0 / 33.0).abs() < 1.0e-12);
}

#[test]
fn an_ultimate_deflection_like_the_787_static_test_is_reported_but_does_not_gate() {
    // Boeing 787 ultimate-load wing test (news release, 28 March 2010): about
    // 7.6 m of tip rise on a 30 m semispan. A certified wing does this, so the
    // linear small-slope budget cannot be a requirement on it.
    let (sizing, report) = beam(7.6 / 3.75, 30.0);
    let result = assess(&sizing, &report, LinearModelLimits::default());
    assert!(result.passes(), "{result:#?}");
    assert!(result.manoeuvre_curvature_relative_error > 0.05);
    assert!((result.max_tip_deflection_ratio - 7.6 / 30.0).abs() < 1.0e-12);
}

#[test]
fn domain_is_dimensionless_and_not_a_metre_cutoff() {
    let (sizing_a, report_a) = beam(0.1, 10.0);
    let (sizing_b, report_b) = beam(1.0, 100.0);
    let a = assess(&sizing_a, &report_a, LinearModelLimits::default());
    let b = assess(&sizing_b, &report_b, LinearModelLimits::default());
    assert!(
        (a.max_linear_curvature_relative_error - b.max_linear_curvature_relative_error).abs()
            < 1.0e-12
    );
}

#[test]
fn invalid_response_and_configuration_fail_closed() {
    let (sizing, report) = beam(0.1, 10.0);
    let mut malformed = report.clone();
    malformed.ei_nm2[2] = 0.0;
    assert!(!assess(&sizing, &malformed, LinearModelLimits::default()).input_valid);
    malformed = report.clone();
    malformed.load_cases[0].moment_nm[1] = f64::NAN;
    assert!(!assess(&sizing, &malformed, LinearModelLimits::default()).input_valid);
    malformed = report.clone();
    malformed.load_cases.pop();
    assert!(!assess(&sizing, &malformed, LinearModelLimits::default()).input_valid);
    malformed = report.clone();
    malformed.load_cases[0].deflection_m.pop();
    assert!(!assess(&sizing, &malformed, LinearModelLimits::default()).input_valid);
    assert!(
        !assess(
            &sizing,
            &report,
            LinearModelLimits {
                max_curvature_relative_error: f64::NAN
            }
        )
        .input_valid
    );
}

#[test]
fn strength_and_rib_deficits_are_independent_of_small_deflection() {
    let (mut sizing, report) = beam(0.1, 10.0);
    sizing.spars[0].margin_of_safety[1] = -0.2;
    let result = assess(&sizing, &report, LinearModelLimits::default());
    assert_eq!(result.max_strength_utilization, 1.25);
    assert!(!result.passes());
    sizing.spars[0].margin_of_safety[1] = f64::INFINITY;
    sizing.num_ribs = 51;
    let result = assess(&sizing, &report, LinearModelLimits::default());
    assert!(result.input_valid);
    assert_eq!(result.rib_spacing_ratio, 2.0);
    assert!(!result.passes());
}

#[test]
fn a_failed_response_stress_cannot_hide_behind_successful_sizing() {
    let (sizing, mut report) = beam(0.1, 10.0);
    report.load_cases[1].spar_stress[0].margin_of_safety[20] = -0.5;
    let result = assess(&sizing, &report, LinearModelLimits::default());
    assert!(result.input_valid);
    assert_eq!(result.max_strength_utilization, 2.0);
    assert!(!result.passes());
    report.load_cases[1].spar_stress[0].margin_of_safety[20] = -1.0;
    assert!(!assess(&sizing, &report, LinearModelLimits::default()).passes());
}

#[test]
fn optional_mesh_mass_is_absent_for_native_assessment_and_invalid_values_fail() {
    let (sizing, report) = beam(0.1, 10.0);
    let mut result = assess(&sizing, &report, LinearModelLimits::default());
    assert_eq!(result.mesh_primary_mass_kg, None);
    assert!(result.passes());
    for mass in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
        result.mesh_primary_mass_kg = Some(mass);
        assert!(!result.passes());
    }
    result.mesh_primary_mass_kg = Some(10.0);
    assert!(
        result.passes(),
        "empirical mass comparisons are diagnostic only"
    );
    for mass in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
        result.primary_mass_kg = mass;
        assert!(!result.passes());
    }
}

#[test]
fn overlapping_cap_flanges_are_not_a_feasible_way_to_add_stiffness() {
    let (mut sizing, mut report) = beam(0.1, 10.0);
    let mut second = sizing.spars[0].clone();
    second.chord_fraction = 0.27;
    sizing.spars.push(second);
    sizing.spar_fracs.push(0.27);
    for case in &mut report.load_cases {
        let mut second = case.spar_stress[0].clone();
        second.chord_fraction = 0.27;
        case.spar_stress.push(second);
    }
    let result = assess(&sizing, &report, LinearModelLimits::default());
    assert!(result.input_valid);
    assert!(result.cap_packaging_ratio > 2.0);
    assert!(!result.passes());
}

#[test]
fn explicit_fuel_relief_is_used_in_the_analyzed_design_load() {
    use alas_config::{
        materials, DesignRequirements, DesignVector, GeometryConfig, MassModelConfig,
        StructuresConfig,
    };
    use alas_geom::airfoil_library::AirfoilLibrary;
    use alas_geom::wing_structure::WingStructureGeometry;
    use alas_struct::{analytical, loads, sizing};
    let cfg = StructuresConfig::default();
    let geometry = GeometryConfig::default();
    let section = AirfoilLibrary::get("naca2412").unwrap();
    let wsg = WingStructureGeometry::new(
        &DesignVector::default(),
        &geometry.wing,
        &section,
        &section,
        &[0.25, 0.70],
        None,
    )
    .unwrap();
    let req = DesignRequirements::default();
    let material = materials::get("Al 7075-T6").unwrap();
    let stations = sizing::sizing_stations(&wsg, &cfg);
    let fuel = vec![100.0; stations.len()];
    let sized = sizing::size_wingbox_with_wing_carried_mass(
        &wsg,
        &cfg,
        &req,
        material,
        material,
        material,
        material,
        Some(&fuel),
        &[],
    );
    let report = analytical::analyze_structure_with_wing_carried_mass(
        &wsg,
        &sized,
        &cfg,
        &req,
        &geometry.engine,
        &MassModelConfig::default(),
        material,
        material,
        material,
        &fuel,
        &[],
    );
    let (front, rear) = sizing::box_chord_band(&wsg);
    let structural_mass =
        sizing::box_running_mass_kg_m(&sized, material, material, material, front, rear);
    for (result, case) in report
        .load_cases
        .iter()
        .zip(loads::load_cases(&req, cfg.additional_safety_factor))
    {
        let aero = loads::elliptic_distributed_load(&stations, wsg.semi_span, case.total_force_n);
        for i in 0..stations.len() {
            let expected =
                aero[i] - case.load_factor * req.gravity_m_s2 * (structural_mass[i] + fuel[i]);
            assert!((result.q_net[i] - expected).abs() < 1.0e-8);
        }
    }
    let before = assess(&sized, &report, LinearModelLimits::default());
    assert!(before.input_valid);
    let target = LinearModelLimits {
        max_curvature_relative_error: before.max_linear_curvature_relative_error / 2.0,
    };
    let stiffened = sizing::size_for_linear_model(
        &wsg,
        sized.clone(),
        &cfg,
        &req,
        &geometry.engine,
        &MassModelConfig::default(),
        material,
        material,
        material,
        &fuel,
        &[],
        target,
    );
    assert!(stiffened.converged, "{:#?}", stiffened.assessment);
    assert!(stiffened.assessment.passes());
    assert!(stiffened.assessment.max_linear_curvature_relative_error
        >= 0.99 * target.max_curvature_relative_error,
        "self-weight redistribution should use the stiffness budget instead of retaining a heavy first feasible iterate");
    assert!(stiffened.sizing.total_mass_kg > sized.total_mass_kg);
    assert_eq!(
        stiffened.assessment.primary_mass_kg,
        2.0 * stiffened.sizing.total_mass_kg
    );
    // Recover the actual structural density from the returned load case.
    // Its integral must be the exact mass charged to the acceptance budget,
    // including swept cap/web length rather than projected length only.
    let level = stiffened
        .response
        .load_cases
        .iter()
        .find(|case| case.name == "level");
    let level = level.unwrap_or(&stiffened.response.load_cases[2]);
    let aero = loads::elliptic_distributed_load(
        &stations,
        wsg.semi_span,
        req.mtow_kg * req.gravity_m_s2 * level.load_factor / 2.0,
    );
    let recovered: Vec<_> = (0..stations.len())
        .map(|j| (aero[j] - level.q_net[j]) / (level.load_factor * req.gravity_m_s2) - fuel[j])
        .collect();
    let recovered_mass: f64 = recovered
        .windows(2)
        .zip(stations.windows(2))
        .map(|(mass, y)| 0.5 * (mass[0] + mass[1]) * (y[1] - y[0]))
        .sum();
    assert!(
        (recovered_mass - stiffened.sizing.total_mass_kg).abs() < 1.0e-7,
        "mass budget and inertial relief differ: {recovered_mass} vs {}",
        stiffened.sizing.total_mass_kg
    );
    assert!(stiffened.iterations > 1 && stiffened.iterations <= 32);
    for (old_spar, new_spar) in sized.spars.iter().zip(&stiffened.sizing.spars) {
        for j in 0..stations.len() {
            assert!(new_spar.a_cap[j] >= old_spar.a_cap[j]);
            assert!(new_spar.t_cap[j] <= new_spar.h[j] / 3.0);
            assert!(new_spar.w_cap[j] <= 0.5 * sized.chord[j]);
        }
    }
    let impossible = sizing::size_for_linear_model(
        &wsg,
        sized,
        &cfg,
        &req,
        &geometry.engine,
        &MassModelConfig::default(),
        material,
        material,
        material,
        &fuel,
        &[],
        LinearModelLimits {
            max_curvature_relative_error: 1.0e-12,
        },
    );
    assert!(!impossible.converged);
    assert!(!impossible.assessment.passes());
    assert!(impossible.iterations <= 32);
}
