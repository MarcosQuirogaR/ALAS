// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reusable two-dimensional OpenFOAM airfoil studies.
//!
//! This crate owns the study contract instead of letting the GUI assemble
//! dictionaries from unrelated text fields.  A case contains the exact
//! database coordinates, SI operating point, versioned domain/mesh template,
//! solver controls, captured logs and a machine-readable provenance record.
//! The initial template is an incompressible steady RANS `simpleFoam` case
//! with a one-cell extrusion and explicit `empty` front/back constraints. It
//! is intended for attached or mildly separated section flow in its documented
//! Reynolds/Mach range; a completed process is never treated as a converged
//! physical answer without residual, force and conservation checks.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use alas_exec::openfoam::{OpenFoamAdapter, OpenFoamProcessStatus};
use alas_geom::airfoil_library::AirfoilLibrary;
use serde::{Deserialize, Serialize};

pub mod mesh;
pub mod surface;

/// Version of the generated topology and dictionary contract.
pub const TEMPLATE_VERSION: &str = "alas-airfoil-2d-openfoam-gmsh-v2";
/// Nominal span of the thin 2-D extrusion, relative to chord.
pub const EXTRUSION_SPAN_TO_CHORD: f64 = 0.01;

mod boundary;
mod case;
mod config;
mod convergence;
mod geometry;
mod parser;
mod result_io;
mod result_types;
mod results;
mod runner;
mod stage;
mod turbulence;

