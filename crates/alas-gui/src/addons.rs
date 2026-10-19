// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Add-ons page: state, the background runner, and the glue that applies
//! add-on outputs to the design with validation and provenance.
//!
//! The GUI-free core is `alas-addons`. Drawing lives in [`view`].

mod view;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use alas_addons::apply::{self, AppliedRecord, PlannedChange, Target};
use alas_addons::{AddonError, AddonSettings, Discovery, RunOutcome};
use alas_config::{validate, AlasConfig, Severity, DESIGN_VARIABLE_SPECS};
use serde_json::Value;

use crate::state::{AppState, LogKind};

pub(crate) use view::show_addons_view;

/// Folder name inside the per-user data folder that holds add-ons.
const ADDONS_FOLDER: &str = "addons";
/// File written beside a run's outputs when add-on values are in effect.
pub const PROVENANCE_FILE: &str = "addon-provenance.json";

/// The finished (or failed) most recent run.
pub(crate) struct LastRun {
    pub(crate) id: String,
    pub(crate) result: Result<RunOutcome, String>,
}

/// State of the Add-ons page.
#[derive(Default)]
pub struct AddonsPanel {
    loaded: bool,
    pub(crate) dir: PathBuf,
    pub(crate) discovery: Discovery,
    pub(crate) settings: AddonSettings,
    pub(crate) selected: Option<String>,
    /// Input text per `<add-on id>/<input name>`.
    pub(crate) input_text: BTreeMap<String, String>,
    pub(crate) python_text: String,
    pub(crate) running: Option<(String, Receiver<Result<RunOutcome, AddonError>>)>,
    pub(crate) last: Option<LastRun>,
    pub(crate) preview: Option<Vec<PlannedChange>>,
    /// Changes applied in this session and still in effect.
    pub(crate) applied: Vec<AppliedRecord>,
    pub(crate) status: Option<String>,
}

impl AddonsPanel {
    /// Whether an add-on is executing right now.
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Changes applied by add-ons that are still in effect.
    pub fn applied(&self) -> &[AppliedRecord] {
        &self.applied
    }
}

/// `<user data root>/addons`, the same per-user folder the File menu shows.
fn addons_dir(state: &AppState) -> PathBuf {
    state.tool_locator.user_data_root().join(ADDONS_FOLDER)
}

fn decode_config(value: &Value) -> Result<AlasConfig, String> {
    let decoded = if value.pointer("/optimizer/objective/mtow_sizing").is_none() {
        AlasConfig::from_value(value).map_err(|error| error.to_string())
    } else {
        serde_json::from_value(value.clone()).map_err(|error| error.to_string())
    };
    decoded
}

fn error_messages(config: &AlasConfig) -> Vec<String> {
    validate(config)
        .into_iter()
        .filter(|issue| issue.severity == Severity::Error)
        .map(|issue| format!("{}: {}", issue.field_path, issue.message))
        .collect()
}

/// Accept a candidate only if it decodes, adds no validation error the current
/// design does not already have, and keeps design variables inside the
/// cross-preset guardrails.
fn candidate_validator(
    current: &Value,
) -> impl Fn(&Value, &BTreeMap<String, f64>) -> Result<(), String> {
    let existing: Vec<String> = decode_config(current)
        .map(|config| error_messages(&config))
        .unwrap_or_default();
    move |config, design| {
        let typed = decode_config(config)?;
        if let Some(new) = error_messages(&typed)
            .into_iter()
            .find(|message| !existing.contains(message))
        {
            return Err(new);
        }
        for spec in DESIGN_VARIABLE_SPECS {
            if let Some(value) = design.get(spec.name) {
                if !(spec.preset_lower..=spec.preset_upper).contains(value) {
                    return Err(format!(
                        "{} = {value} {} is outside the guardrail {} to {}",
                        spec.name, spec.unit, spec.preset_lower, spec.preset_upper
                    ));
                }
            }
        }
        Ok(())
    }
}

impl AppState {
    /// Load the add-ons folder on first use: install the bundled examples if
    /// this is the first run, then scan. Not called at start-up so tests and
    /// embedders never touch the user's folder unless the page is opened.
    pub(crate) fn ensure_addons_loaded(&mut self) {
        if self.addons.loaded {
            return;
        }
        self.addons.loaded = true;
        self.addons.dir = addons_dir(self);
        match alas_addons::install_examples(&self.addons.dir) {
            Ok(created) if !created.is_empty() => self.log(
                format!("Installed example add-ons: {}.", created.join(", ")),
                LogKind::Info,
            ),
            Ok(_) => {}
            Err(error) => self.log(
                format!("Add-on examples not installed: {error}"),
                LogKind::Warn,
            ),
        }
        self.reload_addons();
    }

