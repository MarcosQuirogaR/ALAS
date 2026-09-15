// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Editing the configuration: presets, engine choice, aux-presets, the design
//! space sampler, and file Load/Save.
//!
//! Split out of [`crate::state`] to keep that module under the line limit;
//! everything here is an `impl AppState` block.

use std::collections::BTreeMap;

use alas_config::{
    fidelity_presets, performance_presets, presets, solver_presets, validate, AlasConfig,
    DesignMode, EngineConfig, FuelPolicyConfig, FuelTankLayoutConfig, DESIGN_VARIABLE_SPECS,
};
use alas_exec::ToolPreferences;
use serde_json::Value;

use crate::nav::PresetKind;
use crate::state::{AppState, LogKind};
use crate::views::design_space_view::design_mode_display_name;
use crate::views::{tr, tr_fields};

/// Serialize a configuration for the generic form editor, which edits this
/// JSON directly and needs every schema field present to find it back.
///
/// `fuel_policy` and `fuel_tanks` skip serialization when they equal their
/// default, so a fresh or reloaded configuration, which starts at that
/// default, would otherwise be missing both keys entirely.
pub(crate) fn full_config_values(config: &AlasConfig) -> Value {
    let mut value = serde_json::to_value(config).unwrap_or(Value::Null);
    if let Some(map) = value.as_object_mut() {
        map.entry("fuel_policy").or_insert_with(|| {
            serde_json::to_value(FuelPolicyConfig::default()).unwrap_or(Value::Null)
        });
        map.entry("fuel_tanks").or_insert_with(|| {
            serde_json::to_value(FuelTankLayoutConfig::default()).unwrap_or(Value::Null)
        });
    }
    value
}

impl AppState {
    /// Persist optional-tool locations separately from an aircraft
    /// config, so restarting the GUI retains setup choices without making
    /// mission configuration depend on an obsolete Python runtime.
    pub fn save_tool_preferences(&mut self) {
        let config = match self.typed_config() {
            Some(config) => config,
            None => return,
        };
        self.tool_preferences = ToolPreferences {
            mses_dir: nonempty(&config.mses.mses_dir),
            nastran_exe: nonempty(&config.structures.nastran_exe_path),
            nastran_solver: nonempty(&config.structures.nastran_solver_path),
            nastran95_dir: nonempty(&config.structures.nastran95_dir_path),
            nastran95_runtime: nonempty(&config.structures.nastran95_runtime_path),
            nastran95_rf_stage: nonempty(&config.structures.nastran95_rf_stage_path),
            nastran95_open_core_words: nonempty(&config.structures.nastran95_open_core_words),
            patran_exe: nonempty(&config.structures.patran_exe_path),
            openvsp_dir: self.tool_preferences.openvsp_dir.clone(),
            avl_exe: self.tool_preferences.avl_exe.clone(),
            navdata_dir: nonempty(&config.mission.navdata_dir),
            routes_dir: nonempty(&config.mission.routes_dir),
        };
        if let Err(error) = self.tool_locator.save_preferences(&self.tool_preferences) {
            self.log(
                tr_fields(
                    "Tool preferences not saved: {error}",
                    &[("error", error.to_string())],
                ),
                LogKind::Warn,
            );
        }
    }

