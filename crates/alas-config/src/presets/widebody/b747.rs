// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Boeing 747-400 preset.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, WingConfig,
};

// Source key used in the comments below:
//   ACAP = Boeing 747-400/-400ER Airplane Characteristics for Airport Planning,
//   D6-58326-1 Rev F, December 2024 (PDF page = document page + 17 in sections
//   2 and 3); https://www.boeing.com/content/dam/boeing/v2/airports/acaps/747-400_Rev_F.pdf
//   "drawing read" = a dimension measured off the ACAP section 2.2.1 plan or
//   side view (Figure 2.2.1, document page 2-14) at the scale of its printed
//   dimensions, +-0.3 m.
// FAA TCDS A20WE was NOT retrieved in this session: nothing below cites it as
// read, and certified_max_seats is left unset for that reason.

/// The main-deck doors: ten passenger doors, five each side (ACAP section
/// 2.7.1, p. 2-35), door centres 31 ft 2 in (9.50 m), 61 ft 8 in (18.80 m),
/// 100 ft 5 in (30.61 m), 133 ft 8 in (40.74 m) and 180 ft 11 in (55.14 m)
/// aft of the nose on the 225 ft 2 in (68.63 m) body. The 42 x 76 in clear
/// opening printed there is at least the Type A minimum of CS 25.807(a)
/// (42 x 72 in). The upper-deck emergency exits (section 2.7.5) are not part
/// of this main-deck arrangement; the upper deck keeps the generic exit rule.
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
    station_body_length_m: Some(68.63),
    source: "Boeing 747-400 ACAP D6-58326-1 Rev F, December 2024, section 2.7.1 p. 2-35 (747-400 main deck doors 1-5 at 31-2/9.50, 61-8/18.80, 100-5/30.61, 133-8/40.74, 180-11/55.14 ft-in/m; 42 x 76 in openings)",
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
            // Boeing reference area 5,650 ft2 (525 m2). NOT in the ACAP and not
            // re-verified here (recalled, estimate): it matches a straight-taper
            // trapezoid that leaves out the inboard trailing-edge extension.
            reference_wing_area_m2: Some(525.0),
            // 24 first + 32 business + 302 economy on the main deck and 42
            // business on the upper deck (ACAP section 2.1.1 typical seating).
            planning_seats: Some(400),
            planning_cabin: Some(B747_400_PLANNING_CABIN),
            certified_exit_layout: Some(B747_400_EXIT_LAYOUT),
            design_point: Some(crate::PayloadRangeDesignPoint {
                range_nmi: 5_700.0,
                payload_kg: Some(67_318.0),
                source: "Boeing 747-400 ACAP D6-58326-1 Rev F section 3.2.1 p.3-2 (payload/range 0.85 Mach, CF6-80C2B1F, standard day): maximum-structural-payload corner, 542,500 lb zero-fuel weight (OEW 394,088 lb + 148,412 lb payload = 67,318 kg) meets the 875,000 lb brake-release line at a range read as 5,700 nmi (+-100 nmi); reserves FAR international, 10 % trip allowance, 200 nmi alternate, 30 min hold at 1,500 ft",
            }),
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
            // 36 ft 1 in (11.00 m) wing-gear track over the 6.50 m body.
            track_diameter_factor: 11.00 / 6.50,
            // Section 7.2.1: nose gear to the wing bogie centres 78 ft 11.5 in
            // (24.07 m); the body bogies are 10 ft 1 in (3.07 m) behind them
            // (27.14 m). Section 2.2.1 prints the 84 ft 0 in (25.60 m) wheelbase
            // to the mean of the two groups (24.07 + 3.07 / 2 = 25.61 m).
            reference_wheelbase_m: Some(24.07),
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
            // the 747-400 with the gear extended; the belly upsweep below is
            // solved for a model tail-down angle of about 11 deg (compressed
            // struts, estimate), so the tip-back floor is the tail-down angle
            // itself and no 15 deg Raymer/Roskam floor is imposed.
            min_tip_back_deg: 0.0,
            takeoff_stabilizer_nose_up_deg: Some(crate::landing_gear::TRANSPORT_THS_TAKEOFF_NOSE_UP_DEG),
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            // ACAP section 2.2.1: 211 ft 5 in (64.44 m) jig span; 213 ft 0 in
            // (64.92 m) at maximum gross weight with full fuel.
            span_m: 64.44,
            // Reference trapezoid of the 525 m2 reference area over 64.44 m with
            // the drawn 4.27 m tip chord (section 2.2.1 plan: tip leading edge
            // 152 ft 7 in = 46.51 m and trailing edge 166 ft 7 in = 50.78 m aft
            // of the nose): root 2 S / b - c_tip = 12.02 m. The drawn planform
            // is wider inboard (Yehudi trailing-edge extension, about 16.1 m
            // at the centreline by drawing read); that extension lies outside
            // the reference wing and is not modelled.
            root_chord_m: 12.02,
            break_chord_m: 8.845,
            tip_chord_m: 4.27,
            // Straight leading edge through the side-of-body root station
            // (21.05 m aft of the nose at y = 3.25 m, drawing read) and the
            // drawn tip leading edge (46.51 m at y = 32.22 m) reads 41.3 deg; 40.9 deg is
            // used because the 45 deg sweep guardrail must hold for the +-10 % local
            // sweep study, and the root datum below moves 0.2 m aft so the tip
            // leading edge stays within 0.2 m of the drawing. The
            // quarter-chord sweep of this model wing is 39 deg against the
            // 37.5 deg quoted for the type (recalled, not retrieved).
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
        airfoil_class_source: "Design-era declaration: the 747 wing was designed in 1964-1966 (747-400 EIS 1989 with the same planform plus winglets), before supercritical sections reached transport service, so its technology is conventional. Boeing publishes no 747 sections; the DSMA-523A surrogate (UIUC Airfoil Coordinates Database; 11.0 % t/c) is only the nearest conventional-transonic shape in the library, used as in the DC-10 preset, and its thickness is an ESTIMATE, not the 747 value.",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Leading edge at the centreline 18.4 m aft of the nose (18.19 m on a
                // pure 41.3 deg line through the drawn side-of-body point): the
                // straight leading edge of the section 2.2.1 plan view
                // run in from the side of the body. With no wing shift this
                // puts the MAC leading edge at 30.1 m (the model MAC of this
                // trapezoid is 8.76 m); a 747-400 weight-and-balance LEMAC near
                // 29.7 m is recalled, not retrieved (estimate).
                root_datum_x_m: 18.4,
                // z positions are ESTIMATES from the section 2.2.1 front and
                // side views: low wing at the lower third of the body, about
                // 7 deg of dihedral at the 1 g shape.
                root_z_m: -2.0,
                break_z_m: -1.0,
                tip_z_m: 2.5,
                root_twist_deg: 4.0,
                break_twist_deg: 1.5,
                // Trailing-edge kink of the drawn planform at y = 13.2 m of the
                // 32.22 m semispan (drawing read).
                break_span_fraction: 0.4097,
                // Side of the 6.50 m body at y = 3.25 m; chord there from the
                // reference trapezoid, 11.24 m over the 12.02 m root chord.
                side_of_body_span_fraction: Some(3.25 / 32.22),
                side_of_body_chord_ratio: Some(0.935),
                kink_span_fraction: Some(0.4097),
                // The drawn leading edge is straight.
                outboard_sweep_decrement_deg: 0.0,
                root_airfoil: "dsma523a".to_owned(),
                tip_airfoil: "dsma523a".to_owned(),
                airfoil_class: crate::AirfoilClass::Conventional,
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // Section 2.2.1: tailplane span 72 ft 9 in (22.17 m), tip
                // trailing edge 229 ft 2 in (69.85 m) aft of the nose. Plan
                // drawing reads: tip leading edge 67.07 m, root leading edge
                // 56.9 m at the centreline (10.2 m aft to the tip), tip chord
                // 2.8 m, centreline chord 11.1 m. Height is a side-view read.
                hstab_offset_from_tail_m: 11.73,
                hstab_z_m: 2.7,
                hstab_root_chord_m: 11.1,
                hstab_tip_chord_m: 2.8,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (10.2, 11.085, 0.8),
                // Side view (drawing read): fin root leading edge 53.5 m aft of
                // the nose at the aft crown, tip leading edge 13.3 m behind it
                // and 10.2 m above the root line, tip trailing edge at the
                // 70.67 m overall length. The fin tip lands at about 19.4 m
                // above the ground (ACAP section 2.3.1 prints 18.80 to
                // 19.51 m for the tail height K).
                vstab_offset_from_tail_m: 15.13,
                vstab_z_m: 3.6,
                vstab_root_chord_m: 13.3,
                vstab_tip_chord_m: 3.9,
                vstab_tip_le_m: (13.3, 0.0, 10.2),
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
                // Measured nose (v1.3.2): Boeing D6-58326-1 Rev F
                // sec 2.2.1 side view (PDF p31) and plan view, outline read at 600 dpi. The model
                // nose is the MAIN-lobe nose: the hump rise (fields above) is subtracted from
                // the drawn upper line before fitting. Nose length = L(2 %) = 10.60 m (1 % to
                // 3 %: 11.52 to 9.86 m); the tip is 3.35 m above the keel, 0.27 m below the
                // main-lobe axis. Laws fitted at fixed length to the upper, lower and plan
                // lines (RMS 0.013 / 0.019 / 0.034 D_eff); section exponent unmeasured.
                nose_z_m: -0.07,
                cabin_start_x_m: 10.6,
                cabin_z_m: 0.2,
                nose_windshield_angle_deg: Some(32.0),
                nose_crown_end_fraction: Some(0.58),
                nose_radome_length_fraction: Some(0.1),
                nose_keel_exponent: Some(1.47),
                nose_plan_exponent: Some(1.27),
                nose_section_exponent: None,
                // Generic tail-cone length of the outer mould line (ESTIMATE).
                tailcone_length_m: 17.0,
                tail_z_m: 2.5,
                // ESTIMATE: set so the model tail-down angle from the aft main
                // gear is about 11 deg, against the 12.5 deg of Boeing AERO 2007
                // Q1 "Tail Strikes: Prevention" for the 747-400 with the gear
                // extended. The side view of ACAP section 2.2.1 shows the belly
                // rising from about 46 m aft of the nose (22.7 m); the model scrape angle is the minimum over the lower-contour stations aft of the primary (wing) gear at 31.82 m, set by the first station, so 26.3 m gives about 11 deg with the 1.94 m keel clearance.
                belly_upsweep_length_m: Some(26.3),
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                engine_name: "CF6-80C2".to_owned(),
                // ACAP section 2.2.1 plan view: engine centrelines 38 ft 4 in
                // (11.68 m) and 69 ft (21.03 m) from the airplane centreline.
                spanwise_positions_m: vec![11.68, -11.68, 21.03, -21.03],
                // ESTIMATE: nacelle centre about 1.5 m under the wing station;
                // the 2.84 m GE nacelle low-point clearance of section 2.2.1
                // is the cross-check.
                z_m: -0.5,
                // Section 2.2.1 inlet stations 75 ft 6 in (23.01 m) and 105 ft
                // 6 in (32.16 m) aft of the nose (CF6-80C2 row). The model
                // leading edge at the two engine stations is 28.46 m and
                // 36.65 m, so the offsets are 5.45 m and 4.49 m; one offset
                // serves both pairs, so the inlets land 0.5 m off.
                inlet_x_offset_m: 4.97,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            // ACAP section 3.2.1 prints 0.85 Mach cruise.
            cruise_mach: 0.85,
            cruise_altitude_m: 10_668.0,
            mtow_kg: 396_893.0,
            max_wing_area_m2: 525.0,
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
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("advanced_highlift_widebody"),
    }
}