    /// Rescan the add-ons folder and reload its settings.
    pub(crate) fn reload_addons(&mut self) {
        self.addons.discovery = alas_addons::discover(&self.addons.dir);
        match AddonSettings::load(&self.addons.dir) {
            Ok(settings) => self.addons.settings = settings,
            Err(error) => {
                self.log(format!("Add-on settings ignored: {error}"), LogKind::Warn);
                self.addons.settings = AddonSettings::default();
            }
        }
        self.addons.python_text = self.addons.settings.python.clone().unwrap_or_default();
        let still_there = self
            .addons
            .selected
            .as_ref()
            .is_some_and(|id| self.addons.discovery.addons.iter().any(|a| &a.id == id));
        if !still_there {
            self.addons.selected = self.addons.discovery.addons.first().map(|a| a.id.clone());
        }
        self.addons.preview = None;
    }

    pub(crate) fn save_addon_settings(&mut self) {
        if let Err(error) = self.addons.settings.save(&self.addons.dir) {
            self.log(format!("Add-on settings not saved: {error}"), LogKind::Warn);
        }
    }

    /// The read-only design summary handed to add-ons: the preset, the design
    /// variables, and, after a run, the analysis geometry and mass numbers.
    pub fn addon_design_summary(&self) -> Value {
        let mut summary = serde_json::json!({
            "preset": self.active_preset,
            "design_vector": self.design_values,
        });
        let report = self.pipeline_result.as_ref().and_then(|result| {
            result
                .optimized_report
                .as_ref()
                .or(result.baseline_analysis.as_ref())
        });
        if let (Some(report), Some(map)) = (report, summary.as_object_mut()) {
            map.insert(
                "analysis".to_owned(),
                serde_json::json!({
                    "geometry_summary": report.geometry_summary,
                    "component_masses_kg": report.component_masses,
                    "static_margin": report.static_margin,
                }),
            );
        }
        summary
    }

    /// Start the selected add-on on a worker thread.
    pub(crate) fn run_selected_addon(&mut self) {
        if self.addons.running.is_some() {
            return;
        }
        let Some(id) = self.addons.selected.clone() else {
            return;
        };
        let Some(addon) = self
            .addons
            .discovery
            .addons
            .iter()
            .find(|addon| addon.id == id)
            .cloned()
        else {
            return;
        };
        let mut inputs = BTreeMap::new();
        for spec in &addon.manifest.inputs {
            let text = self
                .addons
                .input_text
                .get(&format!("{id}/{}", spec.name))
                .map(|text| text.trim().to_owned())
                .unwrap_or_default();
            if text.is_empty() {
                continue;
            }
            match text.parse::<f64>() {
                Ok(value) => {
                    inputs.insert(spec.name.clone(), value);
                }
                Err(_) => {
                    self.addons.status = Some(format!("`{}` is not a number: {text}", spec.name));
                    return;
                }
            }
        }
        let design = self.addon_design_summary();
        let settings = self.addons.settings.clone();
        let (tx, rx) = channel();
        let spawned = std::thread::Builder::new()
            .name("alas-addon".to_owned())
            .spawn(move || {
                let _ = tx.send(alas_addons::run_addon(&addon, &inputs, &design, &settings));
            });
        if let Err(error) = spawned {
            self.addons.status = Some(format!("cannot start the add-on worker: {error}"));
            return;
        }
        self.addons.preview = None;
        self.addons.status = None;
        self.addons.running = Some((id, rx));
    }

