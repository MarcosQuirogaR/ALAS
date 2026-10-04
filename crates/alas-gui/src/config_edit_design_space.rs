// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Enforcing the typed design-space envelope's fixed variables, and
//! committing a clean-sheet brief's derived start, box and geometry.
//!
//! Split out of [`crate::config_edit`] to keep that module under the
//! project's line limit.

use std::hash::{Hash, Hasher};

use alas_config::{AerodromeReferenceCode, AlasConfig, DesignMode, DESIGN_VARIABLE_SPECS};

use super::full_config_values;
use crate::state::{AppState, LogKind};
use crate::views::tr_fields;

/// What prompted a clean-sheet commit, which decides whether the design
/// values may be rewritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BriefCommit {
    /// The user chose New aircraft; `from_preset` when that cleared a
    /// registered aircraft, whose objective code is not a clean-sheet choice.
    NewAircraft {
        /// Whether a registered preset was cleared by this choice.
        from_preset: bool,
    },
    /// A run is starting: re-derive only if the brief changed since the last
    /// commit (or was never committed in this session).
    RunStart,
    /// Any other envelope rebuild: bounds only, the design values are kept.
    BoundsOnly,
}

fn hash_of(value: &serde_json::Value) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.to_string().hash(&mut hasher);
    hasher.finish()
}

/// Fingerprints of what a clean-sheet derivation reads: the brief itself
/// (requirements, fuselage cross-section, aerodrome code), which also drives
/// the dependent geometry, and the other inputs the start reads (cabin seat
/// geometry, engine selection and nacelle, wing, fuselage and empennage
/// configuration).
pub(crate) fn brief_fingerprints(config: &AlasConfig) -> (u64, u64) {
    let brief = serde_json::json!({
        "requirements": config.requirements,
        "diameter_m": config.geometry.fuselage.diameter_m,
        "height_m": config.geometry.fuselage.height_m,
        "code": config.optimizer.objective.aerodrome_reference_code,
    });
    let engine = &config.geometry.engine;
    let inputs = serde_json::json!({
        "cabin": config.cabin.passenger,
        "engine": [engine.engine_name.clone()],
        "radius_m": engine.radius_scale_m,
        "nacelle_m": engine.nacelle_length_m(),
        "wing": config.geometry.wing,
        "fuselage": config.geometry.fuselage,
        "empennage": config.geometry.empennage,
        "design_space": config.optimizer.design_space,
    });
    (hash_of(&brief), hash_of(&inputs))
}

