// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Embraer E195-E2 (type-certificate model ERJ 190-400) preset.
//!
//! Source map. The EASA TCDS EASA.IM.A.071 Issue 28 (3 Sep 2026), Section 5 (ERJ 190-400) is the
//! certified source for dimensions, weights, MAC, the datum, seating and exits. The Embraer E195-E2
//! specification sheet (April 2025, `embraer.com/media/ue1bdfnq/e195-e2-spec-1.pdf`) supplies the
//! usable fuel, maximum payload, range and cabin counts. The outer geometry is fitted to Embraer
//! E-Jets E2 APM 5824 Rev 21 Figure 2.2 (195-E2, PDF p21). The E190-E2 to E195-E2 stretch is 5.366
//! m (TCDS length 36.237 m -> 41.603 m); the EASA datum-to-wing-stub-front-spar distance grows from
//! 13,571 mm (ERJ 190-300) to 15,903 mm (ERJ 190-400), so, with the datum fixed to the nose, the
//! forward plug in the hold stations is 2.332 m.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CertifiedExitLayout,
    CertifiedExitPair, CgEnvelopeEvidence, DesignRequirements, DesignVector, EmpennageConfig,
    EngineConfig, FuselageConfig, GeometryConfig, LandingGearConfig, MissingDesignMissionDatum,
    MissionEvidenceApplicability, PartialDesignMissionEvidence, PartialMissionEvidenceKind,
    PublishedRange, SourcedPlanningCabin, SourcedSeatClass, WingConfig,
};

/// Forward plug between the E190-E2 and the E195-E2, m: the difference of the
/// two TCDS datum-to-front-spar distances, 15,903 mm - 13,571 mm (EASA.IM.A.071
/// Issue 28, Section 4 item III.15 and Section 5 item III.15).
const FORWARD_PLUG_M: f64 = 2.332;

/// EASA TCDS overall length of the ERJ 190-400, m.
const LENGTH_M: f64 = 41.603;

/// APM Rev 21 Fig 2.2 (195-E2) prints: nose tip to nose-gear centreline, m.
const NOSE_GEAR_X_M: f64 = 4.55;

/// APM Rev 21 Fig 2.2 (195-E2) prints: nose gear to main gear, m.
const WHEELBASE_M: f64 = 15.71;

/// The ERJ 190-400's 146-seat maximum is the sum of two Type I and two Type III
/// pairs; the TCDS prints the sequence and the 146 limit.
pub const E195_E2_CERTIFIED_EXIT_LAYOUT: CertifiedExitLayout = CertifiedExitLayout {
    label: "I-III-III-I",
    pairs: &[
        CertifiedExitPair {
            exit_type: "I",
            station_m: None,
        },
        CertifiedExitPair {
            exit_type: "III",
            station_m: None,
        },
        CertifiedExitPair {
            exit_type: "III",
            station_m: None,
        },
        CertifiedExitPair {
            exit_type: "I",
            station_m: None,
        },
    ],
    upper_deck_pairs: &[],
    station_body_length_m: None,
    source: "EASA.IM.A.071 Embraer ERJ-190 TCDS Issue 28, 2026-09-03, Section 5 (ERJ 190-400) III.19-20 p.41 (MPSC 146 for I-III-III-I; Type I fwd and aft main doors, two Type III overwing pairs, Type I fwd and aft service doors)",
};

/// The two underfloor Class C holds as `(name, forward station, aft station,
/// volume)`, metres aft of the nose tip and cubic metres.
///
/// Volumes are certified: EASA.IM.A.071 Issue 28 Section 5 III.21, 14.77 m3
/// forward and 15.20 m3 aft. The stations are ESTIMATES. The E190-E2 APM
/// Figure 2.4 draws its holds as 6.36 m (5.03-11.39 m) and 7.89 m
/// (19.70-27.59 m) long (vector drawing, scale from the printed 22.55 m
/// between the outer hold ends); the forward hold ends at the wing front
/// spar and so moves aft with the 2.332 m forward plug, and the aft hold,
/// bounded by the aft pressure bulkhead, moves with the whole 5.366 m
/// stretch. The same datum method reproduces the E190 -> E195 forward hold
/// growth of the first-generation TCDS (datum 14,443 -> 15,256 mm = 0.81 m,
/// hold 12.5 -> 13.8 m3).
pub const E195_E2_HOLD_COMPARTMENTS: [(&str, f64, f64, f64); 2] = [
    ("Forward hold", 5.03, 11.39 + FORWARD_PLUG_M, 14.77),
    (
        "Aft hold",
        19.70 + FORWARD_PLUG_M,
        27.59 + (LENGTH_M - 36.237),
        15.20,
    ),
];

