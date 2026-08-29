// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/presets.py (the single-aisle entries)
// Reference: alas @ rust-port-baseline.

//! Two published single-aisle types, which is where the global assumptions
//! stop fitting.
//!
//! The rest of this crate is calibrated around a modern twin-aisle transport,
//! and neither of these is one. Two corrections follow from that, and both are
//! stated on the presets rather than left to whoever runs them.
//!
//! Mass first. Structural, systems and furnishings mass does not scale
//! linearly with takeoff weight, so the widebody-calibrated Torenbeek
//! fractions under-predict a small aircraft's operating empty weight -- by
//! about 2.8 t on the A220-300, which is most of a revenue payload's worth of
//! error in the wrong direction. Its entry carries fractions of its own.
//!
//! Speeds second. Both types have full-span slats and slotted Fowler flaps,
//! which is a materially better high-lift system than the generic
//! "standard narrow-body" bucket describes; scored with the generic one their
//! rotation and takeoff-safety speeds come out fifteen to twenty knots high,
//! which sizes them out of runways they operate from every day.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MassModelConfig, MissingDesignMissionDatum,
    MissionEvidenceApplicability, PartialDesignMissionEvidence, PartialMissionEvidenceKind,
    PublishedMissionLoadCase, PublishedRange, WingConfig,
};