impl AppState {
    /// Commit a preset-less clean-sheet brief.
    ///
    /// On New aircraft, or at a run start after the brief changed, the brief
    /// is marked (`optimizer.design_space.clean_sheet_brief`), a code still on
    /// the default F becomes `Auto` (New aircraft from a preset, or a brief
    /// never committed), the dependent geometry is re-derived through
    /// [`AlasConfig::apply_clean_sheet_commit`] (the function the file path
    /// uses) when the brief itself changed, and the design values and bounds
    /// become the derived start and box. Every other commit keeps the design
    /// values and rebuilds only the bounds, widened to contain them. An
    /// `Auto` code is re-resolved on every commit.
    ///
    /// Returns `false`, changing nothing, for a registered preset, another
    /// design mode or a brief that is not a clean-sheet brief.
    pub(crate) fn commit_clean_sheet_brief(
        &mut self,
        mut config: AlasConfig,
        commit: BriefCommit,
    ) -> bool {
        if !config.preset.is_empty() || config.optimizer.design_space.mode != DesignMode::CleanSheet
        {
            return false;
        }
        let space = &config.optimizer.design_space;
        let undecided = space.clean_sheet_brief.is_none();
        let fresh = undecided && self.clean_sheet_brief_fingerprint.is_none();
        let departs = config.brief_departs_from_reference();
        let write = match commit {
            BriefCommit::NewAircraft { from_preset } if departs => Some(from_preset || undecided),
            BriefCommit::RunStart if fresh && departs => Some(true),
            BriefCommit::RunStart if config.derives_clean_sheet_start() => self
                .clean_sheet_brief_fingerprint
                .is_some_and(|stored| stored != brief_fingerprints(&config))
                .then_some(false),
            _ if config.derives_clean_sheet_start() => None,
            _ => return false,
        };
        if let Some(default_code_is_auto) = write {
            config.optimizer.design_space.clean_sheet_brief = Some(true);
            let objective = &mut config.optimizer.objective;
            if default_code_is_auto
                && objective.aerodrome_reference_code == AerodromeReferenceCode::default()
            {
                objective.aerodrome_reference_code = AerodromeReferenceCode::Auto;
            }
        }
        let brief_changed = self
            .clean_sheet_brief_fingerprint
            .is_none_or(|(core, _)| core != brief_fingerprints(&config).0);
        let new_aircraft = matches!(commit, BriefCommit::NewAircraft { .. });
        config.apply_clean_sheet_commit(
            &serde_json::json!({}),
            write.is_some() && (brief_changed || new_aircraft),
        );
        let values = full_config_values(&config);
        if values != self.config_values {
            self.config_values = values;
        }
        if write.is_none() {
            if commit == BriefCommit::RunStart && self.clean_sheet_brief_fingerprint.is_none() {
                self.clean_sheet_brief_fingerprint = Some(brief_fingerprints(&config));
            }
            let nominal = self
                .current_design()
                .unwrap_or_else(|| config.configured_nominal_design());
            for variable in config.design_envelope(&nominal) {
                self.bounds
                    .insert(variable.name.to_owned(), (variable.lower, variable.upper));
            }
            return true;
        }
        let nominal = alas_opt::configured_nominal_design(&config)
            .unwrap_or_else(|_| config.configured_nominal_design());
        for (spec, value) in DESIGN_VARIABLE_SPECS.iter().zip(nominal.to_array()) {
            self.design_values.insert(spec.name.to_owned(), value);
        }
        for variable in config.design_envelope(&nominal) {
            self.bounds
                .insert(variable.name.to_owned(), (variable.lower, variable.upper));
        }
        self.clean_sheet_brief_fingerprint = Some(brief_fingerprints(&config));
        let code = config.aerodrome_reference_code();
        let limit = config
            .max_design_span_m()
            .map_or_else(|| "none".to_owned(), |span| format!("{span:.2} m"));
        self.log(
            tr_fields(
                "Clean-sheet start and search box derived from the brief (aerodrome code {code}, span limit {limit}).",
                &[("code", code.as_str().to_owned()), ("limit", limit)],
            ),
            LogKind::Info,
        );
        true
    }

    /// Re-derive a clean-sheet brief's start and box when a run starts and
    /// the brief changed since it was last committed, so typing in a
    /// requirement field never rewrites the design space keystroke by
    /// keystroke.
    pub(crate) fn refresh_clean_sheet_brief(&mut self) {
        if let Some(config) = self.typed_config() {
            self.commit_clean_sheet_brief(config, BriefCommit::RunStart);
        }
    }

    /// Record the current brief as committed without deriving anything: a
    /// loaded or promoted design is the user's explicit design point.
    pub fn seed_clean_sheet_brief(&mut self) {
        self.clean_sheet_brief_fingerprint = self.typed_config().map(|c| brief_fingerprints(&c));
    }

