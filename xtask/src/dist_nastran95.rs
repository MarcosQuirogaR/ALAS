// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! NASTRAN-95 packaging for both package targets.
//!
//! Split out of `dist.rs` (frozen pending decomposition) so the Windows and
//! Linux staging contracts live beside their own tests. The Linux staging
//! tree is produced by `tools/nastran95/build-linux.sh`; its layout is the
//! Windows layout with `build/bin/nastran` instead of `build/bin/nastran.exe`
//! and shared-object (or no) runtime dependencies instead of DLLs.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::dist::{copy_directory, sha256_hex, BundleStatus};

/// Which staging contract a package target needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    Windows,
    Unix,
}

impl Platform {
    pub(crate) fn host() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }

    /// Solver executable relative to the staging root.
    fn executable(self) -> &'static str {
        match self {
            Self::Windows => "build/bin/nastran.exe",
            Self::Unix => "build/bin/nastran",
        }
    }

    /// Whether `name` is a runtime library of this platform.
    fn is_library(self, name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        match self {
            Self::Windows => lower.ends_with(".dll"),
            Self::Unix => lower.ends_with(".so") || lower.contains(".so."),
        }
    }

    fn library_noun(self) -> &'static str {
        match self {
            Self::Windows => "DLL",
            Self::Unix => "shared library",
        }
    }
}

/// Copy NASTRAN-95 as a separately licensed adjacent program when its
/// executable, runtime dependency declaration, license, modification record,
/// and source association are all present and internally checkable.
///
/// The default mode omits incomplete staging and records the reason in the
/// release manifest. Set `ALAS_STRICT_BUNDLED_NASTRAN95=1` to make an
/// intentional bundled release fail closed instead.
pub(crate) fn bundle_nastran95(
    root: &Path,
    package_dir: &Path,
    strict: bool,
) -> Result<BundleStatus, String> {
    let source = root.join("external tools").join("NASTRAN-95");
    let issues = assess_nastran95(&source, Platform::host());
    if !issues.is_empty() {
        let reason = issues.join("; ");
        if strict {
            return Err(format!(
                "strict NASTRAN-95 bundle mode refused incomplete staging at {}: {}",
                source.display(),
                reason
            ));
        }
        println!("[INFO] NASTRAN-95 not bundled: {reason}");
        return Ok(BundleStatus::not_bundled(reason));
    }

    let destination = package_dir.join("external tools").join("NASTRAN-95");
    copy_directory(&source, &destination)?;
    println!(
        "Bundled separately licensed NASTRAN-95 executable, runtime, rigid formats, corresponding source, and notices in {}",
        destination.display()
    );
    Ok(BundleStatus::bundled())
}

pub(crate) fn assess_nastran95(source: &Path, platform: Platform) -> Vec<String> {
    let required_files = [
        platform.executable(),
        "build/bin/nastran95-build.txt",
        "rf/NASINFO",
        "LICENSE",
        "MODIFICATIONS.md",
        "SOURCE-REVISION.txt",
        "source/NASTRAN-95-source.zip",
    ];
    let mut issues = Vec::new();
    if !source.is_dir() {
        issues.push(format!(
            "staging directory is absent ({})",
            source.display()
        ));
        return issues;
    }
    for relative in required_files {
        let artifact = source.join(relative);
        if !artifact.is_file() {
            issues.push(format!("missing {relative}"));
        } else if fs::metadata(&artifact).is_ok_and(|metadata| metadata.len() == 0) {
            issues.push(format!("empty {relative}"));
        }
    }
    #[cfg(unix)]
    if platform == Platform::Unix {
        use std::os::unix::fs::PermissionsExt;
        let executable = source.join(platform.executable());
        if fs::metadata(&executable)
            .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 == 0)
        {
            issues.push(format!("{} is not executable", platform.executable()));
        }
    }

    let runtime = source.join("runtime");
    if !runtime.is_dir() {
        issues.push("runtime directory is absent".to_owned());
    } else {
        let runtime_files = match fs::read_dir(&runtime) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_file()))
                .collect::<Vec<_>>(),
            Err(error) => {
                issues.push(format!("cannot inspect runtime directory: {error}"));
                Vec::new()
            }
        };
        if runtime_files.is_empty() {
            issues.push("runtime directory is empty".to_owned());
        }
        validate_runtime_manifest(&runtime, &runtime_files, platform, &mut issues);
    }

    let revision_path = source.join("SOURCE-REVISION.txt");
    if revision_path.is_file() {
        match fs::read_to_string(&revision_path) {
            Ok(text) => validate_source_revision(&text, source, &mut issues),
            Err(error) => issues.push(format!("cannot read SOURCE-REVISION.txt: {error}")),
        }
    }
    issues
}

