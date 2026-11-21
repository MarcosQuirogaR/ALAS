// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Boeing 747-400 preset.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, WingConfig,
};

use crate::presets::landing_reference::B747_400_REFERENCE_AREA_M2;
// Source key used in the comments below:
//   ACAP = Boeing 747-400/-400ER Airplane Characteristics for Airport Planning,
//   D6-58326-1 Rev F, December 2024 (PDF page = document page + 17 in sections
//   2 and 3); https://www.boeing.com/content/dam/boeing/v2/airports/acaps/747-400_Rev_F.pdf
//   "drawing read" = a dimension measured off the ACAP section 2.2.1 plan or
//   side view (Figure 2.2.1, document page 2-14) at the scale of its printed
//   dimensions, +-0.3 m.
// FAA TCDS A20WE was not retrieved; certified_max_seats 660: EASA TCDS A.196 items 18/20 (PDF p.18).

/// The main-deck doors: ten passenger doors, five each side (ACAP section
/// 2.7.1, p. 2-35), door centres 31 ft 2 in (9.50 m), 61 ft 8 in (18.80 m),
/// 100 ft 5 in (30.61 m), 133 ft 8 in (40.74 m) and 180 ft 11 in (55.14 m)
/// aft of the nose on the 225 ft 2 in (68.63 m) body. The 42 x 76 in clear
/// opening printed there is at least the Type A minimum of CS 25.807(a)
/// (42 x 72 in). The upper deck has one pair of Type A exits: EASA TCDS
/// IM.A.196 Issue 19, section 3 (747-400) III.20, "660 passengers with 5 pair
/// of Type A exits on main deck plus one pair of Type A exits on the upper
/// deck" (main deck limited to 550, upper deck to 110).
const B747_400_EXIT_LAYOUT: crate::CertifiedExitLayout = crate::CertifiedExitLayout {
    label: "A-A-A-A-A",
    pairs: &[
        crate::CertifiedExitPair {
            exit_type: "A",
            station_m: Some(9.50),
        },
        crate::CertifiedExitPair {
            exit_type: "A",
            station_m: Some(18.80),
        },
        crate::CertifiedExitPair {
            exit_type: "A",
            station_m: Some(30.61),
        },
        crate::CertifiedExitPair {
            exit_type: "A",
            station_m: Some(40.74),
        },
        crate::CertifiedExitPair {
            exit_type: "A",
            station_m: Some(55.14),
        },
    ],
    upper_deck_pairs: &[crate::CertifiedExitPair {
        exit_type: "A",
        station_m: None,
    }],
    station_body_length_m: Some(68.63),
    source: "Boeing 747-400 ACAP D6-58326-1 Rev F, December 2024, section 2.7.1 p. 2-35 (747-400 main deck doors 1-5 at 31-2/9.50, 61-8/18.80, 100-5/30.61, 133-8/40.74, 180-11/55.14 ft-in/m; 42 x 76 in openings); EASA TCDS IM.A.196 Issue 19 section 3 III.20 (one upper-deck Type A pair)",
};

/// The ACAP three-class cabin: 24 first, 32 business and 302 economy on the
/// main deck and 42 business on the upper deck (section 2.1.1, p. 2-2).
/// Pitches from the section 2.4.1 tri-class plan (p. 2-22): first 61 in
/// (1.55 m), business 39 in (1.00 m) on the main deck and 38 in (0.96 m) on
/// the upper deck, so 0.98 m for the 74 business seats (seat-weighted mean),
/// economy 32 in (0.81 m). Seat widths from the section 2.5 cross-sections
/// (pp. 2-28 to 2-30): first double 4 ft 9 in (1.45 m), business double 4 ft
/// 1 in (1.24 m), economy triple 4 ft 11.5 in (1.51 m), per seat; abreast
/// printed there: first 6, economy 10. Business abreast differs by deck (8
/// main, 4 upper) and is left to the floor width.
const B747_400_PLANNING_CABIN: crate::SourcedPlanningCabin = crate::SourcedPlanningCabin {
    classes: &[
        crate::SourcedSeatClass {
            class: "First",
            seats: 24,
            pitch_m: Some(1.55),
            abreast: Some(6),
            width_m: Some(0.725),
        },
        crate::SourcedSeatClass {
            class: "Business",
            seats: 74,
            pitch_m: Some(0.98),
            abreast: None,
            width_m: Some(0.62),
        },
        crate::SourcedSeatClass {
            class: "Economy",
            seats: 302,
            pitch_m: Some(0.81),
            abreast: Some(10),
            width_m: Some(0.503),
        },
    ],
    source: "Boeing 747-400 ACAP D6-58326-1 Rev F, December 2024, section 2.1.1 (400 three-class: 24 first, 32 business and 302 economy main deck, 42 business upper deck), section 2.4.1 pitches and section 2.5 seat widths",
};

