// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! AVL packaging: the reviewed win32 executable on Windows, and the GPL
//! corresponding source, licence text, and a written build note everywhere
//! else.
//!
//! Split out of `dist.rs` as its own module (not a textual `include!`
//! fragment) so this platform-conditional logic has its own size budget and
//! its own focused test module, per `docs/source-size-budgets.tsv`'s note on
//! `dist.rs`: frozen pending decomposition into modules with interfaces.

use std::fs;
use std::path::Path;

use crate::dist::BundleStatus;

/// The name of the written note directing a non-Windows recipient to AVL's
/// own Linux build path instead of a bundled executable this task has not
/// reviewed for that platform.
const AVL_LINUX_NOTICE_NAME: &str = "AVL-LINUX-BUILD.txt";

/// Copy AVL's GPL corresponding source and licence text into every package,
/// and the unchanged win32 executable only into a package built for
/// `target_os = "windows"`.
///
/// The only AVL build this task has reviewed and tested is the official
/// win32 executable already staged in `external tools/`. There is no
/// reviewed Linux (or macOS) AVL build to bundle, and assembling one
/// unattended from the Fortran source at package time is a separate,
/// unverified engineering step this task does not take: shipping a binary
/// this task cannot run or check here would be worse than shipping none.
/// Every other target instead gets the exact same source archive and licence
/// text plus a written note pointing at the archive's own gfortran build
/// targets (`bin/Makefile.gfortranDP`, `plotlib/config.make.gfortranDP`,
/// `eispack/Makefile.gfortran`), which is the acquisition path a recipient
/// follows once, on first use of the AVL cross-check.
pub(crate) fn bundle_avl(root: &Path, package_dir: &Path) -> Result<BundleStatus, String> {
    let source_dir = root.join("external tools");
    let package_tools = package_dir.join("external tools");
    fs::create_dir_all(&package_tools)
        .map_err(|e| format!("failed to create {}: {e}", package_tools.display()))?;

    // The corresponding source and licence text travel with every package
    // regardless of whether an executable is bundled: they are what makes
    // the win32 executable's GPL terms satisfiable on the platform that
    // ships it, and what a recipient on any other platform builds from.
    for name in ["avl3.52.tgz", "AVL-GPL-2.0.txt"] {
        let source = source_dir.join(name);
        if !source.is_file() {
            return Err(format!(
                "bundled AVL artifact is missing: {}; release packaging requires the corresponding source archive and GPL text on every target",
                source.display()
            ));
        }
        let destination = package_tools.join(name);
        fs::copy(&source, &destination).map_err(|e| {
            format!(
                "failed to copy bundled AVL artifact to {}: {e}",
                destination.display()
            )
        })?;
    }

    if !cfg!(target_os = "windows") {
        write_avl_linux_acquisition_notice(&package_tools)?;
        let reason = format!(
            "no reviewed AVL 3.52 executable is bundled for target_os={}; the GPL source archive and licence text ship in 'external tools/', with the build path in 'external tools/{AVL_LINUX_NOTICE_NAME}'. ALAS runs its own analytical vortex-lattice stage and reports the AVL cross-check as unavailable until a built or acquired executable is configured under Tools",
            std::env::consts::OS
        );
        println!("[INFO] AVL executable not bundled: {reason}");
        return Ok(BundleStatus::not_bundled(reason));
    }

    let exe_name = "avl352.exe";
    let source = source_dir.join(exe_name);
    if !source.is_file() {
        return Err(format!(
            "bundled AVL artifact is missing: {}; Windows release packaging requires the unchanged executable",
            source.display()
        ));
    }
    let destination = package_tools.join(exe_name);
    fs::copy(&source, &destination).map_err(|e| {
        format!(
            "failed to copy bundled AVL artifact to {}: {e}",
            destination.display()
        )
    })?;
    println!(
        "Bundled AVL 3.52 child executable, corresponding source, and GPL text in {}",
        package_tools.display()
    );
    Ok(BundleStatus::bundled())
}

