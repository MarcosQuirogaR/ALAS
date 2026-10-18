// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `cargo xtask repin <suite> [--dry-run]`: update a suite's pins as data.
//!
//! A pin lives in `tests/golden/<suite>.json` (see `alas_testkit::pins`). When
//! an intended behaviour change moves the numbers, this command runs the
//! suite's `producer` (a Cargo command stored in that file) with
//! `ALAS_REPIN_DIR` set. In that mode `assert_pinned` records what the code
//! produces instead of asserting, and this command rewrites each pin's `value`
//! from the record. Tolerances and notes are left alone, so the JSON diff is
//! exactly the set of values that moved, ready to review.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use serde_json::{json, Map, Value};

/// Tolerance given to a pin the producer recorded but the file lacks. The
/// note makes the unreviewed state visible in the diff.
const NEW_REL_TOL: f64 = 1e-9;

pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let suite = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .ok_or("usage: cargo xtask repin <suite> [--dry-run]")?;
    if !suite
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!("`{suite}` is not a suite name"));
    }
    let path = root
        .join("tests")
        .join("golden")
        .join(format!("{suite}.json"));
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut document: Value = serde_json::from_str(&text)
        .map_err(|e| format!("{} is not valid JSON: {e}", path.display()))?;
    let producer = producer_args(&document)?;

    let record_dir =
        std::env::temp_dir().join(format!("alas-repin-{suite}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&record_dir);
    println!(
        "\ncargo {}  (recording into {})",
        producer.join(" "),
        record_dir.display()
    );
    let status = Command::new(env!("CARGO"))
        .current_dir(root)
        .args(&producer)
        .env("ALAS_REPIN_DIR", &record_dir)
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    let recorded = read_record(&record_dir.join(format!("{suite}.tsv")));
    let _ = std::fs::remove_dir_all(&record_dir);
    let recorded = recorded?;
    if recorded.is_empty() {
        return Err(format!(
            "the producer recorded no pin for `{suite}` (exit status {status}); \
             does it call assert_pinned(\"{suite}\", ..)?"
        ));
    }

    let changes = merge(&mut document, &recorded)?;
    for line in &changes.report {
        println!("{line}");
    }
    println!(
        "{} pin(s): {} changed, {} new, {} unchanged, {} not produced",
        recorded.len(),
        changes.changed,
        changes.added,
        recorded.len() - changes.changed - changes.added,
        changes.stale
    );
    if !status.success() {
        println!("note: the producer exited with {status}; check that its failures are unrelated");
    }
    if dry_run {
        println!("dry run: {} not written", path.display());
        return Ok(());
    }
    let mut out = serde_json::to_string_pretty(&document).map_err(|e| e.to_string())?;
    out.push('\n');
    std::fs::write(&path, out).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    println!("rewrote {}", path.display());
    Ok(())
}

fn producer_args(document: &Value) -> Result<Vec<String>, String> {
    let list = document
        .get("producer")
        .and_then(|producer| producer.get("cargo"))
        .and_then(Value::as_array)
        .ok_or("the pin file has no producer.cargo array")?;
    list.iter()
        .map(|arg| {
            arg.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "producer.cargo holds a non-string".to_owned())
        })
        .collect()
}

/// The recorded `key<TAB>value` lines; a key recorded twice keeps its last.
fn read_record(path: &Path) -> Result<BTreeMap<String, f64>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    let mut recorded = BTreeMap::new();
    for line in text.lines() {
        let (key, value) = line
            .split_once('\t')
            .ok_or_else(|| format!("malformed record line `{line}`"))?;
        let value: f64 = value
            .parse()
            .map_err(|_| format!("`{value}` for pin `{key}` is not a number"))?;
        if !value.is_finite() {
            return Err(format!(
                "pin `{key}` was produced as {value}; a pin is a finite number"
            ));
        }
        recorded.insert(key.to_owned(), value);
    }
    Ok(recorded)
}

