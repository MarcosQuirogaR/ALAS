// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

impl ToolLocator {
    /// Inspect the solver process paired with a visible MSC NASTRAN
    /// launcher.  MSC Student Edition installs commonly put the launcher in
    /// `Nastran/bin` but the actual 64-bit solver under Patran's
    /// `mscnastran_files/.../servermode` tree.  Treating those as one
    /// executable hides a real failure: the launcher can be discovered while
    /// Windows rejects its default child with a missing side-by-side runtime.
    ///
    /// A configured solver path remains authoritative at the pipeline layer;
    /// this discovery is only the safe automatic candidate when that field is
    /// empty. The selected launcher's installation is the only automatic
    /// solver candidate; incomplete installations cannot borrow another
    /// installation's solver.
    pub fn discover_nastran_solver(&self, configured: &Path) -> ExecutableDiscovery {
        self.discover_nastran(configured)
            .ready_path()
            .map_or(ExecutableDiscovery::Absent, |launcher| {
                solver_for_launcher(&launcher)
            })
    }
}

pub(super) fn solver_for_launcher(launcher: &Path) -> ExecutableDiscovery {
    let Some(root) = msc_install_root(launcher) else {
        return ExecutableDiscovery::Absent;
    };
    find_servermode_solver(&root).map_or_else(
        || ExecutableDiscovery::Incomplete {
            directory: root,
            missing: vec!["servermode/analysis.exe".to_owned()],
        },
        ExecutableDiscovery::Ready,
    )
}

fn msc_install_root(path: &Path) -> Option<PathBuf> {
    let nastran = path.parent()?.ancestors().find(|candidate| {
        candidate
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("Nastran"))
    })?;
    let edition = nastran.parent()?;
    Some(if edition.join("Patran").is_dir() {
        edition.to_path_buf()
    } else {
        nastran.to_path_buf()
    })
}

fn find_servermode_solver(root: &Path) -> Option<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let is_analysis = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("analysis.exe"));
            let in_servermode = path.components().any(|component| {
                component
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("servermode")
            });
            if is_analysis && in_servermode {
                return Some(path);
            }
        }
    }
    None
}

pub(super) fn find_embedded_nastran(root: &Path) -> Option<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let is_launcher = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("nastran.exe"));
            let is_versioned_solver = path.components().any(|component| {
                component
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("win64i8")
            });
            let is_nastran_tree = path.components().any(|component| {
                component
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("Nastran")
            });
            let is_visible_bin = path.components().any(|component| {
                component
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("bin")
            });
            if is_launcher && is_versioned_solver && is_nastran_tree && !is_visible_bin {
                return Some(path);
            }
        }
    }
    None
}

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
