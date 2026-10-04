// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Checking the GNU runtime libraries the solver imports, and resolving the runtime directory.

use super::*;

/// Check the imported GNU runtime libraries before the child process starts.
///
/// The dependency list is read from the executable rather than assumed for
/// every NASTRAN-95 build.  This keeps statically linked or differently built
/// solvers usable, while a binary that actually imports the GNU DLLs receives
/// a deterministic diagnostic instead of a Windows loader dialog. Search order
/// (Windows loader): configured runtime, executable directory, then `PATH`.
#[cfg_attr(not(windows), allow(clippy::needless_return))]
pub(super) fn validate_runtime_dependencies(
    executable: &Path,
    configured: Option<&Path>,
) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = (executable, configured);
        return Ok(());
    }

    #[cfg(windows)]
    {
        let bytes = std::fs::read(executable).map_err(|error| {
            format!(
                "cannot inspect NASTRAN-95 executable {} for runtime dependencies: {error}",
                executable.display()
            )
        })?;
        let imported = WINDOWS_GNU_RUNTIME_DLLS
            .iter()
            .copied()
            .filter(|name| contains_ascii_case_insensitive(&bytes, name.as_bytes()))
            .collect::<Vec<_>>();
        if imported.is_empty() {
            return Ok(());
        }

        let mut search_dirs = Vec::new();
        if let Some(runtime) = configured {
            search_dirs.push(absolute_path(runtime).unwrap_or_else(|_| runtime.to_path_buf()));
        }
        if let Some(parent) = executable.parent() {
            search_dirs.push(parent.to_path_buf());
        }
        if let Some(path) = std::env::var_os("PATH") {
            search_dirs.extend(std::env::split_paths(&path));
        }
        let mut unique_search_dirs = Vec::new();
        for directory in search_dirs {
            if !directory.as_os_str().is_empty() && !unique_search_dirs.contains(&directory) {
                unique_search_dirs.push(directory);
            }
        }
        let search_dirs = unique_search_dirs;
        let missing = imported
            .iter()
            .filter(|name| {
                !search_dirs
                    .iter()
                    .any(|directory| directory.join(name).is_file())
            })
            .copied()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(());
        }
        let searched = search_dirs
            .iter()
            .map(|directory| directory.display().to_string())
            .collect::<Vec<_>>();
        Err(format!(
            "NASTRAN-95 executable {} imports missing Windows runtime DLL(s): {}. Searched: {}. Configure ALAS_NASTRAN95_RUNTIME or install the matching GNU Fortran runtime beside the solver; no solver process was spawned.",
            executable.display(),
            missing.join(", "),
            if searched.is_empty() {
                "(none)".to_owned()
            } else {
                searched.join("; ")
            }
        ))
    }
}

/// Resolve a runtime without silently replacing an existing user directory.
/// A stale configured path is recoverable only from the selected solver's
/// adjacent `runtime` directory; no host-wide search is attempted.
pub(super) fn resolve_runtime(
    solver_root: &Path,
    configured: Option<&Path>,
) -> (Option<PathBuf>, Nastran95RuntimeSource) {
    if let Some(path) = configured {
        if path.is_dir() {
            return (Some(path.to_path_buf()), Nastran95RuntimeSource::Configured);
        }
    }

    let adjacent = solver_root.join("runtime");
    if adjacent.is_dir() {
        return (
            Some(adjacent),
            Nastran95RuntimeSource::Adjacent {
                root: solver_root.to_path_buf(),
            },
        );
    }

    (
        configured.map(Path::to_path_buf),
        if configured.is_some() {
            Nastran95RuntimeSource::Configured
        } else {
            Nastran95RuntimeSource::Unspecified
        },
    )
}

#[cfg(windows)]
fn contains_ascii_case_insensitive(bytes: &[u8], needle: &[u8]) -> bool {
    bytes
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}
