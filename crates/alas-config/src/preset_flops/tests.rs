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
    fn atr_hotel_mode_declares_the_apu_absence_without_changing_jet_defaults() {
        let atr = inputs_for("ATR72-600").expect("ATR FLOPS contract");
        assert!(!atr.transport.apu_installed);
        assert!(atr
            .transport
            .provenance
            .architecture
            .uncertainty
            .contains("APU absent by ATR hotel-mode architecture"));

        let a320 = inputs_for("A320-200").expect("A320 FLOPS contract");
        assert!(a320.transport.apu_installed);
    }

    #[test]
    fn certified_engine_scope_does_not_suppress_an_unresolved_starter_system() {
        use crate::{FlopsNozzleScope, FlopsStarterScope};
        let a320 = inputs_for("A320-200").expect("A320 FLOPS contract");
        assert_eq!(
            a320.structure.starter_scope,
            FlopsStarterScope::HardwareIncludedSystemUnresolved
        );
        let a220 = inputs_for("A220-300").expect("A220 FLOPS contract");
        assert_eq!(
            a220.structure.starter_scope,
            FlopsStarterScope::UnknownConservativeSeparate
        );
        let a380 = inputs_for("A380-800").expect("A380 FLOPS contract");
        assert_eq!(
            a320.structure.nozzle_scope,
            FlopsNozzleScope::OutsideUnmodelled
        );
        assert_eq!(
            a220.structure.nozzle_scope,
            FlopsNozzleScope::OutsideUnmodelled
        );
        assert_eq!(
            a380.structure.nozzle_scope,
            FlopsNozzleScope::OutsideUnmodelled
        );
    }

    /// The unit-load-device tare of FLOPS equations 125-126 is hardware, so
    /// it belongs only to the aircraft whose holds take a container. The three
    /// bulk-loaded types must declare that, and none may be given a mixed
    /// arrangement without the share a mixed one needs.
    #[test]
    fn each_registered_aircraft_declares_how_its_holds_are_loaded() {
        use crate::CargoHoldLoading;
        for (name, expected) in [
            // ATR 72-600 factsheet p.22: main-deck, loose-loaded, no lower
            // hold. A220-300 operator WBM 1.6: "ULDs are not used". A320-200:
            // the cargo loading system is an option/STC, not the delivered
            // WV017 arrangement.
            ("ATR72-600", CargoHoldLoading::Bulk),
            ("A220-300", CargoHoldLoading::Bulk),
            ("A320-200", CargoHoldLoading::Bulk),
            ("A340-300", CargoHoldLoading::Containerized),
            ("A380-800", CargoHoldLoading::Containerized),
            ("B787-9", CargoHoldLoading::Containerized),
            ("DC-10", CargoHoldLoading::Containerized),
            ("AVE", CargoHoldLoading::Containerized),
        ] {
            let inputs = inputs_for(name).unwrap_or_else(|| panic!("{name} FLOPS inputs"));
            assert_eq!(inputs.transport.cargo_loading, Some(expected), "{name}");
            // Nothing declared as bulk or containerised may also carry a
            // partial share; that field belongs to the mixed case alone.
            assert_eq!(
                inputs.transport.containerized_baggage_fraction, None,
                "{name}"
            );
        }
    }

    /// The hold architecture is one physical fact and must have one
    /// declaration. It used to have two: the FLOPS container tare read
    /// `declared_cargo_loading`, the cabin layout engine read
    /// `cabin.cargo.lower_deck_uld`, and they disagreed on the ATR 72-600 and
    /// the A320-200 - the ATR, which has no lower hold at all, was still being
    /// offered LD3 positions in one.
    #[test]
    fn the_hold_architecture_has_one_declaration_that_both_consumers_agree_on() {
        use crate::CargoHoldLoading;
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
            let loading = inputs_for(name)
                .unwrap_or_else(|| panic!("{name} FLOPS inputs"))
                .transport
                .cargo_loading
                .unwrap_or_else(|| panic!("{name} declares its hold loading"));
            let uld = preset.planning_cabin_config().cargo.lower_deck_uld;
            match loading {
                CargoHoldLoading::Bulk => assert_eq!(uld, "BLK", "{name}"),
                CargoHoldLoading::Containerized | CargoHoldLoading::Mixed => {
                    assert_ne!(uld, "BLK", "{name}");
                }
            }
        }
    }

    /// The LTH relations' stated domain is a civil transport of **at least**
    /// 40 t maximum takeoff mass **or at least** 70 passenger seats, and the
    /// selection applies both clauses. The ATR 72-600 is admitted by the seat
    /// clause at 72 seats even though it is far below the mass clause, and it
    /// is the one aircraft the method makes worse - which is exactly why this
    /// test exists: the threshold must not be trimmed back to the mass clause
    /// to recover that aircraft's number.
    #[test]
    fn the_cabin_equipment_domain_rule_is_the_whole_published_statement() {
        use crate::CabinEquipmentMethod;
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
            let inputs = inputs_for(name).unwrap_or_else(|| panic!("{name} FLOPS inputs"));
            assert_eq!(
                inputs.transport.cabin_equipment_method,
                CabinEquipmentMethod::LthCivilTransportV1,
                "{name} is inside the LTH domain by mass, by seats, or by both"
            );
        }
        // Negative controls on the rule itself, so "every preset takes LTH"
        // cannot be read as the rule doing nothing.
        assert_eq!(
            CabinEquipmentMethod::for_civil_transport_size(Some(23_000.0), Some(72)),
            CabinEquipmentMethod::LthCivilTransportV1,
            "the ATR 72-600's 72 seats admit it through the seat clause"
        );
        assert_eq!(
            CabinEquipmentMethod::for_civil_transport_size(Some(23_000.0), Some(69)),
            CabinEquipmentMethod::FlopsTransportV1
        );
        assert_eq!(
            CabinEquipmentMethod::for_civil_transport_size(Some(40_000.0), Some(50)),
            CabinEquipmentMethod::LthCivilTransportV1,
            "the mass clause is 'mindestens 40 Tonnen', inclusive"
        );
        assert_eq!(
            CabinEquipmentMethod::for_civil_transport_size(None, None),
            CabinEquipmentMethod::FlopsTransportV1,
            "with neither clause evaluable the published FLOPS baseline stands"
        );
    }

    /// The cabin-equipment domain rule and the FLOPS class split must be
    /// decided on **one** seat number: the occupancy the mass is computed on.
    ///
    /// This also records, rather than hides, the one aircraft where the
    /// published cabin and the computed cabin differ - the A380-800, 555
    /// published against 525 computed. The selection is identical under either
    /// reading, so the consolidation moves no mass; the divergence itself is a
    /// configuration mismatch owned by the cabin layout.
    #[test]
    fn the_cabin_method_and_the_class_split_are_decided_on_the_same_seat_count() {
        use crate::CabinEquipmentMethod;
        let mut divergent = Vec::new();
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
            let transport = inputs_for(name)
                .unwrap_or_else(|| panic!("{name} FLOPS inputs"))
                .transport;
            let split = transport.first_class_passenger_count.unwrap_or_default()
                + transport.business_class_passenger_count.unwrap_or_default()
                + transport.tourist_class_passenger_count.unwrap_or_default();
            let computed = preset.requirements.num_passengers;
            assert_eq!(
                i64::try_from(split).expect("seat count fits"),
                computed,
                "{name}: the FLOPS class split must total the computed occupancy"
            );

            let mtom_kg = preset
                .reference
                .mtow_kg
                .unwrap_or(preset.requirements.mtow_kg);
            let published = preset.reference.planning_seats.unwrap_or(computed);
            if published != computed {
                divergent.push((name, published, computed));
            }
            // Whichever seat number is read, the method must be the same, so
            // the consolidation above cannot have moved any mass.
            assert_eq!(
                CabinEquipmentMethod::for_civil_transport_size(Some(mtom_kg), Some(computed)),
                CabinEquipmentMethod::for_civil_transport_size(Some(mtom_kg), Some(published)),
                "{name}"
            );
            assert_eq!(
                transport.cabin_equipment_method,
                CabinEquipmentMethod::for_civil_transport_size(Some(mtom_kg), Some(computed)),
                "{name}"
            );
        }
        // Pinned so a future cabin change that silently resolves - or silently
        // introduces - a published/computed divergence has to say so here.
        //
        // Every registered aircraft whose reference OEW belongs to a published
        // cabin seats **fewer** passengers in the model than in that cabin, and
        // always in the same direction. Under the LTH method the seat count
        // reaches exactly one mass term, `m_opp`, so the effect is an exact
        // single-term propagation rather than an estimate:
        //
        // | preset | published | computed | `m_opp` kg | at published kg | delta |
        // |---|---:|---:|---:|---:|---:|
        // | A340-300 | 335 | 290 | 19,816.3 | 23,271.1 | **+3,454.8** |
        // | A380-800 | 555 | 525 | 38,387.9 | 40,839.6 | **+2,451.7** |
        // | A220-300 | 140 | 130 |  4,738.3 |  5,110.8 |   +372.4 |
        // | DC-10-30 | 255 | 250 | 16,796.1 | 17,170.8 |   +374.7 |
        //
        // That is a **configuration mismatch, not a model error**, and it is
        // deliberately not closed by editing the FLOPS seat input: doing so
        // would price a cabin the rest of the product does not fly. It is
        // recorded here because it means a real share of the remaining
        // operating-empty deficits is a cabin difference rather than a mass
        // method being wrong.
        assert_eq!(
            divergent,
            vec![
                ("A340-300", 335, 290),
                ("A380-800", 555, 525),
                ("A220-300", 140, 130),
                ("DC-10", 255, 250),
            ],
            "published-versus-computed cabin divergence"
        );
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
