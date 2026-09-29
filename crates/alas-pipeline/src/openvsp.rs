// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! OpenVSP geometry interchange through its supported AngelScript API.
//!
//! A `.vsp3` file is OpenVSP's private, versioned XML serialization. Writing
//! that XML without OpenVSP would couple this application to implementation
//! details that have changed between releases. The supported script API is a
//! stable OpenVSP input format: this exporter writes a `.vspscript` that builds
//! the computed aircraft and calls `WriteVSPFile`, producing the adjacent
//! `.vsp3` with the installed OpenVSP version's own serializer.

mod freshness;
use freshness::{
    is_fresh_native_png, is_fresh_native_vsp3, is_fresh_native_vspgeom, is_native_vsp3,
    preview_failure_reason, text_tail,
};

mod emit;
mod native_preview;
mod project;
mod script;
#[cfg(test)]
// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
mod validation;

use emit::*;
pub use project::materialize_openvsp_project;
use script::*;
use validation::validate_script;

use std::fmt::Write as FmtWrite;
use std::fs;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use alas_config::{AlasConfig, MainGearFallbackRefusal};
use alas_exec::process::{
    timeout_from_seconds, wait_with_timeout, DeadlineWait, NewProcessGroup, NoConsoleWindow,
};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::wing::Wing;
use alas_perf::landing_gear::{size_landing_gear_with_group_stations, LandingGearLayout};

use crate::full_analysis::AnalysisReport;
use crate::gear_stations::resolved_gear_stations;
/// Evidence state of an OpenVSP export artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenVspExportStatus {
    /// The supported input script was written, but no installed runtime read it.
    ScriptWrittenRuntimeUnverified,
    /// The configured executable could not be launched.
    RuntimeLaunchFailed,
    /// The OpenVSP process exceeded its deadline and its process tree was killed.
    RuntimeTimedOut,
    /// The configured timeout was not a finite number of seconds greater than
    /// zero; OpenVSP was not launched.
    InvalidTimeout,
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
            Self::InvalidTimeout => "invalid_timeout",
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
    // A missing main-gear station fails the export rather than writing an
    // aeroplane without gear or with gear at an unmeasured station: this
    // artifact is read downstream as the aircraft, so both would misreport
    // it. The typed refusal names the missing datum and the two heights that
    // decided it.
    let gear = landing_gear_for_report(report, config).map_err(|refusal| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("landing-gear export refused: {refusal}"),
        )
    })?;
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
