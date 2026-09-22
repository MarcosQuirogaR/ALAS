// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reusable two-dimensional OpenFOAM airfoil studies.
//!
//! This crate owns the study contract instead of letting the GUI assemble
//! dictionaries from unrelated text fields.  A case contains the exact
//! database coordinates, SI operating point, versioned domain/mesh template,
//! solver controls, captured logs and a machine-readable provenance record.
//! Low-Mach studies use an incompressible steady RANS `simpleFoam` case; the
//! Mach-derived compressible path uses perfect-gas steady RANS `rhoSimpleFoam`
//! with shock-safe bounded schemes. Both cases use a one-cell extrusion and
//! explicit `empty` front/back constraints. They are intended for attached or
//! mildly separated section flow in the documented Reynolds/Mach range; a
//! completed process is never treated as a converged physical answer without
//! residual, force and conservation checks.

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

/// Version of the generated topology and dictionary contract.  `v5` makes the
/// pressure-equation non-orthogonal treatment, the inner pressure relative
/// tolerance, the gradient limiter, the turbulence convection scheme and the
/// momentum linear solver explicit configuration instead of template literals;
/// a `v4` case is the `v5` default settings and solves the same equations, so
/// the difference is a control surface and a provenance label, not physics.
/// `v4` pairs the
/// SIMPLEC (`consistent yes`) loop with SIMPLEC relaxation factors instead of
/// the SIMPLE pair it previously emitted, and sizes the boundary-layer stack
/// from the estimated turbulent boundary-layer thickness; a `v3` case reaches
/// the same fixed point but takes markedly more outer iterations and leaves
/// the outer boundary layer on isotropic cells.  `v3` fixed the `forceCoeffs`
/// pitch axis to `(0 0 -1)` so the reported `Cm` is positive nose-up, matching
/// the recorded frame convention and the native surface integration; `v2`
/// cases report `Cm` with the opposite sign.
pub const TEMPLATE_VERSION: &str = "alas-airfoil-2d-openfoam-gmsh-v5";
/// Nominal span of the thin 2-D extrusion, relative to chord.
pub const EXTRUSION_SPAN_TO_CHORD: f64 = 0.01;

mod boundary;
mod case;
mod config;
mod conventions;
mod convergence;
mod field_update;
mod geometry;
mod mesh_gate;
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
pub use conventions::*;
pub use convergence::{
    assess_physical_plausibility, classify_convergence, parse_mesh_quality, PhysicalPlausibility,
    PlausibilityBounds, PlausibilityVerdict, PlausibilityViolation, LINEAR_SOLVER_RESIDUAL_FLOOR,
};
pub use field_update::{
    read_field_update_evidence, read_field_update_evidence_for_config, FieldUpdateEvidence,
    FieldUpdateSample, COMPRESSIBLE_FIELD_UPDATE_FIELDS, FIELD_UPDATE_FIELDS,
};
pub use geometry::{resolve_airfoil, validate_coordinates, AirfoilSnapshot};
pub use mesh_gate::{
    qualify_mesh, qualify_mesh_with_boundary, MeshCheckStatus, MeshQualification,
    MeshQualificationStanding, MeshQualityCheck,
};
pub(crate) use parser::extract_numeric_values;
pub use parser::{
    apply_force_decomposition, parse_force_coefficients, parse_force_decomposition,
    parse_mass_balance, parse_residuals,
};
pub use result_io::read_boundary_skewness_max;
pub use result_types::*;
pub use results::write_result_artifacts;
pub(crate) use results::{
    build_results_with_quality, persist_failed_results, persist_failed_results_with_quality,
};
pub use runner::run_study;
pub(crate) use stage::{
    emit_event, execute_gmsh_stage, execute_solver_postprocess_stage_with_tool,
    execute_solver_stage_with_tool, execute_stage, final_write_interval,
    rewrite_solver_control_dict, StageContext,
};
pub use turbulence::*;

