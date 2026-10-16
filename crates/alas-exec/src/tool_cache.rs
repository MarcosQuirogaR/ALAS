// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in, content-addressed result cache for external solver runs.
//!
//! A deterministic solver given the same input deck, the same executable and
//! the same options produces the same files. [`DirCache`] keys a run by the
//! SHA-256 of exactly those things, stores the files the run created or
//! changed (plus a small caller-defined sidecar such as captured stdout), and
//! replays them into the working directory on a later identical request.
//!
//! The cache is off unless [`RuntimeSettings::cache_dir`] names a directory,
//! so the desktop application bypasses it by default. Only runs the caller
//! reports as successful are stored, so a failure is never replayed. A hit
//! restores byte-identical outputs, which is what makes results with the cache
//! on and off bit-identical by construction; it does not skip any parsing,
//! gating or classification the caller performs afterwards.
//!
//! Scratch directories named `scr` are neither hashed nor stored (the legacy
//! solvers write many hundred megabytes there that nothing reads back), and an
//! entry larger than [`MAX_ENTRY_BYTES`] is not stored.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use crate::sha256::Sha256;
use crate::tool_runtime::RuntimeSettings;

/// Largest cache entry (sum of stored output files) that is kept.
pub const MAX_ENTRY_BYTES: u64 = 1 << 30;
/// Files larger than this are identified by length and modification time
/// instead of by content when they are inputs or executables.
const CONTENT_HASH_LIMIT: u64 = 64 << 20;
const KEY_VERSION: &[u8] = b"alas-tool-cache-v1";

static HITS: AtomicU64 = AtomicU64::new(0);
static MISSES: AtomicU64 = AtomicU64::new(0);
static STORES: AtomicU64 = AtomicU64::new(0);
static EXE_IDENTITIES: Mutex<BTreeMap<(PathBuf, u64, u64), String>> = Mutex::new(BTreeMap::new());

/// Process-wide cache counters, for run diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheStats {
    /// Runs replayed from the cache.
    pub hits: u64,
    /// Cache-enabled runs that found no entry.
    pub misses: u64,
    /// Entries written.
    pub stores: u64,
}

/// Current process-wide counters.
pub fn stats() -> CacheStats {
    CacheStats {
        hits: HITS.load(Ordering::Relaxed),
        misses: MISSES.load(Ordering::Relaxed),
        stores: STORES.load(Ordering::Relaxed),
    }
}

/// Identity of a tool executable: its content hash (or length and mtime when
/// it is too large to hash cheaply), so a rebuilt solver never reuses results.
pub fn exe_identity(exe: &Path) -> String {
    let Ok(metadata) = fs::metadata(exe) else {
        return format!("missing:{}", exe.display());
    };
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |elapsed| elapsed.as_secs());
    let memo_key = (exe.to_path_buf(), metadata.len(), mtime);
    if let Ok(memo) = EXE_IDENTITIES.lock() {
        if let Some(found) = memo.get(&memo_key) {
            return found.clone();
        }
    }
    let identity = if metadata.len() <= CONTENT_HASH_LIMIT {
        fs::read(exe).map_or_else(
            |_| format!("unreadable:{}:{mtime}", metadata.len()),
            |bytes| {
                let mut hasher = Sha256::new();
                hasher.update(&bytes);
                hasher.finalize_hex()
            },
        )
    } else {
        format!("size:{}:mtime:{mtime}", metadata.len())
    };
    if let Ok(mut memo) = EXE_IDENTITIES.lock() {
        memo.insert(memo_key, identity.clone());
    }
    identity
}

/// One cache-enabled solver invocation: look up before running, store after.
#[derive(Debug)]
pub struct DirCache {
    entry: PathBuf,
    dir: PathBuf,
    before: BTreeMap<String, String>,
}