struct Changes {
    report: Vec<String>,
    changed: usize,
    added: usize,
    stale: usize,
}

/// Write `recorded` into the document's `pins`, keeping tolerances and notes.
fn merge(document: &mut Value, recorded: &BTreeMap<String, f64>) -> Result<Changes, String> {
    let pins = document
        .as_object_mut()
        .ok_or("the pin file is not an object")?
        .entry("pins")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or("`pins` is not an object")?;
    let mut changes = Changes {
        report: Vec::new(),
        changed: 0,
        added: 0,
        stale: pins
            .keys()
            .filter(|key| !recorded.contains_key(*key))
            .count(),
    };
    for (key, &new) in recorded {
        match pins.get_mut(key) {
            Some(entry) => {
                let old = entry.get("value").and_then(Value::as_f64);
                if old == Some(new) {
                    continue;
                }
                let relative = old.map_or(f64::NAN, |old| {
                    (new - old) / old.abs().max(f64::MIN_POSITIVE)
                });
                changes.report.push(format!(
                    "  {key}: {} -> {new:e} ({relative:+.3e} relative)",
                    old.map_or("none".to_owned(), |old| format!("{old:e}"))
                ));
                entry["value"] = json!(new);
                changes.changed += 1;
            }
            None => {
                changes
                    .report
                    .push(format!("  {key}: NEW {new:e} (set its tolerance and note)"));
                pins.insert(
                    key.clone(),
                    json!({
                        "value": new,
                        "rel_tol": NEW_REL_TOL,
                        "abs_tol": 0.0,
                        "note": "NEW: unreviewed; state why this value and this tolerance"
                    }),
                );
                changes.added += 1;
            }
        }
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> Value {
        json!({
            "producer": { "cargo": ["test", "-p", "x"] },
            "pins": {
                "a": { "value": 1.0, "rel_tol": 1e-6, "abs_tol": 0.0, "note": "keep me" },
                "b": { "value": 2.0, "rel_tol": 1e-3, "abs_tol": 0.5, "note": "same" },
                "gone": { "value": 3.0, "rel_tol": 1e-3, "abs_tol": 0.0, "note": "n" }
            }
        })
    }

    #[test]
    fn merging_moves_values_and_keeps_tolerances_and_notes() {
        let mut doc = document();
        let recorded = BTreeMap::from([
            ("a".to_owned(), 1.5),
            ("b".to_owned(), 2.0),
            ("fresh".to_owned(), 9.0),
        ]);
        let changes = merge(&mut doc, &recorded).expect("merges");
        assert_eq!((changes.changed, changes.added, changes.stale), (1, 1, 1));
        assert_eq!(doc["pins"]["a"]["value"], json!(1.5));
        assert_eq!(doc["pins"]["a"]["rel_tol"], json!(1e-6));
        assert_eq!(doc["pins"]["a"]["note"], json!("keep me"));
        assert_eq!(doc["pins"]["b"], document()["pins"]["b"]);
        assert_eq!(doc["pins"]["gone"], document()["pins"]["gone"]);
        assert_eq!(doc["pins"]["fresh"]["rel_tol"], json!(NEW_REL_TOL));
    }

    #[test]
    fn the_producer_command_comes_from_the_file() {
        assert_eq!(
            producer_args(&document()).expect("has one"),
            ["test", "-p", "x"]
        );
        assert!(producer_args(&json!({})).is_err());
    }

    #[test]
    fn a_non_finite_record_is_refused() {
        let path = std::env::temp_dir().join(format!("alas-repin-test-{}.tsv", std::process::id()));
        std::fs::write(&path, "k\tNaN\n").expect("writes");
        assert!(read_record(&path).is_err());
        std::fs::write(&path, "k\t1e3\nk\t2e3\n").expect("writes");
        assert_eq!(read_record(&path).expect("reads")["k"], 2000.0);
        let _ = std::fs::remove_file(&path);
    }
}
