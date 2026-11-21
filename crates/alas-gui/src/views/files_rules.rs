// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Address-bar path rules of the Files explorer: lexical normalisation, the
//! allowed roots, typed-address validation and breadcrumbs.

use std::path::{Component, Path, PathBuf};

/// Remove `.` and resolve `..` without touching the disk.
pub(crate) fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub(crate) fn lower_components(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_lowercase())
        .collect()
}

/// Whether `path` is one of `roots` or lies below one (component-wise and
/// case-insensitive; `C:\ALAS-evil` is not inside `C:\ALAS`).
pub(crate) fn within(path: &Path, roots: &[PathBuf]) -> bool {
    let wanted = lower_components(&normalize_lexical(path));
    roots.iter().any(|root| {
        let base = lower_components(&normalize_lexical(root));
        !base.is_empty() && wanted.len() >= base.len() && wanted[..base.len()] == base[..]
    })
}

/// Whether the explorer may show `path`: inside the roots by name and, when it
/// exists, also after links are resolved.
pub(crate) fn allowed(path: &Path, roots: &[PathBuf]) -> bool {
    let lexical = normalize_lexical(path);
    if !within(&lexical, roots) {
        return false;
    }
    let Ok(real) = lexical.canonicalize() else {
        return true;
    };
    let real_roots: Vec<PathBuf> = roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect();
    real_roots.is_empty() || within(&real, &real_roots)
}

/// Why a typed address was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathError {
    Empty,
    Outside,
    Missing,
}

/// Resolve an address typed in the address bar: a relative text is taken
/// from `current`; the result must exist and stay inside `roots`.
pub(crate) fn resolve_typed(
    text: &str,
    current: &Path,
    roots: &[PathBuf],
) -> Result<PathBuf, PathError> {
    let text = text.trim().trim_matches('"').trim();
    if text.is_empty() {
        return Err(PathError::Empty);
    }
    let typed = PathBuf::from(text);
    let full = if typed.is_absolute() {
        typed
    } else {
        current.join(typed)
    };
    let full = normalize_lexical(&full);
    if !allowed(&full, roots) {
        return Err(PathError::Outside);
    }
    if full.exists() {
        Ok(full)
    } else {
        Err(PathError::Missing)
    }
}

/// The clickable parts of the address bar: the outermost root containing
/// `path` first, then one segment per folder below it.
pub(crate) fn breadcrumbs(path: &Path, roots: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let path = normalize_lexical(path);
    let base = roots
        .iter()
        .map(|root| normalize_lexical(root))
        .filter(|root| within(&path, std::slice::from_ref(root)))
        .min_by_key(|root| root.components().count());
    let skip = base.as_ref().map_or(0, |root| root.components().count());
    let mut walked = PathBuf::new();
    let mut parts = Vec::new();
    for (index, component) in path.components().enumerate() {
        walked.push(component.as_os_str());
        if index + 1 >= skip.max(1) {
            let name = component.as_os_str().to_string_lossy().into_owned();
            let label = if name.is_empty() {
                walked.display().to_string()
            } else {
                name
            };
            parts.push((label, walked.clone()));
        }
    }
    parts
}

/// The parent of `path` when it is still inside the roots.
pub(crate) fn up_target(path: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    let parent = normalize_lexical(path).parent()?.to_path_buf();
    within(&parent, roots).then_some(parent)
}
