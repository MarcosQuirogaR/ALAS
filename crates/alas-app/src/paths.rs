// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/paths.py
// Reference: alas @ rust-port-baseline.

//! Central path resolution for application bundles, persistent user data, and external tools.

use std::env;
use std::path::{Path, PathBuf};

/// Environment variable specifying the real installation directory when frozen.
pub const APP_DIR_ENV: &str = "ALAS_APP_DIR";

/// Helper to discover the workspace/repository root directory in development checkouts.
pub fn repo_root() -> PathBuf {
    if let Ok(val) = env::var(APP_DIR_ENV) {
        let p = PathBuf::from(val);
        if p.exists() {
            return p;
        }
    }
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let path = PathBuf::from(manifest_dir);
        if let Some(parent) = path.parent().and_then(|p| p.parent()) {
            return parent.to_path_buf();
        }
    }
    if let Ok(cwd) = env::current_dir() {
        if cwd.join("Cargo.toml").exists() {
            return cwd;
        }
    }
    PathBuf::from(".")
}

/// Returns true when running inside a frozen desktop bundle distribution.
pub fn is_frozen() -> bool {
    env::var("ALAS_FROZEN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Root directory for read-only assets frozen into the bundle.
pub fn bundle_root() -> PathBuf {
    if let Ok(meipass) = env::var("_MEIPASS") {
        return PathBuf::from(meipass);
    }
    repo_root()
}

/// Real install directory where externally provisioned tools sit beside the executable.
pub fn app_root() -> PathBuf {
    if let Ok(val) = env::var(APP_DIR_ENV) {
        let p = PathBuf::from(val);
        if p.exists() {
            return p;
        }
    }
    if is_frozen() {
        if let Ok(exe) = env::current_exe() {
            if let Some(parent) = exe.parent() {
                return parent.to_path_buf();
            }
        }
    }
    repo_root()
}

/// Writable per-user directory for persistent data downloaded post-install.
pub fn user_data_root() -> PathBuf {
    if !is_frozen() {
        return repo_root();
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
            return PathBuf::from(local_app_data).join("ALAS");
        }
        if let Ok(user_profile) = env::var("USERPROFILE") {
            return PathBuf::from(user_profile)
                .join("AppData")
                .join("Local")
                .join("ALAS");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("ALAS");
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Ok(xdg) = env::var("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join("ALAS");
        }
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("ALAS");
        }
    }
    repo_root()
}

/// Ordered list of candidate directories to search for relative data paths.
pub fn data_roots() -> Vec<PathBuf> {
    let ordered = [app_root(), user_data_root(), bundle_root(), repo_root()];
    let mut unique = Vec::new();
    for p in ordered {
        if !unique.contains(&p) {
            unique.push(p);
        }
    }
    unique
}

/// Resolve a configured data path against known application roots.
pub fn resolve_data_path(configured: &Path) -> PathBuf {
    if configured.is_absolute() {
        return configured.to_path_buf();
    }
    for root in data_roots() {
        let candidate = root.join(configured);
        if candidate.exists() {
            return candidate;
        }
    }
    user_data_root().join(configured)
}

/// Ordered roots searched for external tool installations.
pub fn candidate_roots() -> Vec<PathBuf> {
    let root = app_root();
    let bin = root.join("bin");
    let parent = root
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| root.clone());
    let repo = repo_root();
    let ordered = [root, bin, parent, repo];
    let mut unique = Vec::new();
    for p in ordered {
        if !unique.contains(&p) {
            unique.push(p);
        }
    }
    unique
}

/// Resolve a configured tool directory against candidate root locations.
pub fn resolve_tool_dir(configured: &Path) -> PathBuf {
    let s = configured.to_string_lossy();
    if s.trim().is_empty() {
        return app_root();
    }
    if configured.is_absolute() {
        return configured.to_path_buf();
    }
    let candidates: Vec<PathBuf> = candidate_roots()
        .into_iter()
        .map(|r| r.join(configured))
        .collect();
    for candidate in &candidates {
        if candidate.exists() {
            return candidate.clone();
        }
    }
    candidates.first().cloned().unwrap_or_else(app_root)
}

/// Locate an existing external tool directory, or return `None` if not present.
pub fn find_tool_dir(configured: &Path) -> Option<PathBuf> {
    let s = configured.to_string_lossy();
    if s.trim().is_empty() {
        return None;
    }
    if configured.is_absolute() {
        return if configured.exists() {
            Some(configured.to_path_buf())
        } else {
            None
        };
    }
    for root in candidate_roots() {
        let candidate = root.join(configured);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Resolve a configured executable file path against `root`.
pub fn resolve_tool_exe(configured: &Path, root: &Path) -> Option<PathBuf> {
    let s = configured.to_string_lossy();
    if s.trim().is_empty() {
        return None;
    }
    let exe = if configured.is_absolute() {
        configured.to_path_buf()
    } else {
        root.join(configured)
    };
    if exe.is_file() {
        Some(exe)
    } else {
        None
    }
}