/// `runtime/DEPENDENCIES.txt` is the exact inventory of runtime libraries
/// beside the solver. A Windows build always needs the GNU runtime DLLs. A
/// Linux build links the GNU Fortran runtime statically (see
/// `build-linux.sh`), so an empty inventory is valid there; the file must
/// still exist so "no libraries" is a recorded statement and not an omission.
fn validate_runtime_manifest(
    runtime: &Path,
    runtime_files: &[fs::DirEntry],
    platform: Platform,
    issues: &mut Vec<String>,
) {
    let noun = platform.library_noun();
    let manifest = runtime.join("DEPENDENCIES.txt");
    if !manifest.is_file() {
        issues.push(
            "runtime/DEPENDENCIES.txt is missing (exact runtime inventory is required)".to_owned(),
        );
        return;
    }
    let text = match fs::read_to_string(&manifest) {
        Ok(text) => text,
        Err(error) => {
            issues.push(format!("cannot read runtime/DEPENDENCIES.txt: {error}"));
            return;
        }
    };
    let mut declared = BTreeSet::new();
    for line in text.lines() {
        let name = line.trim();
        if name.is_empty() || name.starts_with('#') {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if !platform.is_library(name) || Path::new(name).file_name().is_none_or(|file| file != name)
        {
            issues.push(format!(
                "runtime dependency entry is not a {noun} basename: {name}"
            ));
            continue;
        }
        if !declared.insert(lower.clone()) {
            issues.push(format!("runtime {noun} is declared more than once: {name}"));
        }
        let dependency = runtime.join(name);
        if !dependency.is_file() {
            issues.push(format!(
                "declared runtime {noun} is missing: runtime/{name}"
            ));
        } else if fs::metadata(&dependency).is_ok_and(|metadata| metadata.len() == 0) {
            issues.push(format!("declared runtime {noun} is empty: runtime/{name}"));
        }
    }
    if declared.is_empty() && platform == Platform::Windows {
        issues.push("runtime/DEPENDENCIES.txt declares no DLLs".to_owned());
    }

    let actual = runtime_files
        .iter()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            platform.is_library(&name).then_some(name)
        })
        .collect::<BTreeSet<_>>();
    for extra in actual.difference(&declared) {
        issues.push(format!(
            "runtime {noun} is absent from DEPENDENCIES.txt: {extra}"
        ));
    }
}

