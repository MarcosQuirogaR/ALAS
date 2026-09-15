// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::fmt::Write as FmtWrite;
use std::fs;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use alas_config::AlasConfig;
use alas_exec::process::{kill_process_tree, NewProcessGroup, NoConsoleWindow};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::wing::Wing;
use alas_perf::landing_gear::{size_landing_gear_with_group_stations, LandingGearLayout};

use crate::full_analysis::AnalysisReport;

#[path = "../openvsp/validation.rs"]
mod validation;
use validation::validate_script;

/// Evidence state of an OpenVSP export artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenVspExportStatus {
    /// The supported input script was written, but no installed runtime read it.
    ScriptWrittenRuntimeUnverified,
    /// The configured executable could not be launched.
    RuntimeLaunchFailed,
    /// The OpenVSP process exceeded its deadline and its process tree was killed.
    RuntimeTimedOut,
    /// OpenVSP ran, but did not satisfy the completion and native-file checks.
    RuntimeRejected,
    /// OpenVSP read the script and wrote the requested project file.
    Vsp3Materialized,
}

impl OpenVspExportStatus {
    /// Stable status text for the GUI and retained run evidence.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ScriptWrittenRuntimeUnverified => "script_written_runtime_unverified",
            Self::RuntimeLaunchFailed => "runtime_launch_failed",
            Self::RuntimeTimedOut => "runtime_timed_out",
            Self::RuntimeRejected => "runtime_rejected",
            Self::Vsp3Materialized => "vsp3_materialized",
        }
    }
}

/// Files and fidelity notes produced by an OpenVSP geometry export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenVspExportResult {
    /// Script accepted by OpenVSP's AngelScript runner.
    pub script_path: PathBuf,
    /// Project path the script passes to OpenVSP's `WriteVSPFile` API.
    pub vsp3_path: PathBuf,
    /// Thin lifting-surface mesh written by `VSPAEROComputeGeometry` for the
    /// solver's `ThinGeomSet`. This is the exact geometry an installed
    /// VSPAERO run solves; it intentionally excludes the fuselage and
    /// landing-gear bodies, which are not thin lifting surfaces.
    pub vspaero_geometry_path: PathBuf,
    /// Full outer-mold-line project written a second time for the CAD
    /// preview only (`SET_ALL`, distinct file). Never read by the solver.
    pub cad_preview_vsp3_path: PathBuf,
    /// Full outer-mold-line mesh (thin wings plus thick fuselage and
    /// landing-gear bodies) written by a second, preview-only
    /// `VSPAEROComputeGeometry` call. This is CAD display evidence, not
    /// solver input: it is never passed to the native VSPAERO executable.
    pub cad_preview_geometry_path: PathBuf,
    /// True only when the current runtime produced a fresh, valid full
    /// outer-mold-line mesh at `cad_preview_geometry_path`.
    pub cad_preview_geometry_available: bool,
    /// Explanation when the full outer-mold-line preview mesh was not produced.
    pub cad_preview_geometry_error: Option<String>,
    /// Expected native CAD screenshot path produced by OpenVSP's `ScreenGrab`.
    pub preview_path: PathBuf,
    /// True only when the current runtime produced a fresh, structurally valid PNG.
    pub preview_available: bool,
    /// Explanation when the native CAD screenshot was not produced.
    pub preview_error: Option<String>,
    /// First-hand runtime validation state.
    pub status: OpenVspExportStatus,
    /// Executable used for first-hand materialization, when one was attempted.
    pub runtime_executable: Option<PathBuf>,
    /// Actionable reason for an attempted runtime failure.
    pub runtime_error: Option<String>,
    /// Captured standard-output log retained beside the script.
    pub runtime_stdout_path: Option<PathBuf>,
    /// Captured standard-error log retained beside the script.
    pub runtime_stderr_path: Option<PathBuf>,
    /// Number of airframe bodies and lifting surfaces written.
    pub component_count: usize,
    /// Number of sized landing-gear wheels written as fuselage approximations.
    pub wheel_count: usize,
    /// Intentional geometric approximations visible to downstream users.
    pub approximations: Vec<String>,
    /// Model detail that the current physics report does not supply.
    pub unsupported: Vec<String>,
}

