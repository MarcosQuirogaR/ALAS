// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Turning add-on outputs into design changes, and recording where they came
//! from.
//!
//! The flow is plan, preview, apply. [`plan`] reads the outputs and the
//! current design and returns before/after rows without changing anything.
//! [`apply`] writes them to a copy, asks the caller's validator to accept that
//! copy, and only then commits it, so a refused change leaves the design
//! exactly as it was. Each committed row becomes an [`AppliedRecord`] that the
//! run log and the run output folder carry as "set by add-on <name> <version>".

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::error::AddonError;
use crate::manifest::AddonManifest;

/// Prefix of a target that names a design variable instead of a config path.
pub const DESIGN_PREFIX: &str = "design_vector.";

/// Where an output may be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A numeric configuration field, as a JSON pointer (`/mass/some_field`).
    Config(String),
    /// A named design variable.
    Design(String),
}

fn segment_ok(segment: &str) -> bool {
    let mut chars = segment.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl Target {
    /// Parse the manifest's dotted target text.
    pub fn parse(text: &str) -> Result<Self, String> {
        if let Some(name) = text.strip_prefix(DESIGN_PREFIX) {
            return if segment_ok(name) {
                Ok(Self::Design(name.to_owned()))
            } else {
                Err(format!("`{text}` is not a valid design-variable target"))
            };
        }
        if text.split('.').all(segment_ok) {
            let pointer: String = text.split('.').map(|s| format!("/{s}")).collect();
            Ok(Self::Config(pointer))
        } else {
            Err(format!("`{text}` is not a valid dotted field path"))
        }
    }
}

/// One proposed change, shown to the user before anything is written.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlannedChange {
    /// Add-on display name.
    pub addon: String,
    /// Add-on version.
    pub version: String,
    /// Output the value came from.
    pub output: String,
    /// Output unit label.
    pub unit: String,
    /// Target text as written in the manifest.
    pub target: String,
    /// Current value; `None` when the field is unset (`null` or absent).
    pub before: Option<f64>,
    /// Proposed value.
    pub after: f64,
}

/// A committed change, in the shape the provenance file records.
pub type AppliedRecord = PlannedChange;

impl PlannedChange {
    /// The sentence recorded in the run log and the report.
    pub fn provenance_line(&self) -> String {
        let before = self
            .before
            .map_or_else(|| "unset".to_owned(), |value| format!("{value}"));
        format!(
            "{} = {} {} (was {before}), set by add-on {} {}",
            self.target, self.after, self.unit, self.addon, self.version
        )
    }
}

/// Build the before/after rows for every output that declares a target.
///
/// Outputs without a target are display-only and produce no row. Nothing is
/// modified. A target that cannot be written (missing group, non-numeric
/// field, unknown design variable) is reported, not skipped.
pub fn plan(
    manifest: &AddonManifest,
    outputs: &BTreeMap<String, f64>,
    config: &Value,
    design: &BTreeMap<String, f64>,
) -> Result<Vec<PlannedChange>, AddonError> {
    let mut rows = Vec::new();
    for spec in &manifest.outputs {
        let Some(target_text) = &spec.target else {
            continue;
        };
        let value = *outputs
            .get(&spec.name)
            .ok_or_else(|| AddonError::Apply(format!("output `{}` was not produced", spec.name)))?;
        let target = Target::parse(target_text).map_err(AddonError::Apply)?;
        let before =
            match &target {
                Target::Config(pointer) => {
                    let parent = pointer.rsplit_once('/').map_or("", |(parent, _)| parent);
                    if !config.pointer(parent).is_some_and(Value::is_object) {
                        return Err(AddonError::Apply(format!(
                            "target `{target_text}` does not exist in this configuration"
                        )));
                    }
                    match config.pointer(pointer) {
                        None | Some(Value::Null) => None,
                        Some(Value::Number(number)) => number.as_f64(),
                        Some(_) => {
                            return Err(AddonError::Apply(format!(
                                "target `{target_text}` is not a number"
                            )))
                        }
                    }
                }
                Target::Design(name) => Some(*design.get(name).ok_or_else(|| {
                    AddonError::Apply(format!("`{name}` is not a design variable"))
                })?),
            };
        rows.push(PlannedChange {
            addon: manifest.name.clone(),
            version: manifest.version.clone(),
            output: spec.name.clone(),
            unit: spec.unit.clone(),
            target: target_text.clone(),
            before,
            after: value,
        });
    }
    Ok(rows)
}

/// Accepts or refuses a candidate design before it is committed. Receives the
/// edited configuration and design variables; returns the reason on refusal.
pub type Validator<'a> = dyn Fn(&Value, &BTreeMap<String, f64>) -> Result<(), String> + 'a;

/// Write `changes` into `config` and `design` if the validator accepts the
/// result. On any refusal neither is modified.
pub fn apply(
    changes: &[PlannedChange],
    config: &mut Value,
    design: &mut BTreeMap<String, f64>,
    validate: &Validator<'_>,
) -> Result<Vec<AppliedRecord>, AddonError> {
    let mut next_config = config.clone();
    let mut next_design = design.clone();
    for change in changes {
        if !change.after.is_finite() {
            return Err(AddonError::Apply(format!(
                "`{}` is not finite",
                change.output
            )));
        }
        match Target::parse(&change.target).map_err(AddonError::Apply)? {
            Target::Config(pointer) => {
                let (parent, key) = pointer.rsplit_once('/').unwrap_or(("", ""));
                let slot = next_config
                    .pointer_mut(parent)
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| {
                        AddonError::Apply(format!("target `{}` does not exist", change.target))
                    })?;
                let number = serde_json::Number::from_f64(change.after).ok_or_else(|| {
                    AddonError::Apply(format!("`{}` is not finite", change.output))
                })?;
                slot.insert(key.to_owned(), Value::Number(number));
            }
            Target::Design(name) => {
                if !next_design.contains_key(&name) {
                    return Err(AddonError::Apply(format!(
                        "`{name}` is not a design variable"
                    )));
                }
                next_design.insert(name, change.after);
            }
        }
    }
    validate(&next_config, &next_design)
        .map_err(|reason| AddonError::Apply(format!("the design was left unchanged: {reason}")))?;
    *config = next_config;
    *design = next_design;
    Ok(changes.to_vec())
}