/// Write the note directing a recipient without a bundled AVL executable to
/// the archive's own Linux build targets, and to where ALAS discovers a
/// locally built or acquired one.
fn write_avl_linux_acquisition_notice(package_tools: &Path) -> Result<(), String> {
    let path = package_tools.join(AVL_LINUX_NOTICE_NAME);
    let text = "AVL 3.52 on this platform\n\
==========================\n\
\n\
This package does not include a prebuilt AVL executable. The only AVL build\n\
this project has reviewed and tested is the Windows binary shipped in the\n\
Windows package; shipping that binary here would not run, so it is left out\n\
rather than included as a nonfunctional file.\n\
\n\
avl3.52.tgz in this directory is the unmodified upstream AVL 3.52 GPL-2.0\n\
source archive (full text in AVL-GPL-2.0.txt). It already carries build\n\
files for a gfortran toolchain, for example bin/Makefile.gfortranDP,\n\
plotlib/config.make.gfortranDP, and eispack/Makefile.gfortran. Building AVL\n\
is the upstream project's own process, not this project's: extract the\n\
archive and follow its own README and Makefile targets with a Fortran\n\
compiler such as gfortran installed.\n\
\n\
Once a working AVL 3.52 executable is available, either built locally or\n\
obtained from a source you trust, point ALAS at it from Tools > External\n\
Tools > AVL, or place it at 'external tools/avl' beside the ALAS binary so\n\
it is discovered automatically. ALAS runs its own analytical vortex-lattice\n\
stage regardless; no analysis result depends on AVL being present, and\n\
Model Comparison simply omits the AVL cross-check until one is configured.\n";
    fs::write(&path, text).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::{bundle_avl, AVL_LINUX_NOTICE_NAME};

    /// On the Windows target, packaging must still bundle the reviewed win32
    /// executable exactly as before this change.
    #[cfg(windows)]
    #[test]
    fn bundle_avl_on_windows_ships_the_reviewed_win32_executable() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(PathBuf::from)
            .expect("workspace root beside xtask crate");
        let package =
            std::env::temp_dir().join(format!("alas-avl-windows-package-{}", std::process::id()));
        let _ = fs::remove_dir_all(&package);
        fs::create_dir_all(&package).expect("temporary package directory");

        let status = bundle_avl(&root, &package).expect("AVL bundling on the Windows target");
        assert_eq!(status.status, "bundled");
        for name in ["avl352.exe", "avl3.52.tgz", "AVL-GPL-2.0.txt"] {
            assert!(
                package.join("external tools").join(name).is_file(),
                "missing bundled AVL artifact: {name}"
            );
        }
        assert!(
            !package
                .join("external tools")
                .join(AVL_LINUX_NOTICE_NAME)
                .exists(),
            "the Windows package must not carry the non-Windows acquisition note"
        );

        fs::remove_dir_all(package).expect("temporary package cleanup");
    }

    /// On every other target, packaging must never ship the win32 executable
    /// (it cannot run there); it must still ship the corresponding source and
    /// licence text plus a written acquisition note, and record a reason.
    #[cfg(not(windows))]
    #[test]
    fn bundle_avl_on_non_windows_omits_the_windows_executable_and_records_why() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(PathBuf::from)
            .expect("workspace root beside xtask crate");
        let package = std::env::temp_dir().join(format!(
            "alas-avl-non-windows-package-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&package);
        fs::create_dir_all(&package).expect("temporary package directory");

        let status = bundle_avl(&root, &package).expect("AVL source-only bundling");
        assert_eq!(status.status, "not_bundled");
        assert!(status.reason.is_some_and(|reason| !reason.is_empty()));
        assert!(
            !package.join("external tools").join("avl352.exe").exists(),
            "a non-Windows package must never carry the win32 executable"
        );
        for name in ["avl3.52.tgz", "AVL-GPL-2.0.txt", AVL_LINUX_NOTICE_NAME] {
            assert!(
                package.join("external tools").join(name).is_file(),
                "missing AVL provenance artifact: {name}"
            );
        }

        fs::remove_dir_all(package).expect("temporary package cleanup");
    }
}