/// Write a supported OpenVSP input script for `report`.
///
/// Running this artifact from OpenVSP's script interface creates the returned
/// `.vsp3` path. The status remains runtime-unverified until the script is
/// executed and the resulting project is opened by OpenVSP.
pub fn export_openvsp_script(
    report: &AnalysisReport,
    config: &AlasConfig,
    script_path: &Path,
) -> io::Result<OpenVspExportResult> {
    if let Some(parent) = script_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let vsp3_path = script_path.with_extension("vsp3");
    let vspaero_geometry_path = script_path.with_extension("vspgeom");
    let vsp3_name = vsp3_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("optimized_aircraft.vsp3");
    let cad_preview_vsp3_path = script_path.with_extension("cad_preview.vsp3");
    let cad_preview_geometry_path = script_path.with_extension("cad_preview.vspgeom");
    let cad_preview_vsp3_name = cad_preview_vsp3_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("optimized_aircraft.cad_preview.vsp3");
    let preview_file = script_path.with_extension("preview.png");
    let preview_name = preview_file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("optimized_aircraft.preview.png");
    let gear = landing_gear_for_report(report, config);
    let script = render_script(
        &report.airplane,
        Some(&gear),
        vsp3_name,
        cad_preview_vsp3_name,
        preview_name,
    );
    validate_script(&script).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(script_path, script)?;

    Ok(OpenVspExportResult {
        script_path: script_path.to_path_buf(),
        vsp3_path,
        vspaero_geometry_path,
        cad_preview_vsp3_path,
        cad_preview_geometry_path,
        cad_preview_geometry_available: false,
        cad_preview_geometry_error: None,
        preview_path: preview_file,
        preview_available: false,
        preview_error: None,
        status: OpenVspExportStatus::ScriptWrittenRuntimeUnverified,
        runtime_executable: None,
        runtime_error: None,
        runtime_stdout_path: None,
        runtime_stderr_path: None,
        component_count: report.airplane.wings.len() + report.airplane.fuselages.len(),
        wheel_count: gear.wheels.len(),
        approximations: vec![
            "OpenVSP controls the loft interpolation between ALAS cross-section stations."
                .to_owned(),
            "Landing-gear tires are exported as flattened ellipsoidal fuselages at the sized wheel locations."
                .to_owned(),
        ],
        unsupported: vec![
            "Landing-gear struts, trucks, doors, and retraction kinematics are not present in the physics report."
                .to_owned(),
            "Cabin furnishings and internal structural members are not outer-mold-line geometry."
                .to_owned(),
        ],
    })
}

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
        let expected_cad_preview_geometry = expected_cad_preview_geometry_path.display().to_string();
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
    let deadline = Instant::now() + Duration::from_secs_f64(timeout_seconds.max(0.1));
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                kill_process_tree(child.id());
                let _ = child.wait();
                export.status = OpenVspExportStatus::RuntimeTimedOut;
                export.runtime_error = Some(format!(
                    "OpenVSP exceeded the {timeout_seconds:.1} s deadline; process tree force-killed"
                ));
                return export;
            }
            Err(error) => {
                return reject_runtime(export, format!("cannot poll OpenVSP: {error}"));
            }
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
    export
}

fn reject_runtime(mut export: OpenVspExportResult, error: String) -> OpenVspExportResult {
    export.status = OpenVspExportStatus::RuntimeRejected;
    export.runtime_error = Some(error);
    export
}

fn is_native_vsp3(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    bytes.len() > 100
        && String::from_utf8_lossy(&bytes[..bytes.len().min(8192)]).contains("<Vsp_Geometry>")
}

fn is_fresh_native_vsp3(path: &Path, run_started: SystemTime) -> bool {
    is_fresh_file(path, run_started) && is_native_vsp3(path)
}

fn is_native_vspgeom(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let header = String::from_utf8_lossy(&bytes[..bytes.len().min(64)]);
    bytes.len() > 32
        && header
            .lines()
            .next()
            .is_some_and(|line| line.trim() == "# vspgeom v3")
}

fn is_fresh_native_vspgeom(path: &Path, run_started: SystemTime) -> bool {
    is_fresh_file(path, run_started) && is_native_vspgeom(path)
}

