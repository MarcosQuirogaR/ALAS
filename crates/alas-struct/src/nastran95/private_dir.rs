// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Private, unpredictable, short work directories for the legacy solver.
//!
//! The solver's rigid-format path buffer is 37 bytes, so its work tree has to sit
//! near a filesystem root. A fixed name there (`C:\n95rf`, `/tmp/n95rf`) is shared
//! by every user and every ALAS process on the machine: another user can create
//! it first (as a symlink on Linux, with an ACL that locks us out on Windows), and
//! two processes rewrite one `NASINFO` at the same time. Every run therefore gets
//! its own directory, created exclusively, with an unpredictable name and, on
//! Unix, mode 0700. Nothing is created until a solve actually starts.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A name that differs between runs and cannot be guessed by another user.
fn unpredictable_suffix() -> String {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u32(std::process::id());
    format!("{:012x}", hasher.finish() & 0xffff_ffff_ffff)
}

/// Parent directories to try, most private first. The solver is addressed by paths
/// relative to its working directory (see `run_nastran95`), so no short parent is
/// needed: the per-user profile on Windows (private to the user, SYSTEM and
/// administrators), `$XDG_RUNTIME_DIR` or the sticky `/tmp` on Unix. A drive root
/// is never used: `C:` hands Modify to every signed-in user and `D:` may be FAT.
fn parents() -> Vec<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA")
            .map(|base| vec![PathBuf::from(base).join("ALAS").join("run")])
            .unwrap_or_default();
    }
    let mut parents = Vec::new();
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        if runtime.is_absolute() {
            parents.push(runtime);
        }
    }
    parents.push(PathBuf::from("/tmp"));
    parents.push(PathBuf::from("/var/tmp"));
    parents
}

#[cfg(unix)]
fn create_exclusive(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_exclusive(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir(path)?;
    restrict_to_owner(path).inspect_err(|_| {
        let _ = std::fs::remove_dir(path);
    })
}

/// Windows: drop the inherited ACEs and grant full control to the current user and
/// SYSTEM only, so no other account can change the files the solver reads. Fails
/// closed: the caller removes the folder when this fails.
#[cfg(windows)]
fn restrict_to_owner(path: &Path) -> std::io::Result<()> {
    let sid = current_user_sid()?;
    let mut command = std::process::Command::new(system32_tool("icacls.exe"));
    command
        .arg(path)
        .args(["/inheritance:r", "/grant:r", "*S-1-5-18:(OI)(CI)F"])
        .arg(format!("*{sid}:(OI)(CI)F"));
    let output = alas_exec::captured::run_captured(
        &mut command,
        "icacls",
        std::time::Duration::from_secs(30),
    )
    .map_err(|error| std::io::Error::other(format!("icacls failed: {error}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "icacls: {}",
            output.stdout.trim()
        )))
    }
}

/// A tool in the Windows system folder by absolute path, never by a search that
/// could find a planted copy first. `C:WindowsSystem32` is the normal home; a
/// relocated Windows is found through `SystemRoot` only when that is absent.
#[cfg(windows)]
pub(super) fn system32_tool(name: &str) -> PathBuf {
    let usual = PathBuf::from(r"C:WindowsSystem32").join(name);
    if usual.is_file() {
        return usual;
    }
    std::env::var_os("SystemRoot").map_or(usual.clone(), |root| {
        PathBuf::from(root).join("System32").join(name)
    })
}

/// The SID of the account this process runs as, from its token (`whoami /user`),
/// not from an environment variable and not by account name.
#[cfg(windows)]
pub(super) fn current_user_sid() -> std::io::Result<String> {
    let mut command = std::process::Command::new(system32_tool("whoami.exe"));
    command.args(["/user", "/fo", "csv", "/nh"]);
    let output = alas_exec::captured::run_captured(
        &mut command,
        "whoami",
        std::time::Duration::from_secs(30),
    )
    .map_err(|error| std::io::Error::other(format!("whoami failed: {error}")))?;
    output
        .stdout
        .split('"')
        .map(str::trim)
        .find(|field| field.starts_with("S-1-"))
        .map(str::to_owned)
        .ok_or_else(|| std::io::Error::other("whoami did not report a SID"))
}

