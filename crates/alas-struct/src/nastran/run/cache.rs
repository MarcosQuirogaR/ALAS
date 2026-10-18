// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in result cache (`ALAS_TOOL_CACHE_DIR`) for MSC Nastran solves.
//!
//! The deck, the launcher, the solver and the command-line options determine
//! the output files. Directory names are masked out of the options so two run
//! directories holding the same deck share an entry.

use std::path::{Path, PathBuf};

use alas_exec::tool_cache::{exe_identity, DirCache};

/// Cache handle for one solve, or `None` when the cache is off or the deck's
/// input set cannot be established completely.
pub(super) fn begin(
    exe_path: &Path,
    solver_path: Option<&Path>,
    bdf_path: &Path,
    work_dir: &Path,
    arguments: &[String],
    directory_text: &str,
) -> Option<DirCache> {
    let masked_arguments = arguments
        .join("\u{1f}")
        .replace(directory_text, "<dir>")
        .replace(&work_dir.display().to_string(), "<dir>");
    let solver_identity = solver_path.map(exe_identity).unwrap_or_default();
    // The deck and everything it INCLUDEs are inputs: the production SOL 101,
    // 103 and 111 decks are thin wrappers around a shared `wing_mesh.bdf`, so
    // hashing the wrapper alone would return one aircraft's answer for another.
    let inputs = deck_files(bdf_path)?;
    DirCache::begin(
        "msc_nastran",
        exe_path,
        work_dir,
        &inputs,
        &[masked_arguments.as_bytes(), solver_identity.as_bytes()],
    )
}

/// The deck and every file it reaches through `INCLUDE` (recursively), resolved
/// against the deck's directory, which is the solver's working directory.
/// `None` when an included file is missing or unreadable or the nesting is
/// implausibly deep, so the caller runs the solver uncached rather than key on
/// an incomplete input set.
fn deck_files(bdf_path: &Path) -> Option<Vec<PathBuf>> {
    let directory = bdf_path.parent().unwrap_or(Path::new("."));
    let mut files = vec![bdf_path.to_path_buf()];
    let mut next = 0;
    while next < files.len() {
        if files.len() > 64 {
            return None;
        }
        let text = std::fs::read(&files[next]).ok()?;
        for line in String::from_utf8_lossy(&text).lines() {
            let trimmed = line.trim_start();
            let Some(head) = trimmed.get(..7) else {
                continue;
            };
            if !head.eq_ignore_ascii_case("INCLUDE") {
                continue;
            }
            let rest = trimmed[7..].trim();
            let name = match rest.find(['\'', '"']) {
                Some(start) => {
                    let quote = rest.as_bytes()[start] as char;
                    let body = &rest[start + 1..];
                    body[..body.find(quote)?].to_owned()
                }
                None => rest.to_owned(),
            };
            let included = directory.join(name.trim());
            if !included.is_file() {
                return None;
            }
            if !files.contains(&included) {
                files.push(included);
            }
        }
        next += 1;
    }
    Some(files)
}

// A failed unwrap here is a failed test assertion.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decks_include_files_are_cache_inputs_and_a_missing_one_disables_caching() {
        let scratch = std::env::temp_dir().join(format!("alas-deck-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("sol101")).unwrap();
        std::fs::write(scratch.join("mesh.bdf"), "GRID,1\n").unwrap();
        std::fs::write(
            scratch.join("sol101").join("loads.bdf"),
            "include 'mesh.bdf'\n",
        )
        .unwrap();
        let deck = scratch.join("sol101").join("wing.bdf");
        std::fs::write(
            &deck,
            "SOL 101\nCEND\nBEGIN BULK\n  INCLUDE '../mesh.bdf'\nINCLUDE \"loads.bdf\"\n",
        )
        .unwrap();
        // The nested `include 'mesh.bdf'` resolves against the deck directory,
        // where no such file exists yet.
        let files = deck_files(&deck);
        assert!(files.is_none(), "{files:?}");
        std::fs::write(scratch.join("sol101").join("mesh.bdf"), "GRID,1\n").unwrap();
        let files = deck_files(&deck).unwrap();
        assert_eq!(files.len(), 4, "{files:?}");
        std::fs::write(&deck, "SOL 101\nCEND\nBEGIN BULK\nGRID,1\n").unwrap();
        assert_eq!(deck_files(&deck).unwrap(), vec![deck.clone()]);
        let _ = std::fs::remove_dir_all(&scratch);
    }
}