fn is_fresh_native_png(path: &Path, run_started: SystemTime) -> bool {
    if !is_fresh_file(path, run_started) {
        return false;
    }
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    if bytes.len() < 33 || bytes[..8] != *b"\x89PNG\r\n\x1a\n" {
        return false;
    }
    let mut offset: usize = 8;
    let mut saw_ihdr = false;
    while offset.checked_add(12).is_some_and(|end| end <= bytes.len()) {
        let chunk_length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let data_start = offset + 8;
        let Some(data_end) = data_start.checked_add(chunk_length) else {
            return false;
        };
        let Some(chunk_end) = data_end.checked_add(4) else {
            return false;
        };
        if chunk_end > bytes.len() {
            return false;
        }
        let chunk_type = &bytes[offset + 4..offset + 8];
        if !saw_ihdr {
            if chunk_type != b"IHDR" || chunk_length != 13 {
                return false;
            }
            let width = u32::from_be_bytes([
                bytes[data_start],
                bytes[data_start + 1],
                bytes[data_start + 2],
                bytes[data_start + 3],
            ]);
            let height = u32::from_be_bytes([
                bytes[data_start + 4],
                bytes[data_start + 5],
                bytes[data_start + 6],
                bytes[data_start + 7],
            ]);
            if width == 0 || height == 0 {
                return false;
            }
            saw_ihdr = true;
        }
        if chunk_type == b"IEND" {
            return saw_ihdr && chunk_length == 0 && chunk_end == bytes.len();
        }
        offset = chunk_end;
    }
    false
}

fn is_fresh_file(path: &Path, run_started: SystemTime) -> bool {
    let Ok(modified) = fs::metadata(path).and_then(|metadata| metadata.modified()) else {
        return false;
    };
    let earliest_allowed = run_started
        .checked_sub(Duration::from_secs(2))
        .unwrap_or(run_started);
    modified >= earliest_allowed
}

fn preview_failure_reason(stdout: &str, path: &Path) -> String {
    if stdout.contains("ALAS_OPENVSP_PREVIEW_UNAVAILABLE") {
        return format!(
            "OpenVSP completed the native project, but its runtime has no graphics-capable GUI build; no CAD preview was written to {}",
            path.display()
        );
    }
    format!(
        "OpenVSP completed the native project, but no fresh valid PNG preview was written to {}",
        path.display()
    )
}

fn text_tail(text: &str) -> String {
    text.lines()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" | ")
}

fn landing_gear_for_report(report: &AnalysisReport, config: &AlasConfig) -> LandingGearLayout {
    let main_wing = &report.airplane.wings[0];
    let mac = report.airplane.c_ref.max(0.001);
    let x_mac_le = main_wing.aerodynamic_center(0.25)[0] - 0.25 * mac;
    let aero_aft_x = report.x_neutral_point - config.requirements.target_static_margin * mac;
    let aero_fwd_x = aero_aft_x - config.requirements.cg_range_pct_mac / 100.0 * mac;

    let main_fuselage = &report.airplane.fuselages[0];
    let fus_start = main_fuselage
        .xsecs
        .first()
        .map_or(0.0, |section| section.xyz_c[0]);
    let fus_end = main_fuselage
        .xsecs
        .last()
        .map_or(fus_start, |section| section.xyz_c[0]);
    let fallback_x_nlg = fus_start + (fus_end - fus_start) * config.mass_model.nlg_x_fraction;
    let fallback_x_mlg = x_mac_le + config.mass_model.mlg_x_fraction_mac * mac;
    let stations = config.landing_gear.resolved_station_positions(
        fallback_x_nlg,
        fallback_x_mlg,
        fus_start,
        fus_end - fus_start,
    );
    let mass_kg = report.component_masses.values().copied().sum();
    let diameter_m = config.geometry.fuselage.diameter_m;

    size_landing_gear_with_group_stations(
        mass_kg,
        stations.x_nlg_m,
        stations.x_mlg_m,
        aero_fwd_x,
        aero_aft_x,
        diameter_m,
        diameter_m * 1.1,
        &stations.main_gear_x_m,
        &config.landing_gear,
    )
}