    /// Rebuild the GUI bounds from the typed design-mode envelope after a
    /// mode, window or preset change, a load or a Design Space edit. The
    /// design values are kept; a clean-sheet brief's bounds come from its
    /// derived box (see [`Self::commit_clean_sheet_brief`]).
    pub fn reset_design_space_bounds_to_mode(&mut self) {
        let Some(config) = self.typed_config() else {
            return;
        };
        if self.commit_clean_sheet_brief(config.clone(), BriefCommit::BoundsOnly) {
            return;
        }
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

    /// Seed the brief fingerprint and rebuild the bounds around the current
    /// design values: what a workspace load and a sandbox Promote do, since
    /// their design point is the user's own.
    pub fn keep_design_and_reset_bounds(&mut self) {
        self.seed_clean_sheet_brief();
        self.reset_design_space_bounds_to_mode();
    }

    /// Rebuild the design space after a design-mode change: New aircraft
    /// commits a clean-sheet brief, anything else rebuilds the bounds.
    pub(crate) fn reset_design_space_after_mode_change(
        &mut self,
        mode: DesignMode,
        from_preset: bool,
    ) {
        if mode == DesignMode::CleanSheet {
            if let Some(config) = self.typed_config() {
                let commit = BriefCommit::NewAircraft { from_preset };
                if self.commit_clean_sheet_brief(config, commit) {
                    return;
                }
            }
        }
        self.reset_design_space_bounds_to_mode();
    }

    /// Keep variables fixed by the typed envelope fixed immediately before a
    /// run. This protects the baseline sandbox and clean-sheet cabin sizing
    /// path even when the user last edited another page.
    ///
    /// Idempotent, and memoized on that idempotence: a repeat call with an
    /// unchanged configuration and design point is a no-op, verified by
    /// `enforcing_an_already_enforced_design_space_is_a_no_op` below. This
    /// lets a per-frame call site (the Design Space page) skip
    /// `alas_opt::canonicalize_design`'s config clone, payload case load,
    /// and fuselage sizing except when something actually changed.
    pub fn enforce_design_space_fixed_variables(&mut self) {
        let config_fingerprint = crate::state::state_memo::config_fingerprint(&self.config_values);
        let design_fingerprint = self.enforced_design_fingerprint();
        if self.design_space_enforcement_memo == Some((config_fingerprint, design_fingerprint)) {
            return;
        }
        let Some(config) = self.typed_config() else {
            // An unreadable buffer has no fixed point to memoize; let the
            // next call re-check once the buffer is readable again.
            self.design_space_enforcement_memo = None;
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
        self.design_space_enforcement_memo =
            Some((config_fingerprint, self.enforced_design_fingerprint()));
    }

    /// The design values and the bounds together: enforcement writes both,
    /// so a change to either (bounds edited alone, say) must run it again.
    fn enforced_design_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        crate::state::state_memo::design_values_fingerprint(&self.design_values).hash(&mut hasher);
        for (name, (lower, upper)) in &self.bounds {
            name.hash(&mut hasher);
            lower.to_bits().hash(&mut hasher);
            upper.to_bits().hash(&mut hasher);
        }
        hasher.finish()
    }
}

#[cfg(test)]
mod design_space_enforcement_tests {
    use super::{AppState, BriefCommit};
    use crate::sandbox::session::ExitChoice;
    use alas_config::{AerodromeReferenceCode, AlasConfig, DesignMode, DESIGN_VARIABLE_SPECS};

    /// A C919-class brief that names no preset.
    fn brief() -> serde_json::Value {
        serde_json::json!({
            "requirements": {"mtow_kg": 72_500.0, "num_passengers": 168, "cruise_mach": 0.785},
            "geometry": {"fuselage": {"diameter_m": 3.96}}
        })
    }

