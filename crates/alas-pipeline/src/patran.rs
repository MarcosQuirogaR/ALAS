// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/integration/patran_runner.py.
// Reference: alas @ rust-port-baseline.

//! Headless Patran deformation-image export after a successful SOL 101 run.
//!
//! Patran may exit successfully after an internal session failure, so the
//! contract is the same as the Python reference: a run succeeds only when the
//! requested PNG is present. Each load case receives a clean directory because
//! Patran's `Increment` image mode otherwise makes stale images look current.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use alas_exec::process::{kill_process_tree, NewProcessGroup, NoConsoleWindow};
use alas_exec::SupervisedSpawn;

use crate::structural::PatranExportResult;

const VIEW_AA: [f64; 3] = [-57.552_925, -8.686_753, 111.595_818];

/// Render one deformation image per SOL 101 load case.
pub fn run_patran_export(
    work_dir: &Path,
    executable: &Path,
    load_case_names: &[&str],
    timeout_seconds: f64,
) -> PatranExportResult {
    if load_case_names.is_empty() {
        return PatranExportResult {
            status: "ok".to_owned(),
            error: None,
            png_paths: Vec::new(),
        };
    }
    let work_dir = match absolute_path(work_dir, "work directory") {
        Ok(path) => path,
        Err(error) => return failed(error),
    };
    let executable = match absolute_path(executable, "executable") {
        Ok(path) => path,
        Err(error) => return failed(error),
    };
    let Some(install_bin) = executable.parent() else {
        return failed("Patran executable has no installation directory".to_owned());
    };
    let Some(install_root) = install_bin.parent() else {
        return failed("Patran executable has no installation root".to_owned());
    };
    let template_db = install_root.join("template.db");
    if !template_db.is_file() {
        return failed(format!(
            "Patran template.db not found at {} (expected next to the Patran install root)",
            template_db.display()
        ));
    }

    let bdf_path = work_dir.join("wing_mesh.bdf");
    let op2_path = work_dir.join("sol101").join("wing_sol101.op2");
    if !bdf_path.is_file() || !op2_path.is_file() {
        return failed(format!(
            "Missing {} or {}: run NASTRAN SOL 101 first",
            bdf_path.display(),
            op2_path.display()
        ));
    }

    let mut png_paths = Vec::new();
    let mut failures = Vec::new();
    for (index, name) in load_case_names.iter().enumerate() {
        let case_dir = work_dir.join("patran").join(name);
        if case_dir.exists() {
            if let Err(error) = fs::remove_dir_all(&case_dir) {
                failures.push(format!(
                    "{name}: cannot clean {}: {error}",
                    case_dir.display()
                ));
                continue;
            }
        }
        if let Err(error) = fs::create_dir_all(&case_dir) {
            failures.push(format!(
                "{name}: cannot create {}: {error}",
                case_dir.display()
            ));
            continue;
        }

        let db_stem = case_dir.join("preview");
        let h5_path = case_dir.join("wing_sol101.op2.h5");
        let png_stem = case_dir.join(format!("deform_{name}"));
        let session_path = case_dir.join("render.ses");
        let script = session_script(
            &template_db,
            &db_stem,
            &bdf_path,
            &op2_path,
            &h5_path,
            index + 1,
            &png_stem,
        );
        if let Err(error) = fs::write(&session_path, script) {
            failures.push(format!(
                "{name}: cannot write {}: {error}",
                session_path.display()
            ));
            continue;
        }

        match run_one(&executable, &session_path, &png_stem, timeout_seconds) {
            Ok(path) => png_paths.push(((*name).to_owned(), path)),
            Err(detail) => failures.push(format!("{name}: {detail}")),
        }
    }

    if png_paths.is_empty() {
        return failed(failures.join("\n"));
    }
    PatranExportResult {
        status: "ok".to_owned(),
        error: (!failures.is_empty())
            .then(|| format!("Some load cases failed:\n{}", failures.join("\n"))),
        png_paths,
    }
}

