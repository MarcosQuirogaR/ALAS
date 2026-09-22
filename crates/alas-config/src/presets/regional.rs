// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Regional transport presets.
//!
//! The ATR entry records `PW127M` as an identity and binds it to the typed
//! free-turbine/gearbox/568F-1 payload. Legacy flat turbofan fields remain in
//! [`crate::EngineConfig`] only as a migration mirror and are not active for
//! this preset.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MissingDesignMissionDatum, MissionEvidenceApplicability,
    PartialDesignMissionEvidence, PartialMissionEvidenceKind, PublishedRange, WingConfig,
};

/// ATR 72-212A marketed as the ATR 72-600, in the 23,000 kg configuration.
pub fn atr72_600() -> AircraftPreset {
    let mut engine = EngineConfig {
        engine_name: "PW127M".to_owned(),
        ..EngineConfig::default()
    };
    // Materialize the technology binding before overriding installation
    // geometry. The embedded catalogue is the authoritative source for the
    // PW127M/568F power ratings; the preset owns where that system is mounted.
    engine.apply_engine_spec();
    engine.nacelle_profile = vec![
        (0.0, 0.35),
        (0.35, 0.9),
        (0.8, 1.0),
        (2.4, 0.8),
        (3.0, 0.35),
    ];
    engine.radius_scale_m = 0.65;
    // ATR 72-600 factsheet 2020 p.22 states an 8.10 m propeller-axis
    // separation; half of that is the single-engine spanwise station.
    // Physics review v1.2, section 4.3: the previous 4.25 m was 0.20 m
    // off-source.
    engine.spanwise_positions_m = vec![4.05, -4.05];
    engine.z_m = -0.70;
    engine.inlet_x_offset_m = 1.2;

    AircraftPreset {
        name: "ATR72-600",
        display_name: "ATR 72-600",
        description: "ATR 72-212A (ATR 72-600), 23 t variant, with PW127M engines and 568F-1 propellers.",
        identity: AircraftVariantIdentity {
            model: "ATR 72-212A",
            weight_variant: "23,000 kg MTOW",
            engine_model: "PW127M",
            // EASA TCDS A.084 Note VII.1: Mod 5948 is the New Avionic Suite
            // ("Glass Cockpit") change that defines the "ATR 72-600"
            // commercial designation for the ATR 72-212A; it is not a
            // weight variant. The 23,000 kg MTOW / 22,350 kg MLW /
            // 21,000 kg MZFW variant this preset declares is Mod 6219 (TCDS
            // A.084 section III.13.b, "ATR 72-212A models" table), a
            // separate, independently applicable modification. Physics
            // review v1.2, section 4.3: the prior text named only Mod 5948
            // and could be read as the weight-variant citation.
            modification_state: "ATR 72-600 commercial standard (Mod 5948, NAS/glass cockpit); 23,000 kg MTOW weight variant (Mod 6219)",
            tank_configuration: "standard integral wing tanks",
        },
        reference: AircraftReferenceData {
            // EASA TCDS A.084, Issue 14 (23 Feb 2026), section III.13.b
            // "ATR 72-212A models", Mod 6219 column: MRW 23,170 kg, MTOW
            // 23,000 kg, MLW 22,350 kg, MZFW 21,000 kg. The MTOW/MLW/MZFW
            // already matched this preset exactly; only MRW needed
            // correcting from an uncited 23,150 kg (physics review v1.2,
            // section 4.3, decoded from the downloaded TCDS PDF with pypdf).
            mrw_kg: Some(23_170.0),
            mtow_kg: Some(23_000.0),
            mlw_kg: Some(22_350.0),
            mzfw_kg: Some(21_000.0),
            // Factsheet typical in-service value; see `crate::oew_reference`.
            oew_kg: crate::oew_reference::preset_reference_oew_kg("ATR72-600"),
            usable_fuel_mass_kg: Some(5_000.0),
            reference_wing_area_m2: Some(61.0),
            planning_seats: Some(72),
            certified_max_seats: Some(78),
            partial_design_mission_evidence: vec![PartialDesignMissionEvidence {
                kind: PartialMissionEvidenceKind::AdvertisedRange,
                range: Some(PublishedRange::NauticalMiles(740.0)),
                payload_kg: None,
                load_case: None,
                profile_assumptions: None,
                reserve_assumptions: None,
                reserve_contract: None,
                applicability: "ATR advertises the 740 nm capability for the ATR 72-600 product; it does not define a payload, complete flight profile, or reserve-fuel mass for this preset",
                configuration_applicability: MissionEvidenceApplicability::ModelAndEngineFamily,
                missing: vec![
                    MissingDesignMissionDatum::Payload,
                    MissingDesignMissionDatum::Profile,
                    MissingDesignMissionDatum::ReserveFuel,
                ],
                source: "ATR ATR 72-600 Facts and Figures, product specification (accessed 2026-08-30)",
            }],
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            sources: vec![
                "ATR ATR 72-600 Airport Planning Manual, Issue 8, 2021, aircraft characteristics and limitations",
                "EASA Type Certificate Data Sheet EASA.A.084, ATR 42/72, Issue 14, 23 February 2026, section III.13.b \"ATR 72-212A models\" (Mod 6219 column: MRW 23,170 kg, MTOW 23,000 kg, MLW 22,350 kg, MZFW 21,000 kg) and Note VII.1 (Mod 5948 = New Avionic Suite / \"Glass Cockpit\", the commercial \"ATR 72-600\" designation, a separate modification from the weight variant)",
                "ATR 42 / ATR 72 Aircraft Recovery Manual, 1-10-01 p.19 Figure 1-2 (ATR 72-212A main dimensions: 1.728 m nose to nose wheel, 10.772 m wheelbase, 27.166 m length, 4.10 m track), 1-10-04 p.27 fuselage frame stations (drawing 9SMJ 062110 ZON 00110-004, frame 0 at STA 2362 mm), 4-00-02 Figure 4-1 tail-tipping CG limit H-arm 14.848 m = 54 percent MAC",
                "ATR Weight and Balance Manual, LIMITATIONS LIM.1 p.03, 15 JAN 2021, weight variant F2/75 (MAC 2.303 m; station 0 is 2.362 m forward of the fuselage nose; station 0 to reference chord leading edge 13.604 m)",
                "ATR ATR 72-600 Facts and Figures, product specification (accessed 2026-08-30)",
            ],
            ..AircraftReferenceData::default()
        },
        engine_name: "PW127M",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            wheels_per_mlg_strut: 2,
            // Approximate 4.1 m main-wheel track divided by 2.77 m fuselage width.
            track_diameter_factor: 4.1 / 2.77,
            // Longitudinal gear stations, measured, in the nose-tip frame.
            //
            // Source A, the stations. ATR 42 / ATR 72 *Aircraft Recovery
            // Manual* (ATR, 1 Allee Pierre Nadot, Blagnac; "Printed in
            // France"), 1-10-01 "Aircraft Dimensions", p.19 Figure 1-2
            // "ATR 72-212A Main Dimensions". The side elevation carries one
            // dimension chain along the ground line: 1,728 m (68,03 in) from
            // the fuselage nose to the nose-wheel contact, then 10,772 m
            // (424,09 in) from there to the main-wheel contact, drawn above an
            // overall length of 27,166 m (1069,53 in) taken from the same nose
            // tip. p.18 Figure 1-1 "ATR 42-500 Main Dimensions" prints the
            // identical leading 1,728 m (68,03 in) ahead of an 8,781 m
            // (345,70 in) wheelbase on a 22,67 m (892,52 in) aeroplane, which
            // is what a shared forward fuselage requires and is why the
            // 1,728 m reads as the nose-to-nose-gear leg of the chain.
            //
            // Source B, the frame. ATR *Weight and Balance Manual*,
            // LIMITATIONS LIM.1 "Certified Center of Gravity Envelope" p.03
            // (data module _9d8f0447, 15 JAN 2021, weight variant F2/75,
            // MTOW 22 800 kg): "The MAC is 2.303 m long. Station 0 is 2.362 m
            // forward of the fuselage nose. The distance from station 0 to
            // reference chord leading edge is 13.604 m." The recovery manual's
            // own frame table (1-10-04, drawing 9SMJ 062110 ZON 00110-004)
            // places frame 0 at STA 2362 mm, so STA[mm] = 1000 x x_nose[m] +
            // 2362 exactly and the two manuals share one frame.
            //
            // Independent check, nothing here was fitted to it. The recovery
            // manual's Figure 4-1 tail-tipping CG limit is H-arm 14.848 m
            // (54 % MAC) for the ATR 72 and 12.865 m (63 % MAC) for the ATR 42.
            // Converted with the same 2.362 m datum offset those are 12.486 m
            // and 10.503 m aft of the nose, against the 1.728 + 10.772 =
            // 12.500 m and 1.728 + 8.781 = 10.509 m main-wheel stations the
            // two dimension chains give: 14 mm and 6 mm forward of the main
            // wheels, the small margin a tip-back limit must carry.
            //
            // Stored as fractions of the drawing fuselage length so a shrink
            // or clean-sheet run re-applies them to the active fuselage rather
            // than freezing absolute metres
            // (`LandingGearConfig::resolved_station_positions`). These are
            // ground-contact stations, which is what the nose-gear load
            // balance needs; the ATR main gear is a trailing-arm unit, so its
            // axle and its contact station are not the same point and only the
            // contact station is dimensioned. Declaring the three fields is
            // what retires the `StationError::MainGearStationNotMeasured`
            // refusal this block previously carried: the refusal was correct
            // while no ATR station in a stated frame was held, and the
            // wing-mounted fallback (`x_mlg = mac_le + mlg_x_fraction_mac x
            // mac`) remains outside its domain for this sponson gear - it is
            // now simply not reached.
            reference_wheelbase_m: Some(10.772),
            reference_track_m: Some(4.10),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(27.166),
            reference_nlg_x_fraction: Some(1.728 / 27.166),
            reference_mlg_x_fractions: Some(vec![
                (1.728 + 10.772) / 27.166,
                (1.728 + 10.772) / 27.166,
            ]),
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 27.05,
            // ATR 72-600 Factsheets (2020), three-view: S_ref = 61 m^2.
            // Scale the estimated chords together to close the active
            // side-of-body planform area. Chords and MAC remain estimates;
            // the unverified training-manual MAC is not a fitting target.
            root_chord_m: 4.015_779_489_708_101,
            break_chord_m: 2.805_544_575_001_549_7,
            tip_chord_m: 0.935_181_525_000_516_7,
            sweep_deg: 3.0,
            tip_twist_deg: -2.0,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: 27.166,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        geometry: GeometryConfig {
            wing: WingConfig {
                // ATR Weight and Balance Manual, LIMITATIONS LIM.1 p.03 (15
                // JAN 2021): reference-chord leading edge 11.242 m aft of the
                // nose (station 0 is 2.362 m forward of the nose; station 0
                // to reference-chord LE is 13.604 m; 13.604 - 2.362 =
                // 11.242). The built wing's own area-weighted MAC leading
                // edge (`Wing::aerodynamic_center(0.0)`) sat at 10.488 m at
                // the previous 10.2 m datum -- 0.754 m forward of the WBM
                // reference chord -- which is what drove the model's
                // static margin to roughly -46% and the ZFW nose-gear
                // reaction negative (physics review v1.2, section 4.3).
                // Moving the datum aft by that same 0.754 m places the
                // built LEMAC on the WBM station; it does not change the
                // built wing's own MAC length (2.498 m against the WBM's
                // 2.303 m reference chord, an 8% difference the review
                // separately notes and this fix does not close, since doing
                // so would need a planform-shape change, not a translation).
                // 10.2 + (11.242 - 10.488069716279679) = 10.953930283720321,
                // rounded to the WBM source's own three-decimal precision.
                root_datum_x_m: 10.954,
                root_z_m: 1.85,
                break_z_m: 1.85,
                tip_z_m: 1.85,
                root_twist_deg: 2.0,
                break_twist_deg: 0.0,
                break_span_fraction: 0.32,
                kink_span_fraction: None,
                // 2.9615 m over the 4.0158 m centreline chord: model
                // geometry, not a measured manufacturer station. This is the
                // side-of-body chord `WingConfig::transport_planform` already
                // derives for these chords: the straight-trailing-edge clip
                // `min(interpolated_root_to_kink_chord, kink_trailing_edge_x
                // - side_of_body_leading_edge_x)`, which on this planform is
                // narrower than a plain linear root-to-kink interpolation
                // (~0.906 root-chord ratio) would give. Left derived (the
                // WingConfig default), the station is computed by
                // `transport_planform` but never meshed into the production
                // wing, only an explicit ratio drives `build_main_wing`'s
                // side-of-body xsec (see
                // `crates/alas-geom/src/builder_parts/part_01.rs`). Leaving
                // it unset (as this preset originally did) skips the clip
                // that the chords above were fit to close: the built wing
                // came out at 63.926 m^2 against the published 61 m^2
                // three-view area. Pinning the exact ratio the closure test
                // in `preset_dimension_corrections.rs` already assumes (as
                // A320-200/A380-800/DC-10 already do for their own
                // side-of-body clips) makes the production `s_ref` close
                // the published area instead of silently skipping the clip.
                // `transport_planform`/the closure test remain the
                // authoritative computation of this value; the literal below
                // is that computation's output, not an independent estimate.
                side_of_body_chord_ratio: Some(0.737_461_787_891_521_7),
                outboard_sweep_decrement_deg: 0.0,
                root_airfoil: "naca23018".to_owned(),
                tip_airfoil: "naca23012".to_owned(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 4.6,
                hstab_z_m: 1.0,
                hstab_root_chord_m: 2.8,
                hstab_tip_chord_m: 1.0,
                hstab_root_twist_deg: -1.0,
                hstab_tip_twist_deg: -1.0,
                hstab_tip_le_m: (2.8, 3.6, 0.3),
                vstab_offset_from_tail_m: 4.2,
                vstab_z_m: 1.0,
                vstab_root_chord_m: 4.0,
                vstab_tip_chord_m: 1.4,
                vstab_tip_le_m: (3.4, 0.0, 4.6),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 2.77,
                nose_z_m: 0.0,
                cabin_start_x_m: 3.0,
                cabin_z_m: 0.0,
                tailcone_length_m: 6.0,
                tail_z_m: 1.0,
                ..FuselageConfig::default()
            },
            engine,
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.44,
            cruise_altitude_m: 5_180.0,
            mtow_kg: 23_000.0,
            max_wing_area_m2: 65.0,
            min_wing_loading_kg_m2: 330.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 72,
            cargo_payload_kg: 7_550.0,
            max_structural_payload_kg: 7_550.0,
            // Preliminary-design VD consistent with the ATR transport-speed
            // envelope; retained as a modelling input, not an AFM limitation.
            dive_speed_m_s: 150.0,
            ..DesignRequirements::default()
        },
        // No subsystem fractions are inferred from the published OEW. A
        // turboprop/regional-airframe mass model must be calibrated separately.
        mass_model: None,
        performance: super::high_lift("conservative_simple_flaps"),
    }
}

#[cfg(test)]
// Tests assert on the gear anchor fields of the ATR preset this module
// builds, so a failed expect is the assertion failing, not a library
// invariant being broken.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn atr_identity_and_certified_mass_limits_are_not_mixed() {
        let atr = atr72_600();
        assert_eq!(atr.identity.model, "ATR 72-212A");
        assert_eq!(atr.identity.engine_model, "PW127M");
        assert_eq!(atr.reference.mtow_kg, Some(23_000.0));
        assert_eq!(atr.reference.mlw_kg, Some(22_350.0));
        assert_eq!(atr.reference.mzfw_kg, Some(21_000.0));
        assert_eq!(atr.requirements.mtow_kg, 23_000.0);
    }

    #[test]
    fn atr_pins_the_measured_longitudinal_gear_station_anchor() {
        // ATR 42 / ATR 72 Aircraft Recovery Manual, 1-10-01 p.19 Figure 1-2
        // "ATR 72-212A Main Dimensions": one ground-line dimension chain reads
        // 1,728 m (68,03 in) nose to nose wheel, then 10,772 m (424,09 in) to
        // the main wheel, under a 27,166 m (1069,53 in) overall length taken
        // from the same nose tip. The three anchor fields are declared together
        // or not at all; a partial triple is rejected by
        // `LandingGearConfig::validate`, and an estimate in any one of them
        // would be indistinguishable downstream from this measurement.
        let atr = atr72_600();
        let gear = &atr.landing_gear;
        assert_eq!(
            gear.reference_station_frame.as_deref(),
            Some("nose_tip_drawing_reference"),
        );

        let length_m = gear
            .reference_station_fuselage_length_m
            .expect("ATR drawing fuselage length");
        assert!(
            (length_m - 27.166).abs() < 1e-9,
            "drawing fuselage length {length_m} m",
        );

        let nlg_fraction = gear.reference_nlg_x_fraction.expect("ATR NLG fraction");
        let nlg_x_m = length_m * nlg_fraction;
        assert!(
            (nlg_x_m - 1.728).abs() < 1e-9,
            "x_nlg {nlg_x_m} m from nose"
        );

        let mlg_fractions = gear
            .reference_mlg_x_fractions
            .as_deref()
            .expect("ATR MLG fractions");
        assert_eq!(mlg_fractions.len(), 2, "two sponson main-gear legs");
        for fraction in mlg_fractions {
            let mlg_x_m = length_m * fraction;
            assert!(
                (mlg_x_m - 12.500).abs() < 1e-9,
                "x_mlg {mlg_x_m} m from nose"
            );
        }

        // The wheelbase the two anchors imply is the 10.772 m the same figure
        // prints, and the 10.77 m two independent ATR three-views carry.
        let wheelbase_m = length_m * (mlg_fractions[0] - nlg_fraction);
        assert!(
            (wheelbase_m - 10.772).abs() < 1e-9,
            "implied wheelbase {wheelbase_m} m",
        );
        assert_eq!(gear.reference_wheelbase_m, Some(10.772));
        assert_eq!(gear.reference_track_m, Some(4.10));
    }

    /// The ATR Weight and Balance Manual datum, kept as an executable
    /// conversion rather than prose: LIMITATIONS LIM.1 p.03 (15 JAN 2021)
    /// states "Station 0 is 2.362 m forward of the fuselage nose" and puts the
    /// reference chord leading edge 13.604 m aft of station 0, and the recovery
    /// manual's frame table (1-10-04 p.27, drawing 9SMJ 062110 ZON 00110-004)
    /// puts frame 0 at STA 2362 mm. So the nose-tip frame used by the anchors
    /// above and the ATR station frame differ by exactly 2.362 m, and the
    /// manual's own tail-tipping limit lands just forward of the main wheels.
    #[test]
    fn atr_nose_tip_anchor_frame_matches_the_published_atr_station_frame() {
        const STATION_ZERO_AHEAD_OF_NOSE_M: f64 = 2.362;
        const TAIL_TIPPING_H_ARM_M: f64 = 14.848;

        let atr = atr72_600();
        let gear = &atr.landing_gear;
        let length_m = gear
            .reference_station_fuselage_length_m
            .expect("ATR drawing fuselage length");
        let mlg_x_m = length_m
            * gear
                .reference_mlg_x_fractions
                .as_deref()
                .expect("ATR MLG fractions")[0];

        let mlg_station_m = mlg_x_m + STATION_ZERO_AHEAD_OF_NOSE_M;
        let margin_m = mlg_station_m - TAIL_TIPPING_H_ARM_M;
        assert!(
            (0.0..0.05).contains(&margin_m),
            "tail-tipping limit must sit just forward of the main wheels, got {margin_m} m",
        );
    }

    #[test]
    fn atr_wing_root_sits_above_the_fuselage_crown() {
        // The geometric fact that keeps the wing-mounted fallback out of its
        // domain here, pinned at its source: the wing root chord plane is
        // above the fuselage outer surface, so no wing-root gear bay
        // exists. z is measured up in the geometry frame, m.
        let atr = atr72_600();
        let crown_z_m =
            atr.geometry.fuselage.cabin_z_m + atr.geometry.fuselage.effective_height_m() / 2.0;
        assert!(
            atr.geometry.wing.root_z_m > crown_z_m,
            "ATR wing root at {} m is not above the fuselage crown at {crown_z_m} m",
            atr.geometry.wing.root_z_m
        );
    }

    #[test]
    fn atr_mounts_a_symmetric_pw127m_pair() {
        let atr = atr72_600();
        assert_eq!(atr.engine_name, "PW127M");
        assert_eq!(atr.geometry.engine.engine_name, "PW127M");
        assert_eq!(atr.engine_spanwise_positions(), &[4.05, -4.05]);
    }
}
