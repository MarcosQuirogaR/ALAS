// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A cross-process result store for expensive, deterministic test inputs.
//!
//! Nextest runs every test in its own process, so a process-wide cache never
//! shares a nominal assessment between the dozens of tests that compute the
//! same one. This store persists the result as JSON on disk instead, under a
//! key that identifies the source tree, so the first test to need a value
//! computes it, the others that need it at the same moment wait on a file
//! lock, and every later test reads it back.
//!
//! The store is **off unless `ALAS_MEMO_KEY` is set**. `cargo xtask test` and
//! the gate set it to a hash of the workspace sources, so a stored value can
//! never outlive the code that produced it; a plain `cargo test` or `cargo
//! nextest run` computes everything itself, exactly as before. The value a
//! test reads back is the value it would have computed: `f64` fields
//! round-trip bit for bit (`float_roundtrip`), and [`lossless_f64`] keeps
//! `NaN` and the infinities, which JSON cannot spell, from becoming `null`.
//!
//! Only deterministic computations belong here. Anything that depends on the
//! wall clock, the worker count or the network must not be cached.

use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Environment variable holding the tree hash that enables the store.
pub const KEY_ENV: &str = "ALAS_MEMO_KEY";
/// Environment variable naming the directory that holds the keyed
/// sub-directories; defaults to `<target>/alas-memo`.
pub const DIR_ENV: &str = "ALAS_MEMO_DIR";

/// A lock file older than this was left by a process that died while holding
/// it (the longest cached computation takes minutes), and is taken over.
const STALE_LOCK: Duration = Duration::from_secs(60 * 60);
/// How often a waiting process looks at the lock again.
const POLL: Duration = Duration::from_millis(150);

/// The directory holding this tree's stored results, or `None` when the store
/// is disabled.
pub fn store_dir() -> Option<PathBuf> {
    let key = std::env::var(KEY_ENV).ok().filter(|key| !key.is_empty())?;
    let base = std::env::var_os(DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("CARGO_TARGET_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"))
                .join("alas-memo")
        });
    Some(base.join(key))
}

/// The stored value called `name`, computing and storing it on first use.
///
/// With the store disabled this is exactly `compute()`. A value that cannot be
/// read back (a truncated file, a changed type under an unchanged key) is
/// recomputed, never trusted.
pub fn cached<T: Serialize + DeserializeOwned>(name: &str, compute: impl FnOnce() -> T) -> T {
    match store_dir() {
        Some(dir) => cached_in(&dir, name, compute),
        None => compute(),
    }
}

/// As [`cached`], in an explicit directory.
pub fn cached_in<T: Serialize + DeserializeOwned>(
    dir: &Path,
    name: &str,
    compute: impl FnOnce() -> T,
) -> T {
    let file = dir.join(format!("{}.json", sanitize(name)));
    if let Some(value) = read(&file) {
        return value;
    }
    if fs::create_dir_all(dir).is_err() {
        return compute();
    }
    let _lock = Lock::acquire(&file.with_extension("lock"));
    // Whoever held the lock may have stored the value while this process waited.
    if let Some(value) = read(&file) {
        return value;
    }
    let value = compute();
    write(&file, &value);
    value
}

/// A 128-bit hexadecimal digest (two FNV-1a streams with different offsets) of
/// `parts`, for naming a stored value after the inputs that determine it.
///
/// Each part is length-prefixed, so `["ab", "c"]` and `["a", "bc"]` differ. It
/// is a key, not a security measure: the store is local and trusted.
pub fn digest(parts: &[&str]) -> String {
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let (mut a, mut b) = (0xcbf2_9ce4_8422_2325_u64, 0x84222325_cbf29ce4_u64);
    let mut feed = |byte: u8| {
        a = (a ^ u64::from(byte)).wrapping_mul(PRIME);
        b = (b ^ u64::from(byte.rotate_left(3))).wrapping_mul(PRIME);
    };
    for part in parts {
        for byte in (part.len() as u64).to_le_bytes() {
            feed(byte);
        }
        for byte in part.bytes() {
            feed(byte);
        }
    }
    format!("{a:016x}{b:016x}")
}

/// A file name that is the same on every platform for `name`.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn read<T: DeserializeOwned>(file: &Path) -> Option<T> {
    let text = fs::read_to_string(file).ok()?;
    serde_json::from_str(&text).ok()
}

fn write<T: Serialize>(file: &Path, value: &T) {
    let Ok(text) = serde_json::to_string(value) else {
        return;
    };
    // Write beside the target and rename, so a reader never sees half a file.
    let temporary = file.with_extension(format!("tmp{}", std::process::id()));
    if fs::write(&temporary, text).is_ok() && fs::rename(&temporary, file).is_err() {
        let _ = fs::remove_file(&temporary);
    }
}

/// An exclusive lock held by creating a file, released on drop (including
/// during a panic, so a failing computation does not wedge the others).
struct Lock {
    path: Option<PathBuf>,
}

