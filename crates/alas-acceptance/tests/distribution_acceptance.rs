// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in acceptance tests for the packaged `alas` executable.
//!
//! These tests deliberately run only when `ALAS_W55_PACKAGE_DIR` names a
//! previously assembled distribution. Keeping the package input explicit
//! prevents a normal workspace test from silently using a developer checkout,
//! a real user profile, or an installed external solver.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use alas_config::presets;

const PACKAGE_ENV: &str = "ALAS_W55_PACKAGE_DIR";

struct Sandbox {
    path: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!("alas-w55-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&path).expect("W5.5 sandbox can be created");
        Self { path }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn package_dir() -> PathBuf {
    let value = env::var_os(PACKAGE_ENV).unwrap_or_else(|| {
        panic!("{PACKAGE_ENV} must name an assembled package; run cargo xtask dist first")
    });
    let path = PathBuf::from(value);
    assert!(
        path.is_dir(),
        "package directory does not exist: {}",
        path.display()
    );
    path
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("package destination can be created");
    for entry in fs::read_dir(source).expect("package can be read") {
        let entry = entry.expect("package entry can be read");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_tree(&source_path, &destination_path);
        } else {
            fs::copy(&source_path, &destination_path).expect("package file can be copied");
        }
    }
}

fn packaged_executable(bundle: &Path) -> PathBuf {
    let name = if cfg!(windows) { "ALAS.exe" } else { "ALAS" };
    let executable = bundle.join(name);
    assert!(
        executable.is_file(),
        "packaged executable does not exist: {}",
        executable.display()
    );
    executable
}

fn child_output(executable: &Path, cwd: &Path, user_data: &Path, args: &[String]) -> Output {
    Command::new(executable)
        .current_dir(cwd)
        .env("ALAS_FROZEN", "1")
        .env("LOCALAPPDATA", user_data.join("localappdata"))
        .env("XDG_DATA_HOME", user_data.join("xdg"))
        .env("HOME", user_data.join("home"))
        .args(args)
        .output()
        .expect("packaged executable can be launched")
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_overlay(path: &Path, preset: &str, mission: bool, mses: bool, structures: bool) {
    let content = format!(
        "preset: {preset}\nmission:\n  enabled: {mission}\nmses:\n  enabled: {mses}\nstructures:\n  enabled: {structures}\n"
    );
    fs::write(path, content).expect("preset overlay can be written");
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn write_preferences(user_data: &Path, mses: &Path, nastran: &Path, patran: &Path) {
    let directory = user_data.join("localappdata").join("ALAS");
    fs::create_dir_all(&directory).expect("isolated preference directory can be created");
    let text = format!(
        "{{\n  \"mses_dir\": \"{}\",\n  \"nastran_exe\": \"{}\",\n  \"patran_exe\": \"{}\"\n}}\n",
        path_text(mses),
        path_text(nastran),
        path_text(patran)
    );
    fs::write(directory.join("tool-preferences.json"), text)
        .expect("isolated tool preferences can be written");
}

fn adjacent_tools(bundle: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let mses = bundle.join("external tools/MSES");
    let nastran = bundle.join("external tools/NASTRAN/nastran.exe");
    let patran = bundle.join("external tools/Patran/patran.exe");
    fs::create_dir_all(&mses).expect("adjacent MSES directory can be created");
    fs::create_dir_all(nastran.parent().expect("NASTRAN has a parent"))
        .expect("adjacent NASTRAN directory can be created");
    fs::create_dir_all(patran.parent().expect("Patran has a parent"))
        .expect("adjacent Patran directory can be created");
    for name in ["mset.exe", "mses.exe", "mplot.exe"] {
        fs::write(mses.join(name), b"not a solver")
            .expect("fake adjacent MSES launcher can be written");
    }
    fs::write(&nastran, b"not a solver").expect("fake adjacent NASTRAN launcher can be written");
    fs::write(&patran, b"not a post-processor")
        .expect("fake adjacent Patran launcher can be written");
    (mses, nastran, patran)
}

#[test]
#[ignore = "requires an assembled package and is intentionally opt-in"]
fn packaged_executable_covers_w55_runtime_matrix_across_all_presets() {
    let sandbox = Sandbox::new();
    let bundle = sandbox.path.join("bundle");
    copy_tree(&package_dir(), &bundle);
    let executable = packaged_executable(&bundle);
    let user_data = sandbox.path.join("user-data");
    let output_root = sandbox.path.join("runs");
    fs::create_dir_all(&output_root).expect("isolated output root can be created");

    let help = child_output(
        &executable,
        &sandbox.path,
        &user_data,
        &["--help".to_owned()],
    );
    assert_success(&help, "packaged --help");
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage: alas"));

    let (mses, adjacent_nastran, adjacent_patran) = adjacent_tools(&bundle);
    write_preferences(&user_data, &mses, &adjacent_nastran, &adjacent_patran);
    let first_saved = sandbox.path.join("first-restart.yaml");
    let save_args = vec!["--save-config".to_owned(), path_text(&first_saved)];
    let first_save = child_output(&executable, &sandbox.path, &user_data, &save_args);
    assert_success(&first_save, "first persisted-preference load");
    let first_config = fs::read_to_string(&first_saved).expect("first restart config can be read");
    let second_saved = sandbox.path.join("second-restart.yaml");
    let second_save_args = vec!["--save-config".to_owned(), path_text(&second_saved)];
    let second_save = child_output(&executable, &sandbox.path, &user_data, &second_save_args);
    assert_success(&second_save, "post-restart persisted-preference load");
    let second_config = fs::read_to_string(second_saved).expect("restart config can be read");
    assert_eq!(
        first_config, second_config,
        "tool preferences changed across restart"
    );
    assert!(
        second_config.contains(&path_text(&adjacent_nastran)),
        "restart config omitted NASTRAN path:\n{second_config}"
    );
    assert!(
        second_config.contains(&path_text(&adjacent_patran)),
        "restart config omitted Patran path:\n{second_config}"
    );
    assert!(
        second_config.contains(&path_text(&mses)),
        "restart config omitted MSES path:\n{second_config}"
    );

    for preset in presets::available() {
        let no_mission_config = sandbox.path.join(format!("{preset}-no-mission.yaml"));
        write_overlay(&no_mission_config, preset, true, false, false);
        let no_mission_output = output_root.join(format!("{preset}-no-mission"));
        let no_mission_args = vec![
            "--config".to_owned(),
            path_text(&no_mission_config),
            "--no-optimize".to_owned(),
            "--no-baseline".to_owned(),
            "--no-mission".to_owned(),
            "--no-parallel".to_owned(),
            "--quiet".to_owned(),
            "--output".to_owned(),
            path_text(&no_mission_output),
        ];
        let no_mission = child_output(&executable, &sandbox.path, &user_data, &no_mission_args);
        assert_success(&no_mission, &format!("{preset} no-mission"));
        assert!(no_mission_output.join("design_database.json").is_file());

        let mission_config = sandbox.path.join(format!("{preset}-mission.yaml"));
        write_overlay(&mission_config, preset, true, false, false);
        let mission_output = output_root.join(format!("{preset}-mission"));
        let mission_args = vec![
            "--config".to_owned(),
            path_text(&mission_config),
            "--no-optimize".to_owned(),
            "--no-baseline".to_owned(),
            "--no-parallel".to_owned(),
            "--quiet".to_owned(),
            "--output".to_owned(),
            path_text(&mission_output),
        ];
        let mission = child_output(&executable, &sandbox.path, &user_data, &mission_args);
        assert_success(&mission, &format!("{preset} mission-enabled"));
        assert!(mission_output.join("design_database.json").is_file());
    }

    fs::remove_dir_all(adjacent_nastran.parent().expect("NASTRAN has a parent"))
        .expect("adjacent NASTRAN fixture can be removed");
    fs::remove_dir_all(adjacent_patran.parent().expect("Patran has a parent"))
        .expect("adjacent Patran fixture can be removed");
    let missing_config = sandbox.path.join("missing-tools.yaml");
    write_overlay(&missing_config, "AVE", false, false, true);
    let missing_output = output_root.join("missing-tools");
    let missing_args = vec![
        "--config".to_owned(),
        path_text(&missing_config),
        "--no-optimize".to_owned(),
        "--no-baseline".to_owned(),
        "--no-mission".to_owned(),
        "--no-parallel".to_owned(),
        "--quiet".to_owned(),
        "--output".to_owned(),
        path_text(&missing_output),
    ];
    let missing = child_output(&executable, &sandbox.path, &user_data, &missing_args);
    assert_success(&missing, "missing-tool path");
    assert!(missing_output.join("structures").is_dir());

    let configured_nastran = sandbox.path.join("configured/nastran.exe");
    let configured_patran = sandbox.path.join("configured/patran.exe");
    fs::create_dir_all(configured_nastran.parent().expect("configured tool parent"))
        .expect("configured tool directory can be created");
    fs::write(&configured_nastran, b"not a solver").expect("configured NASTRAN can be written");
    fs::write(&configured_patran, b"not a post-processor")
        .expect("configured Patran can be written");
    let configured_config = sandbox.path.join("configured-tools.yaml");
    write_overlay(&configured_config, "AVE", false, false, true);
    let mut configured_text =
        fs::read_to_string(&configured_config).expect("configured overlay read");
    configured_text.push_str(&format!(
        "  nastran_exe_path: {}\n  patran_exe_path: {}\n",
        path_text(&configured_nastran),
        path_text(&configured_patran)
    ));
    fs::write(&configured_config, configured_text).expect("configured overlay can be updated");
    let configured_output = output_root.join("configured-tools");
    let configured_args = vec![
        "--config".to_owned(),
        path_text(&configured_config),
        "--no-optimize".to_owned(),
        "--no-baseline".to_owned(),
        "--no-mission".to_owned(),
        "--no-parallel".to_owned(),
        "--quiet".to_owned(),
        "--output".to_owned(),
        path_text(&configured_output),
    ];
    let configured = child_output(&executable, &sandbox.path, &user_data, &configured_args);
    assert_success(&configured, "configured-tool path");
    assert!(configured_output.join("structures").is_dir());
}