#[cfg(all(not(unix), not(windows)))]
fn restrict_to_owner(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Create a fresh private directory under one of `parents`: the name is new, the
/// creation fails if anything (including a symlink) already has it, and the result
/// is checked to be a real directory.
pub(super) fn create_private_dir_in(parents: &[PathBuf], prefix: &str) -> Result<PathBuf, String> {
    for parent in parents {
        let path = parent.join(format!("{prefix}{}", unpredictable_suffix()));
        if create_exclusive(&path).is_err() {
            continue;
        }
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => return Ok(path),
            _ => {
                let _ = std::fs::remove_dir(&path);
            }
        }
    }
    Err(format!(
        "no private short work directory could be created (tried {})",
        parents
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Exactly what [`create_private_dir_in`] makes: `prefix` and 12 lowercase hex digits,
/// so a user's own `n95-notes` is never touched.
fn is_private_root_name(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix).is_some_and(|rest| {
        rest.len() == 12
            && rest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Remove `prefix`-named directories in `parent` last modified over a day ago.
fn sweep_stale(parent: &Path, prefix: &str) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    let day = std::time::Duration::from_secs(24 * 3600);
    for entry in entries.flatten() {
        if !is_private_root_name(&entry.file_name().to_string_lossy(), prefix) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let old = meta
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > day);
        if meta.is_dir() && !meta.file_type().is_symlink() && old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// A fresh private work root (`prefix` + a random suffix).
///
/// Roots left by a killed app are swept first: directories of this kind older than a
/// day are removed (never a link).
pub(super) fn create_private_dir(prefix: &str) -> Result<PathBuf, String> {
    let parents = parents();
    for parent in &parents {
        let _ = std::fs::create_dir_all(parent);
        sweep_stale(parent, prefix);
    }
    create_private_dir_in(&parents, prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_requests_never_share_a_directory_and_nothing_pre_exists() {
        let base = std::env::temp_dir().join(format!("alas-private-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("base");
        let parents = [base.clone()];
        let a = create_private_dir_in(&parents, "n95-").expect("first");
        let b = create_private_dir_in(&parents, "n95-").expect("second");
        assert_ne!(a, b);
        assert!(a.is_dir() && b.is_dir());
        assert!(a.starts_with(&base) && b.starts_with(&base));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn concurrent_requests_get_distinct_directories() {
        let base = std::env::temp_dir().join(format!("alas-private-c-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("base");
        let parents = [base.clone()];
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let parents = parents.clone();
                std::thread::spawn(move || create_private_dir_in(&parents, "n95-").expect("dir"))
            })
            .collect();
        let mut dirs: Vec<PathBuf> = handles
            .into_iter()
            .map(|h| h.join().expect("thread"))
            .collect();
        dirs.sort();
        dirs.dedup();
        assert_eq!(dirs.len(), 8);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The DACL is protected (no inherited ACEs) and holds exactly SYSTEM and the
    /// token's user SID, read as SDDL so the check does not depend on the language of
    /// Windows (icacls prints localised account names).
    #[cfg(windows)]
    #[test]
    fn the_windows_folder_is_owner_only_by_sid() {
        let dir = create_private_dir("n95t-").expect("a private dir");
        let saved = std::env::temp_dir().join(format!("alas-acl-{}.txt", std::process::id()));
        let mut command = std::process::Command::new(system32_tool("icacls.exe"));
        command.arg(&dir).arg("/save").arg(&saved);
        let status = alas_exec::captured::run_captured(
            &mut command,
            "icacls",
            std::time::Duration::from_secs(30),
        )
        .expect("icacls runs")
        .status;
        let bytes = std::fs::read(&saved).unwrap_or_default();
        let _ = std::fs::remove_file(&saved);
        let user = current_user_sid().expect("the token SID");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(status.success());
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let text = String::from_utf16_lossy(&units);
        let sddl = text
            .trim_start_matches(char::from_u32(0xfeff).unwrap_or(' '))
            .lines()
            .map(str::trim)
            .find(|line| line.contains("D:"))
            .expect("an SDDL line")
            .to_owned();
        let dacl = sddl.split("D:").nth(1).expect("a DACL");
        assert!(dacl.starts_with('P'), "the DACL must be protected: {sddl}");
        let aces: Vec<&str> = dacl.split('(').skip(1).collect();
        assert_eq!(aces.len(), 2, "exactly two ACEs: {sddl}");
        let sids: Vec<String> = aces
            .iter()
            .map(|ace| {
                ace.trim_end_matches(')')
                    .rsplit(';')
                    .next()
                    .unwrap_or("")
                    .to_owned()
            })
            .collect();
        assert!(sids.contains(&"SY".to_owned()), "SYSTEM missing: {sddl}");
        assert!(sids.contains(&user), "user {user} missing: {sddl}");
        for ace in &aces {
            let flags = ace.split(';').nth(1).unwrap_or("");
            assert!(!flags.contains("ID"), "inherited ACE: {sddl}");
        }
    }

    #[test]
    fn only_exact_private_root_names_are_swept() {
        assert!(is_private_root_name("n95-0123456789ab", "n95-"));
        assert!(!is_private_root_name("n95-notes", "n95-"));
        assert!(!is_private_root_name("n95-0123456789AB", "n95-"));
        assert!(!is_private_root_name("n95-0123456789abc", "n95-"));
        assert!(!is_private_root_name("x95-0123456789ab", "n95-"));
    }

    #[cfg(unix)]
    #[test]
    fn the_directory_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("alas-private-u-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("base");
        let dir = create_private_dir_in(std::slice::from_ref(&base), "n95-").expect("dir");
        let mode = std::fs::metadata(&dir).expect("meta").permissions().mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_pre_planted_name_is_never_reused() {
        let base = std::env::temp_dir().join(format!("alas-private-p-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("base");
        // Fill the namespace the way an attacker would: every name we could pick is
        // unpredictable, so planting one directory cannot capture a later request.
        let planted = base.join("n95-planted");
        std::fs::create_dir_all(&planted).expect("planted");
        let dir = create_private_dir_in(std::slice::from_ref(&base), "n95-").expect("dir");
        assert_ne!(dir, planted);
        let _ = std::fs::remove_dir_all(&base);
    }
}