    fn merge(target: &mut serde_json::Value, patch: &serde_json::Value) {
        if let (Some(target), Some(patch)) = (target.as_object_mut(), patch.as_object()) {
            for (key, value) in patch {
                match target.get_mut(key) {
                    Some(slot) if slot.is_object() && value.is_object() => merge(slot, value),
                    _ => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
    }

    /// The A320-200, New aircraft, then the brief entered and committed at
    /// a run start.
    fn committed_brief() -> AppState {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        state.set_design_mode(DesignMode::CleanSheet);
        merge(&mut state.config_values, &brief());
        state.on_config_modified();
        state.refresh_clean_sheet_brief();
        state
    }

    /// Hand values a user typed into the Design Space editor.
    fn hand_edit(state: &mut AppState) -> std::collections::BTreeMap<String, f64> {
        state.design_values.insert("span_m".to_owned(), 30.5);
        state.design_values.insert("sweep_deg".to_owned(), 24.0);
        state.design_values.clone().into_iter().collect()
    }

    fn values(state: &AppState) -> std::collections::BTreeMap<String, f64> {
        state.design_values.clone().into_iter().collect()
    }

    #[test]
    fn new_aircraft_from_a_preset_commits_the_derived_start_box_and_code() {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        let registered = state.bounds.clone();
        state.set_design_mode(DesignMode::CleanSheet);
        let config = state.typed_config().unwrap_or_default();
        assert!(config.preset.is_empty());
        assert_eq!(config.optimizer.design_space.clean_sheet_brief, Some(true));
        // The registered aircraft's code F becomes Auto, resolved to the
        // letter of the span the A320 brief implies.
        assert_eq!(
            config.optimizer.objective.aerodrome_reference_code,
            AerodromeReferenceCode::Auto
        );
        assert_eq!(config.aerodrome_reference_code(), AerodromeReferenceCode::C);
        let nominal = state.current_design().unwrap_or_default();
        let envelope = config.design_envelope(&nominal);
        for (spec, variable) in DESIGN_VARIABLE_SPECS.iter().zip(&envelope) {
            let shown = state.bounds.get(spec.name).copied().unwrap_or_default();
            assert_eq!(shown, (variable.lower, variable.upper), "{}", spec.name);
        }
        assert!(state.bounds["span_m"].1 <= 35.99 + 1e-9);
        assert_ne!(state.bounds["span_m"], registered["span_m"]);
    }

    #[test]
    fn a_changed_brief_is_rederived_when_the_run_starts_not_while_typing() {
        let mut state = committed_brief();
        let before = state.design_values["span_m"];
        if let Some(requirements) = state.group_mut("requirements") {
            requirements["mtow_kg"] = serde_json::json!(60_000.0);
        }
        state.on_config_modified();
        state.reset_design_space_bounds_to_mode();
        assert_eq!(state.design_values["span_m"], before);
        state.refresh_clean_sheet_brief();
        assert!(state.design_values["span_m"] < before);
    }

    #[test]
    fn a_design_space_edit_keeps_the_hand_values_and_contains_them() {
        let mut state = committed_brief();
        let hand = hand_edit(&mut state);
        state.reset_design_space_bounds_to_mode();
        assert_eq!(values(&state), hand);
        let (lower, upper) = state.bounds["span_m"];
        assert!(lower <= 30.5 && 30.5 <= upper);
        // An unchanged brief at the run start keeps them too.
        state.refresh_clean_sheet_brief();
        assert_eq!(values(&state), hand);
    }

    #[test]
    fn a_loaded_workspace_keeps_its_design_vector() {
        let mut state = committed_brief();
        let hand = hand_edit(&mut state);
        let document = state.workspace_document();
        let mut loaded = AppState::default();
        assert!(loaded.apply_workspace_document(&document).is_ok());
        assert_eq!(values(&loaded), hand);
        loaded.refresh_clean_sheet_brief();
        assert_eq!(values(&loaded), hand);
    }

    #[test]
    fn a_promoted_sandbox_keeps_its_design_vector() {
        let mut state = AppState::default();
        assert!(state.enter_sandbox(true));
        if let Some(requirements) = state.group_mut("requirements") {
            requirements["mtow_kg"] = serde_json::json!(300_000.0);
        }
        let hand = hand_edit(&mut state);
        assert!(state.resolve_leave_sandbox(ExitChoice::Promote));
        assert_eq!(values(&state), hand);
        state.refresh_clean_sheet_brief();
        assert_eq!(values(&state), hand);
    }

    #[test]
    fn an_explicit_letter_is_never_replaced_by_auto() {
        let mut state = committed_brief();
        if let Some(objective) = state
            .config_values
            .pointer_mut("/optimizer/objective/aerodrome_reference_code")
        {
            *objective = serde_json::json!("F");
        }
        state.on_config_modified();
        state.refresh_clean_sheet_brief();
        let config = state.typed_config().unwrap_or_default();
        assert_eq!(
            config.optimizer.objective.aerodrome_reference_code,
            AerodromeReferenceCode::F
        );
    }

    /// The keys a clean-sheet commit derives; a file that omits them asks
    /// the file path to derive them.
    const DERIVED_KEYS: &[&str] = &[
        "/optimizer/design_space/clean_sheet_brief",
        "/optimizer/objective/aerodrome_reference_code",
        "/optimizer/objective/derived_aerodrome_code",
        "/geometry/fuselage/cabin_start_x_m",
        "/geometry/fuselage/tailcone_length_m",
        "/geometry/fuselage/nose_z_m",
        "/geometry/fuselage/cabin_z_m",
        "/geometry/fuselage/tail_z_m",
        "/geometry/wing/side_of_body_span_fraction",
        "/geometry/wing/root_z_m",
        "/geometry/wing/break_z_m",
        "/geometry/wing/tip_z_m",
        "/geometry/wing/root_datum_x_m",
        "/geometry/engine/spanwise_positions_m",
        "/geometry/engine/z_m",
        "/geometry/engine/inlet_x_offset_m",
        "/geometry/empennage/hstab_root_chord_m",
        "/geometry/empennage/hstab_tip_chord_m",
        "/geometry/empennage/hstab_offset_from_tail_m",
        "/geometry/empennage/hstab_z_m",
        "/geometry/empennage/hstab_tip_le_m",
        "/geometry/empennage/vstab_root_chord_m",
        "/geometry/empennage/vstab_tip_chord_m",
        "/geometry/empennage/vstab_offset_from_tail_m",
        "/geometry/empennage/vstab_z_m",
        "/geometry/empennage/vstab_tip_le_m",
    ];

    #[test]
    fn one_brief_gives_one_configuration_in_the_file_and_the_desktop_paths() {
        // The desktop path from its default (AVE) state: New aircraft, the
        // brief entered, the run started.
        let mut state = AppState::default();
        state.set_design_mode(DesignMode::CleanSheet);
        merge(&mut state.config_values, &brief());
        state.on_config_modified();
        // The same case saved as a file without the derived keys.
        let mut file = state.config_values.clone();
        for pointer in DERIVED_KEYS {
            let (parent, key) = pointer.rsplit_once('/').unwrap_or_default();
            if let Some(object) = file.pointer_mut(parent).and_then(|v| v.as_object_mut()) {
                object.remove(key);
            }
        }
        state.refresh_clean_sheet_brief();
        let desktop = state.typed_config().unwrap_or_default();
        let file = AlasConfig::from_value(&file).unwrap_or_default();
        assert_eq!(desktop.optimizer.design_space.clean_sheet_brief, Some(true));
        assert_eq!(desktop, file);
        let start = alas_opt::configured_nominal_design(&file).unwrap_or_default();
        assert_eq!(state.current_design(), Some(start));
    }

    #[test]
    fn a_registered_preset_keeps_its_reference_envelope() {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        let config = state.typed_config().unwrap_or_default();
        assert_eq!(config.optimizer.design_space.clean_sheet_brief, None);
        assert!(!state.commit_clean_sheet_brief(config, BriefCommit::BoundsOnly));
    }

    #[test]
    fn enforcing_an_already_enforced_design_space_is_a_no_op() {
        let mut state = AppState::default();
        state.enforce_design_space_fixed_variables();
        let design_after_first = state.design_values.clone();
        let bounds_after_first = state.bounds.clone();
        let memo_after_first = state.design_space_enforcement_memo;

        // A repeat call with nothing changed must leave every value byte-for-
        // byte identical (this is what lets the Design Space page call it
        // every frame) and must not touch the memo either, which is the
        // observable proof the second call actually took the early return.
        state.enforce_design_space_fixed_variables();

        assert_eq!(state.design_values, design_after_first);
        assert_eq!(state.bounds, bounds_after_first);
        assert_eq!(state.design_space_enforcement_memo, memo_after_first);
    }

    #[test]
    fn a_design_edit_invalidates_the_enforcement_memo() {
        let mut state = AppState::default();
        state.enforce_design_space_fixed_variables();
        let memo_before = state.design_space_enforcement_memo;

        state
            .design_values
            .insert("wing_area_m2".to_owned(), 123.456);
        let fingerprint_after_edit =
            crate::state::state_memo::design_values_fingerprint(&state.design_values);
        assert_ne!(
            memo_before.map(|(_, design_fp)| design_fp),
            Some(fingerprint_after_edit),
            "the edit must change the fingerprint the memo is keyed on"
        );

        state.enforce_design_space_fixed_variables();
        assert_ne!(
            state.design_space_enforcement_memo, memo_before,
            "a changed design point must recompute, not reuse the stale memo"
        );
    }

    #[test]
    fn a_bounds_edit_invalidates_the_enforcement_memo() {
        let mut state = AppState::default();
        state.enforce_design_space_fixed_variables();
        let memo_before = state.design_space_enforcement_memo;

        state.bounds.insert("wing_area_m2".to_owned(), (1.0, 2.0));

        state.enforce_design_space_fixed_variables();
        assert_ne!(
            state.design_space_enforcement_memo, memo_before,
            "enforcement writes bounds, so a bounds edit must run it again"
        );
    }

    #[test]
    fn a_config_edit_invalidates_the_enforcement_memo() {
        let mut state = AppState::default();
        state.enforce_design_space_fixed_variables();
        let memo_before = state.design_space_enforcement_memo;

        state.config_values["requirements"]["mtow_kg"] = serde_json::json!(
            state.config_values["requirements"]["mtow_kg"]
                .as_f64()
                .unwrap_or(1000.0)
                + 250.0
        );

        state.enforce_design_space_fixed_variables();
        assert_ne!(
            state.design_space_enforcement_memo, memo_before,
            "a changed configuration must recompute, not reuse the stale memo"
        );
    }
}