#[cfg(test)]
// Tests assert on values they parsed or built here, so a failed expect (or
// expect_err, for the deliberately-invalid-config cases) is the assertion
// failing rather than a library invariant breaking.  Default-then-override
// is the normal way a test builds a config that changes only the one or two
// fields under test.
#[allow(clippy::expect_used, clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Field-update evidence for a case whose solved fields are all still
    /// moving, the ordinary situation, and the one under which the
    /// skipped-equation guard has nothing to act on.
    fn live_fields() -> FieldUpdateEvidence {
        field_evidence(
            82_975,
            &[
                ("p", 82_975),
                ("U", 82_970),
                ("k", 82_871),
                ("omega", 82_894),
            ],
        )
    }

    /// Evidence with an explicit changed-cell count per field.
    fn field_evidence(cells: usize, fields: &[(&str, usize)]) -> FieldUpdateEvidence {
        FieldUpdateEvidence {
            samples: fields
                .iter()
                .map(|(field, changed)| FieldUpdateSample {
                    field: (*field).to_owned(),
                    from_time: "800".to_owned(),
                    to_time: "829".to_owned(),
                    cells,
                    changed_cells: *changed,
                    max_relative_change: if *changed > 0 { 1.42e-3 } else { 0.0 },
                    write_precision: Some(12),
                })
                .collect(),
            unavailable_reason: None,
            write_precision: Some(12),
            write_interval: Some(100.0),
            write_pair_regular: Some(true),
        }
    }

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
    fn mach_diagnostic_uses_the_declared_static_temperature() {
        let mut config = CfdStudyConfig::default();
        config.speed_m_s = 51.0;
        config.freestream_temperature_k = 288.15;
        let expected_sound_speed = (1.4 * 287.052_87 * 288.15_f64).sqrt();
        assert!((config.speed_of_sound_m_s() - expected_sound_speed).abs() < 1.0e-12);
        assert!((config.mach_number() - 51.0 / expected_sound_speed).abs() < 1.0e-12);
        config.freestream_temperature_k = 0.0;
        let errors = config
            .validate()
            .expect_err("zero static temperature is invalid");
        assert!(errors
            .iter()
            .any(|error| error.contains("freestream temperature")));
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
        assert_eq!(
            control
                .matches("executeControl writeTime;\n        writeControl writeTime;")
                .count(),
            2,
            "solver-attached yPlus and wall-shear fields must only write at controlDict write times"
        );
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

    /// A maximum non-orthogonality past `checkMesh`'s 70 deg warning line is
    /// not readable without the face count, and `checkMesh` passes the mesh
    /// either way.  Verbatim from `G3-fine-p404/logs/checkMesh.log`, the
    /// certified fine case: one face out of 436 389 cells, average 5.35 deg.
    #[test]
    fn severely_non_orthogonal_faces_are_recorded_beside_the_maximum_angle() {
        let output = concat!(
            "    Mesh non-orthogonality Max: 71.3691249255 average: 5.34651937631\n",
            "   *Number of severely non-orthogonal (> 70 degrees) faces: 1.\n",
            "    Non-orthogonality check OK.\n",
            "  <<Writing 1 non-orthogonal faces to set nonOrthoFaces\n",
            "    Max skewness = 1.8138019812 OK.\n",
            "\nMesh OK.\n",
        );
        let quality = parse_mesh_quality(output);
        assert_eq!(quality.max_non_orthogonality_deg, Some(71.3691249255));
        assert_eq!(quality.severely_non_orthogonal_faces, Some(1));
        // `checkMesh` reports this as a warning and passes the mesh; the gate
        // follows `checkMesh` rather than inventing its own rejection rule.
        assert!(quality.passed);
        // The common case has no such line at all, and that must read as
        // "not reported" rather than as zero faces.
        assert_eq!(
            parse_mesh_quality(
                "    Mesh non-orthogonality Max: 46.1851445579 average: 5.4\nMesh OK.\n"
            )
            .severely_non_orthogonal_faces,
            None
        );
    }

    /// The inner pressure tolerance is per preset because it was measured per
    /// preset, and an explicit setting must always win over that.
    #[test]
    fn the_inner_pressure_tolerance_is_preset_aware_and_never_overrides_the_user() {
        let mut config = CfdStudyConfig::default();
        assert_eq!(config.solver.pressure_relative_tolerance, None);
        for (preset, expected) in [
            (MeshPreset::Coarse, 0.05),
            (MeshPreset::Medium, 0.05),
            // The fine preset cannot reach `residualControl` at 0.05; it ran
            // its whole 6000-iteration budget in `G3-fine-p404` and stopped at
            // 1781 in `P1-fine-preltol001`.
            (MeshPreset::Fine, 0.01),
        ] {
            config.mesh.preset = preset;
            assert_eq!(
                config.solver.effective_pressure_relative_tolerance(preset),
                expected,
                "{preset:?}"
            );
            let emitted = fv_solution_for_test(&config);
            assert!(
                emitted.contains(&format!("relTol {expected};")),
                "{preset:?}: {emitted}"
            );
        }
        // Provenance must record what was emitted, not the unset `None`, or a
        // study replayed on another preset would quietly become a different
        // case.
        config.mesh.preset = MeshPreset::Fine;
        let path = test_case_dir("preltol-provenance");
        generate_case(&config, &path).unwrap_or_else(|error| panic!("{error}"));
        let study =
            fs::read_to_string(path.join("study.json")).unwrap_or_else(|error| panic!("{error}"));
        let _ = fs::remove_dir_all(&path);
        let recorded: StudyProvenance =
            serde_json::from_str(&study).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            recorded.config.solver.pressure_relative_tolerance,
            Some(0.01),
            "the fine preset resolves to 0.01 and must say so"
        );

        config.solver.pressure_relative_tolerance = Some(0.02);
        for preset in [MeshPreset::Coarse, MeshPreset::Medium, MeshPreset::Fine] {
            assert_eq!(
                config.solver.effective_pressure_relative_tolerance(preset),
                0.02,
                "an explicit setting must win at every preset"
            );
        }
    }

    /// The inner linear-solver tolerance the case emits and the one the
    /// skipped-equation guard reasons about must be the same number.  They were
    /// a literal in five places and a constant in one, and could drift apart
    /// silently; the guard would then have been comparing residuals against a
    /// tolerance the case no longer used.
    #[test]
    fn the_emitted_inner_tolerance_is_the_one_the_convergence_gate_uses() {
        let config = CfdStudyConfig::default();
        let emitted = fv_solution_for_test(&config);
        let expected = format!("tolerance {LINEAR_SOLVER_RESIDUAL_FLOOR:.3e};");
        assert_eq!(
            emitted.matches(expected.as_str()).count(),
            5,
            "Phi, p, U, k and omega all solve to the gate's floor: {emitted}"
        );
    }

    /// `fvSolution` as the shipped emitter writes it, without a mesher run.
    fn fv_solution_for_test(config: &CfdStudyConfig) -> String {
        let path = test_case_dir("fvsolution");
        generate_case(config, &path).unwrap_or_else(|error| panic!("{error}"));
        let text = fs::read_to_string(path.join("system/fvSolution"))
            .unwrap_or_else(|error| panic!("{error}"));
        let _ = fs::remove_dir_all(&path);
        text
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
    fn residual_parser_does_not_treat_execution_time_as_outer_iteration() {
        let rows = parse_residuals(
            "Time = 7\nSolving for Ux, Initial residual = 2e-4, Final residual = 2e-6\nExecutionTime = 8 s  ClockTime = 8 s\nSolving for p, Initial residual = 9e-4, Final residual = 3e-6\n",
        );
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.iteration == 7));
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
            Some(&live_fields()),
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
            Some(&live_fields()),
        );
        assert_eq!(outcome, CfdOutcome::NumericallyConverged);
    }

    fn residuals_push(
        out: &mut Vec<ResidualSample>,
        iteration: u64,
        field: &str,
        initial: f64,
        final_residual: f64,
    ) {
        out.push(ResidualSample {
            iteration,
            field: field.to_owned(),
            initial,
            final_residual,
        });
    }

    fn force_sample(cl: f64, cd: f64, cm: f64) -> ForceSample {
        ForceSample {
            time: 400.0,
            cd,
            cl,
            cm,
            cd_pressure: None,
            cd_viscous: None,
            cl_pressure: None,
            cl_viscous: None,
        }
    }

    #[test]
    fn a_finite_but_impossible_coefficient_is_refused_as_a_failed_result() {
        // Exactly the measured tail of an internal relaxation probe (2026-09-16),
        // case `P3-relax-07-08`: the process exited cleanly, checkMesh passed,
        // and the solver reported finite numbers no section can produce.
        // Before this screen it was labelled only
        // `unconverged`, which reads as "nearly there".
        let config = CfdStudyConfig::default();
        let mesh = MeshQuality {
            passed: true,
            near_wall: Some(NearWallDiagnostics {
                time: 400.0,
                patch_name: "airfoil".to_owned(),
                sample_count: Some(277),
                min_y_plus: 1.07,
                max_y_plus: 101.11,
                average_y_plus: 31.87,
                target_y_plus: 1.0,
                selected_wall_distance_m: 4.456_872_624_298_733e-6,
                estimated_y_plus: Some(1.0),
                source: "postProcessing/yPlus/400/yPlus.dat".to_owned(),
            }),
            ..MeshQuality::default()
        };
        let (outcome, detail) = classify_convergence(
            &config,
            OpenFoamProcessStatus::Completed,
            &mesh,
            &[ResidualSample {
                iteration: 400,
                field: "p".to_owned(),
                initial: 3.22e-2,
                final_residual: 1.0e-4,
            }],
            &[force_sample(-302.74, -230.15, 327.45)],
            &[],
            Some(&live_fields()),
        );
        assert_eq!(outcome, CfdOutcome::Failed);
        assert!(detail.contains("Cl -3.0274e2"), "{detail}");
        assert!(detail.contains("Cd -2.3015e2"), "{detail}");
        assert!(detail.contains("Cm 3.2745e2"), "{detail}");
        assert!(detail.contains("broken solution"), "{detail}");

        // Negative control: the same broken magnitudes must not be reachable
        // through a plausible case, and a genuine high-lift section at the top
        // of the envelope stays a normal unconverged result.
        let plausible = assess_physical_plausibility(
            &[force_sample(4.6, 0.09, -0.42)],
            &MeshQuality {
                passed: true,
                ..MeshQuality::default()
            },
        );
        assert_eq!(plausible.verdict, PlausibilityVerdict::Plausible);
        assert!(plausible.violations.is_empty());

        // Negative control: thrust from a steady section is refused even when
        // every other quantity is ordinary.
        let thrusting = assess_physical_plausibility(
            &[force_sample(0.49, -1.0e-4, -0.005)],
            &MeshQuality::default(),
        );
        assert_eq!(thrusting.verdict, PlausibilityVerdict::Implausible);
        assert_eq!(thrusting.violations.len(), 1);
        assert_eq!(thrusting.violations[0].quantity, "Cd");

        // Negative control: with no force sample the screen states that it did
        // not run rather than passing the result silently.
        let empty = assess_physical_plausibility(&[], &MeshQuality::default());
        assert_eq!(empty.verdict, PlausibilityVerdict::NotEvaluated);
        assert!(empty.detail().contains("No force sample"), "{empty:?}");
    }

    #[test]
    fn the_case_report_states_the_answer_and_the_margin() {
        // A report that says "the criteria are satisfied" without printing a
        // coefficient or a residual cannot be acted on, and until now that is
        // what it said.  Values are `Z1-shipped-defaults-coarse`'s measured tail.
        let path = test_case_dir("report-answer");
        let config = CfdStudyConfig::default();
        let generated = generate_case(&config, &path).expect("case generation should succeed");
        let residuals = [
            ("p", 4.976_997_250_6e-6),
            ("Ux", 4.986_618_688_31e-7),
            ("Uy", 2.960_132_509_34e-7),
            ("k", 4.659_517_833_14e-8),
            ("omega", 8.892_174_494_95e-9),
        ]
        .into_iter()
        .map(|(field, initial)| ResidualSample {
            iteration: 878,
            field: field.to_owned(),
            initial,
            final_residual: initial * 1.0e-2,
        })
        .collect::<Vec<_>>();
        let forces = (0..config.solver.force_window)
            .map(|_| ForceSample {
                time: 878.0,
                cd: 0.010_524_695_044_71,
                cl: 0.463_735_784_590_9,
                cm: -0.003_871_677_402_469,
                cd_pressure: Some(0.004_1),
                cd_viscous: Some(0.006_4),
                cl_pressure: None,
                cl_viscous: None,
            })
            .collect::<Vec<_>>();
        let mut results = build_results_with_quality(
            &config,
            &generated,
            BTreeMap::new(),
            MeshQuality {
                passed: true,
                ..MeshQuality::default()
            },
            OpenFoamProcessStatus::Completed,
        );
        results.residuals = residuals;
        results.forces = forces;
        results.mass_balance = vec![MassBalanceSample {
            time: Some(878.0),
            sum_local: Some(1.186_421_620_88e-8),
            global: Some(7.679_197_545_22e-11),
            cumulative: Some(1.0e-4),
        }];
        let (outcome, detail) = classify_convergence(
            &config,
            OpenFoamProcessStatus::Completed,
            &results.mesh_quality,
            &results.residuals,
            &results.forces,
            &results.mass_balance,
            Some(&live_fields()),
        );
        results.outcome = outcome;
        results.status_detail = detail;
        write_result_artifacts(&results).expect("report should be written");
        let report = fs::read_to_string(path.join("report.md")).expect("report.md");

        // The answer.
        assert!(report.contains("0.4637358"), "{report}");
        assert!(report.contains("0.0105247"), "{report}");
        assert!(report.contains("-0.0038717"), "{report}");
        assert!(report.contains("sampled at outer iteration"), "{report}");
        // The margin, per equation, against the criterion.
        assert!(
            report.contains("| `omega` | `8.8922e-9` | `0.00x` |"),
            "{report}"
        );
        assert!(
            report.contains("| `p` | `4.9770e-6` | `0.50x` |"),
            "{report}"
        );
        // The conservation criterion and its own value.
        assert!(report.contains("1.1864e-8"), "{report}");
        assert!(report.contains("audit data"), "{report}");
        assert!(report.contains("Physical-plausibility screen"), "{report}");
        // A converged case carries no provisional warning.
        assert_eq!(results.outcome, CfdOutcome::NumericallyConverged);
        assert!(!report.contains("PROVISIONAL"), "{report}");
        let _ = fs::remove_dir_all(&path);

        // An unconverged case must carry it, so a coefficient can never be
        // lifted out of a report without the qualification attached.
        let unconverged = test_case_dir("report-answer-unconverged");
        let generated = generate_case(&config, &unconverged).expect("case generation");
        let mut results = build_results_with_quality(
            &config,
            &generated,
            BTreeMap::new(),
            MeshQuality {
                passed: true,
                ..MeshQuality::default()
            },
            OpenFoamProcessStatus::Completed,
        );
        results.forces = vec![force_sample(0.46, 0.0105, -0.0038)];
        results.outcome = CfdOutcome::Unconverged;
        write_result_artifacts(&results).expect("report should be written");
        let report = fs::read_to_string(unconverged.join("report.md")).expect("report.md");
        assert!(report.contains("PROVISIONAL"), "{report}");
        assert!(report.contains("not a qualified answer"), "{report}");
        let _ = fs::remove_dir_all(&unconverged);
    }

    #[test]
    fn the_solver_stage_budget_covers_the_work_actually_requested() {
        // The shipped 1800 s is a fine guard for gmsh and checkMesh and a bad
        // one for the solver.  Cell counts are the measured ones from an
        // internal CFD convergence study (2026-09-16).
        let solver = SolverSettings::default();
        assert_eq!(solver.timeout_seconds, 1_800);
        assert_eq!(solver.max_iterations, 2_000);

        // Before checkMesh there is no cell count, so nothing changes.
        assert_eq!(solver.solver_timeout_seconds(None), 1_800);
        assert_eq!(solver.solver_timeout_seconds(Some(0)), 1_800);

        // Medium, the shipped preset: the configured guard is under 30 % of the
        // budget the requested work needs, so the derived value must win.
        let medium = solver.solver_timeout_seconds(Some(183_071));
        assert_eq!(medium, 10_984);
        assert!(medium > solver.timeout_seconds);

        // Fine at the same iteration budget is where the shipped guard actually
        // kills a healthy run; it needs roughly two and a half times the medium
        // budget and gets it.
        let fine = solver.solver_timeout_seconds(Some(433_840));
        assert_eq!(fine, 26_030);

        // A user who asks for a longer guard keeps it: the configured value is
        // a floor, never a cap.
        let patient = SolverSettings {
            timeout_seconds: 40_000,
            ..SolverSettings::default()
        };
        assert_eq!(patient.solver_timeout_seconds(Some(183_071)), 40_000);

        // And the result never exceeds what the process supervisor will accept.
        let enormous = SolverSettings {
            max_iterations: 100_000,
            ..SolverSettings::default()
        };
        assert_eq!(enormous.solver_timeout_seconds(Some(433_840)), 86_400);
    }

    #[test]
    fn an_equation_that_stopped_being_solved_cannot_satisfy_the_residual_gate() {
        // Every residual series below is the measured tail of a real case in an
        // internal CFD convergence study (2026-09-16), because the whole
        // question is which of two behaviours the artifacts actually show.
        let config = CfdStudyConfig::default();
        let stationary_forces = (0..config.solver.force_window)
            .map(|index| force_sample(0.46 + index as f64 * 1.0e-9, 0.0105, -0.0038))
            .collect::<Vec<_>>();
        let closed_mass = vec![MassBalanceSample {
            time: Some(3_000.0),
            sum_local: Some(1.0e-8),
            global: Some(1.0e-10),
            cumulative: Some(1.0e-4),
        }];
        let good_mesh = MeshQuality {
            passed: true,
            ..MeshQuality::default()
        };
        let judge_with = |residuals: &[ResidualSample], fields: Option<&FieldUpdateEvidence>| {
            classify_convergence(
                &config,
                OpenFoamProcessStatus::Completed,
                &good_mesh,
                residuals,
                &stationary_forces,
                &closed_mass,
                fields,
            )
        };
        // `T3-gradfree-wallsolve` as measured: pressure live in every cell,
        // both turbulence fields byte-identical between the last two writes.
        let dead_turbulence = field_evidence(
            82_993,
            &[("p", 82_993), ("U", 82_990), ("k", 0), ("omega", 0)],
        );
        let judge = |residuals: &[ResidualSample]| judge_with(residuals, Some(&dead_turbulence));
        // `p` still descending, the way `T3-gradfree-wallsolve` was while its
        // turbulence pair was already dead.  `moving` is the live behaviour and
        // `dead` reproduces `T3`'s bit-identical values.
        let live_p = |step: usize| 4.0e-6 * (1.0 - 0.02 * step as f64);
        let build = |frozen_from: usize, total: usize| {
            let mut out = Vec::new();
            for step in 0..total {
                let iteration = 800 + step as u64;
                residuals_push(
                    &mut out,
                    iteration,
                    "p",
                    live_p(step),
                    live_p(step) * 1.0e-2,
                );
                residuals_push(&mut out, iteration, "Ux", 9.0e-7, 9.0e-9);
                residuals_push(&mut out, iteration, "Uy", 8.0e-7, 8.0e-9);
                // Before the freeze the turbulence pair is live; after it, both
                // reproduce exactly and report no solver work.
                let dead = step >= frozen_from;
                let k = if dead {
                    6.661_410_455_41e-9
                } else {
                    6.0e-7 * (1.0 + 0.05 * step as f64)
                };
                let omega = if dead {
                    9.790_306_527_98e-14
                } else {
                    4.0e-7 * (1.0 + 0.05 * step as f64)
                };
                residuals_push(
                    &mut out,
                    iteration,
                    "k",
                    k,
                    if dead { k } else { k * 1.0e-2 },
                );
                residuals_push(
                    &mut out,
                    iteration,
                    "omega",
                    omega,
                    if dead { omega } else { omega * 1.0e-2 },
                );
            }
            out
        };

        // Long freeze, as measured on `T3`: refused.
        let (outcome, detail) = judge(&build(0, 30));
        assert_eq!(outcome, CfdOutcome::Failed, "{detail}");
        assert!(detail.contains("omega 9.790e-14"), "{detail}");
        // The wording must state the OBSERVATION and its validity domain, not
        // assert a proven equation failure.
        assert!(detail.contains("unchanged in all"), "{detail}");
        assert!(detail.contains("ASCII precision"), "{detail}");
        assert!(detail.contains("OBSERVATION, not a proven"), "{detail}");
        assert!(!detail.contains("stopped being updated"), "{detail}");
        // `k` is named too.  It sits at `6.661e-9`, only 0.67x the inner
        // solver's own tolerance, so no residual-depth rule reaches it, but its
        // field is just as frozen as omega's, and that is what is measured.
        // This is the negative control the guard previously failed.
        assert!(detail.contains("k 6.661e-9"), "{detail}");
        assert!(
            detail.contains("unchanged in all 82993 internal cells"),
            "{detail}"
        );

        // Late-onset freeze: the turbulence pair dies six iterations before the
        // solver's own stopping rule fires, the likely case, because the
        // residual collapse that kills an equation is itself what lets
        // `residualControl` stop the run.  When the field is frozen, WHEN the
        // freeze started is irrelevant and the verdict is the same.
        let (outcome, detail) = judge(&build(24, 30));
        assert_eq!(outcome, CfdOutcome::Failed, "{detail}");
        assert!(detail.contains("is unchanged in all"), "{detail}");

        // `L2-medium-le2` in miniature, and the reason the residual pattern is
        // no longer a precondition: the field is frozen while the residual keeps
        // varying, because it is recomputed each outer iteration from a pressure
        // field that is still moving.  Every residual-shaped trigger (run
        // length, depth, zero solver work) misses this.  The field does not.
        let mut varying = Vec::new();
        for step in 0..30_u64 {
            let iteration = 3_971 + step;
            residuals_push(
                &mut varying,
                iteration,
                "p",
                8.5e-6 - 1.0e-9 * step as f64,
                1.3e-7,
            );
            residuals_push(&mut varying, iteration, "Ux", 8.6e-9, 8.6e-9);
            residuals_push(&mut varying, iteration, "Uy", 7.1e-9, 7.1e-9);
            // Wandering a per cent per iteration, so no run ever forms.
            let wobble = 1.0 + 0.01 * ((step % 7) as f64 - 3.0);
            residuals_push(
                &mut varying,
                iteration,
                "k",
                9.592e-9 * wobble,
                9.592e-9 * wobble,
            );
            residuals_push(
                &mut varying,
                iteration,
                "omega",
                6.088e-9 * wobble,
                6.088e-9 * wobble,
            );
        }
        let (outcome, detail) = judge(&varying);
        assert_eq!(
            outcome,
            CfdOutcome::Failed,
            "a frozen field with a varying residual is still a dead equation: {detail}"
        );

        // Without field evidence NOTHING certifies, whatever the residual
        // history looks like.  A decimated or holed history is not evidence of
        // a freeze either; so the verdict is neither `failed` nor
        // `numerically converged`, it is explicitly inconclusive.
        let sparse = build(0, 30)
            .into_iter()
            .filter(|sample| (sample.iteration - 800) % 10 == 0)
            .collect::<Vec<_>>();
        let holed = build(0, 30)
            .into_iter()
            .filter(|sample| sample.iteration != 827)
            .collect::<Vec<_>>();
        for (label, trace) in [
            ("a two-iteration run", build(28, 30)),
            ("a decimated history", sparse),
            ("a one-iteration hole", holed),
            ("a wholly ordinary history", build(30, 30)),
        ] {
            let (outcome, detail) = judge_with(&trace, None);
            assert_eq!(
                outcome,
                CfdOutcome::Unconverged,
                "{label} without field evidence must not certify: {detail}"
            );
            assert!(detail.contains("INCONCLUSIVE"), "{label}: {detail}");
            assert!(detail.contains("NOT CERTIFIED"), "{label}: {detail}");
            assert!(
                !detail.contains("unchanged in all"),
                "{label}: absent evidence must never read as unchanged: {detail}"
            );
        }

        // Partial evidence is missing evidence.  `omega` observed and the rest
        // absent is not a certificate for the rest.
        let partial = field_evidence(82_993, &[("omega", 82_865)]);
        let (outcome, detail) = judge_with(&build(30, 30), Some(&partial));
        assert_eq!(outcome, CfdOutcome::Unconverged, "{detail}");
        assert!(detail.contains("4 of 5 primary equations"), "{detail}");

        // `V1-inletoutlet-coarse`, verbatim from `logs/simpleFoam-final.log`:
        // omega is below the inner solver's 1e-8 tolerance and reports
        // `No Iterations 0` in 19 of its last 20 iterations (exactly like a
        // dead equation) but it drifts about 5 % per iteration in a sawtooth,
        // so it is a new measurement every time and the case is converged.
        let measured = [
            4.893_109_210_71e-9,
            5.155_421_119_69e-9,
            5.421_691_747_48e-9,
            5.693_193_274_42e-9,
            5.968_791_989_69e-9,
            6.247_639_490_11e-9,
            6.530_358_124_58e-9,
            6.814_328_062_13e-9,
            7.098_931_124_56e-9,
            7.383_711_251_97e-9,
            7.668_236_633_1e-9,
            7.952_093_254e-9,
            8.234_885_022_06e-9,
            8.516_241_493_16e-9,
            8.795_827_770_32e-9,
            9.072_973_010_08e-9,
            9.347_571_474_19e-9,
            9.619_457_288_01e-9,
            9.888_487_806_99e-9,
            1.015_454_331_78e-8,
            3.115_128_097_98e-9,
            3.067_469_295_67e-9,
            3.124_650_400_91e-9,
            3.222_638_129_49e-9,
        ];
        let mut v1 = Vec::new();
        for (step, omega) in measured.into_iter().enumerate() {
            let iteration = 784 + step as u64;
            residuals_push(
                &mut v1,
                iteration,
                "p",
                9.986_799_386_12e-6,
                2.053_858_910_49e-7,
            );
            residuals_push(&mut v1, iteration, "Ux", 1.010_646_495_96e-6, 1.0e-8);
            residuals_push(&mut v1, iteration, "Uy", 6.070_693_604_19e-7, 6.0e-9);
            residuals_push(&mut v1, iteration, "k", 1.002_921_445_5e-7, 1.0e-9);
            // Below the inner floor, so zero solver iterations and
            // initial == final, exactly as the real log prints it.
            residuals_push(&mut v1, iteration, "omega", omega, omega);
        }
        // `V1`'s fields really are still moving: 99.88 % of `k` cells and
        // 99.90 % of `omega` cells changed over its last written interval.
        let (outcome, detail) = judge_with(&v1, Some(&live_fields()));
        assert_eq!(outcome, CfdOutcome::NumericallyConverged, "{detail}");

        // NEGATIVE CONTROL 1: healthy sub-floor equation, `G3-fine-p404`'s
        // omega shape: parked at `1.00x` the inner tolerance, reproduced at the
        // endpoint, no solver work, while pressure moves.  Every residual-only
        // rule this gate has carried called this either dead or alive by
        // guesswork.  With live-field evidence it is alive, full stop.
        let mut subfloor = Vec::new();
        for step in 0..30_u64 {
            let iteration = 5_971 + step;
            residuals_push(
                &mut subfloor,
                iteration,
                "p",
                9.97e-6 - 1.0e-9 * step as f64,
                1.3e-7,
            );
            residuals_push(&mut subfloor, iteration, "Ux", 8.47e-9, 8.47e-9);
            residuals_push(&mut subfloor, iteration, "Uy", 8.31e-9, 8.31e-9);
            residuals_push(
                &mut subfloor,
                iteration,
                "k",
                9.856_491_062_35e-9,
                9.856_491_062_35e-9,
            );
            residuals_push(
                &mut subfloor,
                iteration,
                "omega",
                9.996_072_819_9e-9,
                9.996_072_819_9e-9,
            );
        }
        let (outcome, detail) = judge_with(&subfloor, Some(&live_fields()));
        assert_eq!(
            outcome,
            CfdOutcome::NumericallyConverged,
            "an equation parked at the solver floor with a LIVE field is converged: {detail}"
        );

        // NEGATIVE CONTROL 2: the same trace with the fields frozen, which is
        // what `G3-fine-p404` actually measured: 0 of 436 389 cells changed for
        // both `k` and `omega` while pressure changed in 99.99 % of them.  Same
        // residuals, opposite verdict, and the evidence is what changed.
        let frozen_fields = field_evidence(
            436_389,
            &[("p", 436_364), ("U", 436_300), ("k", 0), ("omega", 0)],
        );
        let (outcome, detail) = judge_with(&subfloor, Some(&frozen_fields));
        assert_eq!(
            outcome,
            CfdOutcome::Failed,
            "the same residuals with a FROZEN field are a dead equation: {detail}"
        );
        assert!(detail.contains("k 9.856e-9"), "{detail}");
        assert!(detail.contains("omega 9.996e-9"), "{detail}");

        // NEGATIVE CONTROL 3: dead `k` ONLY, at `T3`'s measured `6.661e-9`,
        // beside a healthy omega.  This is the hole a residual-depth rule
        // leaves open: `0.67x` the floor is never "far below" anything, so no
        // depth cut-off reaches it, and omega cannot rescue the case by being
        // fine.  The field evidence names `k` and refuses.
        let mut dead_k_only = Vec::new();
        for step in 0..30_u64 {
            let iteration = 792 + step;
            residuals_push(
                &mut dead_k_only,
                iteration,
                "p",
                4.0e-6 - 1.0e-9 * step as f64,
                4.0e-8,
            );
            residuals_push(&mut dead_k_only, iteration, "Ux", 5.0e-7, 1.3e-8);
            residuals_push(&mut dead_k_only, iteration, "Uy", 2.9e-7, 8.0e-9);
            residuals_push(
                &mut dead_k_only,
                iteration,
                "k",
                6.661_410_455_41e-9,
                6.661_410_455_41e-9,
            );
            // omega still being solved, so it is a new measurement every time.
            let omega = 3.0e-9 * (1.0 + 0.05 * step as f64);
            residuals_push(&mut dead_k_only, iteration, "omega", omega, omega * 1.0e-2);
        }
        let (outcome, detail) = judge_with(
            &dead_k_only,
            Some(&field_evidence(
                82_993,
                &[("p", 82_993), ("U", 82_990), ("k", 0), ("omega", 82_865)],
            )),
        );
        assert_eq!(
            outcome,
            CfdOutcome::Failed,
            "a dead k beside a healthy omega must still refuse the case: {detail}"
        );
        assert!(detail.contains("k 6.661e-9"), "{detail}");
        assert!(!detail.contains("omega"), "omega is alive: {detail}");

        // NEGATIVE CONTROL 4: no field evidence at all.  The residual history
        // cannot decide, so the gate says so and does NOT certify.  Guessing
        // here in either direction is what produced both previous false
        // verdicts.
        let (outcome, detail) = judge_with(&subfloor, None);
        assert_eq!(
            outcome,
            CfdOutcome::Unconverged,
            "without field evidence the answer is inconclusive: {detail}"
        );
        assert!(detail.contains("INCONCLUSIVE"), "{detail}");
        assert!(detail.contains("NOT CERTIFIED"), "{detail}");

        // A genuinely stationary answer reproduces EVERY equation's residual,
        // because nothing in it is moving.  That is what exact steady
        // convergence looks like and it must not be refused for being steady:
        // the guard fires on the contrast between a dead equation and a
        // solution still changing around it, never on stillness alone.
        let mut steady = Vec::new();
        for step in 0..30_u64 {
            let iteration = 900 + step;
            for (field, value) in [
                ("p", 4.0e-6),
                ("Ux", 9.0e-7),
                ("Uy", 8.0e-7),
                ("k", 5.0e-9),
                ("omega", 2.0e-9),
            ] {
                let worked = value >= 1.0e-8;
                residuals_push(
                    &mut steady,
                    iteration,
                    field,
                    value,
                    if worked { value * 1.0e-2 } else { value },
                );
            }
        }
        let (outcome, detail) = judge_with(
            &steady,
            Some(&field_evidence(
                82_975,
                &[("p", 0), ("U", 0), ("k", 0), ("omega", 0)],
            )),
        );
        assert_eq!(outcome, CfdOutcome::NumericallyConverged, "{detail}");
    }
    #[test]
    fn the_plausibility_screen_does_not_replace_the_residual_gate() {
        // The screen must never turn an unconverged case green: a perfectly
        // ordinary coefficient set with a stalled residual stays unconverged,
        // and the reported reason stays the residual one.
        let config = CfdStudyConfig::default();
        let residuals = [
            ("p", 1.0e-4),
            ("Ux", 1.0e-6),
            ("Uy", 1.0e-6),
            ("k", 1.0e-6),
            ("omega", 1.0e-6),
        ]
        .into_iter()
        .map(|(field, initial)| ResidualSample {
            iteration: 2_000,
            field: field.to_owned(),
            initial,
            final_residual: initial * 1.0e-2,
        })
        .collect::<Vec<_>>();
        let (outcome, detail) = classify_convergence(
            &config,
            OpenFoamProcessStatus::Completed,
            &MeshQuality {
                passed: true,
                ..MeshQuality::default()
            },
            &residuals,
            &[force_sample(0.49049, 0.0093984, -0.0047371)],
            &[],
            Some(&live_fields()),
        );
        assert_eq!(outcome, CfdOutcome::Unconverged);
        assert!(detail.contains("residual tolerance"), "{detail}");
    }

    #[test]
    fn an_unconverged_result_names_the_limiting_equations_and_their_distance() {
        let config = CfdStudyConfig::default();
        // The measured signature of a stalled airfoil case: velocity is well
        // below tolerance while pressure and omega sit on a plateau.
        let levels = [
            ("p", 1.4e-4),
            ("Ux", 1.9e-6),
            ("Uy", 5.5e-6),
            ("k", 2.6e-5),
            ("omega", 4.3e-4),
        ];
        let residuals = levels
            .into_iter()
            .map(|(field, initial)| ResidualSample {
                iteration: 2_000,
                field: field.to_owned(),
                initial,
                final_residual: initial * 1.0e-2,
            })
            .collect::<Vec<_>>();
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
            Some(&live_fields()),
        );
        assert_eq!(outcome, CfdOutcome::Unconverged);
        assert!(detail.contains("iteration 2000"), "{detail}");
        assert!(detail.contains("3 of 5"), "{detail}");
        // Worst first, so the reader sees what to work on.
        let omega = detail.find("omega").expect("omega is named");
        let pressure = detail.find("p 1.400e-4").expect("p is named");
        let k = detail.find("k 2.600e-5").expect("k is named");
        assert!(omega < pressure && pressure < k, "{detail}");
        assert!(detail.contains("43x tolerance"), "{detail}");
        // Equations already below tolerance must not be listed as blockers.
        assert!(!detail.contains("ux"), "{detail}");
        assert!(!detail.contains("uy"), "{detail}");
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
            Some(&live_fields()),
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
            Some(&live_fields()),
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
            Some(&live_fields()),
        );
        assert_eq!(outcome, CfdOutcome::Unconverged);
        assert!(detail.contains("final log iteration"));
        assert!(detail.contains("ux"));
    }
}
