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
fn stale_mses_preference_falls_back_to_the_checkout_and_keeps_provenance(
) -> Result<(), &'static str> {
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
fn napa_program_files_discovery_selects_newest_numeric_version() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!("alas-napa-versions-{}", std::process::id()));
    let program_files = root.join("Program Files");
    let other_program_files = root.join("Program Files (x86)");
    let app_data = root.join("AppData/Roaming");
    let editions = program_files.join("MSC.Software/NaPa_SE");
    for version in ["9999", "10000", "current"] {
        for tool in ["Nastran", "Patran"] {
            let directory = editions.join(version).join(tool).join("bin");
            fs::create_dir_all(&directory)?;
            fs::write(
                directory.join(format!("{}.exe", tool.to_lowercase())),
                b"test",
            )?;
        }
    }
    let newest = other_program_files.join("MSC.Software/NaPa_SE/10001");
    let legacy = app_data.join("MSC.Software/MSC Nastran and Patran Student Editions/9998");
    for edition in [&newest, &legacy] {
        for tool in ["Nastran", "Patran"] {
            let directory = edition.join(tool).join("bin");
            fs::create_dir_all(&directory)?;
            fs::write(
                directory.join(format!("{}.exe", tool.to_lowercase())),
                b"test",
            )?;
        }
    }
    fs::write(editions.join("10002"), b"not a directory")?;
    let inner = newest.join("Nastran/msc10001/win64i8");
    fs::create_dir_all(&inner)?;
    fs::write(inner.join("nastran.exe"), b"inner")?;
    let solver =
        newest.join("Patran/mscnastran_files/10001/servermode/msc10001/win64i8/analysis.exe");
    fs::create_dir_all(solver.parent().unwrap_or(Path::new(".")))?;
    fs::write(&solver, b"solver")?;

    let mut locator = ToolLocator::new(root.join("app"), root.join("prefs"));
    locator.system_tool_roots = msc_roots_from_locations(
        &[program_files.clone(), program_files, other_program_files],
        Some(&app_data),
    );
    assert_eq!(
        locator.system_tool_roots,
        vec![
            newest.clone(),
            editions.join("10000"),
            editions.join("9999"),
            legacy
        ]
    );
    assert_eq!(
        locator.discover_nastran(Path::new("")),
        ExecutableDiscovery::Ready(newest.join("Nastran/bin/nastran.exe"))
    );
    assert_eq!(
        locator.discover_patran(Path::new("")),
        ExecutableDiscovery::Ready(newest.join("Patran/bin/patran.exe"))
    );
    let environment = locator.resolve_environment(
        Path::new(""),
        Path::new(""),
        Path::new(""),
        Path::new(""),
        Path::new(""),
    );
    assert_eq!(environment.nastran_solver, Some(solver));
    fs::remove_dir_all(root)
}

#[test]
fn configured_msc_files_and_bin_directories_keep_priority() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!("alas-napa-configured-{}", std::process::id()));
    let program_files = root.join("Program Files");
    let mut locator = ToolLocator::new(root.join("app"), root.join("prefs"));
    for tool in ["Nastran", "Patran"] {
        let filename = format!("{}.exe", tool.to_lowercase());
        for directory in [
            program_files
                .join("MSC.Software/NaPa_SE/20261")
                .join(tool)
                .join("bin"),
            root.join("configured").join(tool).join("bin"),
            root.join("app/external tools"),
        ] {
            fs::create_dir_all(&directory)?;
            fs::write(directory.join(&filename), b"test")?;
        }
    }
    locator.system_tool_roots = msc_roots_from_locations(&[program_files], None);
    let nastran = root.join("configured/Nastran/bin/nastran.exe");
    let patran = root.join("configured/Patran/bin/patran.exe");
    for configured in [&nastran, &root.join("configured/Nastran/bin")] {
        assert_eq!(
            locator.discover_nastran(configured),
            ExecutableDiscovery::Ready(nastran.clone())
        );
    }
    for configured in [&patran, &root.join("configured/Patran/bin")] {
        assert_eq!(
            locator.discover_patran(configured),
            ExecutableDiscovery::Ready(patran.clone())
        );
    }
    fs::remove_dir_all(root)
}

