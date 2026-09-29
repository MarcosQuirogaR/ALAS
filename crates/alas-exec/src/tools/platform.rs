// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

pub(super) fn versioned_directories(root: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let prefix = prefix.to_ascii_lowercase();
    let mut matches = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.to_ascii_lowercase().starts_with(&prefix))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    matches
}

pub(super) fn is_mses_dir(path: &Path) -> bool {
    ["mset.exe", "mses.exe", "mplot.exe"]
        .iter()
        .all(|name| path.join(name).is_file())
}

pub(super) fn missing_mses_programs(path: &Path) -> Vec<String> {
    ["mset.exe", "mses.exe", "mplot.exe"]
        .iter()
        .filter(|name| !path.join(name).is_file())
        .map(|name| (*name).to_owned())
        .collect()
}

pub(super) fn is_frozen() -> bool {
    env::var("ALAS_FROZEN")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub(super) fn platform_user_data_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(root) = env::var_os("LOCALAPPDATA") {
        return PathBuf::from(root).join("ALAS");
    }
    #[cfg(target_os = "macos")]
    if let Some(root) = env::var_os("HOME") {
        return PathBuf::from(root)
            .join("Library")
            .join("Application Support")
            .join("ALAS");
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    if let Some(root) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(root).join("ALAS");
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    if let Some(root) = env::var_os("HOME") {
        return PathBuf::from(root)
            .join(".local")
            .join("share")
            .join("ALAS");
    }
    PathBuf::from(".")
}

pub(super) fn installed_msc_roots() -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    let Some(app_data) = env::var_os("APPDATA") else {
        return Vec::new();
    };
    #[cfg(not(target_os = "windows"))]
    return Vec::new();
    #[cfg(target_os = "windows")]
    let editions = PathBuf::from(app_data)
        .join("MSC.Software")
        .join("MSC Nastran and Patran Student Editions");
    #[cfg(target_os = "windows")]
    let Ok(entries) = fs::read_dir(editions) else {
        return Vec::new();
    };
    #[cfg(target_os = "windows")]
    {
        let mut roots = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        roots.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
        roots
    }
}