    /// Load a preset by its registry key, replacing geometry, requirements and
    /// the calibrated mass and performance models, as the reference does.
    pub fn load_preset(&mut self, key: &str) {
        let preset = match presets::get(key) {
            Ok(p) => p,
            Err(_) => return,
        };

        let mut config = self.typed_config().unwrap_or_default();
        let operational = preset.operational_mission_defaults();
        config.preset = preset.name.to_owned();
        config.geometry = preset.geometry.clone();
        config.geometry.engine.apply_engine_spec();
        config.requirements = preset.requirements.clone();
        // Keep the interactive path identical to `AlasConfig::from_value`:
        // registered aircraft carry a representable planning cabin seed,
        // while their passenger count remains derived from the editable class
        // shares. Without this assignment a preset loaded after startup kept
        // the previous cabin (often the generic 15/85 mix), so the displayed
        // aircraft and its payload layout disagreed.
        config.cabin = preset.planning_cabin_config();
        config.landing_gear = preset.landing_gear.clone();
        if let Some(mm) = &preset.mass_model {
            config.mass_model = mm.clone();
        }
        if let Some(perf) = &preset.performance {
            config.performance = perf.clone();
        }
        config.departure_airport = operational.departure_airport.to_owned();
        config.arrival_airport = operational.arrival_airport.to_owned();
        config.mission.profile = operational.profile;

        self.config_values = full_config_values(&config);
        self.active_preset = preset.name.to_owned();

        // Recenter the design space on the preset's own design vector, using
        // the selected design-mode envelope rather than the retired fixed
        // percentage window. The same envelope is applied by the product
        // evaluator, so the GUI cannot offer a reference limit it will later
        // ignore.
        if let Ok(dv) = serde_json::to_value(preset.design_vector) {
            if let Some(map) = dv.as_object() {
                for spec in DESIGN_VARIABLE_SPECS {
                    if let Some(v) = map.get(spec.name).and_then(Value::as_f64) {
                        self.design_values.insert(spec.name.to_owned(), v);
                    }
                }
            }
        }
        self.reset_design_space_bounds_to_mode();

        self.log(
            tr_fields(
                "Loaded preset: {name}",
                &[("name", preset.display_name.to_owned())],
            ),
            LogKind::Info,
        );
        self.on_config_modified();
    }

    /// Set the selected engine on the geometry group.
    pub fn set_engine(&mut self, name: &str) {
        if let Some(engine) = self
            .config_values
            .get_mut("geometry")
            .and_then(|g| g.get_mut("engine"))
        {
            if let Ok(mut selected) = serde_json::from_value::<EngineConfig>(engine.clone()) {
                selected.engine_name = name.to_owned();
                selected.apply_engine_spec();
                if let Ok(value) = serde_json::to_value(selected) {
                    *engine = value;
                }
            } else if let Some(obj) = engine.as_object_mut() {
                obj.insert("engine_name".to_owned(), Value::String(name.to_owned()));
            }
        }
        self.log(
            tr_fields("Engine changed to: {name}", &[("name", name.to_owned())]),
            LogKind::Info,
        );
        self.on_config_modified();
    }

    /// Return the design mode currently represented by the edited config.
    ///
    /// A missing `optimizer.design_space` object is valid for a default
    /// serialized config because the config crate skips that default group;
    /// decoding through `AlasConfig` therefore supplies the canonical clean-
    /// sheet mode instead of making the GUI invent a second default.
    pub fn design_mode(&self) -> DesignMode {
        self.typed_config()
            .map(|config| config.optimizer.design_space.mode)
            .unwrap_or_default()
    }

    /// Select the product design mode and synchronize the GUI envelope with
    /// the typed config contract.
    pub fn set_design_mode(&mut self, mode: DesignMode) {
        let Some(mut config) = self.typed_config() else {
            self.log(
                tr("Configuration is not currently valid; the design mode was not changed."),
                LogKind::Error,
            );
            return;
        };
        if config.optimizer.design_space.mode == mode {
            self.run_options.optimize = mode != DesignMode::BaselineSandbox;
            if mode == DesignMode::BaselineSandbox {
                self.run_options.compare_baseline = false;
            }
            if mode == DesignMode::CleanSheet && !config.preset.is_empty() {
                // A registered aircraft is a reference starting point. Once
                // the user explicitly chooses New aircraft, clear that
                // provenance so clean-sheet-only inputs (including passenger
                // target) become available while retaining the current shape
                // as a useful starting geometry.
                config.preset.clear();
                self.active_preset.clear();
                self.config_values = full_config_values(&config);
                self.on_config_modified();
            }
            self.reset_design_space_bounds_to_mode();
            return;
        }
        config.optimizer.design_space.mode = mode;
        if mode == DesignMode::CleanSheet {
            // New aircraft studies may start from the currently displayed
            // geometry, but they must not carry a registered aircraft's
            // fixed passenger load case into the product model.
            config.preset.clear();
            self.active_preset.clear();
        }
        self.config_values = full_config_values(&config);
        self.run_options.optimize = mode != DesignMode::BaselineSandbox;
        if mode == DesignMode::BaselineSandbox {
            self.run_options.compare_baseline = false;
        }
        self.reset_design_space_bounds_to_mode();
        self.log(
            tr_fields(
                "Design mode changed to: {mode}",
                &[("mode", design_mode_display_name(mode))],
            ),
            LogKind::Info,
        );
        self.on_config_modified();
    }