#[test]
fn incomplete_newest_msc_installation_falls_back_to_complete_version() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!("alas-napa-incomplete-{}", std::process::id()));
    let program_files = root.join("Program Files");
    let editions = program_files.join("MSC.Software/NaPa_SE");
    for tool in ["Nastran", "Patran"] {
        fs::create_dir_all(editions.join("20262").join(tool).join("bin"))?;
        let directory = editions.join("20261").join(tool).join("bin");
        fs::create_dir_all(&directory)?;
        fs::write(
            directory.join(format!("{}.exe", tool.to_lowercase())),
            b"test",
        )?;
    }
    let mut locator = ToolLocator::new(root.join("app"), root.join("prefs"));
    locator.system_tool_roots = msc_roots_from_locations(&[program_files], None);
    assert_eq!(
        locator.discover_nastran(Path::new("")),
        ExecutableDiscovery::Ready(editions.join("20261/Nastran/bin/nastran.exe"))
    );
    assert_eq!(
        locator.discover_patran(Path::new("")),
        ExecutableDiscovery::Ready(editions.join("20261/Patran/bin/patran.exe"))
    );
    fs::remove_dir_all(root)
}

fn write_msc_fixture(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    fs::write(path, b"test")
}

fn resolve_msc_fixture(locator: &ToolLocator, configured: &Path) -> RunEnvironment {
    locator.resolve_environment(
        Path::new(""),
        configured,
        Path::new(""),
        Path::new(""),
        Path::new(""),
    )
}

#[test]
fn solver_only_newer_edition_cannot_override_the_selected_launcher_pair() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!("alas-msc-solver-only-{}", std::process::id()));
    let older = root.join("NaPa_SE/20261");
    let newer = root.join("NaPa_SE/20262");
    let launcher = older.join("Nastran/bin/nastran.exe");
    let solver = older.join("Patran/servermode/win64i8/analysis.exe");
    write_msc_fixture(&launcher)?;
    write_msc_fixture(&solver)?;
    write_msc_fixture(&newer.join("Patran/servermode/win64i8/analysis.exe"))?;
    let mut locator = ToolLocator::new(root.join("app"), root.join("prefs"));
    locator.system_tool_roots = vec![newer, older];

    let environment = resolve_msc_fixture(&locator, Path::new(""));
    assert_eq!(environment.nastran_exe, Some(launcher));
    assert_eq!(environment.nastran_solver, Some(solver.clone()));
    assert_eq!(
        locator.discover_nastran_solver(Path::new("")),
        ExecutableDiscovery::Ready(solver)
    );
    fs::remove_dir_all(root)
}

#[test]
fn launcher_only_newer_edition_cannot_borrow_an_older_solver() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!("alas-msc-launcher-only-{}", std::process::id()));
    let older = root.join("NaPa_SE/20261");
    let newer = root.join("NaPa_SE/20262");
    let launcher = newer.join("Nastran/bin/nastran.exe");
    write_msc_fixture(&launcher)?;
    fs::create_dir_all(newer.join("Patran"))?;
    write_msc_fixture(&older.join("Nastran/bin/nastran.exe"))?;
    write_msc_fixture(&older.join("Patran/servermode/win64i8/analysis.exe"))?;
    let mut locator = ToolLocator::new(root.join("app"), root.join("prefs"));
    locator.system_tool_roots = vec![newer.clone(), older];

    let environment = resolve_msc_fixture(&locator, Path::new(""));
    assert_eq!(environment.nastran_exe, Some(launcher));
    assert_eq!(environment.nastran_solver, None);
    assert_eq!(
        locator.discover_nastran_solver(Path::new("")),
        ExecutableDiscovery::Incomplete {
            directory: newer,
            missing: vec!["servermode/analysis.exe".to_owned()],
        }
    );
    fs::remove_dir_all(root)
}

#[test]
fn configured_msc_launchers_keep_their_own_installation_solver() -> std::io::Result<()> {
    let root =
        std::env::temp_dir().join(format!("alas-msc-configured-pair-{}", std::process::id()));
    let installed = root.join("NaPa_SE/20262");
    let configured = root.join("configured/20261");
    let launcher = configured.join("Nastran/bin/nastran.exe");
    let inner = configured.join("Nastran/msc20261/win64i8/nastran.exe");
    let solver = configured.join("Patran/servermode/win64i8/analysis.exe");
    for file in [
        &launcher,
        &inner,
        &solver,
        &installed.join("Nastran/bin/nastran.exe"),
        &installed.join("Patran/servermode/win64i8/analysis.exe"),
    ] {
        write_msc_fixture(file)?;
    }
    let mut locator = ToolLocator::new(root.join("app"), root.join("prefs"));
    locator.system_tool_roots = vec![installed];
    for (selected, expected) in [
        (&launcher, &launcher),
        (&configured.join("Nastran/bin"), &launcher),
        (&inner, &inner),
    ] {
        let environment = resolve_msc_fixture(&locator, selected);
        assert_eq!(environment.nastran_exe.as_ref(), Some(expected));
        assert_eq!(environment.nastran_solver.as_ref(), Some(&solver));
        assert_eq!(
            locator.discover_nastran_solver(selected),
            ExecutableDiscovery::Ready(solver.clone())
        );
    }
    fs::remove_dir_all(root)
}

