// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

pub(crate) fn build_results(
    config: &CfdStudyConfig,
    generated: &GeneratedCase,
    command_logs: BTreeMap<String, String>,
    mesh_output: String,
    process_status: OpenFoamProcessStatus,
) -> CfdResults {
    build_results_with_quality(
        config,
        generated,
        command_logs,
        parse_mesh_quality(&mesh_output),
        process_status,
    )
}

pub(crate) fn build_results_with_quality(
    config: &CfdStudyConfig,
    generated: &GeneratedCase,
    command_logs: BTreeMap<String, String>,
    mesh_quality: MeshQuality,
    process_status: OpenFoamProcessStatus,
) -> CfdResults {
    let execution_config = config.with_effective_simulation();
    let solver_name = execution_config.solver_kind().executable();
    let mut mesh_quality = mesh_quality;
    mesh_quality.distributions = result_io::read_mesh_quality_distributions(&generated.path);
    mesh_quality.near_wall =
        result_io::read_y_plus_summary(&generated.path, "airfoil").map(|summary| {
            let sizing = mesh::boundary_layer_sizing(config).ok();
            NearWallDiagnostics {
                time: summary.time,
                patch_name: summary.patch_name,
                sample_count: summary.sample_count,
                min_y_plus: summary.min_y_plus,
                max_y_plus: summary.max_y_plus,
                average_y_plus: summary.average_y_plus,
                target_y_plus: config.mesh.target_y_plus,
                // Report the distance the mesh actually used, which is not the
                // configured length when it is derived from the y+ target.
                selected_wall_distance_m: sizing
                    .as_ref()
                    .map_or(config.mesh.first_layer_height_m, |value| {
                        value.selected_wall_distance_m
                    }),
                estimated_y_plus: sizing.map(|value| value.estimated_y_plus),
                source: summary.source,
            }
        });
    mesh_quality.near_wall_distribution =
        result_io::read_y_plus_distribution(&generated.path, "airfoil");
    // The exact boundary-face skewness maximum, which `checkMesh`'s headline
    // number is not: that one is the internal maximum.
    if let Some((max, patch)) = result_io::read_boundary_skewness_max(&generated.path) {
        mesh_quality.max_boundary_skewness = Some(max);
        mesh_quality.max_boundary_skewness_patch = Some(patch);
    }
    let solution_log = logs_with_prefix(&command_logs, solver_name);
    let mut residuals = parse_residuals(&solution_log);
    let post_log = logs_with_prefix(&command_logs, &format!("{solver_name}-postProcess"));
    residuals.extend(parse_residuals(&post_log));
    let mut mass_balance = parse_mass_balance(&solution_log);
    mass_balance.extend(parse_mass_balance(&post_log));
    let mut forces = result_io::read_force_history(&generated.path);
    let decomposition = result_io::read_force_decomposition(&generated.path);
    let reference = ReferenceConventions::from_config(config);
    apply_force_decomposition(
        &mut forces,
        &decomposition,
        config.density_kg_m3,
        reference.speed_m_s,
        reference.area_m2,
        reference.drag_direction,
        reference.lift_direction,
    );
    let final_stage_forces = forces_for_final_stage(&forces, &command_logs, solver_name);
    // Direct evidence that the solved fields are still being updated, read from
    // the case's own written times.  The gate needs it to tell a converged
    // equation from an abandoned one; a residual history cannot.
    let field_updates = read_field_update_evidence_for_config(
        &generated.path,
        execution_config.effective_simulation().compressible,
    );
    let (numerical_convergence, computed_status_detail) = classify_convergence(
        &execution_config,
        process_status,
        &mesh_quality,
        &residuals,
        &final_stage_forces,
        &mass_balance,
        Some(&field_updates),
    );
    // The declared mesh limits are a separate contract from the solver's
    // convergence criteria, and a case can satisfy one while violating the
    // other.  Both verdicts are kept; the reported outcome is the worse of the
    // two, and the status line says which one objected, so a mesh failure is
    // never hidden behind a converged solver and vice versa.
    // Read the converted mesh's own boundary file so the declared patch
    // contract is a measured check rather than an unproven declaration.
    let boundary =
        mesh::inspect_boundary_patch_types(&generated.path.join("constant/polyMesh/boundary")).ok();
    let mesh_qualification = qualify_mesh_with_boundary(
        &mesh::MeshQualityThresholds::template_defaults(),
        &mesh_quality,
        boundary.as_ref(),
    );
    let outcome = if mesh_qualification.passed {
        numerical_convergence
    } else {
        CfdOutcome::Failed
    };
    let computed_status_detail = if mesh_qualification.passed {
        computed_status_detail
    } else {
        format!(
            "{} Numerical solver verdict, reported separately and unchanged: {}; {computed_status_detail}",
            mesh_qualification.summary(),
            numerical_convergence.as_str(),
        )
    };
    let status_detail = command_logs
        .get("__failure")
        .cloned()
        .unwrap_or(computed_status_detail);
    let surface_reference = surface::SurfaceReference {
        density_kg_m3: config.density_kg_m3,
        speed_m_s: config.effective_speed_m_s(),
        chord_m: config.chord_m,
        angle_of_attack_deg: config.angle_of_attack_deg,
        reference_area_m2: reference.area_m2,
        pressure_reference_pa: if execution_config.effective_simulation().compressible {
            execution_config.effective_static_pressure_pa()
        } else {
            config.boundaries.pressure_reference_pa
        },
        moment_reference_m: reference.moment_reference_m,
    };
    let (surface, surface_error) =
        match surface::parse_surface_case(&generated.path, "latest", &surface_reference) {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error.to_string())),
        };
    let (backend, openfoam_version, file_hashes) =
        provenance_from_logs(&generated.path, &command_logs);
    let mut public_command_logs = command_logs;
    // These entries are an internal hand-off between the runner and the
    // result builder.  Keep the user-facing log map limited to utility
    // output; the resolved values belong in the typed provenance record.
    public_command_logs.retain(|name, _| {
        name != "__backend"
            && name != "__openfoam_version"
            && !name.starts_with("__executable_path:")
    });
    CfdResults {
        outcome,
        case_dir: generated.path.clone(),
        provenance: StudyProvenance {
            template_version: TEMPLATE_VERSION.to_owned(),
            config: execution_config,
            airfoil: generated.airfoil.clone(),
            effective_speed_m_s: generated.effective_speed_m_s,
            effective_reynolds: generated.effective_reynolds,
            frame: FrameConvention::default(),
            reference: Some(reference),
            backend,
            openfoam_version,
            file_hashes,
        },
        residuals,
        forces,
        mass_balance,
        mesh_quality,
        mesh_qualification,
        numerical_convergence,
        field_updates,
        fields: collect_field_artifacts(&generated.path),
        surface,
        surface_error,
        command_logs: public_command_logs,
        status_detail,
    }
}

