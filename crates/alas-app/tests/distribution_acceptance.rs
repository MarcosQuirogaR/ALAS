// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Isolated packaged-executable acceptance checks for the public CLI.
//!
//! These tests copy the actual `alas` binary into a temporary installation and
//! run it with no repository working directory. That catches path assumptions
//! which library tests cannot see: the optional tools and persisted preferences
//! must be found beside the executable or in user data, while an absent tool
//! must remain an actionable run status rather than a process failure.

// This integration binary uses assertions and fallible setup to report the
// packaged runtime's exact failure, rather than hiding it behind a helper.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use alas_report::RESULT_FIGURES;

const PRESETS: &[&str] = &[
    "AVE", "A340-300", "A380-800", "B787-9", "A320-200", "A220-300", "DC-10",
];

struct UnlaunchableTools {
    #[cfg(windows)]
    _guards: Vec<fs::File>,
}

fn write_unlaunchable_tools(directory: &Path) -> UnlaunchableTools {
    fs::create_dir_all(directory).expect("stand-in tool directory is created");
    for name in ["mset.exe", "mses.exe", "mplot.exe"] {
        fs::write(directory.join(name), b"not an executable").expect("tool stand-in exists");
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        let guards = ["mset.exe", "mses.exe", "mplot.exe"]
            .into_iter()
            .map(|name| {
                fs::OpenOptions::new()
                    .read(true)
                    .share_mode(0)
                    .open(directory.join(name))
                    .expect("exclusive stand-in handle is opened")
            })
            .collect();
        UnlaunchableTools { _guards: guards }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        for name in ["mset.exe", "mses.exe", "mplot.exe"] {
            let path = directory.join(name);
            let mut permissions = fs::metadata(&path)
                .expect("stand-in metadata is readable")
                .permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(path, permissions).expect("execute permission is removed");
        }
        UnlaunchableTools {}
    }
}

struct IsolatedPackage {
    root: PathBuf,
    user_data: PathBuf,
    executable: PathBuf,
}

impl IsolatedPackage {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let root = env::temp_dir().join(format!("alas-distribution-{stamp}"));
        let user_data = root.join("user-data");
        fs::create_dir_all(&user_data).expect("isolated package directory is created");
        fs::create_dir_all(root.join("outputs")).expect("package output directory is created");

        let source = PathBuf::from(env!("CARGO_BIN_EXE_ALAS"));
        let executable = root.join(if cfg!(windows) { "ALAS.exe" } else { "ALAS" });
        fs::copy(&source, &executable).expect("test binary is copied into the package");

        Self {
            root,
            user_data,
            executable,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .current_dir(self.root.join("isolated-working-directory"))
            .env("ALAS_APP_DIR", &self.root)
            .env("ALAS_FROZEN", "1")
            .env("LOCALAPPDATA", &self.user_data)
            .env("XDG_DATA_HOME", &self.user_data)
            .env("HOME", &self.user_data);
        fs::create_dir_all(self.root.join("isolated-working-directory"))
            .expect("isolated working directory is created");
        command
    }

    fn write_config(&self, name: &str, value: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::write(&path, value).expect("configuration is written");
        path
    }

    fn output_path(&self, name: &str) -> PathBuf {
        self.root.join("outputs").join(name)
    }
}

impl Drop for IsolatedPackage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn run(package: &IsolatedPackage, args: &[&str]) -> Output {
    package
        .command()
        .args(args)
        .output()
        .expect("packaged executable starts")
}

