// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airbus (CASA) C-295M preset, the baseline without winglets.
//!
//! Source key: T1 EASA TCDS EASA.A.186 Issue 08 (9 December 2025), Section 5,
//! Model C-295; J1 Jane's All the World's Aircraft 2008-2009 p.544-545, EADS
//! CASA C-295M entry; A1 Airbus C295 product page (accessed 2026-11-05); D =
//! measured on the J1 p.545 general arrangement by James Goulding, the best
//! drawing found. It prints no dimensions: each view is scaled on printed
//! values (plan 72.0 px/m in x on length 24.50 m and 73.8 px/m in y on span
//! 25.81 m; side 71.6 px/m in x and 70.1 px/m in z on height 8.66 m). Confidence
//! C, picking +-0.04 m, and it disagrees with printed values and with itself:
//! wheelbase 7.53 m against 8.435 m, tailplane 10.97 m against 10.60 m, and the
//! front view draws the fuselage about 3.1 m wide and the fin 0.7 m shorter. E =
//! engineering estimate. No value here is a calibration to C295 results or a
//! physical validation of the C295.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MassModelConfig, WingConfig,
};

/// Nose to tail end, m (T1 III.4 overall length; D: the plan and side outlines
/// end at the fuselage tail, not the tail surfaces). J1 prints 24.45 m.
const FUSELAGE_LENGTH_M: f64 = 24.50;

/// The C-295M cargo hold, seeded by `cabin_sources::seed_freight_hold`.
///
/// S: J1 p.545 DIMENSIONS, INTERNAL: length excluding ramp 12.69 m, including
/// ramp 15.73 m (ramp 3.04 m), width at floor 2.36 m, maximum height 1.90 m.
/// D: floor start behind the flight deck, just ahead of the crew door at 4.3 m.
/// E: ramp load limit, carried from the study brief, no source.
pub(super) const C295_FREIGHT_HOLD: crate::FreightHoldConfig = crate::FreightHoldConfig {
    floor_start_x_m: 4.0,
    floor_length_m: 12.69,
    ramp_length_m: 3.04,
    clear_width_m: 2.36,
    clear_height_m: 1.90,
    ramp_load_limit_kg: Some(4_000.0),
};