/// Resolve execution metadata and reproducibility hashes after a run.
///
/// The runner records the selected backend and each actual executable path in
/// private command-log entries.  Hashing is deliberately a small, stable
/// FNV-1a implementation so a case can be audited without pulling a crypto
/// dependency into the desktop application.  Case output files are included
/// when readable, while result/report/provenance/log files are excluded to
/// avoid a self-referential or timestamp-dependent hash.
fn provenance_from_logs(
    case_dir: &Path,
    command_logs: &BTreeMap<String, String>,
) -> (Option<String>, Option<String>, BTreeMap<String, String>) {
    let backend = command_logs
        .get("__backend")
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty() && *value != "unknown")
        .map(str::to_owned);
    let openfoam_version = command_logs
        .get("__openfoam_version")
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty() && *value != "unknown")
        .map(str::to_owned);

    let mut hashes = BTreeMap::new();
    collect_case_hashes(case_dir, case_dir, &mut hashes);
    for (key, value) in command_logs {
        let Some(tool) = key.strip_prefix("__executable_path:") else {
            continue;
        };
        let path = Path::new(value);
        if let Ok(bytes) = fs::read(path) {
            hashes.insert(format!("executable:{tool}"), fnv1a_hash(&bytes));
        }
    }
    (backend, openfoam_version, hashes)
}