fn output_text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[cfg(windows)]
#[test]
fn packaged_executable_uses_the_windows_gui_subsystem() {
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_ALAS"));
    let bytes = fs::read(&executable).expect("packaged executable is readable");
    assert!(
        bytes.len() >= 0x40,
        "executable is shorter than a DOS header"
    );

    let pe_offset = u32::from_le_bytes(
        bytes[0x3c..0x40]
            .try_into()
            .expect("DOS header stores a four-byte PE offset"),
    ) as usize;
    let signature_end = pe_offset
        .checked_add(4)
        .expect("PE signature offset does not overflow");
    assert!(
        signature_end <= bytes.len(),
        "executable PE signature is outside the file"
    );
    assert_eq!(&bytes[pe_offset..signature_end], b"PE\0\0");

    // The PE optional header starts after the four-byte signature, twenty-byte
    // COFF header, and is followed by the standard `Subsystem` field at +68.
    let optional_header = pe_offset
        .checked_add(24)
        .expect("PE optional-header offset does not overflow");
    let subsystem_offset = optional_header
        .checked_add(68)
        .expect("PE subsystem offset does not overflow");
    let subsystem_end = subsystem_offset
        .checked_add(2)
        .expect("PE subsystem field does not overflow");
    assert!(
        subsystem_end <= bytes.len(),
        "executable PE subsystem field is outside the file"
    );
    let subsystem = u16::from_le_bytes(
        bytes[subsystem_offset..subsystem_end]
            .try_into()
            .expect("PE subsystem field is two bytes"),
    );
    assert_eq!(
        subsystem, 2,
        "ALAS must be linked as IMAGE_SUBSYSTEM_WINDOWS_GUI (PE subsystem 2)"
    );
}

#[test]
fn packaged_cli_invocation_preserves_diagnostics_and_failure_status() {
    let package = IsolatedPackage::new();
    let output = run(
        &package,
        &[
            "--config",
            package
                .root
                .join("missing-config.yaml")
                .to_str()
                .expect("missing config path is valid UTF-8"),
        ],
    );

    assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Configuration Error:"),
        "{}",
        output_text(&output)
    );
}

fn preset_config(preset: &str, mission_enabled: bool) -> String {
    serde_json::json!({
        "preset": preset,
        "mission": {"enabled": mission_enabled}
    })
    .to_string()
}

fn preset_tool_config(
    preset: &str,
    mission_enabled: bool,
    mses_enabled: bool,
    mses_dir: &str,
) -> String {
    serde_json::json!({
        "preset": preset,
        "mission": {"enabled": mission_enabled},
        "mses": {"enabled": mses_enabled, "mses_dir": mses_dir}
    })
    .to_string()
}

#[test]
fn packaged_help_runs_without_the_checkout() {
    let package = IsolatedPackage::new();
    let output = run(&package, &["--help"]);

    assert!(output.status.success(), "{}", output_text(&output));
    let text = output_text(&output);
    assert!(text.contains("Usage: alas"), "{text}");
    assert!(text.contains("--no-mission"), "{text}");
}

#[test]
fn persisted_tool_preferences_survive_a_packaged_restart() {
    let package = IsolatedPackage::new();
    let preferences_dir = package.user_data.join("ALAS");
    fs::create_dir_all(&preferences_dir).expect("preference directory is created");
    let preferences = serde_json::json!({
        "mses_dir": "configured/MSES",
        "nastran_exe": "configured/NASTRAN/nastran.exe",
        "patran_exe": "configured/Patran/patran.exe"
    });
    fs::write(
        preferences_dir.join("tool-preferences.json"),
        serde_json::to_string(&preferences).expect("preferences serialize"),
    )
    .expect("preferences are persisted");

    let first = package.output_path("first.yaml");
    let first_output = run(
        &package,
        &[
            "--save-config",
            first.to_str().expect("first path is valid UTF-8"),
        ],
    );
    assert!(
        first_output.status.success(),
        "{}",
        output_text(&first_output)
    );
    let first_text = fs::read_to_string(&first).expect("first effective config exists");

    let second = package.output_path("second.yaml");
    let second_output = run(
        &package,
        &[
            "--save-config",
            second.to_str().expect("second path is valid UTF-8"),
        ],
    );
    assert!(
        second_output.status.success(),
        "{}",
        output_text(&second_output)
    );
    let second_text = fs::read_to_string(&second).expect("second effective config exists");

    for expected in [
        "configured/MSES",
        "configured/NASTRAN/nastran.exe",
        "configured/Patran/patran.exe",
    ] {
        assert!(
            first_text.contains(expected),
            "first config lacks {expected}"
        );
        assert!(second_text.contains(expected), "restart lost {expected}");
    }
}