impl Lock {
    fn acquire(path: &Path) -> Self {
        loop {
            match OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(_) => {
                    return Self {
                        path: Some(path.to_owned()),
                    }
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    let age = fs::metadata(path)
                        .and_then(|meta| meta.modified())
                        .ok()
                        .and_then(|time| SystemTime::now().duration_since(time).ok());
                    if age.is_some_and(|age| age > STALE_LOCK) {
                        let _ = fs::remove_file(path);
                    } else {
                        std::thread::sleep(POLL);
                    }
                }
                // No lock is available (read-only directory, say): compute
                // without one rather than fail the test.
                Err(_) => return Self { path: None },
            }
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = fs::remove_file(path);
        }
    }
}

/// `serde(with = "...")` support for an `f64` that may be `NaN` or infinite.
///
/// JSON has no spelling for them and `serde_json` writes `null`, which cannot
/// be read back as a number. Finite values stay JSON numbers.
pub mod lossless_f64 {
    use serde::de::{self, Visitor};
    use serde::{Deserializer, Serializer};

    /// Serialize `value`, spelling non-finite values as strings.
    pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
        if value.is_finite() {
            serializer.serialize_f64(*value)
        } else if value.is_nan() {
            serializer.serialize_str("NaN")
        } else if *value > 0.0 {
            serializer.serialize_str("inf")
        } else {
            serializer.serialize_str("-inf")
        }
    }

    struct Number;

    impl Visitor<'_> for Number {
        type Value = f64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a number or one of \"NaN\", \"inf\", \"-inf\"")
        }

        fn visit_f64<E: de::Error>(self, value: f64) -> Result<f64, E> {
            Ok(value)
        }

        fn visit_i64<E: de::Error>(self, value: i64) -> Result<f64, E> {
            Ok(value as f64)
        }

        fn visit_u64<E: de::Error>(self, value: u64) -> Result<f64, E> {
            Ok(value as f64)
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<f64, E> {
            match value {
                "NaN" => Ok(f64::NAN),
                "inf" => Ok(f64::INFINITY),
                "-inf" => Ok(f64::NEG_INFINITY),
                other => Err(E::custom(format!("not a number: {other}"))),
            }
        }
    }

    /// Read what [`serialize`] wrote.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
        deserializer.deserialize_any(Number)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[derive(Debug, PartialEq, Serialize, serde::Deserialize)]
    struct Sample {
        #[serde(with = "lossless_f64")]
        a: f64,
        #[serde(with = "lossless_f64")]
        b: f64,
        c: Vec<f64>,
        d: Result<f64, String>,
    }

    /// A fresh directory under the system temp dir, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "alas-memo-test-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&path);
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_stored_value_is_computed_once_and_read_back_bit_for_bit() {
        let scratch = Scratch::new("once");
        let calls = AtomicUsize::new(0);
        let make = || {
            calls.fetch_add(1, Ordering::SeqCst);
            Sample {
                a: f64::NAN,
                b: f64::NEG_INFINITY,
                c: vec![0.1 + 0.2, 1.0019999742507935, -0.0],
                d: Err("rejected".to_owned()),
            }
        };
        let first = cached_in(&scratch.0, "case/one", make);
        let second = cached_in(&scratch.0, "case/one", make);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(second.a.is_nan() && second.b == f64::NEG_INFINITY);
        for (x, y) in first.c.iter().zip(&second.c) {
            assert_eq!(x.to_bits(), y.to_bits());
        }
        assert_eq!(first.d, second.d);
    }

    #[test]
    fn concurrent_requests_for_one_value_compute_it_once() {
        let scratch = Scratch::new("race");
        let calls = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    let value: f64 = cached_in(&scratch.0, "slow", || {
                        calls.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(300));
                        42.5
                    });
                    assert_eq!(value, 42.5);
                });
            }
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn an_unreadable_file_is_recomputed_not_trusted() {
        let scratch = Scratch::new("corrupt");
        fs::create_dir_all(&scratch.0).unwrap();
        fs::write(scratch.0.join("x.json"), "{ truncated").unwrap();
        let value: f64 = cached_in(&scratch.0, "x", || 7.0);
        assert_eq!(value, 7.0);
        let again: f64 = cached_in(&scratch.0, "x", || 9.0);
        assert_eq!(again, 7.0);
    }

    #[test]
    fn a_panicking_computation_releases_the_lock() {
        let scratch = Scratch::new("panic");
        let failed = std::panic::catch_unwind(|| {
            cached_in::<f64>(&scratch.0, "p", || panic!("computation failed"))
        });
        assert!(failed.is_err());
        let value: f64 = cached_in(&scratch.0, "p", || 3.0);
        assert_eq!(value, 3.0);
    }

    #[test]
    fn the_digest_separates_parts_and_is_stable() {
        assert_ne!(digest(&["ab", "c"]), digest(&["a", "bc"]));
        assert_eq!(digest(&["x"]), digest(&["x"]));
        assert_eq!(digest(&["x"]).len(), 32);
    }

    #[test]
    fn names_become_safe_file_names() {
        assert_eq!(
            sanitize("A320-200/fixed requirement"),
            "A320-200_fixed_requirement"
        );
    }
}
