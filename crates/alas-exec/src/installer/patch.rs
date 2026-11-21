// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A byte-exact unified-diff applier for the source patches the installer ships.
//!
//! `build_from_source` tools are patched on the user's machine, which may have no
//! `git` or `patch`. The patches are plain `git diff` output over existing files
//! (no renames, creations or binary hunks), so this applier supports exactly that
//! and rejects anything else. Lines are compared as raw bytes including a trailing
//! carriage return: the legacy sources are CRLF and the patch carries the same
//! bytes, so a context mismatch means the source is not the pinned revision.

use std::fs;
use std::path::{Component, Path};

use super::InstallError;

struct Hunk {
    old_start: usize,
    old: Vec<Vec<u8>>,
    new: Vec<Vec<u8>>,
}

struct FilePatch {
    path: String,
    hunks: Vec<Hunk>,
}

fn invalid(what: impl Into<String>) -> InstallError {
    InstallError::Archive(format!("patch rejected: {}", what.into()))
}

/// `a/dir/file` or `b/dir/file` to `dir/file`.
fn strip_prefix(name: &str) -> Result<String, InstallError> {
    let name = name
        .split('\t')
        .next()
        .unwrap_or(name)
        .trim_end_matches('\r');
    let stripped = name
        .strip_prefix("a/")
        .or_else(|| name.strip_prefix("b/"))
        .ok_or_else(|| invalid(format!("unexpected path {name:?}")))?;
    let safe = Path::new(stripped)
        .components()
        .all(|part| matches!(part, Component::Normal(_)));
    if stripped.is_empty() || !safe {
        return Err(invalid(format!("unsafe path {stripped:?}")));
    }
    Ok(stripped.to_owned())
}

fn parse_range(text: &str) -> Option<usize> {
    let number = text.trim_start_matches(['-', '+']);
    number.split(',').next()?.parse().ok()
}

/// The line count of a `@@ -a,b +c,d @@` range (1 when the comma part is absent).
fn parse_count(range: &str) -> Option<usize> {
    match range.split_once(',') {
        Some((_, count)) => count.parse().ok(),
        None => Some(1),
    }
}

fn parse(patch: &[u8]) -> Result<Vec<FilePatch>, InstallError> {
    let lines: Vec<&[u8]> = patch.split(|&b| b == b'\n').collect();
    let mut files: Vec<FilePatch> = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if line.starts_with(b"--- ") {
            let old_name = String::from_utf8_lossy(&line[4..]).into_owned();
            let next = lines
                .get(index + 1)
                .filter(|next| next.starts_with(b"+++ "))
                .ok_or_else(|| invalid("--- without +++"))?;
            let new_name = String::from_utf8_lossy(&next[4..]).into_owned();
            if old_name.starts_with("/dev/null") || new_name.starts_with("/dev/null") {
                return Err(invalid("file creation and deletion are not supported"));
            }
            let path = strip_prefix(&new_name)?;
            if strip_prefix(&old_name)? != path {
                return Err(invalid("renames are not supported"));
            }
            files.push(FilePatch {
                path,
                hunks: Vec::new(),
            });
            index += 2;
        } else if line.starts_with(b"@@ ") {
            let header = String::from_utf8_lossy(line).into_owned();
            let mut fields = header.split_whitespace().skip(1);
            let old_start = fields.next().and_then(parse_range);
            let new_start = fields.next().and_then(parse_range);
            let (Some(old_start), Some(_)) = (old_start, new_start) else {
                return Err(invalid(format!("bad hunk header {header:?}")));
            };
            let mut ranges = header.split_whitespace().skip(1);
            let old_count = ranges
                .next()
                .and_then(|r| parse_count(r.trim_start_matches('-')));
            let new_count = ranges
                .next()
                .and_then(|r| parse_count(r.trim_start_matches('+')));
            let (Some(old_count), Some(new_count)) = (old_count, new_count) else {
                return Err(invalid(format!("bad hunk header {header:?}")));
            };
            let (mut old, mut new) = (Vec::new(), Vec::new());
            index += 1;
            while let Some(&body) = lines.get(index) {
                match body.first() {
                    Some(b' ') => {
                        old.push(body[1..].to_vec());
                        new.push(body[1..].to_vec());
                    }
                    Some(b'-') => old.push(body[1..].to_vec()),
                    Some(b'+') => new.push(body[1..].to_vec()),
                    Some(b'\\') => {}
                    // An editor may strip the single space of an empty context line;
                    // while the header still expects lines, it is one.
                    None if old.len() < old_count || new.len() < new_count => {
                        old.push(Vec::new());
                        new.push(Vec::new());
                    }
                    _ => break,
                }
                index += 1;
            }
            if old.len() != old_count || new.len() != new_count {
                return Err(invalid(format!(
                    "hunk {header:?} has {} old and {} new lines, not the {old_count} and {new_count} its header states",
                    old.len(),
                    new.len()
                )));
            }
            let file = files
                .last_mut()
                .ok_or_else(|| invalid("hunk before any file header"))?;
            file.hunks.push(Hunk {
                old_start,
                old,
                new,
            });
        } else {
            index += 1;
        }
    }
    if files.is_empty() || files.iter().any(|file| file.hunks.is_empty()) {
        return Err(invalid("no hunks"));
    }
    Ok(files)
}

