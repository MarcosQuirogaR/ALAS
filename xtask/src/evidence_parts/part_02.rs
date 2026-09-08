// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn audit_fixture_reference(
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

fn audit_fixture_consumer(
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

fn workspace_file_named(root: &Path, name: &str) -> Result<bool, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("alas-evidence-{suffix}"));
        fs::create_dir_all(path.join("docs")).expect("temporary root should be writable");
        path
    }

    #[test]
    fn green_rows_need_a_declared_evidence_kind() {
        let root = temp_root();
        let findings = audit_ledger(
            &root,
            "| source.py | 1 | `alas-types` | \u{2014} | \u{2014} | green |\n",
        );
        assert!(findings
            .iter()
            .any(|finding| finding.contains("evidence declaration")));
    }

    #[test]
    fn duplicate_green_identifiers_are_reported() {
        let root = temp_root();
        let row = "| `source.py` | 1 | `alas-types` | `alas-types` | \u{2014} | green \u{2014} unit-tested |\n";
        let findings = audit_ledger(&root, &format!("{row}{row}"));
        assert!(findings
            .iter()
            .any(|finding| finding.contains("duplicate green source")));
        assert!(findings
            .iter()
            .any(|finding| finding.contains("duplicate green target")));
    }

    #[test]
    fn missing_fixture_and_manifest_registration_are_reported() {
        let root = temp_root();
        fs::create_dir_all(root.join("golden").join("demo")).expect("fixture directory");
        fs::write(
            root.join("golden/demo/manifest.json"),
            r#"{"fixtures":{},"alas_commit":"abc","environment":{"python":"x","platform":"y"}}"#,
        )
        .expect("manifest");
        let reference = FixtureReference {
            family: "demo".to_owned(),
            fixture: Some(fixture_id("demo", "missing.json")),
            source_line: 3,
        };
        let findings =
            audit_fixture_reference(&root, &reference, &BTreeSet::new()).expect("audit should run");
        assert!(findings.iter().any(|finding| finding.contains("is absent")));
        assert!(findings
            .iter()
            .any(|finding| finding.contains("not registered")));
    }

    #[test]
    fn manifest_requires_provenance_and_generator_output() {
        let root = temp_root();
        fs::create_dir_all(root.join("golden/demo")).expect("fixture directory");
        fs::create_dir_all(root.join("golden/generators")).expect("generator directory");
        fs::write(
            root.join("golden/demo/manifest.json"),
            r#"{"fixtures":{"demo":{"generator":"gen_demo.py"}}}"#,
        )
        .expect("manifest");
        fs::write(
            root.join("golden/generators/gen_demo.py"),
            "print('nothing')",
        )
        .expect("generator");
        let manifest_findings = audit_manifest(&root, &root.join("golden/demo/manifest.json"))
            .expect("manifest audit should run");
        assert!(manifest_findings
            .iter()
            .any(|finding| finding.contains("per-fixture")));
        let generator_findings = audit_generators(&root).expect("generator audit should run");
        assert!(generator_findings
            .iter()
            .any(|finding| finding.contains("no fixture output")));
    }

    #[test]
    fn legacy_family_metadata_is_rejected_for_mixed_runtimes() {
        let root = temp_root();
        fs::create_dir_all(root.join("golden/demo")).expect("fixture directory");
        fs::create_dir_all(root.join("golden/generators")).expect("generator directory");
        fs::write(
            root.join("golden/demo/manifest.json"),
            r#"{"fixtures":{"a":{"generator":"gen_a.py"},"b":{"generator":"gen_b.py"}},"alas_commit":"abc","environment":{"python":"x","platform":"y"}}"#,
        )
        .expect("manifest");
        fs::write(
            root.join("golden/generators/gen_a.py"),
            "_framework.add_alas_to_path()\n_framework.write()",
        )
        .expect("ALAS generator");
        fs::write(
            root.join("golden/generators/gen_b.py"),
            "_framework.add_suave_to_path()\n_framework.write()",
        )
        .expect("SUAVE generator");
        let findings = audit_manifest(&root, &root.join("golden/demo/manifest.json"))
            .expect("manifest audit should run");
        assert!(findings
            .iter()
            .any(|finding| finding.contains("legacy family-level provenance")));
        assert!(findings
            .iter()
            .any(|finding| finding.contains("per-fixture `runtime`")));
    }

    #[test]
    fn suave_fixture_requires_tree_and_version_identity() {
        let root = temp_root();
        fs::create_dir_all(root.join("golden/demo")).expect("fixture directory");
        fs::create_dir_all(root.join("golden/generators")).expect("generator directory");
        fs::write(
            root.join("golden/demo/manifest.json"),
            r#"{"fixtures":{"demo":{"generator":"gen_demo.py","runtime":"suave","alas_commit":"abc","environment":{"python":"x"}}}}"#,
        )
        .expect("manifest");
        fs::write(
            root.join("golden/generators/gen_demo.py"),
            "_framework.write()",
        )
        .expect("generator");
        let findings = audit_manifest(&root, &root.join("golden/demo/manifest.json"))
            .expect("manifest audit should run");
        assert!(findings
            .iter()
            .any(|finding| finding.contains("SUAVE tree/version identity")));
    }

    #[test]
    fn unmanifested_helper_load_is_rejected_for_a_wip_row() {
        let root = temp_root();
        fs::create_dir_all(root.join("crates/demo/tests/support")).expect("consumer directory");
        fs::create_dir_all(root.join("golden/demo")).expect("fixture directory");
        fs::write(root.join("golden/demo/case.json"), "{}\n").expect("fixture");
        fs::write(
            root.join("crates/demo/tests/support/mod.rs"),
            "fn fixture() { let _ = alas_testkit::load_json(\"demo\", \"case\"); }\n",
        )
        .expect("consumer");
        fs::write(
            root.join("docs/PORTING.md"),
            "| `source.py` | 1 | `demo` | - | `exact` | wip - `golden/demo/case.json` |\n",
        )
        .expect("ledger");

        let findings = check(&root).expect("audit should run");
        assert!(findings.iter().any(|finding| {
            finding.contains("crates/demo/tests/support/mod.rs")
                && finding.contains("has no family manifest")
        }));
    }

    #[test]
    fn unmanifested_direct_includes_are_rejected() {
        let root = temp_root();
        fs::create_dir_all(root.join("crates/demo/tests")).expect("consumer directory");
        fs::create_dir_all(root.join("golden/demo")).expect("fixture directory");
        fs::write(root.join("golden/demo/text.json"), "{}\n").expect("text fixture");
        fs::write(root.join("golden/demo/blob.bin"), [1_u8, 2, 3]).expect("binary fixture");
        fs::write(root.join("golden/demo/manifest.json"), r#"{"fixtures":{}}"#).expect("manifest");
        fs::write(
            root.join("crates/demo/tests/direct.rs"),
            "const TEXT: &str = include_str!(\"../../../golden/demo/text.json\");\n\
             const BLOB: &[u8] = include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../../golden/demo/blob.bin\"));\n",
        )
        .expect("consumer");
        fs::write(
            root.join("docs/PORTING.md"),
            "| `source.py` | 1 | `demo` | - | `exact` | wip - `golden/demo/text.json`, `golden/demo/blob.bin` |\n",
        )
        .expect("ledger");

        let findings = check(&root).expect("audit should run");
        for file in ["text.json", "blob.bin"] {
            assert!(findings.iter().any(|finding| {
                finding.contains(file) && finding.contains("is not registered in its manifest")
            }));
        }
    }

    #[test]
    fn unmanifested_central_golden_directory_is_rejected() {
        let root = temp_root();
        fs::create_dir_all(root.join("crates/demo/tests/support")).expect("consumer directory");
        fs::create_dir_all(root.join("golden/demo/inputs")).expect("fixture directory");
        fs::write(root.join("golden/demo/inputs/case.dat"), "fixture\n").expect("fixture");
        fs::write(root.join("golden/demo/manifest.json"), r#"{"fixtures":{}}"#).expect("manifest");
        fs::write(
            root.join("crates/demo/tests/support/mod.rs"),
            r#"fn inputs_dir() -> PathBuf {
    alas_testkit::golden_dir().join("demo").join("inputs")
}
fn read() { let _ = std::fs::read_to_string(inputs_dir().join("case.dat")); }
"#,
        )
        .expect("consumer");
        fs::write(
            root.join("docs/PORTING.md"),
            "| `source.py` | 1 | `demo` | - | `exact` | wip - `golden/demo/inputs/case.dat` |\n",
        )
        .expect("ledger");

        let findings = check(&root).expect("audit should run");
        assert!(findings.iter().any(|finding| {
            finding.contains("inputs/case.dat")
                && finding.contains("is not registered in its manifest")
        }));
    }

    #[test]
    fn consumed_fixture_without_ledger_linkage_is_rejected() {
        let root = temp_root();
        fs::create_dir_all(root.join("crates/demo/tests")).expect("consumer directory");
        fs::create_dir_all(root.join("golden/demo")).expect("fixture directory");
        fs::create_dir_all(root.join("golden/generators")).expect("generator directory");
        fs::write(root.join("golden/demo/case.json"), "{}\n").expect("fixture");
        fs::write(
            root.join("golden/demo/manifest.json"),
            r#"{"fixtures":{"case":{"generator":"gen_demo.py","runtime":"alas","alas_commit":"abc","environment":{"python":"x"}}}}"#,
        )
        .expect("manifest");
        fs::write(
            root.join("golden/generators/gen_demo.py"),
            "_framework.write(\"demo\", \"case\", {})\n",
        )
        .expect("generator");
        fs::write(
            root.join("crates/demo/tests/direct.rs"),
            "fn fixture() { let _ = alas_testkit::load_json(\"demo\", \"case\"); }\n",
        )
        .expect("consumer");
        fs::write(root.join("docs/PORTING.md"), "# no fixture link\n").expect("ledger");

        let findings = check(&root).expect("audit should run");
        assert!(findings
            .iter()
            .any(|finding| finding.contains("has no docs/PORTING.md linkage")));
    }
}