fn render_script(
    airplane: &Airplane,
    landing_gear: Option<&LandingGearLayout>,
    vsp3_name: &str,
    cad_preview_vsp3_name: &str,
    preview_name: &str,
) -> String {
    let mut script = String::new();
    script.push_str("// Generated by ALAS from computed geometry.\n");
    script.push_str(
        "// OpenVSP API reference: AddGeom, SetParmVal, SetAirfoilPnts, WriteVSPFile.\n\n",
    );
    script.push_str("int main()\n{\n    ClearVSPModel();\n\n");

    for (index, fuselage) in airplane.fuselages.iter().enumerate() {
        emit_fuselage(&mut script, index, fuselage);
    }
    for (index, wing) in airplane.wings.iter().enumerate() {
        emit_wing(&mut script, index, wing);
    }
    if let Some(gear) = landing_gear {
        emit_landing_gear(&mut script, airplane, gear);
    }

    script.push_str(
        "    Update();\n\
         // Keep the native CAD preview as a best-effort artifact.  The\n\
         // model/error contract below remains authoritative for runtime\n\
         // acceptance, so a graphics-context limitation cannot turn a valid\n\
         // VSP3 export into a false solver failure.\n\
         FitAllViews();\n\
         SetViewAxis( false );\n\
         SetShowBorders( false );\n",
    );
    let _ = writeln!(
        script,
        "    WriteVSPFile(\"{}\", SET_ALL);",
        script_string(vsp3_name)
    );
    script.push_str(
        "    string alas_vspaero_geometry_analysis = \"VSPAEROComputeGeometry\";\n\
         \x20   SetAnalysisInputDefaults( alas_vspaero_geometry_analysis );\n\
         \x20   SetIntAnalysisInput( alas_vspaero_geometry_analysis, \"GeomSet\", { SET_NONE }, 0 );\n\
         \x20   SetIntAnalysisInput( alas_vspaero_geometry_analysis, \"ThinGeomSet\", { 3 }, 0 );\n\
         \x20   string alas_vspaero_geometry_result = ExecAnalysis( alas_vspaero_geometry_analysis );\n\
         \x20   if ( alas_vspaero_geometry_result.length() > 0 )\n\
         \x20   {\n\
         \x20       Print( string( \"ALAS_VSPAERO_GEOMETRY_COMPLETE\\n\" ) );\n\
         \x20   }\n",
    );
    script.push_str(
        "    int alas_model_error_count = GetNumTotalErrors();\n\
         \x20   while ( GetNumTotalErrors() > 0 )\n\
         \x20   {\n\
         \x20       ErrorObj err = PopLastError();\n\
         \x20       Print( err.GetErrorString() );\n\
         \x20   }\n\
         \x20   if ( alas_model_error_count == 0 )\n\
         \x20   {\n",
    );
    script.push_str(
        "         \x20   // The thin-surface mesh above is exactly what the native\n\
         \x20   // VSPAERO solver reads and is never modified for display\n\
         \x20   // purposes. This second, independent export adds the fuselage\n\
         \x20   // and landing-gear bodies as thick VSPAERO panels so the\n\
         \x20   // report/GUI CAD preview can show the whole outer mold line.\n\
         \x20   // A failure here is cosmetic and must not affect the solver\n\
         \x20   // export accepted above.\n",
    );
    let _ = writeln!(
        script,
        "    \x20   WriteVSPFile(\"{}\", SET_ALL);",
        script_string(cad_preview_vsp3_name)
    );
    script.push_str(
        "    \x20   string alas_cad_preview_geometry_analysis = \"VSPAEROComputeGeometry\";\n\
         \x20       SetAnalysisInputDefaults( alas_cad_preview_geometry_analysis );\n\
         \x20       SetIntAnalysisInput( alas_cad_preview_geometry_analysis, \"GeomSet\", { 4 }, 0 );\n\
         \x20       SetIntAnalysisInput( alas_cad_preview_geometry_analysis, \"ThinGeomSet\", { 3 }, 0 );\n\
         \x20       string alas_cad_preview_geometry_result = ExecAnalysis( alas_cad_preview_geometry_analysis );\n\
         \x20       if ( alas_cad_preview_geometry_result.length() > 0 )\n\
         \x20       {\n\
         \x20           Print( string( \"ALAS_OPENVSP_CAD_PREVIEW_GEOMETRY_COMPLETE\\n\" ) );\n\
         \x20       }\n\
         \x20       while ( GetNumTotalErrors() > 0 )\n\
         \x20       {\n\
         \x20           ErrorObj preview_geometry_error = PopLastError();\n\
         \x20           Print( string( \"ALAS_OPENVSP_CAD_PREVIEW_WARNING: \" ) + preview_geometry_error.GetErrorString() );\n\
         \x20       }\n\
         \x20       // ScreenGrab requires OpenVSP's GUI build and an available\n\
         \x20       // graphics context. Keep that optional artifact separate\n\
         \x20       // from model validity so headless export stays truthful.\n\
         \x20       if ( IsGUIBuild() )\n\
         \x20       {\n",
    );
    let _ = writeln!(
        script,
        "         \x20       ScreenGrab(\"{}\", 1600, 900, true, true);",
        script_string(preview_name)
    );
    script.push_str(
        "         \x20       while ( GetNumTotalErrors() > 0 )\n\
         \x20       {\n\
         \x20           ErrorObj preview_error = PopLastError();\n\
         \x20           Print( string( \"ALAS_OPENVSP_PREVIEW_WARNING: \" ) + preview_error.GetErrorString() );\n\
         \x20       }\n\
         \x20       }\n\
         \x20       else\n\
         \x20       {\n\
         \x20           Print( string( \"ALAS_OPENVSP_PREVIEW_UNAVAILABLE: runtime has no graphics-capable GUI build\\n\" ) );\n\
         \x20       }\n\
         \x20       Print( string( \"ALAS_OPENVSP_EXPORT_COMPLETE\\n\" ) );\n\
         \x20       return 0;\n\
         \x20   }\n\
         \x20   return 1;\n\
         }\n",
    );
    script
}

