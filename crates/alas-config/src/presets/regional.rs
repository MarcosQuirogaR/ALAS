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
    engine.spanwise_positions_m = vec![4.25, -4.25];
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
            modification_state: "ATR 72-600 commercial standard; Mod 5948",
            tank_configuration: "standard integral wing tanks",
        },
        reference: AircraftReferenceData {
            mrw_kg: Some(23_150.0),
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
                "EASA Type Certificate Data Sheet EASA.A.084, ATR 42/72, ATR 72-212A model and engine eligibility",
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
                root_datum_x_m: 10.2,
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
    fn atr_mounts_a_symmetric_pw127m_pair() {
        let atr = atr72_600();
        assert_eq!(atr.engine_name, "PW127M");
        assert_eq!(atr.geometry.engine.engine_name, "PW127M");
        assert_eq!(atr.engine_spanwise_positions(), &[4.25, -4.25]);
    }
}