/// Short and medium-range twin, the reference single-aisle.
pub fn a320_200() -> AircraftPreset {
    AircraftPreset {
        name: "A320-200",
        display_name: "Airbus A320-200",
        description: "Airbus A320-214 WV017 with CFM56-5B4/3 engines and sharklets.",
        identity: AircraftVariantIdentity {
            model: "A320-214",
            weight_variant: "WV017",
            engine_model: "CFM56-5B4/3",
            modification_state: "MOD160500 sharklets; MOD37147 Tech Insertion",
            tank_configuration: "three tanks; MOD37331 + MOD160001",
        },
        reference: AircraftReferenceData {
            mrw_kg: Some(78_400.0),
            mtow_kg: Some(78_000.0),
            mlw_kg: Some(66_000.0),
            mzfw_kg: Some(62_500.0),
            usable_fuel_volume_l: Some(24_167.0),
            usable_fuel_mass_kg: Some(19_334.0),
            fuel_density_kg_l: Some(0.8),
            partial_design_mission_evidence: vec![PartialDesignMissionEvidence {
                kind: PartialMissionEvidenceKind::PayloadRangeChart,
                range: None,
                payload_kg: None,
                load_case: Some(PublishedMissionLoadCase::TakeoffMassesKg(vec![
                    73_500.0, 78_000.0,
                ])),
                profile_assumptions: Some("ISA conditions only"),
                reserve_assumptions: None,
                reserve_contract: None,
                applicability: "A320-200 sharklet chart includes 73,500 kg and 78,000 kg base/one-ACT curves; Airbus marks the curves informational and selects no WV017 design point",
                configuration_applicability: MissionEvidenceApplicability::ModelOnly,
                missing: vec![
                    MissingDesignMissionDatum::Range,
                    MissingDesignMissionDatum::Payload,
                    MissingDesignMissionDatum::Profile,
                    MissingDesignMissionDatum::ReserveFuel,
                ],
                source: "Airbus A320 Aircraft Characteristics Rev 46, 2026-07-01, section 3-2-1 p.3, Figure 3-2-1-991-017-A01",
            }],
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            reference_wing_area_m2: Some(122.6),
            sources: vec![
                "Airbus A320 Aircraft Characteristics Rev 46, 2026-07-01, section 2-1-1 p.2",
                "EASA.A.064 Issue 62, pp.37-48",
                "EASA.E.003 Issue 06, pp.10-11",
            ],
            ..AircraftReferenceData::default()
        },
        engine_name: "CFM56-5B4/3",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            wheels_per_mlg_strut: 2,
            track_diameter_factor: 7.59 / 3.95,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 35.80,
            root_chord_m: 6.10,
            break_chord_m: 3.80,
            tip_chord_m: 1.20,
            sweep_deg: 25.0,
            tip_twist_deg: -1.5,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: 37.57,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        geometry: GeometryConfig {
            wing: WingConfig {
                root_datum_x_m: 12.90,
                root_z_m: -1.2,
                break_z_m: -0.2,
                tip_z_m: 1.5,
                root_twist_deg: 3.0,
                break_twist_deg: 1.0,
                break_span_fraction: 0.37,
                kink_span_fraction: Some(0.377_380_002_280_241_9),
                outboard_sweep_decrement_deg: 1.5,
                root_airfoil: "sc20610".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 5.5,
                hstab_z_m: 0.8,
                hstab_root_chord_m: 4.0,
                hstab_tip_chord_m: 1.2,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (3.5, 6.0, 0.5),
                vstab_offset_from_tail_m: 6.5,
                vstab_z_m: 1.2,
                vstab_root_chord_m: 5.2,
                vstab_tip_chord_m: 1.8,
                vstab_tip_le_m: (5.0, 0.0, 5.8),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 3.95,
                nose_z_m: -0.3,
                cabin_start_x_m: 3.5,
                cabin_z_m: 0.1,
                tailcone_length_m: 7.5,
                tail_z_m: 1.0,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                spanwise_positions_m: vec![5.5, -5.5],
                // About 0.35 m of clearance under the wing lower surface,
                // which is what a large-fan engine on a low-slung single-aisle
                // has to live with.
                z_m: -1.71,
                inlet_x_offset_m: 2.5,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.78,
            cruise_altitude_m: 11278.0,
            mtow_kg: 78_000.0,
            max_wing_area_m2: 130.0,
            min_wing_loading_kg_m2: 500.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 150,
            cargo_payload_kg: 18_000.0,
            // Maximum zero-fuel weight 62.5 t less an operating empty weight
            // of about 42.6 t.
            max_structural_payload_kg: 19_900.0,
            dive_speed_m_s: 180.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: super::high_lift("modern_narrowbody"),
    }
}

/// The smallest type in the registry, and the only one with its own mass model.
pub fn a220_300() -> AircraftPreset {
    AircraftPreset {
        name: "A220-300",
        display_name: "Airbus A220-300",
        description: "BD-500-1A11 legacy-weight A220-300 with PW1521G-3 engines.",
        identity: AircraftVariantIdentity {
            model: "BD-500-1A11",
            weight_variant: "legacy 149,000 lb MTOW",
            engine_model: "PW1521G-3",
            modification_state: "S/N 55001-59999 planning configuration",
            tank_configuration: "standard integral tanks",
        },
        reference: AircraftReferenceData {
            mrw_kg: Some(68_039.0),
            mtow_kg: Some(67_585.0),
            mlw_kg: Some(58_740.0),
            mzfw_kg: Some(55_792.0),
            oew_kg: Some(37_149.0),
            usable_fuel_volume_l: Some(21_504.92),
            usable_fuel_mass_kg: Some(17_395.27),
            fuel_density_kg_l: Some(0.8089),
            planning_seats: Some(140),
            partial_design_mission_evidence: vec![
                PartialDesignMissionEvidence {
                    kind: PartialMissionEvidenceKind::AdvertisedRange,
                    range: Some(PublishedRange::NauticalMiles(3_400.0)),
                    payload_kg: None,
                    load_case: Some(PublishedMissionLoadCase::TakeoffMassesKg(vec![70_900.0])),
                    profile_assumptions: None,
                    reserve_assumptions: None,
                    reserve_contract: None,
                    applicability: "Airbus advertises up to 3,400 nm for the up-to-70.9 t product; that capability claim does not select payload or apply exactly to the preset's legacy 67,585 kg weight variant",
                    configuration_applicability: MissionEvidenceApplicability::DifferentWeightVariant,
                    missing: vec![
                        MissingDesignMissionDatum::Payload,
                        MissingDesignMissionDatum::Profile,
                        MissingDesignMissionDatum::ReserveFuel,
                    ],
                    source: "Airbus A220 Digital Pamphlet FAI V5.2, July 2022, p.1",
                },
                PartialDesignMissionEvidence {
                    kind: PartialMissionEvidenceKind::PayloadRangeChart,
                    range: None,
                    payload_kg: None,
                    load_case: Some(PublishedMissionLoadCase::ZeroFuelWeightRangeEnvelope),
                    profile_assumptions: Some("ISA conditions only"),
                    reserve_assumptions: None,
                    reserve_contract: None,
                    applicability: "BD-500-1A11 S/N 55001-59999 ZFW/range chart; Airbus marks the publication superseded and the chart selects no legacy-weight design point",
                    configuration_applicability: MissionEvidenceApplicability::ExactPreset,
                    missing: vec![
                        MissingDesignMissionDatum::Range,
                        MissingDesignMissionDatum::Payload,
                        MissingDesignMissionDatum::Profile,
                        MissingDesignMissionDatum::ReserveFuel,
                    ],
                    source: "Airbus A220-300 APP Issue 031, 2023-10-19, data module BD500-A-J00-00-00-13AAB-030A-A pp.2-3, Figure 1",
                },
            ],
            cg_evidence: CgEnvelopeEvidence::PublicPlanning,
            reference_wing_area_m2: Some(112.3),
            planning_cg_envelope: Some(super::cg_envelope::A220_300_PLANNING_CG_ENVELOPE),
            sources: vec![
                "Airbus A220 Aircraft Recovery Publication BD500-3AB48-10400-00, May 2026, J06-20-01 p.14 and J08-41-03-01 p.2",
                "Airbus A220 ARP J07-40-00-06AAA-030A-A, 2019-10-22 p.2",
            ],
            ..AircraftReferenceData::default()
        },
        engine_name: "PW1500G",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            wheels_per_mlg_strut: 2,
            track_diameter_factor: 6.731 / 3.50,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 35.10,
            root_chord_m: 5.80,
            break_chord_m: 3.50,
            tip_chord_m: 1.10,
            sweep_deg: 25.0,
            tip_twist_deg: -1.5,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: 38.70,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        geometry: GeometryConfig {
            wing: WingConfig {
                root_datum_x_m: 13.30,
                root_z_m: -1.0,
                break_z_m: -0.2,
                tip_z_m: 1.5,
                root_twist_deg: 3.0,
                break_twist_deg: 1.0,
                break_span_fraction: 0.37,
                kink_span_fraction: Some(0.382_736_255_076_680_5),
                outboard_sweep_decrement_deg: 1.5,
                root_airfoil: "SC2-0714".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 5.0,
                hstab_z_m: 0.7,
                hstab_root_chord_m: 3.8,
                hstab_tip_chord_m: 1.1,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (3.2, 5.5, 0.5),
                vstab_offset_from_tail_m: 6.0,
                vstab_z_m: 1.0,
                vstab_root_chord_m: 5.0,
                vstab_tip_chord_m: 1.6,
                vstab_tip_le_m: (4.5, 0.0, 5.5),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 3.50,
                nose_z_m: -0.2,
                cabin_start_x_m: 3.2,
                cabin_z_m: 0.1,
                tailcone_length_m: 7.0,
                tail_z_m: 0.8,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                spanwise_positions_m: vec![5.2, -5.2],
                z_m: -1.71,
                inlet_x_offset_m: 2.3,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.78,
            cruise_altitude_m: 11278.0,
            mtow_kg: 67_585.0,
            max_wing_area_m2: 120.0,
            min_wing_loading_kg_m2: 480.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 130,
            cargo_payload_kg: 15_000.0,
            // The same published planning configuration gives MZFW 55,792 kg
            // and OEW 37,149 kg.
            max_structural_payload_kg: 18_643.0,
            dive_speed_m_s: 175.0,
            ..DesignRequirements::default()
        },
        // Compatibility-only outcome calibration for the frozen
        // ReferenceCompatibleFractions method: the published operating empty
        // weight is 37.08 t while the global fractions predict about 34.3 t.
        // These raised fractions close that gap; they are not source-backed
        // A220 avionics or furnishings subsystem masses. A physical method
        // must replace them only after its architecture inputs are declared.
        mass_model: Some(MassModelConfig {
            systems_mass_fraction: 0.13,
            furnishings_mass_fraction: 0.12,
            ..MassModelConfig::default()
        }),
        performance: super::high_lift("modern_narrowbody"),
    }
}
