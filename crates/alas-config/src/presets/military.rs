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
//! published. No value here is a calibration to A400M results or a physical
//! validation of the A400M.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MissionProfileConfig, WingConfig,
};

/// Overall length, m (S, T1 III.4).
const FUSELAGE_LENGTH_M: f64 = 45.091;

/// Airbus A400M Atlas, military standard, 141 t MTOW, four TP400-D6.
///
/// The cargo and payload model cannot represent military payload: the cargo
/// loader fills lower-deck LD3-class containers (about 24 t net, payload
/// capped near 25.4 t) and has no main-deck ramp-loaded vehicle or 463L
/// pallet positions. The design mission is therefore the brochure's 20 t over
/// 6,300 km point, which the loader can carry; the 37 t and 30 t points
/// cannot be represented.
pub fn a400m() -> AircraftPreset {
    let mut engine = EngineConfig {
        engine_name: "TP400-D6".to_owned(),
        ..EngineConfig::default()
    };
    // Materialize the catalogue entry (TP400-D6 ratings, FH385/FH386
    // propeller) before overriding the installation geometry.
    engine.apply_engine_spec();
    // E: ATR nacelle silhouette stretched to 5.5 m, radius 1.1 m; T2 gives
    // only the engine length 4.180 m and radius 1.218 m.
    engine.nacelle_profile = vec![
        (0.0, 0.35),
        (0.64, 0.9),
        (1.47, 1.0),
        (4.4, 0.8),
        (5.5, 0.35),
    ];
    engine.radius_scale_m = 1.1;
    // E: engine stations are not published. Inner/outer at 0.285 and 0.66 of
    // the 21.18 m semispan; the inner disc edge at 6.0 - 2.667 = 3.33 m
    // clears the 2.8 m fuselage radius.
    engine.spanwise_positions_m = vec![14.0, 6.0, -6.0, -14.0];
    engine.z_m = -1.0; // E
    engine.inlet_x_offset_m = 4.5; // E: spinner/inlet ahead of the wing LE

    AircraftPreset {
        name: "A400M",
        display_name: "Airbus A400M Atlas",
        description: "Airbus A400M Atlas, military standard (141 t MTOW), four TP400-D6. Payload limitation: the cargo model has no main-deck military loading, so payload is capped near 25.4 t by lower-deck containers and the design mission is the 20 t over 6300 km point; the 37 t and 30 t brochure points are not representable. Many geometry inputs are estimates.",
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
                range_nmi: 3_400.0,
                payload_kg: Some(20_000.0),
                source: "Airbus A400M brochure TMMA0026/01/2025 p024 range-payload table: 20 t at 6300 km (3400 nmi, read as 6300 km / 1.852); reserves, altitude and speed not stated. The 37 t / 3300 km and 30 t / 4450 km points are not used because the cargo model cannot carry more than about 25.4 t",
            }),
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            sources: vec![
                "EASA TCDS EASA.A.169 Airbus A400M, Issue 07, 28 November 2025 (civil A400M-180 WV001): span 42.357 m, length 45.091 m, width 5.600 m, MAC 5.671 m, MMO 0.72, fuel tanks (III.9, 0.785 kg/L), gear wheel counts (III.21)",
                "EASA TCDS EASA.E.033 TP400-D6, Issue 08, 29 March 2021: ratings, dry mass 1,938.1 / 1,965.1 kg",
                "EASA TCDS P.012 FH385/FH386 propeller, Issue 04, 15 December 2015: eight blades, 683 kg maximum assembly mass",
                "Airbus Defence and Space, A400M The 21st Century Airlifter, TMMA0026/01/2025, p024: military MTOW 141 t, MLW 123 t, maximum payload 37 t, range-payload table, p012 cruise M0.72",
                "Bundeswehr A400M transport aircraft page (accessed 2026-10-04): empty weight 78.6 t, definition not stated",
            ],
            ..AircraftReferenceData::default()
        },
        engine_name: "TP400-D6",
        n_engines: 4,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,        // S: T1 III.21
            n_mlg_struts: 6,        // S(secondary): three twin-wheel legs per side
            wheels_per_mlg_strut: 2, // S: T1 III.21, 12 main wheels in total
            // E: 6.0 m track over the 5.6 m fuselage width.
            track_diameter_factor: 6.0 / 5.6,
            // Gear stations, all E except where noted. The 13.4 m wheelbase is
            // a very-low-confidence secondary figure, the nose gear 6.1 m aft
            // of the nose is a guess, and the 6.0 m track is a guess (the
            // secondary 7.9 m is probably the outer tyre width). The main
            // legs sit at 19.5 m +- 1.2 m (E leg spacing).
            reference_wheelbase_m: Some(13.4),
            reference_track_m: Some(6.0),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(FUSELAGE_LENGTH_M),
            reference_nlg_x_fraction: Some(6.1 / FUSELAGE_LENGTH_M),
            reference_mlg_x_fractions: Some(
                [18.3, 19.5, 20.7, 18.3, 19.5, 20.7]
                    .iter()
                    .map(|x| x / FUSELAGE_LENGTH_M)
                    .collect(),
            ),
            // No published tail-strike attitude backs this preset's aft-fuselage
            // geometry, so its model tail-down angle is unvalidated (the generic
            // tailcone loft understates it): keep the Raymer/Roskam 15 deg floor on
            // top of the tail-down criterion.
            min_tip_back_deg: 15.0,
            // E: trimmable horizontal stabiliser at the class takeoff setting;
            // the A400M THS travel is unpublished.
            takeoff_stabilizer_nose_up_deg: Some(
                crate::landing_gear::TRANSPORT_THS_TAKEOFF_NOSE_UP_DEG,
            ),
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 42.357, // S: T1 III.4
            // I: single trapezoid (no kink, taper 0.33) reproducing the
            // TCDS MAC 5.671 m at S = 221.5 m2; the chords are not published.
            root_chord_m: 7.86,
            // E: ALAS needs a break station; the planform has none, so the
            // break chord is on the straight root-tip line at 0.32 semispan.
            break_chord_m: 7.86 + (2.60 - 7.86) * 0.32,
            tip_chord_m: 2.60, // I
            // I: leading-edge sweep from a 15 deg quarter-chord sweep (S1,
            // secondary) and taper 0.33.
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
        airfoil_class_source: "E: Airbus A400M sections are proprietary. Supercritical class declared for the M0.72 cruise (SC(2)-0714 root, SC(2)-0410 tip as generic stand-ins); no source states the technology level.",
        geometry: GeometryConfig {
            wing: WingConfig {
                // E: LEMAC (hence wing placement) is unpublished (WBM not
                // public). Chosen so the main gear (19.5 m) sits about 1.1 m
                // behind a 30 %MAC CG: LEMAC 17.4 m, MAC LE 2.9 m aft of the
                // root LE.
                root_datum_x_m: 14.5,
                // E: high wing, root above the 2.8 m fuselage crown; flat
                // wing (dihedral unpublished).
                root_z_m: 3.3,
                break_z_m: 3.3,
                tip_z_m: 3.3,
                root_twist_deg: 2.0,  // E
                break_twist_deg: 0.6, // E: linear root to tip
                break_span_fraction: 0.32,
                kink_span_fraction: None,
                side_of_body_chord_ratio: None,
                outboard_sweep_decrement_deg: 0.0,
                root_airfoil: "SC2-0714".to_owned(), // E
                tip_airfoil: "sc20410".to_owned(),   // E
                airfoil_class: crate::AirfoilClass::Supercritical,
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // T-tail. S1 (design stage 2004, secondary): horizontal tail
                // span 19.03 m, vertical tail height 8.02 m. Everything else is
                // E (areas unpublished).
                hstab_offset_from_tail_m: FUSELAGE_LENGTH_M - 36.5, // E
                hstab_z_m: 2.0 + 8.02, // E fin root 2.0 m + S1 fin height 8.02 m
                hstab_root_chord_m: 4.4, // E
                hstab_tip_chord_m: 1.9,  // E: horizontal tail area 59.9 m2
                hstab_root_twist_deg: 0.0,
                hstab_tip_twist_deg: 0.0,
                // Half-span 9.515 m (S1); leading-edge sweep 34 deg is E.
                hstab_tip_le_m: (6.42, 9.515, 0.0),
                vstab_offset_from_tail_m: FUSELAGE_LENGTH_M - 30.9, // E
                vstab_z_m: 2.0,                                     // E
                vstab_root_chord_m: 8.0,                            // E
                // E: equals the horizontal-tail root chord (T-tail join).
                vstab_tip_chord_m: 4.4,
                // Height from S1; sweep 35 deg is E.
                vstab_tip_le_m: (5.6, 0.0, 8.02),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 5.6, // S: T1 "Width", read as fuselage maximum width
                // Circular section assumed (E): the height is unpublished.
                height_m: None,
                nose_z_m: 0.0,
                cabin_start_x_m: 8.0, // E: hold starts behind the flight deck
                cabin_z_m: 0.0,
                // E: hold 17.7 m (B1) plus the 5.4 m ramp, 23.1 m, ends at 31.1 m.
                tailcone_length_m: 14.0,
                tail_z_m: 3.0, // E: strong aft upsweep for the ramp
                // No tail-strike angle is published; left unset.
                belly_upsweep_length_m: None,
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
            // E: VMO is 154.3 m/s IAS (T1); VD is not published.
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
/// E: the A400M high-lift system (FVF flaps plus propeller-wash lift) has no
/// published CLmax (Airbus P1 states only that the slipstream increment is of
/// the order of the flap system). The generic 'conservative_simple_flaps'
/// values (CLmax_TO 1.60) make the declared 141 t aeroplane fail the model's
/// takeoff lift-coefficient guard at any plausible takeoff speed, so the
/// effective values the ATR 72-600 recovers from its own published V2 and Vref
/// (CLmax_TO 2.44, CLmax_land 2.63) are carried as a stand-in. They are of the
/// order of the 2.45 power-on landing CLmax inferred from the S4 Vat of 130 kt
/// (research note Item 6, assumed landing mass 100 t), but they are ATR
/// numbers and not A400M data: model field lengths for this aircraft are
/// conceptual and too long for a tactical airlifter.
fn a400m_performance() -> Option<crate::PerformanceConfig> {
    let mut performance = super::high_lift("conservative_simple_flaps")?;
    performance.cl_max_to = 2.44;
    performance.cl_max_to_source = "E: no published A400M CLmax; ATR 72-600 effective CLmax_TO recovered from its published V2 minimum carried as a stand-in (see presets/military.rs)".to_owned();
    performance.cl_max_land = 2.63;
    performance.cl_max_land_source = "E: no published A400M CLmax; ATR 72-600 effective CLmax_land recovered from its published Vref carried as a stand-in; same order as the 2.45 inferred from the S4 Vat of 130 kt (see presets/military.rs)".to_owned();
    // 14 CFR / CS 25.125: Vref not less than 1.23 VSR0.
    performance.vapp_vstall_land_factor = 1.23;
    Some(performance)
}