/// The E195-E2 lower holds as cabin-config compartments.
pub fn e195_e2_hold_compartments() -> Vec<crate::HoldCompartmentConfig> {
    E195_E2_HOLD_COMPARTMENTS
        .iter()
        .map(
            |&(name, x_start_m, x_end_m, volume_m3)| crate::HoldCompartmentConfig {
                name: name.to_owned(),
                x_start_m,
                x_end_m,
                volume_m3: Some(volume_m3),
                max_net_kg: None,
                deck: crate::HoldDeck::Lower,
                container_rows: None,
                clear_width_m: None,
                narrow_forward_part_m: None,
                behind_seats: false,
            },
        )
        .collect()
}

/// Embraer's single-class E195-E2 with the 31 in pitch it advertises.
pub const E195_E2_PLANNING_CABIN: SourcedPlanningCabin = SourcedPlanningCabin {
    classes: &[SourcedSeatClass {
        class: "Economy",
        seats: 132,
        pitch_m: Some(31.0 * 0.0254),
        // Two-two seating: the E-Jet cabin is four abreast.
        abreast: Some(4),
        width_m: None,
    }],
    source: "Embraer E195-E2 specification sheet, April 2025, page 1 (single class configuration 132 seats @ 31 in; the same sheet prints 146 seats @ 28 in and 120 seats three-class 12 @ 36 in / 24 @ 34 in / 84 @ 31 in)",
};

