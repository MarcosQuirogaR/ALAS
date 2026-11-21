// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Military transport presets.
//!
//! Source key used on every number below: **S** = sourced from the document
//! named beside it (T1 EASA TCDS A.169 Issue 07, T2 EASA TCDS E.033 Issue 08,
//! T3 EASA TCDS P.012 Issue 04, B1 Airbus brochure TMMA0026/01/2025, G1
//! Bundeswehr A400M page, S1 Defence Turkey 2025 secondary article, S4
//! EUROCONTROL aircraft performance database); **I** = inferred from sourced
//! numbers by a stated relation; **E** = engineering estimate, nothing
//! published; **D** = measured on the Airbus brochure three-view (B1 p025, a
//! CGI render with printed overall length 45.1 m, span 42.4 m and height
//! 14.7 m; scale per axis: 20.04 px/m on length, 20.5 px/m on height, 19.78 px/m
//! on span; confidence C, about +-0.1 m); J = Jane's All the World's Aircraft
//! 2008-2009, A400M entry, p.298 dimension table. No value here is a calibration to A400M results or a physical
//! validation of the A400M.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MissionProfileConfig, WingConfig,
};

/// The overall length of 45.091 m (S, T1 III.4) is measured to the trailing
/// edge of the horizontal-tail tips, not to the end of the fuselage.
///
/// Nose to end of the fuselage tail cone, m (J p.298, printed; the brochure side
/// view reads 39.27 m on the 45.09 m length scale).
const FUSELAGE_LENGTH_M: f64 = 39.09;

/// The A400M cargo hold, seeded into the cabin configuration by
/// `cabin_sources::seed_freight_hold`.
///
/// S: Jane's All the World's Aircraft 2008-2009, A400M entry, book p.298 (PDF
/// p.342), DIMENSIONS, INTERNAL: hold length excluding ramp 17.71 m, ramp
/// length 5.40 m, width at floor (continuous) 4.00 m, height forward of the
/// wing box 3.85 m and aft of it 4.00 m (the lower value is used for the whole
/// floor). S: Airbus brochure TMMA0026/01/2025 p.4 prints 3.80 m / 4.00 m and
/// "9 military pallets"; p.5 "Load Carrying Ramp: able to accommodate up to 6
/// tonnes when airborne". The floor starts at the cabin start of this preset
/// (E, behind the flight deck, 8.0 m).
pub(super) const A400M_FREIGHT_HOLD: crate::FreightHoldConfig = crate::FreightHoldConfig {
    floor_start_x_m: 8.0,
    floor_length_m: 17.71,
    ramp_length_m: 5.40,
    clear_width_m: 4.00,
    clear_height_m: 3.85,
    ramp_load_limit_kg: Some(6_000.0),
};

