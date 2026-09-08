// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MissingDesignMissionDatum, MissionEvidenceApplicability,
    PartialDesignMissionEvidence, PartialMissionEvidenceKind, PublishedRange,
    PublishedReserveContract, WingConfig,
};

/// How far below the wing reference plane the A380's engines hang, in metres.
///
/// Named rather than written in place because the lint below reads any 3.14 as
/// a mistyped pi. It is a ground-clearance calibration: set by the inboard
/// pair at about 0.35 m under the wing lower surface, which leaves the
/// outboard pair about 1.4 m.
// The value is a length in metres that happens to round to pi's first three
// digits; there is no circle anywhere near it.
#[allow(clippy::approx_constant)]
const A380_ENGINE_Z_M: f64 = -3.14;

/// Long-range quad with a conventional tail.
pub fn a340_300() -> AircraftPreset {
    AircraftPreset {
        name: "A340-300",
        display_name: "Airbus A340-300",
        description: "Airbus A340-312 WV029 with CFM56-5C3-family engines.",
        identity: AircraftVariantIdentity {
            model: "A340-312",
            weight_variant: "WV029",
            engine_model: "CFM56-5C3/F",
            modification_state: "public WV029 planning baseline",
            tank_configuration: "three tanks",
        },
        reference: AircraftReferenceData {
            mrw_kg: Some(260_900.0),
            mtow_kg: Some(260_000.0),
            mlw_kg: Some(188_000.0),
            mzfw_kg: Some(178_000.0),
            usable_fuel_volume_l: Some(141_500.0),
            usable_fuel_mass_kg: Some(113_200.0),
            fuel_density_kg_l: Some(0.8),
            planning_seats: Some(335),
            certified_max_seats: Some(375),
            partial_design_mission_evidence: vec![PartialDesignMissionEvidence {
                kind: PartialMissionEvidenceKind::PayloadRangeChart,
                range: None,
                payload_kg: None,
                load_case: None,
                profile_assumptions: Some("ISA cruise at 39,000 ft and Mach 0.82 only"),
                reserve_assumptions: None,
                reserve_contract: None,
                applicability: "A340-300 CFM56-5C3 chart matches the model and engine family but selects no weight-variant design point",
                configuration_applicability: MissionEvidenceApplicability::ModelAndEngineFamily,
                missing: vec![
                    MissingDesignMissionDatum::Range,
                    MissingDesignMissionDatum::Payload,
                    MissingDesignMissionDatum::Profile,
                    MissingDesignMissionDatum::ReserveFuel,
                ],
                source: "Airbus A340-200/-300 Aircraft Characteristics Rev 33, 2025-12-01, section 3-2-1 p.4, Figure 3-2-1-991-013-A01",
            }],
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            reference_wing_area_m2: Some(361.6),
            sources: vec![
                "EASA.A.015 Issue 28, 2026-01-15, pp.32-36",
                "Airbus A340-200/-300 Aircraft Characteristics Rev 33, 2025-12-01, section 2-1-1",
            ],
            ..AircraftReferenceData::default()
        },
        engine_name: "CFM56-5C3/F",
        n_engines: 4,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 3,
            // The two four-wheel wing bogies and two-wheel center gear cannot
            // be represented by one uniform per-strut count, so tire sizing
            // remains automatic while the load-bearing leg count is exact.
            wheels_per_mlg_strut: 0,
            track_diameter_factor: 10.684 / 5.64,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 60.30,
            root_chord_m: 12.00,
            break_chord_m: 6.50,
            tip_chord_m: 1.80,
            sweep_deg: 30.0,
            tip_twist_deg: -2.0,
            wing_x_shift_m: -1.4,
            tail_scale: 1.0,
            fuselage_length_m: 63.66,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        geometry: GeometryConfig {
            wing: WingConfig {
                root_datum_x_m: 22.0,
                root_z_m: -1.8,
                break_z_m: -0.3,
                tip_z_m: 2.0,
                root_twist_deg: 3.5,
                break_twist_deg: 1.5,
                break_span_fraction: 0.35,
                // Active side-of-body planform calibrated to the Airbus
                // 361.6 m^2 reference area without changing the chords.
                kink_span_fraction: Some(0.362_094_754_983_253_8),
                outboard_sweep_decrement_deg: 2.0,
                root_airfoil: "sc20612".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 9.0,
                hstab_z_m: 1.0,
                hstab_root_chord_m: 6.5,
                hstab_tip_chord_m: 1.8,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                // Airbus A340 Aircraft Characteristics, general dimensions:
                // 19.4 m full horizontal-tail span.
                hstab_tip_le_m: (6.0, 9.7, 0.8),
                vstab_offset_from_tail_m: 10.5,
                vstab_z_m: 1.8,
                vstab_root_chord_m: 8.0,
                vstab_tip_chord_m: 2.8,
                vstab_tip_le_m: (7.5, 0.0, 8.5),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 5.64,
                nose_z_m: -0.4,
                cabin_start_x_m: 5.5,
                cabin_z_m: 0.2,
                tailcone_length_m: 12.0,
                tail_z_m: 1.5,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                spanwise_positions_m: vec![7.5, -7.5, 14.0, -14.0],
                // Calibrated for the inboard pair at 0.35 m of clearance. The
                // outboard pair sits under a thinner, higher part of the wing
                // and clears by about 1.4 m at the same offset.
                z_m: -1.94,
                inlet_x_offset_m: 3.0,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.82,
            cruise_altitude_m: 11887.2,
            mtow_kg: 260_000.0,
            max_wing_area_m2: 370.0,
            min_wing_loading_kg_m2: 500.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 290,
            cargo_payload_kg: 45_000.0,
            // Maximum zero-fuel weight 178.0 t less an operating empty weight
            // of about 129.4 t.
            max_structural_payload_kg: 48_600.0,
            dive_speed_m_s: 200.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: super::high_lift("advanced_highlift_widebody"),
    }
}

/// Double-deck quad, the largest airliner in the registry.
pub fn a380_800() -> AircraftPreset {
    AircraftPreset {
        name: "A380-800",
        display_name: "Airbus A380-800",
        description: "Airbus A380-841 WV000 with Trent 970-84 engines.",
        identity: AircraftVariantIdentity {
            model: "A380-841",
            weight_variant: "WV000",
            engine_model: "Trent 970-84",
            modification_state: "WV000 public planning baseline",
            tank_configuration: "323,546 L tanks + 793 L usable system inventory",
        },
        reference: AircraftReferenceData {
            mrw_kg: Some(562_000.0),
            mtow_kg: Some(560_000.0),
            mlw_kg: Some(386_000.0),
            mzfw_kg: Some(361_000.0),
            usable_fuel_volume_l: Some(324_339.0),
            usable_fuel_mass_kg: Some(259_471.0),
            fuel_density_kg_l: Some(0.8),
            reference_wing_area_m2: Some(845.0),
            planning_seats: Some(555),
            certified_max_seats: Some(868),
            partial_design_mission_evidence: vec![PartialDesignMissionEvidence {
                kind: PartialMissionEvidenceKind::PayloadRangeChart,
                range: None,
                payload_kg: None,
                load_case: None,
                profile_assumptions: Some(
                    "ISA, no wind, labeled typical international flight profile",
                ),
                reserve_assumptions: Some(
                    "200 nm diversion; 5% trip fuel allowance; 30 min holding",
                ),
                reserve_contract: Some(PublishedReserveContract {
                    diversion_range: Some(PublishedRange::NauticalMiles(200.0)),
                    trip_fuel_allowance_fraction: Some(0.05),
                    holding_time_minutes: Some(30.0),
                }),
                applicability: "A380-800 Trent 900 chart matches the model and engine family but selects no weight-variant design point",
                configuration_applicability: MissionEvidenceApplicability::ModelAndEngineFamily,
                missing: vec![
                    MissingDesignMissionDatum::Range,
                    MissingDesignMissionDatum::Payload,
                    MissingDesignMissionDatum::Profile,
                    MissingDesignMissionDatum::ReserveFuel,
                ],
                source: "Airbus A380 Aircraft Characteristics Rev 20, 2025-12-01, section 3-2-1 p.2, Figure 3-2-1-991-001-A01",
            }],
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            sources: vec![
                "EASA.A.110 Issue 17, 2026-08-05, pp.10-15",
                "Airbus A380 Aircraft Characteristics Rev 20, 2025-12-01, section 2-1-1",
                "Airbus A380 Facts and Figures, February 2022, p.3",
            ],
            ..AircraftReferenceData::default()
        },
        engine_name: "Trent 970-84",
        n_engines: 4,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 4,
            // Wing and body bogies carry four and six wheels respectively.
            wheels_per_mlg_strut: 0,
            track_diameter_factor: 14.34 / 7.14,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 79.75,
            // Airbus does not publish these three chords. Uniformly scaling
            // the estimated distribution preserves both taper ratios while
            // closing the projected planform on the published 845 m^2 S_ref.
            root_chord_m: 22.952_583_900_271_1,
            break_chord_m: 11.276_704_264_046_2,
            tip_chord_m: 3.492_784_506_562_99,
            // Airbus A380 Facts and Figures (2022) gives 33.5 deg wing
            // sweep; Jane's identifies the quarter-chord convention. The
            // outboard taper converts that to this leading-edge angle.
            sweep_deg: 36.429_099_956_878_43,
            tip_twist_deg: -2.5,
            wing_x_shift_m: -7.5,
            tail_scale: 1.0,
            fuselage_length_m: 72.73,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        geometry: GeometryConfig {
            wing: WingConfig {
                root_datum_x_m: 26.70,
                root_z_m: -2.5,
                break_z_m: -0.4,
                tip_z_m: 3.0,
                root_twist_deg: 4.5,
                break_twist_deg: 2.0,
                break_span_fraction: 0.33,
                // Preserve the area-calibrated body chord explicitly: sweep
                // must not change it through the trailing-edge clipping rule.
                side_of_body_chord_ratio: Some(0.789_394_889_312_722_8),
                // This station and chord distribution recover 845 m^2.
                kink_span_fraction: Some(0.359_236_516_064_625_5),
                outboard_sweep_decrement_deg: 2.5,
                root_airfoil: "SC2-0714".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 11.0,
                hstab_z_m: 1.5,
                hstab_root_chord_m: 9.0,
                hstab_tip_chord_m: 2.5,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (8.5, 12.5, 1.2),
                vstab_offset_from_tail_m: 13.0,
                vstab_z_m: 2.5,
                vstab_root_chord_m: 11.0,
                vstab_tip_chord_m: 3.5,
                vstab_tip_le_m: (10.0, 0.0, 11.0),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 7.14,
                // The one non-circular body in the registry: two decks make it
                // taller than it is wide.
                height_m: Some(8.41),
                nose_z_m: -0.6,
                cabin_start_x_m: 7.0,
                cabin_z_m: 0.3,
                tailcone_length_m: 15.0,
                tail_z_m: 2.0,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                spanwise_positions_m: vec![10.0, -10.0, 18.5, -18.5],
                z_m: A380_ENGINE_Z_M,
                inlet_x_offset_m: 4.5,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.85,
            cruise_altitude_m: 11887.2,
            mtow_kg: 560_000.0,
            max_wing_area_m2: 845.0,
            min_wing_loading_kg_m2: 450.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 525,
            cargo_payload_kg: 150_000.0,
            // Maximum zero-fuel weight about 361 t less an operating empty
            // weight of about 277 t.
            max_structural_payload_kg: 84_000.0,
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: super::high_lift("advanced_highlift_widebody"),
    }
}
