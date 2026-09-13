// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::preset_flops::inputs_for;

    #[test]
    fn registered_jet_presets_have_a_complete_transport_contract() {
        for name in [
            "AVE", "A340-300", "A380-800", "B787-9", "A320-200", "A220-300", "DC-10",
        ] {
            let inputs = inputs_for(name).unwrap_or_else(|| panic!("{name} FLOPS inputs"));
            assert!(inputs.transport.maximum_mach.unwrap_or_default() > 0.0);
            assert!(inputs.transport.design_range_nmi.unwrap_or_default() > 0.0);
            assert!(inputs.transport.provenance.is_complete(), "{name}");
            assert!(inputs.structure.validate().is_ok(), "{name}");
        }
    }

    #[test]
    fn atr_contract_is_present_but_does_not_change_the_unsupported_domain() {
        let inputs = inputs_for("ATR72-600").expect("ATR FLOPS contract");
        assert_eq!(inputs.transport.fuselage_mounted_engine_count, Some(0));
        assert_eq!(inputs.transport.maximum_mach, Some(0.55));
    }

    #[test]
    fn mounting_counts_match_each_registered_geometry() {
        for name in [
            "AVE",
            "A340-300",
            "A380-800",
            "B787-9",
            "A320-200",
            "A220-300",
            "ATR72-600",
            "DC-10",
        ] {
            let preset = crate::presets::get(name).expect("preset");
            let inputs = inputs_for(name).expect("FLOPS inputs");
            let total = inputs
                .transport
                .wing_mounted_engine_count
                .unwrap_or_default()
                + inputs
                    .transport
                    .fuselage_mounted_engine_count
                    .unwrap_or_default();
            assert_eq!(total, preset.n_engines, "{name}");
        }
    }

    #[test]
    fn source_matched_cabin_layouts_are_used_without_resizing_other_cases() {
        let a320 = inputs_for("A320-200").expect("A320 FLOPS inputs").transport;
        assert_eq!(a320.first_class_passenger_count, Some(12));
        assert_eq!(a320.business_class_passenger_count, Some(0));
        assert_eq!(a320.tourist_class_passenger_count, Some(138));
        assert_eq!(a320.flight_attendant_count, Some(4));

        let b787 = inputs_for("B787-9").expect("B787 FLOPS inputs").transport;
        assert_eq!(b787.first_class_passenger_count, Some(0));
        assert_eq!(b787.business_class_passenger_count, Some(28));
        assert_eq!(b787.tourist_class_passenger_count, Some(262));

        // Published A340 and A380 layouts have different passenger totals;
        // they remain explicit all-economy study cabins until a matching
        // installed configuration is sourced.
        let a340 = inputs_for("A340-300").expect("A340 FLOPS inputs").transport;
        assert_eq!(a340.first_class_passenger_count, Some(0));
        assert_eq!(a340.business_class_passenger_count, Some(0));
        assert_eq!(a340.tourist_class_passenger_count, Some(290));
    }
}