/// Airbus A400M Atlas, military standard, 141 t MTOW, four TP400-D6.
///
/// The payload rides on the declared freight hold (17.71 m floor plus 5.40 m
/// ramp, 4.00 m wide, Jane's 2008-2009 p.298): nine 463L pallets in a single
/// row, trimmed to the loader's balance target. Vehicles and helicopters are
/// still not represented, only palletised load. The design mission remains the
/// brochure's 20 t over 6,300 km point; the floor and ramp also take the 30 t
/// and 37 t points, which are not adopted here (a separate decision).
pub fn a400m() -> AircraftPreset {
    let mut engine = EngineConfig {
        engine_name: "TP400-D6".to_owned(),
        ..EngineConfig::default()
    };
    // Materialize the catalogue entry (TP400-D6 ratings, FH385/FH386
    // propeller) before overriding the installation geometry.
    engine.apply_engine_spec();
    // D: engine axes measured on the brochure top view (spinner centres at
    // y = 6.79 and 12.74 m) and front view (6.72 and 12.71 m): 6.75 m and
    // 12.72 m, +-0.15 m. The inner disc edge at 6.75 - 2.667 = 4.08 m clears the
    // 2.8 m fuselage radius (propeller diameter 5.334 m, T3).
    engine.spanwise_positions_m = vec![12.72, 6.75, -6.75, -12.72];
    // D: spinner centres below the wing leading edge, brochure front view hub centres
    // against the built leading edge (both engines within 0.05 m).
    engine.z_m = -1.23;
    // D: spinner tips 4.55 m (inner) and 4.89 m (outer) ahead of the leading edge
    // on the engine axes (top view); the scalar is their mean.
    engine.inlet_x_offset_m = 4.72;
    engine.inlet_x_per_engine_m = vec![4.89, 4.55, 4.55, 4.89];

    AircraftPreset {
        name: "A400M",
        display_name: "Airbus A400M Atlas",
        class: crate::presets::AircraftClass::Transport,
        description: "Airbus A400M Atlas, military standard (141 t MTOW), four TP400-D6. Payload is nine 463L pallets on the 17.71 m hold floor and 5.40 m ramp; vehicles and helicopters are not represented. The design mission is the 20 t over 6300 km point. Many geometry inputs are estimates.",
        identity: AircraftVariantIdentity {
            model: "A400M-180 (military standard)",
            weight_variant: "141,000 kg MTOW military (B1); the civil WV001 limits of TCDS A.169 are 137,500 kg MTOW / 121,500 kg MLW / 109,600 kg MZFW",
            engine_model: "TP400-D6",
            modification_state: "series production military standard; the EASA type design is civil only and has no eligible MSN (T1 Note 1)",
            tank_configuration: "centre tank, two inner wing tanks and four feed tanks, normal fill (T1 III.9)",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Table 1-1: span 42.357 m (S, T1 III.4) lies in
            // 36 m <= b < 52 m, code D.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::D),
            // No military MRW is published; the civil 137,900 kg is not used.
            mrw_kg: None,
            mtow_kg: Some(141_000.0), // S: B1 p024 (military), G1
            mlw_kg: Some(123_000.0),  // S: B1 p024
            // No military MZFW is published; the civil 109,600 kg (T1) is a
            // different weight variant and is not mixed in.
            mzfw_kg: None,
            oew_kg: crate::oew_reference::preset_reference_oew_kg("A400M"),
            usable_fuel_volume_l: Some(62_267.0), // S: T1 III.9 normal fill
            usable_fuel_mass_kg: Some(48_879.0),  // S: T1 III.9 normal fill
            fuel_density_kg_l: Some(0.785),       // S: T1 III.9
            reference_wing_area_m2: Some(221.5),  // S(secondary): research note Item 1
            planning_seats: None,
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: Some("Jane's All the World's Aircraft 2008-09 p 298 (A400M performance, estimated): logistic mission range at long-range cruise speed; the brochure's own 3,400 nmi carries the same mission and prints no speed"),
                cruise: None,
                range_nmi: 3_400.0,
                payload_kg: Some(20_000.0),
                source: "Airbus A400M brochure TMMA0026/01/2025 p024 range-payload table: 20 t at 6300 km (3400 nmi, read as 6300 km / 1.852); reserves, altitude and speed not stated. The 37 t / 3300 km and 30 t / 4450 km points are not adopted as the design point (the pallet floor and ramp could carry them)",
                reserve: Some(crate::presets::DesignPointReserve {
                    scheme: Some(crate::FuelScheme::StudyConvention),
                    contingency_trip_fraction: Some(0.05),
                    contingency_minimum_hold_min: Some(0.0),
                    alternate_distance_nmi: Some(0.0),
                    final_reserve_hold_min: Some(30.0),
                    flag_flight_time_fraction: None,
                    holding_altitude_ft: Some(1_500.0),
                    source: "Jane's All the World's Aircraft 2008-09 p 298 (A400M performance, estimated): logistic mission range at long-range cruise speed, reserves as per MIL-C-5011B (5% trip fuel and 30 min hold at 1,500 ft); no alternate is stated, and the same entry gives 3,450 n miles with 20,000 kg against the brochure's 3,400 nmi",
                }),
            }),
            // Design mission, one provenance row per datum: the brochure's 20 t range point. The
            // reserve rule is MIL-C-5011B, which Jane's attributes to this logistic mission.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::Manufacturer,
                    stated_conditions: Some("range with 20 t payload 3,400 nm (6,300 km); the brochure prints no reserves, take-off mass or speed, and the same mission at long-range cruise with MIL-C-5011B reserves is quoted as 3,450 nmi, carried in the uncertainty"),
                    relative_uncertainty: Some(50.0 / 3_400.0),
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "Airbus A400M brochure TMMA0026/01/2025 printed pp.24-25 (range-payload table)",
                }),
                payload: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "20,000 kg logistic payload, a point of the range-payload table (not a corner)",
                    "Airbus A400M brochure TMMA0026/01/2025 printed pp.24-25",
                )),
                profile: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "cruise up to Mach 0.72 at 37,000 ft (cruise band 0.68-0.72 M); no climb or descent schedule is printed, so the model's configured schedules apply to those phases",
                    "Airbus A400M brochure TMMA0026/01/2025 printed pp.12-13 and p.24",
                )),
                reserve_fuel: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::StandardCitedViaReference,
                    "MIL-C-5011B reserves: 5 % of trip fuel and 30 min hold at 1,500 ft, no alternate; the reserve mass follows from the modelled trip",
                    "MIL-C-5011B (standard aircraft characteristics and performance, fuel reserves); attributed to the A400M logistic mission in Jane's All the World's Aircraft 2008-09, A400M entry, Performance, p.298",
                )),
            },
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            sources: vec![
                "EASA TCDS EASA.A.169 Airbus A400M, Issue 07, 28 November 2025 (civil A400M-180 WV001): span 42.357 m, length 45.091 m, width 5.600 m, MAC 5.671 m, MMO 0.72, fuel tanks (III.9, 0.785 kg/L), gear wheel counts (III.21)",
                "EASA TCDS EASA.E.033 TP400-D6, Issue 08, 29 March 2021: ratings, dry mass 1,938.1 / 1,965.1 kg",
                "EASA TCDS P.012 FH385/FH386 propeller, Issue 04, 15 December 2015: eight blades, 683 kg maximum assembly mass",
                "Airbus Defence and Space, A400M The 21st Century Airlifter, TMMA0026/01/2025, p024: military MTOW 141 t, MLW 123 t, maximum payload 37 t, range-payload table, p012 cruise M0.72",
                "Bundeswehr A400M transport aircraft page (accessed 2026-10-04): empty weight 78.6 t, definition not stated",
            ],
            operating_speed_limits: Some(super::operating_limits::A400M_180),
            ..AircraftReferenceData::default()
        },
        engine_name: "TP400-D6",
        n_engines: 4,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,        // S: T1 III.21
            n_mlg_struts: 6,        // S(secondary): three twin-wheel legs per side
            // Main tire: EASA TCDS A.169 sec. 21 prints 43x15.5-17 22PR. No data book rates it,
            // so the rated load is the dominated-envelope lower bound H37x14.0-15/22PR.
            mlg_tire: "43x15.5-17/22PR".to_owned(),
            nlg_tire: "37x14.0-14/22PR".to_owned(),
            wheels_per_mlg_strut: 2, // S: T1 III.21, 12 main wheels in total
            // J p.298: mean wheel track 6.22 m (brochure front view reads 6.31 m).
            track_diameter_factor: 6.22 / 5.6,
            // J p.298: wheelbase 11.76 / 13.29 / 14.82 m to the front, middle and rear axles. D: nose
            // wheels 5.09 m aft of the nose tip; the brochure reads the axles at 16.57, 18.36 and 20.11 m.
            reference_wheelbase_m: Some(13.29),
            reference_track_m: Some(6.22),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(FUSELAGE_LENGTH_M),
            reference_nlg_x_fraction: Some(5.09 / FUSELAGE_LENGTH_M),
            // In strut order: the left and right leg of each of the three rows.
            reference_mlg_x_fractions: Some(
                [16.85, 16.85, 18.38, 18.38, 19.91, 19.91]
                    .iter()
                    .map(|x| x / FUSELAGE_LENGTH_M)
                    .collect(),
            ),
            // Belly clearance read from the Airbus A400M brochure side view (TMMA0026/01/2025 p.025,
            // page 13 rendered at 8x): the vertical scale is the 14.7 m overall-height rule (60.07 px/m between its two reference lines,
            // the lower one at the wheel bottoms); the flat fuselage bottom between the nose-gear bay and the main-gear sponson stands
            // 73 px = 1.21 m above that line (the sponson 0.87 m). Reading uncertainty +-0.05 m (outline blur, anti-aliasing) plus the
            // scale (+-0.02 m); the render is a nominal static aeroplane, weight and CG unstated. Cross-check: the lowest propeller blade
            // tip reads about 1.9-2.0 m there, the model tip at this plane 1.845 m: 0.05-0.15 m apart, more than the stated
            // +-0.05 m. The fuselage-bottom reading governs (it is the datum this field names, read directly); fitting the tip
            // instead, as for the ATR, would put the plane at 1.31-1.41 m. The disagreement is model nacelle geometry or the
            // render's pose, not separable from this one drawing. A primary dimensional source (Airbus airport
            // characteristics, a flight-manual excerpt) is still wanted; the value is open to replacement by one.
            // Needs data: a primary dimensional source for the A400M belly clearance (Airbus airport characteristics or a flight-manual
            // excerpt with the static ground line) and for the lowest propeller tip (brochure 1.9-2.0 m against the model 1.845 m at this plane). Neither
            // is on disk; the brochure reading above stays the value until one is supplied.
            fuselage_ground_clearance_m: Some(1.21),
            // No published tail-strike attitude backs this preset's aft-fuselage
            // geometry, so its model tail-down angle is unvalidated (the generic
            // tailcone loft understates it): keep the Raymer/Roskam 15 deg floor on
            // top of the tail-down criterion.
            min_tip_back_deg: 15.0,
            // Tail-scrape requirement: no published tail-strike attitude found (no A400M source on disk or found),
            // so the 10 deg default (an assumption) stands.
            // E: trimmable horizontal stabiliser (the takeoff setting is derived
            // from the V2 trim balance); the A400M THS travel is not stated in
            // the public documents read.
            trimmable_stabilizer: true,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 42.357, // S: T1 III.4
            // I: single trapezoid (no kink, taper 0.33) reproducing the
            // TCDS MAC 5.671 m at S = 221.5 m2.
            // D: the brochure top view confirms one straight leading edge
            // (x = 12.75 + 0.332 y, 18.4 deg) and one straight trailing edge
            // (4.2 deg aft). J p.298 prints chords of 7.36 m (root, at the side of
            // the fuselage) and 2.54 m (tip); tip set to the printed 2.54 m (2.60 read 0.1 m wide).
            root_chord_m: 7.86,
            // E: ALAS needs a break station; the planform has none, so the
            // break chord is on the straight root-tip line at 0.5 semispan.
            break_chord_m: 7.86 + (2.54 - 7.86) * 0.5,
            tip_chord_m: 2.54, // S: J p.298 tip chord
            // I: leading-edge sweep from a 15 deg quarter-chord sweep (S1,
            // secondary) and taper 0.33; D: 18.4 deg measured on the drawing.
            sweep_deg: 18.3,
            tip_twist_deg: -2.0, // E
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: FUSELAGE_LENGTH_M, // S: T1 III.4
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        speed_schedule_source: crate::presets::SpeedScheduleSource::HandWritten,
        airfoil_class_source: "E: Airbus A400M sections are proprietary. Supercritical class declared for the M0.72 cruise (SC(2)-0714 root, SC(2)-0410 tip as generic stand-ins); no source states the technology level.",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values; the drawn planform stays the geometry.
                // S 221.5 m2 and MAC 5.671 m: EASA TCDS EASA.A.169 Airbus A400M (Issue 07, 2025-11-28); LEMAC not published: the MAC leading edge
                // is the drawn MAC leading edge, 15.665 m aft of the nose tip (the drawn MAC, 5.6708 m, equals the TCDS value).
                reference_wing: crate::ReferenceWingConfig::published(221.5, Some(5.671), Some(15.665)),
                // D: brochure top view, leading edge x = 12.75 + 0.332 y (LE rows at eta 0.3-0.9, scatter 0.05 m).
                root_datum_x_m: 12.75,
                // Side of body at the 2.80 m fuselage half width (TCDS width 5.60 m).
                side_of_body_span_fraction: Some(2.80 / (42.357 / 2.0)),
                // D: brochure front view, wing upper line, about 5 deg anhedral from the fuselage to the
                // tip with no break (blurred propeller discs removed from the reading; the earlier 2.9 and
                // 6.9 deg pair read the discs). Leading-edge heights = upper line less the built section
                // depth. Ground shape; the 1 g flight shape is not published and no flight rise is declared.
                root_z_m: 3.11,
                break_z_m: 2.21,
                tip_z_m: 1.45,
                dihedral_stations: vec![
                    crate::WingDihedralStation { span_fraction: 0.25, z_m: 2.65, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.75, z_m: 1.87, flight_rise_m: None },
                ],
                root_twist_deg: 2.0,  // E
                break_twist_deg: 0.0, // E: linear root to tip
                break_span_fraction: 0.5,
                kink_span_fraction: None,
                side_of_body_chord_ratio: None,
                outboard_sweep_decrement_deg: 0.0,
                root_airfoil: "SC2-0714".to_owned(), // E
                tip_airfoil: "sc20410".to_owned(),   // E
                airfoil_class: crate::AirfoilClass::SupercriticalAdvanced,
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                // Drawn tip edges read off the Airbus A400M brochure tail views; analysis stays on
                // the trapezoid.
                hstab_edge_stations: vec![
                    crate::TailEdgeStation {
                        span_fraction: 0.92,
                        le_offset_m: 0.0,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.94,
                        le_offset_m: 0.15,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.96,
                        le_offset_m: 0.32,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.98,
                        le_offset_m: 0.54,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.99,
                        le_offset_m: 0.73,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 1.0,
                        le_offset_m: 0.84,
                        te_offset_m: 0.0,
                    },
                ],
                vstab_edge_stations: vec![
                    crate::TailEdgeStation {
                        span_fraction: 0.8,
                        le_offset_m: 0.0,
                        te_offset_m: -0.006,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.88,
                        le_offset_m: -0.15,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.895,
                        le_offset_m: -0.21,
                        te_offset_m: -0.804,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.905,
                        le_offset_m: -0.25,
                        te_offset_m: -1.367,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.93,
                        le_offset_m: -0.33,
                        te_offset_m: -2.619,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.97,
                        le_offset_m: -0.36,
                        te_offset_m: -4.549,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.99,
                        le_offset_m: -0.27,
                        te_offset_m: -5.693,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 1.0,
                        le_offset_m: -0.05,
                        te_offset_m: -6.240,
                    },
                ],
                tail_airfoil: "naca0012".to_owned(),
                // T-tail. J p.298: tailplane area 67.0 m2 (this trapezoid 67.8 m2).
                // D (brochure top and side views, straight-edge fit at eta 0.35-0.9): LE apex 36.45 m, LE
                // 35.8 deg, TE 20.1 deg, half-span 9.45 m + tip corner (drawn 9.50 m; S1, 2004, 9.515 m).
                hstab_offset_from_tail_m: FUSELAGE_LENGTH_M - 36.45,
                hstab_z_m: 10.13,
                hstab_root_chord_m: 5.22,
                hstab_tip_chord_m: 1.82,
                hstab_root_twist_deg: 0.0,
                hstab_tip_twist_deg: 0.0,
                hstab_tip_le_m: (6.81, 9.45, 0.0),
                // D (side view): fin LE and TE straight and near parallel at 4 heights, chord 6.78 m. The TE runs straight up to the corner at
                // 0.88 height, where it meets the drawn cap, whose top slopes down aft from the 14.70 m printed height. The edge stations
                // carry that cap. The drawn outline aft of the corner at that height is the tailplane tip, not the fin; the stations
                // here formerly ran the TE out to it, 1.4 m aft of the drawn cap at 0.89 height.
                vstab_offset_from_tail_m: FUSELAGE_LENGTH_M - 31.62,
                vstab_z_m: 2.8,
                vstab_root_chord_m: 6.79, // Drawn chords 6.805 and 6.766 m up the straight edges, near parallel
                vstab_tip_chord_m: 6.77, // Straight-edge value; the edge stations above close the cap to the built tip chord
                vstab_tip_le_m: (5.60, 0.0, 8.36),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 5.6, // S: T1 "Width", read as fuselage maximum width
                // D: hull height 5.12 m, the drawn hull without sponsons, gear bays and wing-root fairing,
                // brochure side view at the height-arrow scale (105 rows; no printed value). Crown 2.80 m,
                // keel -2.32 m. The main-gear sponson is not built.
                height_m: Some(5.12),
                // D: brochure side view x 13.1 to 21.3 m, about 0.5 m below the keel; front view lobes
                // to y 3.91 m at the 5.6 m hull width. Parasite drag only.
                gear_sponson_length_m: Some(8.2),
                gear_sponson_half_width_m: Some(3.91),
                gear_sponson_depth_m: Some(0.5),
                // Drawing nose tip, mid of the nose dome.
                nose_z_m: -0.273,
                cabin_start_x_m: 8.0, // E: hold starts behind the flight deck
                // Centre of the cabin cylinder: crown 2.80 m - 5.12 / 2.
                cabin_z_m: 0.24,
                // The embedded loft draws the body (fit recorded in the data file); these keep the parametric
                // law near it. D: half-width 2.8 m to 28 m; belly leaves the keel at 22.4 m and rises at 18 deg
                // to the blunt end, 0.58 m high around z = 3.04 m, 1.05 m wide.
                tailcone_length_m: 9.99,
                tail_z_m: 3.04,
                belly_upsweep_length_m: Some(16.69),
                aft_belly_exponent: Some(1.0),
                tail_end_z_m: Some(3.04),
                tail_end_width_m: Some(1.05),
                tail_end_height_m: Some(0.58),
                aft_plan_length_m: Some(11.09),
                aft_plan_exponent: Some(1.32),
                aft_crown_length_m: Some(2.0),
                aft_crown_exponent: Some(1.5),
                // The loadable floor (17.71 m hold behind the 8.0 m flight deck,
                // plus the 5.4 m ramp: Airbus A400M brochure data, secondary)
                // ends at 31.1 m; the outer belly rising from 24.1 m does not
                // shorten it.
                hold_interior_end_x_m: Some(31.1),
                ..FuselageConfig::default()
            },
            engine,
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.72,           // S: B1 p012, T1 MMO
            cruise_altitude_m: 11_278.0, // S: B1 p012, 37,000 ft
            mtow_kg: 141_000.0,          // S: B1 p024
            max_wing_area_m2: 230.0,     // E: above 221.5 m2
            min_wing_loading_kg_m2: 300.0, // E: loose floor (MTOW/S = 637)
            aircraft_type: "cargo".to_owned(), // military cargo hold, not a passenger cabin
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: false,
            num_passengers: 0,
            cargo_payload_kg: 20_000.0, // S: B1 p024 design point
            max_structural_payload_kg: 37_000.0, // S: B1 p024
            // Superseded at registration: CS 25.335(b) from the civil VMO/MMO, a declared
            // method (SC C-02 governs the certified VD, text not public).
            dive_speed_m_s: 170.0,
            ..DesignRequirements::default()
        },
        // No subsystem fractions are inferred from the published OEW.
        mass_model: None,
        performance: a400m_performance(),
    }
}