#[test]
fn packaged_no_mission_runs_every_shipped_aircraft_preset() {
    let package = IsolatedPackage::new();

    for preset in PRESETS {
        let config = package.write_config(
            &format!("{preset}-no-mission.json"),
            &preset_config(preset, false),
        );
        let output_dir = package.output_path(preset);
        let output = run(
            &package,
            &[
                "--config",
                config.to_str().expect("config path is valid UTF-8"),
                "--no-optimize",
                "--no-baseline",
                "--no-parallel",
                "--no-mission",
                "--quiet",
                "--output",
                output_dir.to_str().expect("output path is valid UTF-8"),
            ],
        );
        assert!(
            output.status.success(),
            "packaged no-mission run failed for {preset}: {}",
            output_text(&output)
        );
        assert!(
            output_dir.join("design_database.json").is_file(),
            "no-mission run did not export a design database for {preset}"
        );
        let text = output_text(&output);
        assert!(
            !text.contains("--- MSES"),
            "quiet run printed MSES output: {text}"
        );
        assert!(
            !text.contains("--- Structural"),
            "quiet run printed structural output: {text}"
        );
        assert!(
            !text.contains("Saved "),
            "quiet run printed plot output: {text}"
        );
    }
}

#[test]
fn packaged_plot_manifest_covers_every_registered_result_figure() {
    let package = IsolatedPackage::new();
    let config = package.write_config("plots.json", &preset_config("AVE", false));
    let output_dir = package.output_path("plots");
    let output = run(
        &package,
        &[
            "--config",
            config.to_str().expect("config path is valid UTF-8"),
            "--no-optimize",
            "--no-baseline",
            "--no-parallel",
            "--no-mission",
            "--plots",
            "--quiet",
            "--output",
            output_dir.to_str().expect("output path is valid UTF-8"),
        ],
    );

    assert!(
        output.status.success(),
        "packaged plot export failed: {}",
        output_text(&output)
    );
    let manifest_path = output_dir.join("plots/plot_manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path).expect("plot manifest exists");
    let manifest: Vec<serde_json::Value> =
        serde_json::from_str(&manifest_text).expect("plot manifest is valid JSON");
    let registry_ids = RESULT_FIGURES
        .iter()
        .map(|descriptor| descriptor.id)
        .collect::<BTreeSet<_>>();
    let manifest_ids = manifest
        .iter()
        .map(|entry| entry["id"].as_str().expect("manifest entry has an id"))
        .collect::<BTreeSet<_>>();

    assert_eq!(manifest.len(), RESULT_FIGURES.len());
    assert_eq!(manifest_ids, registry_ids);
    for entry in &manifest {
        let id = entry["id"].as_str().expect("manifest entry has an id");
        let descriptor = RESULT_FIGURES
            .iter()
            .find(|descriptor| descriptor.id == id)
            .expect("manifest id belongs to the result registry");
        assert_eq!(entry["title"].as_str(), Some(descriptor.title));
        assert!(
            entry["required_stage"]
                .as_str()
                .is_some_and(|stage| !stage.is_empty()),
            "manifest entry has no required stage: {id}"
        );
        match entry["status"].as_str() {
            Some("written") => {
                let relative_path = entry["path"]
                    .as_str()
                    .expect("written manifest entry has a path");
                assert!(
                    output_dir.join("plots").join(relative_path).is_file(),
                    "written plot is missing: {relative_path}"
                );
                assert!(entry["reason"].is_null());
            }
            Some("unavailable") => {
                assert!(entry["path"].is_null());
                assert!(entry["reason"]
                    .as_str()
                    .is_some_and(|reason| !reason.is_empty()));
            }
            status => panic!("unexpected plot status for {id}: {status:?}"),
        }
    }
}

#[test]
fn packaged_mission_enabled_path_completes_for_every_shipped_preset() {
    let package = IsolatedPackage::new();

    for preset in PRESETS {
        let config = package.write_config(
            &format!("{preset}-mission.json"),
            &preset_config(preset, true),
        );
        let output_dir = package.output_path(&format!("{preset}-mission"));
        let output = run(
            &package,
            &[
                "--config",
                config.to_str().expect("config path is valid UTF-8"),
                "--no-optimize",
                "--no-baseline",
                "--no-parallel",
                "--quiet",
                "--output",
                output_dir.to_str().expect("output path is valid UTF-8"),
            ],
        );

        assert!(
            output.status.success(),
            "packaged mission-enabled run failed for {preset}: {}",
            output_text(&output)
        );
        assert!(
            output_dir.join("design_database.json").is_file(),
            "mission-enabled run did not complete its public export for {preset}"
        );
    }
}

#[test]
fn packaged_missing_tool_path_reports_unavailability_without_aborting_the_run() {
    let package = IsolatedPackage::new();
    let config = package.write_config(
        "missing-tool.json",
        &preset_tool_config(
            "AVE",
            false,
            true,
            &package.root.join("does-not-exist").to_string_lossy(),
        ),
    );
    let output = run(
        &package,
        &[
            "--config",
            config.to_str().expect("config path is valid UTF-8"),
            "--no-optimize",
            "--no-baseline",
            "--no-parallel",
            "--output",
            package
                .output_path("missing-tool")
                .to_str()
                .expect("output path is valid UTF-8"),
        ],
    );

    assert!(output.status.success(), "{}", output_text(&output));
    let text = output_text(&output);
    assert!(text.contains("MSES analysis: absent"), "{text}");
    assert!(text.contains("not configured"), "{text}");
}

#[test]
fn packaged_adjacent_and_configured_tool_paths_are_resolved_from_the_install() {
    let package = IsolatedPackage::new();
    let adjacent = package.root.join("external tools/MSES");
    let _adjacent_tools = write_unlaunchable_tools(&adjacent);

    let adjacent_config = package.write_config(
        "adjacent-tool.json",
        &preset_tool_config("AVE", false, true, ""),
    );
    let adjacent_output = run(
        &package,
        &[
            "--config",
            adjacent_config
                .to_str()
                .expect("config path is valid UTF-8"),
            "--no-optimize",
            "--no-baseline",
            "--no-parallel",
            "--output",
            package
                .output_path("adjacent-tool")
                .to_str()
                .expect("output path is valid UTF-8"),
        ],
    );
    assert!(
        adjacent_output.status.success(),
        "{}",
        output_text(&adjacent_output)
    );
    let adjacent_text = output_text(&adjacent_output);
    assert!(
        adjacent_text.contains("MSES analysis: launch_failure"),
        "{adjacent_text}"
    );
    assert!(
        adjacent_text.contains("external tools\\MSES")
            || adjacent_text.contains("external tools/MSES"),
        "{adjacent_text}"
    );

    let configured = package.root.join("configured/MSES");
    let _configured_tools = write_unlaunchable_tools(&configured);
    let configured_path = configured.to_string_lossy().replace('\\', "/");
    let configured_config = package.write_config(
        "configured-tool.json",
        &preset_tool_config("AVE", false, true, &configured_path),
    );
    let configured_output = run(
        &package,
        &[
            "--config",
            configured_config
                .to_str()
                .expect("config path is valid UTF-8"),
            "--no-optimize",
            "--no-baseline",
            "--no-parallel",
            "--output",
            package
                .output_path("configured-tool")
                .to_str()
                .expect("output path is valid UTF-8"),
        ],
    );
    assert!(
        configured_output.status.success(),
        "{}",
        output_text(&configured_output)
    );
    let configured_text = output_text(&configured_output);
    assert!(
        configured_text.contains("MSES analysis: launch_failure"),
        "{configured_text}"
    );
    assert!(
        configured_text.contains("configured/MSES"),
        "{configured_text}"
    );
}
