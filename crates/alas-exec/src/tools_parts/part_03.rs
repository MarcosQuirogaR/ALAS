// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_external_tools_are_discovered_without_configuration() {
        let root = std::env::temp_dir().join(format!("alas-tools-{}", std::process::id()));
        let tools = root.join("external tools/MSES");
        let _ = fs::create_dir_all(&tools);
        let _ = fs::write(tools.join("mset.exe"), b"test");
        let _ = fs::write(tools.join("mses.exe"), b"test");
        let _ = fs::write(tools.join("mplot.exe"), b"test");
        let locator = ToolLocator::new(&root, root.join("prefs"));
        assert_eq!(locator.resolve_mses_dir(Path::new("")), Some(tools));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn incomplete_mses_directory_is_not_reported_as_an_installation() {
        let root =
            std::env::temp_dir().join(format!("alas-incomplete-mses-{}", std::process::id()));
        let tools = root.join("external tools/MSES");
        let _ = fs::create_dir_all(&tools);
        let _ = fs::write(tools.join("mplot.exe"), b"test");
        let locator = ToolLocator::new(&root, root.join("prefs"));

        assert_eq!(locator.resolve_mses_dir(Path::new("")), None);
        assert_eq!(locator.resolve_mses_dir(&tools), None);
        assert_eq!(
            locator.discover_mses(&tools),
            MsesDiscovery::Incomplete {
                directory: tools.clone(),
                missing: vec!["mset.exe".to_owned(), "mses.exe".to_owned()],
            }
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_mses_directory_is_reported_as_absent() {
        let root = std::env::temp_dir().join(format!("alas-absent-mses-{}", std::process::id()));
        let locator = ToolLocator::new(&root, root.join("prefs"));
        assert_eq!(
            locator.discover_mses(Path::new("missing/MSES")),
            MsesDiscovery::Absent
        );
    }

    #[test]
    fn executables_are_discovered_in_conventional_tool_subdirectories() {
        let root = std::env::temp_dir().join(format!("alas-executables-{}", std::process::id()));
        let mses = root.join("external tools/MSES");
        let nastran = root.join("external tools/NASTRAN/nastran.exe");
        let patran = root.join("external tools/Patran/patran.exe");
        let openvsp = root.join("external tools/OpenVSP/vspscript.exe");
        let vspaero = root.join("external tools/OpenVSP/vspaero.exe");
        let avl = root.join("external tools/AVL/avl.exe");
        let _ = fs::create_dir_all(&mses);
        let _ = fs::create_dir_all(nastran.parent().unwrap_or(Path::new(".")));
        let _ = fs::create_dir_all(patran.parent().unwrap_or(Path::new(".")));
        let _ = fs::create_dir_all(openvsp.parent().unwrap_or(Path::new(".")));
        let _ = fs::write(mses.join("mset.exe"), b"test");
        let _ = fs::write(mses.join("mses.exe"), b"test");
        let _ = fs::write(mses.join("mplot.exe"), b"test");
        let _ = fs::write(&nastran, b"test");
        let _ = fs::write(&patran, b"test");
        let _ = fs::write(&openvsp, b"test");
        let _ = fs::write(&vspaero, b"test");
        let _ = fs::create_dir_all(avl.parent().unwrap_or(Path::new(".")));
        let _ = fs::write(&avl, b"test");

        let locator = ToolLocator::new(&root, root.join("prefs"));
        let environment = locator.resolve_environment(
            Path::new(""),
            Path::new(""),
            Path::new(""),
            Path::new(""),
            Path::new(""),
        );

        assert_eq!(environment.mses_dir, Some(mses));
        assert_eq!(environment.nastran_exe, Some(nastran));
        assert_eq!(environment.patran_exe, Some(patran));
        assert_eq!(environment.openvsp_exe, Some(openvsp));
        assert_eq!(environment.vspaero_exe, Some(vspaero));
        assert_eq!(environment.avl_exe, Some(avl));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn incomplete_msc_installations_are_distinct_from_absent_tools() {
        let root = std::env::temp_dir().join(format!("alas-incomplete-msc-{}", std::process::id()));
        let nastran_dir = root.join("external tools/NASTRAN");
        let patran_dir = root.join("external tools/Patran");
        let _ = fs::create_dir_all(&nastran_dir);
        let _ = fs::create_dir_all(&patran_dir);
        let locator = ToolLocator::new(&root, root.join("prefs"));

        assert_eq!(
            locator.discover_executable(Path::new(""), &["nastran.exe", "nastran"], &["NASTRAN"]),
            ExecutableDiscovery::Incomplete {
                directory: nastran_dir,
                missing: vec!["nastran.exe".to_owned(), "nastran".to_owned()],
            }
        );
        assert_eq!(
            locator.discover_executable(
                Path::new("missing/patran.exe"),
                &["patran.exe", "patran"],
                &["Patran"]
            ),
            ExecutableDiscovery::Incomplete {
                directory: patran_dir,
                missing: vec!["patran.exe".to_owned(), "patran".to_owned()],
            }
        );
        let _ = fs::remove_dir_all(root);
    }
}

#[cfg(test)]
#[path = "../tools_external_tests.rs"]
mod external_tests;

