// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

impl ToolLocator {
    /// Inspect the configured FLOWUnsteady/Julia adapter launcher.
    ///
    /// The configured preference overrides discovery, mirroring how
    /// `avl_exe`/`openvsp_dir` already override their own adjacent-tool
    /// discovery. `ALAS_FLOWUNSTEADY_EXE` remains a valid override for
    /// headless/CI use when no preference is configured.
    pub fn discover_flowunsteady(&self, configured: &Path) -> ExecutableDiscovery {
        if !configured.as_os_str().is_empty() {
            return if configured.is_file() {
                ExecutableDiscovery::Ready(configured.to_path_buf())
            } else {
                ExecutableDiscovery::Incomplete {
                    directory: configured.to_path_buf(),
                    missing: vec!["FLOWUnsteady/Julia executable".to_owned()],
                }
            };
        }
        env::var_os("ALAS_FLOWUNSTEADY_EXE")
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .map_or(ExecutableDiscovery::Absent, ExecutableDiscovery::Ready)
    }

    /// Inspect the configured and adjacent MSC Nastran installation.
    pub fn discover_nastran(&self, configured: &Path) -> ExecutableDiscovery {
        let adjacent = self.discover_executable(
            configured,
            &["nastran.exe", "nastran"],
            &["NASTRAN", "Nastran", "nastran"],
        );
        if matches!(adjacent, ExecutableDiscovery::Ready(_)) {
            return adjacent;
        }
        self.discover_msc_executable(
            "Nastran/bin",
            &["nastran.exe", "nastranw.exe"],
            configured.as_os_str().is_empty(),
        )
        .unwrap_or(adjacent)
    }

    /// Inspect the configured and adjacent MSC Patran installation.
    pub fn discover_patran(&self, configured: &Path) -> ExecutableDiscovery {
        let adjacent = self.discover_executable(
            configured,
            &["patran.exe", "patran"],
            &["Patran", "PATRAN", "patran"],
        );
        if matches!(adjacent, ExecutableDiscovery::Ready(_)) {
            return adjacent;
        }
        self.discover_msc_executable("Patran/bin", &["patran.exe"], false)
            .unwrap_or(adjacent)
    }

    /// Inspect a configured or adjacent headless OpenVSP installation.
    ///
    /// The official Windows distribution calls the batch runner
    /// `vspscript.exe`. The interactive `vsp.exe` also accepts `-script`, but
    /// is deliberately not selected automatically because it loads the GUI
    /// and graphics stack for a pipeline operation that does not need either.
    pub fn discover_openvsp(&self, configured: &Path) -> ExecutableDiscovery {
        self.discover_executable(
            configured,
            &["vspscript.exe", "vspscript"],
            &["OpenVSP", "openvsp"],
        )
    }

    /// Inspect a configured or adjacent native VSPAERO installation.
    pub fn discover_vspaero(&self, configured: &Path) -> ExecutableDiscovery {
        self.discover_executable(
            configured,
            &["vspaero.exe", "vspaero"],
            &["OpenVSP", "openvsp"],
        )
    }

    /// Inspect a configured or adjacent native Athena Vortex Lattice install.
    pub fn discover_avl(&self, configured: &Path) -> ExecutableDiscovery {
        // The official Windows executable is commonly versioned (`avl352.exe`)
        // and the ALAS external-tools bundle uses that name.  Keep the generic
        // names for user installations, but recognize the bundled binary
        // without requiring a manual path entry.
        self.discover_executable(
            configured,
            &["avl352.exe", "avl.exe", "avl352", "avl"],
            &["AVL", "avl"],
        )
    }
}
