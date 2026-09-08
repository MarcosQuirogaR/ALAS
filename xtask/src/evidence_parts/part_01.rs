// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

#[path = "../evidence/consumers.rs"]
mod consumers;

use self::consumers::fixture_consumers;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const NUMERIC_TIERS: [&str; 7] = [
    "exact", "closed", "linalg", "f32", "iter", "stat", "converge",
];
const EM_DASH: &str = "\u{2014}";

/// Return objective evidence-bookkeeping findings, without judging numbers.
pub fn check(root: &Path) -> Result<Vec<String>, String> {
    let ledger_path = root.join("docs").join("PORTING.md");
    let ledger = fs::read_to_string(&ledger_path)
        .map_err(|e| format!("cannot read {}: {e}", ledger_path.display()))?;
    let mut findings = audit_ledger(root, &ledger);
    let references = ledger_fixture_references(&ledger);
    let consumers = fixture_consumers(root)?;

    for manifest in manifest_paths(&root.join("golden"))? {
        findings.extend(audit_manifest(root, &manifest)?);
    }
    findings.extend(audit_generators(root)?);
    for reference in &references {
        findings.extend(audit_fixture_reference(root, reference, &consumers)?);
    }
    let linked: BTreeSet<FixtureId> = references
        .iter()
        .filter_map(|reference| reference.fixture.clone())
        .collect();
    for consumer in &consumers {
        findings.extend(audit_fixture_consumer(root, consumer, &linked)?);
    }

    findings.sort();
    findings.dedup();
    Ok(findings)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FixtureId {
    family: String,
    file: String,
    manifest_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FixtureReference {
    family: String,
    fixture: Option<FixtureId>,
    source_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FixtureConsumer {
    fixture: FixtureId,
    source_path: String,
    source_line: usize,
}

fn audit_ledger(root: &Path, ledger: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let mut sources: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut targets: BTreeMap<String, Vec<usize>> = BTreeMap::new();

    for (line_index, line) in ledger.lines().enumerate() {
        let fields: Vec<&str> = line.split('|').map(str::trim).collect();
        if fields.len() != 8 || !fields[6].starts_with("green") {
            continue;
        }
        let line_number = line_index + 1;
        let source = fields[1].trim_matches('`');
        let target = fields[3].trim_matches('`');
        if source != EM_DASH && !source.is_empty() {
            sources
                .entry(source.to_owned())
                .or_default()
                .push(line_number);
        }
        if target != EM_DASH && !target.is_empty() {
            targets
                .entry(target.to_owned())
                .or_default()
                .push(line_number);
            let crate_name = target.split("::").next().unwrap_or(target);
            if !root.join("crates").join(crate_name).is_dir()
                && !root
                    .join("crates")
                    .join(crate_name.replace('-', "_"))
                    .is_dir()
            {
                findings.push(format!(
                    "docs/PORTING.md:{line_number}: green target crate `{crate_name}` is absent"
                ));
            }
        }

        let status = fields[6];
        let evidence = status
            .strip_prefix("green")
            .map(str::trim)
            .unwrap_or_default();
        if evidence.is_empty() {
            findings.push(format!(
                "docs/PORTING.md:{line_number}: green row has no evidence declaration"
            ));
        }

        let tier = fields[5].trim_matches('`');
        if tier == EM_DASH && !has_non_numeric_evidence(evidence) {
            findings.push(format!(
                "docs/PORTING.md:{line_number}: green row has neither a tolerance tier nor an explicit non-numeric evidence type"
            ));
        }
        let unknown_tier = tier != EM_DASH
            && !tier.is_empty()
            && tier.split('+').any(|part| {
                let part = part.trim().trim_matches('`');
                !NUMERIC_TIERS.contains(&part)
            });
        if unknown_tier {
            findings.push(format!(
                "docs/PORTING.md:{line_number}: green row names unknown tolerance tier `{tier}`"
            ));
        }
    }

    duplicate_findings("source identifier", sources, &mut findings);
    duplicate_findings("target identifier", targets, &mut findings);
    findings
}

fn duplicate_findings(
    kind: &str,
    entries: BTreeMap<String, Vec<usize>>,
    findings: &mut Vec<String>,
) {
    for (identifier, lines) in entries {
        if lines.len() > 1 {
            findings.push(format!(
                "docs/PORTING.md: duplicate green {kind} `{identifier}` on lines {}",
                lines
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
}

fn has_non_numeric_evidence(evidence: &str) -> bool {
    let evidence = evidence
        .strip_prefix(EM_DASH)
        .map(str::trim)
        .unwrap_or(evidence)
        .to_ascii_lowercase();
    [
        "behavioral",
        "unit-tested",
        "re-export",
        "parser",
        "catalog",
        "no numeric reference",
    ]
    .iter()
    .any(|marker| evidence.contains(marker))
}

fn ledger_fixture_references(ledger: &str) -> Vec<FixtureReference> {
    let mut references = Vec::new();
    for (line_index, line) in ledger.lines().enumerate() {
        if !line.trim_start().starts_with('|') || !line.contains("golden/") {
            continue;
        }
        let mut rest = line;
        while let Some(start) = rest.find("golden/") {
            rest = &rest[start + "golden/".len()..];
            let token_end = rest
                .find(|character: char| {
                    character.is_whitespace() || matches!(character, '`' | ')' | ';' | ',')
                })
                .unwrap_or(rest.len());
            let token = rest[..token_end].trim_end_matches(['.', ':']);
            if let Some((family, file)) = token.split_once('/') {
                let fixture =
                    (!file.is_empty() && !file.ends_with('/')).then(|| fixture_id(family, file));
                references.push(FixtureReference {
                    family: family.to_owned(),
                    fixture,
                    source_line: line_index + 1,
                });
            }
            rest = &rest[token_end..];
        }
    }
    references.sort();
    references.dedup();
    references
}

fn fixture_id(family: &str, file: &str) -> FixtureId {
    FixtureId {
        family: family.to_owned(),
        file: file.to_owned(),
        manifest_key: file.strip_suffix(".json").unwrap_or(file).to_owned(),
    }
}

fn manifest_paths(golden: &Path) -> Result<Vec<PathBuf>, String> {
    if !golden.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in
        fs::read_dir(golden).map_err(|e| format!("cannot read {}: {e}", golden.display()))?
    {
        let path = entry
            .map_err(|e| format!("cannot read {}: {e}", golden.display()))?
            .path();
        if path.is_dir() {
            let manifest = path.join("manifest.json");
            if manifest.is_file() {
                paths.push(manifest);
            }
        }
    }
    paths.sort();
    Ok(paths)
}

fn audit_manifest(root: &Path, path: &Path) -> Result<Vec<String>, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let display = path.strip_prefix(root).unwrap_or(path).display();
    let mut findings = Vec::new();
    let Some(entries) = manifest_fixture_entries(&text) else {
        findings.push(format!("{display}: manifest has no fixtures object"));
        return Ok(findings);
    };
    if (!text.contains("\"runtime\"") && text.contains("\"alas_commit\""))
        || text.lines().any(|line| {
            line.starts_with("  \"alas_commit\"") || line.starts_with("  \"environment\"")
        })
    {
        findings.push(format!(
            "{display}: legacy family-level provenance is stale; provenance must be per fixture"
        ));
    }
    for (fixture, entry) in entries {
        for key in ["runtime", "alas_commit", "environment"] {
            if !entry.contains(&format!("\"{key}\"")) {
                findings.push(format!(
                    "{display}: fixture `{fixture}` has no per-fixture `{key}` field"
                ));
            }
        }
        if let Some(runtime) = quoted_values_after(entry, "runtime").into_iter().next() {
            if runtime != "alas" && runtime != "suave" {
                findings.push(format!(
                    "{display}: fixture `{fixture}` has unknown runtime `{runtime}`; expected `alas` or `suave`"
                ));
            }
            if runtime == "suave"
                && !entry.contains("\"suave\": {")
                && !entry.contains("\"suave\":{")
            {
                findings.push(format!(
                    "{display}: fixture `{fixture}` has SUAVE runtime but no SUAVE tree/version identity"
                ));
            }
        }
        let Some(generator) = quoted_values_after(entry, "generator").into_iter().next() else {
            findings.push(format!(
                "{display}: fixture `{fixture}` has no generator linkage"
            ));
            continue;
        };
        let generator_name = generator
            .split_whitespace()
            .next()
            .unwrap_or(generator.as_str());
        let generator_path = root.join("golden").join("generators").join(generator_name);
        if !generator_path.is_file() && !workspace_file_named(&root.join("crates"), generator_name)?
        {
            findings.push(format!(
                "{display}: generator `{generator}` named by the manifest is absent"
            ));
            continue;
        }
    }
    Ok(findings)
}

fn manifest_fixture_entries(text: &str) -> Option<Vec<(String, &str)>> {
    let fixtures = json_object_after_key(text, "fixtures")?;
    let bytes = fixtures.as_bytes();
    let mut entries = Vec::new();
    let mut index = 1;
    while index + 1 < bytes.len() {
        while index + 1 < bytes.len()
            && (bytes[index].is_ascii_whitespace() || bytes[index] == b',')
        {
            index += 1;
        }
        if index + 1 >= bytes.len() || bytes[index] == b'}' {
            break;
        }
        let (key, after_key) = json_string_at(fixtures, index)?;
        index = after_key;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if bytes.get(index) != Some(&b':') {
            return None;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if bytes.get(index) != Some(&b'{') {
            return None;
        }
        let close = matching_delimiter(fixtures, index, b'{', b'}')?;
        entries.push((key, &fixtures[index..=close]));
        index = close + 1;
    }
    Some(entries)
}

fn json_object_after_key<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("\"{key}\"");
    let marker_start = text.find(&marker)?;
    let after_marker = marker_start + marker.len();
    let colon = after_marker + text[after_marker..].find(':')?;
    let open = colon + text[colon..].find('{')?;
    let close = matching_delimiter(text, open, b'{', b'}')?;
    Some(&text[open..=close])
}

fn matching_delimiter(text: &str, open: usize, left: u8, right: u8) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0_usize;
    let mut quoted = false;
    let mut escaped = false;
    for (index, byte) in bytes.iter().copied().enumerate().skip(open) {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            continue;
        }
        if byte == b'"' {
            quoted = true;
        } else if byte == left {
            depth += 1;
        } else if byte == right {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn json_string_at(text: &str, open: usize) -> Option<(String, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(open) != Some(&b'"') {
        return None;
    }
    let mut value = String::new();
    let mut index = open + 1;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if escaped {
            value.push(byte as char);
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Some((value, index + 1));
        } else {
            value.push(byte as char);
        }
        index += 1;
    }
    None
}

fn quoted_values_after(text: &str, key: &str) -> Vec<String> {
    let marker = format!("\"{key}\"");
    let mut values = Vec::new();
    let mut rest = text;
    while let Some(position) = rest.find(&marker) {
        rest = &rest[position + marker.len()..];
        let Some(colon) = rest.find(':') else { break };
        rest = &rest[colon + 1..];
        let Some(open) = rest.find('"') else { break };
        rest = &rest[open + 1..];
        let Some(close) = rest.find('"') else { break };
        values.push(rest[..close].to_owned());
        rest = &rest[close + 1..];
    }
    values
}

fn audit_generators(root: &Path) -> Result<Vec<String>, String> {
    let directory = root.join("golden").join("generators");
    let mut findings = Vec::new();
    if !directory.is_dir() {
        return Ok(findings);
    }
    for entry in
        fs::read_dir(&directory).map_err(|e| format!("cannot read {}: {e}", directory.display()))?
    {
        let path = entry
            .map_err(|e| format!("cannot read {}: {e}", directory.display()))?
            .path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("py")
            || path.file_name().and_then(|name| name.to_str()) == Some("_framework.py")
        {
            continue;
        }
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let writes_fixture = text.contains("_framework.write(")
            || text.contains("framework.write(")
            || text.contains(".write_text(")
            || text.contains(".open(\"w\"")
            || text.contains(".open('w'");
        if !writes_fixture {
            let display = path.strip_prefix(root).unwrap_or(&path).display();
            findings.push(format!("{display}: generator has no fixture output"));
        }
    }
    Ok(findings)
}
