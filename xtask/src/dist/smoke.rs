// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Isolated packaged CLI smoke checks with the bundled AVL cross-check.

use std::{env, fs, path::Path, process::Command};

use super::{path_arg, validate_bundled_mses_osmap, validate_release_manifest, BundleStatus};

const NATIVE_OVERLAY: &str = r#"{
  "preset": "AVE",
  "mses": {"enabled": false},
  "structures": {"run_nastran": false, "run_patran_export": false}
}"#;

/// Isolate machine preferences and inherited solver overrides for a child CLI.
pub(super) fn isolated_command(exe: &Path, user_data: &Path) -> Command {
    let mut command = Command::new(exe);
    command
        .env("ALAS_APP_DIR", exe.parent().unwrap_or(Path::new(".")))
        .env("ALAS_FROZEN", "1")
        .env("LOCALAPPDATA", user_data)
        .env("XDG_DATA_HOME", user_data)
        .env("HOME", user_data)
        .env_remove("ALAS_TOOL_DISCOVERY")
        .env_remove("ALAS_NASTRAN95_DIR")
        .env_remove("ALAS_FLOWUNSTEADY_EXE");
    command
}

fn export_overlay(
    exe: &Path,
    source: &Path,
    destination: &Path,
    user_data: &Path,
) -> Result<(), String> {
    let status = isolated_command(exe, user_data)
        .env("ALAS_TOOL_DISCOVERY", "disabled")
        .arg("--config")
        .arg(source)
        .arg("--save-config")
        .arg(destination)
        .status()
        .map_err(|error| format!("failed to export smoke configuration: {error}"))?;
    if !status.success() || !destination.is_file() {
        return Err("packaged binary failed to read/export smoke configuration".to_owned());
    }
    Ok(())
}

fn pin_bundled_avl(package_root: &Path, user_data: &Path) -> Result<(), String> {
    let executable = fs::canonicalize(package_root.join("external tools/avl352.exe"))
        .map_err(|error| format!("bundled AVL executable is missing: {error}"))?;
    let preferences_root = if cfg!(target_os = "macos") {
        user_data.join("Library/Application Support/ALAS")
    } else {
        user_data.join("ALAS")
    };
    fs::create_dir_all(&preferences_root)
        .map_err(|error| format!("cannot create isolated AVL preferences: {error}"))?;
    let preferences = serde_json::json!({"avl_exe": executable});
    fs::write(
        preferences_root.join("tool-preferences.json"),
        preferences.to_string(),
    )
    .map_err(|error| format!("cannot pin bundled AVL preferences: {error}"))
}