fn collect_case_hashes(root: &Path, current: &Path, hashes: &mut BTreeMap<String, String>) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            // Logs are persisted separately and contain process-dependent
            // output ordering/timing; they must not make the case hash drift.
            if relative == "logs" || relative.starts_with("logs/") {
                continue;
            }
            collect_case_hashes(root, &path, hashes);
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if matches!(
            relative.as_str(),
            "study.json" | "results.json" | "report.md"
        ) || relative == "logs"
            || relative.starts_with("logs/")
        {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        hashes.insert(relative, fnv1a_hash(&bytes));
    }
}

fn fnv1a_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3_u64);
    }
    format!("fnv1a64:{hash:016x}")
}

fn logs_with_prefix(logs: &BTreeMap<String, String>, prefix: &str) -> String {
    let mut matching = logs
        .iter()
        .filter(|(name, _)| {
            !(name.as_str() == format!("{prefix}-postProcess"))
                && (name.as_str() == prefix || name.starts_with(&format!("{prefix}-")))
        })
        .collect::<Vec<_>>();
    matching.sort_by_key(|(name, _)| {
        if name.starts_with(&format!("{prefix}-startup")) {
            0_u8
        } else if name.as_str() == prefix {
            1
        } else if name.starts_with(&format!("{prefix}-final")) {
            2
        } else {
            3
        }
    });
    let mut combined = String::new();
    for (_, log) in matching {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(log);
    }
    combined
}

fn forces_for_final_stage(
    forces: &[ForceSample],
    command_logs: &BTreeMap<String, String>,
    solver_name: &str,
) -> Vec<ForceSample> {
    let Some(start_time) = command_logs
        .iter()
        .filter(|(name, _)| name.starts_with(&format!("{solver_name}-final")))
        .filter_map(|(_, log)| first_solver_time(log))
        .min_by(f64::total_cmp)
    else {
        return forces.to_vec();
    };
    // The final solver is restarted from the latest startup time.  Exclude
    // the restart marker itself and every startup sample so a short or
    // failed final stage cannot be declared stable by inherited history.
    let tolerance = 1.0e-9 * start_time.abs().max(1.0);
    forces
        .iter()
        .filter(|sample| sample.time > start_time + tolerance)
        .cloned()
        .collect()
}

fn first_solver_time(log: &str) -> Option<f64> {
    log.lines().find_map(|line| {
        let tail = line.trim_start().strip_prefix("Time =")?;
        tail.split_whitespace()
            .find_map(|token| token.parse::<f64>().ok())
    })
}

fn collect_field_artifacts(case_dir: &Path) -> Vec<FieldArtifact> {
    let mut fields = Vec::new();
    collect_artifacts_recursive(case_dir, case_dir, None, &mut fields);
    fields.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    fields
}

fn collect_artifacts_recursive(
    root: &Path,
    current: &Path,
    inherited_time: Option<f64>,
    fields: &mut Vec<FieldArtifact>,
) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if path.is_dir() {
            let time = name.parse::<f64>().ok().or(inherited_time);
            collect_artifacts_recursive(root, &path, time, fields);
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if inherited_time.is_none() && !relative_path.starts_with("postProcessing/") {
            continue;
        }
        let kind = if relative_path.starts_with("postProcessing/") {
            "OpenFOAM post-processing artifact"
        } else if inherited_time.is_some() {
            "OpenFOAM field"
        } else {
            "Case artifact"
        };
        fields.push(FieldArtifact {
            relative_path,
            name: name.to_owned(),
            time: inherited_time,
            kind: kind.to_owned(),
        });
    }
}

