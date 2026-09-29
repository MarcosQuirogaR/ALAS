// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Checks that tie one ledger fixture reference to its manifest, generator and consumer.

use super::*;

pub(super) fn audit_fixture_reference(
    root: &Path,
    reference: &FixtureReference,
    consumers: &BTreeSet<FixtureConsumer>,
) -> Result<Vec<String>, String> {
    let mut findings = Vec::new();
    let family_dir = root.join("golden").join(&reference.family);
    let manifest = family_dir.join("manifest.json");
    if !manifest.is_file() {
        findings.push(format!(
            "docs/PORTING.md:{}: declared golden family `{}` has no manifest",
            reference.source_line, reference.family
        ));
        return Ok(findings);
    }
    let manifest_text = fs::read_to_string(&manifest)
        .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?;
    if let Some(fixture) = &reference.fixture {
        let artifact = family_dir.join(&fixture.file);
        if !artifact.is_file() {
            findings.push(format!(
                "docs/PORTING.md:{}: declared fixture `{}/{}` is absent",
                reference.source_line, reference.family, fixture.file
            ));
        }
        if manifest_entry(&manifest_text, &fixture.manifest_key).is_none() {
            findings.push(format!(
                "golden/{}/manifest.json: fixture `{}` is not registered",
                reference.family, fixture.manifest_key
            ));
        }
        if !consumers
            .iter()
            .any(|consumer| consumer.fixture == *fixture)
        {
            findings.push(format!(
                "docs/PORTING.md:{}: no literal fixture consumer loads `{}/{}`",
                reference.source_line, reference.family, fixture.file
            ));
        }
    }
    Ok(findings)
}

pub(super) fn audit_fixture_consumer(
    root: &Path,
    consumer: &FixtureConsumer,
    linked: &BTreeSet<FixtureId>,
) -> Result<Vec<String>, String> {
    let mut findings = Vec::new();
    let fixture = &consumer.fixture;
    let artifact = root
        .join("golden")
        .join(&fixture.family)
        .join(&fixture.file);
    if !artifact.is_file() {
        findings.push(format!(
            "{}:{}: consumed fixture `golden/{}/{}` is absent",
            consumer.source_path, consumer.source_line, fixture.family, fixture.file
        ));
    }
    let manifest = root
        .join("golden")
        .join(&fixture.family)
        .join("manifest.json");
    if !manifest.is_file() {
        findings.push(format!(
            "{}:{}: consumed fixture `golden/{}/{}` has no family manifest",
            consumer.source_path, consumer.source_line, fixture.family, fixture.file
        ));
    } else {
        let text = fs::read_to_string(&manifest)
            .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?;
        if let Some(entry) = manifest_entry(&text, &fixture.manifest_key) {
            if quoted_values_after(entry, "generator").is_empty() {
                findings.push(format!(
                    "{}:{}: consumed fixture `golden/{}/{}` has no generator linkage",
                    consumer.source_path, consumer.source_line, fixture.family, fixture.file
                ));
            }
        } else {
            findings.push(format!(
                "{}:{}: consumed fixture `golden/{}/{}` is not registered in its manifest",
                consumer.source_path, consumer.source_line, fixture.family, fixture.file
            ));
        }
    }
    if !linked.contains(fixture) {
        findings.push(format!(
            "{}:{}: consumed fixture `golden/{}/{}` has no docs/PORTING.md linkage",
            consumer.source_path, consumer.source_line, fixture.family, fixture.file
        ));
    }
    Ok(findings)
}

fn manifest_entry<'a>(text: &'a str, fixture: &str) -> Option<&'a str> {
    manifest_fixture_entries(text)?
        .into_iter()
        .find_map(|(name, entry)| (name == fixture).then_some(entry))
}

pub(super) fn workspace_file_named(root: &Path, name: &str) -> Result<bool, String> {
    if !root.is_dir() {
        return Ok(false);
    }
    let mut directories = vec![root.to_owned()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|e| format!("cannot read {}: {e}", directory.display()))?
        {
            let path = entry
                .map_err(|e| format!("cannot read {}: {e}", directory.display()))?
                .path();
            if path.is_dir() {
                directories.push(path);
            } else if path.file_name().and_then(|file| file.to_str()) == Some(name) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