/// Resolve paths before Patran switches into a per-case working directory.
///
/// Recorded Patran session commands resolve their paths relative to the
/// directory from which Patran replays them. Passing a repository-relative
/// output location therefore creates a second, nonexistent `outputs/...`
/// subtree below each case directory. Keep all paths written into the session
/// absolute so the process working directory is no longer significant.
fn absolute_path(path: &Path, label: &str) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(|error| format!("Cannot resolve Patran {label} {}: {error}", path.display()))
}

fn failed(error: String) -> PatranExportResult {
    PatranExportResult {
        status: "error".to_owned(),
        error: Some(error),
        png_paths: Vec::new(),
    }
}

fn run_one(
    executable: &Path,
    session_path: &Path,
    png_stem: &Path,
    timeout_seconds: f64,
) -> Result<PathBuf, String> {
    let Some(case_dir) = session_path.parent() else {
        return Err("Patran session has no working directory".to_owned());
    };
    let Some(session_name) = session_path.file_name() else {
        return Err("Patran session has no file name".to_owned());
    };
    let command_line = format!(
        "{} -b -graphics -sfp {}",
        executable.display(),
        session_name.to_string_lossy()
    );
    let mut command = Command::new(executable);
    command
        .args(["-b", "-graphics", "-sfp"])
        .arg(session_name)
        .current_dir(case_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .no_window()
        .new_process_group();
    let mut child = command.spawn_supervised("Patran render").map_err(|error| {
        let classification = if error.kind() == std::io::ErrorKind::PermissionDenied {
            "OS denied execution (check sandbox or executable permissions)"
        } else if error.kind() == std::io::ErrorKind::NotFound {
            "executable disappeared or its installation is incomplete"
        } else {
            "process creation failed"
        };
        format!(
            "Failed to launch {}: {error} ({classification})",
            executable.display()
        )
    })?;
    let deadline = Instant::now() + Duration::from_secs_f64(timeout_seconds.max(0.0));
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                kill_process_tree(child.id());
                let _ = child.wait();
                return Err(format!(
                    "Timed out after {timeout_seconds:.0}s running: {command_line} (process tree force-killed)"
                ));
            }
            Err(error) => return Err(format!("Cannot poll Patran: {error}")),
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("Cannot collect Patran output: {error}"))?;
    if let Some(path) = first_png(png_stem) {
        return Ok(path);
    }
    Err(format!(
        "Patran exited (code {}) but wrote no {}*.png. stdout (tail): {} stderr (tail): {} session journal (tail): {}",
        output
            .status
            .code()
            .map_or_else(|| "unknown".to_owned(), |code| code.to_string()),
        png_stem.display(),
        tail(&String::from_utf8_lossy(&output.stdout)),
        tail(&String::from_utf8_lossy(&output.stderr)),
        session_journal_tail(case_dir)
    ))
}

fn first_png(stem: &Path) -> Option<PathBuf> {
    let directory = stem.parent()?;
    let prefix = stem.file_name()?.to_string_lossy();
    let mut matches = fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            let Some(name) = path.file_name().map(|name| name.to_string_lossy()) else {
                return false;
            };
            name.starts_with(prefix.as_ref())
                && name.to_ascii_lowercase().ends_with(".png")
                && is_png(path)
        })
        .collect::<Vec<_>>();
    matches.sort();
    matches.into_iter().next()
}

fn is_png(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}

fn tail(text: &str) -> String {
    text.lines()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

/// Read the most recent Patran replay journal when stdout does not expose the
/// session failure. Patran commonly exits zero after a `NO` command result.
fn session_journal_tail(case_dir: &Path) -> String {
    let Ok(entries) = fs::read_dir(case_dir) else {
        return "journal unavailable".to_owned();
    };
    let mut journals = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("patran.ses."))
        })
        .collect::<Vec<_>>();
    journals.sort();
    let Some(journal) = journals.last() else {
        return "no journal written".to_owned();
    };
    match fs::read_to_string(journal) {
        Ok(text) => tail(&text),
        Err(error) => format!("cannot read {}: {error}", journal.display()),
    }
}

