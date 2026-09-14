// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per-file repository checks.
//!
//! Each of these encodes a rule from CONTRIBUTING.md that a formatter cannot
//! express. They are deliberately mechanical: passing them is the floor, not
//! the standard, and none of them is a substitute for reading the change.

use std::path::{Path, PathBuf};

/// The SPDX and copyright lines every file opens with.
const HEADER_LINES: usize = 2;

/// Phrases that describe the change rather than the code.
///
/// The list is short on purpose. A longer one would catch more, at the cost of
/// flagging prose that happens to contain a common word, and a check that cries
/// wolf gets suppressed rather than heeded.
const BANNED_PHRASES: &[&str] = &[
    "as requested",
    "per the review",
    "per your request",
    "now uses",
    "now returns",
    "now handles",
    "previously this",
    "used to be",
    "note that we",
    "here we",
    "we changed",
    "this was changed",
    "for backwards compat",
];

/// Every Rust source file the checks apply to.
pub fn rust_sources(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for dir in ["crates", "xtask"] {
        let path = root.join(dir);
        if path.is_dir() {
            collect(&path, &mut out)?;
        }
    }
    out.sort();
    Ok(out)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        let path = entry.path();

        if path.is_dir() {
            // Build output is generated, and is not ours to police.
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Apply every per-file check, returning one message per finding.
pub fn check_file(root: &Path, path: &Path, text: &str) -> Vec<String> {
    let display = path
        .strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/");
    let lines: Vec<&str> = text.lines().collect();

    let mut findings = Vec::new();
    findings.extend(check_ascii(&display, &lines));
    findings.extend(check_license_header(&display, &lines));
    findings.extend(check_banned_phrases(&display, &lines));
    findings.extend(check_allow_is_explained(&display, &lines));
    findings
}

/// Source files stay ASCII so they behave identically on every platform and
/// editor. Prose that needs a Greek letter or an accent belongs in the
/// documentation, which is under no such constraint.
fn check_ascii(display: &str, lines: &[&str]) -> Vec<String> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.is_ascii())
        .map(|(i, line)| {
            let column = line.chars().position(|c| !c.is_ascii()).unwrap_or(0) + 1;
            format!("{display}:{}:{column}: non-ASCII character", i + 1)
        })
        .collect()
}

/// Every source file opens with the two-line licence header, which is what
/// makes the licence claim in NOTICE checkable rather than asserted.
fn check_license_header(display: &str, lines: &[&str]) -> Vec<String> {
    let has_spdx = lines
        .first()
        .is_some_and(|l| l.contains("SPDX-License-Identifier"));
    let has_copyright = lines.get(1).is_some_and(|l| l.contains("Copyright (C)"));

    if has_spdx && has_copyright {
        Vec::new()
    } else {
        vec![format!("{display}: missing the SPDX and copyright header")]
    }
}

/// Whether `phrase` occurs in `text` as whole words. A match that begins or
/// ends inside a longer word -- the tail of "was", the tail of "where" -- is
/// ordinary prose, not change narration.
fn contains_phrase(text: &str, phrase: &str) -> bool {
    text.match_indices(phrase).any(|(start, matched)| {
        let before = text[..start].chars().next_back();
        let after = text[start + matched.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

fn check_banned_phrases(display: &str, lines: &[&str]) -> Vec<String> {
    let mut findings = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !is_comment(line) {
            continue;
        }
        let lowered = line.to_lowercase();
        for phrase in BANNED_PHRASES {
            if contains_phrase(&lowered, phrase) {
                findings.push(format!(
                    "{display}:{}: comment narrates the change: \"{phrase}\"",
                    i + 1
                ));
            }
        }
    }
    findings
}

/// An unexplained `allow` is an unreviewed decision, so the check asks for the
/// sentence that would have been said in review.
fn check_allow_is_explained(display: &str, lines: &[&str]) -> Vec<String> {
    let mut findings = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let is_attribute = trimmed.starts_with("#[") || trimmed.starts_with("#![");
        if !is_attribute || !trimmed.contains("allow(") {
            continue;
        }
        // The licence header is a comment too, so an allow placed directly
        // under it would otherwise look explained by it.
        let explained = lines[..i]
            .iter()
            .enumerate()
            .rev()
            .find(|(_, previous)| !previous.trim().is_empty())
            .is_some_and(|(index, previous)| index >= HEADER_LINES && is_comment(previous));

        if !explained {
            findings.push(format!(
                "{display}:{}: allow without a reason above it",
                i + 1
            ));
        }
    }
    findings
}

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn check(text: &str) -> Vec<String> {
        let root = PathBuf::from("/repo");
        check_file(&root, &root.join("crates/a/src/lib.rs"), text)
    }

    const HEADER: &str = "// SPDX-License-Identifier: AGPL-3.0-or-later\n\
                          // Copyright (C) 2026 Marcos Quiroga Rodriguez\n";

    #[test]
    fn accepts_a_conforming_file() {
        let text = format!("{HEADER}\n//! A module.\n\npub fn f() {{}}\n");
        assert!(check(&text).is_empty());
    }

    #[test]
    fn rejects_a_missing_header() {
        let findings = check("pub fn f() {}\n");
        assert!(findings.iter().any(|f| f.contains("SPDX")));
    }

    #[test]
    fn rejects_non_ascii() {
        let text = format!("{HEADER}// alpha is \u{3b1}\n");
        assert!(check(&text).iter().any(|f| f.contains("non-ASCII")));
    }

    #[test]
    fn a_banned_phrase_inside_a_longer_word_is_ordinary_prose() {
        let text = format!(
            "{HEADER}// Free transition was requested but no map resolved.\n\
             // A uniform station where weighting is invariant.\n"
        );
        assert!(!check(&text).iter().any(|f| f.contains("narrates")));
    }

    #[test]
    fn rejects_change_narration_in_a_comment() {
        let text = format!("{HEADER}// This now uses a spline.\n");
        assert!(check(&text).iter().any(|f| f.contains("narrates")));
    }

    #[test]
    fn ignores_a_banned_phrase_outside_a_comment() {
        let text = format!("{HEADER}const M: &str = \"now uses\";\n");
        assert!(!check(&text).iter().any(|f| f.contains("narrates")));
    }

    #[test]
    fn rejects_an_unexplained_allow() {
        let text = format!("{HEADER}#[allow(dead_code)]\npub fn f() {{}}\n");
        assert!(check(&text)
            .iter()
            .any(|f| f.contains("allow without a reason")));
    }

    #[test]
    fn accepts_an_explained_allow() {
        let text = format!(
            "{HEADER}\n// Kept for the parity fixture.\n#[allow(dead_code)]\npub fn f() {{}}\n"
        );
        assert!(!check(&text)
            .iter()
            .any(|f| f.contains("allow without a reason")));
    }

    #[test]
    fn does_not_accept_the_license_header_as_an_explanation() {
        let text = format!("{HEADER}#[allow(dead_code)]\npub fn f() {{}}\n");
        assert!(check(&text)
            .iter()
            .any(|f| f.contains("allow without a reason")));
    }
}