/// First-generation four-engine twin-aisle airliner with the partial upper deck under its hump.
pub fn b747_400() -> AircraftPreset {
    AircraftPreset {
        name: "B747-400",
        display_name: "Boeing 747-400 (CF6-80C2B1F)",
        class: crate::presets::AircraftClass::Transport,
        description: "Boeing 747-400 passenger at the 875,000 lb takeoff weight with four CF6-80C2B1F engines.",
        identity: AircraftVariantIdentity {
            model: "747-400 passenger",
            weight_variant: "ACAP 875,000 lb MTOW column with optional 630,000 lb MLW and 542,500 lb MZFW",
            engine_model: "CF6-80C2B1F",
            modification_state: "D6-58326-1 Rev F section 2.1.1, fifth weight column, with the 3,300 US gal tail tank",
            tank_configuration: "57,065 US gal including the optional 3,300 US gal horizontal-stabiliser tank",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Vol. I Table 1-1 applied to the 64.44 m span
            // (ACAP section 2.2.1): 52 m <= b < 65 m is code E, although the
            // 64.92 m span at maximum gross weight and the FAA ADG class V
            // sit close to the code F limit (65 m).
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::E),
            // ACAP section 2.1.1, fifth weight column (all weights in lb and kg
            // as printed): MTW 877,000 lb = 397,800 kg.
            mrw_kg: Some(397_800.0),
            // Takeoff 875,000 lb = 396,893 kg.
            mtow_kg: Some(396_893.0),
            // Optional 630,000 lb landing weight = 285,763 kg.
            mlw_kg: Some(285_763.0),
            // Optional 542,500 lb zero-fuel weight = 246,073 kg.
            mzfw_kg: Some(246_073.0),
            // Spec operating empty weight 394,088 lb = 178,755 kg, three-class
            // 400-passenger arrangement (ACAP section 2.1.1 note 3).
            oew_kg: crate::oew_reference::preset_reference_oew_kg("B747-400"),
            // 57,065 US gal = 216,014 L = 382,335 lb = 173,459 kg (section
            // 2.1.1, optional tail fuel of 3,300 US gal included).
            usable_fuel_volume_l: Some(216_014.0),
            usable_fuel_mass_kg: Some(173_459.0),
            fuel_density_kg_l: Some(173_459.0 / 216_014.0),
            reference_wing_area_m2: Some(B747_400_REFERENCE_AREA_M2),
            // 24 first + 32 business + 302 economy on the main deck and 42
            // business on the upper deck (ACAP section 2.1.1 typical seating).
            planning_seats: Some(400),
            certified_max_seats: Some(660),
            planning_cabin: Some(B747_400_PLANNING_CABIN),
            certified_exit_layout: Some(B747_400_EXIT_LAYOUT),
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: None,
                cruise: None,
                range_nmi: 5_700.0,
                payload_kg: Some(67_318.0),
                source: "Boeing 747-400 ACAP D6-58326-1 Rev F section 3.2.1 p.3-2 (payload/range 0.85 Mach, CF6-80C2B1F, standard day): maximum-structural-payload corner, 542,500 lb zero-fuel weight (OEW 394,088 lb + 148,412 lb payload = 67,318 kg) meets the 875,000 lb brake-release line at a range read as 5,700 nmi (+-100 nmi); reserves FAR international, 10 % trip allowance, 200 nmi alternate, 30 min hold at 1,500 ft",
                reserve: Some(crate::presets::DesignPointReserve {
                    scheme: Some(crate::FuelScheme::FaaFlagSupplemental),
                    contingency_trip_fraction: None,
                    contingency_minimum_hold_min: None,
                    alternate_distance_nmi: Some(200.0),
                    final_reserve_hold_min: Some(30.0),
                    flag_flight_time_fraction: Some(0.10),
                    holding_altitude_ft: Some(1_500.0),
                    source: "Boeing 747-400 ACAP D6-58326-1 Rev F section 3.2.1 p 3-2 notes: FAR international reserves, 10% trip air time, 200-nmi alternative, 1/2 hour hold at 1,500 ft",
                }),
            }),
            // Design mission, one provenance row per datum. Reserve and cruise Mach
            // are printed in the chart notes of the same figure; the climb and descent
            // schedules are not printed there.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::Manufacturer,
                    stated_conditions: Some("standard day, 0.85 Mach cruise, CF6-80C2B1F engines, 875,000 lb brake-release line, FAR international reserves; range read from the chart at the maximum-structural-payload corner"),
                    relative_uncertainty: Some(100.0 / 5_700.0),
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "Boeing 747-400 ACAP D6-58326-1 Rev F, December 2024, section 3.2.1 p.3-2 (payload/range, Model 747-400 and 400 Combi, 0.85 Mach cruise, CF6-80C2B1F); corner read as 5,700 nmi, +-100 nmi",
                }),
                payload: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::Manufacturer,
                    stated_conditions: Some("542,500 lb maximum zero-fuel weight less the 394,088 lb operating empty weight of the three-class 400-passenger arrangement (ACAP section 2.1.1 note 3): 148,412 lb"),
                    relative_uncertainty: None,
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "Boeing 747-400 ACAP D6-58326-1 Rev F, section 2.1.1 (fifth weight column) and section 3.2.1 p.3-2",
                }),
                profile: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::Manufacturer,
                    stated_conditions: Some("0.85 Mach cruise, standard day; the ACAP prints no climb or descent schedule, so the model's configured schedules apply to those phases"),
                    relative_uncertainty: None,
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "Boeing 747-400 ACAP D6-58326-1 Rev F, section 3.2.1 p.3-2 (title and notes)",
                }),
                reserve_fuel: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::Manufacturer,
                    stated_conditions: Some("FAR international reserves: 10 % of trip air time, 200 nmi alternate, 30 min hold at 1,500 ft; the reserve mass follows from the modelled trip"),
                    relative_uncertainty: None,
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "Boeing 747-400 ACAP D6-58326-1 Rev F, section 3.2.1 p.3-2 notes",
                }),
            },
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            // ACAP section 7.4.1 (p.7-9) ground envelope and section 7.3.
            aft_cg_nose_load: Some(crate::presets::gear_load::B747_400),
            sources: vec![
                "Boeing 747-400 ACAP D6-58326-1 Rev F, December 2024, the Engines table (p.1-5), sections 2.1.1, 2.2.1, 3.2.1, 7.2.1, 7.3, 7.4.1",
            ],
            ..AircraftReferenceData::default()
        },
        engine_name: "CF6-80C2",
        n_engines: 4,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 4,
            // ACAP section 7.2.1 (p.7-5): two four-wheel wing bogies and two
            // four-wheel body bogies (49x17 nose, H49x19.0-22 main tires).
            mlg_strut_bogie_wheels: Some(vec![4, 4, 4, 4]),
            wheels_per_mlg_strut: 0,
            mlg_tire: "H49x19.0-22/32PR".to_owned(),
            nlg_tire: "49x17/32PR".to_owned(), // ACAP 7.2.1 prints both
            // 36 ft 1 in (11.00 m) wing-gear track over the 6.50 m body.
            track_diameter_factor: 11.00 / 6.50,
            // Section 2.2.1 prints the 84 ft 0 in (25.60 m) wheelbase to the mean of the two main groups (wing bogies 24.07 m, body bogies 27.14 m, section 7.2.1); the station fractions below keep the groups, this scalar is the printed datum.
            reference_wheelbase_m: Some(25.60),
            reference_body_wheelbase_m: Some(27.14),
            reference_track_m: Some(11.00),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            // Section 2.2.1 side view: 225 ft 2 in (68.63 m) body length.
            reference_station_fuselage_length_m: Some(68.63),
            // Section 2.2.1: nose gear 25 ft 5 in (7.75 m) aft of the nose tip.
            reference_nlg_x_fraction: Some(7.75 / 68.63),
            reference_mlg_x_fractions: Some(vec![
                (7.75 + 24.07) / 68.63,
                (7.75 + 24.07) / 68.63,
                (7.75 + 27.14) / 68.63,
                (7.75 + 27.14) / 68.63,
            ]),
            // Section 2.2.1 side view, keel above the ground line read as 1.94 m
            // (drawing read, +-0.2 m).
            fuselage_ground_clearance_m: Some(1.94),
            // Boeing AERO 2007 Q1 "Tail Strikes: Prevention" gives 12.5 deg for
            // the 747-400 with the gear extended; about 11 deg is the estimate
            // for compressed struts (ESTIMATE). The generic tailcone loft
            // understates the geometric tail-down angle, so 11 deg is the floor
            // and no 15 deg Raymer/Roskam floor is imposed.
            min_tip_back_deg: 11.0,
            // Tail-strike pitch attitude 12.5 deg: AERO 2007 Q1 Fig. 1 (takeoff, flaps 10/20, lift-off attitude 10.1/10.0 deg).
            // The table prints no gear state: state unspecified, compared with the gear-extended model attitude (a warning).
            required_rotation_angle_deg: 12.5,
            required_rotation_basis: crate::landing_gear::RotationAngleBasis::PublishedStateUnspecified,
            trimmable_stabilizer: true,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            // ACAP section 2.2.1: 211 ft 5 in (64.44 m) jig span; 213 ft 0 in
            // (64.92 m) at maximum gross weight with full fuel.
            span_m: 64.44,
            // Centreline chord of the drawn planform (section 2.2.1 plan read, with the Yehudi trailing-edge extension);
            // a datum since the wing inside the body runs on the built edge lines to y = 0 and the box holds the side-of-body chord. Built gross area about 565 m2, NOT the
            // published 525 m2 (the reference wing states the published area; the sections do not).
            root_chord_m: 16.10,
            // Section 2.2.1 plan trace at y = 13.2 m: LE 30.10 m, TE 39.14 m aft of the nose. Was 8.845.
            break_chord_m: 9.045,
            // Section 2.2.1 plan: the outer trailing edge is one straight line (0.58 m/m) ending at chord 3.85 m at y = 31.29 m (the 64.44 m span includes the winglets).
            tip_chord_m: 3.85,
            // Leading-edge angle of the panel outboard of the kink: the drawn tip (section 2.2.1 plan, 45.8 m at
            // y = 31.29 m) reads 41.0 deg from the kink; 40.9 deg holds the 45 deg guardrail of the +-10 % local sweep
            // study and leaves the tip within 0.1 m. The inner panel has its own angle (`inboard_le_sweep_deg`).
            sweep_deg: 40.9,
            tip_twist_deg: -2.0,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            // ACAP section 2.2.1 side view: 225 ft 2 in (68.63 m) body length;
            // the 231 ft 10.25 in (70.67 m) overall length ends at the fin tip.
            fuselage_length_m: 68.63,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        speed_schedule_source: crate::presets::SpeedScheduleSource::SourcedLadder,
        airfoil_class_source: "Design-era declaration: the 747 wing was designed in 1964-1966 (747-400 EIS 1989 with the same planform plus winglets), before supercritical sections reached transport service, so its technology is conventional; its upper-surface velocities peak close to the leading edge, the influence of peaky sections (Obert, Aerodynamic Design of Transport Aircraft, 2009, p. 239). Boeing publishes no 747 section coordinates, so the wing draws the five 747 stations Obert prints (Fig. 24.82, p. 243, from NASA SP-347), digitised as B747-Y109 (side of the body) to B747-Y500 (2y/b 0.50), each at its own span station; the thickness stations set the t/c.",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values; the drawn planform stays the geometry.
                // S 524.90 m2 (5,650 ft2), MAC 327.78 in (8.33 m), span 195.67 ft (59.64 m): Boeing 747-400 wing reference values (FAA DOT/FAA/AR-04/44 Table 1, retrieved).
                // MAC and LEMAC: NASA CR-2144 (Heffley and Jewell, Aircraft Handling Qualities Data, 1972), Fig. IX-2, printed p. 213, prints
                // c = 27.31 ft (the 327.78 in above) with 0.25 c at F.S. 1339.9 in. The nose tip is F.S. 93.3 in (+-6 in) read on the same figure's
                // side view by its own F.S. 307.9 and F.S. 1339.9 marks [E]: LEMAC (1339.9 - 93.3) in - 0.25 c = 29.582 m aft of the nose
                // (the drawn reference trapezoid of the kink and tip lines has MAC 9.15 m at 29.78 m; built drawn S about 575 m2 with the Yehudi).
                reference_wing: crate::ReferenceWingConfig::published(
                    B747_400_REFERENCE_AREA_M2,
                    Some(327.78 * 0.0254),
                    Some((1339.9 - 93.3) * 0.0254 - 0.25 * 327.78 * 0.0254),
                ),
                // Section 2.2.1 plan trace, y = 3.5 to 19.5 m: LE x = 21.24 + 0.9133 (y - 3.5), so 18.04 m on the centreline
                // (was 18.4 with one 40.9 deg line, which left the outer wing up to 0.6 m forward of the drawing).
                root_datum_x_m: 18.04,
                inboard_le_sweep_deg: Some(42.4),
                // Heights: section 2.2.1 front view (wing upper and lower lines both halves) and side view (root upper line
                // z -0.7 m at x = 23 m, tip line 1.35 m). The two views disagree by 0.3 to 0.5 m, so the heights lie between them.
                root_z_m: -1.74, // was -1.22
                break_z_m: -0.37, // was -0.1709
                tip_z_m: 0.95, // D6-58326-1 Rev F 2.2.1 front view: wing and winglet top 0.2 m lower at the tip; was 1.138
                root_twist_deg: 4.0,
                break_twist_deg: 1.5,
                // Section 2.2.1 plan: trailing-edge kink where the 0.323 and 0.582 m/m lines cross, y = 13.2 m of 31.29 m.
                break_span_fraction: 0.4219,
                // Side of the 6.50 m body, y = 3.25 m; section 2.2.1 plan trace: LE 21.01 m, TE 35.92 m, chord 14.91 m.
                side_of_body_span_fraction: Some(3.25 / 31.29),
                side_of_body_chord_ratio: Some(0.9263), // 14.91 / 16.10; was 0.928
                kink_span_fraction: Some(0.4219),
                outboard_sweep_decrement_deg: 0.0,
                // Section 2.2.1 plan: the leading edge breaks at y = 21.6 m, 0.9133 m/m inboard, 0.833 outboard (hidden by
                // the outer pylon at the break; lines read either side). 37.77 m there against 37.37 m on the straight line.
                edge_stations: vec![crate::WingEdgeStation { span_fraction: 0.6903, le_offset_m: 0.40, te_offset_m: 0.0 }],
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                // the -400 winglet (ACAP D6-58326-1 Rev F section 1.3: the -400 has winglets, the -400D none).
                // No winglet dimension is printed. Section 2.2.1: top about 1.1 m above the wing tip in the front view, 1.45 m in the
                // side view (1.45 m kept with the side-view top, was 1.3 m), 0.93 m outboard (cant 32.7 deg from vertical); plan view: tip
                // chord 1.8 m, tip trailing edge at the published 50.78 m (leading edge 62.3 deg in the device plane; was 65.3 deg).
                // The published 64.44 m span (211 ft 5 in) includes the winglets, so the wing proper ends at y = 31.29 m.
                wingtip_device: Some(crate::WingtipDevice {
                    kind: crate::WingtipDeviceKind::Winglet,
                    height_m: 1.45,
                    down_height_m: 0.0,
                    cant_deg: 32.7,
                    root_chord_m: None,
                    root_te_flare: None,
                    tip_chord_m: 1.8,
                    le_sweep_deg: 62.3,
                    toe_out_deg: 0.0,
                    blend_radius_m: 0.0,
                    arc_start_deg: None,
                    // Section 2.2.1 plan: the wing leading edge runs on to (46.30, 31.66), its outline 31.71, with a rounded corner, then a tip edge near y 31.71 to x 47.20 where the winglet leading edge starts;
                    // trailing edge on the drawn line, x 49.83 at y 31.60, 49.96 at y 31.72. Offsets from the wing-tip leading edge (45.76, 31.29). The root and these sections keep the wing-tip plane,
                    // so the depth of the last one stays under the drawn tip edge (front view: outer skin y 31.71 at z 1.0) and the winglet turns up from it. The drawn winglet tip reaches y 32.45 against the printed 32.22: printed kept.
                    path_stations: [(0.347, 0.311, 0.01, 3.72), (0.537, 0.37, 0.02, 3.66), (1.437, 0.42, 0.05, 2.76)]
                        .map(|(dx_m, dy_m, dz_m, chord_m)| crate::WingtipPathStation { dx_m, dy_m, dz_m, chord_m })
                        .to_vec(),
                    airfoil: None,
                    span_includes_device: true,
                    include_in_vlm: true,
                }),
                // Sections: Boeing publishes no 747 coordinates. Obert (2009), Fig. 24.82 (p. 243, from NASA SP-347) draws
                // the stations at 2y/b 0.109, 0.206, 0.303, 0.40 and 0.50, digitised as B747-Y109 ... B747-Y500
                // (alas-geom data/digitised_sections.txt; thin-airfoil c_m,c/4 -0.014, -0.014, -0.030, -0.016, -0.044).
                // Each is a section station at its own span station. 2y/b is on the span of the source wing, the
                // 747-100/-200 59.64 m (195.67 ft, the -400 reference span above; the -400 adds the tip extension and
                // winglet): y = 2y/b x 29.82 m, so 2y/b 0.109 is the side of the 6.50 m body, y 3.25 m. Fractions of the
                // 31.29 m semispan: eta = 2y/b x 29.82 / 31.29. Inboard of Y109 the root holds it; outboard of Y500 the
                // tip holds Y500. Was Y109 root and Y500 tip only, then the conventional-class surrogate pair (b737a
                // root, b737d outboard; -0.013 and -0.022), and before that DSMA-523A, an aft-loaded section (-0.149).
                root_airfoil: "B747-Y109".to_owned(),
                tip_airfoil: "B747-Y500".to_owned(),
                section_stations: [
                    (3.25 / 31.29, "B747-Y109"), // 2y/b 0.109, y 3.25 m (side of body)
                    (0.1963, "B747-Y206"),       // 2y/b 0.206, y 6.14 m
                    (0.2888, "B747-Y303"),       // 2y/b 0.303, y 9.04 m
                    (0.3812, "B747-Y400"),       // 2y/b 0.400, y 11.93 m
                    (0.4765, "B747-Y500"),       // 2y/b 0.500, y 14.91 m
                ]
                .map(|(span_fraction, airfoil)| crate::WingSectionStation { span_fraction, airfoil: airfoil.to_owned() })
                .to_vec(),
                airfoil_class: crate::AirfoilClass::Peaky,
                // Thickness: NASA SP-347 via Obert (2009) Fig. 24.82, p. 243 (PDF p. 262): t/c 13.5 % at 2y/b 0.109, 10.4 % 0.206,
                // 8.9 % 0.303, 8.0 % 0.40-0.50, held outboard (ESTIMATE); the CRM distribution before was 1.5-2 points thicker.
                // On the section stations above (eta = 2y/b x 29.82 / 31.29; formerly x 32.22 / 31.29, the -400 span
                // with winglets, which put 2y/b 0.206-0.50 up to 1.2 m outboard), plus the kink and the tip on the plateau.
                // Multipliers act on the section law's own maximum t/c on the aerodynamic 101-station sampling.
                // (span fraction, thickness multiplier): t/c of the section law -> documented t/c.
                thickness_stations: [
                    (0.1039, 0.9772), // t/c 0.1382 -> 0.1350 (B747-Y109, 2y/b 0.109)
                    (0.1963, 1.0158), // t/c 0.1024 -> 0.1040 (B747-Y206, 2y/b 0.206)
                    (0.2888, 1.0363), // t/c 0.0859 -> 0.0890 (B747-Y303, 2y/b 0.303)
                    (0.3812, 0.9802), // t/c 0.0816 -> 0.0800 (B747-Y400, 2y/b 0.40)
                    (0.4219, 0.9792), // t/c 0.0817 -> 0.0800 (kink, Y400-Y500 blend)
                    (0.4765, 0.9778), // t/c 0.0818 -> 0.0800 (B747-Y500, 2y/b 0.50, printed plateau end)
                    (0.6691, 0.9778), // t/c 0.0818 -> 0.0800 (held, estimate; plateau nodes on the dihedral stations keep the C2 spline flat)
                    (0.8235, 0.9778), // t/c 0.0818 -> 0.0800 (held, estimate)
                    (1.0000, 0.9778), // t/c 0.0818 -> 0.0800 (held, estimate)
                ]
                .map(|(span_fraction, thickness_scale)| crate::WingThicknessStation { span_fraction, thickness_scale })
                .to_vec(),
                // Dihedral: section 2.2.1 front and side views as at root_z_m; fractions of the 31.29 m semispan, on thickness nodes where they can be.
                // The first two follow the section stations (were 0.2121 z -0.955 and 0.3120 z -0.70), z read on that polyline.
                // (span fraction, z m):
                dihedral_stations: [(0.1963, -1.013), (0.2888, -0.759), (0.5147, -0.064), (0.6691, 0.361), (0.8235, 0.70)]
                    .map(|(span_fraction, z_m)| crate::WingDihedralStation { span_fraction, z_m, flight_rise_m: None })
                    .to_vec(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                // 2.2.1 front view over the side chords: fin t/c 0.116 at z 6 m, 0.09 at z 11-13 m, 11 % across the height.
                vstab_airfoil: Some("naca0011".to_owned()),
                tail_airfoil: "naca0009".to_owned(), // 2.2.1 front view: tailplane root depth 0.34 m under the drawn lower edge at 12 %; 9 % reads the drawn depth
                // Section 2.2.1: tailplane span 72 ft 9 in (22.17 m), tip
                // trailing edge 229 ft 2 in (69.85 m) aft of the nose. Plan
                // drawing reads: tip leading edge 67.07 m, root leading edge
                // 56.9 m at the centreline (10.2 m aft to the tip), tip chord
                // 2.8 m, centreline chord 11.1 m. Height is a side-view read.
                // the tailplane sweep (42.6 deg), span, tip trailing edge and root position match the drawing
                // within 0.2 deg / 0.1 m (B747-400): unchanged. Height and dihedral (C, one view) also stay.
                // Tail fit: tailplane straight-edge fit to the drawing, centreline values.
                hstab_offset_from_tail_m: 11.65,
                hstab_z_m: 2.310, // upper surface at the root kept on the 2.2.1 front view with the thinner section (was 2.160)
                hstab_root_chord_m: 10.01,
                hstab_tip_chord_m: 2.64,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (10.31, 11.085, 1.05), // 2.2.1 front view: tip 0.27 m above the drawn tip with the old section and root; was z 1.320
                // Side view (drawing read): fin root leading edge 53.5 m aft of
                // the nose at the aft crown, tip leading edge 13.3 m behind it
                // and 10.2 m above the root line, tip trailing edge at the
                // 70.67 m overall length. The fin tip lands at about 19.4 m
                // above the ground (ACAP section 2.3.1 prints 18.80 to
                // 19.51 m for the tail height K).
                // the model leading edge is 0.72 m forward of the
                // drawn one at the crown line (54.91 vs 55.63 m, B): offset 15.13 -> 14.41 m moves the whole fin aft
                // (the measurement table prints 15.85, which has the wrong sign for the L - offset datum). The tip
                // leading edge then sits 12.55 m behind the root (was 13.3 m): the drawn sweep is 49.9 deg against 52.5
                // deg (C) and the tip trailing edge lands on the published 70.67 m overall length (231 ft 10.25 in).
                // Root chord 13.3 -> 12.35 m: with the leading edge moved the trailing edge at the crown line falls on
                // the drawn 66.98 m (chord 11.4 m there against 11.35 m drawn, C).
                // Root chord: trailing edge 0.15 m aft of the drawn line at the side-view door scale (door 5 and overall length); tip corner stays on the printed 70.67 m.
                vstab_offset_from_tail_m: 14.29,
                vstab_z_m: 3.6,
                vstab_root_chord_m: 12.02, // was 12.19
                vstab_tip_chord_m: 4.22, // tip trailing edge on the printed 70.67 m overall length; was 4.27
                vstab_tip_le_m: (12.11, 0.0, 10.41), // W1: fin top 14.06 m in the side view (was 10.2)
                // ACAP side view: dorsal line leaves the crown at x 53.0 m, nearly straight, and joins the fin LE about z 4.98 m.
                vstab_dorsal_length_m: Some(1.33),
                vstab_dorsal_height_m: Some(1.38),
                vstab_dorsal_shoulder: Some(0.1),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                // ACAP section 2.5.1: constant 21 ft 4 in (6.50 m) cabin width.
                diameter_m: 6.50,
                // Main lobe, keel to the crown aft of the hump: 7.24 m, read
                // between outline centres off the section 2.2.1 side view
                // (Figure 2.2.1, p. 2-14) rendered at 600 dpi, 32.06 px/m from
                // the 68.63 m body length (the 7.75 m nose-gear station reads
                // 7.74 m at that scale); +-0.06 m. The previous 7.6 m was a
                // length-weighted mean standing in for the unmodelled hump.
                height_m: Some(7.24),
                // Upper-deck hump, crown line only, fitted to the same side
                // view (crown read per pixel column from 4.5 m to 31 m; fit
                // RMS 0.026 m, worst 0.05 m aft of the 6 m cabin start): the
                // crown rises from 5.4 m, is 0.85 m above the main crown
                // (8.09 m above the keel) from 11.25 m to 20.5 m and is back
                // on the main crown at 29.5 m. Drawing reads, +-0.06 m in
                // height and +-0.3 m in station.
                hump_height_m: Some(0.85),
                hump_start_x_m: Some(5.4),
                hump_crown_start_x_m: Some(11.25),
                hump_crown_end_x_m: Some(20.5),
                hump_end_x_m: Some(29.5),
                hump_fairing_exponent: Some(2.0),
                // ACAP section 2.5.2 (p. 2-29): 9 ft 0 in (2.73 m) from the
                // main-deck floor to the upper-deck floor.
                upper_deck_floor_height_m: Some(2.73),
                // Usable upper-deck floor: the upper-deck window run of the
                // section 2.2.1 side view, 9.5 m to 21.6 m aft of the nose
                // (drawing read, +-0.3 m), extended 1.4 m aft to the stair and
                // galley of the section 2.4.1 upper-deck plan (p. 2-22;
                // ESTIMATE), which seats 42 business at 38 in (0.96 m) pitch.
                upper_deck_start_x_m: Some(9.5),
                upper_deck_end_x_m: Some(23.0),
                // Nose tip on the body axis, 3.42 m above the keel and 5.36 m
                // above the ground (the 600 dpi side view reads 3.35 m above
                // the keel).
                nose_z_m: 0.52,
                cabin_start_x_m: 6.0,
                cabin_z_m: 0.2,
                // Generic tail-cone length of the outer mould line (ESTIMATE).
                tailcone_length_m: 17.0,
                tail_z_m: 2.5,
                // FUSELAGE-G2 shaped nose and aft body. Outline traced from the manufacturer drawing
                // (side: same; top: Boeing 747-400 D6-58326-1 Rev F),
                // calibrated to the drawn length; the nose law (windshield, radome, keel and plan
                // exponents) is a least-squares fit to the traced upper, lower and plan lines (RMS 0.07 m),
                // the aft law (belly ramp, crown drop, plan taper, closing section) to the traced aft lines
                // (RMS 0.071 m). Cabin start, tailcone length and every payload and mass station are unchanged:
                // only the outer loft moves. Section exponent 2.0 (ellipse): no drawing measures it.
                // Data quality: aft crown and closing section read by eye from the thin-line ACAP drawing.
                nose_length_m: Some(14.28),
                nose_windshield_angle_deg: Some(37.1),
                nose_crown_end_fraction: Some(0.55),
                nose_radome_length_fraction: Some(0.26),
                nose_keel_exponent: Some(2.59),
                nose_plan_exponent: Some(1.65),
                nose_section_exponent: Some(2.0),
                belly_upsweep_length_m: Some(26.39),
                aft_belly_exponent: Some(1.28),
                tail_end_z_m: Some(2.75),
                tail_end_width_m: Some(0.48),
                tail_end_height_m: Some(1.59),
                aft_plan_length_m: Some(19.36),
                aft_plan_exponent: Some(1.23),
                aft_crown_length_m: Some(8.62),
                aft_crown_exponent: Some(3.0),
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                engine_name: "CF6-80C2".to_owned(),
                // Section 2.2.1 plan view: engine centrelines 38 ft 4 in (11.68 m) and 69 ft (21.03 m).
                nacelle_model: crate::NacelleModel::V2,
                spanwise_positions_m: vec![11.68, -11.68, 21.03, -21.03],
                // Section 2.2.1 front and side views, nacelle centres -2.66 and -1.77 m (the views disagree by 0.3 m on the inner pair). Was -2.25.
                z_m: -2.14,
                // Section 2.2.1 table: CF6-80C2 inlets 23.01 and 32.16 m against LE 28.71 and 37.25 m; the drawn
                // nacelles sit 0.25 m aft, at the PW4200 row. Offsets below put the built inlets on the CF6-80C2 row.
                inlet_x_offset_m: 5.345, // mean of the pairs below
                inlet_x_per_engine_m: vec![5.65, 5.65, 5.04, 5.04],
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            // ACAP section 3.2.1 prints 0.85 Mach cruise.
            cruise_mach: 0.85,
            cruise_altitude_m: 10_668.0,
            mtow_kg: 396_893.0,
            max_wing_area_m2: B747_400_REFERENCE_AREA_M2,
            min_wing_loading_kg_m2: 550.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 400,
            // ESTIMATE: lower-deck containers (157 m3) and bulk hold (24 m3)
            // at a representative 160 kg/m3 plus main-deck baggage allowance.
            cargo_payload_kg: 70_000.0,
            // Maximum zero-fuel weight 246,073 kg less operating empty weight
            // 178,755 kg (ACAP section 2.1.1: 148,412 lb = 67,318 kg).
            max_structural_payload_kg: 67_318.0,
            // Superseded at registration: the profile declares the TCDS VMO/MMO, so VD is the
            // CS 25.335(b) value on VC = VMO (`crate::design_speeds`, `registry`).
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("advanced_highlift_widebody"),
    }
}
