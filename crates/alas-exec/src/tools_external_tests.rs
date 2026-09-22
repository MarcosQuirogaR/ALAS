// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Installed-tool discovery regression tests kept apart from the locator API.

use std::fs;
use std::path::Path;

use super::*;

#[test]
fn openvsp_and_vspaero_require_their_own_executables() {
    let root = std::env::temp_dir().join(format!("alas-openvsp-tools-{}", std::process::id()));
    let directory = root.join("external tools/OpenVSP-3.51.2-win64");
    let _ = fs::create_dir_all(&directory);
    let locator = ToolLocator::new(&root, root.join("prefs"));
    assert!(matches!(
        locator.discover_openvsp(Path::new("")),
        ExecutableDiscovery::Incomplete { .. }
    ));
    let runner = directory.join("vspscript.exe");
    let solver = directory.join("vspaero.exe");
    let _ = fs::write(&runner, b"headless runner");
    assert_eq!(
        locator.discover_openvsp(Path::new("")),
        ExecutableDiscovery::Ready(runner)
    );
    assert!(matches!(
        locator.discover_vspaero(Path::new("")),
        ExecutableDiscovery::Incomplete { .. }
    ));
    let _ = fs::write(&solver, b"native solver");
    assert_eq!(
        locator.discover_vspaero(Path::new("")),
        ExecutableDiscovery::Ready(solver)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn configured_openvsp_directory_resolves_both_native_programs() {
    let root = std::env::temp_dir().join(format!("alas-openvsp-configured-{}", std::process::id()));
    let directory = root.join("standalone/OpenVSP");
    let runner = directory.join("vspscript.exe");
    let solver = directory.join("vspaero.exe");
    let _ = fs::create_dir_all(&directory);
    let _ = fs::write(&runner, b"headless runner");
    let _ = fs::write(&solver, b"native solver");
    let locator = ToolLocator::new(root.join("app"), root.join("prefs"));
    let environment = locator.resolve_environment(
        Path::new(""),
        Path::new(""),
        Path::new(""),
        &directory,
        Path::new(""),
    );
    assert_eq!(environment.openvsp_exe, Some(runner));
    assert_eq!(environment.vspaero_exe, Some(solver));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn avl_discovery_accepts_adjacent_versioned_binaries() {
    let root = std::env::temp_dir().join(format!("alas-avl-versioned-{}", std::process::id()));
    let executable = root.join("external tools/avl352.exe");
    let _ = fs::create_dir_all(executable.parent().unwrap_or(Path::new(".")));
    let _ = fs::write(&executable, b"native solver");
    let locator = ToolLocator::new(&root, root.join("prefs"));
    assert_eq!(
        locator.discover_avl(Path::new("")),
        ExecutableDiscovery::Ready(executable)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn versioned_mses_distribution_is_discovered_without_a_hard_coded_release() {
    let root = std::env::temp_dir().join(format!("alas-versioned-mses-{}", std::process::id()));
    let directory = root.join("external tools/Mses3.12c-win32");
    let _ = fs::create_dir_all(&directory);
    for name in ["mset.exe", "mses.exe", "mplot.exe"] {
        let _ = fs::write(directory.join(name), b"test");
    }
    let locator = ToolLocator::new(&root, root.join("prefs"));
    assert_eq!(
        locator.discover_mses(Path::new("external tools/MSES")),
        MsesDiscovery::Ready {
            directory: directory.clone(),
            source: MsesSource::Adjacent { root: root.clone() },
        }
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stale_mses_preference_falls_back_to_the_checkout_and_keeps_provenance() -> Result<(), &'static str> {
    let root = std::env::temp_dir().join(format!("alas-relocated-mses-{}", std::process::id()));
    let app_root = root.join("target/release");
    let directory = root.join("external tools/Mses3.12c-win32");
    let stale = root.join("old-checkout/external tools/Mses3.12c-win32");
    let _ = fs::create_dir_all(&directory);
    let _ = fs::write(root.join("Cargo.toml"), b"[package]\nname = \"fixture\"\n");
    for name in ["mset.exe", "mses.exe", "mplot.exe"] {
        let _ = fs::write(directory.join(name), b"test");
    }
    let locator = ToolLocator::new(&app_root, root.join("prefs"));

    let discovery = locator.discover_mses(&stale);
    assert_eq!(
        discovery,
        MsesDiscovery::Ready {
            directory: directory.clone(),
            source: MsesSource::Adjacent { root: root.clone() },
        }
    );
    let warning = discovery
        .fallback_warning()
        .ok_or("a stale preference must be observable as a fallback")?;
    assert!(warning.contains("using adjacent installation"), "{warning}");
    assert!(warning.contains("Mses3.12c-win32"), "{warning}");
    assert!(
        warning.contains(
            root.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref()
        ),
        "{warning}"
    );
    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn valid_mses_preference_wins_over_an_adjacent_bundle() {
    let root = std::env::temp_dir().join(format!("alas-configured-mses-{}", std::process::id()));
    let configured = root.join("configured/MSES");
    let adjacent = root.join("external tools/Mses3.12c-win32");
    let _ = fs::create_dir_all(&configured);
    let _ = fs::create_dir_all(&adjacent);
    for name in ["mset.exe", "mses.exe", "mplot.exe"] {
        let _ = fs::write(configured.join(name), b"configured");
        let _ = fs::write(adjacent.join(name), b"adjacent");
    }
    let locator = ToolLocator::new(&root, root.join("prefs"));

    assert_eq!(
        locator.discover_mses(&configured),
        MsesDiscovery::Ready {
            directory: configured,
            source: MsesSource::Configured,
        }
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn student_edition_layout_resolves_nastran_and_patran_separately() {
    let root = std::env::temp_dir().join(format!("alas-msc-layout-{}", std::process::id()));
    let edition = root.join("20261");
    let nastran = edition.join("Nastran/bin/nastran.exe");
    let patran = edition.join("Patran/bin/patran.exe");
    let solver =
        edition.join("Patran/mscnastran_files/20261/servermode/msc20261/win64i8/analysis.exe");
    let _ = fs::create_dir_all(nastran.parent().unwrap_or(Path::new(".")));
    let _ = fs::create_dir_all(patran.parent().unwrap_or(Path::new(".")));
    let _ = fs::create_dir_all(solver.parent().unwrap_or(Path::new(".")));
    let _ = fs::write(&nastran, b"test");
    let _ = fs::write(&patran, b"test");
    let _ = fs::write(&solver, b"test");
    let mut locator = ToolLocator::new(root.join("app"), root.join("prefs"));
    locator.system_tool_roots = vec![edition];
    assert_eq!(
        locator.discover_nastran(Path::new("")),
        ExecutableDiscovery::Ready(nastran)
    );
    assert_eq!(
        locator.discover_patran(Path::new("")),
        ExecutableDiscovery::Ready(patran)
    );
    let environment = locator.resolve_environment(
        Path::new(""),
        Path::new(""),
        Path::new(""),
        Path::new(""),
        Path::new(""),
    );
    assert_eq!(environment.nastran_solver, Some(solver));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn automatic_student_edition_resolution_prefers_the_versioned_nastran_launcher() {
    let root = std::env::temp_dir().join(format!("alas-msc-inner-launcher-{}", std::process::id()));
    let edition = root.join("20261");
    let visible = edition.join("Nastran/bin/nastran.exe");
    let inner = edition.join("Nastran/msc20261/win64i8/nastran.exe");
    let solver =
        edition.join("Patran/mscnastran_files/20261/servermode/msc20261/win64i8/analysis.exe");
    let _ = fs::create_dir_all(visible.parent().unwrap_or(Path::new(".")));
    let _ = fs::create_dir_all(inner.parent().unwrap_or(Path::new(".")));
    let _ = fs::create_dir_all(solver.parent().unwrap_or(Path::new(".")));
    let _ = fs::write(&visible, b"visible");
    let _ = fs::write(&inner, b"inner");
    let _ = fs::write(&solver, b"solver");

    let mut locator = ToolLocator::new(root.join("app"), root.join("prefs"));
    locator.system_tool_roots = vec![edition];
    assert_eq!(
        locator.discover_nastran(Path::new("")),
        ExecutableDiscovery::Ready(inner)
    );
    let environment = locator.resolve_environment(
        Path::new(""),
        Path::new(""),
        Path::new(""),
        Path::new(""),
        Path::new(""),
    );
    assert_eq!(environment.nastran_solver, Some(solver));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn preferences_keep_all_nastran_paths_and_openvsp_directory() {
    let root = std::env::temp_dir().join(format!("alas-prefs-{}", std::process::id()));
    let locator = ToolLocator::new(root.join("app"), &root);
    let expected = ToolPreferences {
        mses_dir: Some("C:/MSES".to_owned()),
        nastran_exe: Some("C:/nastran.exe".to_owned()),
        nastran_solver: Some("C:/analysis.exe".to_owned()),
        nastran95_dir: Some("C:/nastran-95".to_owned()),
        nastran95_runtime: Some("C:/msys64/mingw64/bin".to_owned()),
        nastran95_rf_stage: Some("C:/nas-rf".to_owned()),
        nastran95_open_core_words: Some("32000000".to_owned()),
        patran_exe: None,
        openvsp_dir: Some("C:/OpenVSP".to_owned()),
        avl_exe: Some("C:/AVL/avl.exe".to_owned()),
        navdata_dir: Some("C:/ALAS/navdata".to_owned()),
        routes_dir: Some("C:/ALAS/routes".to_owned()),
        flowunsteady_exe: Some("C:/FLOWUnsteady/launch.exe".to_owned()),
    };
    assert!(locator.save_preferences(&expected).is_ok());
    assert_eq!(locator.load_preferences(), expected);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn older_preferences_without_new_optional_locations_remain_readable() {
    let preferences: ToolPreferences = serde_json::from_str(r#"{"mses_dir":null,"nastran_exe":"C:/nastran.exe","patran_exe":"C:/patran.exe","avl_exe":null}"#).unwrap_or_default();
    assert_eq!(preferences.nastran_exe.as_deref(), Some("C:/nastran.exe"));
    assert_eq!(preferences.nastran_solver, None);
    assert_eq!(preferences.nastran95_dir, None);
    assert_eq!(preferences.nastran95_runtime, None);
    assert_eq!(preferences.nastran95_rf_stage, None);
    assert_eq!(preferences.nastran95_open_core_words, None);
    assert_eq!(preferences.openvsp_dir, None);
    assert_eq!(preferences.navdata_dir, None);
    assert_eq!(preferences.routes_dir, None);
}

#[test]
fn preference_replacement_leaves_no_partially_written_file() {
    let root = std::env::temp_dir().join(format!("alas-atomic-prefs-{}", std::process::id()));
    let locator = ToolLocator::new(root.join("app"), &root);
    let original = ToolPreferences {
        mses_dir: Some("C:/old/MSES".to_owned()),
        ..ToolPreferences::default()
    };
    let replacement = ToolPreferences {
        routes_dir: Some("C:/new/routes".to_owned()),
        ..ToolPreferences::default()
    };

    assert!(locator.save_preferences(&original).is_ok());
    assert!(locator.save_preferences(&replacement).is_ok());
    assert_eq!(locator.load_preferences(), replacement);
    assert!(!root
        .join(format!(".tool-preferences-{}.tmp", std::process::id()))
        .exists());
    let _ = fs::remove_dir_all(root);
}