    /// Rebuild the GUI bounds from the same typed design-mode envelope the
    /// product optimizer intersects with its request. This is called after a
    /// mode/window/preset change; ordinary design-space edits still remain
    /// explicit run bounds within that declared envelope.
    pub fn reset_design_space_bounds_to_mode(&mut self) {
        let Some(config) = self.typed_config() else {
            return;
        };
        let nominal = self.current_design().unwrap_or_default();
        for variable in config.optimizer.design_space.envelope(&nominal) {
            self.bounds
                .insert(variable.name.to_owned(), (variable.lower, variable.upper));
            if variable.fixed {
                self.design_values
                    .insert(variable.name.to_owned(), variable.nominal);
            }
        }
    }

    /// Keep variables fixed by the typed envelope fixed immediately before a
    /// run. This protects the baseline sandbox and clean-sheet cabin sizing
    /// path even when the user last edited another page.
    pub fn enforce_design_space_fixed_variables(&mut self) {
        let Some(config) = self.typed_config() else {
            return;
        };
        let mut nominal = self.current_design().unwrap_or_default();
        // A cabin-sized clean-sheet fuselage is a derived coordinate. Keep
        // the editor, the initial point, and the optimizer bounds on the same
        // materialized length so a GUI run cannot publish a vector that the
        // evaluator silently replaces during candidate construction.
        if config.optimizer.design_space.sizes_fuselage_from_cabin()
            && config.requirements.aircraft_type != "cargo"
        {
            if let Ok(canonical) = alas_opt::canonicalize_design(&config, nominal) {
                nominal = canonical;
                self.design_values
                    .insert("fuselage_length_m".to_owned(), nominal.fuselage_length_m);
            }
        }
        for variable in config.optimizer.design_space.envelope(&nominal) {
            if variable.fixed {
                self.design_values
                    .insert(variable.name.to_owned(), variable.nominal);
                self.bounds
                    .insert(variable.name.to_owned(), (variable.lower, variable.upper));
            }
        }
    }

    /// Apply one aux-preset (the Py6-era fidelity/solver/performance pickers)
    /// on top of the current configuration, overwriting only the group or
    /// sub-group it names, unlike an aircraft preset, which replaces the
    /// whole configuration.
    pub fn apply_aux_preset(&mut self, kind: PresetKind, name: &str) {
        let applied = match kind {
            PresetKind::Fidelity => fidelity_presets::get(name).ok().and_then(|p| {
                serde_json::to_value(&p.analysis)
                    .ok()
                    .map(|v| ("analysis".to_owned(), v))
            }),
            PresetKind::Performance => performance_presets::get(name).ok().and_then(|p| {
                serde_json::to_value(&p.settings)
                    .ok()
                    .map(|v| ("performance".to_owned(), v))
            }),
            PresetKind::Solver => solver_presets::get(name).ok().and_then(|p| {
                serde_json::to_value(&p.settings).ok().map(|v| {
                    let mut wrapper = serde_json::Map::new();
                    wrapper.insert("solver".to_owned(), v);
                    ("optimizer".to_owned(), Value::Object(wrapper))
                })
            }),
        };
        let Some((group, patch)) = applied else {
            return;
        };
        if let Some(slot) = self.config_values.get_mut(&group) {
            // The solver patch only names `solver`; merge rather than replace
            // so the rest of `optimizer` (the objective weights) survives.
            if let (Some(dst), Some(src)) = (slot.as_object_mut(), patch.as_object()) {
                for (k, v) in src {
                    dst.insert(k.clone(), v.clone());
                }
            }
        }
        self.selected_aux_preset
            .insert(kind.code().to_owned(), name.to_owned());
        self.log(
            tr_fields(
                "Applied {kind} preset: {name}",
                &[("kind", tr(kind.code())), ("name", name.to_owned())],
            ),
            LogKind::Info,
        );
        self.on_config_modified();
    }

    /// Revalidate and rebuild the live preview after an edit.
    ///
    /// A registered preset's protected geometry is restored first, and a
    /// sandbox edit is routed to the sandbox's own revision bookkeeping.
    pub fn on_config_modified(&mut self) {
        if self.sandbox.active() {
            self.on_sandbox_model_changed();
            return;
        }
        self.enforce_preset_geometry();
        if let Some(config) = self.typed_config() {
            self.validation_findings = validate(&config);
        }
        self.update_preview_scene();
    }

