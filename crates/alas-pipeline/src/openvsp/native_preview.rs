// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Optional graphics process. Its failure must never reject a valid solver export.
use super::*;

/// Wall-clock bound on one native screenshot: far above a healthy capture,
/// short enough that a hung renderer does not hold the export open.
const PREVIEW_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) fn capture(export: &OpenVspExportResult, runner: &Path) -> Result<(), String> {
    let python = std::env::var_os("ALAS_OPENVSP_PREVIEW_PYTHON")
        .map(PathBuf::from)
        .or_else(|| {
            runner
                .parent()
                .map(|p| p.join("preview-runtime/python.exe"))
        })
        .filter(|p| p.is_file())
        .ok_or_else(|| {
            "Native preview runtime unavailable; run tools/setup_openvsp_preview.ps1".to_owned()
        })?;
    let python = std::path::absolute(python).map_err(|e| e.to_string())?;
    let model = std::path::absolute(&export.cad_preview_vsp3_path).map_err(|e| e.to_string())?;
    if !is_native_vsp3(&model) {
        return Err("No valid full-aircraft CAD model is available for native capture".into());
    }
    let output = std::path::absolute(&export.preview_path).map_err(|e| e.to_string())?;
    let script = export.script_path.with_extension("capture.py");
    fs::write(&script, include_str!("native_preview.py")).map_err(|e| e.to_string())?;
    let script = std::path::absolute(script).map_err(|e| e.to_string())?;
    let stdout_path = export.script_path.with_extension("preview.stdout.txt");
    let stderr_path = export.script_path.with_extension("preview.stderr.txt");
    let mut command = Command::new(python);
    command
        .arg(script)
        .arg(model)
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            File::create(&stdout_path).map_err(|e| e.to_string())?,
        ))
        .stderr(Stdio::from(
            File::create(&stderr_path).map_err(|e| e.to_string())?,
        ))
        .no_window()
        .new_process_group();
    let started = SystemTime::now();
    let mut child =
        alas_exec::SupervisedSpawn::spawn_supervised(&mut command, "OpenVSP native preview")
            .map_err(|e| e.to_string())?;
    let status = match wait_with_timeout(&mut child, PREVIEW_TIMEOUT) {
        DeadlineWait::Exited(status) => status,
        DeadlineWait::TimedOut => {
            return Err("Native preview stopped after the 30 s timeout".to_owned());
        }
        DeadlineWait::PollFailed(error) => {
            return Err(format!(
                "Native preview stopped after a process error: {error}"
            ));
        }
    };
    let stdout = fs::read_to_string(stdout_path).unwrap_or_default();
    if status.success()
        && stdout.contains("ALAS_NATIVE_CAPTURE_COMPLETE")
        && is_fresh_native_png(&export.preview_path, started)
    {
        Ok(())
    } else {
        let stderr = fs::read_to_string(stderr_path).unwrap_or_default();
        Err(format!(
            "Native screenshot failed ({status}): {}",
            text_tail(&stderr)
        ))
    }
}
