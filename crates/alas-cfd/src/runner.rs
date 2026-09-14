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
    if !capabilities.available {
        command_logs.insert(
            "__failure".to_owned(),
            format!("OpenFOAM unavailable: {}", capabilities.detail),
        );
        final_process_status = OpenFoamProcessStatus::LaunchFailed;
        return finish_failed(command_logs, mesh_output, final_process_status);
    }

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
        case_dir,
        config.solver.timeout_seconds,
        cancel,
        &mut emit,
        started,
        &mut command_logs,
        &mut mesh_output,
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
        case_dir,
        config.solver.timeout_seconds,
        cancel,
        &mut emit,
        started,
        &mut command_logs,
        &mut mesh_output,
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
        case_dir,
        config.solver.timeout_seconds,
        cancel,
        &mut emit,
        started,
        &mut command_logs,
        &mut mesh_output,
        CfdStage::QualityGate,
        "checkMesh",
        Vec::new(),
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
    if capabilities
        .commands
        .get("potentialFoam")
        .copied()
        .unwrap_or(false)
    {
        let init_status = match execute_stage(
            adapter,
            case_dir,
            config.solver.timeout_seconds,
            cancel,
            &mut emit,
            started,
            &mut command_logs,
            &mut mesh_output,
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
    } else {
        emit_event(
            &mut emit,
            started,
            CfdStage::Initialization,
            CfdEventSeverity::Warning,
            "potentialFoam is unavailable; solving from the documented initial fields.".to_owned(),
        );
    }
    let two_stage_solver = config.solver.startup_iterations > 0
        && config.solver.startup_iterations < config.solver.max_iterations
        && config.solver.convection_scheme != ConvectionScheme::BoundedUpwind;
    if two_stage_solver {
        if let Err(error) = rewrite_solver_control_dict(
            case_dir,
            config.solver.startup_iterations,
            false,
            Some(
                config
                    .solver
                    .startup_iterations
                    .min(config.solver.write_interval.max(1)),
            ),
        ) {
            command_logs.insert(
                "__failure".to_owned(),
                format!("simpleFoam-startup: {error}"),
            );
            final_process_status = OpenFoamProcessStatus::Failed;
            return finish_failed_quality(command_logs, quality, final_process_status);
        }
        let startup_status = match execute_solver_stage(
            adapter,
            case_dir,
            config.solver.timeout_seconds,
            cancel,
            &mut emit,
            started,
            &mut command_logs,
            &mut mesh_output,
            CfdStage::Solution,
            "simpleFoam-startup",
        ) {
            Ok(status) => status,
            Err(error) => {
                command_logs.insert(
                    "__failure".to_owned(),
                    format!("simpleFoam-startup: {error}"),
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
                format!("simpleFoam-final: cannot switch to final convection scheme: {error}"),
            );
            final_process_status = OpenFoamProcessStatus::Failed;
            return finish_failed_quality(command_logs, quality, final_process_status);
        }
        if let Err(error) = rewrite_solver_control_dict(
            case_dir,
            config.solver.max_iterations,
            true,
            Some(final_write_interval(
                config.solver.max_iterations,
                config.solver.write_interval,
            )),
        ) {
            command_logs.insert("__failure".to_owned(), format!("simpleFoam-final: {error}"));
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
                config.solver.convection_scheme.as_str()
            ),
        );
    } else {
        if let Err(error) = rewrite_solver_control_dict(
            case_dir,
            config.solver.max_iterations,
            false,
            Some(final_write_interval(
                config.solver.max_iterations,
                config.solver.write_interval,
            )),
        ) {
            command_logs.insert("__failure".to_owned(), format!("simpleFoam: {error}"));
            final_process_status = OpenFoamProcessStatus::Failed;
            return finish_failed_quality(command_logs, quality, final_process_status);
        }
        if let Err(error) = fs::write(case_dir.join("system/fvSchemes"), fv_schemes(config, false))
        {
            command_logs.insert(
                "__failure".to_owned(),
                format!("simpleFoam: cannot write final convection scheme: {error}"),
            );
            final_process_status = OpenFoamProcessStatus::Failed;
            return finish_failed_quality(command_logs, quality, final_process_status);
        }
    }
    let solution_label = if two_stage_solver {
        "simpleFoam-final"
    } else {
        "simpleFoam"
    };
    let solution_status = match execute_solver_stage(
        adapter,
        case_dir,
        config.solver.timeout_seconds,
        cancel,
        &mut emit,
        started,
        &mut command_logs,
        &mut mesh_output,
        CfdStage::Solution,
        solution_label,
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
    let post_status = match execute_solver_postprocess_stage(
        adapter,
        case_dir,
        config.solver.timeout_seconds,
        cancel,
        &mut emit,
        started,
        &mut command_logs,
        &mut mesh_output,
    ) {
        Ok(status) => status,
        Err(error) => {
            command_logs.insert(
                "__failure".to_owned(),
                format!("simpleFoam-postProcess: {error}"),
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