pub(crate) fn persisted_results(results: CfdResults) -> Result<CfdResults, String> {
    write_result_artifacts(&results)?;
    Ok(results)
}

pub(crate) fn persist_failed_results(
    config: &CfdStudyConfig,
    generated: &GeneratedCase,
    command_logs: BTreeMap<String, String>,
    mesh_output: String,
    process_status: OpenFoamProcessStatus,
) -> Result<CfdResults, String> {
    persisted_results(build_results(
        config,
        generated,
        command_logs,
        mesh_output,
        process_status,
    ))
}

pub(crate) fn persist_failed_results_with_quality(
    config: &CfdStudyConfig,
    generated: &GeneratedCase,
    command_logs: BTreeMap<String, String>,
    quality: MeshQuality,
    process_status: OpenFoamProcessStatus,
) -> Result<CfdResults, String> {
    persisted_results(build_results_with_quality(
        config,
        generated,
        command_logs,
        quality,
        process_status,
    ))
}

/// Write `results.json` and `report.md` for a parsed result.
///
/// Writes into [`CfdResults::case_dir`], so pointing that at a scratch
/// directory re-renders a stored result without touching the original case.
pub fn write_result_artifacts(results: &CfdResults) -> Result<(), String> {
    let study = serde_json::to_string_pretty(&results.provenance)
        .map_err(|error| format!("cannot encode final CFD provenance: {error}"))?;
    fs::write(results.case_dir.join("study.json"), study)
        .map_err(|error| format!("cannot write final CFD provenance: {error}"))?;
    let json = serde_json::to_string_pretty(results)
        .map_err(|error| format!("cannot encode CFD results: {error}"))?;
    fs::write(results.case_dir.join("results.json"), json)
        .map_err(|error| format!("cannot write CFD results: {error}"))?;
    let simulation = results.provenance.config.effective_simulation();
    let report = format!(
        "# ALAS OpenFOAM result\n\nOutcome: **{}**\n\nStatus: {}\n\nAirfoil: `{}`\nCoordinate hash: `{}`\nTemplate: `{}`\nBackend: `{}`\nOpenFOAM version: `{}`\nReproducibility hashes: `{}`\nSpeed: `{:.8} m/s`\nReynolds: `{:.8e}`\nChord: `{:.8} m`\nAngle of attack: `{:.6} deg`\nTemperature: `{:.8} K`\nDiagnostic Mach: `{:.8}`\nFlow regime: `{}`\nOpenFOAM solver: `{}`\nCompressible equations: `{}`\nStatic pressure used: `{:.8} Pa`\nAutomatic maximum iterations: `{}`\nAutomatic startup iterations: `{}`\nAutomatic pressure relaxation: `{:.3}`\nAutomatic momentum relaxation: `{:.3}`\nAutomatic turbulence relaxation: `{:.3}`\n\nMesh passed: `{}`\nCells: `{}`\nMax non-orthogonality: `{}`
Severely non-orthogonal faces (> 70 deg, checkMesh warning): `{}`
Mesh qualification (declared limits): `{}`
{}
Max skewness: `{}`\nMinimum cell volume: `{}`\nNative quality distributions: `{}`\nNear-wall y+: `{}`\nNear-wall distribution: `{}`\nResidual samples: `{}`\nForce samples: `{}`\nContinuity samples: `{}`\nField artifacts: `{}`\n\nThis report records numerical evidence from the generated case. It does not claim physical validation against experiment. Review the captured logs and the documented model limits in README.md before using coefficients.\n",
        results.outcome.as_str(),
        results.status_detail,
        results.provenance.airfoil.name,
        results.provenance.airfoil.coordinate_hash,
        results.provenance.template_version,
        results.provenance.backend.as_deref().unwrap_or("unknown"),
        results
            .provenance
            .openfoam_version
            .as_deref()
            .unwrap_or("unknown"),
        results.provenance.file_hashes.len(),
        results.provenance.effective_speed_m_s,
        results.provenance.effective_reynolds,
        results.provenance.config.chord_m,
        results.provenance.config.angle_of_attack_deg,
        results.provenance.config.freestream_temperature_k,
        results.provenance.config.mach_number(),
        simulation.regime.as_str(),
        simulation.solver.executable(),
        simulation.compressible,
        results.provenance.config.effective_static_pressure_pa(),
        simulation.max_iterations,
        simulation.startup_iterations,
        simulation.pressure_relaxation,
        simulation.equation_relaxation,
        simulation.turbulence_relaxation,
        results.mesh_quality.passed,
        results.mesh_quality
            .cells
            .map_or_else(|| "unknown".to_owned(), |value| value.to_string()),
        results.mesh_quality.max_non_orthogonality_deg.map_or_else(
            || "unknown".to_owned(),
            |value| format!("{value:.8}")
        ),
        results.mesh_quality.severely_non_orthogonal_faces.map_or_else(
            || "none reported".to_owned(),
            |value| value.to_string()
        ),
        results.mesh_qualification.standing.as_str(),
        mesh_qualification_table(&results.mesh_qualification),
        results.mesh_quality.max_skewness.map_or_else(
            || "unknown".to_owned(),
            |value| format!("{value:.8}")
        ),
        results.mesh_quality.min_volume_m3.map_or_else(
            || "unknown".to_owned(),
            |value| format!("{value:.8e}")
        ),
        results.mesh_quality.distributions.len(),
        results.mesh_quality.near_wall.as_ref().map_or_else(
            || "unavailable".to_owned(),
            |value| format!(
                "time {:.8}, min {:.8}, max {:.8}, average {:.8}, target {:.8}",
                value.time,
                value.min_y_plus,
                value.max_y_plus,
                value.average_y_plus,
                value.target_y_plus,
            ),
        ),
        results.mesh_quality.near_wall_distribution.as_ref().map_or_else(
            || "unavailable".to_owned(),
            |value| format!("{} ({} finite wall faces)", value.source, value.sample_count),
        ),
        results.residuals.len(),
        results.forces.len(),
        results.mass_balance.len(),
        results.fields.len(),
    );
    let report = format!("{report}\n{}", answer_section(results));
    fs::write(results.case_dir.join("report.md"), report)
        .map_err(|error| format!("cannot write CFD report: {error}"))
}

