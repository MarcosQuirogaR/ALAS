// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
/// Run a complete isolated airfoil study through the configured OpenFOAM backend.
pub fn run_study<F>(
    config: &CfdStudyConfig,
    adapter: &OpenFoamAdapter,
    case_dir: &Path,
    cancel: &Arc<AtomicBool>,
    mut emit: F,
) -> Result<CfdResults, String>
where
    F: FnMut(CfdRunEvent),
{
    let started = std::time::Instant::now();
    config.validate().map_err(|errors| errors.join(" "))?;
    let simulation = config.effective_simulation();
    let solver_name = simulation.solver.executable();
    let generated = generate_case(config, case_dir)?;
    let finish_failed =
        |logs, output, status| persist_failed_results(config, &generated, logs, output, status);
    let finish_failed_quality = |logs, quality, status| {
        persist_failed_results_with_quality(config, &generated, logs, quality, status)
    };
    emit_event(
        &mut emit,
        started,
        CfdStage::GeometryPreparation,
        CfdEventSeverity::Info,
        format!(
            "Prepared {} with coordinate hash {}.",
            generated.airfoil.name, generated.airfoil.coordinate_hash
        ),
    );
    let mut command_logs = BTreeMap::new();
    let mut final_process_status = OpenFoamProcessStatus::Completed;
    let mut mesh_output = String::new();

    let capabilities = adapter.probe();
    command_logs.insert(
        "__backend".to_owned(),
        capabilities.backend.as_str().to_owned(),
    );
    command_logs.insert(
        "__openfoam_version".to_owned(),
        capabilities
            .version
            .clone()
            .unwrap_or_else(|| "unknown".to_owned()),
    );
    let required_commands_available = ["gmshToFoam", "checkMesh", "postProcess", solver_name]
        .iter()
        .all(|tool| capabilities.commands.get(*tool).copied().unwrap_or(false))
        && capabilities.commands.get("gmsh").copied().unwrap_or(false);
    if !required_commands_available {
        let mut missing = ["gmshToFoam", "checkMesh", "postProcess", solver_name]
            .iter()
            .filter(|tool| !capabilities.commands.get(**tool).copied().unwrap_or(false))
            .map(|tool| (*tool).to_owned())
            .collect::<Vec<_>>();
        if !capabilities.commands.get("gmsh").copied().unwrap_or(false) {
            missing.push("gmsh".to_owned());
        }
        command_logs.insert(
            "__failure".to_owned(),
            format!(
                "OpenFOAM unavailable for Mach-derived {} path; missing {} ({}).",
                solver_name,
                missing.join(", "),
                capabilities.detail,
            ),
        );
        final_process_status = OpenFoamProcessStatus::LaunchFailed;
        return finish_failed(command_logs, mesh_output, final_process_status);
    }
    emit_event(
        &mut emit,
        started,
        CfdStage::GeometryPreparation,
        CfdEventSeverity::Info,
        format!(
            "Mach {:.3} -> {} ({}) with automatic regime-specific settings.",
            config.mach_number(),
            simulation.regime.as_str(),
            solver_name
        ),
    );

    if !adapter.probe_gmsh() {
        command_logs.insert(
            "__failure".to_owned(),
            "Gmsh is unavailable. Configure the Gmsh executable in CFD settings or add gmsh to PATH; the study will not substitute snappyHexMesh."
                .to_owned(),
        );
        final_process_status = OpenFoamProcessStatus::LaunchFailed;
        return finish_failed(command_logs, mesh_output, final_process_status);
    }
    let gmsh_status = match execute_gmsh_stage(
        adapter,
        &mut StageContext {
            case_dir,
            timeout_seconds: config.solver.timeout_seconds,
            cancel,
            started,
            command_logs: &mut command_logs,
            mesh_output: &mut mesh_output,
        },
        &mut emit,
        CfdStage::Meshing,
        vec![
            "-3".into(),
            "system/airfoil.geo".into(),
            "-format".into(),
            "msh2".into(),
            "-o".into(),
            "constant/triSurface/airfoil.msh".into(),
        ],
    ) {
        Ok(status) => status,
        Err(error) => {
            command_logs.insert("__failure".to_owned(), format!("gmsh: {error}"));
            final_process_status = OpenFoamProcessStatus::LaunchFailed;
            return finish_failed(command_logs, mesh_output, final_process_status);
        }
    };
    if gmsh_status != OpenFoamProcessStatus::Completed {
        final_process_status = gmsh_status;
        return finish_failed(command_logs, mesh_output, final_process_status);
    }
    let convert_status = match execute_stage(
        adapter,
        &mut StageContext {
            case_dir,
            timeout_seconds: config.solver.timeout_seconds,
            cancel,
            started,
            command_logs: &mut command_logs,
            mesh_output: &mut mesh_output,
        },
        &mut emit,
        CfdStage::Meshing,
        "gmshToFoam",
        vec!["constant/triSurface/airfoil.msh".into()],
    ) {
        Ok(status) => status,
        Err(error) => {
            command_logs.insert("__failure".to_owned(), format!("gmshToFoam: {error}"));
            final_process_status = OpenFoamProcessStatus::LaunchFailed;
            return finish_failed(command_logs, mesh_output, final_process_status);
        }
    };
    if convert_status != OpenFoamProcessStatus::Completed {
        final_process_status = convert_status;
        return finish_failed(command_logs, mesh_output, final_process_status);
    }
    let boundary_path = case_dir.join("constant/polyMesh/boundary");
    let boundary_report = match mesh::ensure_boundary_patch_types(&boundary_path) {
        Ok(report) => report,
        Err(error) => {
            command_logs.insert(
                "__failure".to_owned(),
                format!("boundary contract: converted mesh could not be validated: {error}"),
            );
            final_process_status = OpenFoamProcessStatus::Failed;
            return finish_failed(command_logs, mesh_output, final_process_status);
        }
    };
    emit_event(
        &mut emit,
        started,
        CfdStage::Meshing,
        CfdEventSeverity::Info,
        format!(
            "Converted Gmsh mesh exposes {} required patches (updated={}).",
            boundary_report.patches.len(),
            boundary_report.updated
        ),
    );
    let check_status = match execute_stage(
        adapter,
        &mut StageContext {
            case_dir,
            timeout_seconds: config.solver.timeout_seconds,
            cancel,
            started,
            command_logs: &mut command_logs,
            mesh_output: &mut mesh_output,
        },
        &mut emit,
        CfdStage::QualityGate,
        "checkMesh",
        vec!["-writeAllFields".into(), "-meshQuality".into()],
    ) {
        Ok(status) => status,
        Err(error) => {
            command_logs.insert("__failure".to_owned(), format!("checkMesh: {error}"));
            final_process_status = OpenFoamProcessStatus::LaunchFailed;
            return finish_failed(command_logs, mesh_output, final_process_status);
        }
    };
    if check_status != OpenFoamProcessStatus::Completed {
        final_process_status = check_status;
        return finish_failed(command_logs, mesh_output, final_process_status);
    }
    let quality = parse_mesh_quality(&mesh_output);
    // The solver deserves a budget proportional to the work it was asked to do;
    // the configured timeout stays the guard for the short utilities and the
    // floor for this one.  See `SolverSettings::solver_timeout_seconds`.
    let mut timeout_settings = config.solver.clone();
    timeout_settings.max_iterations = simulation.max_iterations;
    let solver_timeout_seconds = timeout_settings.solver_timeout_seconds(quality.cells);
    if !quality.passed {
        emit_event(
            &mut emit,
            started,
            CfdStage::QualityGate,
            CfdEventSeverity::Error,
            "Mesh rejected by the quality gate; solution was not launched.".to_owned(),
        );
        final_process_status = OpenFoamProcessStatus::Failed;
        return finish_failed_quality(command_logs, quality, final_process_status);
    }
    if !simulation.compressible
        && capabilities
            .commands
            .get("potentialFoam")
            .copied()
            .unwrap_or(false)
    {
        let init_status = match execute_stage(
            adapter,
            &mut StageContext {
                case_dir,
                timeout_seconds: config.solver.timeout_seconds,
                cancel,
                started,
                command_logs: &mut command_logs,
                mesh_output: &mut mesh_output,
            },
            &mut emit,
            CfdStage::Initialization,
            "potentialFoam",
            vec!["-initialiseUBCs".into(), "-writephi".into()],
        ) {
            Ok(status) => status,
            Err(error) => {
                command_logs.insert("__failure".to_owned(), format!("potentialFoam: {error}"));
                final_process_status = OpenFoamProcessStatus::LaunchFailed;
                return finish_failed_quality(command_logs, quality.clone(), final_process_status);
            }
        };
        if init_status != OpenFoamProcessStatus::Completed {
            final_process_status = init_status;
            return finish_failed_quality(command_logs, quality.clone(), final_process_status);
        }
    } else if simulation.compressible {
        emit_event(
            &mut emit,
            started,
            CfdStage::Initialization,
            CfdEventSeverity::Info,
            format!(
                "Skipping potentialFoam for the compressible {} path; starting from the thermodynamic freestream fields.",
                solver_name
            ),
        );
    } else {
        emit_event(
            &mut emit,
            started,
            CfdStage::Initialization,
            CfdEventSeverity::Warning,
            "potentialFoam is unavailable; solving from the documented initial fields.".to_owned(),
        );
    }
    let two_stage_solver = simulation.startup_iterations > 0
        && simulation.startup_iterations < simulation.max_iterations
        && simulation.convection_scheme != ConvectionScheme::BoundedUpwind;
    if two_stage_solver {
        if let Err(error) = rewrite_solver_control_dict(
            case_dir,
            simulation.startup_iterations,
            false,
            Some(
                simulation
                    .startup_iterations
                    .min(simulation.write_interval.max(1)),
            ),
        ) {
            command_logs.insert(
                "__failure".to_owned(),
                format!("{solver_name}-startup: {error}"),
            );
            final_process_status = OpenFoamProcessStatus::Failed;
            return finish_failed_quality(command_logs, quality, final_process_status);
        }
        let startup_label = format!("{solver_name}-startup");
        let startup_status = match execute_solver_stage_with_tool(
            adapter,
            &mut StageContext {
                case_dir,
                timeout_seconds: solver_timeout_seconds,
                cancel,
                started,
                command_logs: &mut command_logs,
                mesh_output: &mut mesh_output,
            },
            &mut emit,
            CfdStage::Solution,
            &startup_label,
            solver_name,
        ) {
            Ok(status) => status,
            Err(error) => {
                command_logs.insert(
                    "__failure".to_owned(),
                    format!("{solver_name}-startup: {error}"),
                );
                final_process_status = OpenFoamProcessStatus::LaunchFailed;
                return finish_failed_quality(command_logs, quality, final_process_status);
            }
        };
        if startup_status != OpenFoamProcessStatus::Completed {
            final_process_status = startup_status;
            return finish_failed_quality(command_logs, quality, final_process_status);
        }
        if let Err(error) = fs::write(case_dir.join("system/fvSchemes"), fv_schemes(config, false))
        {
            command_logs.insert(
                "__failure".to_owned(),
                format!("{solver_name}-final: cannot switch to final convection scheme: {error}"),
            );
            final_process_status = OpenFoamProcessStatus::Failed;
            return finish_failed_quality(command_logs, quality, final_process_status);
        }
        if let Err(error) = rewrite_solver_control_dict(
            case_dir,
            simulation.max_iterations,
            true,
            Some(final_write_interval(
                simulation.max_iterations,
                simulation.write_interval,
            )),
        ) {
            command_logs.insert(
                "__failure".to_owned(),
                format!("{solver_name}-final: {error}"),
            );
            final_process_status = OpenFoamProcessStatus::Failed;
            return finish_failed_quality(command_logs, quality, final_process_status);
        }
        emit_event(
            &mut emit,
            started,
            CfdStage::Solution,
            CfdEventSeverity::Info,
            format!(
                "Switching from bounded upwind startup to {} for the final SIMPLE stage.",
                simulation.convection_scheme.as_str()
            ),
        );
    } else {
        if let Err(error) = rewrite_solver_control_dict(
            case_dir,
            simulation.max_iterations,
            false,
            Some(final_write_interval(
                simulation.max_iterations,
                simulation.write_interval,
            )),
        ) {
            command_logs.insert("__failure".to_owned(), format!("{solver_name}: {error}"));
            final_process_status = OpenFoamProcessStatus::Failed;
            return finish_failed_quality(command_logs, quality, final_process_status);
        }
        if let Err(error) = fs::write(case_dir.join("system/fvSchemes"), fv_schemes(config, false))
        {
            command_logs.insert(
                "__failure".to_owned(),
                format!("{solver_name}: cannot write final convection scheme: {error}"),
            );
            final_process_status = OpenFoamProcessStatus::Failed;
            return finish_failed_quality(command_logs, quality, final_process_status);
        }
    }
    let solution_label = if two_stage_solver {
        format!("{solver_name}-final")
    } else {
        solver_name.to_owned()
    };
    let solution_status = match execute_solver_stage_with_tool(
        adapter,
        &mut StageContext {
            case_dir,
            timeout_seconds: solver_timeout_seconds,
            cancel,
            started,
            command_logs: &mut command_logs,
            mesh_output: &mut mesh_output,
        },
        &mut emit,
        CfdStage::Solution,
        &solution_label,
        solver_name,
    ) {
        Ok(status) => status,
        Err(error) => {
            command_logs.insert("__failure".to_owned(), format!("{solution_label}: {error}"));
            final_process_status = OpenFoamProcessStatus::LaunchFailed;
            return finish_failed_quality(command_logs, quality, final_process_status);
        }
    };
    if solution_status != OpenFoamProcessStatus::Completed {
        final_process_status = solution_status;
        return finish_failed_quality(command_logs, quality, final_process_status);
    }
    let post_status = match execute_solver_postprocess_stage_with_tool(
        adapter,
        &mut StageContext {
            case_dir,
            timeout_seconds: solver_timeout_seconds,
            cancel,
            started,
            command_logs: &mut command_logs,
            mesh_output: &mut mesh_output,
        },
        &mut emit,
        solver_name,
    ) {
        Ok(status) => status,
        Err(error) => {
            command_logs.insert(
                "__failure".to_owned(),
                format!("{solver_name}-postProcess: {error}"),
            );
            final_process_status = OpenFoamProcessStatus::LaunchFailed;
            return finish_failed_quality(command_logs, quality, final_process_status);
        }
    };
    if post_status != OpenFoamProcessStatus::Completed {
        final_process_status = post_status;
        return finish_failed_quality(command_logs, quality, final_process_status);
    }
    let quality = parse_mesh_quality(&mesh_output);
    let results = build_results_with_quality(
        config,
        &generated,
        command_logs,
        quality,
        final_process_status,
    );
    emit_event(
        &mut emit,
        started,
        CfdStage::PostProcessing,
        if results.outcome == CfdOutcome::NumericallyConverged {
            CfdEventSeverity::Info
        } else {
            CfdEventSeverity::Warning
        },
        format!(
            "Study finished as {}: {}",
            results.outcome.as_str(),
            results.status_detail
        ),
    );
    write_result_artifacts(&results)?;
    Ok(results)
}
