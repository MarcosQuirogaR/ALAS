// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The `addon.toml` manifest: what an add-on is, what it reads and what it
//! produces.

use std::collections::BTreeSet;

use serde::Deserialize;

use crate::apply::Target;
use crate::error::AddonError;

/// How an add-on runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AddonKind {
    /// A Rhai script evaluated inside the application's sandbox.
    Script,
    /// An external executable or Python script run as a child process.
    Process,
}

impl AddonKind {
    /// Lower-case name as written in the manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Script => "script",
            Self::Process => "process",
        }
    }
}

/// One scalar input of an add-on.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputSpec {
    /// Identifier used as the key in the `inputs` map.
    pub name: String,
    /// Unit label shown beside the field. The value is passed unchanged.
    #[serde(default)]
    pub unit: String,
    /// Initial value of the input form.
    #[serde(default)]
    pub default: f64,
    /// One-line explanation shown beside the field.
    #[serde(default)]
    pub description: String,
}

/// One scalar output of an add-on.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputSpec {
    /// Identifier used as the key in the returned map.
    pub name: String,
    /// Unit label. A targeted output is written unchanged, so it must already
    /// be in the unit (SI) of the field it targets.
    #[serde(default)]
    pub unit: String,
    /// One-line explanation shown beside the value.
    #[serde(default)]
    pub description: String,
    /// Optional design or configuration field this output may be written to:
    /// a dotted configuration path such as `mass.some_field`, or
    /// `design_vector.<name>` for a design variable. Never applied without the
    /// user's confirmation.
    #[serde(default)]
    pub target: Option<String>,
}

/// A parsed and validated manifest.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonManifest {
    /// Display name.
    pub name: String,
    /// Version text, recorded in the provenance of every applied output.
    pub version: String,
    /// Author or maintainer.
    #[serde(default)]
    pub author: String,
    /// What the add-on does and which source its method comes from.
    #[serde(default)]
    pub description: String,
    /// `script` or `process`.
    pub kind: AddonKind,
    /// Entry file, relative to the add-on folder.
    pub entry: String,
    /// Optional wall-clock limit in seconds (default 10, at most 120).
    #[serde(default)]
    pub timeout_s: Option<f64>,
    /// Scalar inputs.
    #[serde(default)]
    pub inputs: Vec<InputSpec>,
    /// Scalar outputs.
    #[serde(default)]
    pub outputs: Vec<OutputSpec>,
}

/// Default wall-clock limit of one run.
pub const DEFAULT_TIMEOUT_S: f64 = 10.0;
/// Largest wall-clock limit a manifest may request.
pub const MAX_TIMEOUT_S: f64 = 120.0;

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl AddonManifest {
    /// Parse and validate manifest text.
    pub fn parse(text: &str) -> Result<Self, AddonError> {
        let manifest: Self =
            toml::from_str(text).map_err(|error| AddonError::Manifest(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Wall-clock limit of one run, seconds.
    pub fn timeout_seconds(&self) -> f64 {
        self.timeout_s.unwrap_or(DEFAULT_TIMEOUT_S)
    }

    fn validate(&self) -> Result<(), AddonError> {
        let bad = |message: String| Err(AddonError::Manifest(message));
        if self.name.trim().is_empty() {
            return bad("`name` must not be empty".into());
        }
        if self.version.trim().is_empty() {
            return bad("`version` must not be empty".into());
        }
        let entry = std::path::Path::new(&self.entry);
        let escapes = entry.is_absolute()
            || self.entry.is_empty()
            || self.entry.starts_with(['/', '\\'])
            || entry
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir));
        if escapes || self.entry.contains(':') {
            return bad(format!(
                "`entry` must be a relative path inside the add-on folder, got `{}`",
                self.entry
            ));
        }
        if let Some(timeout) = self.timeout_s {
            if !(timeout.is_finite() && timeout > 0.0 && timeout <= MAX_TIMEOUT_S) {
                return bad(format!(
                    "`timeout_s` must be in (0, {MAX_TIMEOUT_S}] seconds, got {timeout}"
                ));
            }
        }
        let mut seen = BTreeSet::new();
        for input in &self.inputs {
            if !is_identifier(&input.name) {
                return bad(format!("input name `{}` is not an identifier", input.name));
            }
            if !input.default.is_finite() {
                return bad(format!("input `{}` has a non-finite default", input.name));
            }
            if !seen.insert(input.name.as_str()) {
                return bad(format!("duplicate input `{}`", input.name));
            }
        }
        let mut seen = BTreeSet::new();
        for output in &self.outputs {
            if !is_identifier(&output.name) {
                return bad(format!(
                    "output name `{}` is not an identifier",
                    output.name
                ));
            }
            if !seen.insert(output.name.as_str()) {
                return bad(format!("duplicate output `{}`", output.name));
            }
            if let Some(target) = &output.target {
                Target::parse(target).map_err(|reason| {
                    AddonError::Manifest(format!("output `{}`: {reason}", output.name))
                })?;
            }
        }
        if self.outputs.is_empty() {
            return bad("an add-on must declare at least one output".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
name = "Demo"
version = "1.2.0"
author = "A"
description = "d"
kind = "script"
entry = "main.rhai"
[[inputs]]
name = "x"
unit = "m"
default = 2.5
[[outputs]]
name = "y"
unit = "kg"
target = "mass.some_field"
"#;

    #[test]
    fn a_complete_manifest_parses() {
        let manifest = AddonManifest::parse(GOOD).unwrap();
        assert_eq!(manifest.kind, AddonKind::Script);
        assert_eq!(manifest.inputs[0].default, 2.5);
        assert_eq!(
            manifest.outputs[0].target.as_deref(),
            Some("mass.some_field")
        );
        assert_eq!(manifest.timeout_seconds(), DEFAULT_TIMEOUT_S);
    }

    #[test]
    fn unknown_keys_bad_kinds_and_escaping_entries_are_rejected() {
        let cases = [
            GOOD.replace("kind = \"script\"", "kind = \"dll\""),
            GOOD.replace("entry = \"main.rhai\"", "entry = \"../evil.rhai\""),
            GOOD.replace("entry = \"main.rhai\"", "entry = \"C:/evil.rhai\""),
            GOOD.replace("entry = \"main.rhai\"", "entry = \"/etc/passwd\""),
            GOOD.replace("name = \"x\"", "name = \"x y\""),
            GOOD.replace("version = \"1.2.0\"", "version = \"1.2.0\"\nsurprise = 1"),
            GOOD.replace("default = 2.5", "default = inf"),
            GOOD.replace("target = \"mass.some_field\"", "target = \"mass..x\""),
            GOOD.replace(
                "entry = \"main.rhai\"",
                "entry = \"main.rhai\"\ntimeout_s = 0.0",
            ),
            GOOD.replace(
                "entry = \"main.rhai\"",
                "entry = \"main.rhai\"\ntimeout_s = 9999.0",
            ),
        ];
        for text in cases {
            assert!(AddonManifest::parse(&text).is_err(), "accepted:\n{text}");
        }
    }

    #[test]
    fn duplicate_names_and_missing_outputs_are_rejected() {
        let dup = format!("{GOOD}[[inputs]]\nname = \"x\"\n");
        assert!(AddonManifest::parse(&dup).is_err());
        let none = GOOD.split("[[outputs]]").next().unwrap_or_default();
        assert!(AddonManifest::parse(none).is_err());
    }
}
