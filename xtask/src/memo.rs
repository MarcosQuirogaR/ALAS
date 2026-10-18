// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The environment that switches on the shared test-result store.
//!
//! `alas_testkit::memo` persists expensive deterministic test inputs, but only
//! when `ALAS_MEMO_KEY` is set. The key is a hash of every file that can change
//! a result (sources, manifests, fixtures, assets), so an edit starts a fresh
//! store and a stored value never outlives the code that produced it. Plain
//! `cargo test` and `cargo nextest run` leave the variable unset and compute
//! everything themselves.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Paths whose content can change what a test computes.
const HASHED: &[&str] = &[
    "crates",
    "golden",
    "assets",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
];

/// How many older keyed stores to keep beside the current one.
const KEEP_STORES: usize = 2;

/// The `ALAS_MEMO_KEY` and `ALAS_MEMO_DIR` values for `root`'s current tree,
/// including uncommitted edits.
pub fn memo_env(root: &Path) -> Result<Vec<(String, String)>, String> {
    let key = tree_key(root)?;
    let base = std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| root.join("target"), PathBuf::from)
        .join("alas-memo");
    prune(&base, &key);
    Ok(vec![
        ("ALAS_MEMO_KEY".to_owned(), key),
        (
            "ALAS_MEMO_DIR".to_owned(),
            base.to_string_lossy().into_owned(),
        ),
    ])
}

/// A 128-bit hex digest of the tracked and untracked-but-not-ignored files
/// under [`HASHED`], by path and content.
pub fn tree_key(root: &Path) -> Result<String, String> {
    let mut args = vec![
        "ls-files",
        "-z",
        "--cached",
        "--others",
        "--exclude-standard",
    ];
    args.push("--");
    args.extend(HASHED);
    let output = Command::new("git")
        .current_dir(root)
        .args(["-c", "core.quotepath=off"])
        .args(&args)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    files.sort();
    files.dedup();

    let mut hash = Fnv128::new();
    for file in &files {
        // A tracked file deleted in the working tree has no content to hash.
        let Ok(content) = std::fs::read(root.join(file)) else {
            continue;
        };
        hash.feed(file.as_bytes());
        hash.feed(&(content.len() as u64).to_le_bytes());
        // Line endings must not change the key: the checkout may be CRLF.
        hash.feed_without_cr(&content);
    }
    Ok(hash.finish())
}

/// Two FNV-1a streams with different offsets, 128 bits in all. A key, not a
/// security measure.
struct Fnv128(u64, u64);

impl Fnv128 {
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325, 0x8422_2325_cbf2_9ce4)
    }

    fn byte(&mut self, byte: u8) {
        self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(Self::PRIME);
        self.1 = (self.1 ^ u64::from(byte.rotate_left(3))).wrapping_mul(Self::PRIME);
    }

    fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.byte(byte);
        }
    }

    fn feed_without_cr(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if byte != b'\r' {
                self.byte(byte);
            }
        }
    }

    fn finish(&self) -> String {
        format!("{:016x}{:016x}", self.0, self.1)
    }
}

/// Delete all but the newest [`KEEP_STORES`] stores other than `current`.
fn prune(base: &Path, current: &str) {
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    let mut others: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy() != current)
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .collect();
    others.sort();
    let excess = others.len().saturating_sub(KEEP_STORES);
    for (_, path) in others.into_iter().take(excess) {
        let _ = std::fs::remove_dir_all(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endings_do_not_change_the_digest() {
        let digest = |bytes: &[u8]| {
            let mut hash = Fnv128::new();
            hash.feed_without_cr(bytes);
            hash.finish()
        };
        assert_eq!(digest(b"a\r\nb\r\n"), digest(b"a\nb\n"));
        assert_ne!(digest(b"a\nb\n"), digest(b"a\nc\n"));
    }

    #[test]
    fn the_key_of_this_checkout_is_stable_and_hex() {
        let root = crate::repo_root();
        let first = tree_key(&root).expect("the checkout hashes");
        assert_eq!(first.len(), 32);
        assert!(first.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(first, tree_key(&root).expect("the checkout hashes"));
    }
}