/// The A400M operating-speed schedule loaded with the preset.
///
/// All E: climb 155 kt then 220 kt CAS with 2000/800 ft/min are EUROCONTROL
/// BADA-style ATC values (S4, low confidence); the initial climb speed is
/// raised to 95 m/s because 79.7 m/s (155 kt) breaches the model climb CL
/// limit of 1.5 at 134 t; the takeoff speed is above the 1.0 g stall at the
/// model CL limit of 2.44 at 141 t (64.6 m/s); landing speed is Vat 130 kt
/// (S4). Cruise TAS is set by the caller from M0.72 at 11,278 m.
pub(super) fn apply_a400m_speed_schedule(p: &mut MissionProfileConfig) {
    // E: climb and descent speeds are calibrated airspeeds (ATC-style values);
    // one assigned cruise level, no step climbs, as for any single-sector
    // schedule (the default is a long-haul cruise-climb).
    p.climb_descent_speed_reference = crate::SpeedReference::CalibratedAirspeed;
    super::atr72_600_schedule_fix::fly_single_assigned_level(p);
    p.takeoff_air_speed_m_s = 74.0;
    p.takeoff_climb_rate_m_s = 8.0;
    p.initial_climb_air_speed_m_s = 95.0;
    p.initial_climb_rate_m_s = 10.2;
    p.step_climb_1_air_speed_m_s = 113.0;
    p.step_climb_1_rate_m_s = 6.0;
    p.step_climb_2_air_speed_m_s = 113.0;
    p.step_climb_2_rate_m_s = 4.1;
    // E: descent ladder altitudes 10,000 / 6,000 / 3,000 / 1,500 ft and a
    // 600 ft/min final descent, generic transport-aircraft values.
    p.descent_1_altitude_ft = 10_000.0;
    p.descent_2_altitude_ft = 6_000.0;
    p.descent_3_altitude_ft = 3_000.0;
    p.descent_4_altitude_ft = 1_500.0;
    p.landing_descent_rate_m_s = 600.0 * 0.3048 / 60.0;
    p.descent_1_air_speed_m_s = 125.0;
    p.descent_1_rate_m_s = 10.0;
    p.descent_2_air_speed_m_s = 110.0;
    p.descent_2_rate_m_s = 8.0;
    p.descent_3_air_speed_m_s = 95.0;
    p.descent_3_rate_m_s = 6.0;
    p.descent_4_air_speed_m_s = 80.0;
    p.descent_4_rate_m_s = 4.5;
    p.landing_air_speed_m_s = 72.0;
}

/// Field-performance block of the A400M.
///
/// The A400M has no published stall or approach speed at a stated mass, so its
/// coefficients come from the straight-wing slatless turboprop class of
/// `high_lift_system` (Obert DHC-7 and DHC-8, ATR 72 Vref), on the idle-power
/// stall of CS 25.103. The propeller wash over the flaps (of the order of the flap
/// system, Airbus P1) is real lift the short-field procedures fly on and CS 25.103
/// does not credit. Field lengths are CS 25 lengths, not tactical ones.
fn a400m_performance() -> Option<crate::PerformanceConfig> {
    let mut performance = super::high_lift("conservative_simple_flaps")?;
    // CS 25.107(b)(2): V2MIN is 1.08 VSR for turbo-propeller aeroplanes with
    // more than three engines (the A400M has four).
    performance.v2_min_vsr_factor = 1.08;
    Some(performance)
}