    /// Collect a finished add-on run; called every frame.
    pub fn poll_addons(&mut self) {
        let Some((id, rx)) = self.addons.running.as_ref() else {
            return;
        };
        let finished = match rx.try_recv() {
            Ok(result) => Some(result.map_err(|error| error.to_string())),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("the add-on worker stopped".to_owned())),
        };
        let Some(result) = finished else {
            return;
        };
        let id = id.clone();
        self.addons.running = None;
        let name = self
            .addons
            .discovery
            .addons
            .iter()
            .find(|addon| addon.id == id)
            .map(|addon| addon.manifest.name.clone())
            .unwrap_or_else(|| id.clone());
        match &result {
            Ok(outcome) => {
                let summary: Vec<String> = outcome
                    .outputs
                    .iter()
                    .map(|(key, value)| format!("{key} = {value}"))
                    .collect();
                self.log(
                    format!("Add-on {name}: {}", summary.join(", ")),
                    LogKind::Info,
                );
                if !outcome.log.trim().is_empty() {
                    self.log(
                        format!("Add-on {name} log: {}", outcome.log.trim()),
                        LogKind::Info,
                    );
                }
            }
            Err(error) => self.log(format!("Add-on {name} failed: {error}"), LogKind::Warn),
        }
        self.addons.last = Some(LastRun { id, result });
    }

    /// Before/after rows for the last run's targeted outputs.
    pub(crate) fn preview_addon_changes(&mut self) {
        let Some(last) = self.addons.last.as_ref() else {
            return;
        };
        let Ok(outcome) = &last.result else {
            return;
        };
        let Some(addon) = self
            .addons
            .discovery
            .addons
            .iter()
            .find(|addon| addon.id == last.id)
        else {
            return;
        };
        match apply::plan(
            &addon.manifest,
            &outcome.outputs,
            &self.config_values,
            &self.design_values,
        ) {
            Ok(rows) => {
                self.addons.status = rows
                    .is_empty()
                    .then(|| "This add-on has no output that targets a design field.".to_owned());
                self.addons.preview = Some(rows);
            }
            Err(error) => {
                self.addons.preview = None;
                self.addons.status = Some(error.to_string());
            }
        }
    }

    /// Write the previewed rows into the design, through validation.
    pub(crate) fn apply_addon_changes(&mut self) {
        let Some(rows) = self.addons.preview.take() else {
            return;
        };
        if rows.is_empty() {
            return;
        }
        // A registered preset keeps its geometry and design point; the policy
        // would silently restore them, so refuse up front instead of claiming
        // a change that does not hold.
        let touches_geometry = rows.iter().any(|row| {
            row.target.starts_with(apply::DESIGN_PREFIX) || row.target.starts_with("geometry.")
        });
        if touches_geometry && self.manual_geometry_locked() {
            self.addons.status = Some(
                "Preset geometry and the design point are protected from manual edits; open the sandbox to apply geometry values."
                    .to_owned(),
            );
            self.addons.preview = Some(rows);
            return;
        }
        let validator = candidate_validator(&self.config_values);
        let mut config = self.config_values.clone();
        let mut design = self.design_values.clone();
        match apply::apply(&rows, &mut config, &mut design, &validator) {
            Ok(records) => {
                self.config_values = config;
                self.design_values = design;
                self.on_config_modified();
                let kept = self.addon_records_in_effect(&records);
                for record in &records {
                    let line = record.provenance_line();
                    if kept.iter().any(|k| k.target == record.target) {
                        self.log(format!("Add-on applied: {line}"), LogKind::Info);
                    } else {
                        self.log(
                            format!("Add-on value was reverted by preset protection: {line}"),
                            LogKind::Warn,
                        );
                    }
                }
                self.addons
                    .applied
                    .retain(|old| !kept.iter().any(|new| new.target == old.target));
                let all_kept = kept.len() == records.len();
                self.addons.applied.extend(kept);
                self.addons.status = Some(if all_kept {
                    "Applied.".to_owned()
                } else {
                    "Some values were reverted by the model's own protection; see the run log."
                        .to_owned()
                });
            }
            Err(error) => {
                self.addons.preview = Some(rows);
                self.addons.status = Some(error.to_string());
            }
        }
    }

    /// The records whose target still holds the value the add-on set.
    fn addon_records_in_effect(&self, records: &[AppliedRecord]) -> Vec<AppliedRecord> {
        records
            .iter()
            .filter(|record| {
                let current = match Target::parse(&record.target) {
                    Ok(Target::Config(pointer)) => {
                        self.config_values.pointer(&pointer).and_then(Value::as_f64)
                    }
                    Ok(Target::Design(name)) => self.design_values.get(&name).copied(),
                    Err(_) => None,
                };
                current.is_some_and(|value| {
                    (value - record.after).abs() <= 1e-12 * record.after.abs().max(1.0)
                })
            })
            .cloned()
            .collect()
    }

    /// Called when a run starts: log every add-on value still in effect and
    /// write `addon-provenance.json` into the run's output folder, so the
    /// results say which numbers were set by an add-on.
    pub(crate) fn record_addon_provenance(&mut self, output_dir: Option<&Path>) {
        let in_effect = self.addon_records_in_effect(&self.addons.applied.clone());
        self.addons.applied = in_effect.clone();
        if in_effect.is_empty() {
            return;
        }
        for record in &in_effect {
            self.log(
                format!("Add-on: {}", record.provenance_line()),
                LogKind::Info,
            );
        }
        if let Some(dir) = output_dir {
            let written = std::fs::create_dir_all(dir).and_then(|()| {
                std::fs::write(
                    dir.join(PROVENANCE_FILE),
                    apply::provenance_json(&in_effect),
                )
            });
            if let Err(error) = written {
                self.log(
                    format!("Add-on provenance file not written: {error}"),
                    LogKind::Warn,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_addons::{AddonManifest, InstalledAddon};

    fn state_with_addon(target: &str, value: f64) -> AppState {
        let mut state = AppState::default();
        let manifest = AddonManifest::parse(&format!(
            "name = \"Demo\"\nversion = \"3.1\"\nkind = \"script\"\nentry = \"m.rhai\"\n[[outputs]]\nname = \"y\"\nunit = \"-\"\ntarget = \"{target}\"\n"
        ))
        .unwrap();
        state.addons.selected = Some("demo".to_owned());
        state.addons.discovery.addons.push(InstalledAddon {
            id: "demo".to_owned(),
            dir: PathBuf::new(),
            manifest,
        });
        state.addons.last = Some(LastRun {
            id: "demo".to_owned(),
            result: Ok(RunOutcome {
                outputs: BTreeMap::from([("y".to_owned(), value)]),
                messages: Vec::new(),
                log: String::new(),
            }),
        });
        state
    }

    #[test]
    fn applying_a_design_variable_records_provenance_and_writes_the_run_file() {
        let mut state = state_with_addon("design_vector.span_m", 55.5);
        // No active preset: the geometry lock applies only to a registered case.
        state.active_preset.clear();
        state.preview_addon_changes();
        let rows = state.addons.preview.clone().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].after, 55.5);
        state.apply_addon_changes();
        assert_eq!(
            state.design_values["span_m"], 55.5,
            "{:?}",
            state.addons.status
        );
        assert_eq!(state.addons.applied().len(), 1);

        let dir = std::env::temp_dir().join(format!("alas-addon-prov-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        state.record_addon_provenance(Some(&dir));
        let text = std::fs::read_to_string(dir.join(PROVENANCE_FILE)).unwrap();
        assert!(text.contains("set by add-on Demo 3.1"), "{text}");
        assert!(state
            .logs
            .iter()
            .any(|line| line.text.contains("set by add-on Demo 3.1")));
        let _ = std::fs::remove_dir_all(&dir);

        // A value edited by hand afterwards is no longer attributed to the add-on.
        state.design_values.insert("span_m".to_owned(), 60.0);
        state.record_addon_provenance(None);
        assert!(state.addons.applied().is_empty());
    }

    #[test]
    fn a_value_outside_the_guardrail_is_refused_and_leaves_the_design_unchanged() {
        let mut state = state_with_addon("design_vector.span_m", 5000.0);
        state.active_preset.clear();
        let before = state.design_values["span_m"];
        state.preview_addon_changes();
        state.apply_addon_changes();
        assert_eq!(state.design_values["span_m"], before);
        assert!(state.addons.applied().is_empty());
        assert!(state
            .addons
            .status
            .as_deref()
            .unwrap_or("")
            .contains("guardrail"));
    }

    #[test]
    fn a_registered_preset_refuses_geometry_targets_instead_of_claiming_them() {
        let mut state = state_with_addon("design_vector.span_m", 55.5);
        state.active_preset = "a-preset".to_owned();
        let before = state.design_values["span_m"];
        state.preview_addon_changes();
        state.apply_addon_changes();
        assert_eq!(state.design_values["span_m"], before);
        assert!(state.addons.applied().is_empty());
        assert!(state
            .addons
            .status
            .as_deref()
            .unwrap_or("")
            .contains("protected"));
        assert!(state.addons.preview.is_some());
    }

    #[test]
    fn a_config_target_is_validated_through_the_typed_configuration() {
        let mut state = state_with_addon("mass_model.systems_mass_fraction", 0.11);
        state.preview_addon_changes();
        assert!(state.addons.preview.is_some(), "{:?}", state.addons.status);
        state.apply_addon_changes();
        assert_eq!(
            state
                .config_values
                .pointer("/mass_model/systems_mass_fraction"),
            Some(&serde_json::json!(0.11))
        );
        assert_eq!(state.addons.applied().len(), 1);

        let mut unknown = state_with_addon("mass_model.no_such_field", 1.0);
        unknown.preview_addon_changes();
        unknown.apply_addon_changes();
        assert!(unknown.addons.applied().is_empty());
        assert!(unknown
            .config_values
            .pointer("/mass_model/no_such_field")
            .is_none());
    }
}