    /// Restore one schema group without discarding edits on other pages.
    pub fn reset_group_to_defaults(&mut self, group: &str) -> bool {
        let Ok(defaults) = serde_json::to_value(alas_config::AlasConfig::default()) else {
            return false;
        };
        let Some(default_group) = defaults.get(group).cloned() else {
            return false;
        };
        let Some(root) = self.config_values.as_object_mut() else {
            return false;
        };
        root.insert(group.to_owned(), default_group);
        self.log(
            tr_fields(
                "Reset {group} settings to defaults.",
                &[("group", tr(group))],
            ),
            LogKind::Info,
        );
        self.on_config_modified();
        true
    }

    /// Draw one design point per variable, uniformly within bounds widened by
    /// `widen` on each side (0.0 keeps it inside the bounds; 0.3 is Random's
    /// reach beyond them). Reproduces the reference's client-side sampler.
    pub fn sample_design(&self, widen: f64) -> BTreeMap<String, f64> {
        let mut out = BTreeMap::new();
        for spec in DESIGN_VARIABLE_SPECS {
            let (base_lo, base_hi) = self
                .bounds
                .get(spec.name)
                .copied()
                .unwrap_or((spec.lower, spec.upper));
            let span = base_hi - base_lo;
            let lo = base_lo - span * widen;
            let hi = base_hi + span * widen;
            // A cheap uniform draw off the wall clock; the reference used
            // Math.random(), an equally unseeded PRNG, for the same purpose.
            let t = pseudo_random(spec.name);
            out.insert(spec.name.to_owned(), lo + t * (hi - lo));
        }
        out
    }

    /// Save the current configuration to [`AppState::config_path`] as JSON.
    pub fn save_config(&mut self) {
        let text = match serde_json::to_string_pretty(&self.workspace_document()) {
            Ok(t) => t,
            Err(e) => {
                self.log(
                    tr_fields("Save failed: {error}", &[("error", e.to_string())]),
                    LogKind::Error,
                );
                return;
            }
        };
        let path = self.config_path.clone();
        match std::fs::write(&path, text) {
            Ok(()) => self.log(
                tr_fields("Saved configuration to {path}.", &[("path", path)]),
                LogKind::Info,
            ),
            Err(e) => self.log(
                tr_fields("Save failed: {error}", &[("error", e.to_string())]),
                LogKind::Error,
            ),
        }
    }

    /// Load a configuration from [`AppState::config_path`] (JSON or YAML).
    pub fn load_config(&mut self) {
        let path = self.config_path.clone();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.log(
                    tr_fields("Load failed: {error}", &[("error", e.to_string())]),
                    LogKind::Error,
                );
                return;
            }
        };
        let parsed: Result<Value, String> = if path.ends_with(".yaml") || path.ends_with(".yml") {
            serde_yaml::from_str(&text).map_err(|e| e.to_string())
        } else {
            serde_json::from_str(&text).map_err(|e| e.to_string())
        };
        match parsed {
            Ok(value) => match self.apply_workspace_document(&value) {
                Ok(()) => {
                    self.save_tool_preferences();
                    self.log(
                        tr_fields("Loaded configuration from {path}.", &[("path", path)]),
                        LogKind::Info,
                    );
                }
                Err(error) => self.log(
                    tr_fields("Load failed: {error}", &[("error", error)]),
                    LogKind::Error,
                ),
            },
            Err(e) => self.log(
                tr_fields("Load failed: {error}", &[("error", e.to_string())]),
                LogKind::Error,
            ),
        }
    }
}

fn nonempty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

/// A deterministic-per-name pseudo-random draw in `[0, 1)`.
///
/// Seeded off the wall clock and the variable name so each variable of one
/// sample gets a different value; the reference used `Math.random()`, which is
/// equally unseeded, so nothing here depends on the sequence being reproducible.
fn pseudo_random(seed: &str) -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let mut hash = nanos ^ 0x9e37_79b9_7f4a_7c15;
    for byte in seed.bytes() {
        hash = hash
            .wrapping_mul(0x0100_0000_01b3)
            .wrapping_add(byte as u64);
    }
    // Take the top 53 bits, the way a double's mantissa is filled.
    ((hash >> 11) as f64) / ((1u64 << 53) as f64)
}