pub(super) fn validate_distribution(
    exe: &Path,
    packaged_config: &Path,
    release_manifest: &Path,
    dist_root: &Path,
    avl_status: &BundleStatus,
) -> Result<(), String> {
    // 1. Verify --help output.
    let package_root = exe.parent().unwrap_or(dist_root);
    let output = Command::new(exe)
        .arg("--help")
        .current_dir(package_root)
        .env("ALAS_FROZEN", "1")
        .output()
        .map_err(|e| format!("failed to execute packaged binary: {e}"))?;

    if !output.status.success() {
        return Err("packaged binary --help failed".to_owned());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains("Usage: alas") {
        return Err("unexpected --help output from packaged binary".to_owned());
    }
    println!("  [OK] Standalone binary execution (--help)");

    // 2. Verify the manifest and the generated package configuration before
    // invoking the more expensive headless/AVL smoke run.
    validate_release_manifest(release_manifest, package_root)?;
    validate_bundled_mses_osmap(package_root)?;
    let packaged_config_metadata = fs::metadata(packaged_config).map_err(|e| {
        format!(
            "packaged configuration template is missing at {}: {e}",
            packaged_config.display()
        )
    })?;
    if packaged_config_metadata.len() == 0 {
        return Err(format!(
            "packaged configuration template is empty: {}",
            packaged_config.display()
        ));
    }
    println!("  [OK] Release manifest and packaged configuration template");

    // 3. Verify a save/read config round-trip and a writable output path that
    // contains spaces. This catches path handling that a repository-local
    // output directory would hide.
    let smoke_root = env::temp_dir().join(format!("ALAS package smoke {}", std::process::id()));
    let _ = fs::remove_dir_all(&smoke_root);
    fs::create_dir_all(&smoke_root)
        .map_err(|e| format!("failed to create portable smoke directory: {e}"))?;
    let user_data = smoke_root.join("runtime user-data");
    let shipped_roundtrip = smoke_root.join("shipped config roundtrip.yaml");
    export_overlay(exe, packaged_config, &shipped_roundtrip, &user_data)?;
    if fs::read(packaged_config).map_err(|error| error.to_string())?
        != fs::read(&shipped_roundtrip).map_err(|error| error.to_string())?
    {
        return Err("packaged configuration changed across read/export round-trip".to_owned());
    }
    let native_overlay = smoke_root.join("native overlay.json");
    fs::write(&native_overlay, NATIVE_OVERLAY).map_err(|error| error.to_string())?;
    let native_config = smoke_root.join("native roundtrip.yaml");
    export_overlay(exe, &native_overlay, &native_config, &user_data)?;
    if avl_status.status == "bundled" {
        pin_bundled_avl(package_root, &user_data)?;
    }
    let temp_out = smoke_root.join("run output");
    let config_text = path_arg(&native_config)?;
    let output_text = path_arg(&temp_out)?;
    let status = isolated_command(exe, &user_data)
        .args([
            "--config",
            &config_text,
            "--no-optimize",
            "--no-mission",
            "--plots",
            "--quiet",
            "--output",
            &output_text,
        ])
        .current_dir(package_root)
        .status()
        .map_err(|e| format!("failed to execute config round-trip smoke run: {e}"))?;

    if !status.success() {
        return Err("packaged binary config round-trip smoke run failed".to_owned());
    }

    // The AVL cross-check only runs when this package actually bundles an
    // executable for it (see `bundle_avl`): today that is the Windows
    // package alone. Every other target ran the smoke analysis above through
    // ALAS's own analytical vortex-lattice stage with no AVL executable
    // configured, so the total-force files and the "Athena AVL" overlay this
    // block would otherwise demand never exist; requiring them there would
    // fail every non-Windows package regardless of whether packaging itself
    // is correct.
    let avl_force_count = if avl_status.status == "bundled" {
        let avl_dir = temp_out.join("avl");
        let count = fs::read_dir(&avl_dir)
            .map_err(|e| format!("packaged AVL output directory is missing: {e}"))?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "ft")
            })
            .count();
        if count == 0 {
            return Err("packaged run did not produce AVL total-force files".to_owned());
        }
        let comparison = temp_out.join("plots/model_comparison.svg");
        let comparison_text = fs::read_to_string(&comparison)
            .map_err(|e| format!("packaged Model Comparison figure is missing: {e}"))?;
        if !comparison_text.contains("Athena AVL") {
            return Err("packaged Model Comparison figure does not name Athena AVL".to_owned());
        }
        Some(count)
    } else {
        let comparison = temp_out.join("plots/model_comparison.svg");
        if !comparison.is_file() {
            return Err("packaged Model Comparison figure is missing".to_owned());
        }
        None
    };

    // The shipped template is read/exported above with empty discovery. The
    // native JSON overlay is also run directly to cover both input formats.
    let packaged_out = smoke_root.join("native overlay output");
    let packaged_config_text = path_arg(&native_overlay)?;
    let packaged_output_text = path_arg(&packaged_out)?;
    let packaged_status = isolated_command(exe, &user_data)
        .args([
            "--config",
            &packaged_config_text,
            "--no-optimize",
            "--no-mission",
            "--plots",
            "--quiet",
            "--output",
            &packaged_output_text,
        ])
        .current_dir(package_root)
        .status()
        .map_err(|e| format!("failed to execute native overlay smoke run: {e}"))?;
    if !packaged_status.success() {
        return Err("packaged binary failed to run the native smoke overlay".to_owned());
    }
    validate_release_manifest(release_manifest, package_root)?;
    match avl_force_count {
        Some(count) => println!(
            "  [OK] Config round-trip, shipped config read, and writable path-with-spaces; bundled AVL completed with {count} total-force files and Model Comparison overlay"
        ),
        None => println!(
            "  [OK] Config round-trip, shipped config read, and writable path-with-spaces; no bundled AVL for this target, Model Comparison used the analytical fallback"
        ),
    }

    let _ = fs::remove_dir_all(&smoke_root);
    println!("  [OK] Isolated headless execution test passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, ffi::OsStr, path::Path};

    use super::{isolated_command, NATIVE_OVERLAY};

    #[test]
    fn smoke_child_isolates_preferences_and_inherited_solver_overrides() {
        let command = isolated_command(Path::new("package/ALAS"), Path::new("profile"));
        let environment: BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(
            environment[OsStr::new("ALAS_APP_DIR")],
            Some(OsStr::new("package"))
        );
        for variable in ["LOCALAPPDATA", "XDG_DATA_HOME", "HOME"] {
            assert_eq!(
                environment[OsStr::new(variable)],
                Some(OsStr::new("profile"))
            );
        }
        for variable in [
            "ALAS_TOOL_DISCOVERY",
            "ALAS_NASTRAN95_DIR",
            "ALAS_FLOWUNSTEADY_EXE",
        ] {
            assert_eq!(environment[OsStr::new(variable)], None);
        }
    }

    #[test]
    fn smoke_overlay_keeps_native_structures_without_external_solver_requests() {
        let overlay: serde_json::Value = serde_json::from_str(NATIVE_OVERLAY).unwrap();
        assert_eq!(overlay["structures"]["run_nastran"], false);
        assert_eq!(overlay["structures"]["run_patran_export"], false);
        assert_eq!(overlay["mses"]["enabled"], false);
        assert!(overlay["structures"].get("enabled").is_none());
    }
}