fn validate_source_revision(text: &str, source: &Path, issues: &mut Vec<String>) {
    let lower = text.to_ascii_lowercase();
    if lower.contains("not reproducibly associated") {
        issues.push(
            "SOURCE-REVISION.txt explicitly disclaims reproducible executable/source association"
                .to_owned(),
        );
    }
    let source_archive = source.join("source/NASTRAN-95-source.zip");
    let actual_hash = fs::read(&source_archive)
        .ok()
        .map(|bytes| sha256_hex(&bytes));
    // A revision record may also contain a full Git commit hash. Only accept
    // a 64-digit value on the line explicitly labelling the source archive;
    // otherwise a valid commit identifier could be mistaken for the archive
    // digest.
    let declared_hash = text.lines().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        if !(lower.contains("source archive") && lower.contains("sha-256")) {
            return None;
        }
        line.split(|character: char| !character.is_ascii_hexdigit())
            .find(|token| {
                token.len() == 64 && token.chars().all(|character| character.is_ascii_hexdigit())
            })
            .map(str::to_ascii_lowercase)
    });
    match (declared_hash, actual_hash) {
        (Some(declared), Some(actual)) if declared != actual => issues.push(format!(
            "SOURCE-REVISION.txt source-archive hash {declared} does not match SHA-256 {actual}"
        )),
        (Some(_), Some(_)) => {}
        _ => issues.push(
            "SOURCE-REVISION.txt must label the exact 64-digit SHA-256 of source/NASTRAN-95-source.zip"
                .to_owned(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("alas-nastran-staging-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// A complete staging tree for `platform`; the Linux one links the GNU
    /// runtime statically and declares no libraries.
    fn stage(dir: &Path, platform: Platform) {
        let archive = b"source archive bytes";
        let files: [(&str, &[u8]); 6] = [
            (platform.executable(), b"binary"),
            ("build/bin/nastran95-build.txt", b"build"),
            ("rf/NASINFO", b"nasinfo"),
            ("LICENSE", b"licence"),
            ("MODIFICATIONS.md", b"mods"),
            ("source/NASTRAN-95-source.zip", archive),
        ];
        for (relative, bytes) in files {
            let path = dir.join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("dir");
            fs::write(&path, bytes).expect("file");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let exe = dir.join(platform.executable());
            fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        fs::write(
            dir.join("SOURCE-REVISION.txt"),
            format!("Source archive SHA-256: {}\n", sha256_hex(archive)),
        )
        .expect("revision");
        fs::create_dir_all(dir.join("runtime")).expect("runtime");
        match platform {
            Platform::Windows => {
                fs::write(dir.join("runtime/libgfortran-5.dll"), b"dll").expect("dll");
                fs::write(dir.join("runtime/DEPENDENCIES.txt"), "libgfortran-5.dll\n")
                    .expect("deps");
            }
            Platform::Unix => {
                fs::write(
                    dir.join("runtime/DEPENDENCIES.txt"),
                    "# statically linked libgfortran/libquadmath/libgcc\n",
                )
                .expect("deps");
            }
        }
    }

    #[test]
    fn missing_nastran_staging_is_reported_without_a_placeholder() {
        let source = scratch("absent");
        for platform in [Platform::Windows, Platform::Unix] {
            let issues = assess_nastran95(&source, platform);
            assert!(issues
                .iter()
                .any(|issue| issue.contains("staging directory is absent")));
        }
        assert!(!source.exists());
    }

    #[test]
    fn complete_staging_passes_on_each_platform() {
        for (tag, platform) in [("win", Platform::Windows), ("unix", Platform::Unix)] {
            let dir = scratch(tag);
            stage(&dir, platform);
            assert_eq!(assess_nastran95(&dir, platform), Vec::<String>::new());
            fs::remove_dir_all(dir).expect("cleanup");
        }
    }

    #[test]
    fn each_platform_requires_its_own_executable_name() {
        let dir = scratch("exe");
        stage(&dir, Platform::Windows);
        let issues = assess_nastran95(&dir, Platform::Unix);
        assert!(issues
            .iter()
            .any(|i| i.contains("missing build/bin/nastran")));
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn linux_runtime_inventory_may_be_empty_but_must_exist_and_match_files() {
        let dir = scratch("inventory");
        stage(&dir, Platform::Unix);
        fs::write(dir.join("runtime/libgfortran.so.5"), b"so").expect("so");
        let issues = assess_nastran95(&dir, Platform::Unix);
        assert!(issues
            .iter()
            .any(|i| i.contains("absent from DEPENDENCIES.txt: libgfortran.so.5")));
        fs::write(dir.join("runtime/DEPENDENCIES.txt"), "libgfortran.so.5\n").expect("deps");
        assert!(assess_nastran95(&dir, Platform::Unix).is_empty());
        fs::write(dir.join("runtime/DEPENDENCIES.txt"), "libgfortran-5.dll\n").expect("deps");
        assert!(assess_nastran95(&dir, Platform::Unix)
            .iter()
            .any(|i| i.contains("not a shared library basename")));
        fs::remove_file(dir.join("runtime/DEPENDENCIES.txt")).expect("rm");
        assert!(assess_nastran95(&dir, Platform::Unix)
            .iter()
            .any(|i| i.contains("DEPENDENCIES.txt is missing")));
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn windows_runtime_still_needs_at_least_one_dll() {
        let dir = scratch("windll");
        stage(&dir, Platform::Windows);
        fs::remove_file(dir.join("runtime/libgfortran-5.dll")).expect("rm");
        fs::write(dir.join("runtime/DEPENDENCIES.txt"), "# none\n").expect("deps");
        assert!(assess_nastran95(&dir, Platform::Windows)
            .iter()
            .any(|i| i.contains("declares no DLLs")));
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn revision_must_match_the_source_archive_hash() {
        let dir = scratch("hash");
        stage(&dir, Platform::Unix);
        fs::write(
            dir.join("SOURCE-REVISION.txt"),
            format!("Source archive SHA-256: {}\n", "0".repeat(64)),
        )
        .expect("revision");
        assert!(assess_nastran95(&dir, Platform::Unix)
            .iter()
            .any(|i| i.contains("does not match SHA-256")));
        fs::remove_dir_all(dir).expect("cleanup");
    }
}

#[cfg(test)]
mod staged_tree {
    use super::*;

    /// CI hook: assess a real staging tree built by `build-linux.sh`
    /// (`ALAS_NASTRAN95_STAGING_ASSESS=<dir>`) with the contract `dist` applies.
    #[test]
    #[ignore = "needs ALAS_NASTRAN95_STAGING_ASSESS: a staged NASTRAN-95 tree"]
    fn a_staged_tree_passes_the_dist_contract() {
        let dir = std::env::var("ALAS_NASTRAN95_STAGING_ASSESS")
            .expect("set ALAS_NASTRAN95_STAGING_ASSESS to the staged tree");
        let issues = assess_nastran95(Path::new(&dir), Platform::host());
        assert!(issues.is_empty(), "{issues:#?}");
    }
}
