// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// The status a stage leaves the study in, or `None` when it completed.
///
/// A stage that could not even be resolved or logged (`Err`) is recorded as
/// `__failure` under `label` and reported as a launch failure; a stage that
/// ran and did not complete has already recorded its own `__failure` in
/// `execute_resolved_stage`.
fn stage_failure(
    outcome: Result<OpenFoamProcessStatus, String>,
    label: &str,
    command_logs: &mut BTreeMap<String, String>,
) -> Option<OpenFoamProcessStatus> {
    match outcome {
        Ok(OpenFoamProcessStatus::Completed) => None,
        Ok(status) => Some(status),
        Err(error) => {
            command_logs.insert("__failure".to_owned(), format!("{label}: {error}"));
            Some(OpenFoamProcessStatus::LaunchFailed)
        }
    }
}

/// Record a case-file failure between stages as `__failure` and return the
/// status it leaves the study in.
fn file_failure(
    command_logs: &mut BTreeMap<String, String>,
    message: String,
) -> OpenFoamProcessStatus {
    command_logs.insert("__failure".to_owned(), message);
    OpenFoamProcessStatus::Failed
}

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
    let command_available = |tool: &str| capabilities.commands.get(tool).copied().unwrap_or(false);
    let mut missing = ["gmshToFoam", "checkMesh", "postProcess", solver_name]
        .into_iter()
        .filter(|tool| !command_available(tool))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if !command_available("gmsh") {
        missing.push("gmsh".to_owned());
    }
    if !missing.is_empty() {
        command_logs.insert(
            "__failure".to_owned(),
            format!(
                "OpenFOAM unavailable for Mach-derived {} path; missing {} ({}).",
                solver_name,
                missing.join(", "),
                capabilities.detail,
            ),
        );
        return finish_failed(
            command_logs,
            mesh_output,
            OpenFoamProcessStatus::LaunchFailed,
        );
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
        return finish_failed(
            command_logs,
            mesh_output,
            OpenFoamProcessStatus::LaunchFailed,
        );
    }
    let gmsh = execute_gmsh_stage(
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
    );
    if let Some(status) = stage_failure(gmsh, "gmsh", &mut command_logs) {
        return finish_failed(command_logs, mesh_output, status);
    }
    let convert = execute_stage(
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
    );
    if let Some(status) = stage_failure(convert, "gmshToFoam", &mut command_logs) {
        return finish_failed(command_logs, mesh_output, status);
    }
    let boundary_path = case_dir.join("constant/polyMesh/boundary");
    let boundary_report = match mesh::ensure_boundary_patch_types(&boundary_path) {
        Ok(report) => report,
        Err(error) => {
            let status = file_failure(
                &mut command_logs,
                format!("boundary contract: converted mesh could not be validated: {error}"),
            );
            return finish_failed(command_logs, mesh_output, status);
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
    let check = execute_stage(
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
    );
    if let Some(status) = stage_failure(check, "checkMesh", &mut command_logs) {
        return finish_failed(command_logs, mesh_output, status);
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
        return finish_failed_quality(command_logs, quality, OpenFoamProcessStatus::Failed);
    }
    if !simulation.compressible && command_available("potentialFoam") {
        let init = execute_stage(
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
        );
        if let Some(status) = stage_failure(init, "potentialFoam", &mut command_logs) {
            return finish_failed_quality(command_logs, quality, status);
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
            let status = file_failure(&mut command_logs, format!("{solver_name}-startup: {error}"));
            return finish_failed_quality(command_logs, quality, status);
        }
        let startup_label = format!("{solver_name}-startup");
        let startup = execute_solver_stage_with_tool(
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
        );
        if let Some(status) = stage_failure(startup, &startup_label, &mut command_logs) {
            return finish_failed_quality(command_logs, quality, status);
        }
        if let Err(error) = fs::write(case_dir.join("system/fvSchemes"), fv_schemes(config, false))
        {
            let status = file_failure(
                &mut command_logs,
                format!("{solver_name}-final: cannot switch to final convection scheme: {error}"),
            );
            return finish_failed_quality(command_logs, quality, status);
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
            let status = file_failure(&mut command_logs, format!("{solver_name}-final: {error}"));
            return finish_failed_quality(command_logs, quality, status);
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
            let status = file_failure(&mut command_logs, format!("{solver_name}: {error}"));
            return finish_failed_quality(command_logs, quality, status);
        }
        if let Err(error) = fs::write(case_dir.join("system/fvSchemes"), fv_schemes(config, false))
        {
            let status = file_failure(
                &mut command_logs,
                format!("{solver_name}: cannot write final convection scheme: {error}"),
            );
            return finish_failed_quality(command_logs, quality, status);
        }
    }
    let solution_label = if two_stage_solver {
        format!("{solver_name}-final")
    } else {
        solver_name.to_owned()
    };
    let solution = execute_solver_stage_with_tool(
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
    );
    if let Some(status) = stage_failure(solution, &solution_label, &mut command_logs) {
        return finish_failed_quality(command_logs, quality, status);
    }
    let post = execute_solver_postprocess_stage_with_tool(
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
    );
    let post_label = format!("{solver_name}-postProcess");
    if let Some(status) = stage_failure(post, &post_label, &mut command_logs) {
        return finish_failed_quality(command_logs, quality, status);
    }
    let quality = parse_mesh_quality(&mesh_output);
    let results = build_results_with_quality(
        config,
        &generated,
        command_logs,
        quality,
        OpenFoamProcessStatus::Completed,
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

#[cfg(test)]
// A failed expect here is the test's own fixture or assertion failing.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use alas_exec::openfoam::{OpenFoamBackend, OpenFoamPreferences};

    #[test]
    fn an_unavailable_toolchain_is_a_persisted_launch_failure_naming_every_missing_tool() {
        let root = std::env::temp_dir().join(format!(
            "alas-cfd-runner-missing-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        let absent = root.join("absent-openfoam-bin");
        let adapter = OpenFoamAdapter::resolve(OpenFoamPreferences {
            backend: OpenFoamBackend::Native,
            native_bin_dir: Some(absent.to_string_lossy().into_owned()),
            gmsh_executable: Some(absent.join("gmsh.exe").to_string_lossy().into_owned()),
            ..OpenFoamPreferences::default()
        });
        let config = CfdStudyConfig::default();
        let solver = config.effective_simulation().solver.executable();
        let cancel = Arc::new(AtomicBool::new(false));

        let results = run_study(&config, &adapter, &root.join("case"), &cancel, |_| {})
            .expect("a missing toolchain is a recorded outcome, not an error");

        let failure = results
            .command_logs
            .get("__failure")
            .expect("the failure reason is recorded");
        assert!(
            failure.contains(&format!(
                "missing gmshToFoam, checkMesh, postProcess, {solver}, gmsh"
            )),
            "{failure}"
        );
        assert_eq!(
            results.provenance.template_version, TEMPLATE_VERSION,
            "the generated case is still recorded"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
