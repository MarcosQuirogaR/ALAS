// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{generate, read_exemptions};
use std::path::{Path, PathBuf};

const CONFIG: &str = r#"
#[derive(ConfigNode)]
pub struct AlasConfig {
    #[config(help = "read by the model", unit = "m")]
    pub used: f64,
    #[config(help = "read by nothing")]
    pub dead: f64,
    #[config(help = "read only by the editor")]
    pub editor_only: f64,
    #[config(nested, help = "group")]
    pub group: Inner,
}

#[derive(ConfigNode)]
pub struct Inner {
    #[config(help = "read by a validation function only")]
    pub checked: f64,
    #[config(help = "read through a config method the model calls")]
    pub via_method: f64,
}

impl Inner {
    pub fn validate(&self) -> bool {
        self.checked > 0.0
    }
    pub fn resolved(&self) -> f64 {
        self.via_method * 2.0
    }
}
"#;

const MODEL: &str = r#"
pub fn model(c: &AlasConfig) -> f64 {
    c.used + c.group.resolved()
}

#[cfg(test)]
mod tests {
    fn only_in_tests(c: &AlasConfig) -> f64 {
        c.dead
    }
}
"#;

const GUI: &str = "pub fn show(c: &AlasConfig) -> f64 { c.editor_only }";

fn fixture(name: &str, exemptions: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("alas-capmap-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in [
        ("crates/alas-config/src/lib.rs", CONFIG),
        ("crates/alas-model/src/lib.rs", MODEL),
        ("crates/alas-gui/src/lib.rs", GUI),
        ("docs/architecture/field-exemptions.txt", exemptions),
    ] {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().expect("parent")).expect("mkdir");
        std::fs::write(full, text).expect("write");
    }
    root
}

fn field_findings(root: &Path) -> Vec<String> {
    generate(root)
        .expect("generate")
        .findings
        .into_iter()
        .filter(|f| !f.starts_with("capability family"))
        .collect()
}

#[test]
fn an_unread_field_fails_and_read_fields_pass() {
    let root = fixture("unread", "");
    let findings = field_findings(&root);
    let text = findings.join("\n");
    assert!(text.contains("`dead` is unread"), "{text}");
    assert!(text.contains("`editor_only` is gui_only"), "{text}");
    assert!(text.contains("`group.checked` is config_only"), "{text}");
    assert!(!text.contains("`used`"), "{text}");
    assert!(!text.contains("`group.via_method`"), "{text}");
    assert_eq!(findings.len(), 3, "{text}");
}

#[test]
fn an_exemption_silences_a_field_and_a_stale_one_fails() {
    let exemptions = "dead | deprecated | retired\n\
                      editor_only | display_only | shown only\n\
                      group.checked | display_only | validated, unused\n\
                      used | display_only | wrongly exempted\n\
                      gone | deprecated | not a field\n";
    let root = fixture("exempt", exemptions);
    let text = field_findings(&root).join("\n");
    assert!(!text.contains("`dead`"), "{text}");
    assert!(text.contains("`used` is read by model code now"), "{text}");
    assert!(
        text.contains("`gone` is not a configuration field"),
        "{text}"
    );
}

#[test]
fn the_generated_map_is_deterministic() {
    let root = fixture("determinism", "");
    let first = generate(&root).expect("first");
    let second = generate(&root).expect("second");
    assert_eq!(first.json, second.json);
    assert_eq!(first.markdown, second.markdown);
}

#[test]
fn a_known_gap_exemption_is_rejected() {
    let root = fixture(
        "no-known-gap",
        "group.checked | known_gap | validated, unused\n",
    );
    let error = read_exemptions(&root)
        .err()
        .expect("known_gap must not parse");
    assert!(
        error.contains("expected `path | display_only|deprecated | reason`"),
        "{error}"
    );
}
