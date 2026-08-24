// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in product-path evidence against an installed MSES distribution.

use std::fs;
use std::path::PathBuf;

use alas_aero::mses::MsesStatus;
use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, PipelineOptions};
use serde_json::json;

#[test]
#[ignore = "requires ALAS_MSES_DIR to name the installed mset/mses/mplot directory"]
fn installed_mses_keeps_the_requested_local_section_condition_visible() {
    let directory = std::env::var_os("ALAS_MSES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set ALAS_MSES_DIR to the installed MSES directory"));
    for name in ["mset.exe", "mses.exe", "mplot.exe"] {
        assert!(directory.join(name).is_file(), "missing {name}");
    }
    let output = std::env::var_os("ALAS_MSES_OUTPUT").map(PathBuf::from);

    let mut config = AlasConfig::default();
    config.mission.enabled = false;
    config.structures.enabled = false;
    config.mses.enabled = true;
    let freestream_mach = config.requirements.cruise_mach;
    let inboard_twist_deg = config
        .geometry
        .wing
        .inboard_aerodynamic_station(&alas_config::DesignVector::default())
        .unwrap_or_else(|error| panic!("the default inboard station is valid: {error}"))
        .twist_deg;
    let alpha_sweep_halfwidth_deg = config.mses.alpha_sweep_halfwidth_deg;
    let alpha_sweep_n_points = config.mses.alpha_sweep_n_points as usize;
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    };
    let environment = RunEnvironment {
        mses_dir: Some(directory),
        ..RunEnvironment::default()
    };
    let result = DesignPipeline::new(config)
        .run(&options, &environment)
        .unwrap_or_else(|error| panic!("installed MSES pipeline: {error}"));
    let (expected_mach, expected_alpha_deg) = {
        let report = result
            .optimized_report
            .as_ref()
            .unwrap_or_else(|| panic!("MSES condition uses the analyzed report"));
        (
            freestream_mach * report.design.sweep_deg.to_radians().cos(),
            {
                let body_alpha_deg = report
                    .trimmed_design_point
                    .map_or(report.design_point.alpha_deg, |trim| {
                        trim.geometric_body_alpha_deg
                    });
                let cl = report
                    .trimmed_design_point
                    .map_or(report.design_point.cl, |trim| trim.cl);
                let denominator = std::f64::consts::PI
                    * report.polar_fit.aspect_ratio
                    * report.polar_fit.oswald_e;
                let induced_angle_deg = if denominator.is_finite() && denominator > 1e-9 {
                    (cl / denominator).atan().to_degrees()
                } else {
                    0.0
                };
                body_alpha_deg + inboard_twist_deg - induced_angle_deg
            },
        )
    };
    let polar = result
        .mses_result
        .unwrap_or_else(|| panic!("pipeline did not publish the MSES polar"));
    let pressure = result
        .mses_pressure
        .unwrap_or_else(|| panic!("pipeline did not publish MSES pressure data"));

    assert!(
        matches!(
            polar.status,
            MsesStatus::Ok | MsesStatus::PartialConvergence | MsesStatus::Error
        ),
        "MSES exposes the solver outcome rather than fabricating a result: {:?}",
        polar.error
    );
    assert!((polar.mach - expected_mach).abs() < 1e-12);
    assert_eq!(
        pressure.status,
        MsesStatus::Error,
        "pressure error: {:?}; polar status: {:?}; converged alpha: {:?}; requested alpha: {:?}",
        pressure.error,
        polar.status,
        polar.alpha_deg,
        polar
            .point_diagnostics
            .iter()
            .map(|point| (point.requested_alpha_deg, point.status.as_str()))
            .collect::<Vec<_>>()
    );
    assert!(pressure
        .error
        .as_deref()
        .is_some_and(|error| error.contains("did not converge")));
    assert_eq!(polar.requested_alpha_count, alpha_sweep_n_points);
    assert_eq!(polar.converged_alpha_count, polar.alpha_deg.len());
    assert_eq!(polar.point_diagnostics.len(), polar.requested_alpha_count);
    assert!(
        (polar.point_diagnostics[0].requested_alpha_deg
            - (expected_alpha_deg - alpha_sweep_halfwidth_deg))
            .abs()
            < 1e-12
    );
    assert!(
        (polar
            .point_diagnostics
            .last()
            .unwrap_or_else(|| panic!("MSES retains the requested schedule"))
            .requested_alpha_deg
            - (expected_alpha_deg + alpha_sweep_halfwidth_deg))
            .abs()
            < 1e-12
    );
    assert!(polar
        .point_diagnostics
        .iter()
        .all(|point| !point.solver_output.is_empty()));
    assert!(polar
        .point_diagnostics
        .iter()
        .filter(|point| point.status.as_str() == "not_converged")
        .all(|point| !point.solver_output.contains("Converged on tolerance")));
    assert_eq!(polar.alpha_deg.len(), polar.cl.len());
    assert_eq!(polar.alpha_deg.len(), polar.cd.len());
    assert!(polar.cd.iter().all(|drag| drag.is_finite() && *drag > 0.0));
    assert!((pressure.alpha_deg - expected_alpha_deg).abs() < 1e-12);
    assert!(pressure.x_upper.is_empty());
    assert!(pressure.x_lower.is_empty());
    assert!(pressure.field_x.is_empty());
    assert!(pressure.raw_bl_dump.is_empty());
    assert!(pressure.raw_flowfield_dump.is_empty());

    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("create {}: {error}", parent.display()));
            fs::write(parent.join("bl_dump.txt"), &pressure.raw_bl_dump)
                .unwrap_or_else(|error| panic!("write retained BL dump: {error}"));
            fs::write(parent.join("flowfield.txt"), &pressure.raw_flowfield_dump)
                .unwrap_or_else(|error| panic!("write retained flowfield dump: {error}"));
        }
        let summary = json!({
            "runtime": "MSES 3.12c Windows",
            "status": polar.status.as_str(),
            "airfoil": polar.airfoil_name,
            "mach": polar.mach,
            "reynolds": polar.reynolds,
            "requested_alpha_count": polar.requested_alpha_count,
            "converged_alpha_count": polar.converged_alpha_count,
            "partial_convergence": polar.converged_alpha_count < polar.requested_alpha_count,
            "point_diagnostics": polar.point_diagnostics.iter().map(|point| json!({
                "requested_alpha_deg": point.requested_alpha_deg,
                "status": point.status.as_str(),
                "solver_output": point.solver_output,
            })).collect::<Vec<_>>(),
            "alpha_deg": polar.alpha_deg,
            "cl": polar.cl,
            "cd": polar.cd,
            "cm": polar.cm,
            "pressure_status": pressure.status.as_str(),
            "pressure_error": pressure.error,
            "pressure_alpha_deg": pressure.alpha_deg,
            "upper_surface_points": pressure.x_upper.len(),
            "lower_surface_points": pressure.x_lower.len(),
            "field_points": pressure.field_x.len(),
            "airfoil_points": pressure.airfoil_x.len(),
        });
        let text = serde_json::to_string_pretty(&summary)
            .unwrap_or_else(|error| panic!("encode MSES summary: {error}"));
        fs::write(&path, text).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    }
}
