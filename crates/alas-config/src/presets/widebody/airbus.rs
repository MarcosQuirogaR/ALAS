// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airbus twin-aisle presets: the A340-300 here, the A380-800 in its own
//! module.

mod a380;

pub use a380::a380_800;

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MissingDesignMissionDatum, MissionEvidenceApplicability,
    PartialDesignMissionEvidence, PartialMissionEvidenceKind, PublishedAftCgNoseLoad, WingConfig,
};

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
            // ICAO Annex 14 Vol. I Table 1-1 (aerodrome reference code) applied to the
            // preset wingspan of 60.30 m (Airbus A340-300 Aircraft Characteristics):
            // 52 m <= b < 65 m is code E.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::E),
            mrw_kg: Some(260_900.0),
            mtow_kg: Some(260_000.0),
            mlw_kg: Some(188_000.0),
            mzfw_kg: Some(178_000.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("A340-300"),
            usable_fuel_volume_l: Some(141_500.0),
            usable_fuel_mass_kg: Some(113_200.0),
            fuel_density_kg_l: Some(0.8),
            planning_seats: Some(335),
            design_point: Some(crate::PayloadRangeDesignPoint {
                range_nmi: 5_000.0,
                payload_kg: Some(50_800.0),
                source: "Airbus A340-200/-300 Aircraft Characteristics Rev 33 (2025-12) section 3-2-1 Figure 3-2-1-991-013-A01 (payload/range ISA, CFM56-5C3, A340-300): maximum-payload corner read as 5,000 nmi at 112,000 lb (50.8 t); read uncertainty +-100 nmi, +-1,000 lb; chart MTOW not printed; reserves not stated",
            }),
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
            // 1 - (2 x 102,950 + 41,120) / 260,900: two wing-gear legs and
            // the centre gear at the most-aft CG.
            aft_cg_nose_load: Some(PublishedAftCgNoseLoad {
                mass_kg: 260_900.0,
                nose_gear_fraction: 1.0 - (2.0 * 102_950.0 + 41_120.0) / 260_900.0,
                aft_cg_pct_mac: Some(38.0),
                source: "Airbus A340-200/-300 Aircraft Characteristics Rev 33, 2025-12-01, Figure 7-3-0-991-007-A01 sheet 2 (WV029, MRW 260,900 kg: wing-gear 102,950 kg per strut and centre-gear 41,120 kg static at the most-aft CG, 38 % MAC)",
            }),
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
            // AC 2-9-0: two 4-wheel wing bogies + 1 twin-wheel centreline gear.
            mlg_strut_bogie_wheels: Some(vec![4, 4, 2]),
            wheels_per_mlg_strut: 0,
            track_diameter_factor: 10.684 / 5.64,
            // AC 7-2-0 wheelbase/track; AC 2-2-0 nose-tip stations normalized.
            reference_wheelbase_m: Some(25.375),
            reference_track_m: Some(10.684),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(63.66),
            reference_nlg_x_fraction: Some(6.67 / 63.66),
            reference_mlg_x_fractions: Some(vec![
                32.05 / 63.66,
                32.05 / 63.66,
                33.04 / 63.66,
            ]),
            // Airbus A340-200/-300 AC Rev 33, Figure 2-3-0-991-005-A01 (PDF
            // p.44, ground clearances, aft CG): fuselage bottom ahead of the
            // wing (F2) 2.13 m; the 1.83 m BF is the belly fairing.
            fuselage_ground_clearance_m: Some(2.13),
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 60.30,
            root_chord_m: 12.00,
            break_chord_m: 6.50,
            // Airbus A340-200/-300 Aircraft Characteristics Rev 33,
            // 2025-12-01, FIGURE-2-2-0-991-007-A01 sheet 2 (PDF p.40,
            // A340-300 plan view): the wing tip runs 2.5 m streamwise from
            // its leading edge, 39.1 m aft of the nose, to its trailing edge
            // at the winglet root.
            tip_chord_m: 2.5,
            // 30 deg is the quarter-chord sweep of the A330-200/300 wing the
            // A340-200/300 shares (NASA/TP-20210023843, December 2022,
            // Table I). The outboard taper converts it to this leading-edge
            // angle, which matches the 32.0 deg the same plan view reads at
            // the leading edge.
            sweep_deg: 32.037_361_654_529_35,
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
                // Quarter-MAC: LEMAC 28.083 m aft of nose (section 7 pavement-load
                // two-point statics, 13 rows within +-0.014 m; TCDS MAC 7.270 m).
                // The model quarter-MAC point sits on the manufacturer's
                // 29.901 m (model MAC 7.093 m, 2.4 % short of the TCDS).
                root_datum_x_m: 22.341,
                // Heights fitted to the Figure 2-3-0-991-005-A01 static
                // clearances (aft CG), the drooped ground shape: wing tip
                // lower surface 5.94 m above the ground (W2), and the kink
                // height that, with one engine offset, puts the nacelle low
                // points at N1 1.28 m and N2 2.35 m.
                root_z_m: -1.8,
                break_z_m: -0.922,
                tip_z_m: 1.315,
                root_twist_deg: 3.5,
                break_twist_deg: 1.5,
                break_span_fraction: 0.35,
                // Active side-of-body planform calibrated to the Airbus
                // 361.6 m^2 reference area (section 2-1-1) without changing
                // the chords: with the 12.0 m root, 6.5 m kink and 2.5 m tip
                // chords over the 30.15 m semispan this puts the kink at
                // 9.50 m, where the same plan view draws the trailing-edge
                // break (about 8.5-9.2 m, read against the 7.46 m and
                // 10.88 m flap-track dimensions).
                kink_span_fraction: Some(9.5 / 30.15),
                outboard_sweep_decrement_deg: 2.0,
                root_airfoil: "sc20612".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // Figure 2-2-0-991-007-A01 sheet 2 (plan view): tailplane tip
                // leading edge 61.67 m aft of the nose and 6.43 m aft of the
                // root leading edge (55.24 m), tip trailing edge 63.69 m. The
                // root chord and height are not dimensioned.
                hstab_offset_from_tail_m: 8.42,
                hstab_z_m: 1.0,
                hstab_root_chord_m: 6.5,
                hstab_tip_chord_m: 2.02,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                // Airbus A340 Aircraft Characteristics, general dimensions:
                // 19.4 m full horizontal-tail span.
                hstab_tip_le_m: (6.43, 9.7, 0.8),
                // Sheet 1 (side view): fin root leading edge 52.42 m aft of
                // the nose, a 7.78 m root chord (role read off the drawing,
                // low confidence), tip leading edge 8.14 m aft of the root,
                // tip chord 62.84 - 60.56 m, and 8.3 m of fin above the
                // local fuselage top. The root keeps its estimated height and
                // the span holds the tip 8.3 m above the crown. The level
                // model stands 16.07 m tall against the 16.67 m fin-top
                // clearance, which the about 1 deg nose-down MRW attitude of
                // Figure 2-3-0-991-005-A01 raises by about 0.6 m.
                vstab_offset_from_tail_m: 11.24,
                vstab_z_m: 1.8,
                vstab_root_chord_m: 7.78,
                vstab_tip_chord_m: 2.28,
                vstab_tip_le_m: (8.14, 0.0, 9.52),
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
                // Figure 2-2-0-991-007-A01 sheet 1 (front view): engine
                // centrelines 18.74 m and 38.54 m apart.
                spanwise_positions_m: vec![9.37, -9.37, 19.27, -19.27],
                // Figure 2-3-0-991-005-A01 (aft CG): nacelle low points N1
                // 1.28 m and N2 2.35 m above the ground under the 1.1 m
                // nacelle radius.
                z_m: -1.436,
                // Sheet 2 inlets 22.39 m and 28.96 m aft of the nose; the mean
                // of the two offsets from the leading edge leaves each inlet
                // within 0.19 m.
                inlet_x_offset_m: 4.227,
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
            num_passengers: 335,
            cargo_payload_kg: 45_000.0,
            // Maximum zero-fuel weight 178.0 t less an operating empty weight
            // of about 129.4 t.
            max_structural_payload_kg: 48_600.0,
            dive_speed_m_s: 200.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("advanced_highlift_widebody"),
    }
}
