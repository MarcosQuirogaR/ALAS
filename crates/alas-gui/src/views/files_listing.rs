// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reading folders for the Files explorer (worker-thread code): one listing
//! and the breadth-first subfolder search.

use std::path::Path;

use super::{classify, Entry};

fn unix_seconds(time: std::io::Result<std::time::SystemTime>) -> Option<u64> {
    time.ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|elapsed| elapsed.as_secs())
}

fn read_head(path: &Path) -> Vec<u8> {
    use std::io::Read;
    let mut buffer = vec![0_u8; 1024];
    let read = std::fs::File::open(path)
        .and_then(|mut file| file.read(&mut buffer))
        .unwrap_or(0);
    buffer.truncate(read);
    buffer
}

/// List one folder. `sniff` reads the start of each JSON file to learn its kind.
/// Runs on a worker thread: it reads the disk.
pub(crate) fn list_dir(dir: &Path, sniff: bool) -> Result<Vec<Entry>, String> {
    let entries = std::fs::read_dir(dir).map_err(|error| error.to_string())?;
    let mut out = Vec::new();
    for item in entries.flatten() {
        let path = item.path();
        let meta = item.metadata().ok();
        let is_dir = meta.as_ref().is_some_and(|meta| meta.is_dir())
            || meta
                .as_ref()
                .is_some_and(|meta| meta.file_type().is_symlink())
                && path.is_dir();
        let name = item.file_name().to_string_lossy().into_owned();
        let run_folder =
            is_dir && (name.starts_with("run-") || path.join("run_manifest.json").is_file());
        let is_json = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
        let head = if sniff && !is_dir && is_json {
            read_head(&path)
        } else {
            Vec::new()
        };
        out.push(Entry {
            kind: classify(&path, is_dir, run_folder, &head),
            lower: name.to_lowercase(),
            size: if is_dir {
                0
            } else {
                meta.as_ref().map_or(0, std::fs::Metadata::len)
            },
            modified: meta.and_then(|meta| unix_seconds(meta.modified())),
            name,
            path,
        });
    }
    Ok(out)
}

/// Items of `dir` and its subfolders whose name contains `query` (lower
/// case), at most `limit`, breadth first. Names are paths relative to `dir`.
pub(crate) fn search_tree(dir: &Path, query: &str, limit: usize) -> (Vec<Entry>, bool) {
    const MAX_DEPTH: usize = 8;
    let mut found = Vec::new();
    let mut queue = std::collections::VecDeque::from([(dir.to_path_buf(), 0_usize)]);
    while let Some((folder, depth)) = queue.pop_front() {
        let Ok(items) = list_dir(&folder, false) else {
            continue;
        };
        for mut item in items {
            if item.is_folder() && depth < MAX_DEPTH {
                queue.push_back((item.path.clone(), depth + 1));
            }
            if item.lower.contains(query) {
                if found.len() >= limit {
                    return (found, true);
                }
                item.name = item
                    .path
                    .strip_prefix(dir)
                    .map_or(item.name.clone(), |relative| relative.display().to_string());
                found.push(item);
            }
        }
    }
    (found, false)
}
