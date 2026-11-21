// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Relative-link check for the Markdown documentation.
//!
//! Every inline `[text](target)` and reference definition `[label]: target`
//! whose target is a relative path must resolve to a git-tracked file or
//! directory, and must not point into a private documentation tree (which
//! never ships). External URLs, mail links and same-page anchors are not
//! checked here; the `site/` tree has its own link check in site CI.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git_ls_files(root: &Path, pattern: Option<&str>) -> Result<Vec<String>, String> {
    let mut command = Command::new("git");
    command.current_dir(root).args(["ls-files", "-z"]);
    if let Some(pattern) = pattern {
        command.args(["--", pattern]);
    }
    let output = command
        .output()
        .map_err(|e| format!("failed to list tracked files: {e}"))?;
    if !output.status.success() {
        return Err(format!("git ls-files failed ({})", output.status));
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|item| !item.is_empty())
        .filter_map(|item| std::str::from_utf8(item).ok())
        .map(str::to_owned)
        .collect())
}

/// Tracked files and every directory that contains one.
fn tracked_paths(root: &Path) -> Result<BTreeSet<PathBuf>, String> {
    let mut paths = BTreeSet::new();
    for file in git_ls_files(root, None)? {
        let mut current = PathBuf::from(file);
        loop {
            if !paths.insert(current.clone()) {
                break;
            }
            if !current.pop() || current.as_os_str().is_empty() {
                break;
            }
        }
    }
    Ok(paths)
}

/// `%20`-style escapes in a link target.
fn percent_decode(target: &str) -> String {
    let bytes = target.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = char::from(bytes[index + 1]).to_digit(16);
            let low = char::from(bytes[index + 2]).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                out.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Link targets of one Markdown line: inline links (code spans removed) and
/// reference-style definitions.
fn link_targets(line: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let trimmed = line.trim_start();
    if trimmed.starts_with('[') {
        if let Some(close) = trimmed.find("]:") {
            let rest = trimmed[close + 2..].trim();
            let target = rest.split_whitespace().next().unwrap_or("");
            targets.push(target.trim_matches(|c| c == '<' || c == '>').to_owned());
        }
    }
    let mut stripped = String::new();
    let mut in_code = false;
    for ch in line.chars() {
        if ch == '`' {
            in_code = !in_code;
        } else if !in_code {
            stripped.push(ch);
        }
    }
    let mut rest = stripped.as_str();
    while let Some(open) = rest.find("](") {
        let after = &rest[open + 2..];
        let Some(close) = after.find(')') else {
            break;
        };
        // `[text](target "title")` and `<target>` forms.
        let target = after[..close].split_whitespace().next().unwrap_or("");
        targets.push(target.trim_matches(|c| c == '<' || c == '>').to_owned());
        rest = &after[close + 1..];
    }
    targets
}

fn is_checked(target: &str) -> bool {
    !(target.is_empty()
        || target.starts_with('#')
        || target.contains("://")
        || target.starts_with("mailto:"))
}

/// Findings for one Markdown file's text; `exists` decides whether a path
/// relative to the repository root is tracked.
pub(crate) fn check_text(file: &str, text: &str, exists: &dyn Fn(&Path) -> bool) -> Vec<String> {
    let base = Path::new(file).parent().unwrap_or(Path::new(""));
    let mut findings = Vec::new();
    let mut fence: Option<&str> = None;
    for (index, line) in text.lines().enumerate() {
        let opener = ["```", "~~~"]
            .into_iter()
            .find(|marker| line.trim_start().starts_with(marker));
        match (fence, opener) {
            (None, Some(marker)) => {
                fence = Some(marker);
                continue;
            }
            (Some(open), Some(marker)) if open == marker => {
                fence = None;
                continue;
            }
            _ => {}
        }
        if fence.is_some() {
            continue;
        }
        for target in link_targets(line) {
            if !is_checked(&target) {
                continue;
            }
            let path_part = target.split(['#', '?']).next().unwrap_or("");
            if path_part.is_empty() {
                continue;
            }
            let decoded = percent_decode(path_part);
            let joined: PathBuf = match decoded.strip_prefix('/') {
                Some(absolute) => PathBuf::from(absolute),
                None => base.join(&decoded),
            };
            let resolved = normalize(&joined);
            let text = resolved.to_string_lossy().replace('\\', "/");
            if crate::hygiene::is_internal_path(&text) {
                findings.push(format!(
                    "{file}:{}: link into a private documentation tree ({target})",
                    index + 1
                ));
            } else if !exists(&resolved) {
                findings.push(format!(
                    "{file}:{}: broken link, not a tracked path ({target})",
                    index + 1
                ));
            }
        }
    }
    findings
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

pub(crate) fn check(root: &Path) -> Result<Vec<String>, String> {
    let tracked = tracked_paths(root)?;
    let exists = |path: &Path| tracked.contains(path);
    let mut findings = Vec::new();
    for file in git_ls_files(root, Some("*.md"))? {
        if crate::hygiene::is_internal_path(&file) || file.starts_with("site/") {
            continue;
        }
        let Ok(bytes) = std::fs::read(root.join(&file)) else {
            continue;
        };
        findings.extend(check_text(&file, &String::from_utf8_lossy(&bytes), &exists));
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(file: &str, text: &str, present: &[&str]) -> Vec<String> {
        let present: Vec<PathBuf> = present.iter().map(PathBuf::from).collect();
        check_text(file, text, &|p| present.iter().any(|q| q == p))
    }

    #[test]
    fn a_resolving_relative_link_passes_and_a_missing_one_is_reported() {
        let text = "See [a](other.md#part) and [b](gone.md).\n";
        let findings = run("docs/x.md", text, &["docs/other.md"]);
        assert_eq!(
            findings,
            vec!["docs/x.md:1: broken link, not a tracked path (gone.md)"]
        );
    }

    #[test]
    fn parent_directory_and_root_relative_links_resolve() {
        let text = "[a](../README.md) [b](/docs/y.md)\n";
        assert!(run("docs/x.md", text, &["README.md", "docs/y.md"]).is_empty());
    }

    #[test]
    fn urls_anchors_code_spans_and_fences_are_ignored() {
        let text = "[u](https://example.org) [a](#top) `[c](nope.md)`\n```\n[f](nope.md)\n```\n~~~\n[g](nope.md)\n~~~\n";
        assert!(run("docs/x.md", text, &[]).is_empty());
    }

    #[test]
    fn escaped_paths_and_reference_definitions_are_checked() {
        let text = "[a](my%20file.md)\n\n[ref]: other.md \"Title\"\n[gone]: missing.md\n";
        let findings = run("docs/x.md", text, &["docs/my file.md", "docs/other.md"]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("docs/x.md:4"));
    }

    #[test]
    fn a_percent_sign_before_a_multibyte_character_does_not_panic() {
        let findings = run("docs/x.md", "[a](100%\u{e9}.md)\n", &[]);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn a_link_into_a_private_documentation_tree_is_reported() {
        let target = ["docs/", "internal/v15/RULES.md"].concat();
        let text = format!("[rules]({target})\n");
        let findings = run("README.md", &text, &[target.as_str()]);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].contains("private documentation tree"));
    }
}
