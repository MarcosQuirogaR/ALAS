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
    #[cfg(not(target_os = "windows"))]
    return Vec::new();
    #[cfg(target_os = "windows")]
    {
        let program_files = ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"]
            .into_iter()
            .filter_map(env::var_os)
            .map(PathBuf::from)
            .chain([PathBuf::from("C:/Program Files")])
            .collect::<Vec<_>>();
        let app_data = env::var_os("APPDATA").map(PathBuf::from);
        msc_roots_from_locations(&program_files, app_data.as_deref())
    }
}

#[cfg(any(target_os = "windows", test))]
pub(super) fn msc_roots_from_locations(
    program_files: &[PathBuf],
    app_data: Option<&Path>,
) -> Vec<PathBuf> {
    let mut locations = program_files
        .iter()
        .map(|root| root.join("MSC.Software/NaPa_SE"))
        .collect::<Vec<_>>();
    if let Some(root) = app_data {
        locations.push(root.join("MSC.Software/MSC Nastran and Patran Student Editions"));
    }
    let mut roots = Vec::new();
    for location in locations {
        for root in msc_versioned_roots(&location) {
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
    }
    roots.sort_by_key(|(version, _)| std::cmp::Reverse(*version));
    roots.into_iter().map(|(_, path)| path).collect()
}

#[cfg(any(target_os = "windows", test))]
fn msc_versioned_roots(location: &Path) -> Vec<(u64, PathBuf)> {
    let Ok(entries) = fs::read_dir(location) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| {
            let version = path.file_name()?.to_str()?.parse::<u64>().ok()?;
            Some((version, path))
        })
        .collect()
}
