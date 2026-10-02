// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Boeing 787-9 and McDonnell Douglas DC-10 presets.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MissingDesignMissionDatum, MissionEvidenceApplicability,
    PartialDesignMissionEvidence, PartialMissionEvidenceKind, PublishedAftCgNoseLoad, WingConfig,
};

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
            // ICAO Annex 14 Vol. I Table 1-1 (aerodrome reference code) applied to the
            // preset wingspan of 60.12 m (Boeing 787-9 airport-planning dimensions):
            // 52 m <= b < 65 m is code E.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::E),
            mrw_kg: Some(255_372.0),
            mtow_kg: Some(254_692.0),
            mlw_kg: Some(192_776.0),
            mzfw_kg: Some(181_436.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("B787-9"),
            usable_fuel_volume_l: Some(126_429.0),
            usable_fuel_mass_kg: Some(101_522.0),
            fuel_density_kg_l: Some(101_522.0 / 126_429.0),
            reference_wing_area_m2: Some(360.464),
            planning_seats: Some(290),
            design_point: Some(crate::PayloadRangeDesignPoint {
                range_nmi: 5_300.0,
                payload_kg: Some(52_586.0),
                source: "Boeing 787 Airplane Characteristics for Airport Planning Rev Q section 3.2.2 (payload/range long-range cruise, 787-9 typical engines): maximum-zero-fuel-weight corner read as 5,300 nmi (+-60 nmi); payload is the preset MZFW minus its reference OEW; reserves not stated",
            }),
            certified_max_seats: Some(420),
            certified_exit_layout: Some(crate::presets::B787_9_EXIT_LAYOUT),
            planning_cabin: Some(crate::presets::B787_9_PLANNING_CABIN),
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
            // 1 - 2 x 259,574 lb / 563,000 lb: both main-gear struts at the
            // most-aft CG; the table does not print that CG.
            aft_cg_nose_load: Some(PublishedAftCgNoseLoad {
                mass_kg: 255_372.0,
                nose_gear_fraction: 1.0 - 2.0 * 259_574.0 / 563_000.0,
                aft_cg_pct_mac: None,
                source: "Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 7.3 (787-9 at maximum design taxi weight 563,000 lb: 259,574 lb static per main-gear strut at the most-aft CG)",
            }),
            sources: vec![
                "Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 2",
                "Boeing 787 ARFF composite-content diagram, p.3: https://www.boeing.com/content/dam/boeing/v2/airports/arff/787_composite_arff_data_2025.pdf",
                crate::preset_structures::TRANSPORT_CAP_SOURCE,
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
            // Holds the 9.80 m track on the 5.77 m body width.
            track_diameter_factor: 9.8 / 5.77,
            // D6-58333 Rev P 2.2.2: nose-NLG 5.41 m, wheelbase 25.83 m, track 9.80 m.
            reference_wheelbase_m: Some(25.83),
            reference_track_m: Some(9.80),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(62.00),
            reference_nlg_x_fraction: Some(5.41 / 62.00),
            reference_mlg_x_fractions: Some(vec![31.24 / 62.00, 31.24 / 62.00]),
            // D6-58333 Rev Q section 2.3.2 (PDF p.25, ground clearances):
            // fuselage bottom ahead of the wing (D) 1.75 m minimum.
            fuselage_ground_clearance_m: Some(1.75),
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 60.12,
            root_chord_m: 12.60,
            break_chord_m: 6.50,
            tip_chord_m: 1.60,
            // The 32.2 deg quoted for the 787-9 (32 deg in NASA/TP-20210023843,
            // December 2022, Table I) is the outboard quarter-chord sweep:
            // the outboard taper converts it to this leading-edge angle, and
            // the straight 34.7 deg leading edge of the D6-58333 Rev Q
            // section 2.2.2 plan view confirms it.
            sweep_deg: 34.714_008_340_548_3,
            tip_twist_deg: -2.0,
            wing_x_shift_m: -1.5,
            tail_scale: 1.0,
            // EASA TCDS: 62.0014 m body length. The 62.81 m (206 ft 1 in) of
            // D6-58333 Rev Q section 2.2.2 ends at the tailplane tip trailing
            // edge, behind the tail cone.
            fuselage_length_m: 62.00,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        airfoil_class_source: "Design-era declaration: the 787-9 (787 EIS 2011, -9 2014) wing is of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC(2)-0612 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Quarter-MAC: D6-58333 7.4.2 %MAC/main-gear-load two-point statics
                // give LEMAC 27.784 m aft of nose (TCDS MAC 6.271 m; model MAC 20% longer).
                root_datum_x_m: 21.552,
                root_z_m: -1.8,
                break_z_m: -0.2,
                tip_z_m: 2.5,
                root_twist_deg: 3.5,
                break_twist_deg: 1.5,
                break_span_fraction: 0.35,
                kink_span_fraction: Some(0.353_771_245_388_011_8),
                outboard_sweep_decrement_deg: 2.0,
                // Boeing publishes no 787 sections. The comparable public
                // M 0.85 widebody wing, the NASA Common Research Model
                // (Vassberg et al., AIAA 2008-6919, Table 2), runs from t/c
                // 0.154 at the centreline and 0.138 at the side of body to
                // 0.105 at the 37 % yehudi break and 0.095 at the tip: 0.115
                // area-weighted over the whole wing. This loft holds the
                // root section to the kink, so a 14 % root would carry 0.14
                // to the kink (0.133 overall); the 12 % section lands the
                // loft on the CRM mean, with the 10 % tip section close to
                // its 9.5 %.
                root_airfoil: "sc20612".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                airfoil_class: crate::AirfoilClass::Supercritical,
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // D6-58333 Rev Q section 2.2.2 (PDF p.22): tailplane span
                // 65 ft 0 in (19.81 m) with its tip trailing edge at the
                // 206 ft 1 in (62.81 m) overall length; this root station
                // puts it there. Chords and sweep are not dimensioned.
                hstab_offset_from_tail_m: 6.99,
                hstab_z_m: 1.0,
                hstab_root_chord_m: 6.5,
                hstab_tip_chord_m: 1.8,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (6.0, 9.905, 0.8),
                // No fin station is published: the offset holds the fin root
                // leading edge at 52.31 m on the 62.00 m body. The root line
                // is the crown of the 5.94 m body, the tip at the 16.81 m
                // minimum fin-top height of section 2.3.2 (PDF p.25, N), and
                // the builder carries the edges on down to the tail cone
                // under the root. Chords are not dimensioned.
                vstab_offset_from_tail_m: 9.69,
                vstab_z_m: 3.17,
                vstab_root_chord_m: 8.0,
                vstab_tip_chord_m: 2.8,
                vstab_tip_le_m: (7.5, 0.0, 9.12),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                // D6-58333 Rev Q section 2.2.2 (PDF p.22): body width 18 ft
                // 11 in and height 19 ft 6 in.
                diameter_m: 5.77,
                height_m: Some(5.94),
                nose_z_m: -0.4,
                // Generic nose-taper and tail-cone lengths of the outer mould
                // line: the ACAP prints no flight-deck or aft
                // pressure-bulkhead station. They shape the body only; the
                // passenger cabin is bounded by the section 2.7.1 door
                // stations (`B787_9_EXIT_LAYOUT`).
                cabin_start_x_m: 5.5,
                cabin_z_m: 0.2,
                tailcone_length_m: 12.0,
                tail_z_m: 1.5,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // Boeing 787 ACAP D6-58333 Rev Q, October 2025, section
                // 2.2.2 (PDF p.22, General Dimensions: Model 787-9) plan
                // view: engine centreline 32 ft 6 in (9.91 m) from the
                // airplane centreline, nacelle inlet face 68 ft 3 in
                // (20.80 m) aft of the nose tip.
                spanwise_positions_m: vec![9.91, -9.91],
                // Section 2.3.2 (PDF p.25): GEnx nacelle low point (F) 0.69 m
                // above the ground under the 1.70 m nacelle radius.
                z_m: -1.821,
                // Puts that inlet face 20.80 m aft of the nose: the 20.052 m
                // root leading edge (root datum plus wing shift) plus the
                // 6.866 m the planform leading edge runs aft by y = 9.91 m,
                // less this offset.
                inlet_x_offset_m: 6.118,
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
        performance: crate::presets::high_lift("advanced_highlift_widebody"),
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
            // ICAO Annex 14 Vol. I Table 1-1 (aerodrome reference code) applied to the
            // preset wingspan of 50.39 m (McDonnell Douglas DC-10-30): 36 m <= b < 52 m
            // is code D.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::D),
            // The ACAP 572,000 lb footnote restates no ramp limit or OEW; the
            // standard-row values (253,105 kg MRW) are not mixed in here.
            mrw_kg: None,
            mtow_kg: Some(259_454.0),
            mlw_kg: Some(190_962.0),
            mzfw_kg: Some(166_922.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("DC-10"),
            usable_fuel_volume_l: Some(137_509.0),
            usable_fuel_mass_kg: Some(111_387.0),
            reference_wing_area_m2: Some(338.84),
            planning_seats: Some(255),
            design_point: Some(crate::PayloadRangeDesignPoint {
                range_nmi: 4_120.0,
                payload_kg: Some(45_993.0),
                source: "Boeing DC-10 Airplane Characteristics for Airport Planning section 3.2.1 p.54 (payload/range M0.82 step cruise, Series 30, 572,000 lb MTOGW curve): maximum-payload corner read as 4,120 nmi at 101,396 lb (45,993 kg); read uncertainty +-100 nmi; reserves per FAR 121.645 at a 200 nmi alternate",
            }),
            certified_max_seats: Some(399),
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            aft_cg_nose_load: Some(PublishedAftCgNoseLoad {
                mass_kg: 259_454.0,
                nose_gear_fraction: 0.057,
                aft_cg_pct_mac: None,
                source: "Boeing DC/MD-10 ACAP DAC-67803A Rev A, Figure 7.4.2 (572,000 lb option: 5.7 % of weight on the nose gear at the most-aft CG)",
            }),
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
            mlg_strut_bogie_wheels: Some(vec![4, 4, 2]),
            wheels_per_mlg_strut: 0,
            track_diameter_factor: 1.77,
            // DAC-67803A Rev A, Figure 7.2.2 (footprint, Series 30): nose
            // gear to wing-gear bogie centers 72 ft 4.6 in (22.06 m), center
            // gear axle 30 in (0.76 m) aft of them, wing-gear track 35 ft
            // (10.67 m). Figure 2.2 (Series 30 side view, printed length
            // 181 ft 7.2 in, 55.35 m) places the nose gear 8.1 m aft of the
            // nose tip, scaled against its own 22.07 m wheelbase dimension
            // (+-0.2 m, a drawing read).
            reference_wheelbase_m: Some(22.06),
            reference_body_wheelbase_m: Some(22.82),
            reference_track_m: Some(10.67),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(55.35),
            reference_nlg_x_fraction: Some(8.1 / 55.35),
            reference_mlg_x_fractions: Some(vec![30.16 / 55.35, 30.16 / 55.35, 30.92 / 55.35]),
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 50.39,
            // The Douglas reference wing: the straight-tapered trapezoid of
            // the 338.84 m2 TCDS area (IM.A.210, 3,647.5 ft2) over this span
            // whose MAC is the 7.51 m weight-and-balance chord (NASA
            // CR-3677, 1984: Re 6.95e6 at 19.7e6 /m on the 4.7 % model).
            // Area, span and MAC fix the taper at 0.2562; the 2.74 m tip
            // chord agrees with DAC-67803A Rev A Figure 2.2 (about 2.7 m,
            // drawing read). The inboard trailing-edge extension of the real
            // planform lies outside the reference wing and is not modelled.
            root_chord_m: 10.705_424_735_302_275,
            break_chord_m: 7.918_672_469_211_44,
            tip_chord_m: 2.743_275_403_614_176,
            // NASA CR-3119 (1979) specifies 35 deg at quarter chord; the
            // reference taper converts it to this leading-edge angle.
            sweep_deg: 37.926_177_529_939_146,
            tip_twist_deg: -2.0,
            wing_x_shift_m: -3.25,
            tail_scale: 1.0,
            // DAC-67803A Rev A Figure 2.2: printed 181 ft 7.2 in, the length
            // the gear station fractions above are normalized to.
            fuselage_length_m: 55.35,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        airfoil_class_source: "Design-era declaration: the DC-10 wing was designed in 1966-1968 (EIS 1971), before supercritical sections, first flown on the NASA F-8 SCW in 1971, reached transport service, so its technology is conventional (NACA 6-series era). The drawn root and tip stations are the DSMA-523A surrogate (UIUC Airfoil Coordinates Database; 11.0 % t/c), the nearest published member of the Douglas Santa Monica series the DC-10 used (DSMA-496, -521 and -522 at the root, -519 and -520 at the tip; Lednicer, UIUC Incomplete Guide to Airfoil Usage; coordinates unpublished); it fixes thickness and shape only, its aft loading is not credited in the Korn factor, and it does not change this declaration.",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Wing-root leading edge at the centreline 19.33 m aft of the
                // nose (19.26 m on the 55.35 m drawing): the straight leading
                // edge of DAC-67803A Figure 2.2 meets the fuselage side at
                // 21.6 m. With the 3.25 m design-vector shift this places
                // LEMAC at 27.21 m, against 27.23 m from the Figure 7.4.2
                // nose-gear point (chart-derived, +-0.3 m).
                root_datum_x_m: 22.58,
                root_z_m: -1.6,
                break_z_m: -0.3,
                tip_z_m: 2.0,
                root_twist_deg: 4.0,
                break_twist_deg: 1.5,
                break_span_fraction: 0.35,
                // The reference trapezoid's own chord at the side of body,
                // pinned independently of the automatic clipping rule.
                side_of_body_chord_ratio: Some(0.925_625_096_354_822),
                kink_span_fraction: Some(0.35),
                outboard_sweep_decrement_deg: 2.0,
                // Section family: Douglas Santa Monica sections, DSMA-496,
                // -521 and -522 at the root and DSMA-519 and -520 at the tip
                // (D. Lednicer, "The Incomplete Guide to Airfoil Usage",
                // UIUC Airfoil Data Site, entry "Douglas DC-10-30"). Their
                // coordinates are not published. Thickness: 11.0 % average
                // t/c for the DC-10-10 and -30 (L. R. Jenkinson, P. Simpkin
                // and D. Rhodes, "Civil Jet Aircraft Design", Arnold, 1999,
                // Data A, Table 6). No published root, kink or tip t/c was
                // found, so the wing carries one section at that average:
                // DSMA-523A (UIUC Airfoil Coordinates Database, "McDonnell/
                // Douglas DSMA-523 transonic airfoil with sharp trailing
                // edge"), the nearest member of the same Douglas series in
                // the library, whose maximum t/c is 0.110.
                root_airfoil: "dsma523a".to_owned(),
                tip_airfoil: "dsma523a".to_owned(),
                // Korn class: the DSMA aft-loaded sections have no sourced
                // Korn kappa, so the conventional class (0.87) is the sourced
                // lower bound (Mason, ch. 7; Malone & Mason 1995).
                airfoil_class: crate::AirfoilClass::Conventional,
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // DAC-67803A Rev A Figure 2.2 (Series 30 plan view, 5 ft
                // grid): tailplane span 71 ft 2 in (21.69 m, printed); chords
                // 8.93 m at the centreline and 3.07 m at the tip, leading edge
                // swept 40.9 deg, root leading edge 13.58 m forward of the
                // tail tip on the 55.35 m drawing (drawing reads, +-0.2 m).
                // Gross area 130.1 m2.
                hstab_offset_from_tail_m: 13.63,
                hstab_z_m: 1.0,
                hstab_root_chord_m: 8.93,
                hstab_tip_chord_m: 3.07,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (9.39, 10.845, 0.8),
                // No fin station is published. The offset keeps the root
                // chord over the body: where the fin meets the tail cone and
                // the centre-engine nacelle, 0.24 m above the line below, its
                // trailing edge ends 0.03 m short of the tail tip, where the
                // nacelle ends too.
                vstab_offset_from_tail_m: 10.55,
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
                // tail engine correctly; whatever draws it has to know. The
                // underwing pair sits 26 ft 10 in (8.18 m) from the centreline
                // on the DAC-67803A Rev A Figure 2.2 plan view (reference line
                // not stated).
                spanwise_positions_m: vec![8.18, -8.18, 0.0],
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
            num_passengers: 255,
            cargo_payload_kg: 65_000.0,
            // ACAP 572k planning option: MZFW 166,922 kg less OEW 120,914 kg.
            max_structural_payload_kg: 46_008.0,
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("advanced_highlift_widebody"),
    }
}