/// Render the provenance of every applied change as the JSON file kept beside
/// a run's outputs.
pub fn provenance_json(records: &[AppliedRecord]) -> String {
    let rows: Vec<Value> = records
        .iter()
        .map(|record| {
            serde_json::json!({
                "statement": record.provenance_line(),
                "addon": record.addon,
                "addon_version": record.version,
                "output": record.output,
                "target": record.target,
                "unit": record.unit,
                "before": record.before,
                "after": record.after,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({ "set_by_addon": rows }))
        .unwrap_or_else(|_| "{}".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest() -> AddonManifest {
        AddonManifest::parse(
            r#"
name = "Demo"
version = "2.0"
kind = "script"
entry = "m.rhai"
[[outputs]]
name = "mass"
unit = "kg"
target = "mass.override_kg"
[[outputs]]
name = "span"
unit = "m"
target = "design_vector.span_m"
[[outputs]]
name = "shown"
"#,
        )
        .unwrap()
    }

    fn outputs() -> BTreeMap<String, f64> {
        [("mass", 1234.5), ("span", 60.0), ("shown", 7.0)]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect()
    }

    fn design() -> BTreeMap<String, f64> {
        BTreeMap::from([("span_m".to_owned(), 50.0)])
    }

    #[test]
    fn targets_parse_to_pointers_and_design_names() {
        assert_eq!(Target::parse("a.b").unwrap(), Target::Config("/a/b".into()));
        assert_eq!(
            Target::parse("design_vector.span_m").unwrap(),
            Target::Design("span_m".into())
        );
        assert!(Target::parse("a/b").is_err());
        assert!(Target::parse("").is_err());
    }

    #[test]
    fn the_plan_previews_before_and_after_and_skips_display_only_outputs() {
        let config = json!({"mass": {"override_kg": null}});
        let rows = plan(&manifest(), &outputs(), &config, &design()).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].before, None);
        assert_eq!(rows[0].after, 1234.5);
        assert_eq!(rows[1].before, Some(50.0));
        assert_eq!(config, json!({"mass": {"override_kg": null}}));
    }

    #[test]
    fn plans_fail_on_missing_groups_non_numeric_fields_and_unknown_variables() {
        let missing = json!({"other": {}});
        assert!(plan(&manifest(), &outputs(), &missing, &design()).is_err());
        let text = json!({"mass": {"override_kg": "x"}});
        assert!(plan(&manifest(), &outputs(), &text, &design()).is_err());
        let config = json!({"mass": {"override_kg": 1.0}});
        assert!(plan(&manifest(), &outputs(), &config, &BTreeMap::new()).is_err());
    }

    #[test]
    fn apply_commits_only_when_the_validator_accepts() {
        let mut config = json!({"mass": {"override_kg": 1.0}});
        let mut vars = design();
        let rows = plan(&manifest(), &outputs(), &config, &vars).unwrap();

        let refuse = |_: &Value, _: &BTreeMap<String, f64>| Err("no".to_owned());
        assert!(apply(&rows, &mut config, &mut vars, &refuse).is_err());
        assert_eq!(config["mass"]["override_kg"], json!(1.0));
        assert_eq!(vars["span_m"], 50.0);

        let accept = |c: &Value, d: &BTreeMap<String, f64>| {
            assert_eq!(c["mass"]["override_kg"], json!(1234.5));
            assert_eq!(d["span_m"], 60.0);
            Ok(())
        };
        let records = apply(&rows, &mut config, &mut vars, &accept).unwrap();
        assert_eq!(config["mass"]["override_kg"], json!(1234.5));
        assert_eq!(vars["span_m"], 60.0);
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn provenance_names_the_addon_and_its_version() {
        let config = json!({"mass": {"override_kg": 1.0}});
        let rows = plan(&manifest(), &outputs(), &config, &design()).unwrap();
        let line = rows[0].provenance_line();
        assert!(line.contains("set by add-on Demo 2.0"), "{line}");
        assert!(
            line.contains("mass.override_kg = 1234.5 kg (was 1)"),
            "{line}"
        );
        let json = provenance_json(&rows);
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["set_by_addon"][0]["addon_version"], json!("2.0"));
    }

    #[test]
    fn a_non_finite_value_is_refused_before_any_write() {
        let mut config = json!({"mass": {"override_kg": 1.0}});
        let mut vars = design();
        let mut rows = plan(&manifest(), &outputs(), &config, &vars).unwrap();
        rows[0].after = f64::NAN;
        let accept = |_: &Value, _: &BTreeMap<String, f64>| Ok(());
        assert!(apply(&rows, &mut config, &mut vars, &accept).is_err());
        assert_eq!(config["mass"]["override_kg"], json!(1.0));
    }
}
