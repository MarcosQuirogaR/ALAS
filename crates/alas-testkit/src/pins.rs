// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Product-state pins kept as data, not as literals in test sources.
//!
//! A pin is a number a test holds a model output to: an acceptance fuel
//! figure, a calibrated centre of gravity. Written as a Rust literal it needs
//! a source edit, and a justification comment, every time an intended model
//! change moves it. Here it lives in `tests/golden/<suite>.json`:
//!
//! ```text
//! { "producer": { "cargo": ["test", "-p", "alas-x", "--test", "suite"] },
//!   "pins": { "A320-200/closure_fuel_kg":
//!             { "value": 17450.2, "rel_tol": 1e-6, "abs_tol": 0.0,
//!               "note": "closure at the declared mass, reference basis" } } }
//! ```
//!
//! A test calls [`assert_pinned`]. After an intended behaviour change,
//! `cargo xtask repin <suite>` runs the `producer` command with
//! `ALAS_REPIN_DIR` set, in which mode [`assert_pinned`] records what the code
//! produces instead of asserting, then rewrites the `value`s and leaves the
//! tolerances and notes alone. The resulting JSON diff is what gets reviewed.
//!
//! The tolerance is part of the pin, so tightening or loosening one is a
//! visible data change with its own `note`, never a silent edit of a test.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Environment variable naming the directory a repin run records into.
pub const REPIN_ENV: &str = "ALAS_REPIN_DIR";

/// One pinned number and the tolerance the test holds the code to.
#[derive(Debug, Clone, PartialEq)]
pub struct Pin {
    /// The pinned value.
    pub value: f64,
    /// Relative tolerance, applied to `|value|`.
    pub rel_tol: f64,
    /// Absolute tolerance floor, in the unit of `value`.
    pub abs_tol: f64,
    /// Why this value and this tolerance.
    pub note: String,
}

impl Pin {
    /// Whether `actual` is within the tolerance of the pinned value.
    ///
    /// A non-finite `actual` never is; a pin is a physical number.
    pub fn admits(&self, actual: f64) -> bool {
        actual.is_finite()
            && (actual - self.value).abs() <= (self.rel_tol * self.value.abs()).max(self.abs_tol)
    }
}

/// The directory holding the pin files.
pub fn pin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden")
}

/// Read the pin called `key` from `suite`'s file.
///
/// # Errors
///
/// When the file is missing or malformed, or holds no such key.
pub fn load_pin(dir: &Path, suite: &str, key: &str) -> Result<Pin, String> {
    let path = dir.join(format!("{suite}.json"));
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let document: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("{} is not valid JSON: {error}", path.display()))?;
    let entry = document
        .get("pins")
        .and_then(|pins| pins.get(key))
        .ok_or_else(|| format!("{} has no pin `{key}`", path.display()))?;
    let number = |field: &str| {
        entry
            .get(field)
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| format!("{} pin `{key}` has no numeric `{field}`", path.display()))
    };
    Ok(Pin {
        value: number("value")?,
        rel_tol: number("rel_tol")?,
        abs_tol: number("abs_tol")?,
        note: entry
            .get("note")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

/// Hold `actual` to the pin `key` of `suite`, or record it during a repin.
///
/// # Panics
///
/// When the pin is missing or `actual` is outside its tolerance (outside a
/// repin run), which is how a test reports failure.
pub fn assert_pinned(suite: &str, key: &str, actual: f64) {
    if let Some(dir) = std::env::var_os(REPIN_ENV) {
        record(Path::new(&dir), suite, key, actual);
        return;
    }
    let pin = load_pin(&pin_dir(), suite, key)
        .unwrap_or_else(|error| panic!("{error}\nadd the pin with `cargo xtask repin {suite}`"));
    assert!(
        pin.admits(actual),
        "{suite} / {key}: got {actual:.17e}, pinned {:.17e} (rel_tol {:e}, abs_tol {:e}): {}\n\
         if the change is intended, run `cargo xtask repin {suite}` and review the diff",
        pin.value,
        pin.rel_tol,
        pin.abs_tol,
        pin.note
    );
}

/// Append one `key<TAB>value` line to the suite's record file.
fn record(dir: &Path, suite: &str, key: &str, actual: f64) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let path = dir.join(format!("{suite}.tsv"));
    // One short line per write: append mode keeps concurrent test threads from
    // interleaving within a line.
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{key}\t{actual:e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin() -> Pin {
        Pin {
            value: 100.0,
            rel_tol: 1e-3,
            abs_tol: 1e-6,
            note: String::new(),
        }
    }

    #[test]
    fn a_value_inside_the_tolerance_is_admitted() {
        assert!(pin().admits(100.05));
        assert!(!pin().admits(100.2));
    }

    #[test]
    fn the_absolute_floor_governs_a_pin_at_zero() {
        let zero = Pin {
            value: 0.0,
            ..pin()
        };
        assert!(zero.admits(5e-7));
        assert!(!zero.admits(5e-6));
    }

    #[test]
    fn a_non_finite_value_is_never_admitted() {
        assert!(!pin().admits(f64::NAN));
        assert!(!pin().admits(f64::INFINITY));
    }

    #[test]
    fn pins_load_from_the_data_file_format() {
        let dir = std::env::temp_dir().join(format!("alas-pin-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("demo.json"),
            r#"{"pins":{"a/b":{"value":2.5,"rel_tol":1e-9,"abs_tol":0.0,"note":"why"}}}"#,
        )
        .unwrap();
        let loaded = load_pin(&dir, "demo", "a/b").unwrap();
        assert_eq!(loaded.value, 2.5);
        assert_eq!(loaded.note, "why");
        assert!(load_pin(&dir, "demo", "missing").is_err());
        assert!(load_pin(&dir, "absent", "a/b").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