fn emit_fuselage(script: &mut String, index: usize, fuselage: &Fuselage) {
    if fuselage.xsecs.len() < 2 {
        return;
    }
    let id = format!("body_{index}");
    let surf = format!("body_surf_{index}");
    let first = fuselage.xsecs[0];
    let last = fuselage.xsecs[fuselage.xsecs.len() - 1];
    let length = (last.xyz_c[0] - first.xyz_c[0]).abs().max(1.0e-6);
    let _ = writeln!(script, "    string {id} = AddGeom( \"FUSELAGE\", \"\" );");
    let _ = writeln!(script, "    SetSetFlag( {id}, 4, true );");
    let _ = writeln!(
        script,
        "    SetGeomName( {id}, \"{}\" );",
        script_string(&fuselage.name)
    );
    set_parm(script, &id, "Length", "Design", length);
    set_parm(script, &id, "X_Rel_Location", "XForm", first.xyz_c[0]);
    set_parm(script, &id, "Y_Rel_Location", "XForm", first.xyz_c[1]);
    set_parm(script, &id, "Z_Rel_Location", "XForm", first.xyz_c[2]);
    let _ = writeln!(script, "    string {surf} = GetXSecSurf( {id}, 0 );");
    let _ = writeln!(
        script,
        "    while ( GetNumXSec( {surf} ) > 2 ) {{ CutXSec( {id}, 1 ); }}"
    );
    for insert_index in 0..fuselage.xsecs.len().saturating_sub(2) {
        let _ = writeln!(
            script,
            "    InsertXSec( {id}, {insert_index}, XS_ELLIPSE );"
        );
    }
    for (section_index, section) in fuselage.xsecs.iter().enumerate() {
        let x_fraction = (section.xyz_c[0] - first.xyz_c[0]) / length;
        let y_fraction = (section.xyz_c[1] - first.xyz_c[1]) / length;
        let z_fraction = (section.xyz_c[2] - first.xyz_c[2]) / length;
        let shape = if section.width <= 1.0e-9 || section.height <= 1.0e-9 {
            "XS_POINT"
        } else {
            "XS_ELLIPSE"
        };
        let xsec_id = format!("body_xsec_{index}_{section_index}");
        let _ = writeln!(
            script,
            "    ChangeXSecShape( {surf}, {section_index}, {shape} );"
        );
        let _ = writeln!(
            script,
            "    string {xsec_id} = GetXSec( {surf}, {section_index} );"
        );
        let _ = writeln!(
            script,
            "    SetXSecWidthHeight( {xsec_id}, {:.12}, {:.12} );",
            section.width, section.height
        );
        set_xsec_parm(script, &xsec_id, "XLocPercent", x_fraction);
        set_xsec_parm(script, &xsec_id, "YLocPercent", y_fraction);
        set_xsec_parm(script, &xsec_id, "ZLocPercent", z_fraction);
    }
    script.push('\n');
}
