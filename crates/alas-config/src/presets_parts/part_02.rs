// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// The named high-lift technology level a preset is scored with.
///
/// Every preset states one, so a `None` here is a name that does not exist
/// rather than a type happy with the generic default; the registry's own
/// tests are what catch that, since falling back silently would revert a
/// widebody to a narrowbody's flaps and its V-speeds with them.
fn high_lift(name: &'static str) -> Option<PerformanceConfig> {
    match crate::performance_presets::get(name) {
        Ok(preset) => Some(preset.settings.clone()),
        Err(error) => {
            tracing::error!(%error, "an aircraft preset names an unregistered high-lift level");
            None
        }
    }
}

fn sorted_names() -> Vec<String> {
    let mut names: Vec<String> = registry()
        .iter()
        .map(|preset| preset.name.to_owned())
        .collect();
    names.sort();
    names
}

fn build() -> Vec<AircraftPreset> {
    let mut presets = vec![
        reference::ave(),
        widebody::a340_300(),
        widebody::a380_800(),
        widebody::b787_9(),
        narrowbody::a320_200(),
        narrowbody::a220_300(),
        regional::atr72_600(),
        widebody::dc_10(),
    ];

    // The engine name is stated on the preset and read off the geometry, and
    // two of the seven state it in both places. Copying it down here is what
    // makes the two agree for the other five, and is upstream's `__post_init__`.
    for preset in &mut presets {
        preset.geometry.engine.engine_name = preset.engine_name.to_owned();
    }
    presets
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dropdown_lists_the_aircraft_in_registration_order() {
        assert_eq!(
            available(),
            vec![
                "AVE",
                "A340-300",
                "A380-800",
                "B787-9",
                "A320-200",
                "A220-300",
                "ATR72-600",
                "DC-10",
            ]
        );
    }

    #[test]
    fn an_unknown_aircraft_is_an_error_that_says_what_there_is() {
        let error = get("Concorde").unwrap_err();
        assert!(error.available.contains(&"A320-200".to_owned()));
        assert!(error.to_string().contains("A320-200"));
    }

    #[test]
    fn every_preset_carries_its_engine_name_into_its_geometry() {
        // Five of the seven state it only on the preset, and everything
        // downstream reads it off the geometry.
        for preset in registry() {
            assert_eq!(
                preset.geometry.engine.engine_name, preset.engine_name,
                "{}",
                preset.name
            );
        }
    }

    #[test]
    fn every_operational_default_uses_two_registered_airports() {
        for preset in registry() {
            let defaults = preset.operational_mission_defaults();
            assert_ne!(
                defaults.departure_airport, defaults.arrival_airport,
                "{}",
                preset.name
            );
            assert!(
                crate::airports::get(defaults.departure_airport).is_ok(),
                "{}: unknown departure {}",
                preset.name,
                defaults.departure_airport
            );
            assert!(
                crate::airports::get(defaults.arrival_airport).is_ok(),
                "{}: unknown arrival {}",
                preset.name,
                defaults.arrival_airport
            );
            assert!(defaults.profile.cruise_1_air_speed_m_s.is_finite());
            assert!(!defaults.provenance.is_empty());
        }
    }

    #[test]
    fn a_preset_mounts_exactly_as_many_engines_as_it_claims_to_have() {
        for preset in registry() {
            assert_eq!(
                preset.engine_spanwise_positions().len(),
                preset.n_engines,
                "{}",
                preset.name
            );
        }
    }

    #[test]
    fn multi_bogie_aircraft_do_not_fall_back_to_the_two_leg_weight_threshold() {
        assert_eq!(get("A340-300").unwrap().landing_gear.n_mlg_struts, 3);
        assert_eq!(get("A380-800").unwrap().landing_gear.n_mlg_struts, 4);
        assert_eq!(get("DC-10").unwrap().landing_gear.n_mlg_struts, 3);
    }

    #[test]
    fn a_preset_still_carries_the_fallback_cycle_until_configuration_loading() {
        let a320 = get("A320-200").unwrap();
        assert_eq!(a320.geometry.engine.thrust_kn, 467.0);
    }

    #[test]
    fn every_engine_the_presets_name_is_one_the_table_carries() {
        // The name is a selector, and one the table does not carry leaves the
        // GE9X fallback in place for good -- a silent widebody engine on
        // whatever type mistyped it. Every technology now has a typed binding;
        // no preset may rely on a different engine's fallback physics.
        for preset in registry() {
            assert!(
                crate::engines::get(preset.engine_name).is_ok(),
                "{}: no engine called {}",
                preset.name,
                preset.engine_name
            );
        }
    }

    #[test]
    fn every_real_preset_names_one_coherent_weight_variant() {
        for preset in registry().iter().filter(|preset| preset.name != "AVE") {
            assert!(!preset.identity.model.is_empty(), "{}", preset.name);
            assert!(
                !preset.identity.weight_variant.is_empty(),
                "{}",
                preset.name
            );
            assert!(!preset.identity.engine_model.is_empty(), "{}", preset.name);
            assert!(!preset.reference.sources.is_empty(), "{}", preset.name);
            assert_eq!(
                preset.reference.mtow_kg,
                Some(preset.requirements.mtow_kg),
                "{} mixes its public run weight with another weight variant",
                preset.name
            );
        }
    }

    #[test]
    fn every_passenger_preset_uses_candidate_geometry_for_capacity() {
        for preset in registry().iter().filter(|preset| preset.name != "AVE") {
            assert_eq!(
                preset.requirements.cabin_preset, "Custom",
                "{} has no sourced operator class layout",
                preset.name
            );
            assert!(
                preset.requirements.optimize_passenger_capacity,
                "{}",
                preset.name
            );
        }

        let ave = get("AVE").unwrap();
        assert!(ave.requirements.optimize_passenger_capacity);
    }

    #[test]
    fn every_preset_leaves_design_mission_compliance_unverified_without_a_source() {
        for preset in registry() {
            assert_eq!(
                preset.reference.design_mission_evidence,
                DesignMissionEvidence::Unverified,
                "{} must not inherit the application's default route as evidence",
                preset.name
            );
        }
    }

    #[test]
    fn only_public_planning_evidence_carries_a_planning_curve() {
        for preset in registry() {
            match preset.reference.cg_evidence {
                CgEnvelopeEvidence::PublicPlanning => assert!(
                    preset.reference.planning_cg_envelope.is_some(),
                    "{}",
                    preset.name
                ),
                CgEnvelopeEvidence::AfmRequired => assert!(
                    preset.reference.planning_cg_envelope.is_none(),
                    "{} must not invent limits the AFM/WBM owns",
                    preset.name
                ),
                CgEnvelopeEvidence::DesignRequirement | CgEnvelopeEvidence::Unknown => {}
            }
        }
    }

    #[test]
    fn only_the_reference_twin_fits_inside_the_unnarrowed_design_space() {
        // The design space's bounds describe AVE's family and nothing else, so
        // a real type of any other size starts outside them. Stated as a test
        // because it looks like a defect and is not: whoever runs a search
        // narrows the bounds around the design it starts from.
        let inside = |preset: &AircraftPreset| {
            preset
                .design_vector
                .to_array()
                .iter()
                .zip(crate::DESIGN_VARIABLE_SPECS)
                .all(|(value, spec)| *value >= spec.lower && *value <= spec.upper)
        };
        let fitting: Vec<&str> = registry()
            .iter()
            .filter(|preset| inside(preset))
            .map(|preset| preset.name)
            .collect();
        assert_eq!(fitting, vec!["AVE"]);
    }

    #[test]
    fn every_preset_baseline_lies_inside_the_cross_preset_guardrails() {
        // `lower`/`upper` define the AVE clean-sheet family. Preset-centred
        // redesigns need broader safety guardrails so an A320, regional jet,
        // or widebody remains centred on its own baseline instead of being
        // projected into the AVE family before optimization starts.
        for preset in registry() {
            for (value, spec) in preset
                .design_vector
                .to_array()
                .iter()
                .zip(crate::DESIGN_VARIABLE_SPECS)
            {
                assert!(
                    *value >= spec.preset_lower && *value <= spec.preset_upper,
                    "{} {}={} lies outside cross-preset guardrails [{}, {}]",
                    preset.name,
                    spec.name,
                    value,
                    spec.preset_lower,
                    spec.preset_upper
                );
            }
        }
    }

    #[test]
    fn every_preset_supports_an_unclipped_default_ten_percent_local_sweep() {
        // Guardrails are safety envelopes, not the requested search width.
        // Keep the default preset study symmetric around the exact baseline;
        // otherwise clipping silently moves the midpoint and changes the
        // user's requested +10% side into a smaller perturbation.
        for preset in registry() {
            for (value, spec) in preset
                .design_vector
                .to_array()
                .iter()
                .zip(crate::DESIGN_VARIABLE_SPECS)
            {
                let reference = if value.abs() < f64::EPSILON {
                    spec.preset_local_scale
                } else {
                    value.abs()
                };
                let lower = value - 0.10 * reference;
                let upper = value + 0.10 * reference;
                assert!(
                    lower >= spec.preset_lower && upper <= spec.preset_upper,
                    "{} {} raw +/-10% interval [{lower}, {upper}] exceeds guardrails [{}, {}]",
                    preset.name,
                    spec.name,
                    spec.preset_lower,
                    spec.preset_upper
                );
            }
        }
    }

    #[test]
    fn only_the_types_the_global_assumptions_miss_carry_their_own_calibration() {
        // A calibration on every preset would mean the defaults describe
        // nothing; one on none of them would mean the small narrowbody comes
        // out several tonnes light.
        let calibrated: Vec<&str> = registry()
            .iter()
            .filter(|preset| preset.mass_model.is_some())
            .map(|preset| preset.name)
            .collect();
        assert_eq!(calibrated, vec!["A220-300"]);
    }

    #[test]
    fn every_preset_states_a_high_lift_system_rather_than_inheriting_one() {
        // The default performance configuration is a generic narrowbody, and
        // it fits none of these seven well enough to leave unstated.
        for preset in registry() {
            assert!(preset.performance.is_some(), "{}", preset.name);
        }
    }
}