impl DirCache {
    /// Prepare a cached invocation, or `None` when the cache is disabled or an
    /// input cannot be read (in which case the caller simply runs the tool).
    ///
    /// * `tool` names the cache namespace (`avl`, `vspaero`, ...).
    /// * `exe` is the executable; its [`exe_identity`] is part of the key.
    /// * `dir` is the working directory whose new and changed files are the
    ///   outputs.
    /// * `inputs` lists the input files; empty means every file under `dir`.
    /// * `options` are length-prefixed byte fields (arguments, stdin, thread
    ///   count, environment overrides) that change the answer.
    pub fn begin(
        tool: &str,
        exe: &Path,
        dir: &Path,
        inputs: &[PathBuf],
        options: &[&[u8]],
    ) -> Option<Self> {
        let root = RuntimeSettings::current().cache_dir?;
        let before = snapshot(dir)?;
        let mut hasher = Sha256::new();
        hasher.update_field(KEY_VERSION);
        hasher.update_field(tool.as_bytes());
        hasher.update_field(exe_identity(exe).as_bytes());
        for field in options {
            hasher.update_field(field);
        }
        if inputs.is_empty() {
            for (name, digest) in &before {
                hasher.update_field(name.as_bytes());
                hasher.update_field(digest.as_bytes());
            }
        } else {
            for path in inputs {
                let bytes = fs::read(path).ok()?;
                let name = path.file_name()?.to_string_lossy().into_owned();
                hasher.update_field(name.as_bytes());
                hasher.update_field(&bytes);
            }
        }
        let key = hasher.finalize_hex();
        let entry = root.join(tool).join(&key[..2]).join(&key);
        Some(Self {
            entry,
            dir: dir.to_path_buf(),
            before,
        })
    }

    /// Replay a stored run into the working directory and return its sidecar,
    /// or `None` on a miss (counted) or if the entry cannot be replayed.
    pub fn restore(&self) -> Option<Vec<u8>> {
        let manifest = fs::read_to_string(self.entry.join("manifest.txt")).ok();
        let Some(manifest) = manifest else {
            MISSES.fetch_add(1, Ordering::Relaxed);
            return None;
        };
        let sidecar = fs::read(self.entry.join("sidecar.bin")).ok()?;
        for line in manifest.lines() {
            let (kind, relative) = line.split_once(' ')?;
            let target = safe_join(&self.dir, relative)?;
            match kind {
                "F" => {
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent).ok()?;
                    }
                    let _ = fs::remove_file(&target);
                    fs::copy(self.entry.join("files").join(relative), &target).ok()?;
                }
                "D" => {
                    let _ = fs::remove_file(&target);
                }
                _ => return None,
            }
        }
        HITS.fetch_add(1, Ordering::Relaxed);
        Some(sidecar)
    }

    /// Store the files the run created, changed or deleted, plus `sidecar`.
    /// Call only after a successful run. Failure to store is silent: the cache
    /// is an optimisation and must never fail a run.
    pub fn store(&self, sidecar: &[u8]) {
        let _ = self.try_store(sidecar);
    }

    fn try_store(&self, sidecar: &[u8]) -> Option<()> {
        let after = snapshot(&self.dir)?;
        let temporary = self
            .entry
            .with_extension(format!("tmp{}", std::process::id()));
        let _ = fs::remove_dir_all(&temporary);
        let result = self.write_entry(&temporary, &after, sidecar);
        if result.is_none() || fs::rename(&temporary, &self.entry).is_err() {
            let _ = fs::remove_dir_all(&temporary);
            return None;
        }
        STORES.fetch_add(1, Ordering::Relaxed);
        Some(())
    }

    fn write_entry(
        &self,
        temporary: &Path,
        after: &BTreeMap<String, String>,
        sidecar: &[u8],
    ) -> Option<()> {
        fs::create_dir_all(temporary.join("files")).ok()?;
        fs::create_dir_all(self.entry.parent()?).ok()?;
        let mut manifest = String::new();
        let mut total = 0_u64;
        for (relative, digest) in after {
            if self.before.get(relative) == Some(digest) {
                continue;
            }
            let source = safe_join(&self.dir, relative)?;
            total += fs::metadata(&source).ok()?.len();
            if total > MAX_ENTRY_BYTES {
                return None;
            }
            let target = temporary.join("files").join(relative);
            fs::create_dir_all(target.parent()?).ok()?;
            fs::copy(&source, &target).ok()?;
            manifest.push_str(&format!("F {relative}\n"));
        }
        for relative in self.before.keys().filter(|name| !after.contains_key(*name)) {
            manifest.push_str(&format!("D {relative}\n"));
        }
        fs::write(temporary.join("sidecar.bin"), sidecar).ok()?;
        fs::write(temporary.join("manifest.txt"), manifest).ok()
    }
}

/// `dir` joined with a slash-separated relative path that cannot escape it.
fn safe_join(dir: &Path, relative: &str) -> Option<PathBuf> {
    let mut path = dir.to_path_buf();
    for part in relative.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(['\\', ':']) {
            return None;
        }
        path.push(part);
    }
    Some(path)
}

