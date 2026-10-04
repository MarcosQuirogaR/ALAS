// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Dispatch-time preset barrier.
//!
//! In preset mode the run leaves the GUI with the registered preset's locked
//! geometry and initial design point, and with optimizer bounds anchored to
//! the preset reference envelope rather than to whatever the Design Space
//! editor currently holds. Anything restored or re-anchored is logged so a
//! bypassed buffer is visible instead of surfacing as a pipeline rejection.

use alas_config::{preset_policy, presets, AlasConfig, DesignVector, DESIGN_VARIABLE_SPECS};
use alas_opt::DesignOptimizer;

use crate::state::{AppState, LogKind};
use crate::views::tr_fields;

impl AppState {
    /// Restore locked preset fields on the dispatched configuration and
    /// design, anchor the bounds to the preset envelope, and mirror the
    /// anchored bounds into the Design Space editor.
    pub(crate) fn apply_preset_dispatch_policy(
        &mut self,
        config: &mut AlasConfig,
        design: &mut DesignVector,
        bounds: &mut Vec<(f64, f64)>,
    ) {
        if !preset_policy::preset_mode_active(config) {
            return;
        }
        let restored = preset_policy::restore_locked_fields(config, design);
        if !restored.is_empty() {
            self.enforce_preset_geometry();
            self.log(
                tr_fields(
                    "Preset protection restored {count} locked value(s) before dispatch: {list}",
                    &[
                        ("count", restored.len().to_string()),
                        ("list", preset_policy::describe_violations(&restored)),
                    ],
                ),
                LogKind::Warn,
            );
        }
        let Ok(preset) = presets::get(&config.preset) else {
            return;
        };
        // The optimizer owns the envelope derivation; asking it with no bounds
        // returns the box a headless run searches for this preset.
        let anchored = match DesignOptimizer::new(config.clone())
            .resolved_bounds(None, Some(&preset.design_vector))
        {
            Ok(anchored) => anchored,
            Err(error) => {
                self.log(
                    tr_fields(
                        "Preset bound anchoring was skipped ({error}); the run uses the Design Space bounds as entered.",
                        &[("error", error.to_string())],
                    ),
                    LogKind::Warn,
                );
                return;
            }
        };
        let differing = anchored
            .iter()
            .zip(bounds.iter())
            .filter(|(anchor, current)| anchor != current)
            .count();
        if differing > 0 {
            self.log(
                tr_fields(
                    "Optimizer bounds anchored to the preset reference envelope; {count} bound(s) differed from the Design Space values.",
                    &[("count", differing.to_string())],
                ),
                LogKind::Warn,
            );
            for (spec, bound) in DESIGN_VARIABLE_SPECS.iter().zip(anchored.iter()) {
                self.bounds.insert(spec.name.to_owned(), *bound);
            }
            *bounds = anchored;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::state::AppState;
    use alas_config::preset_policy;

    #[test]
    fn a_bypassed_preset_geometry_and_widened_bounds_are_restored_before_dispatch() {
        let mut state = AppState::default();
        let preset_key = state
            .preset_names
            .first()
            .map(|(key, _)| key.clone())
            .expect("a registered preset");
        state.load_preset(&preset_key);
        state.set_design_mode(alas_config::DesignMode::ReferenceAdaptation);
        let mut config = state.typed_config().expect("valid config");
        assert!(preset_policy::preset_mode_active(&config));
        let mut design = state.current_design().expect("design");
        let mut bounds = state.current_design_bounds().expect("bounds");
        config.geometry.fuselage.diameter_m += 1.0;
        design.span_m += 5.0;
        bounds[0] = (bounds[0].0 - 100.0, bounds[0].1 + 100.0);
        state.apply_preset_dispatch_policy(&mut config, &mut design, &mut bounds);
        assert!(
            preset_policy::dispatch_violations(&config, Some(&design), Some(&bounds))
                .expect("registered preset")
                .is_empty()
        );
        assert!(state
            .logs
            .iter()
            .any(|line| line.text.contains("Preset protection restored")));
        assert!(state.logs.iter().any(|line| line
            .text
            .contains("anchored to the preset reference envelope")));
    }

    #[test]
    fn a_failed_bound_derivation_is_logged_instead_of_silently_skipped() {
        let mut state = AppState::default();
        let preset_key = state
            .preset_names
            .first()
            .map(|(key, _)| key.clone())
            .expect("a registered preset");
        state.load_preset(&preset_key);
        state.set_design_mode(alas_config::DesignMode::ReferenceAdaptation);
        let mut config = state.typed_config().expect("valid config");
        let mut design = state.current_design().expect("design");
        let mut bounds = state.current_design_bounds().expect("bounds");
        config.mass_model.mass_architecture =
            alas_config::MassArchitecture::LegacyReferenceCompatibleComparison;
        let logged_before = state.logs.len();
        state.apply_preset_dispatch_policy(&mut config, &mut design, &mut bounds);
        assert!(
            state.logs[logged_before..]
                .iter()
                .any(|line| line.text.contains("anchoring was skipped")
                    && line.text.contains("pure_flops_transport_v1")),
            "the skipped anchoring must be visible"
        );
    }

    #[test]
    fn a_clean_sheet_run_is_left_untouched() {
        let mut state = AppState::default();
        state.set_design_mode(alas_config::DesignMode::CleanSheet);
        let mut config = state.typed_config().expect("valid config");
        let mut design = state.current_design().expect("design");
        let mut bounds = state.current_design_bounds().expect("bounds");
        let before = (config.clone(), design, bounds.clone());
        state.apply_preset_dispatch_policy(&mut config, &mut design, &mut bounds);
        assert_eq!((config, design, bounds), before);
    }
}
