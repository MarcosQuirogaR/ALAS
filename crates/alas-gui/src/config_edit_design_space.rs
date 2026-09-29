// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Enforcing the typed design-space envelope's fixed variables.
//!
//! Split out of [`crate::config_edit`] to keep that module under the
//! project's line limit; this is a single `impl AppState` method.

use crate::state::AppState;

impl AppState {
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
    use super::AppState;

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
