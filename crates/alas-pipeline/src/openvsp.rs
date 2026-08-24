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

use std::fmt::Write as FmtWrite;
use std::fs;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use alas_config::AlasConfig;
use alas_exec::process::{kill_process_tree, NewProcessGroup, NoConsoleWindow};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::wing::Wing;
use alas_perf::landing_gear::{size_landing_gear, LandingGearLayout};

use crate::full_analysis::AnalysisReport;

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

/// Files and fidelity notes produced by an OpenVSP geometry export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenVspExportResult {
    /// Script accepted by OpenVSP's AngelScript runner.
    pub script_path: PathBuf,
    /// Project path the script passes to OpenVSP's `WriteVSPFile` API.
    pub vsp3_path: PathBuf,
    /// Thin lifting-surface mesh written by `VSPAEROComputeGeometry`.
    pub vspaero_geometry_path: PathBuf,
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
    let gear = landing_gear_for_report(report, config);
    let script = render_script(&report.airplane, Some(&gear), vsp3_name);
    validate_script(&script).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(script_path, script)?;

    Ok(OpenVspExportResult {
        script_path: script_path.to_path_buf(),
        vsp3_path,
        vspaero_geometry_path,
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
    let Some(work_dir) = export.script_path.parent().map(Path::to_path_buf) else {
        return reject_runtime(export, "OpenVSP script has no working directory".to_owned());
    };
    let Some(script_name) = export.script_path.file_name().map(ToOwned::to_owned) else {
        return reject_runtime(export, "OpenVSP script has no file name".to_owned());
    };
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
    let mut child = match command.spawn() {
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
    if !is_native_vsp3(&export.vsp3_path) {
        let detail = format!(
            "OpenVSP reported completion but {} is absent or not a native VSP3 XML document",
            export.vsp3_path.display()
        );
        return reject_runtime(export, detail);
    }
    if !is_native_vspgeom(&export.vspaero_geometry_path) {
        let detail = format!(
            "OpenVSP reported completion but {} is absent or not a native VSP geometry mesh",
            export.vspaero_geometry_path.display()
        );
        return reject_runtime(export, detail);
    }
    export.status = OpenVspExportStatus::Vsp3Materialized;
    export.runtime_error = None;
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
    let x_nlg = fus_start + (fus_end - fus_start) * config.mass_model.nlg_x_fraction;
    let x_mlg = x_mac_le + config.mass_model.mlg_x_fraction_mac * mac;
    let mass_kg = report.component_masses.values().copied().sum();
    let diameter_m = config.geometry.fuselage.diameter_m;

    size_landing_gear(
        mass_kg,
        x_nlg,
        x_mlg,
        aero_fwd_x,
        aero_aft_x,
        diameter_m,
        diameter_m * 1.1,
        &config.landing_gear,
    )
}

fn render_script(
    airplane: &Airplane,
    landing_gear: Option<&LandingGearLayout>,
    vsp3_name: &str,
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

    script.push_str("    Update();\n");
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
        "    int alas_error_count = GetNumTotalErrors();\n\
         \x20   while ( GetNumTotalErrors() > 0 )\n\
         \x20   {\n\
         \x20       ErrorObj err = PopLastError();\n\
         \x20       Print( err.GetErrorString() );\n\
         \x20   }\n\
         \x20   if ( alas_error_count == 0 )\n\
         \x20   {\n\
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

fn emit_wing(script: &mut String, index: usize, wing: &Wing) {
    if wing.xsecs.len() < 2 {
        return;
    }
    let id = format!("wing_{index}");
    let surf = format!("wing_surf_{index}");
    let root = &wing.xsecs[0];
    let _ = writeln!(script, "    string {id} = AddGeom( \"WING\", \"\" );");
    let _ = writeln!(script, "    SetSetFlag( {id}, 3, true );");
    let _ = writeln!(
        script,
        "    SetGeomName( {id}, \"{}\" );",
        script_string(&wing.name)
    );
    set_parm(script, &id, "X_Rel_Location", "XForm", root.xyz_le[0]);
    set_parm(script, &id, "Y_Rel_Location", "XForm", root.xyz_le[1]);
    set_parm(script, &id, "Z_Rel_Location", "XForm", root.xyz_le[2]);
    set_parm(
        script,
        &id,
        "Sym_Planar_Flag",
        "Sym",
        if wing.symmetric { 2.0 } else { 0.0 },
    );
    for section_index in 1..wing.xsecs.len().saturating_sub(1) {
        let _ = writeln!(
            script,
            "    InsertXSec( {id}, {section_index}, XS_FILE_AIRFOIL );"
        );
    }
    let _ = writeln!(script, "    string {surf} = GetXSecSurf( {id}, 0 );");

    for section_index in 1..wing.xsecs.len() {
        let inside = &wing.xsecs[section_index - 1];
        let outside = &wing.xsecs[section_index];
        let dx = outside.xyz_le[0] - inside.xyz_le[0];
        let dy = outside.xyz_le[1] - inside.xyz_le[1];
        let dz = outside.xyz_le[2] - inside.xyz_le[2];
        let span = dy.hypot(dz).max(1.0e-6);
        let sweep = dx.atan2(span).to_degrees();
        let dihedral = dz.atan2(dy.abs().max(1.0e-12)).to_degrees();
        let _ = writeln!(
            script,
            "    SetDriverGroup( {id}, {section_index}, SPAN_WSECT_DRIVER, ROOTC_WSECT_DRIVER, TIPC_WSECT_DRIVER );"
        );
        let group = format!("XSec_{section_index}");
        set_parm(script, &id, "Span", &group, span);
        set_parm(script, &id, "Root_Chord", &group, inside.chord);
        set_parm(script, &id, "Tip_Chord", &group, outside.chord);
        set_parm(script, &id, "Sweep", &group, sweep);
        set_parm(script, &id, "Sweep_Location", &group, 0.0);
        set_parm(script, &id, "Dihedral", &group, dihedral);
        set_parm(script, &id, "Twist", &group, outside.twist);
    }
    for (section_index, section) in wing.xsecs.iter().enumerate() {
        emit_airfoil(
            script,
            &surf,
            index,
            section_index,
            &section.airfoil.coordinates,
        );
    }
    script.push('\n');
}

fn emit_airfoil(
    script: &mut String,
    wing_surf: &str,
    wing_index: usize,
    section_index: usize,
    coordinates: &[(f64, f64)],
) {
    if coordinates.len() < 3 {
        return;
    }
    let leading_edge = coordinates
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.0.total_cmp(&b.0))
        .map_or(0, |(index, _)| index);
    let xsec = format!("wing_xsec_{wing_index}_{section_index}");
    let upper = format!("upper_{wing_index}_{section_index}");
    let lower = format!("lower_{wing_index}_{section_index}");
    let _ = writeln!(
        script,
        "    ChangeXSecShape( {wing_surf}, {section_index}, XS_FILE_AIRFOIL );"
    );
    let _ = writeln!(
        script,
        "    string {xsec} = GetXSec( {wing_surf}, {section_index} );"
    );
    let _ = writeln!(script, "    array< vec3d > {upper};");
    for &(x, y) in coordinates[..=leading_edge].iter().rev() {
        let _ = writeln!(
            script,
            "    {upper}.insertLast( vec3d( {x:.12}, {y:.12}, 0.0 ) );"
        );
    }
    let _ = writeln!(script, "    array< vec3d > {lower};");
    for &(x, y) in &coordinates[leading_edge..] {
        let _ = writeln!(
            script,
            "    {lower}.insertLast( vec3d( {x:.12}, {y:.12}, 0.0 ) );"
        );
    }
    let _ = writeln!(script, "    SetAirfoilPnts( {xsec}, {upper}, {lower} );");
}

fn emit_landing_gear(script: &mut String, airplane: &Airplane, gear: &LandingGearLayout) {
    let belly_z = airplane
        .fuselages
        .first()
        .and_then(|fuselage| {
            fuselage
                .xsecs
                .iter()
                .map(|section| section.xyz_c[2] - section.height / 2.0)
                .reduce(f64::min)
        })
        .unwrap_or(0.0);
    for (index, wheel) in gear.wheels.iter().enumerate() {
        let id = format!("wheel_{index}");
        let surf = format!("wheel_surf_{index}");
        let width = wheel.width_m.max(0.05);
        let diameter = wheel.diameter_m.max(0.05);
        let _ = writeln!(script, "    string {id} = AddGeom( \"FUSELAGE\", \"\" );");
        let _ = writeln!(
            script,
            "    SetGeomName( {id}, \"{} wheel {}\" );",
            wheel.group,
            index + 1
        );
        set_parm(script, &id, "Length", "Design", width);
        set_parm(script, &id, "X_Rel_Location", "XForm", wheel.x);
        set_parm(
            script,
            &id,
            "Y_Rel_Location",
            "XForm",
            wheel.y - width / 2.0,
        );
        set_parm(
            script,
            &id,
            "Z_Rel_Location",
            "XForm",
            belly_z - wheel.diameter_m / 2.0,
        );
        set_parm(script, &id, "Z_Rel_Rotation", "XForm", 90.0);
        let _ = writeln!(script, "    string {surf} = GetXSecSurf( {id}, 0 );");
        let _ = writeln!(
            script,
            "    while ( GetNumXSec( {surf} ) > 3 ) {{ CutXSec( {id}, 1 ); }}"
        );
        for section_index in 0..3 {
            let shape = if section_index == 1 {
                "XS_ELLIPSE"
            } else {
                "XS_POINT"
            };
            let xsec = format!("wheel_xsec_{index}_{section_index}");
            let section_diameter = if section_index == 1 { diameter } else { 0.0 };
            let _ = writeln!(
                script,
                "    ChangeXSecShape( {surf}, {section_index}, {shape} );"
            );
            let _ = writeln!(
                script,
                "    string {xsec} = GetXSec( {surf}, {section_index} );"
            );
            let _ = writeln!(
                script,
                "    SetXSecWidthHeight( {xsec}, {section_diameter:.12}, {section_diameter:.12} );"
            );
            set_xsec_parm(script, &xsec, "XLocPercent", section_index as f64 / 2.0);
        }
    }
    script.push('\n');
}

fn set_parm(script: &mut String, id: &str, name: &str, group: &str, value: f64) {
    let _ = writeln!(
        script,
        "    SetParmVal( {id}, \"{name}\", \"{group}\", {value:.12} );"
    );
}

fn set_xsec_parm(script: &mut String, xsec_id: &str, name: &str, value: f64) {
    let _ = writeln!(
        script,
        "    SetParmVal( GetXSecParm( {xsec_id}, \"{name}\" ), {value:.12} );"
    );
}

fn script_string(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\\' => '/',
            '"' | '\n' | '\r' => '_',
            other if other.is_ascii() => other,
            _ => '_',
        })
        .collect()
}

fn validate_script(script: &str) -> Result<(), &'static str> {
    if !script.contains("int main()")
        || !script.contains("WriteVSPFile(")
        || !script.contains("VSPAEROComputeGeometry")
        || !script.contains("ThinGeomSet")
        || !script.contains("ALAS_OPENVSP_EXPORT_COMPLETE")
    {
        return Err("OpenVSP script is missing its entry point or completion contract");
    }
    let mut depth = 0_i64;
    for character in script.chars() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return Err("OpenVSP script has an unmatched closing brace");
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err("OpenVSP script has unbalanced braces");
    }
    if script.contains("NaN") || script.contains("inf") {
        return Err("OpenVSP script contains a non-finite geometry value");
    }
    Ok(())
}

#[cfg(test)]
#[path = "openvsp_tests.rs"]
mod tests;
