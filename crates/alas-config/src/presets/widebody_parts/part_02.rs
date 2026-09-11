// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// Composite long-range twin, the type the global mass model is tuned on.
pub fn b787_9() -> AircraftPreset {
    AircraftPreset {
        name: "B787-9",
        display_name: "Boeing 787-9 Dreamliner",
        description: "Long-range composite widebody twin with GEnx-1B engines.",
        identity: AircraftVariantIdentity {
            model: "787-9",
            weight_variant: "legacy 561,500 lb MTOW",
            engine_model: "GEnx-1B74/75 P2 family",
            modification_state: "Boeing Rev Q legacy-weight planning baseline",
            tank_configuration: "standard 33,399 US gal usable system",
        },
        reference: AircraftReferenceData {
            mrw_kg: Some(255_372.0),
            mtow_kg: Some(254_692.0),
            mlw_kg: Some(192_776.0),
            mzfw_kg: Some(181_436.0),
            oew_kg: Some(128_850.0),
            usable_fuel_volume_l: Some(126_429.0),
            usable_fuel_mass_kg: Some(101_522.0),
            fuel_density_kg_l: Some(101_522.0 / 126_429.0),
            reference_wing_area_m2: Some(360.464),
            planning_seats: Some(290),
            certified_max_seats: Some(420),
            certified_exit_layout: None,
            design_mission_evidence: crate::DesignMissionEvidence::Unverified,
            partial_design_mission_evidence: vec![PartialDesignMissionEvidence {
                kind: PartialMissionEvidenceKind::PayloadRangeChart,
                range: None,
                payload_kg: None,
                load_case: None,
                profile_assumptions: Some("long-range cruise with typical engines only"),
                reserve_assumptions: None,
                reserve_contract: None,
                applicability: "787-9 chart is model-level planning evidence and selects no legacy-weight design point",
                configuration_applicability: MissionEvidenceApplicability::ModelOnly,
                missing: vec![
                    MissingDesignMissionDatum::Range,
                    MissingDesignMissionDatum::Payload,
                    MissingDesignMissionDatum::Profile,
                    MissingDesignMissionDatum::ReserveFuel,
                ],
                source: "Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 3.2.2 p.3-3",
            }],
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            planning_cg_envelope: None,
            sources: vec![
                "Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 2",
                "NASA/TP-20210023843, December 2022, Table I",
                "Boeing 787 ACAP D6-58333 Rev L, December 2015, p.2-3 (typical OEW only)",
            ],
        },
        engine_name: "GEnx-1B",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            wheels_per_mlg_strut: 4,
            track_diameter_factor: 9.8 / 5.94,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 60.12,
            root_chord_m: 12.60,
            break_chord_m: 6.50,
            tip_chord_m: 1.60,
            sweep_deg: 32.2,
            tip_twist_deg: -2.0,
            wing_x_shift_m: -1.5,
            tail_scale: 1.0,
            fuselage_length_m: 62.81,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        geometry: GeometryConfig {
            wing: WingConfig {
                root_datum_x_m: 21.00,
                root_z_m: -1.8,
                break_z_m: -0.2,
                tip_z_m: 2.5,
                root_twist_deg: 3.5,
                break_twist_deg: 1.5,
                break_span_fraction: 0.35,
                kink_span_fraction: Some(0.353_771_245_388_011_8),
                outboard_sweep_decrement_deg: 2.0,
                root_airfoil: "sc20614".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 9.5,
                hstab_z_m: 1.0,
                hstab_root_chord_m: 6.5,
                hstab_tip_chord_m: 1.8,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (6.0, 9.5, 0.8),
                vstab_offset_from_tail_m: 10.5,
                vstab_z_m: 1.8,
                vstab_root_chord_m: 8.0,
                vstab_tip_chord_m: 2.8,
                vstab_tip_le_m: (7.5, 0.0, 8.5),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 5.94,
                nose_z_m: -0.4,
                cabin_start_x_m: 5.5,
                cabin_z_m: 0.2,
                tailcone_length_m: 12.0,
                tail_z_m: 1.5,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                spanwise_positions_m: vec![9.5, -9.5],
                z_m: -2.55,
                inlet_x_offset_m: 3.5,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.85,
            cruise_altitude_m: 11887.2,
            mtow_kg: 254_692.0,
            max_wing_area_m2: 385.0,
            min_wing_loading_kg_m2: 480.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 290,
            cargo_payload_kg: 55_000.0,
            // Maximum zero-fuel weight 181.4 t less an operating empty weight
            // of 128.8 t.
            max_structural_payload_kg: 52_586.0,
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: super::high_lift("advanced_highlift_widebody"),
    }
}

/// Trijet with a centerline engine, the oldest type in the registry.
pub fn dc_10() -> AircraftPreset {
    AircraftPreset {
        name: "DC-10",
        display_name: "McDonnell Douglas DC-10-30 (572k option)",
        description: "DC-10-30 ACAP 572,000 lb option with three CF6-50C engines.",
        identity: AircraftVariantIdentity {
            model: "DC-10-30 passenger",
            weight_variant: "ACAP 572,000 lb option",
            engine_model: "CF6-50C family",
            modification_state: "DAC-67803A Rev A footnoted 572k planning option",
            tank_configuration: "36,652 US gal with center-wing auxiliary tank",
        },
        reference: AircraftReferenceData {
            // The ACAP footnote defines the 572,000 lb takeoff option but does
            // not restate a matching ramp limit. 253,105 kg belongs to the
            // standard 555,000 lb row and must not be mixed into this record.
            mrw_kg: None,
            mtow_kg: Some(259_454.0),
            mlw_kg: Some(190_962.0),
            mzfw_kg: Some(166_922.0),
            oew_kg: Some(120_914.0),
            usable_fuel_volume_l: Some(137_509.0),
            usable_fuel_mass_kg: Some(111_387.0),
            reference_wing_area_m2: Some(338.84),
            planning_seats: Some(255),
            certified_max_seats: Some(399),
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            sources: vec![
                "Boeing DC/MD-10 ACAP DAC-67803A Rev A, Figure 2.1",
                "FAA TCDS A22WE Rev 13, 2018-04-30",
                "NASA CR-3119, April 1979",
            ],
            ..AircraftReferenceData::default()
        },
        engine_name: "CF6-50",
        n_engines: 3,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 3,
            // Two four-wheel wing bogies plus one two-wheel center gear.
            wheels_per_mlg_strut: 0,
            track_diameter_factor: 1.77,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 50.39,
            // The chord distribution is an estimate; scale it as one shape
            // so the NASA reference area closes without changing its tapers.
            root_chord_m: 12.798_762_957_481_8,
            break_chord_m: 7.799_246_177_215_49,
            tip_chord_m: 1.799_826_040_895_88,
            // NASA CR-3119 (1979) specifies 35 deg at quarter chord. The
            // outboard taper converts it to this leading-edge angle.
            sweep_deg: 38.371_897_798_492_91,
            tip_twist_deg: -2.0,
            wing_x_shift_m: -3.25,
            tail_scale: 1.0,
            fuselage_length_m: 55.55,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        geometry: GeometryConfig {
            wing: WingConfig {
                root_datum_x_m: 21.40,
                root_z_m: -1.6,
                break_z_m: -0.3,
                tip_z_m: 2.0,
                root_twist_deg: 4.0,
                break_twist_deg: 1.5,
                break_span_fraction: 0.35,
                // Pin the area-calibrated chord independently of sweep and
                // its automatic side-of-body trailing-edge clipping rule.
                side_of_body_chord_ratio: Some(0.888_392_857_142_857_5),
                kink_span_fraction: Some(0.35),
                outboard_sweep_decrement_deg: 2.0,
                root_airfoil: "sc20612".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 8.5,
                hstab_z_m: 1.0,
                hstab_root_chord_m: 7.2,
                hstab_tip_chord_m: 2.0,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (5.8, 8.5, 0.8),
                vstab_offset_from_tail_m: 10.0,
                vstab_z_m: 2.2,
                vstab_root_chord_m: 10.5,
                vstab_tip_chord_m: 3.8,
                vstab_tip_le_m: (7.5, 0.0, 9.5),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 6.02,
                nose_z_m: -0.4,
                cabin_start_x_m: 5.5,
                cabin_z_m: 0.2,
                tailcone_length_m: 11.5,
                tail_z_m: 1.6,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                engine_name: "CF6-50".to_owned(),
                // The third position is the centerline engine, which sits on
                // the fin rather than under the wing. The vertical offset
                // below is the underwing pair's, so nothing here places the
                // tail engine correctly; whatever draws it has to know.
                spanwise_positions_m: vec![8.8, -8.8, 0.0],
                z_m: -2.12,
                inlet_x_offset_m: 3.5,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.82,
            cruise_altitude_m: 10668.0,
            mtow_kg: 259_454.0,
            max_wing_area_m2: 338.84,
            min_wing_loading_kg_m2: 500.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 250,
            cargo_payload_kg: 65_000.0,
            // ACAP 572k planning option: MZFW 166,922 kg less OEW 120,914 kg.
            max_structural_payload_kg: 46_008.0,
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: super::high_lift("advanced_highlift_widebody"),
    }
}
