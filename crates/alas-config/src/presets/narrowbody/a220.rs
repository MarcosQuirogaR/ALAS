// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airbus A220-300 preset.

use super::exit_layouts::A220_300_CERTIFIED_EXIT_LAYOUT;
use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MassModelConfig, MissingDesignMissionDatum,
    MissionEvidenceApplicability, PartialDesignMissionEvidence, PartialMissionEvidenceKind,
    PublishedMissionLoadCase, PublishedRange, WingConfig,
};

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
            // ICAO Annex 14 Vol. I Table 1-1 (aerodrome reference code) applied to the
            // preset wingspan of 35.10 m (Airbus A220-300 airport-planning dimensions):
            // 24 m <= b < 36 m is code C.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::C),
            mrw_kg: Some(68_039.0),
            mtow_kg: Some(67_585.0),
            mlw_kg: Some(58_740.0),
            mzfw_kg: Some(55_792.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("A220-300"),
            usable_fuel_volume_l: Some(21_504.92),
            usable_fuel_mass_kg: Some(17_395.27),
            fuel_density_kg_l: Some(0.8089),
            planning_seats: Some(140),
            design_point: Some(crate::PayloadRangeDesignPoint {
                range_nmi: 2_150.0,
                payload_kg: Some(18_643.0),
                source: "Airbus A220 Airport Planning (ACP Issue 013, 2025-11) p.239 Figure 1 zero-fuel weight vs range, ISA, applicability S/N 55001-59999: maximum-zero-fuel-weight corner read as 2,150 nmi (+-25 nmi), drawn for a 156,300 lb MTOW variant, so a heavier weight variant than this 149,000 lb preset; payload is the preset MZFW minus its reference OEW",
            }),
            // EASA's BD-500 type-certificate data sheet sets 145 as the
            // baseline maximum passenger seating capacity for the selected
            // legacy S/N 55001-59999 configuration.  The optional 149-seat
            // arrangement is a different exit installation and is therefore
            // deliberately not folded into this preset.
            certified_max_seats: Some(145),
            certified_exit_layout: Some(A220_300_CERTIFIED_EXIT_LAYOUT),
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
            planning_cg_envelope: Some(crate::presets::cg_envelope::A220_300_PLANNING_CG_ENVELOPE),
            aft_cg_nose_load: Some(crate::presets::gear_load::A220_300),
            sources: vec![
                "Airbus FAST 63 (2019), Flying the A220; Airbus A220 airframe features (July 2025), composite wing: https://www.aircraft.airbus.com/en/newsroom/stories/2025-07-the-clean-sheet-single-aisle-aircraft-at-the-vanguard-of-innovation",
                crate::preset_structures::TRANSPORT_CAP_SOURCE,
                "Airbus A220 Aircraft Recovery Publication BD500-3AB48-10400-00, May 2026, J06-20-01 p.14 and J08-41-03-01 p.2",
                "Airbus A220 ARP J07-40-00-06AAA-030A-A, 2019-10-22 p.2",
                "Airbus A220 Aircraft Characteristics - Airport and Maintenance Planning, A220-ACP-Issue013-00-27Nov2025, DM BD500-A-J06-10-00-00AAA-030A-A Rev 2023-11-01, pp.150-156 (nominal nose-tip drawing frame; dimensions vary with weight/CG)",
                "EASA.IM.A.570 BD-500 TCDS Issue 24, 2026-02-20, Section 2 BD-500-1A11 III.19 p.23 (baseline MPSC 145; Option C25631002 is required for 149)",
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
            // Airbus' ACP side/ground drawing gives the A220-300 nominal
            // longitudinal anchors for S/N 55001-59999.  The dimensions
            // originate at the geometric nose-tip extension, so retain that
            // frame explicitly and normalize the stations before applying
            // them to a resized active fuselage.  These are drawing/group
            // centres, not certified WBM/AFM datum or attachment points.
            reference_wheelbase_m: Some(15.23238),
            reference_track_m: Some(6.731),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(38.68928),
            reference_nlg_x_fraction: Some(3.401568 / 38.68928),
            reference_mlg_x_fractions: Some(vec![
                18.633948 / 38.68928,
                18.633948 / 38.68928,
            ]),
            mlg_strut_bogie_wheels: Some(vec![2, 2]),
            // ACP Issue 013 clearance table: fuselage top 5.385 m minimum
            // less the 3.721 m body height (Figure 1, locator B).
            fuselage_ground_clearance_m: Some(1.664),
            // No published tail-strike attitude backs this preset's aft-fuselage
            // geometry, so its model tail-down angle is unvalidated (the generic
            // tailcone loft understates it): keep the Raymer/Roskam 15 deg floor on
            // top of the tail-down criterion.
            min_tip_back_deg: 15.0,
            takeoff_stabilizer_nose_up_deg: Some(crate::landing_gear::TRANSPORT_THS_TAKEOFF_NOSE_UP_DEG),
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 35.10,
            root_chord_m: 5.80,
            break_chord_m: 3.50,
            tip_chord_m: 1.10,
            // Leading-edge sweep read off the plan view of the A220 ACP
            // Issue 013 (DM BD500-A-J06-10-00-00AAA-030A-A, Figure 1 sheet
            // 2): a straight 29.5 deg from the side of the body to the
            // winglet on both wings. No quarter-chord value is published;
            // with this taper the outboard quarter chord is 27.0 deg.
            sweep_deg: 29.5,
            tip_twist_deg: -1.5,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: 38.70,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        airfoil_class_source: "Design-era declaration: the A220-300 (CSeries, EIS 2016) wing is of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC2-0714 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Puts the model's quarter-MAC point on the published one
                // (LEMAC 16.535 m + 0.25 x 3.781 m reference chord, the
                // planning MAC reference below); the model MAC is 0.8 %
                // shorter, so its own LEMAC sits 0.09 m aft of 16.535 m.
                root_datum_x_m: 12.760,
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
                airfoil_class: crate::AirfoilClass::Supercritical,
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // ACP Issue 013: 36.6 m^2 tailplane area (Table 6), 12.263 m
                // span (locator D, derived), tip trailing edge 37.82 m aft of
                // the nose (H) and 4.94 m aft of the root leading edge (V,
                // role read off the drawing), at the previous 0.2895 taper.
                hstab_offset_from_tail_m: 5.82,
                hstab_z_m: 0.7,
                hstab_root_chord_m: 4.629,
                hstab_tip_chord_m: 1.340,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (3.60, 6.1315, 0.5),
                // ACP Issue 013 Table 6: 28.2 m^2 fin area, a theoretical
                // area with the leading and trailing edges extended down to
                // the fuselage axis, as the same table states the wing area
                // including the part within the fuselage. Over the 8.05 m
                // axis-to-tip height at 0.30 taper (axis chord 5.39 m) the
                // panel from the crown is 19.0 m^2. The tip stands at
                // 11.578 m (locator C), 6.193 m above the 5.385 m fuselage
                // top of the clearance table, so the root line is the crown
                // and the chords are the crown and tip chords of that
                // trapezoid; the builder carries the edges on down to the
                // tail cone under the root.
                vstab_offset_from_tail_m: 6.0,
                vstab_z_m: 1.9605,
                vstab_root_chord_m: 4.516,
                vstab_tip_chord_m: 1.616,
                vstab_tip_le_m: (4.5, 0.0, 6.193),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 3.50,
                // ACP Issue 013 Figure 1 / table 5, locator B: 146.5 in body
                // height.
                height_m: Some(3.721),
                nose_z_m: -0.2,
                cabin_start_x_m: 3.2,
                cabin_z_m: 0.1,
                tailcone_length_m: 7.0,
                tail_z_m: 0.8,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // ACP Issue 013 locators P, Q and BB: engine centreline
                // P - (P - BB/2 - Q)/2 = 5.44 m outboard (derived, +-0.15 m).
                spanwise_positions_m: vec![5.44, -5.44],
                // ACP nacelle clearance 22.9 in (0.582 m) minimum under the
                // 1.08 m nacelle radius.
                z_m: -1.410,
                // Locator K table value: inlet 12.17 m aft of the nose (the
                // drawn K is not to scale).
                inlet_x_offset_m: 3.668,
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
            num_passengers: 140,
            cargo_payload_kg: 15_000.0,
            // Airbus recovery publication: MZFW 55,792 kg less OEW 37,149 kg.
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
        performance: crate::presets::high_lift("modern_narrowbody"),
    }
}