/// Airbus C-295M, 23,200 kg overload limits, two PW127G, no winglets.
pub fn c295() -> AircraftPreset {
    let mut engine = EngineConfig {
        engine_name: "PW127G".to_owned(),
        ..EngineConfig::default()
    };
    // Catalogue entry: ratings, 568F-5 propeller and the nacelle outline.
    engine.apply_engine_spec();
    // S: J1 p.545 distance between propeller centres 7.00 m. D: plan 7.14 m,
    // front 7.30 m; the printed value governs.
    engine.spanwise_positions_m = vec![3.50, -3.50];
    // D: nacelle centre 1.24 m above the fuselage axis, root LE datum 1.59 m: the top meets the wing
    // upper line and the bottom the side-view 0.6 m above the axis (propeller hub drawn 1.30-1.40 m).
    engine.z_m = -0.35;
    // D (plan): spinner tip at x 6.45 m, 3.43 m ahead of the built wing LE at the nacelle. The side
    // view draws the whole nacelle 0.46 m further aft; the plan fixes both ends
    // against the wing, so it governs.
    engine.inlet_x_offset_m = 3.43;

    AircraftPreset {
        name: "C295",
        display_name: "Airbus C295",
        class: crate::presets::AircraftClass::Transport,
        description: "Airbus (CASA) C-295M, the baseline without winglets (25.81 m span), 21,050 kg certified MTOW (TCDS EASA.A.186 Issue 08, Section 5 item 13, printed p.79), two PW127G with 568F-5 propellers. Payload is palletised on the 12.69 m hold floor and 3.04 m ramp. Geometry is read off a publisher general arrangement, not a manufacturer drawing.",
        identity: AircraftVariantIdentity {
            model: "C-295M (T1 Model C-295), not the C-295FW winglet variant",
            weight_variant: "21,000 kg MTOW, 20,700 kg MLW, 18,500 kg MZFW, 21,050 kg ramp: EASA TCDS EASA.A.186 Issue 08, Section 5 item 13 Maximum Certified Masses (printed p.79). The J1 p.545 overload limits (23,200 kg MTOW and MLW) are not certified under the TCDS and are not the baseline",
            engine_model: "PW127G",
            modification_state: "C-295 type design standard DT-5-ADG-99001, without winglets",
            tank_configuration: "main and auxiliary tank in each wing, 7,532 L usable (T1 III.9)",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Table 1-1: span 25.81 m lies in 24 m <= b < 36 m, code C.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::C),
            mrw_kg: Some(21_050.0),  // S: TCDS A.186 I08 item 13 p.79 "Ramp"
            mtow_kg: Some(21_000.0), // S: TCDS A.186 I08 item 13 p.79 "Take off"
            mlw_kg: Some(20_700.0),  // S: TCDS A.186 I08 item 13 p.79 "Landing"
            mzfw_kg: Some(18_500.0), // S: TCDS A.186 I08 item 13 p.79 "Zero Fuel"
            oew_kg: None,            // no empty mass in T1, J1 or A1
            // S: T1 III.9 tank sum 2 x (1,605 + 2,161) L. The printed totals (8,532 L,
            // 7,626 kg) do not add up and are not used. J1 prints 7,650 L.
            usable_fuel_volume_l: Some(7_532.0),
            usable_fuel_mass_kg: Some(6_026.0), // I: 7,532 L x 0.80 kg/L
            fuel_density_kg_l: Some(0.80),      // S: T1 III.9
            reference_wing_area_m2: Some(60.0), // S: T1 III.4
            planning_seats: None,
            // Payload-defined design mission: the range is the model's output at the certified
            // MTOW (PayloadRangeDesignPoint::MODEL_DEFINED_RANGE_NMI), not a declared number.
            // The point is the normal-weights one with a stated reserve basis (Jane's A); the
            // brochure's 6 t / 2,000 nm ring is drawn at the 23,200 kg overload mass, which
            // the TCDS does not certify, so it is not the certified aircraft's point.
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: Some("Jane's All the World's Aircraft 2008-09, EADS CASA C-295M, PERFORMANCE, printed p.546: the range with max fuel is a long-range-cruise range (T33 coordinator decision, item 5; the page prints no speed, so the range is flown at the model's long-range cruise: 99 % of maximum specific range, high-speed side)"),
                // Jane's p.546 prints no cruise level for its ranges. Source-typical level:
                // FL250 (7,620 m) at 260 kt TAS, inside the "up to 30,000 ft" Airbus states.
                // The preset's 6,100 m (requirements.cruise_altitude_m) is the study brief and
                // stays the sizing and route level; this level prices the charted point only.
                cruise: Some(crate::presets::DesignPointCruise {
                    altitude_m: 25_000.0 * 0.3048,
                    mach: None,
                    source: "Jane's p.546 states no cruise level; source-typical FL250 (25,000 ft = 7,620 m, ISA): CASA C-295 'cruising altitude 25,000 ft' at 260 KTAS (FAS Military Analysis Network, CN-295, 1999), inside the 'up to 30,000 ft' and 'standard cruise 260 kt' of the Airbus C295 product page; configured 260 kt-class true airspeed kept",
                }),
                range_nmi: crate::PayloadRangeDesignPoint::MODEL_DEFINED_RANGE_NMI,
                payload_kg: Some(2_940.0),
                source: "Jane's All the World's Aircraft 2008-09, EADS CASA C-295M, PERFORMANCE (C-295M at normal MTOW except where indicated), printed p.546: range with max fuel, A (normal weights) with 2,940 kg payload, 2,477 n miles (4,587 km), ISA, reserves for 45 min hold at 460 m. The C-295M has no winglets, as the preset (TCDS span 25.81 m). The design mission flies this payload at the certified MTOW with the stated reserve, and its range is the model's output: the largest range whose reserve fuel plan fits MTOW - OEW - payload, capped by the usable tanks less taxi fuel",
                reserve: Some(crate::presets::DesignPointReserve {
                    // Read as given, as the A400M's Jane's MIL-C-5011B quote is: the
                    // EASA basic scheme adds its 15 min no-alternate hold
                    // (CAT.OP.MPA.181) to a zero alternate, a term Jane's does not state
                    // (114 kg on the 2,940 kg point, about 56 n miles).
                    scheme: Some(crate::FuelScheme::StudyConvention),
                    contingency_trip_fraction: Some(0.0),
                    contingency_minimum_hold_min: Some(0.0),
                    alternate_distance_nmi: Some(0.0),
                    final_reserve_hold_min: Some(45.0),
                    flag_flight_time_fraction: None,
                    // 460 m printed; 1,509 ft.
                    holding_altitude_ft: Some(460.0 / 0.3048),
                    source: "Jane's All the World's Aircraft 2008-09, EADS CASA C-295M, PERFORMANCE, printed p.546: ranges quoted with reserves for 45 min hold at 460 m (1,500 ft); no contingency or alternate term is stated, so none is flown, and no regulatory scheme is named, so the terms are read as given (no EASA no-alternate hold)",
                }),
            }),
            // Payload-defined mission: 2,940 kg at the TCDS MTOW. The range is the model's
            // output at that payload and mass, scored against the 2,477 n miles Jane's prints;
            // Jane's prints no profile, so the model's cruise schedule applies. Jane's is a
            // reference annual, so the payload and reserve are adopted as the design
            // requirement (tier DesignRequirement) with the annual cited, not as manufacturer data.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::DesignRequirement,
                    "range is a model output (PayloadRangeDesignPoint::MODEL_DEFINED_RANGE_NMI) at the 2,940 kg payload and the TCDS MTOW; Jane's prints 2,477 n miles for it, which is the validation value, not an input",
                    "model output at the design payload; MTOW from EASA TCDS EASA.A.186 Issue 08, Section 5 item 13, p.79",
                )),
                payload: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::DesignRequirement,
                    "design payload 2,940 kg, the normal-weights range point with max fuel, flown at the TCDS MTOW",
                    "Jane's All the World's Aircraft 2008-09, EADS CASA C-295M, PERFORMANCE, printed p.546 (A: 2,940 kg payload, 2,477 n miles); MTOW from EASA TCDS EASA.A.186 Issue 08, Section 5 item 13, p.79",
                )),
                profile: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::DesignRequirement,
                    "Jane's prints no profile; the payload-defined mission flies the model's configured cruise Mach, altitude and speed schedules, ISA",
                    "payload-defined design mission of this preset (reference.design_point); preset mission block",
                )),
                reserve_fuel: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::DesignRequirement,
                    "45 min hold at 460 m, no contingency and no alternate stated; the reserve mass follows from the modelled hold",
                    "Jane's All the World's Aircraft 2008-09, EADS CASA C-295M, PERFORMANCE, printed p.546 (reserves for 45 min hold at 460 m)",
                )),
            },
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            sources: vec![
                "EASA TCDS EASA.A.186 CN-235/C-295, Issue 08, 9 December 2025, Section 5 III: span 25.81 m, length 24.50 m, height 8.66 m, wing area 60 m2, MAC 2.561 m, propeller 3.932 m, tanks, flaps, certified masses",
                "Jane's All the World's Aircraft 2008-2009 p.544-545, EADS CASA C-295M: tailplane 10.60 m, track 3.98 m, wheelbase 8.435 m, propeller centres 7.00 m, hold, normal and overload weights, general arrangement (James Goulding)",
                "Airbus C295 product page, accessed 2026-11-05: 9 t maximum payload, 260 kt cruise, about 1,500 km with 9 t",
                "Airbus Defence and Space brochure 'C295 The Reliable Workhorse', ref. TMMA0009/01/2025, pp.22-23, payload-range chart at the 23,200 kg overload mass (not TCDS-certified, context only, not scored against the baseline): 9 t maximum payload; 6 t over 2,000 nm; 3 t over 2,700 nm; 3,100 nm ferry range",
            ],
            operating_speed_limits: Some(super::operating_limits::C295),
            ..AircraftReferenceData::default()
        },
        engine_name: "PW127G",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,          // S: J1 p.545 twin nosewheels
            // S: T1 III.22, two main wheels in tandem each side; J1 p.545 "main gear as CN-235 but
            // reinforced"; EADS-CASA CN-235-300 Technical Review (2002), Landing Gear, p.26: "two
            // legs in tandem configuration on each side of the fuselage. Each leg has an independent
            // oleo-pneumatic type shock absorber ... Each main landing gear leg has one wheel". Four
            // legs of one wheel, so the aircraft tips back about the aft wheels, not the pair centre.
            n_mlg_struts: 4,
            wheels_per_mlg_strut: 1,
            track_diameter_factor: 3.98 / 2.82,
            reference_wheelbase_m: Some(8.435), // S: J1 p.545; the drawing shows 7.53 m
            reference_track_m: Some(3.98),      // S: J1 p.545; the front view shows 4.15 m
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(FUSELAGE_LENGTH_M),
            // D: nose wheels 2.73 m aft of the nose tip (side view, under the flight deck).
            // S: the main tandem pair centre follows from the printed wheelbase, 2.73 + 8.435
            // = 11.165 m, 44 %MAC on the drawn wing (LEMAC 10.04 m). The side view draws the
            // pair at 10.27 m, 0.23 m aft of LEMAC (9 %MAC): with the main gear there the
            // aircraft tips back at any CG aft of about -17 %MAC, ahead of every CG the
            // tail can trim. The drawn wing is the station the rest agrees with (hold centre
            // 10.35 m, propellers 3.43 m ahead of the LE and aft of the crew door), so the
            // drawing's 0.9 m wheelbase shortfall sits at the main gear.
            reference_nlg_x_fraction: Some(2.73 / FUSELAGE_LENGTH_M),
            // D: tandem wheel centres 0.91 m apart (side view, 99 px/m; the J1 p.293 CN-235 plan
            // by Punnett draws 1.27 tyre diameters, 0.90 m on the 28 in tyre). The printed wheelbase
            // is taken to the pair centre. Order: forward left, forward right, aft left, aft right.
            reference_mlg_x_fractions: Some(vec![
                (2.73 + 8.435 - 0.455) / FUSELAGE_LENGTH_M,
                (2.73 + 8.435 - 0.455) / FUSELAGE_LENGTH_M,
                (2.73 + 8.435 + 0.455) / FUSELAGE_LENGTH_M,
                (2.73 + 8.435 + 0.455) / FUSELAGE_LENGTH_M,
            ]),
            // D: keel 0.70 m above the side-view ground line.
            fuselage_ground_clearance_m: Some(0.70),
            // E: no published tail-strike attitude; Raymer/Roskam floor, as the A400M.
            min_tip_back_deg: 15.0,
            // Tail-scrape requirement: no published tail-strike attitude found (no C295 source found), so the 10
            // deg default (an assumption) stands.
            elevator_up_travel_deg: Some(25.0), // S: T1 III.12.2
            // S: T1 III.12.2 (printed p.78): normal trim tab up 0, down +12 deg; the
            // elevator balance tab goes +7.7 deg (trailing edge down) at -25 deg elevator.
            // A geared balance tab exists only on a reversible (unpowered) elevator.
            elevator_trim_tab_nose_up_deg: Some(12.0),
            elevator_balance_tab_gearing: Some(7.7 / -25.0),
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 25.81, // S: T1 III.4 Model C-295; the C-295FW is 27.59 m
            // S: T1 III.4 and III.16: chords solved so the built outline, rake included, is 60 m2
            // with a 2.561 m MAC. Constant chord to the drawn break y 4.24 m. The J1 p.545 outline scans
            // at 3.385/1.18 m, 68 m2, 2.85 m MAC, about 12 % too wide: the J1 p.293 CN-235 entry, a wing of the
            // same 25.81 m span, prints root 3.00 m, and its plan (Punnett) reads 3.06/2.54/1.36 m at y 3.9/6.45/11.6 m.
            root_chord_m: 3.058,
            break_chord_m: 3.058,
            tip_chord_m: 0.909,
            // S: Jane's All the World's Aircraft 2008-09, Airtech CN-235 entry, DESIGN FEATURES (the C-295 is its stretched
            // derivative, same 25.81 m span and wing): "tapered outer panels have ... 3 deg 51 min 36 s sweepback at
            // quarter-chord". Outer-panel LE from tan(LE) = tan(c/4) + (c_kink - c_tip) / (4 (y_tip - y_kink)) on the chords
            // above and the y 4.241 m break: tan 3.86 deg + 2.149 / 34.658 = 0.12948, 7.378 deg. Replaces the 6.66 deg that
            // held the drawn mid-chord line of a scan already 12 % wide (0.11 m at the tip, below that scan's resolution).
            sweep_deg: 7.378,
            tip_twist_deg: -2.0, // E
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: FUSELAGE_LENGTH_M,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        speed_schedule_source: crate::presets::SpeedScheduleSource::DefaultRule,
        airfoil_class_source: "Jane's All the World's Aircraft 2008-09, p 292: CN-235 wing NACA 65(3)-218; p 545: the C-295 stretches its fuselage and reinforces its wing; its own sections are not published. Conventional class for the M0.42 cruise. Thickness tapers to the 12 % of the front view",
        geometry: GeometryConfig {
            wing: WingConfig {
                // S: T1 III.4 and III.16, area 60 m2, MAC 2.561 m. The TCDS LEMAC is 11.115 m aft of a datum whose
                // offset from the nose tip is not printed (datum 2.367 m ahead of a skin rivet), so I: MAC LE of the built outline.
                reference_wing: crate::ReferenceWingConfig::published(60.0, Some(2.561), Some(10.04)),
                root_datum_x_m: 9.839, // D: plan, drawn centre-section mid-chord less half the root chord
                // D: wing upper line 0.55 m above the crown (front 0.63 m, side 0.56 m).
                root_z_m: 1.59,
                break_z_m: 1.59,
                // D: the upper surface stays level to the tip in the front view, only the lower rises.
                tip_z_m: 1.87,
                root_twist_deg: 2.0,  // E
                break_twist_deg: 0.0, // E
                // D: constant chord centre section ends at the nacelle, y 4.24 m.
                break_span_fraction: 0.3286,
                side_of_body_span_fraction: Some(0.1095), // D: fuselage half width 1.41 m
                side_of_body_chord_ratio: Some(1.0),
                kink_span_fraction: None,
                outboard_sweep_decrement_deg: 0.0,
                inboard_le_sweep_deg: Some(0.75), // D: plan, LE 0.05 m aft over the centre section
                // D: tip LE swept aft from y 12.34 m; the drawn tip to straight-edge chord ratio 0.415.
                tip_rake_tip_chord_m: Some(0.377),
                tip_rake_start_span_fraction: Some(0.956),
                // Jane's 2008-09 p 292, CN-235: NACA 65(3)-218; the C-295 stretches its fuselage and reinforces its wing (p 545).
                // Thickness: D, front view, 18 % at the root to 12 % at the tip, as drawn before.
                root_airfoil: "naca653218".to_owned(),
                tip_airfoil: "naca653218".to_owned(),
                thickness_stations: vec![
                    crate::WingThicknessStation {
                        span_fraction: 0.0,
                        thickness_scale: 1.0,
                    },
                    crate::WingThicknessStation {
                        span_fraction: 1.0,
                        thickness_scale: 12.0 / 18.0,
                    },
                ],
                airfoil_class: crate::AirfoilClass::Conventional,
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // D (plan): LE extrapolated to the centreline 21.50 m, root chord 2.555 m,
                // tip chord 1.36 m, tip LE 0.736 m aft. S: J1 p.545 span 10.60 m (drawn 10.97 m).
                // D (side): 1.53 m above the axis. The drawn root strake from x 21.0 m is not expressible.
                hstab_offset_from_tail_m: FUSELAGE_LENGTH_M - 21.50,
                hstab_z_m: 1.53,
                hstab_root_chord_m: 2.555,
                hstab_tip_chord_m: 1.36,
                hstab_root_twist_deg: 0.0,
                hstab_tip_twist_deg: 0.0,
                hstab_tip_le_m: (0.736, 5.30, 0.0),
                // D (side): root line 1.826 m above the axis, LE 20.87 m, root chord 3.155 m, tip
                // chord 1.55 m, tip LE 1.97 m aft. Top 6.55 m: S: T1 height 8.66 m over the 2.11 m ground.
                vstab_offset_from_tail_m: FUSELAGE_LENGTH_M - 20.87,
                vstab_z_m: 1.826,
                vstab_root_chord_m: 3.155,
                vstab_tip_chord_m: 1.55,
                vstab_tip_le_m: (1.97, 0.0, 4.72),
                // D (side): dorsal fin line, crown at x 17.38 m to the fin LE 1.80 m above the root line. The
                // builder starts it on the root line, which the drawn line crosses at x 17.98 m.
                vstab_dorsal_length_m: Some(3.49), // leaves the crown at 17.38 m
                vstab_dorsal_shoulder: Some(0.03), // the drawn line is nearly straight
                vstab_dorsal_height_m: Some(1.80),
                // Side view: closed outline under the keel, a ventral strake: lower edge (20.45, 0.82) (20.7, 0.62)
                // (22.4, 0.82) (22.55, 1.0). Only the side view draws it: the plan view is from above and the front
                // view hides it behind the fuselage, and no dimensioned front or rear view or photograph was found,
                // so count, y and cant are unknown. Kept is what the side view states: one surface of the drawn
                // side outline, built on the centreline (its side projection is the drawn outline; wetted area,
                // drag and paint read one fin of that plan, the least any count and cant could have).
                // The root station is held inside the body, at the keel height of the drawn upper edge.
                ventral_fin_stations: vec![
                    crate::VentralFinStation::drawn((20.87, 1.826), 0.0, (20.40, 1.10), (22.60, 1.40)),
                    crate::VentralFinStation::drawn((20.87, 1.826), 0.0, (20.45, 0.82), (22.55, 1.00)),
                    crate::VentralFinStation::drawn((20.87, 1.826), 0.0, (20.70, 0.62), (22.40, 0.82)),
                ],
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                // D: plan width 2.82 m, side height 2.81 m. J1 p.545 prints only the cabin (2.70 m wide). The
                // outer shape is the embedded loft data/lofts/c295.json, fitted to the same drawing; its
                // tallest section is 2.837 m at the start of the belly upsweep. Sponsons are not built.
                diameter_m: 2.82,
                height_m: Some(2.83),
                // D: side view x 6.1 to 13.2 m, plan lobes to y 2.3 m, front view about 0.1 m below the
                // keel. Parasite drag only.
                gear_sponson_length_m: Some(7.1),
                gear_sponson_half_width_m: Some(2.3),
                gear_sponson_depth_m: Some(0.1),
                nose_z_m: -0.556,     // D: side, nose tip
                cabin_start_x_m: 4.0, // D: see C295_FREIGHT_HOLD
                cabin_z_m: 0.0,
                tailcone_length_m: 9.84, // D: from the start of the belly upsweep at x 14.66 m
                tail_z_m: 1.54,          // D: side, tail end
                // Loadable floor and ramp end, 4.0 + 15.73 m.
                hold_interior_end_x_m: Some(19.73),
                ..FuselageConfig::default()
            },
            engine,
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            // S: A1 260 kt cruise; M0.42 at 6,100 m is 258 kt TAS, study brief altitude.
            cruise_mach: 0.42,
            cruise_altitude_m: 6_100.0,
            mtow_kg: 21_000.0,      // S: TCDS A.186 I08 item 13 p.79 "Take off"
            max_wing_area_m2: 62.0, // E: above 60 m2
            min_wing_loading_kg_m2: 300.0, // E: loose floor (MTOW/S = 387)
            aircraft_type: "cargo".to_owned(),
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: false,
            num_passengers: 0,
            cargo_payload_kg: 6_000.0,           // S: brochure TMMA0009/01/2025 pp.22-23, lowest published payload point (design payload)
            max_structural_payload_kg: 7_050.0,  // S: J1 p.545 A (normal weights; B is overload)
            // Superseded at registration: the profile declares the TCDS VMO/MMO, so VD is the
            // CS 25.335(b) value on VC = VMO (`crate::design_speeds`, `registry`).
            dive_speed_m_s: 163.0,
            // S: T1 Section 5 item 4.1, FAR 25 to amendment 25-89: 25.337(b) at the 21,000 kg
            // (46,297 lb) certified MTOW, n = 2.1 + 24,000 / (W + 10,000) = 2.526, with the
            // 1.5 factor of 25.303. The 23,200 kg overload mass (J1 p.545 B) is not certified.
            ultimate_load_factor: crate::requirements::cs25_ultimate_load_factor(21_000.0),
            ultimate_load_factor_from_mtow: true,
            ..DesignRequirements::default()
        },
        mass_model: Some(MassModelConfig {
            mlw_fraction_mtow: 20_700.0 / 21_000.0, // S: TCDS A.186 I08 item 13 p.79, MLW / MTOW
            ..MassModelConfig::default()
        }),
        performance: super::high_lift("conservative_simple_flaps"),
    }
}
