// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Resolved external tools for one pipeline run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEnvironment {
    /// Directory containing `mset`, `mses`, and `mplot`.
    pub mses_dir: Option<PathBuf>,
    /// NASTRAN executable, when installed and configured.
    pub nastran_exe: Option<PathBuf>,
    /// Paired MSC NASTRAN solver process, when a split Student Edition
    /// installation exposes one.  The visible launcher can exist while its
    /// default solver path is unusable, so keeping this resolved separately
    /// lets the pipeline pass `a.solver` without requiring a manual setup
    /// override.
    #[serde(default)]
    pub nastran_solver: Option<PathBuf>,
    /// Patran executable, when installed and configured.
    pub patran_exe: Option<PathBuf>,
    /// Headless OpenVSP AngelScript executable, when installed adjacent to ALAS.
    #[serde(default)]
    pub openvsp_exe: Option<PathBuf>,
    /// Native VSPAERO solver executable from the OpenVSP distribution.
    #[serde(default)]
    pub vspaero_exe: Option<PathBuf>,
    /// Native Athena Vortex Lattice executable.
    #[serde(default)]
    pub avl_exe: Option<PathBuf>,
    /// Explicit FLOWUnsteady adapter launcher (`ALAS_FLOWUNSTEADY_EXE`).
    #[serde(default)]
    pub flowunsteady_exe: Option<PathBuf>,
}

/// User-level persisted locations for optional tools.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolPreferences {
    /// Configured MSES directory.
    pub mses_dir: Option<String>,
    /// Configured NASTRAN executable.
    pub nastran_exe: Option<String>,
    /// Optional MSC solver binary passed through the NASTRAN launcher.
    ///
    /// Student Edition installations can split the visible launcher from the
    /// `analysis.exe` process that receives the deck. This must survive a
    /// desktop restart just like the launcher path, or Setup can appear ready
    /// while the next solve loses the required `a.solver` override.
    #[serde(default)]
    pub nastran_solver: Option<String>,
    /// Directory containing the locally built NASTRAN-95 tree.
    #[serde(default)]
    pub nastran95_dir: Option<String>,
    /// Optional directory containing the local NASTRAN-95 GNU Fortran runtime.
    #[serde(default)]
    pub nastran95_runtime: Option<String>,
    /// Optional short absolute directory for staged NASTRAN-95 rigid formats.
    #[serde(default)]
    pub nastran95_rf_stage: Option<String>,
    /// Optional open-core allocation for the local NASTRAN-95 solver.
    #[serde(default)]
    pub nastran95_open_core_words: Option<String>,
    /// Configured Patran executable.
    pub patran_exe: Option<String>,
    /// OpenVSP installation directory or its headless script runner.
    ///
    /// The script runner and VSPAERO are separate programs in one official
    /// distribution. Retaining their common location lets the pipeline report
    /// a missing runner instead of treating the solver alone as ready.
    #[serde(default)]
    pub openvsp_dir: Option<String>,
    /// Configured Athena Vortex Lattice executable.
    #[serde(default)]
    pub avl_exe: Option<String>,
    /// Directory containing the optional open navigation-data dataset.
    ///
    /// This remains a user preference rather than an installation-relative
    /// default: the data is sizeable, licensed separately, and may be shared
    /// by more than one ALAS installation on the same computer.
    #[serde(default)]
    pub navdata_dir: Option<String>,
    /// Directory containing user-imported and saved flight routes.
    #[serde(default)]
    pub routes_dir: Option<String>,
}

/// Result of inspecting one MSES installation directory.
///
/// MSES is a three-program installation. Reporting which requirement is
/// missing keeps an incomplete adjacent install distinct from a directory
/// that is not present at all, so the caller can show an actionable status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MsesDiscovery {
    /// No candidate directory exists.
    Absent,
    /// The directory exists but one or more required programs are absent.
    Incomplete {
        /// Directory that was inspected.
        directory: PathBuf,
        /// Required program names that were not regular files.
        missing: Vec<String>,
    },
    /// All required MSES programs are present.
    Ready(PathBuf),
}

/// Result of inspecting one executable-based installation.
///
/// An executable path can point at an unpacked but incomplete installation
/// directory. Keeping that state separate from an absent candidate lets the
/// setup page tell the user whether a path is wrong or a bundle is missing a
/// required launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutableDiscovery {
    /// No configured or adjacent installation candidate exists.
    Absent,
    /// A candidate installation directory exists but has no usable launcher.
    Incomplete {
        /// Directory that was inspected.
        directory: PathBuf,
        /// Launcher names that were not regular files.
        missing: Vec<String>,
    },
    /// A usable launcher was found.
    Ready(PathBuf),
}

impl ExecutableDiscovery {
    fn ready_path(self) -> Option<PathBuf> {
        match self {
            Self::Ready(path) => Some(path),
            Self::Absent | Self::Incomplete { .. } => None,
        }
    }
}

/// Roots and preference location used to resolve optional tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolLocator {
    app_root: PathBuf,
    user_data_root: PathBuf,
    system_tool_roots: Vec<PathBuf>,
}