pub use boundary::*;
pub(crate) use case::fv_schemes;
pub use case::{generate_case, GeneratedCase};
pub use config::*;
pub use convergence::{classify_convergence, parse_mesh_quality};
pub use geometry::{resolve_airfoil, validate_coordinates, AirfoilSnapshot};
pub(crate) use parser::extract_numeric_values;
pub use parser::{
    apply_force_decomposition, parse_force_coefficients, parse_force_decomposition,
    parse_mass_balance, parse_residuals,
};
pub use result_types::*;
pub(crate) use results::{
    build_results_with_quality, persist_failed_results, persist_failed_results_with_quality,
    write_result_artifacts,
};
pub use runner::run_study;
pub(crate) use stage::{
    emit_event, execute_gmsh_stage, execute_solver_postprocess_stage, execute_solver_stage,
    execute_stage, final_write_interval, rewrite_solver_control_dict,
};
pub use turbulence::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_case_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("alas-cfd-{label}-{stamp}"))
    }

    #[test]
    fn effective_speed_and_reynolds_are_linked_by_si_properties() {
        let mut config = CfdStudyConfig::default();
        config.operating_input = OperatingInput::Speed;
        config.speed_m_s = 51.0;
        let expected_re =
            config.density_kg_m3 * 51.0 * config.chord_m / config.dynamic_viscosity_pa_s;
        assert!((config.effective_reynolds() - expected_re).abs() < 1.0e-6);
        config.operating_input = OperatingInput::Reynolds;
        config.reynolds = expected_re;
        assert!((config.effective_speed_m_s() - 51.0).abs() < 1.0e-12);
    }

    #[test]
    fn default_turbulence_state_matches_external_flow_ratio() {
        let config = CfdStudyConfig::default();
        let state = config.effective_turbulence();
        assert_eq!(state.specification, TurbulenceSpecification::ViscosityRatio);
        assert!((state.intensity_fraction - 0.00052).abs() < 1.0e-12);
        assert!((state.nu_t_over_nu - 0.009).abs() < 1.0e-12);
        let nu = config.dynamic_viscosity_pa_s / config.density_kg_m3;
        assert!((state.omega_s_inv - state.k_m2_s2 / (0.009 * nu)).abs() < 1.0e-10);
    }

    #[test]
    fn length_scale_turbulence_mode_remains_explicit_and_reproducible() {
        let mut config = CfdStudyConfig::default();
        config.turbulence_specification = TurbulenceSpecification::LengthScale;
        config.turbulence_intensity = 0.01;
        config.turbulence_length_m = 0.07;
        let state = config.effective_turbulence();
        let expected = state.k_m2_s2.sqrt() / (0.09_f64.powf(0.25) * config.turbulence_length_m);
        assert!((state.omega_s_inv - expected).abs() < 1.0e-12);
        assert!(state.nu_t_over_nu > 100.0);
    }

    #[test]
    fn final_convection_scheme_cannot_be_hidden_by_full_length_startup() {
        let mut config = CfdStudyConfig::default();
        config.solver.max_iterations = 100;
        config.solver.startup_iterations = 100;
        let errors = config
            .validate()
            .expect_err("startup must leave final iterations");
        assert!(errors
            .iter()
            .any(|error| error.contains("final-stage iteration")));
    }

    #[test]
    fn terminal_write_interval_reaches_the_requested_one_stage_end_time() {
        let path = test_case_dir("terminal-write");
        let config = CfdStudyConfig::default();
        generate_case(&config, &path).expect("case generation should succeed");
        rewrite_solver_control_dict(&path, 2_050, false, Some(final_write_interval(2_050, 100)))
            .expect("control dictionary should be rewritten");
        let control = fs::read_to_string(path.join("system/controlDict")).expect("control dict");
        assert!(control.contains("endTime 2050;"));
        assert!(control.contains("writeInterval 2050;"));
        assert_eq!(control.matches("writeInterval 1;").count(), 2);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn repeated_closing_coordinate_is_provenance_only_for_topology() {
        let coordinates = vec![
            (1.0, 0.0),
            (0.75, 0.12),
            (0.25, 0.12),
            (0.0, 0.0),
            (0.25, -0.12),
            (0.75, -0.12),
            (0.9, -0.06),
            (1.0, 0.0),
        ];
        assert!(validate_coordinates(&coordinates).is_ok());
    }

    #[test]
    fn case_generation_preserves_airfoil_identity_and_named_patches() {
        let path = test_case_dir("generation");
        let config = CfdStudyConfig::default();
        let result = generate_case(&config, &path).expect("case generation should succeed");
        assert_eq!(result.airfoil.name, config.airfoil_name);
        assert!(result.airfoil.coordinate_hash.starts_with("fnv1a64-"));
        let geo = fs::read_to_string(path.join("system/airfoil.geo")).unwrap_or_default();
        assert!(geo.contains("Physical Surface(\"inlet\")"));
        assert!(geo.contains("Physical Surface(\"frontAndBack\")"));
        assert!(geo.contains("Physical Volume(\"fluid\")"));
        assert!(path.join("system/mesh-report.json").is_file());
        let study = fs::read_to_string(path.join("study.json")).unwrap_or_default();
        assert!(study.contains(&result.airfoil.coordinate_hash));
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn case_generation_rejects_nonempty_directory_without_touching_it() {
        let path = test_case_dir("nonempty");
        fs::create_dir_all(&path).expect("test directory");
        let sentinel = path.join("sentinel.txt");
        fs::write(&sentinel, "retain").expect("sentinel");
        let error = generate_case(&CfdStudyConfig::default(), &path)
            .expect_err("stale case directory must be rejected");
        assert!(error.contains("not empty"));
        assert_eq!(fs::read_to_string(sentinel).ok().as_deref(), Some("retain"));
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn runtime_provenance_captures_backend_and_executable_hashes() {
        let path = test_case_dir("provenance");
        let config = CfdStudyConfig::default();
        let generated = generate_case(&config, &path).expect("case generation should succeed");
        let executable = std::env::current_exe().expect("test executable path");
        let mut logs = BTreeMap::new();
        logs.insert("__backend".to_owned(), "native".to_owned());
        logs.insert("__openfoam_version".to_owned(), "OpenFOAM-v2606".to_owned());
        logs.insert(
            "__executable_path:simpleFoam".to_owned(),
            executable.to_string_lossy().into_owned(),
        );
        logs.insert("simpleFoam".to_owned(), "solver output".to_owned());
        logs.insert("__failure".to_owned(), "test failure".to_owned());
        let results = build_results_with_quality(
            &config,
            &generated,
            logs,
            MeshQuality::default(),
            OpenFoamProcessStatus::LaunchFailed,
        );
        assert_eq!(results.provenance.backend.as_deref(), Some("native"));
        assert_eq!(
            results.provenance.openfoam_version.as_deref(),
            Some("OpenFOAM-v2606")
        );
        assert!(results
            .provenance
            .file_hashes
            .contains_key("constant/transportProperties"));
        assert!(results
            .provenance
            .file_hashes
            .contains_key("executable:simpleFoam"));
        assert!(!results.command_logs.contains_key("__backend"));
        assert!(!results
            .command_logs
            .contains_key("__executable_path:simpleFoam"));
        assert!(results.command_logs.contains_key("__failure"));
        write_result_artifacts(&results).expect("result artifacts should be writable");
        let study = fs::read_to_string(path.join("study.json")).expect("final study metadata");
        assert!(study.contains("\"backend\": \"native\""));
        assert!(study.contains("\"openfoam_version\": \"OpenFOAM-v2606\""));
        assert!(path.join("report.md").is_file());
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn quality_gate_rejects_zero_exit_failed_checks() {
        let output = "Mesh OK.\nFailed 1 mesh checks.\n";
        assert!(!parse_mesh_quality(output).passed);
        assert!(parse_mesh_quality("Mesh OK.\n").passed);
    }

    #[test]
    fn quality_parser_keeps_checkmesh_metrics_when_labels_are_reordered() {
        let output = "    cells: 68083\n    Mesh non-orthogonality Max: 64.2597893176 average: 2.7\n    Max skewness = 5.27205198547\n    Min volume = 2.00101475713e-10.\n    Failed 1 mesh checks.\n";
        let quality = parse_mesh_quality(output);
        assert_eq!(quality.cells, Some(68_083));
        assert_eq!(quality.max_non_orthogonality_deg, Some(64.2597893176));
        assert_eq!(quality.max_skewness, Some(5.27205198547));
        assert_eq!(quality.min_volume_m3, Some(2.00101475713e-10));
        assert!(!quality.passed);
    }

    #[test]
    fn force_parser_handles_openfoam_ten_column_header_layout() {
        let text = "# Time Cd Cd(f) Cd(r) Cl Cl(f) Cl(r) CmPitch CmRoll CmYaw\n1 0.04 0.01 0.03 0.5 0.2 0.3 -0.02 0 0\n";
        let rows = parse_force_coefficients(text);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cd, 0.04);
        assert_eq!(rows[0].cl, 0.5);
        assert_eq!(rows[0].cm, -0.02);
        assert_eq!(rows[0].cd_pressure, None);
        assert_eq!(rows[0].cd_viscous, None);
        assert_eq!(rows[0].cl_pressure, None);
        assert_eq!(rows[0].cl_viscous, None);
    }

    #[test]
    fn force_parser_accepts_explicit_pressure_viscous_columns() {
        let text = "# Time Cd CdPressure CdViscous Cl ClPressure ClViscous Cm\n1 0.04 0.03 0.01 0.5 0.4 0.1 -0.02\n";
        let rows = parse_force_coefficients(text);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cd_pressure, Some(0.03));
        assert_eq!(rows[0].cd_viscous, Some(0.01));
        assert_eq!(rows[0].cl_pressure, Some(0.4));
        assert_eq!(rows[0].cl_viscous, Some(0.1));
    }

    #[test]
    fn force_parser_keeps_ambiguous_headerless_ten_columns_unattributed() {
        let rows = parse_force_coefficients("1 0.04 0.01 0.03 0.5 0.2 0.3 -0.02 0 0\n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cd, 0.04);
        assert_eq!(rows[0].cl, 0.01);
        assert_eq!(rows[0].cm, 0.03);
        assert_eq!(rows[0].cd_pressure, None);
    }

    #[test]
    fn force_decomposition_projects_dimensional_components_without_f_r_aliasing() {
        let decomposition = parse_force_decomposition("# Time total_x total_y total_z pressure_x pressure_y pressure_z viscous_x viscous_y viscous_z\n1 3 3 0 2 3 0 1 0 0\n");
        assert_eq!(decomposition.len(), 1);
        let mut coefficients = vec![ForceSample {
            time: 1.0,
            cd: 0.0,
            cl: 0.0,
            cm: 0.0,
            cd_pressure: None,
            cd_viscous: None,
            cl_pressure: None,
            cl_viscous: None,
        }];
        apply_force_decomposition(
            &mut coefficients,
            &decomposition,
            1.0,
            2.0,
            1.0,
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        );
        assert_eq!(coefficients[0].cd_pressure, Some(1.0));
        assert_eq!(coefficients[0].cl_pressure, Some(1.5));
        assert_eq!(coefficients[0].cd_viscous, Some(0.5));
        assert_eq!(coefficients[0].cl_viscous, Some(0.0));
    }

    #[test]
    fn residual_parser_groups_pressure_corrections_by_outer_time() {
        let rows = parse_residuals(
            "Time = 41\nSolving for Ux, Initial residual = 2e-4, Final residual = 2e-6\nSolving for p, Initial residual = 9e-4, Final residual = 3e-6\nSolving for p, Initial residual = 1e-5, Final residual = 4e-7\nSolving for Uy, Initial residual = 3e-4, Final residual = 2e-6\nTime = 42\nSolving for Ux, Initial residual = 2e-6, Final residual = 2e-8\n",
        );
        assert_eq!(rows.len(), 5);
        assert!(rows[..4].iter().all(|row| row.iteration == 41));
        assert_eq!(rows[4].iteration, 42);
    }

    #[test]
    fn continuity_parser_keeps_the_outer_time_for_each_diagnostic() {
        let rows = parse_mass_balance(
            "Time = 7\ntime step continuity errors : sum local = 2e-4, global = -3e-5, cumulative = 4e-4\nTime = 8\ntime step continuity errors : sum local = 2e-5, global = -3e-6, cumulative = 4e-4\n",
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].time, Some(7.0));
        assert_eq!(rows[1].time, Some(8.0));
    }

    #[test]
    fn convergence_requires_residual_force_and_continuity_evidence() {
        let config = CfdStudyConfig {
            solver: SolverSettings {
                force_window: 3,
                ..SolverSettings::default()
            },
            ..CfdStudyConfig::default()
        };
        let residuals = ["p", "Ux", "Uy", "k", "omega"]
            .into_iter()
            .map(|field| ResidualSample {
                iteration: 702,
                field: field.to_owned(),
                initial: 1.0e-6,
                final_residual: 1.0e-6,
            })
            .collect::<Vec<_>>();
        let forces = vec![
            ForceSample {
                time: 1.0,
                cd: 0.04,
                cl: 0.5,
                cm: -0.02,
                cd_pressure: None,
                cd_viscous: None,
                cl_pressure: None,
                cl_viscous: None,
            },
            ForceSample {
                time: 2.0,
                cd: 0.0401,
                cl: 0.5001,
                cm: -0.0201,
                cd_pressure: None,
                cd_viscous: None,
                cl_pressure: None,
                cl_viscous: None,
            },
            ForceSample {
                time: 3.0,
                cd: 0.0399,
                cl: 0.4999,
                cm: -0.0199,
                cd_pressure: None,
                cd_viscous: None,
                cl_pressure: None,
                cl_viscous: None,
            },
        ];
        let mass = vec![MassBalanceSample {
            time: Some(3.0),
            sum_local: Some(1.0e-7),
            global: Some(1.0e-8),
            cumulative: Some(1.0e-7),
        }];
        let quality = MeshQuality {
            passed: true,
            ..MeshQuality::default()
        };
        let (outcome, _) = classify_convergence(
            &config,
            OpenFoamProcessStatus::Completed,
            &quality,
            &residuals,
            &forces,
            &mass,
        );
        assert_eq!(outcome, CfdOutcome::NumericallyConverged);
    }

    #[test]
    fn convergence_does_not_gate_cumulative_continuity_with_per_iteration_limit() {
        let config = CfdStudyConfig {
            solver: SolverSettings {
                force_window: 3,
                ..SolverSettings::default()
            },
            ..CfdStudyConfig::default()
        };
        let residuals = ["p", "Ux", "Uy", "k", "omega"]
            .into_iter()
            .map(|field| ResidualSample {
                iteration: 9,
                field: field.to_owned(),
                initial: 1.0e-7,
                final_residual: 1.0e-8,
            })
            .collect::<Vec<_>>();
        let forces = (0..3)
            .map(|index| ForceSample {
                time: index as f64,
                cd: 0.04,
                cl: 0.5,
                cm: -0.02,
                cd_pressure: None,
                cd_viscous: None,
                cl_pressure: None,
                cl_viscous: None,
            })
            .collect::<Vec<_>>();
        let mass = vec![MassBalanceSample {
            time: Some(9.0),
            sum_local: Some(1.0e-7),
            global: Some(1.0e-8),
            cumulative: Some(1.0),
        }];
        let (outcome, _) = classify_convergence(
            &config,
            OpenFoamProcessStatus::Completed,
            &MeshQuality {
                passed: true,
                ..MeshQuality::default()
            },
            &residuals,
            &forces,
            &mass,
        );
        assert_eq!(outcome, CfdOutcome::NumericallyConverged);
    }

    #[test]
    fn convergence_rejects_nonfinite_equation_history_before_certification() {
        let config = CfdStudyConfig::default();
        let residuals = vec![ResidualSample {
            iteration: 1,
            field: "p".to_owned(),
            initial: f64::NAN,
            final_residual: 1.0e-8,
        }];
        let (outcome, detail) = classify_convergence(
            &config,
            OpenFoamProcessStatus::Completed,
            &MeshQuality {
                passed: true,
                ..MeshQuality::default()
            },
            &residuals,
            &[],
            &[],
        );
        assert_eq!(outcome, CfdOutcome::Unconverged);
        assert!(detail.contains("non-finite"));
    }

    #[test]
    fn convergence_rejects_nonfinite_force_history_before_certification() {
        let config = CfdStudyConfig {
            solver: SolverSettings {
                force_window: 3,
                ..SolverSettings::default()
            },
            ..CfdStudyConfig::default()
        };
        let residuals = ["p", "Ux", "Uy", "k", "omega"]
            .into_iter()
            .map(|field| ResidualSample {
                iteration: 12,
                field: field.to_owned(),
                initial: 1.0e-7,
                final_residual: 1.0e-8,
            })
            .collect::<Vec<_>>();
        let forces = (0..3)
            .map(|index| ForceSample {
                time: index as f64,
                cd: if index == 2 { f64::NAN } else { 0.04 },
                cl: 0.5,
                cm: -0.02,
                cd_pressure: None,
                cd_viscous: None,
                cl_pressure: None,
                cl_viscous: None,
            })
            .collect::<Vec<_>>();
        let mass = vec![MassBalanceSample {
            time: Some(12.0),
            sum_local: Some(1.0e-7),
            global: Some(1.0e-8),
            cumulative: Some(1.0e-7),
        }];
        let (outcome, detail) = classify_convergence(
            &config,
            OpenFoamProcessStatus::Completed,
            &MeshQuality {
                passed: true,
                ..MeshQuality::default()
            },
            &residuals,
            &forces,
            &mass,
        );
        assert_eq!(outcome, CfdOutcome::Unconverged);
        assert!(detail.contains("Force history"));
    }

    #[test]
    fn convergence_does_not_reuse_an_earlier_complete_iteration() {
        let config = CfdStudyConfig::default();
        let mut residuals = ["p", "Ux", "Uy", "k", "omega"]
            .into_iter()
            .map(|field| ResidualSample {
                iteration: 11,
                field: field.to_owned(),
                initial: 1.0e-7,
                final_residual: 1.0e-8,
            })
            .collect::<Vec<_>>();
        residuals.push(ResidualSample {
            iteration: 12,
            field: "p".to_owned(),
            initial: 1.0e-7,
            final_residual: 1.0e-8,
        });
        let (outcome, detail) = classify_convergence(
            &config,
            OpenFoamProcessStatus::Completed,
            &MeshQuality {
                passed: true,
                ..MeshQuality::default()
            },
            &residuals,
            &[],
            &[],
        );
        assert_eq!(outcome, CfdOutcome::Unconverged);
        assert!(detail.contains("final log iteration"));
        assert!(detail.contains("ux"));
    }
}