/// Relative slash-separated path to content digest for every file under
/// `dir`, skipping `scr` scratch directories. `None` if `dir` is unreadable.
fn snapshot(dir: &Path) -> Option<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    collect(dir, "", &mut files)?;
    Some(files)
}

fn collect(dir: &Path, prefix: &str, files: &mut BTreeMap<String, String>) -> Option<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        // A directory that does not exist yet has no files.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(()),
        Err(_) => return None,
    };
    for entry in entries {
        let entry = entry.ok()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let file_type = entry.file_type().ok()?;
        if file_type.is_dir() {
            if name != "scr" {
                collect(&entry.path(), &relative, files)?;
            }
        } else if file_type.is_file() {
            let metadata = entry.metadata().ok()?;
            let digest = if metadata.len() <= CONTENT_HASH_LIMIT {
                let mut hasher = Sha256::new();
                hasher.update(&fs::read(entry.path()).ok()?);
                hasher.finalize_hex()
            } else {
                let mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |elapsed| elapsed.as_nanos());
                format!("size:{}:mtime:{mtime}", metadata.len())
            };
            files.insert(relative, digest);
        }
    }
    Some(())
}

// A failed expectation here is a failed test assertion.
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "alas-tool-cache-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).ok();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn enable(cache: &Path) {
        RuntimeSettings::set_override(Some(RuntimeSettings {
            cache_dir: Some(cache.to_path_buf()),
            ..RuntimeSettings::default()
        }));
    }

    #[test]
    fn a_stored_run_is_replayed_byte_for_byte_option_changes_miss_and_off_means_off() {
        let scratch = Scratch::new("roundtrip");
        let cache_root = scratch.0.join("cache");
        let work = scratch.0.join("work");
        fs::create_dir_all(&work).ok();
        fs::write(work.join("deck.in"), b"deck v1").ok();
        let exe = scratch.0.join("tool.exe");
        fs::write(&exe, b"binary v1").ok();
        enable(&cache_root);
        let inputs = [work.join("deck.in")];

        let first = DirCache::begin("unit", &exe, &work, &inputs, &[b"-a"]);
        let first = first.expect("cache enabled");
        assert!(first.restore().is_none(), "empty cache misses");
        fs::write(work.join("out.ft"), b"forces").ok();
        fs::create_dir_all(work.join("sub")).ok();
        fs::write(work.join("sub").join("log.txt"), b"log").ok();
        first.store(b"stdout text");

        fs::remove_file(work.join("out.ft")).ok();
        fs::remove_dir_all(work.join("sub")).ok();
        let again = DirCache::begin("unit", &exe, &work, &inputs, &[b"-a"]).expect("enabled");
        assert_eq!(again.restore().as_deref(), Some(&b"stdout text"[..]));
        assert_eq!(
            fs::read(work.join("out.ft")).ok().as_deref(),
            Some(&b"forces"[..])
        );
        assert_eq!(
            fs::read(work.join("sub").join("log.txt")).ok().as_deref(),
            Some(&b"log"[..])
        );

        let other_option = DirCache::begin("unit", &exe, &work, &inputs, &[b"-b"]).expect("on");
        assert!(other_option.restore().is_none());
        fs::write(work.join("deck.in"), b"deck v2").ok();
        let other_deck = DirCache::begin("unit", &exe, &work, &inputs, &[b"-a"]).expect("on");
        assert!(other_deck.restore().is_none());
        fs::write(&exe, b"binary v2 longer").ok();
        let other_exe = DirCache::begin("unit", &exe, &work, &inputs, &[b"-a"]).expect("on");
        assert!(other_exe.restore().is_none());
        // Off by default (the override is process-wide, so this stays in the
        // one test that owns it).
        RuntimeSettings::set_override(Some(RuntimeSettings::default()));
        assert!(DirCache::begin("unit", &exe, &work, &[], &[]).is_none());
        RuntimeSettings::set_override(None);
    }

    #[test]
    fn paths_never_escape_the_working_directory() {
        let scratch = Scratch::new("paths");
        assert!(safe_join(&scratch.0, "../escape").is_none());
        assert!(safe_join(&scratch.0, "a/b.txt").is_some());
        assert!(safe_join(&scratch.0, "C:evil").is_none());
    }
}
