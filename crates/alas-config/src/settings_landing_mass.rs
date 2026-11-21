// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The landing-mass limit of a configuration, by design mode.

use super::{is_valid_declared_mass_kg, AlasConfig, MassModelConfig};

impl AlasConfig {
    /// The maximum landing mass to enforce for `candidate_mtow_kg`.
    ///
    /// In [`crate::optimizer::DesignMode::BaselineSandbox`], a valid declared
    /// reference MLW (the registered preset's own certified limit) governs
    /// as a fixed aircraft limit and does not scale with `candidate_mtow_kg`:
    /// BaselineSandbox replays that certified airframe unchanged, so its
    /// certified MLW does not move because a candidate MTOW does.
    /// [`crate::optimizer::DesignMode::ReferenceAdaptation`] applies the same
    /// declared value, but there it is a *reference-aircraft* limit rather
    /// than a certification of the adapted design: that mode may move some
    /// design variables inside configured windows, so the aircraft actually
    /// being evaluated is no longer necessarily the certified article the MLW
    /// was published for. Using the reference value there models "hold to the
    /// envelope of the aircraft this design is adapted from," not "this
    /// modified design is certified to that mass." In
    /// [`crate::optimizer::DesignMode::CleanSheet`], where there is no
    /// reference airframe to anchor to at all, the configured mass-model
    /// fraction of `candidate_mtow_kg` is used for sizing instead. A preset
    /// with no declared MLW, or no preset at all, falls back to the fraction
    /// in every mode; nothing here invents a value the registry does not
    /// carry.
    ///
    /// An explicit requirement, `requirements.mlw_kg` (the certified MLW of
    /// the variant flown, such as a freighter flown on its passenger donor's
    /// preset), is checked ahead of the design-mode branch and governs in
    /// every mode, clipped to `candidate_mtow_kg` like the declared value; it
    /// is not folded into either fallback.
    pub fn landing_mass_limit_kg(&self, candidate_mtow_kg: f64) -> f64 {
        use crate::optimizer::DesignMode;

        if let Some(required_kg) = self
            .requirements
            .mlw_kg
            .filter(|m| is_valid_declared_mass_kg(*m))
        {
            return crate::landing_mass_ratio::clip_to_takeoff_mass(required_kg, candidate_mtow_kg);
        }
        let fraction_limit_kg = candidate_mtow_kg * self.mass_model.mlw_fraction_mtow;
        match self.optimizer.design_space.mode {
            DesignMode::CleanSheet => fraction_limit_kg,
            DesignMode::BaselineSandbox | DesignMode::ReferenceAdaptation => {
                let declared_kg = self
                    .declared_reference_mlw_kg()
                    .unwrap_or(fraction_limit_kg);
                crate::landing_mass_ratio::clip_to_takeoff_mass(declared_kg, candidate_mtow_kg)
            }
        }
    }

    /// The registered preset's own declared MLW, if it names one and it is a
    /// finite positive mass.
    fn declared_reference_mlw_kg(&self) -> Option<f64> {
        let mlw_kg = crate::presets::get(&self.preset).ok()?.reference.mlw_kg?;
        is_valid_declared_mass_kg(mlw_kg).then_some(mlw_kg)
    }

    /// This configuration's saved mass model, with `mlw_fraction_mtow`
    /// replaced by the ratio that reproduces [`Self::landing_mass_limit_kg`]
    /// for `candidate_mtow_kg`.
    ///
    /// Mass APIs below `AlasConfig` (e.g.
    /// `run_mass_analysis_with_model_checked_product_with_gear`) take a
    /// [`MassModelConfig`] and read a landing-mass fraction straight out of
    /// it; they have no `DesignMode`/preset identity to resolve a reference
    /// MLW themselves. `mlw_fraction_mtow` is this method's only way to carry
    /// [`Self::landing_mass_limit_kg`]'s mode-aware result through that
    /// existing slot; it is not a new scalable design fraction. In
    /// BaselineSandbox/ReferenceAdaptation it is a fixed reference MLW
    /// divided by whatever `candidate_mtow_kg` is, so it changes if
    /// `candidate_mtow_kg` does and must be recomputed per call rather than
    /// cached. Every other field, and the caller's own saved configuration,
    /// is returned unchanged.
    ///
    /// For a non-finite or non-positive `candidate_mtow_kg` the configured
    /// fraction is left as-is: no ratio can be formed from it, and inventing
    /// one would manufacture a finite result for an input the caller's own
    /// design-requirements validation should already have rejected before it
    /// reaches a mass call.
    pub fn analysis_mass_model(&self, candidate_mtow_kg: f64) -> MassModelConfig {
        let mut model = self.mass_model.clone();
        if is_valid_declared_mass_kg(candidate_mtow_kg) {
            model.mlw_fraction_mtow =
                self.landing_mass_limit_kg(candidate_mtow_kg) / candidate_mtow_kg;
        }
        model
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use crate::optimizer::DesignMode;
    use crate::AlasConfig;
    use serde_json::json;

    const MODES: [DesignMode; 3] = [
        DesignMode::BaselineSandbox,
        DesignMode::ReferenceAdaptation,
        DesignMode::CleanSheet,
    ];

    #[test]
    fn an_explicit_mlw_requirement_governs_in_every_design_mode() {
        let mut config = AlasConfig::from_value(&json!({"preset": "ATR72-600"})).unwrap();
        config.requirements.mlw_kg = Some(21_850.0);
        for mode in MODES {
            config.optimizer.design_space.mode = mode;
            // Ahead of the declared preset MLW (22,350 kg) and the fraction.
            assert_eq!(config.landing_mass_limit_kg(23_000.0), 21_850.0, "{mode:?}");
            // Never above the candidate's take-off mass.
            assert_eq!(config.landing_mass_limit_kg(20_000.0), 20_000.0, "{mode:?}");
            // The mass model's ratio reproduces the same limit.
            let model = config.analysis_mass_model(23_000.0);
            assert!((model.mlw_fraction_mtow * 23_000.0 - 21_850.0).abs() < 1e-9);
        }
    }

    #[test]
    fn an_invalid_or_unset_mlw_requirement_leaves_the_mode_rule() {
        let mut config = AlasConfig::from_value(&json!({"preset": "ATR72-600"})).unwrap();
        config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
        for value in [None, Some(f64::NAN), Some(0.0), Some(-1.0)] {
            config.requirements.mlw_kg = value;
            assert_eq!(
                config.landing_mass_limit_kg(23_000.0),
                22_350.0,
                "{value:?}"
            );
        }
    }

    #[test]
    fn an_unset_mlw_requirement_is_not_saved_and_a_set_one_round_trips() {
        let mut config = AlasConfig::default();
        let saved = serde_json::to_value(&config).unwrap();
        assert!(saved["requirements"].get("mlw_kg").is_none());
        config.requirements.mlw_kg = Some(302_092.0);
        let saved = serde_json::to_value(&config).unwrap();
        assert_eq!(saved["requirements"]["mlw_kg"], json!(302_092.0));
        let loaded = AlasConfig::from_value(&saved).unwrap();
        assert_eq!(loaded.requirements.mlw_kg, Some(302_092.0));
    }
}
