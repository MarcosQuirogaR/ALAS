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
//! published; **D** = measured on the Airbus brochure three-view (B1 p025,
//! local PDF page 13, a CGI render with printed overall length 45.1 m, span
//! 42.4 m and height 14.7 m, scale checked on span and length: top view
//! 39.4 px/m, side view 40.1 px/m, front view 19.9 px/m; confidence C, about
//! +-0.2 m). No value here is a calibration to A400M results or a physical
//! validation of the A400M.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MissionProfileConfig, WingConfig,
};

/// The overall length of 45.091 m (S, T1 III.4) is measured to the trailing
/// edge of the horizontal-tail tips (D: the swept T-tail tips are the aft-most
/// point of the top view), not to the end of the fuselage.
///
/// Nose to end of the fuselage tail cone, m (D: side view, the rounded end of
/// the upswept tail sits at 39.5 m +- 0.2; the tailcone loft closes to a point,
/// so 39.8 m keeps the model within the drawn end).
const FUSELAGE_LENGTH_M: f64 = 39.8;

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
    // D: TP400 nacelle silhouette from the brochure top view, measured from the
    // spinner tip: the maximum diameter is 1.65 m (radius 0.82 m, about 2.4 m
    // behind the spinner tip), the spinner base 1.0 m across, and the body
    // reaches back about 7 m to the exhaust fairing under the wing (side view,
    // +-0.5 m). T2 gives only the engine length 4.180 m and radius 1.218 m, an
    // engine-internal radius that is not the nacelle skin.
    engine.nacelle_profile = vec![
        (0.0, 0.12),
        (0.5, 0.55),
        (1.0, 0.65),
        (2.0, 0.88),
        (2.6, 1.0),
        (4.0, 0.95),
        (5.5, 0.75),
        (7.0, 0.35),
    ];
    engine.radius_scale_m = 0.82;
    // D: engine axes measured on the brochure top view (spinner centres at
    // y = 6.79 and 12.74 m) and front view (6.72 and 12.71 m): 6.75 m and
    // 12.72 m, +-0.15 m. The inner disc edge at 6.75 - 2.667 = 4.08 m clears the
    // 2.8 m fuselage radius (propeller diameter 5.334 m, T3).
    engine.spanwise_positions_m = vec![12.72, 6.75, -6.75, -12.72];
    // D: spinner centres 0.9 m (inner) and 0.65 m (outer) above the fuselage
    // axis (side view), 1.9 m below the wing upper surface (front view); the
    // wing leading-edge height is 0.5 m under the upper surface, so about
    // 1.4 m below it.
    // Step B w1 (measure/A400M, confidence C, brochure p.25 front hub centroids): the inner and the outer engine
    // axes are both 0.15-0.19 m higher in the drawing than in the model (1.25 vs 1.10 m, 0.86 vs 0.67 m above the
    // axis); two independent measurements of the same sign, mean +0.17 m (render scale uncertainty 0.1 m). Was -1.4.
    engine.z_m = -1.23;
    // D: spinner tips 4.5 m (inner) and 4.8 m (outer) ahead of the leading edge
    // on the engine axes (top view).
    engine.inlet_x_offset_m = 4.65;

    AircraftPreset {
        name: "A400M",
        display_name: "Airbus A400M Atlas",
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
                range_nmi: 3_400.0,
                payload_kg: Some(20_000.0),
                source: "Airbus A400M brochure TMMA0026/01/2025 p024 range-payload table: 20 t at 6300 km (3400 nmi, read as 6300 km / 1.852); reserves, altitude and speed not stated. The 37 t / 3300 km and 30 t / 4450 km points are not adopted as the design point (the pallet floor and ramp could carry them)",
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
            // D: 6.9 m between the main-gear wheel centres of the two sides
            // (front view, 3.45 m each side of the axis) over the 5.6 m
            // fuselage width.
            track_diameter_factor: 6.9 / 5.6,
            // Gear stations from the brochure side and front views (D, +-0.2 m):
            // nose wheels 5.3 m aft of the nose tip, main-gear axles at 16.9,
            // 18.65 and 20.4 m (1.75 m axle spacing, three axles per side). The
            // 13.35 m wheelbase (nose gear to the middle axle) agrees with the
            // very-low-confidence secondary 13.4 m. The secondary 7.9 m track
            // is probably the outer tyre width.
            reference_wheelbase_m: Some(13.35),
            reference_track_m: Some(6.9),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(FUSELAGE_LENGTH_M),
            reference_nlg_x_fraction: Some(5.3 / FUSELAGE_LENGTH_M),
            reference_mlg_x_fractions: Some(
                [16.9, 18.65, 20.4, 16.9, 18.65, 20.4]
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
            // D: the brochure top view confirms one straight leading edge
            // (x = 12.5 + 0.332 y, 18.4 deg) and one straight trailing edge
            // (4.2 deg aft), a root chord of 8.0 m +- 0.2 m on the centreline
            // and a tip chord of 2.57 m.
            root_chord_m: 7.86,
            // E: ALAS needs a break station; the planform has none, so the
            // break chord is on the straight root-tip line at 0.5 semispan.
            break_chord_m: 7.86 + (2.60 - 7.86) * 0.5,
            tip_chord_m: 2.60, // I
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
        airfoil_class_source: "E: Airbus A400M sections are proprietary. Supercritical class declared for the M0.72 cruise (SC(2)-0714 root, SC(2)-0410 tip as generic stand-ins); no source states the technology level.",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values (geometry freeze 2026-10-12, owner decision); the drawn planform stays the geometry.
                // S 221.5 m2 and MAC 5.671 m: EASA TCDS EASA.A.169 Airbus A400M (Issue 07, 2025-11-28); LEMAC not published: the MAC leading edge
                // is the drawn MAC leading edge, 15.415 m aft of the nose tip (the drawn MAC, 5.6708 m, equals the TCDS value).
                reference_wing: crate::ReferenceWingConfig::published(221.5, Some(5.671), Some(15.415)),
                // D: the leading edge of the brochure top view is the straight
                // line x = 12.5 + 0.332 y (checked at 14 stations from y = 4.5 to
                // 20.5 m, scatter 0.1 m), so the centreline apex sits at 12.5 m
                // (the earlier 14.5 m came from fitting the main gear to a
                // chosen LEMAC and put the wing 2.0 m too far aft). LEMAC
                // follows at 15.5 m.
                root_datum_x_m: 12.5,
                // D: high wing. The root upper surface reaches z = 3.15 m (side
                // view hump over the 2.8 m crown); the leading-edge datum of the
                // loft sits under it, and is held 0.02 m above the crown so the
                // model reads a high wing (no wing-root gear bay, fuselage-
                // mounted main gear) as the A400M is.
                root_z_m: 2.82,
                // D: anhedral (front view, wing upper edge relative to the
                // root): 0.15 m down at y = 3.8 m, 0.5 m at 10.9 m, 1.3 m at
                // 16.6 m and 1.75 m at the tip (21.1 m), i.e. about 2.9 deg out
                // to 10.6 m and 6.9 deg outboard of it. This is the shape of the
                // rendered aircraft on its gear; the 1 g flight shape is not
                // published and no flight rise is declared.
                break_z_m: 2.32,
                tip_z_m: 1.07,
                root_twist_deg: 2.0,  // E
                break_twist_deg: 0.0, // E: linear root to tip
                break_span_fraction: 0.5,
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
                // D (top and side views): the horizontal tail root leading edge
                // is at 36.9 m, 10.15 m above the fuselage axis (tail plane on the
                // fin-tip fairing); root chord 4.55 m, tip chord 2.1 m, leading
                // edge swept 33 deg and the tip trailing edge at 45.09 m, the
                // published overall length (T1). Half-span 9.515 m (S1).
                // geo/tailfit 2026-10-11: tailplane straight-edge fit to the drawing, centreline values; was off FUSELAGE_LENGTH_M - 36.9, chords 4.55/2.1, tip (6.09, 9.515, 0.0)
                hstab_offset_from_tail_m: FUSELAGE_LENGTH_M - 36.75,
                hstab_z_m: 2.7 + 7.45,
                hstab_root_chord_m: 4.94,
                hstab_tip_chord_m: 2.17,
                hstab_root_twist_deg: 0.0,
                hstab_tip_twist_deg: 0.0,
                hstab_tip_le_m: (6.23, 9.515, 0.0),
                // D (side view): the fin is a parallelogram of constant 6.8 m
                // chord (measured at 6 heights, 6.78 to 6.83 m) with both edges
                // swept 34 deg (0.68 m aft per metre of height); the leading edge
                // meets the fuselage top (2.7 m) at 31.7 m. The fin tip is cut at
                // the tail plane (10.15 m), 7.45 m above its root (S1 gives a fin
                // height of 8.02 m): the drawn fin reaches 11.2 m through a tip
                // cap above the tail plane, which this attached T-tail cannot
                // hold.
                // geo/tailfit 2026-10-11: fin straight-edge fit to the drawing at the root line (rms 0.014/0.014 m); was off FUSELAGE_LENGTH_M - 31.7, chords 6.9/6.7, tip (5.2, 0.0, 7.45)
                vstab_offset_from_tail_m: FUSELAGE_LENGTH_M - 31.72,
                vstab_z_m: 2.7,
                vstab_root_chord_m: 6.84,
                vstab_tip_chord_m: 6.70, // geo/close 2026-10-12: drawn cap TE bends slightly forward at the top (6.83 left 1.4 m2 over, 6.55 left 2.0 m2 under); was 6.83
                // geo/close 2026-10-12 (owner: fin too small vs the render): the fin continues 0.93 m above the tailplane seat to the drawn cap top (side render: drawn top 11.075 m vs 10.15 m); same straight
                // LE/TE shear lines (LE offset 5.03 -> 5.654 m, chord unchanged). The tailplane stays at 10.15 m, so it is no longer on the fin tip in (x, z): attachment relaxed, conflict recorded in
                // geo-close-2026-10-12.md. was tip (5.03, 0, 7.45)
                vstab_tip_le_m: (5.654, 0.0, 8.375),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 5.6, // S: T1 "Width", read as fuselage maximum width
                // Hull height 5.26 m (loft fit round 4, "Fit round 4" in
                // loft-fit-2026-10-11.md; geometry phase step A): the drawn hull
                // WITHOUT sponsons, gear bays and wing-root fairing, measured on
                // the Airbus A400M brochure TMMA0026/01/2025 side render (class C;
                // no external height in TCDS A.169 or the brochure). Old value:
                // `None` (circular 5.6 m). Crown stays at 2.80 m (cabin_z_m 0.17 +
                // 5.26 / 2), so wing-root and tail attachments referenced to the
                // crown do not move; the keel is at -2.46 m. The main-gear
                // sponson (down to -3.2 m between 14 and 21 m) is not modelled.
                height_m: Some(5.26),
                // Drawing nose tip, mid of the nose dome (loft fit; old value 0.0).
                nose_z_m: -0.37,
                cabin_start_x_m: 8.0, // E: hold starts behind the flight deck
                // Centre of the cabin cylinder: crown 2.80 m - 5.26 / 2 (old 0.0).
                cabin_z_m: 0.17,
                // D (top view): the half-width stays 2.8 m to 29 m, then falls
                // to 2.6 m at 31 m, 2.3 m at 33 m, 2.0 m at 35 m and 1.4 m at
                // 37 m; a 10.7 m tailcone from 29.1 m is the closest of the
                // generated law (the real end, 0.9 m half-width and 0.3 m high at
                // 39.5 m, is blunt; the loft closes to a point).
                tailcone_length_m: 10.7,
                // D (side view): the rounded tail end is 2.8 m above the axis.
                tail_z_m: 2.8,
                // FUSELAGE-G2 shaped aft body, read from the brochure three-view
                // side and top views (CGI render, confidence C): the belly leaves
                // the keel at 24.1 m and rises, almost straight (0.34 per metre,
                // 18.7 deg), to the blunt tail end at the fuselage length, where
                // the closing section is 0.3 m high and 0.5 m wide; the crown stays
                // level to the end; the half-width starts to fall at 29.2 m
                // (2.55 m at 31 m, 2.2 m at 33 m, 1.65 m at 35 m, 1.05 m at 37 m).
                // Least-squares fit of the aft law to those reads, RMS 0.08 m.
                belly_upsweep_length_m: Some(15.67),
                aft_belly_exponent: Some(1.0),
                tail_end_z_m: Some(2.655),
                tail_end_width_m: Some(0.48),
                tail_end_height_m: Some(0.29),
                aft_plan_length_m: Some(10.62),
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
    // CS 25.107(b)(2): V2MIN is 1.08 VSR for turbo-propeller aeroplanes with
    // more than three engines (the A400M has four).
    performance.v2_min_vsr_factor = 1.08;
    Some(performance)
}