#[test]
fn arbitrary_configured_launcher_cannot_inherit_an_installation_solver() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!("alas-msc-arbitrary-pair-{}", std::process::id()));
    let launcher = root.join("custom/launcher.exe");
    let installed = root.join("NaPa_SE/20261");
    write_msc_fixture(&launcher)?;
    write_msc_fixture(&installed.join("Nastran/bin/nastran.exe"))?;
    write_msc_fixture(&installed.join("Patran/servermode/win64i8/analysis.exe"))?;
    write_msc_fixture(&root.join("Nastran/bin/nastran.exe"))?;
    write_msc_fixture(&root.join("Patran/servermode/win64i8/analysis.exe"))?;
    let mut locator = ToolLocator::new(root.join("app"), root.join("prefs"));
    locator.system_tool_roots = vec![installed];

    let environment = resolve_msc_fixture(&locator, &launcher);
    assert_eq!(environment.nastran_exe, Some(launcher.clone()));
    assert_eq!(environment.nastran_solver, None);
    assert_eq!(
        locator.discover_nastran_solver(&launcher),
        ExecutableDiscovery::Absent
    );
    fs::remove_dir_all(root)
}

#[test]
fn solver_without_a_selected_launcher_is_not_an_environment_candidate() -> std::io::Result<()> {
    let root =
        std::env::temp_dir().join(format!("alas-msc-unpaired-solver-{}", std::process::id()));
    let installed = root.join("NaPa_SE/20261");
    write_msc_fixture(&installed.join("Patran/servermode/win64i8/analysis.exe"))?;
    let mut locator = ToolLocator::new(root.join("app"), root.join("prefs"));
    locator.system_tool_roots = vec![installed];

    let environment = resolve_msc_fixture(&locator, Path::new(""));
    assert_eq!(environment.nastran_exe, None);
    assert_eq!(environment.nastran_solver, None);
    assert_eq!(
        locator.discover_nastran_solver(Path::new("")),
        ExecutableDiscovery::Absent
    );
    fs::remove_dir_all(root)
}

#[test]
fn disabled_environment_discovery_skips_even_configured_tools() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!("alas-tools-disabled-{}", std::process::id()));
    let mses = root.join("MSES");
    let nastran = root.join("Nastran/bin/nastran.exe");
    let patran = root.join("Patran/bin/patran.exe");
    let openvsp = root.join("OpenVSP");
    let avl = root.join("avl.exe");
    for file in [
        mses.join("mset.exe"),
        mses.join("mses.exe"),
        mses.join("mplot.exe"),
        nastran.clone(),
        patran.clone(),
        root.join("Patran/servermode/win64i8/analysis.exe"),
        openvsp.join("vspscript.exe"),
        openvsp.join("vspaero.exe"),
        avl.clone(),
    ] {
        write_msc_fixture(&file)?;
    }
    let locator = ToolLocator::new(root.join("app"), root.join("prefs"));
    locator
        .save_preferences(&ToolPreferences {
            flowunsteady_exe: Some(avl.display().to_string()),
            ..ToolPreferences::default()
        })
        .map_err(std::io::Error::other)?;

    assert_eq!(
        locator.resolve_environment_with_discovery(&mses, &nastran, &patran, &openvsp, &avl, false),
        RunEnvironment::default()
    );
    let environment =
        locator.resolve_environment_with_discovery(&mses, &nastran, &patran, &openvsp, &avl, true);
    assert_eq!(environment.mses_dir, Some(mses));
    assert_eq!(environment.nastran_exe, Some(nastran));
    assert_eq!(
        environment.nastran_solver,
        Some(root.join("Patran/servermode/win64i8/analysis.exe"))
    );
    assert_eq!(environment.patran_exe, Some(patran));
    assert_eq!(environment.openvsp_exe, Some(openvsp.join("vspscript.exe")));
    assert_eq!(environment.vspaero_exe, Some(openvsp.join("vspaero.exe")));
    assert_eq!(environment.avl_exe, Some(avl.clone()));
    assert_eq!(environment.flowunsteady_exe, Some(avl));
    fs::remove_dir_all(root)
}

#[test]
fn external_tool_discovery_is_disabled_only_by_an_explicit_mode() {
    assert!(discovery_mode_enabled(None));
    assert!(discovery_mode_enabled(Some("")));
    assert!(discovery_mode_enabled(Some("enabled")));
    assert!(!discovery_mode_enabled(Some("disabled")));
    assert!(!discovery_mode_enabled(Some("DISABLED")));
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