/// The E195-E2 at the 62,500 kg weight variant, with PW1923G engines.
/// The Embraer E-Jets E2 APM 5824 Rev 25 (Table 2.2, PDF p17) lists the
/// standard model at 61,500 kg MTOW; its payload-range charts are drawn for
/// that weight, so model-versus-chart range differences include a 1,000 kg
/// variant offset. The SB 190E2-00-0058 weight is kept (EASA TCDS IM.A.071
/// Issue 28) until evidence supports changing the modelled variant (F16).
pub fn e195_e2() -> AircraftPreset {
    AircraftPreset {
        name: "E195-E2",
        display_name: "Embraer E195-E2",
        class: crate::presets::AircraftClass::Transport,
        description: "ERJ 190-400 (marketed E195-E2) at 62,500 kg MTOW with PW1923G engines.",
        identity: AircraftVariantIdentity {
            model: "ERJ 190-400 (E195-E2)",
            weight_variant: "62,500 kg MTOW (SB 190E2-00-0058)",
            engine_model: "PW1923G",
            modification_state: "EASA.IM.A.071 Issue 28 planning configuration",
            tank_configuration: "standard wing integral tanks, no centre tank",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Vol. I Table 1-1 applied to the TCDS span of
            // 35.124 m: 24 m <= b < 36 m is code C.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::C),
            // EASA.IM.A.071 Issue 28 Section 5 III.13: taxi and ramp 62,700 kg
            // (138,229 lb) with SB 190E2-00-0058, take-off 62,500 kg, landing
            // 54,000 kg (119,049 lb), zero fuel 51,850 kg (114,309 lb).
            mrw_kg: Some(62_700.0),
            mtow_kg: Some(62_500.0),
            mlw_kg: Some(54_000.0),
            mzfw_kg: Some(51_850.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("E195-E2"),
            // Embraer E195-E2 specification sheet: maximum usable fuel
            // 13,690 kg at the 0.803 kg/l it prints; 13,690 / 0.803 = 17,048.6 l.
            usable_fuel_volume_l: Some(17_048.57),
            usable_fuel_mass_kg: Some(13_690.0),
            fuel_density_kg_l: Some(0.803),
            // Embraer specification sheet single-class 132 seats at 31 in.
            planning_seats: Some(132),
            // The 3,000 nm of the specification sheet at MTOW, flown with the 132-seat
            // cabin at the APM's 100 kg per passenger; the APM Figure 3.3 chart reproduces it.
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: None,
                cruise: None,
                range_nmi: 3_000.0,
                payload_kg: Some(132.0 * 100.0),
                source: "Embraer E195-E2 specification sheet, April 2025, page 1 (Range Full PAX, LRC, typical reserves, 100 nm alternate: 3,000 nm / 5,556 km); at the 62,500 kg MTOW; payload 132 seats (sheet p.2, single class) x 100 kg per passenger (Embraer E-Jets E2 APM-5824 Rev 25 section 03 p.2); the APM Figure 3.3 chart at 61,500 kg plus 1,000 kg of fuel reads 2,998 nmi at that payload",
                reserve: Some(crate::presets::DesignPointReserve {
                    scheme: None,
                    contingency_trip_fraction: Some(0.0),
                    contingency_minimum_hold_min: Some(0.0),
                    alternate_distance_nmi: Some(100.0),
                    final_reserve_hold_min: Some(45.0),
                    flag_flight_time_fraction: None,
                    holding_altitude_ft: None,
                    source: "Embraer E195-E2 specification sheet, April 2025: Range Full PAX, LRC, Typical Reserves, 100 nm alternate; the Embraer E-Jets E2 APM-5824 Rev 25 section 03 Figure 3.3 prints the rule as 100 nm alternate + 45 min flight with no contingency term; the holding height is not printed, so it is the configured policy's",
                }),
            }),
            certified_max_seats: Some(146),
            certified_exit_layout: Some(E195_E2_CERTIFIED_EXIT_LAYOUT),
            planning_cabin: Some(E195_E2_PLANNING_CABIN),
            partial_design_mission_evidence: vec![PartialDesignMissionEvidence {
                kind: PartialMissionEvidenceKind::AdvertisedRange,
                range: Some(PublishedRange::NauticalMiles(3_000.0)),
                payload_kg: None,
                load_case: None,
                profile_assumptions: Some("long-range cruise (LRC), ISA, sea-level start"),
                reserve_assumptions: Some("typical reserves, 100 nm alternate"),
                reserve_contract: None,
                applicability: "Embraer advertises 3,000 nm with full passengers; the cabin layout, payload and takeoff weight behind that figure are not stated",
                configuration_applicability: MissionEvidenceApplicability::ModelAndEngineFamily,
                missing: vec![
                    MissingDesignMissionDatum::Payload,
                    MissingDesignMissionDatum::Profile,
                    MissingDesignMissionDatum::ReserveFuel,
                ],
                source: "Embraer E195-E2 specification sheet, April 2025, page 1",
            }],
            // TCDS Section 5 III.14: "Centre of Gravity: See Airplane Flight
            // Manual"; no planning envelope is public for the E195-E2.
            // Design mission, one provenance row per datum: the specification-sheet range at
            // MTOW with the 132-seat cabin; the APM defines the passenger mass, cruise and reserves.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "full passengers, long-range cruise, typical reserves, 100 nm alternate, 3,000 nm at the 62,500 kg MTOW; the APM Figure 3.3 chart (61,500 kg) at 132 x 100 kg plus 1,000 kg of fuel reproduces it (2,998 nmi)",
                    "Embraer E195-E2 specification sheet, April 2025, p.1 (Range Full PAX, LRC, Typical Reserves, 100 nm alternate); Embraer E-Jets E2 Airport Planning Manual APM-5824 Rev 25, section 03 Figure 3.3",
                )),
                payload: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "132 seats single class (31 in pitch) at 100 kg per passenger including baggage: 13,200 kg",
                    "Embraer E195-E2 specification sheet, April 2025, p.2 (cabin layouts); Embraer E-Jets E2 Airport Planning Manual APM-5824 Rev 25, section 03 p.2 (100 kg per passenger)",
                )),
                profile: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "ISA, flight level 370, long-range cruise Mach 0.78; the APM prints no climb or descent schedule, so the model's configured schedules apply to those phases",
                    "Embraer E-Jets E2 Airport Planning Manual APM-5824 Rev 25, section 03 Figure 3.3 (195-E2 payload x range, ISA)",
                )),
                reserve_fuel: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "100 nm alternate + 45 min flight, no contingency term printed; the reserve mass follows from the modelled trip",
                    "Embraer E-Jets E2 Airport Planning Manual APM-5824 Rev 25, section 03 Figure 3.3 (RESERVE: 100 nm ALTERNATE + 45 min FLIGHT)",
                )),
            },
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            reference_wing_area_m2: Some(103.0),
            aft_cg_nose_load: Some(crate::presets::gear_load::E195_E2),
            sources: vec![
                "EASA.IM.A.071 Embraer ERJ-190 TCDS Issue 28, 2026-09-03, Section 5 (ERJ 190-400) III.4 length 41.603 m, span 35.124 m, height 10.71 m, wing area 103 m2; III.11 maximum operating altitude 12,497 m (41,000 ft); III.13 weights; III.15 datum 15,903 mm ahead of the wing stub front spar; III.16 MAC 3.665 m; III.19-22 seating, exits, holds and tyres (nose 27x8.5R12, main H42x16.0R20)",
                "EASA.IM.A.071 Issue 28, Section 4 (ERJ 190-300) III.15 datum 13,571 mm ahead of the wing stub front spar and III.4 length 36.237 m, span 33.72 m",
                "Embraer E195-E2 specification sheet, April 2025 (https://www.embraer.com/media/ue1bdfnq/e195-e2-spec-1.pdf), page 1: MTOW 62,500 kg, MLW 54,000 kg, maximum payload 16,150 kg, usable fuel 13,690 kg at 0.803 kg/l, maximum cruise M 0.82, service ceiling 41,000 ft, 3,000 nm range",
                "Embraer E-Jets E2 Airport Planning Manual APM 5824 Rev 21, 2022-08-05, Figure 2.2 (195-E2, PDF p21): printed length 41.60 m, span 35.12 m, tailplane span 10.0 m, nose gear 4.55 m, wheelbase 15.71 m, track 6.73 m; plan, side and front views read for the planform, nacelle and tail; Tables 2.7-2.8 ground clearances",
                "ICAO Aircraft Engine Emissions Databank v32 (EASA, 2026) UID 04P20PW204 and EASA.IM.E.090 Pratt & Whitney PW1500G series TCDS Issue 11, 2026-09-21 (PW1923G rating 10,593 daN)",
            ],
            operating_speed_limits: Some(crate::presets::operating_limits::E195_E2),
            high_speed_protection: Some(crate::presets::operating_limits::E2_HIGH_SPEED_PROTECTION),
            ..AircraftReferenceData::default()
        },
        // Needs data: the PW1923G catalogue row (TCDS thrust, cruise SFC, nacelle) is not on disk; PW1900G is carried as a proxy.
        engine_name: "PW1900G",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            // Main tire: E-Jets E2 APM Fig. 7.2 prints H42X16.0R20 24PR. No data book rates it,
            // so the rated load is the dominated-envelope lower bound H41x15.0-19/24PR.
            mlg_tire: "H42x16.0R20/24PR".to_owned(),
            nlg_tire: "27x8.5R12/16PR".to_owned(),
            wheels_per_mlg_strut: 2,
            // 6.73 m track and 15.71 m wheelbase printed on APM Rev 21 Fig 2.2 (195-E2), over the 3.02 m body width.
            track_diameter_factor: 6.73 / 3.02,
            reference_wheelbase_m: Some(WHEELBASE_M),
            reference_track_m: Some(6.73),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(LENGTH_M),
            reference_nlg_x_fraction: Some(NOSE_GEAR_X_M / LENGTH_M),
            reference_mlg_x_fractions: Some(vec![(NOSE_GEAR_X_M + WHEELBASE_M) / LENGTH_M, (NOSE_GEAR_X_M + WHEELBASE_M) / LENGTH_M]),
            mlg_strut_bogie_wheels: Some(vec![2, 2]),
            // Belly clearance read from E-Jets E2 APM 5824 Rev 21 (2022-08-05, PDF p.31) Table 2.7 (E195-E2, -45 C, row 61,700 kg, CG 34 % MAC,
            // fuselage angle -0.37 deg): nose bottom (B) 2.33 m, nacelle low point (F) 0.37 m, wing tip lower surface (G) 3.90 m. The belly
            // is not tabulated: it is the ground plane that puts the three model heights on those readings at a level fuselage, 1.668 /
            // 1.680 / 1.598 m, mean 1.65 m. The +20 C table (2.8) reads 1.70 m: the cold-soak low end is kept.
            fuselage_ground_clearance_m: Some(1.65),
            // Tail-strike attitude 9.8 deg, static: APM 5824 Rev 21 Tables 2.7-2.9 (E195-E2, -45/+20/+54 C, PDF pp.31-33) print the
            // fuselage angle (A) and tail-skid angular clearance (N) per weight and CG; their sum, the fuselage-datum attitude at skid
            // contact, spans 9.80 (-0.70 + 10.5, +20 C, 54,000 kg, 40 %MAC) to 10.3 deg. The model fuselage sits level on its static
            // ground, so its scrape angle is that datum attitude; the minimum is the requirement. Raymer/Roskam 15 deg tip-back floor kept.
            min_tip_back_deg: 15.0,
            required_rotation_angle_deg: 9.8,
            required_rotation_basis: crate::landing_gear::RotationAngleBasis::PublishedStatic,
            trimmable_stabilizer: true,
            // FAA TCDS A57NM (Rev 27, p.10, ERJ 190-400 = E195-E2): stabiliser 12.0 deg TE up
            // (nose up), 3.5 deg TE down; elevator 25 deg TE up, 18 deg TE down (ERJ 190 family
            // entry of the same sheet).
            stabilizer_nose_up_travel_deg: Some(12.0),
            elevator_up_travel_deg: Some(25.0),
            ..LandingGearConfig::default()
        },
        // Planform: APM Rev 21 Fig 2.2 plan view read against its printed length and span. A straight 28.9 deg leading edge from the
        // body to the rake, an almost unswept inboard TE, the TE kink at 5.67 m. The published 103 m2 and MAC 3.665 m are held by
        // the reference wing below, not by the drawn outline (which is 110 m2 projected to the extreme tip).
        design_vector: DesignVector {
            span_m: 35.124,
            // Datum to the plan view of APM Rev 21 Fig 2.2 (195-E2): inboard TE x 21.09 m, TE line x 21.61 + 0.339 (y - 7.1) reaching
            // the kink at y 5.67 m (TE x 21.125 m) and x 25.156 m at the semispan.
            root_chord_m: 7.05, // datum, not lofted: centreline chord the plan outline implies; must not be under the SoB chord
            break_chord_m: 3.5954,
            tip_chord_m: 1.061, // straight edges only: the rake chord is below
            sweep_deg: 28.9,
            tip_twist_deg: -1.5,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: 41.60,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        speed_schedule_source: crate::presets::SpeedScheduleSource::SourcedLadder,
        airfoil_class_source: "Design-era declaration: the E2 wing (certified 2018) is of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC2-0714 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values; the drawn planform stays the geometry.
                // S 103 m2: Embraer E2 APM section 1 reference area and EASA.IM.A.071 TCDS III.4 wing area; MAC 3.665 m: the same TCDS.
                // LEMAC is not given there. Derived (not published): 17.958 m aft of the nose tip, which keeps the quarter-MAC point of the drawn MAC
                // (LE 17.881 m, MAC 3.975 m): 17.881 + 0.25 (3.975 - 3.665).
                // Needs data: the 17.958 m LEMAC is derived, not published; a published MAC leading-edge station would close it. Do not move LEMAC to pass the gear check.
                reference_wing: crate::ReferenceWingConfig::published(103.0, Some(3.665), Some(17.958)),
                // Root datum on the drawn straight leading edge: APM Rev 21 Fig 2.2 plan view at the printed 41.60 m scale, 28.9 deg.
                root_datum_x_m: 14.40,
                // Low wing. Heights: front view of the same figure, registered on the crown and fin tip.
                root_z_m: -0.7534,
                break_z_m: -0.2120,
                tip_z_m: 0.6450,
                root_twist_deg: 3.0,
                break_twist_deg: 1.0,
                break_span_fraction: 0.3228, // drawn TE corner
                kink_span_fraction: Some(0.3228),
                // Half the 3.02 m body width over the 17.562 m semi-span.
                side_of_body_span_fraction: Some(0.086),
                // SoB chord on the straight edges (TE x 21.09 m); edge_stations below draw the fillet on it.
                side_of_body_chord_ratio: Some(0.8309),
                outboard_sweep_decrement_deg: 1.5,
                root_airfoil: "SC2-0714".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                airfoil_class: crate::AirfoilClass::SupercriticalAdvanced,
                                // Thickness profile: the NASA Common Research Model t/c 0.154 at the
                // centreline, 0.138 at the side of body, 0.105 at the yehudi break and 0.095 at the tip
                // (Vassberg, DeHaan, Rivers and Wahls, AIAA 2008-6919, Table 2) is the public transport-wing
                // distribution; the multipliers below act on the root-kink-tip section blend above.
                thickness_stations: vec![
                    crate::WingThicknessStation {
                        span_fraction: 0.0000,
                        thickness_scale: 1.1037, // t/c 0.1395 -> 0.1540
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.0860,
                        thickness_scale: 0.9890, // t/c 0.1395 -> 0.1380
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.3228,
                        thickness_scale: 0.7525, // t/c 0.1395 -> 0.1050
                    },
                    crate::WingThicknessStation {
                        span_fraction: 1.0000,
                        thickness_scale: 0.9539, // t/c 0.0996 -> 0.0950
                    },
                ],
                // Tip of the plan view: the leading edge leaves the straight line at y 15.58 m and runs to a 0.40 m tip chord, the
                // trailing edge stays one line. Fillet: the plan outline reaches the fuselage side at x 14.75 m and 21.75 m (y 1.51 m)
                // and rejoins the straight edges at y 2.0 m. Offsets from those straight lines, read from the same figure.
                tip_rake_tip_chord_m: Some(0.40),
                tip_rake_start_span_fraction: Some(0.8870),
                edge_stations: vec![
                    crate::WingEdgeStation { span_fraction: 0.0860, le_offset_m: -0.48, te_offset_m: 0.66 },
                    crate::WingEdgeStation { span_fraction: 0.0968, le_offset_m: -0.17, te_offset_m: 0.64 },
                    crate::WingEdgeStation { span_fraction: 0.1139, le_offset_m: 0.0, te_offset_m: 0.025 },
                    crate::WingEdgeStation { span_fraction: 0.9168, le_offset_m: -0.08, te_offset_m: 0.0 },
                    crate::WingEdgeStation { span_fraction: 0.9453, le_offset_m: -0.15, te_offset_m: 0.0 },
                    crate::WingEdgeStation { span_fraction: 0.9737, le_offset_m: -0.24, te_offset_m: 0.0 },
                    crate::WingEdgeStation { span_fraction: 0.9851, le_offset_m: -0.24, te_offset_m: 0.0 },
                ],
                // Spanwise dihedral: leading-edge heights fitted to the upper edge of the APM Rev 21 Fig 2.2 front view, registered on
                // the crown and fin tip (the view is drawn level, with its ground line at the main wheels' bottom, 5.19 m below the crown).
                dihedral_stations: vec![
                    crate::WingDihedralStation {
                        span_fraction: 0.0860,
                        z_m: -0.6397,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.2000,
                        z_m: -0.4582,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.2900,
                        z_m: -0.2568,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.5000,
                        z_m: 0.0307,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.6500,
                        z_m: 0.2387,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.8000,
                        z_m: 0.4078,
                        flight_rise_m: None,
                    },
                ],
                // C2 cubic spline through the dihedral, twist and thickness stations.
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                ..WingConfig::default()
            },
            // Tail straight edges read off APM Rev 21 Fig 2.2 (195-E2) plan and side views. Tailplane span 10.0 m printed there;
            // the root and tip heights match the front view (upper surface) once it is registered on the crown and the fin tip.
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 5.31,
                hstab_z_m: 0.680,
                hstab_root_chord_m: 3.36,
                hstab_tip_chord_m: 1.28,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (3.418, 4.99, 0.649),
                // Rounded tip leading-edge corner, APM Rev 21 Fig 2.2 plan view: about 0.3 m long, 0.10 m aft at the tip.
                hstab_edge_stations: vec![
                    crate::TailEdgeStation { span_fraction: 0.94, le_offset_m: 0.0, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.97, le_offset_m: 0.03, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 1.0, le_offset_m: 0.10, te_offset_m: 0.0 },
                ],
                // Fin tip top 6.88 m (side view), same LE slope; tip chord 1.44 m at the tip rib.
                vstab_offset_from_tail_m: 6.51,
                vstab_z_m: 1.70,
                vstab_root_chord_m: 4.64,
                vstab_tip_chord_m: 1.44,
                vstab_tip_le_m: (4.40, 0.0, 5.18),
                // Rounded tip leading-edge corner, APM Rev 21 Fig 2.2 side view: leaves the straight edge at z 6.7 m, x 39.8 m at the tip.
                vstab_edge_stations: vec![
                    crate::TailEdgeStation { span_fraction: 0.9653, le_offset_m: 0.0, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.975, le_offset_m: 0.02, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.9846, le_offset_m: 0.08, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.9942, le_offset_m: 0.18, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 1.0, le_offset_m: 0.35, te_offset_m: 0.0 },
                ],
                // Dorsal fillet, geometry only. APM Rev 21 Fig 2.2 side view: leaves the crown at x 34.5 m and joins the fin leading edge at x 35.45 m, 0.55 m above it.
                vstab_dorsal_x_start_m: Some(34.6),
                vstab_dorsal_height_m: Some(0.70),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                // Width 3.02 m from the half-width of the E190-E2 APM plan
                // view (Figure 2.1, 1.51 m); height 3.35 m is the published
                // E-Jet envelope (ESTIMATE, the APM side view reads 3.4 m).
                diameter_m: 3.02,
                height_m: Some(3.35),
                nose_z_m: -0.77,
                cabin_start_x_m: 3.4,
                cabin_z_m: 0.1,
                // ESTIMATE: 19 % of the length, the A220 and A320 ratio.
                tailcone_length_m: 7.9,
                tail_z_m: 1.4,
                // FUSELAGE-G2 shaped nose and aft body. Outline traced from the manufacturer drawing
                // (side: same; top: Embraer E-Jets E2 APM Rev 21 Fig 2.2 (195-E2)),
                // calibrated to the drawn length; the nose law (windshield, radome, keel and plan
                // exponents) is a least-squares fit to the traced upper, lower and plan lines (RMS 0.014 m),
                // the aft law (belly ramp, crown drop, plan taper, closing section) to the traced aft lines
                // (RMS 0.089 m). Cabin start, tailcone length and every payload and mass station are unchanged:
                // only the outer loft moves. Section exponent 2.0 (ellipse): no drawing measures it.
                nose_length_m: Some(5.36),
                nose_windshield_angle_deg: Some(32.5),
                nose_crown_end_fraction: Some(0.99),
                nose_radome_length_fraction: Some(0.1),
                nose_keel_exponent: Some(2.65),
                nose_plan_exponent: Some(1.93),
                nose_section_exponent: Some(2.0),
                belly_upsweep_length_m: Some(11.88),
                aft_belly_exponent: Some(1.3),
                tail_end_z_m: Some(0.98),
                tail_end_width_m: Some(0.35),
                tail_end_height_m: Some(0.52),
                aft_plan_length_m: Some(8.93),
                aft_plan_exponent: Some(1.0),
                aft_crown_length_m: Some(10.37),
                aft_crown_exponent: Some(1.7),
                // Cabin section: Embraer E-Jets E2 APM 5824 fig. 2.20 (vector): floor 2.115 m, hold ceiling 2.256 m below the crown, 0.95 m high, 0.74 m flat floor, 1.65 m between walls at 0.50 m above it; cabin-datum-evidence.md.
                cabin_section: Some(crate::CabinSectionConfig { cabin_widths: Some(crate::CabinWidthsConfig { armrest_m: 2.74, shoulder_m: 2.54 }), floor_below_crown_m: Some(2.115), hold: Some(crate::HoldSectionConfig { ceiling_below_crown_m: Some(2.256), height_m: 0.95, floor_width_m: 0.74, side_width_m: 1.65, chamfer_height_m: 0.5 }) }),
                ..FuselageConfig::default()
            },
            // Nacelle outline in the engine catalogue. Axis from APM Rev 21 Fig 2.2: plan view y 4.84 m, front-view circle centre z -1.58 m; lip at x 13.41 m (plan and side views).
            engine: EngineConfig {
                nacelle_model: crate::NacelleModel::V2,
                spanwise_positions_m: vec![4.84, -4.84],
                z_m: -1.297,             // below the local wing leading edge, z -0.283 m
                inlet_x_offset_m: 3.662, // ahead of the local leading edge, x 17.072 m
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            // ESTIMATE: brochure LRC M 0.78 at 35,000 ft (2025 sheet: only the M 0.82 maximum).
            cruise_mach: 0.78,
            cruise_altitude_m: 10_668.0,
            mtow_kg: 62_500.0,
            max_wing_area_m2: 110.0,
            min_wing_loading_kg_m2: 500.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 132,
            cargo_payload_kg: 13_000.0,
            // Embraer spec sheet maximum payload (also MZFW 51,850 kg less the 35,700 kg BOW).
            max_structural_payload_kg: 16_150.0,
            // Superseded at registration by the CS 25.335(b) value on the TCDS VMO.
            dive_speed_m_s: 175.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("modern_narrowbody"),
    }
}