// These seven paths and the subcase number are the variable fields of one
// recorded Patran journal; grouping them would hide the one-to-one template.
#[allow(clippy::too_many_arguments)]
fn session_script(
    template_db: &Path,
    db_stem: &Path,
    bdf_path: &Path,
    op2_path: &Path,
    h5_path: &Path,
    subcase_number: usize,
    png_stem: &Path,
) -> String {
    let sc = format!("SC{subcase_number}:");
    let lines = [
        format!("uil_file_new.go( \"{}\", \"{}\" )", template_db.display(), db_stem.display()),
        format!("set_current_dir( \"{}\" )", db_stem.parent().unwrap_or(Path::new(".")).display()),
        format!("nastran_input_import( \"{}\", \"default_group\", 11, [TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, FALSE, TRUE, TRUE], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], [-2000000000, -2000000000, -2000000000, -2000000000, -2000000000, -2000000000, -2000000000, -2000000000, 0, 0, 0] )", bdf_path.display()),
        format!("op2_to_hdf5_translate( \"{}\", FALSE, \"{}\", TRUE )", op2_path.display(), h5_path.display()),
        "msc_dra_init_stream(  )".to_owned(),
        format!("msc_dra_add_param( \"DATABASE\", \"{}.db\" )", db_stem.display()),
        "msc_dra_add_param( \"JOBNAME\", \"wing_mesh\" )".to_owned(),
        format!("msc_dra_add_param( \"RESULTS FILE\", \"{}\" )", h5_path.display()),
        "msc_dra_add_param( \"OBJECT\", \"Result Entities\" )".to_owned(),
        "msc_dra_add_param( \"ANALYSIS TYPE\", \"Structural\" )".to_owned(),
        "msc_dra_add_param( \"DIVISION TOLERANCE\", \"1.0E-8\" )".to_owned(),
        "msc_dra_add_param( \"NUMERICAL TOLERANCE\", \"1.0E-4\" )".to_owned(),
        "msc_dra_add_param( \"MODEL TOLERANCE\", \"0.0049999999\" )".to_owned(),
        "msc_dra_add_param( \"OBJECTIVE FUNCTION\", \"ON\" )".to_owned(),
        "msc_dra_add_param( \"DESIGN CONSTRAINTS\", \"ON\" )".to_owned(),
        "msc_dra_add_param( \"DESIGN VARIABLES\", \"ON\" )".to_owned(),
        "msc_dra_add_param( \"COMBINE RESULTCASES\", \"ON\" )".to_owned(),
        "msc_dra_add_param( \"COMBINE MODULES\", \"OFF\" )".to_owned(),
        "msc_dra_add_param( \"SPLINE IMPORT DATA\", \"OFF\" )".to_owned(),
        "msc_dra_add_param( \"SPLINE POST DATA\", \"ZERO\" )".to_owned(),
        "msc_dra_add_param( \"ROTATIONAL NODAL RESULTS\", \"ON\" )".to_owned(),
        "msc_dra_add_param( \"STRESS/STRAIN INVARIANTS\", \"OFF\" )".to_owned(),
        "msc_dra_add_param( \"PRINCIPAL DIRECTIONS\", \"OFF\" )".to_owned(),
        "msc_dra_add_param( \"CREATE P-ORDER FIELD\", \"OFF\" )".to_owned(),
        "msc_dra_add_param( \"ELEMENT RESULTS POSITIONS\", \"Both        \" )".to_owned(),
        "msc_dra_add_param( \"NASTRAN VERSION\", \"2026.1\" )".to_owned(),
        "msc_dra_add_param( \"TITLE DESCRIPTION\", \"ON\" )".to_owned(),
        "msc_dra_finish_stream(  )".to_owned(),
        format!("analysis_import( \"MSC.Nastran\", \"wing_mesh\", \"Attach HDF5 Results File\", \"{}\", TRUE )", h5_path.display()),
        format!("op2hdf5_import2( \"{}\", \"ON\", \"OFF\", \"OFF\", \"BOTH\", FALSE )", h5_path.display()),
        "res_dra_detach_file( 1, 150, 25 )".to_owned(),
        "res_display_tool_unpost( \"Fringe\", \"default_Fringe\" )".to_owned(),
        format!("res_data_load_dbresult( 0, \"Nodal\", \"Vector\", \"{sc}\", \"Static subcase\", \"Displacements\", \"Translational\", \"(NON-LAYERED)\", \"\", \"Global\", \"\", \"\", \"\" )"),
        "res_data_title( 0, \"Nodal\", \"Vector\", 1, [\"$POFF@@@$PT: @@@$LCN, @@@$SCN, @@@$PRN, @@@$SRN, @@@$DRVL\"] )".to_owned(),
        // Patran 2026.1's batch viewport places its result title outside the
        // left edge, leaving a misleading partial word in every PNG. The
        // report supplies the complete load-case label beside the native image.
        "res_display_deformation_create( \"\", \"Elements\", 0, [\"\"], 10, [\"DeformedStyle:White,Solid,1,Wireframe\", \"DeformedScale:Model=0.1\", \"UndeformedStyle:ON,Blue,Solid,1,Wireframe\", \"TitleDisplay:OFF\", \"MinMaxDisplay:ON\", \"ScaleFactor:1.\", \"LabelStyle:Exponential, 12, White, 3\", \"DeformDisplay:Resultant\", \"DeformComps:OFF,OFF,OFF\", \"RelativeToGeom:ORIG\"] )".to_owned(),
        "res_display_deformation_post( \"\", 0 )".to_owned(),
        format!("ga_view_aa_set( {}, {}, {} )", VIEW_AA[0], VIEW_AA[1], VIEW_AA[2]),
        format!("gm_write_image( \"PNG\", \"{}.png\", \"Increment\", 0., 0., 1., 1., 10, \"Viewport\" )", png_stem.display()),
    ];
    format!("{}\n", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_uses_the_requested_subcase_and_recorded_batch_commands() {
        let script = session_script(
            Path::new("C:/Patran/template.db"),
            Path::new("C:/work/preview"),
            Path::new("C:/work/wing_mesh.bdf"),
            Path::new("C:/work/sol101/wing_sol101.op2"),
            Path::new("C:/work/result.h5"),
            2,
            Path::new("C:/work/deform_push-down"),
        );
        assert!(script.contains("uil_file_new.go"));
        assert!(script.contains("\"SC2:\""));
        assert!(script.contains("gm_write_image"));
        assert!(script.contains("TitleDisplay:OFF"));
        assert!(script.ends_with('\n'));
    }

    #[test]
    fn relative_session_paths_are_resolved_before_patran_changes_directory() {
        let relative = Path::new("outputs/structures");
        let resolved = absolute_path(relative, "work directory");
        let expected = std::env::current_dir().map(|cwd| cwd.join(relative));
        assert_eq!(resolved.ok(), expected.ok());
    }

    #[test]
    fn a_session_journal_is_included_in_missing_artifact_diagnostics() {
        let root = std::env::temp_dir().join(format!("alas-patran-journal-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::create_dir_all(&root);
        let journal = root.join("patran.ses.01");
        let _ = fs::write(&journal, "first\nsecond\nrecorded failure");
        assert!(session_journal_tail(&root).contains("recorded failure"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_template_is_an_explicit_error_before_launch() {
        let result = run_patran_export(
            Path::new("C:/missing-work"),
            Path::new("C:/missing/bin/patran.exe"),
            &["pull-up"],
            1.0,
        );
        assert_eq!(result.status, "error");
        assert!(result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("template.db")));
    }

    #[test]
    fn an_empty_load_case_list_is_a_successful_empty_export() {
        let result = run_patran_export(
            Path::new("C:/missing-work"),
            Path::new("C:/missing/bin/patran.exe"),
            &[],
            1.0,
        );
        assert_eq!(result.status, "ok");
        assert!(result.error.is_none());
        assert!(result.png_paths.is_empty());
    }

    #[test]
    fn an_invalid_png_is_reported_as_a_failed_artifact_parse() {
        let work = std::env::temp_dir().join(format!("alas-patran-invalid-{}", std::process::id()));
        let _ = fs::remove_dir_all(&work);
        let install = work.join("patran");
        let executable = install.join("bin/patran.exe");
        let _ = fs::create_dir_all(executable.parent().unwrap_or(Path::new(".")));
        let _ = fs::write(install.join("template.db"), b"template");
        let _ = fs::create_dir_all(work.join("sol101"));
        let _ = fs::write(work.join("wing_mesh.bdf"), b"BEGIN BULK\nENDDATA\n");
        let _ = fs::write(work.join("sol101/wing_sol101.op2"), b"op2");

        // The executable is intentionally absent: the test reaches the same
        // artifact contract through the pure PNG filter below without relying
        // on a licensed Patran installation.
        let stem = work.join("deform_pull-up");
        let _ = fs::write(stem.with_extension("png"), b"not a png");
        assert!(first_png(&stem).is_none());
        let _ = fs::remove_dir_all(work);
    }

    #[test]
    fn a_missing_patran_launcher_is_reported_after_input_validation() {
        let work = std::env::temp_dir().join(format!("alas-patran-launch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&work);
        let install = work.join("patran");
        let executable = install.join("bin/patran.exe");
        let _ = fs::create_dir_all(executable.parent().unwrap_or(Path::new(".")));
        let _ = fs::write(install.join("template.db"), b"template");
        let _ = fs::create_dir_all(work.join("sol101"));
        let _ = fs::write(work.join("wing_mesh.bdf"), b"BEGIN BULK\nENDDATA\n");
        let _ = fs::write(work.join("sol101/wing_sol101.op2"), b"op2");

        let result = run_patran_export(&work, &executable, &["pull-up"], 1.0);
        assert_eq!(result.status, "error");
        assert!(result
            .error
            .as_deref()
            .is_some_and(|detail| detail.contains("Failed to launch")));
        let _ = fs::remove_dir_all(work);
    }

    #[test]
    fn valid_incremented_png_references_are_selected_in_filename_order() {
        let root = std::env::temp_dir().join(format!("alas-patran-order-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::create_dir_all(&root);
        let stem = root.join("deform_pull-up");
        let signature = b"\x89PNG\r\n\x1a\n";
        let _ = fs::write(root.join("deform_pull-up_2.png"), signature);
        let _ = fs::write(root.join("deform_pull-up_1.png"), signature);
        assert_eq!(first_png(&stem), Some(root.join("deform_pull-up_1.png")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[ignore = "requires ALAS_PATRAN_EXE and ALAS_PATRAN_WORK_DIR with retained SOL 101 inputs"]
    fn installed_patran_renders_real_sol101_deformations() {
        let executable = std::env::var_os("ALAS_PATRAN_EXE")
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("set ALAS_PATRAN_EXE to a licensed Patran executable"));
        let work_dir = std::env::var_os("ALAS_PATRAN_WORK_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("set ALAS_PATRAN_WORK_DIR to retained SOL 101 inputs"));
        assert!(
            executable.is_file(),
            "{} is not a file",
            executable.display()
        );
        assert!(work_dir.join("wing_mesh.bdf").is_file());
        assert!(work_dir.join("sol101/wing_sol101.op2").is_file());

        let result = run_patran_export(
            &work_dir,
            &executable,
            &["pull-up", "push-down", "level"],
            180.0,
        );
        assert_eq!(result.status, "ok", "{:?}", result.error);
        assert_eq!(result.png_paths.len(), 3, "{:?}", result.error);
        assert!(result.png_paths.iter().all(|(_, path)| is_png(path)));
    }
}
