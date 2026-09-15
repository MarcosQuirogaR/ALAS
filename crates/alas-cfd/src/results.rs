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
                selected_wall_distance_m: config.mesh.first_layer_height_m,
                estimated_y_plus: sizing.map(|value| value.estimated_y_plus),
                source: summary.source,
            }
        });
    mesh_quality.near_wall_distribution =
        result_io::read_y_plus_distribution(&generated.path, "airfoil");
    let solution_log = logs_with_prefix(&command_logs, "simpleFoam");
    let mut residuals = parse_residuals(&solution_log);
    let post_log = logs_with_prefix(&command_logs, "simpleFoam-postProcess");
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
    let final_stage_forces = forces_for_final_stage(&forces, &command_logs);
    let (outcome, computed_status_detail) = classify_convergence(
        config,
        process_status,
        &mesh_quality,
        &residuals,
        &final_stage_forces,
        &mass_balance,
    );
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
        pressure_reference_pa: config.boundaries.pressure_reference_pa,
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
            config: config.clone(),
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
            !(prefix == "simpleFoam" && name.as_str() == "simpleFoam-postProcess")
                && (name.as_str() == prefix || name.starts_with(&format!("{prefix}-")))
        })
        .collect::<Vec<_>>();
    if prefix == "simpleFoam" {
        matching.sort_by_key(|(name, _)| {
            if name.starts_with("simpleFoam-startup") {
                0_u8
            } else if name.as_str() == "simpleFoam" {
                1
            } else if name.starts_with("simpleFoam-final") {
                2
            } else {
                3
            }
        });
    }
    let mut combined = String::new();
    for (_, log) in matching {
        if !combined.is_empty() {
            combined.push_str("\n");
        }
        combined.push_str(log);
    }
    combined
}

fn forces_for_final_stage(
    forces: &[ForceSample],
    command_logs: &BTreeMap<String, String>,
) -> Vec<ForceSample> {
    let Some(start_time) = command_logs
        .iter()
        .filter(|(name, _)| name.starts_with("simpleFoam-final"))
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

pub(crate) fn write_result_artifacts(results: &CfdResults) -> Result<(), String> {
    let study = serde_json::to_string_pretty(&results.provenance)
        .map_err(|error| format!("cannot encode final CFD provenance: {error}"))?;
    fs::write(results.case_dir.join("study.json"), study)
        .map_err(|error| format!("cannot write final CFD provenance: {error}"))?;
    let json = serde_json::to_string_pretty(results)
        .map_err(|error| format!("cannot encode CFD results: {error}"))?;
    fs::write(results.case_dir.join("results.json"), json)
        .map_err(|error| format!("cannot write CFD results: {error}"))?;
    let report = format!(
        "# ALAS OpenFOAM result\n\nOutcome: **{}**\n\nStatus: {}\n\nAirfoil: `{}`\nCoordinate hash: `{}`\nTemplate: `{}`\nBackend: `{}`\nOpenFOAM version: `{}`\nReproducibility hashes: `{}`\nSpeed: `{:.8} m/s`\nReynolds: `{:.8e}`\nChord: `{:.8} m`\nAngle of attack: `{:.6} deg`\nTemperature: `{:.8} K`\nDiagnostic Mach: `{:.8}`\n\nMesh passed: `{}`\nCells: `{}`\nMax non-orthogonality: `{}`\nMax skewness: `{}`\nMinimum cell volume: `{}`\nNative quality distributions: `{}`\nNear-wall y+: `{}`\nNear-wall distribution: `{}`\nResidual samples: `{}`\nForce samples: `{}`\nContinuity samples: `{}`\nField artifacts: `{}`\n\nThis report records numerical evidence from the generated case. It does not claim physical validation against experiment. Review the captured logs and the documented model limits in README.md before using coefficients.\n",
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
        results.mesh_quality.passed,
        results.mesh_quality
            .cells
            .map_or_else(|| "unknown".to_owned(), |value| value.to_string()),
        results.mesh_quality.max_non_orthogonality_deg.map_or_else(
            || "unknown".to_owned(),
            |value| format!("{value:.8}")
        ),
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
    fs::write(results.case_dir.join("report.md"), report)
        .map_err(|error| format!("cannot write CFD report: {error}"))
}
