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
    PartialDesignMissionEvidence, PartialMissionEvidenceKind, WingConfig,
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
            certified_exit_layout: Some(crate::presets::A340_300_EXIT_LAYOUT),
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
            aft_cg_nose_load: Some(crate::presets::gear_load::A340_300),
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
            takeoff_stabilizer_nose_up_deg: Some(crate::landing_gear::TRANSPORT_THS_TAKEOFF_NOSE_UP_DEG),
            // Published tail-strike attitude: 10.1 deg pitch to ground contact with
            // the main gear compressed (Airbus, "Avoiding Tail Strike", Operational
            // Liaison Meeting, FBW; NTSB docket attachment "Airbus Material -
            // Avoiding Tail Strike", PDF p.12). The generic tailcone loft
            // understates the geometric tail-down angle, so it is the floor.
            min_tip_back_deg: 10.1,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 60.30,
            root_chord_m: 12.2600, // geometry round 2 (wing lane); was 12.00. Final merge: the step B w1 drawn inboard TE (14.679 m) is REVERTED, see below
            break_chord_m: 7.7000, // geometry round 2 (wing lane): the sourced straight-edge tip chord is kept; this kink chord closes the published reference area behind the swept-back tip. Final merge: step B w1 drawn value 7.48 m reverted
            // Airbus A340-200/-300 Aircraft Characteristics Rev 33,
            // 2025-12-01, FIGURE-2-2-0-991-007-A01 sheet 2 (PDF p.40,
            // A340-300 plan view): the wing tip runs 2.5 m streamwise from
            // its leading edge, 39.1 m aft of the nose, to its trailing edge
            // at the winglet root.
            tip_chord_m: 2.35, // chord the straight edges reach at the tip (the extreme tip chord is the rake chord below)
            // 30 deg is the quarter-chord sweep of the A330-200/300 wing the
            // A340-200/300 shares (NASA/TP-20210023843, December 2022,
            // Table I). The outboard taper converts it to this leading-edge
            // angle, which matches the 32.0 deg the same plan view reads at
            // the leading edge.
            sweep_deg: 32.06841507580666, // geometry round 2 (wing lane): the published c/4 sweep re-converted through the closed kink chord (was 32.03736165452935)
            tip_twist_deg: -2.0,
            wing_x_shift_m: -1.4,
            tail_scale: 1.0,
            fuselage_length_m: 63.66,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        airfoil_class_source: "Design-era declaration: the A340-300 (EIS 1993) shares the A330 wing, of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC(2)-0612 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values (geometry freeze 2026-10-12, owner decision); the drawn planform stays the geometry.
                // S 361.6 m2: Airbus A340-200/-300 Aircraft Characteristics section 2-1-1 and Jane's 2008-09 (gross wing 361.60 m2); MAC 7.270 m: EASA TCDS (as recorded below);
                // LEMAC 28.083 m: section 7 pavement-load two-point statics (a derived value, 13 rows within +-0.014 m).
                reference_wing: crate::ReferenceWingConfig::published(361.6, Some(7.27), Some(28.083)),
                // Quarter-MAC: LEMAC 28.083 m aft of nose (section 7 pavement-load
                // two-point statics, 13 rows within +-0.014 m; TCDS MAC 7.270 m).
                // The model quarter-MAC point sits on the manufacturer's
                // 29.901 m (model MAC 7.093 m, 2.4 % short of the TCDS).
                root_datum_x_m: 22.3680, // geometry round 2 (wing lane): was 22.341; keeps the published quarter-MAC station with the swept-back tip, the closed kink chord and the re-converted sweep
                // Heights fitted to the Figure 2-3-0-991-005-A01 static
                // clearances (aft CG), the drooped ground shape: wing tip
                // lower surface 5.94 m above the ground (W2), and the kink
                // height that, with one engine offset, puts the nacelle low
                // points at N1 1.28 m and N2 2.35 m.
                root_z_m: -1.8000, // static ground shape (clearance fit) kept; the drawing fit is the flight shape (geometry round 2)
                break_z_m: -1.1100, // ground shape kept (kink moved inboard to 8.3 m; interpolated)
                tip_z_m: 1.2296, // ground shape kept; 1.3150 moved by half the tip thickness x chord change so the published tip clearance holds
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
                // FINAL MERGE (DECISIONS: A340 inboard TE edit is kept only if reference_wing = Trapezoid reproduces the published 361.6 m2 within 1 %): the drawn
                // inboard TE (root chord 14.679 m, kink 9.2 m, SOB ratio 12.323/14.679) gave drawn S 411.1 m2 and the kink-to-rake-start trapezoid 366.8 m2 (+1.45 % > 1 %),
                // so the planform edit is reverted to the round-2 values (S_ref 361.6 m2 by the area-calibrated crank). Drawn TE evidence: overlay wing mismatch only 23.1 -> 20.9 m2.
                kink_span_fraction: Some(8.3 / 30.15), // step B w1 tried 9.2 / 30.15
                outboard_sweep_decrement_deg: 2.0,
                root_airfoil: "sc20612".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                airfoil_class: crate::AirfoilClass::Supercritical,
                                // Thickness profile (geometry round 2): the NASA Common Research Model t/c 0.154 at the
                // centreline, 0.138 at the side of body, 0.105 at the yehudi break and 0.095 at the tip
                // (Vassberg, DeHaan, Rivers and Wahls, AIAA 2008-6919, Table 2) is the public transport-wing
                // distribution; the multipliers below act on the root-kink-tip section blend above.
                thickness_stations: vec![
                    crate::WingThicknessStation {
                        span_fraction: 0.0000,
                        thickness_scale: 1.2486, // t/c 0.1200 -> 0.1498
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.1000,
                        thickness_scale: 1.1189, // t/c 0.1200 -> 0.1342
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.3151, // final merge: step B w1 0.3051 reverted with the kink
                        thickness_scale: 0.8513, // t/c 0.1200 -> 0.1021
                    },
                    crate::WingThicknessStation {
                        span_fraction: 1.0000,
                        thickness_scale: 0.9278, // t/c 0.0996 -> 0.0924
                    },
                ],
                // Swept-back tip (geometry round 2): the leading and trailing edges follow the straight lines to the
                // design tip chord; outboard of the rake start the leading edge sweeps further aft so that the chord at the
                // extreme tip is 0.86 m (silhouette of the drawing 0.15 m inboard of the tip), the trailing edge staying one line.
                // The design tip chord closes the published reference area including the rake.
                tip_rake_tip_chord_m: Some(0.86),
                tip_rake_start_span_fraction: Some(0.9620),
                // Step B (wing profile splines, loft-wing-2026-10-11): C2 cubic spline through the dihedral, twist and thickness stations; was Linear (the default).
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                // Flight-shape dihedral (geometry round 2): the front-view drawing shows the wing higher than the static ground
                // shape fitted to the published clearances (the airport-planning figures are maximum-ramp-weight states), so the
                // ground heights stay and each station carries the rise to the shape fitted to the drawing.
                flight_root_rise_m: Some(0.7182),
                // The drawn and tabulated nacelle heights are the static ones: keep them in the flight shape.
                nacelles_hold_ground_height: true,
                flight_tip_rise_semispan_fraction: Some(0.013999),
                dihedral_stations: vec![
                    crate::WingDihedralStation {
                        span_fraction: 0.1000,
                        z_m: -1.5493,
                        flight_rise_m: Some(0.7461),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.2000,
                        z_m: -1.2986,
                        flight_rise_m: Some(0.7989),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.3151, // final merge: step B w1 0.3051 reverted with the kink
                        z_m: -1.0100,
                        flight_rise_m: Some(0.8675),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.5000,
                        z_m: -0.4054,
                        flight_rise_m: Some(0.7478),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.6500,
                        z_m: 0.0851,
                        flight_rise_m: Some(0.6303),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.8000,
                        z_m: 0.5756,
                        flight_rise_m: Some(0.5168),
                    },
                ],
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // Figure 2-2-0-991-007-A01 sheet 2 (plan view): tailplane tip
                // leading edge 61.67 m aft of the nose and 6.43 m aft of the
                // root leading edge (55.24 m), tip trailing edge 63.69 m. The
                // root chord and height are not dimensioned.
                // geo/tailfit 2026-10-11: tailplane straight-edge fit to the drawing, centreline values; was off 8.42, chords 6.5/2.02, tip (6.43, 9.7, 1.080)
                hstab_offset_from_tail_m: 8.4,
                hstab_z_m: 1.510, // geometry round 2 (wing lane): upper surface at the root matched to the front view; was 1.0
                hstab_root_chord_m: 5.36,
                hstab_tip_chord_m: 2.0,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                // Airbus A340 Aircraft Characteristics, general dimensions:
                // 19.4 m full horizontal-tail span.
                hstab_tip_le_m: (6.43, 9.7, 1.080), // tailplane dihedral matched to the front view; was z 0.8
                // Sheet 1 (side view): fin root leading edge 52.42 m aft of
                // the nose, a 7.78 m root chord (role read off the drawing,
                // low confidence), tip leading edge 8.14 m aft of the root,
                // tip chord 62.84 - 60.56 m, and 8.3 m of fin above the
                // local fuselage top, so the root line is the crown; the
                // builder carries the edges on down to the tail cone under
                // the root. The level model stands 16.07 m tall against the
                // 16.67 m fin-top clearance, which the about 1 deg nose-down
                // MRW attitude of Figure 2-3-0-991-005-A01 raises by about
                // 0.6 m.
                // geo/tailfit 2026-10-11: fin straight-edge fit to the drawing at the root line (rms 0.009/0.004 m); was off 11.24, chords 7.78/2.28, tip (8.14, 0.0, 8.3)
                vstab_offset_from_tail_m: 11.19,
                vstab_z_m: 3.02,
                vstab_root_chord_m: 7.79,
                // geo/close 2026-10-12: fin top 0.12 m higher (drawn top 0.14 m above the built one on the calibrated side sheet, ~0.05 m of it stroke); same straight LE/TE lines;
                // was tip chord 3.15, tip LE (7.99, 0, 8.3)
                vstab_tip_chord_m: 3.08,
                vstab_tip_le_m: (8.105, 0.0, 8.42),
                // geo/tailfit 2026-10-11: dorsal fillet fitted to the drawn LE (new, geometry only)
                vstab_dorsal_x_start_m: Some(50.97),
                vstab_dorsal_height_m: Some(1.45),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 5.64,
                nose_z_m: 0.25,
                cabin_start_x_m: 5.5,
                cabin_z_m: 0.2,
                tailcone_length_m: 12.0,
                tail_z_m: 1.5,
                // FUSELAGE-G2 shaped nose and aft body. Outline traced from the manufacturer drawing
                // (side: same 2-2-0 p2; top: Airbus A340-200/-300 AC (Dec 01/25) 2-2-0 p3),
                // calibrated to the drawn length; the nose law (windshield, radome, keel and plan
                // exponents) is a least-squares fit to the traced upper, lower and plan lines (RMS 0.087 m),
                // the aft law (belly ramp, crown drop, plan taper, closing section) to the traced aft lines
                // (RMS 0.124 m). Cabin start, tailcone length and every payload and mass station are unchanged:
                // only the outer loft moves. Section exponent 2.0 (ellipse): no drawing measures it.
                // Data quality: aft crown read by eye (fin root hides it).
                nose_length_m: Some(10.53),
                nose_windshield_angle_deg: Some(22.3),
                nose_crown_end_fraction: Some(0.87),
                nose_radome_length_fraction: Some(0.1),
                nose_keel_exponent: Some(2.81),
                nose_plan_exponent: Some(1.43),
                nose_section_exponent: Some(2.0),
                belly_upsweep_length_m: Some(19.51),
                aft_belly_exponent: Some(1.23),
                tail_end_z_m: Some(2.54),
                tail_end_width_m: Some(0.22),
                tail_end_height_m: Some(0.58),
                aft_plan_length_m: Some(18.41),
                aft_plan_exponent: Some(1.26),
                aft_crown_length_m: Some(8.27),
                aft_crown_exponent: Some(1.0),
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // Final merge: engines.json carries a V2 nacelle set (nacelle_shape_v2) that agrees with the measured length, diameter and inlet; was Legacy.
                nacelle_model: crate::NacelleModel::V2,
                // Figure 2-2-0-991-007-A01 sheet 1 (front view): engine
                // centrelines 18.74 m and 38.54 m apart.
                spanwise_positions_m: vec![9.37, -9.37, 19.27, -19.27],
                // Figure 2-3-0-991-005-A01 (aft CG): nacelle low points N1
                // 1.28 m and N2 2.35 m above the ground under the 1.1 m
                // nacelle radius.
                // Low point held with the drawn 1.175 m radius (nacelle lane: -1.436 ->
                // -1.361), plus the wing lane's re-fit (-1.436 -> -1.3492, +0.0868 m).
                z_m: -1.2742,
                // Sheet 2 inlets 22.39 m and 28.96 m aft of the nose; the mean
                // of the two offsets from the leading edge leaves each inlet
                // within 0.19 m.
                inlet_x_offset_m: 4.2648, // geometry round 2 (wing lane): was 4.227; the wing datum and sweep moved by +0.0378 m at the engine, this keeps the inlet station
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
