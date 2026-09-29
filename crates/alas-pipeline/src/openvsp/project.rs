// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Running the exported script through an installed OpenVSP and verifying its products.

use super::*;

/// Execute an exported script through OpenVSP's headless `vspscript` runner.
///
/// Success requires all three independent pieces of evidence: a zero process
/// exit code, the script's completion marker, and a non-empty native OpenVSP
/// XML document. A stale `.vsp3` is removed before launch so it cannot turn a
/// failed run into apparent success.
pub fn materialize_openvsp_project(
    mut export: OpenVspExportResult,
    executable: &Path,
    timeout_seconds: f64,
) -> OpenVspExportResult {
    export.runtime_executable = Some(executable.to_path_buf());
    export.preview_available = false;
    export.preview_error = None;
    export.cad_preview_geometry_available = false;
    export.cad_preview_geometry_error = None;
    let timeout = match timeout_from_seconds("OpenVSP", timeout_seconds) {
        Ok(timeout) => timeout,
        Err(error) => {
            export.status = OpenVspExportStatus::InvalidTimeout;
            export.runtime_error = Some(error.to_string());
            return export;
        }
    };
    let Some(work_dir) = export.script_path.parent().map(Path::to_path_buf) else {
        return reject_runtime(export, "OpenVSP script has no working directory".to_owned());
    };
    let Some(script_name) = export.script_path.file_name().map(ToOwned::to_owned) else {
        return reject_runtime(export, "OpenVSP script has no file name".to_owned());
    };
    if !export.script_path.is_file() {
        let script_path = export.script_path.display().to_string();
        return reject_runtime(
            export,
            format!("OpenVSP script does not exist: {script_path}"),
        );
    }
    let expected_vsp3_path = export.script_path.with_extension("vsp3");
    if export.vsp3_path != expected_vsp3_path {
        let expected_vsp3 = expected_vsp3_path.display().to_string();
        let actual_vsp3 = export.vsp3_path.display().to_string();
        return reject_runtime(
            export,
            format!("OpenVSP project path must be the script sibling {expected_vsp3}; got {actual_vsp3}"),
        );
    }
    let expected_vspaero_geometry_path = export.script_path.with_extension("vspgeom");
    if export.vspaero_geometry_path != expected_vspaero_geometry_path {
        let expected_vspaero_geometry = expected_vspaero_geometry_path.display().to_string();
        let actual_vspaero_geometry = export.vspaero_geometry_path.display().to_string();
        return reject_runtime(
            export,
            format!("VSPAERO geometry path must be the script sibling {expected_vspaero_geometry}; got {actual_vspaero_geometry}"),
        );
    }
    let expected_preview_path = export.script_path.with_extension("preview.png");
    if export.preview_path != expected_preview_path {
        let expected_preview = expected_preview_path.display().to_string();
        let actual_preview = export.preview_path.display().to_string();
        return reject_runtime(
            export,
            format!("OpenVSP preview path must be the script sibling {expected_preview}; got {actual_preview}"),
        );
    }
    let expected_cad_preview_vsp3_path = export.script_path.with_extension("cad_preview.vsp3");
    if export.cad_preview_vsp3_path != expected_cad_preview_vsp3_path {
        let expected_cad_preview_vsp3 = expected_cad_preview_vsp3_path.display().to_string();
        let actual_cad_preview_vsp3 = export.cad_preview_vsp3_path.display().to_string();
        return reject_runtime(
            export,
            format!("OpenVSP CAD preview project path must be the script sibling {expected_cad_preview_vsp3}; got {actual_cad_preview_vsp3}"),
        );
    }
    let expected_cad_preview_geometry_path =
        export.script_path.with_extension("cad_preview.vspgeom");
    if export.cad_preview_geometry_path != expected_cad_preview_geometry_path {
        let expected_cad_preview_geometry =
            expected_cad_preview_geometry_path.display().to_string();
        let actual_cad_preview_geometry = export.cad_preview_geometry_path.display().to_string();
        return reject_runtime(
            export,
            format!("OpenVSP CAD preview geometry path must be the script sibling {expected_cad_preview_geometry}; got {actual_cad_preview_geometry}"),
        );
    }
    if export.vsp3_path.exists() {
        if let Err(error) = fs::remove_file(&export.vsp3_path) {
            let detail = format!(
                "cannot remove stale {}: {error}",
                export.vsp3_path.display()
            );
            return reject_runtime(export, detail);
        }
    }
    if export.vspaero_geometry_path.exists() {
        if let Err(error) = fs::remove_file(&export.vspaero_geometry_path) {
            let detail = format!(
                "cannot remove stale {}: {error}",
                export.vspaero_geometry_path.display()
            );
            return reject_runtime(export, detail);
        }
    }
    if export.preview_path.exists() {
        if let Err(error) = fs::remove_file(&export.preview_path) {
            let detail = format!(
                "cannot remove stale {}: {error}",
                export.preview_path.display()
            );
            return reject_runtime(export, detail);
        }
    }
    if export.cad_preview_vsp3_path.exists() {
        if let Err(error) = fs::remove_file(&export.cad_preview_vsp3_path) {
            let detail = format!(
                "cannot remove stale {}: {error}",
                export.cad_preview_vsp3_path.display()
            );
            return reject_runtime(export, detail);
        }
    }
    if export.cad_preview_geometry_path.exists() {
        if let Err(error) = fs::remove_file(&export.cad_preview_geometry_path) {
            let detail = format!(
                "cannot remove stale {}: {error}",
                export.cad_preview_geometry_path.display()
            );
            return reject_runtime(export, detail);
        }
    }

    let stdout_path = export.script_path.with_extension("openvsp.stdout.txt");
    let stderr_path = export.script_path.with_extension("openvsp.stderr.txt");
    export.runtime_stdout_path = Some(stdout_path.clone());
    export.runtime_stderr_path = Some(stderr_path.clone());
    let stdout_file = match File::create(&stdout_path) {
        Ok(file) => file,
        Err(error) => {
            return reject_runtime(
                export,
                format!("cannot create {}: {error}", stdout_path.display()),
            )
        }
    };
    let stderr_file = match File::create(&stderr_path) {
        Ok(file) => file,
        Err(error) => {
            return reject_runtime(
                export,
                format!("cannot create {}: {error}", stderr_path.display()),
            )
        }
    };

    let run_started = SystemTime::now();
    let mut command = Command::new(executable);
    command
        .args(["-script"])
        .arg(script_name)
        .current_dir(&work_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .no_window()
        .new_process_group();
    let mut child = match alas_exec::SupervisedSpawn::spawn_supervised(&mut command, "OpenVSP") {
        Ok(child) => child,
        Err(error) => {
            export.status = OpenVspExportStatus::RuntimeLaunchFailed;
            export.runtime_error = Some(format!(
                "failed to launch {}: {error}",
                executable.display()
            ));
            return export;
        }
    };
    let status = match wait_with_timeout(&mut child, timeout) {
        DeadlineWait::Exited(status) => status,
        DeadlineWait::TimedOut => {
            export.status = OpenVspExportStatus::RuntimeTimedOut;
            export.runtime_error = Some(format!(
                "OpenVSP exceeded the {timeout_seconds:.1} s deadline; process tree force-killed"
            ));
            return export;
        }
        DeadlineWait::PollFailed(error) => {
            return reject_runtime(
                export,
                format!("cannot poll OpenVSP: {error}; process tree force-killed"),
            );
        }
    };

    let stdout = fs::read_to_string(&stdout_path).unwrap_or_default();
    let stderr = fs::read_to_string(&stderr_path).unwrap_or_default();
    if !status.success() {
        return reject_runtime(
            export,
            format!(
                "OpenVSP exited with {}; stdout tail: {}; stderr tail: {}",
                status
                    .code()
                    .map_or_else(|| "unknown code".to_owned(), |code| code.to_string()),
                text_tail(&stdout),
                text_tail(&stderr)
            ),
        );
    }
    if !stdout.contains("ALAS_OPENVSP_EXPORT_COMPLETE") {
        return reject_runtime(
            export,
            format!(
                "OpenVSP returned success without the ALAS completion marker; stdout tail: {}; stderr tail: {}",
                text_tail(&stdout),
                text_tail(&stderr)
            ),
        );
    }
    if !is_fresh_native_vsp3(&export.vsp3_path, run_started) {
        let detail = format!(
            "OpenVSP reported completion but {} is absent, stale, or not a native VSP3 XML document",
            export.vsp3_path.display()
        );
        return reject_runtime(export, detail);
    }
    if !is_fresh_native_vspgeom(&export.vspaero_geometry_path, run_started) {
        let detail = format!(
            "OpenVSP reported completion but {} is absent, stale, or not a native VSP geometry mesh",
            export.vspaero_geometry_path.display()
        );
        return reject_runtime(export, detail);
    }
    export.status = OpenVspExportStatus::Vsp3Materialized;
    export.runtime_error = None;
    if is_fresh_native_png(&export.preview_path, run_started) {
        export.preview_available = true;
    } else {
        export.preview_error = Some(preview_failure_reason(&stdout, &export.preview_path));
    }
    // The full outer-mold-line mesh is a best-effort CAD-preview artifact
    // produced by a second, independent VSPAERO geometry call (see
    // render_script). Its absence never revokes the solver-facing
    // Vsp3Materialized status determined above from the thin-surface mesh.
    if is_fresh_native_vspgeom(&export.cad_preview_geometry_path, run_started) {
        export.cad_preview_geometry_available = true;
    } else {
        export.cad_preview_geometry_error = Some(format!(
            "OpenVSP completed the solver-facing export, but no fresh full outer-mold-line preview mesh was written to {}",
            export.cad_preview_geometry_path.display()
        ));
    }
    if !export.preview_available {
        match native_preview::capture(&export, executable) {
            Ok(()) => {
                export.preview_available = true;
                export.preview_error = None;
            }
            Err(error) => export.preview_error = Some(error),
        }
    }
    export
}

fn reject_runtime(mut export: OpenVspExportResult, error: String) -> OpenVspExportResult {
    export.status = OpenVspExportStatus::RuntimeRejected;
    export.runtime_error = Some(error);
    export
}