/// The part of the report a reader actually came for: the coefficients, and
/// how far each criterion was from its limit.
///
/// Without this the report recorded that the criteria "satisfy the configured
/// criteria" and never printed a single coefficient or residual, so a user
/// could not read the answer or check the margin without opening the raw logs.
/// Every declared mesh limit with the value measured against it.
///
/// Printed in full, including the checks that were **not measured**: a reader
/// must be able to see how much of the declared contract was actually tested,
/// not just that nothing failed.
fn mesh_qualification_table(qualification: &MeshQualification) -> String {
    let mut out = String::from(
        "\n| declared check | status | limit | measured | provenance |\n|---|---|---:|---:|---|\n",
    );
    for check in &qualification.checks {
        // A fixed-point format renders a legitimate 1.8e-13 minimum cell
        // volume as `0.000000`, which reads as the violation it is not.
        let number = |value: Option<f64>| {
            value.map_or_else(
                || "-".to_owned(),
                |value| {
                    if value != 0.0 && value.abs() < 1.0e-4 {
                        format!("{value:.6e}")
                    } else {
                        format!("{value:.6}")
                    }
                },
            )
        };
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            check.name,
            check.status.as_str(),
            number(check.limit),
            number(check.measured),
            check.provenance,
        ));
    }
    if let Some(faces) = qualification.severely_non_orthogonal_faces {
        out.push_str(&format!(
            "\nFaces past `checkMesh`'s severe non-orthogonality line: `{faces}`{}. That is prevalence, not influence: the location of those faces and their effect on the integrated loads were not measured.\n",
            qualification
                .severely_non_orthogonal_faces_per_cell
                .map_or_else(String::new, |ratio| format!(" (`{ratio:.3e}` faces per cell; NOT a fraction of mesh faces)")),
        ));
    }
    out.push_str(&format!("\n{}\n", qualification.summary()));
    out
}