fn find_hunk(lines: &[&[u8]], hunk: &Hunk, cursor: usize) -> Option<usize> {
    let matches_at = |at: usize| {
        hunk.old.len() + at <= lines.len()
            && hunk
                .old
                .iter()
                .zip(&lines[at..])
                .all(|(want, got)| want == got)
    };
    let nominal = hunk.old_start.saturating_sub(1).max(cursor);
    if matches_at(nominal) {
        return Some(nominal);
    }
    // The pinned revision should match in place; accept a unique shifted match.
    let mut found = (cursor..lines.len()).filter(|&at| matches_at(at));
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

fn apply_file(original: &[u8], file: &FilePatch) -> Result<Vec<u8>, InstallError> {
    let lines: Vec<&[u8]> = original.split(|&b| b == b'\n').collect();
    let mut out: Vec<u8> = Vec::with_capacity(original.len());
    let mut cursor = 0;
    for hunk in &file.hunks {
        let at = find_hunk(&lines, hunk, cursor).ok_or_else(|| {
            invalid(format!(
                "{}: the hunk at line {} does not match the source (not the pinned revision?)",
                file.path, hunk.old_start
            ))
        })?;
        for line in &lines[cursor..at] {
            out.extend_from_slice(line);
            out.push(b'\n');
        }
        for line in &hunk.new {
            out.extend_from_slice(line);
            out.push(b'\n');
        }
        cursor = at + hunk.old.len();
    }
    // The last element of `lines` is the text after the final newline (possibly
    // empty); the remainder is copied as it is.
    let remainder = &lines[cursor..];
    for (position, line) in remainder.iter().enumerate() {
        out.extend_from_slice(line);
        if position + 1 < remainder.len() {
            out.push(b'\n');
        }
    }
    // A hunk consumed the final empty element only if the file had no trailing
    // newline and the hunk reached the end; both shapes keep the source's ending.
    if cursor >= lines.len() && out.last() == Some(&b'\n') && !original.ends_with(b"\n") {
        out.pop();
    }
    Ok(out)
}

/// Apply `patch` to the files below `root`. Every file is patched in memory first,
/// so a mismatch leaves the tree untouched. Returns the patched file names.
pub(super) fn apply_unified(root: &Path, patch: &[u8]) -> Result<Vec<String>, InstallError> {
    let files = parse(patch)?;
    let mut staged = Vec::new();
    for file in &files {
        let target = root.join(&file.path);
        let original = fs::read(&target).map_err(|error| {
            InstallError::Io(format!("cannot read {}: {error}", target.display()))
        })?;
        staged.push((target, apply_file(&original, file)?));
    }
    for (target, bytes) in &staged {
        fs::write(target, bytes).map_err(|error| {
            InstallError::Io(format!("cannot write {}: {error}", target.display()))
        })?;
    }
    Ok(files.into_iter().map(|file| file.path).collect())
}

#[cfg(test)]
// The tests build fixtures and assert on them; a failed setup call is a test-authoring error.
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn temp(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("alas-patch-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    const DIFF: &str = "diff --git a/f.txt b/f.txt\nindex 1..2 100644\n--- a/f.txt\n+++ b/f.txt\n@@ -2,3 +2,3 @@ ctx\n b\r\n-c\r\n+C\r\n d\r\n";

    #[test]
    fn a_crlf_hunk_applies_byte_exactly_and_keeps_the_rest() {
        let root = temp("ok");
        fs::write(root.join("f.txt"), b"a\r\nb\r\nc\r\nd\r\ne\r\n").expect("write");
        let files = apply_unified(&root, DIFF.as_bytes()).expect("applies");
        assert_eq!(files, ["f.txt"]);
        assert_eq!(
            fs::read(root.join("f.txt")).expect("read"),
            b"a\r\nb\r\nC\r\nd\r\ne\r\n"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_hunk_shorter_than_its_header_is_rejected_not_silently_truncated() {
        let root = temp("short");
        fs::write(root.join("f.txt"), b"a\nb\nc\nd\ne\n").expect("write");
        let diff = "--- a/f.txt\n+++ b/f.txt\n@@ -2,3 +2,3 @@\n b\n-c\n";
        let error = apply_unified(&root, diff.as_bytes()).expect_err("truncated hunk");
        assert!(error.to_string().contains("its header states"), "{error}");
        assert_eq!(
            fs::read(root.join("f.txt")).expect("read"),
            b"a\nb\nc\nd\ne\n"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_context_line_whose_space_an_editor_stripped_still_counts() {
        let root = temp("blank");
        fs::write(root.join("f.txt"), b"a\n\nc\nd\n").expect("write");
        let diff = "--- a/f.txt\n+++ b/f.txt\n@@ -1,3 +1,3 @@\n a\n\n-c\n+C\n";
        apply_unified(&root, diff.as_bytes()).expect("applies");
        assert_eq!(fs::read(root.join("f.txt")).expect("read"), b"a\n\nC\nd\n");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_shifted_unique_hunk_applies_and_a_mismatch_changes_nothing() {
        let root = temp("shift");
        fs::write(root.join("f.txt"), b"x\r\na\r\nb\r\nc\r\nd\r\n").expect("write");
        apply_unified(&root, DIFF.as_bytes()).expect("unique shifted match");
        assert_eq!(
            fs::read(root.join("f.txt")).expect("read"),
            b"x\r\na\r\nb\r\nC\r\nd\r\n"
        );
        let other = b"a\nb\nZ\nd\n";
        fs::write(root.join("f.txt"), other).expect("write");
        let error = apply_unified(&root, DIFF.as_bytes()).expect_err("mismatch");
        assert!(error.to_string().contains("does not match"), "{error}");
        assert_eq!(fs::read(root.join("f.txt")).expect("read"), other);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn creations_renames_and_unsafe_paths_are_rejected() {
        let root = temp("reject");
        for bad in [
            "--- /dev/null\n+++ b/n.txt\n@@ -0,0 +1 @@\n+x\n",
            "--- a/x\n+++ b/y\n@@ -1 +1 @@\n-a\n+b\n",
            "--- a/../x\n+++ b/../x\n@@ -1 +1 @@\n-a\n+b\n",
        ] {
            assert!(apply_unified(&root, bad.as_bytes()).is_err(), "{bad}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_shipped_nastran95_patch_parses() {
        let patch = include_bytes!("../../../../tools/nastran95/alas-local.patch");
        let files = parse(patch).expect("parses");
        let names: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(names, ["CMakeLists.txt", "bin/nastrn.f"]);
    }
}