fn answer_section(results: &CfdResults) -> String {
    let mut out = String::from("\n## Coefficients\n\n");
    let converged = results.outcome == CfdOutcome::NumericallyConverged;
    match results.forces.last() {
        Some(force) => {
            if !converged {
                out.push_str(
                    "**PROVISIONAL: this result did not satisfy the convergence criteria.** \
                     The numbers below are the last finite sample the solver produced and are \
                     not a qualified answer.\n\n",
                );
            }
            out.push_str(&format!(
                "Reference: chord `{:.8} m`, quarter-chord moment reference, `Cm` positive nose-up.\n\n\
                 | quantity | value |\n|---|---:|\n\
                 | `Cl` | `{:.7}` |\n| `Cd` | `{:.7}` |\n| `Cm,c/4` | `{:.7}` |\n",
                results.provenance.config.chord_m,
                force.cl,
                force.cd,
                force.cm,
            ));
            for (label, value) in [
                ("`Cd` pressure", force.cd_pressure),
                ("`Cd` viscous", force.cd_viscous),
                ("`Cl` pressure", force.cl_pressure),
                ("`Cl` viscous", force.cl_viscous),
            ] {
                if let Some(value) = value {
                    out.push_str(&format!("| {label} | `{value:.7}` |\n"));
                }
            }
            out.push_str(&format!(
                "| sampled at outer iteration | `{}` |\n",
                force.time
            ));
        }
        None => out.push_str("No force sample was parsed, so no coefficient is reported.\n"),
    }

    out.push_str("\n## Criteria, with the measured margin\n\n");
    let tolerance = results.provenance.config.solver.residual_tolerance;
    let latest = results
        .residuals
        .iter()
        .map(|sample| sample.iteration)
        .max()
        .unwrap_or_default();
    let mut per_equation: BTreeMap<String, f64> = BTreeMap::new();
    for sample in results
        .residuals
        .iter()
        .filter(|sample| sample.iteration == latest)
    {
        per_equation
            .entry(sample.field.to_ascii_lowercase())
            .and_modify(|value| *value = value.max(sample.initial))
            .or_insert(sample.initial);
    }
    if per_equation.is_empty() {
        out.push_str("No residual history was parsed.\n");
    } else {
        out.push_str(&format!(
            "Outer SIMPLE iteration `{latest}`, initial residual per primary equation, \
             against the `{tolerance:.3e}` tolerance:\n\n\
             | equation | residual | multiple of tolerance |\n|---|---:|---:|\n"
        ));
        for (field, value) in &per_equation {
            out.push_str(&format!(
                "| `{field}` | `{value:.4e}` | `{:.2}x` |\n",
                value / tolerance
            ));
        }
    }
    let mass_tolerance = results.provenance.config.solver.mass_balance_tolerance;
    match results
        .mass_balance
        .iter()
        .rev()
        .find(|row| row.sum_local.is_some() || row.global.is_some())
    {
        Some(row) => out.push_str(&format!(
            "\nMass balance against `{mass_tolerance:.3e}`: `sum local` `{}`, `global` `{}`. \
             The `cumulative` entry is a running total over the whole run and is audit data, \
             not a criterion.\n",
            row.sum_local
                .map_or_else(|| "unavailable".to_owned(), |value| format!("{value:.4e}")),
            row.global
                .map_or_else(|| "unavailable".to_owned(), |value| format!("{value:.4e}")),
        )),
        None => out.push_str("\nNo mass-balance diagnostic was parsed.\n"),
    }
    let plausibility = assess_physical_plausibility(&results.forces, &results.mesh_quality);
    out.push_str(&format!(
        "\nPhysical-plausibility screen: {}\n",
        plausibility.detail()
    ));
    out
}
