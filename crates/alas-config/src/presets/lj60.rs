// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Learjet 60 preset (Bombardier Learjet Model 60, PW305A).
//!
//! Variant: the 10,659 kg (23,500 lb) optional takeoff weight of ECR 3845 with
//! the 7,711 kg (17,000 lb) zero-fuel weight of SB 60-8-1, not the 10,319 kg
//! TCDS standard. It is the weight every public performance figure is quoted at
//! (Jane's 2008-09 p 791) and the one whose CG envelope the FlightSafety manual
//! prints (fig 19-1).
//!
//! Source map. Weights and fuel: EASA TCDS IM.A.212 Issue 04 III.5-III.7; the
//! Learjet 60 AFM Rev 37 limitations for the SB weight. External dimensions,
//! shapes and stations: FlightSafety Learjet 60 Pilot Training Manual, fig 1-2
//! (three-view with printed dimensions) and fig 19-2 (fuselage stations), and
//! Jane's All the World's Aircraft 2008-09 pp 825-826. The three-view is a
//! training-manual drawing, not a manufacturer engineering drawing. Frame: x aft
//! of the nose tip, z up from the cabin barrel centre, metres; x [m] = 0.0254
//! (F.S. [in] - 12.77), the nose tip being F.S. 12.77 in (fig 19-2), which puts
//! the 99.57 in forward pressure bulkhead 7.07 m ahead of the 378.05 in aft
//! bulkhead, the cabin length Jane's prints.

use crate::{
    AftFuselageEngineInstallation, AircraftPreset, AircraftReferenceData, AircraftVariantIdentity,
    CertifiedExitLayout, CertifiedExitPair, CgEnvelopeEvidence, DesignRequirements, DesignVector,
    EmpennageConfig, EngineConfig, FuselageConfig, GeometryConfig, LandingGearConfig, WingConfig,
};

/// The Learjet 60's emergency exits: the main passenger door on the left,
/// forward, a Type I exit of clamshell design, and a Type III escape hatch on
/// the right, aft, in the lavatory area (NTSB Aircraft Accident Report
/// AAR-10/02, Learjet 60 N999LJ, section 1.15.3 "Postaccident Examination of
/// Airplane Exits", p.17, with its footnotes 35 and 36 on 14 CFR 25.807(a)).
///
/// The door and the hatch stand on opposite sides, so the arrangement is one
/// pair rated by its smaller exit, Type III. CS/FAR 25.807(g)(2) asks 10 to 19
/// seats for a Type III or larger exit on each side of the fuselage, which the
/// door and the hatch give: the exits admit 19 seats, above the certified 8
/// passengers (TCDS IM.A.212 III.9; FAA A10CE note 7: 9, optional 10) that the
/// approved floor plans fix. No station is printed, so the generic door
/// placement stands.
pub const LJ60_EXIT_LAYOUT: CertifiedExitLayout = CertifiedExitLayout {
    label: "III",
    pairs: &[CertifiedExitPair {
        exit_type: "III",
        station_m: None,
    }],
    upper_deck_pairs: &[],
    station_body_length_m: None,
    source: "NTSB Aircraft Accident Report AAR-10/02 (Learjet 60 N999LJ) section 1.15.3 p.17: main boarding door a Type I exit (left forward, clamshell), Type III escape hatch (right rear, lavatory); CS/FAR 25.807(a)(1) and (a)(3) types; pair rated by the smaller exit",
};

/// Maximum takeoff mass of the variant, kg (23,500 lb).
const MTOW_KG: f64 = 10_659.0;

/// Fuselage length nose tip to tail-cone tip, m: 56 ft 2 in (PTM fig 1-2).
const LENGTH_M: f64 = 17.12;

/// Metres aft of the nose tip of a fuselage station given in inches (PTM fig 19-2).
fn station_m(fs_in: f64) -> f64 {
    0.0254 * (fs_in - 12.77)
}

/// Pounds to kilograms (exact by definition).
const LB_KG: f64 = 0.453_592_37;
/// Cubic feet to cubic metres (exact by definition).
const CU_FT_M3: f64 = 0.028_316_846_592;

/// The aft pressure bulkhead, F.S. in (PTM fig 19-2), which ends the cabin.
const AFT_PRESSURE_BULKHEAD_FS_IN: f64 = 378.05;

/// The Learjet 60's two baggage compartments, declared at their certified
/// loading arms.
///
/// EASA TCDS IM.A.212 Issue 04 III.10 (p 13) prints the compartments and their
/// arms: cabin 260 lb at F.S. 367.0 in, tail 300 lb at F.S. 515.0 in. Neither
/// end of either compartment is printed, so each is placed with its centre on
/// its arm, which is where the weight-and-balance manual puts the bag load, from
/// the one end the sources fix:
///
/// - Cabin: the internal pressurised compartment at the aft end of the cabin
///   (FlightSafety Learjet 60 Pilot Training Manual ch 1, p 1-6), behind the
///   full-across aft lavatory with its wardrobe (Jane's All the World's Aircraft
///   2008-09, Learjet 60, p 790). It ends at the aft pressure bulkhead, so it
///   starts at F.S. 355.95 in. Its volume is the one the built cabin section
///   encloses over that extent: no source prints it. It stands behind the last
///   seat row, so it bounds the compartment the cabin derives there instead of
///   taking seat floor; the floor between the last seat and F.S. 355.95 in is
///   the lavatory's, which the generic monument bays cannot place in a cabin
///   this narrow.
/// - Tail: the unpressurised tailcone compartment aft of the fuselage fuel
///   section, opened by the forward of the two left-hand tailcone doors (PTM ch
///   1, p 1-12, fig 1-18). It starts at the F.S. 470.28 in bulkhead that closes
///   the fuel section (fig 19-2), so it ends at F.S. 559.72 in. Volume: 24 cu
///   ft (0.680 m3), the external compartment both published operator splits
///   print (24 : 24 cu ft, GlobalAir Learjet 60 specifications; 31 : 24 cu ft,
///   Magellan Jets Learjet 60 page). Jane's prints only the 1.67 m3 (59 cu ft)
///   total of the two (p 790), so the tail volume is an input and is not
///   scaled to it.
pub fn lj60_baggage_holds() -> Vec<crate::HoldCompartmentConfig> {
    // A compartment from `fixed_fs_in` to the station that centres it on
    // `arm_fs_in`, forward to aft.
    let about_arm = |fixed_fs_in: f64, arm_fs_in: f64| {
        let other = 2.0 * arm_fs_in - fixed_fs_in;
        (
            station_m(fixed_fs_in.min(other)),
            station_m(fixed_fs_in.max(other)),
        )
    };
    let (cabin_start, cabin_end) = about_arm(AFT_PRESSURE_BULKHEAD_FS_IN, 367.0);
    let (tail_start, tail_end) = about_arm(470.28, 515.0);
    vec![
        crate::HoldCompartmentConfig {
            name: "Cabin baggage".to_owned(),
            x_start_m: cabin_start,
            x_end_m: cabin_end,
            volume_m3: None,
            max_net_kg: Some(260.0 * LB_KG),
            deck: crate::HoldDeck::Main,
            container_rows: None,
            behind_seats: true,
            clear_width_m: None,
            narrow_forward_part_m: None,
        },
        crate::HoldCompartmentConfig {
            name: "Tailcone baggage".to_owned(),
            x_start_m: tail_start,
            x_end_m: tail_end,
            volume_m3: Some(24.0 * CU_FT_M3),
            max_net_kg: Some(300.0 * LB_KG),
            deck: crate::HoldDeck::Lower,
            container_rows: None,
            behind_seats: false,
            clear_width_m: None,
            narrow_forward_part_m: None,
        },
    ]
}

/// The Learjet 60 at 10,659 kg with PW305A engines.
pub fn lj60() -> AircraftPreset {
    AircraftPreset {
        name: "LJ60",
        display_name: "Learjet 60",
        class: crate::presets::AircraftClass::BusinessJet,
        description: "Bombardier Learjet Model 60 at 10,659 kg MTOW (ECR 3845) with PW305A engines on the rear fuselage and a T-tail.",
        identity: AircraftVariantIdentity {
            model: "Learjet Model 60",
            weight_variant: "23,500 lb (10,659 kg) MTOW, ECR 3845; 17,000 lb MZFW, SB 60-8-1",
            engine_model: "PW305A",
            modification_state: "EASA TCDS IM.A.212 Issue 04 optional takeoff weight",
            tank_configuration: "two wing tanks and one fuselage tank",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Table 1-1: span under 15 m is code A.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::A),
            // Jane's 2008-09 p 791: ramp 23,750 lb, T-O 23,500 lb, landing 19,500 lb, zero-fuel 17,000 lb.
            mrw_kg: Some(10_773.0),
            mtow_kg: Some(10_659.0),
            mlw_kg: Some(8_845.0),
            mzfw_kg: Some(7_711.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("LJ60"),
            // EASA TCDS IM.A.212 III.7: usable wing 2,898 lb + fuselage 5,012 lb = 7,910 lb.
            usable_fuel_mass_kg: Some(3_588.0),
            reference_wing_area_m2: Some(24.57),
            certified_exit_layout: Some(LJ60_EXIT_LAYOUT),
            planning_seats: Some(8),
            // TCDS III.9: 10 occupants, 2 crew + 8 passengers.
            certified_max_seats: Some(8),
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: None,
                cruise: None,
                range_nmi: 2_493.0,
                payload_kg: Some(400.0),
                source: "Jane's All the World's Aircraft 2008-09 p 791: range with two crew and four passengers, NBAA IFR, 2,493 n miles (Learjet 60), NBAA reserve terms not printed; payload is four passengers at the 100 kg requirement mass (assumed)",
                // The range is quoted under NBAA IFR reserves, so the design mission flies
                // that rule and not the configured (EASA basic) policy with its 5 % of trip
                // fuel as contingency, a term the NBAA profile does not have. With the
                // published masses the point is flown on full tanks (10,773 kg ramp less the
                // 6,885 kg operating weight and 400 kg leaves 3,488 kg against 3,588 kg
                // usable), so every kilogram of reserve the rule does not ask for is range
                // the model would otherwise be denied.
                reserve: Some(crate::presets::DesignPointReserve {
                    scheme: None,
                    contingency_trip_fraction: Some(0.0),
                    contingency_minimum_hold_min: Some(5.0),
                    alternate_distance_nmi: Some(200.0),
                    final_reserve_hold_min: Some(30.0),
                    flag_flight_time_fraction: None,
                    holding_altitude_ft: Some(5_000.0),
                    source: "NBAA IFR range format for jets (NBAA Management Guide, Appendix D, as described in AOPA Pilot, 'On Instruments: Prepping for plan B', May 2018, and 'Fuel reserves in a jet', March 2024): missed approach at the destination, climb to 5,000 ft, 5 min hold, 200 n mile alternate at long-range cruise, landing with 30 min of holding fuel at 5,000 ft; no contingency as a share of trip fuel. The guide itself was not read",
                }),
            }),
            // Design mission, one provenance row per datum. Range, payload and cruise are the
            // manufacturer's data as quoted by Jane's: no Learjet primary document was located
            // (the only lead found, a learjet.com factsheet, is for the 60XR). The
            // reserve rule is the NBAA IFR standard the figure is quoted at.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "range with two crew and four passengers, NBAA IFR reserves: 2,493 nmi",
                    "manufacturer data as quoted by Jane's; no Learjet primary document located: Jane's All the World's Aircraft 2008-09, Learjet 60 entry, Performance, p.791",
                )),
                payload: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "four passengers; the passenger mass is not printed (400 kg at the 100 kg requirement mass)",
                    "manufacturer data as quoted by Jane's; no Learjet primary document located: Jane's All the World's Aircraft 2008-09, Learjet 60 entry, Performance, p.791",
                )),
                profile: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "no cruise schedule printed with the range; the model's configured cruise Mach and speed schedules apply",
                    "manufacturer data as quoted by Jane's; no Learjet primary document located: Jane's All the World's Aircraft 2008-09, Learjet 60 entry, Performance, p.791",
                )),
                reserve_fuel: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::StandardCitedViaReference,
                    "NBAA IFR reserves, the rule the range is quoted at, with a 200 nmi alternate; terms the attribution does not print follow the preset's configured fuel policy, and the reserve mass follows from the modelled trip",
                    "NBAA Management Guide, IFR range reserve definition; attributed to the Learjet 60 range in Jane's All the World's Aircraft 2008-09, Learjet 60 entry, Performance, p.791",
                )),
            },
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            usable_cg_range: Some(crate::presets::usable_cg_range::LJ60),
            sources: vec![
                "EASA TCDS IM.A.212 Learjet M60 Issue 04 (12 Nov 2024): III.5-III.10 CG, weights, fuel, seating, baggage",
                "Bombardier Learjet 60 Airplane Flight Manual Rev 37 (30 Nov 2009): maximum zero fuel weight 17,000 lb with SB 60-8-1",
                "FlightSafety International, Learjet 60 Pilot Training Manual: fig 1-2 General Dimensions; fig 19-2 Dimensional Data",
                "Jane's All the World's Aircraft 2008-09 pp 825-826: Learjet 60 weights, areas, range",
            ],
            operating_speed_limits: Some(super::operating_limits::LJ60),
            // S: TCDS III.5 ground-handling aft limit, nose share between the PTM fig 19-2 wheels.
            aft_cg_nose_load: Some(crate::presets::gear_load::LJ60),
            horizontal_tail_ice_protection: Some(crate::presets::tail_ice::LJ60),
            ..AircraftReferenceData::default()
        },
        engine_name: "PW305A",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            // TCDS III.15: single dual-chine nose wheel 18 x 4.4, dual main wheels 17.5 x 5.75; neither size is in the certified tyre list, so the sizing selects.
            n_nlg_wheels: 1,
            n_mlg_struts: 2,
            wheels_per_mlg_strut: 2,
            // PTM fig 1-2: track 8 ft 3 in (2.51 m); Jane's prints the same.
            track_diameter_factor: 2.51 / 1.96,
            reference_track_m: Some(2.51),
            // PTM fig 19-2, static: nose wheel F.S. 92.42 in, main wheels F.S. 396.82 in; 7.73 m, as Jane's prints.
            reference_wheelbase_m: Some(0.0254 * (396.82 - 92.42)),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(LENGTH_M),
            reference_nlg_x_fraction: Some(station_m(92.42) / LENGTH_M),
            reference_mlg_x_fractions: Some(vec![
                station_m(396.82) / LENGTH_M,
                station_m(396.82) / LENGTH_M,
            ]),
            // PTM fig 1-2 side view: keel 0.60 m above the printed ground line (scale from 17.89 m).
            fuselage_ground_clearance_m: Some(0.60),
            // PTM fig 1-2 side view ("all dimensions shown for aircraft in static position", scale
            // from the 17.12 m length): the line from the main-wheel contact point to the ventral
            // delta-fin trailing tip stands 11.5 deg above the ground line (fin tip 7.29 m aft of
            // the wheels, 1.49 m up); the keel alone gives 16 deg. The built fins give 11.6 deg.
            // TCDS IM.A.212 III.5 agrees: at the 25.0 %MAC aft flight limit (F.S. 385.11) the CG is
            // 0.297 m ahead of the wheels, a tip-back angle of 12 deg at the model's 1.38 m CG
            // height, so the certified aircraft clears its own tail-down angle and not the 15 deg
            // Raymer/Roskam floor this field held as an estimate.
            min_tip_back_deg: 11.5,
            // The same drawn static tail-down angle is the tail-scrape requirement.
            required_rotation_angle_deg: 11.5,
            required_rotation_basis: crate::landing_gear::RotationAngleBasis::PublishedStatic,
            // PTM ch 1 Empennage: the aft stabilizer attachment is the pivot for pitch trim.
            trimmable_stabilizer: true,
            // FAA TCDS A10CE Rev 67 (Learjet 60, FAA DRS), p. 25 of 38, "Control Surface Movements": horizontal stabilizer
            // lower limit (nose-up end) L.E. down 11 deg 9 min to 11 deg 25 min, upper limit L.E. down 1 deg 6 min to
            // 1 deg 21 min (the sheet does not state the zero reference); elevator up 15 deg (+-30 min), down 16.5 deg
            // (+-30 min). The nose-up stop takes the middle of the printed rigging range, 11.28 deg. Were the class values
            // 12 deg and 25 deg.
            stabilizer_nose_up_travel_deg: Some(11.28),
            elevator_up_travel_deg: Some(15.0),
            ..LandingGearConfig::default()
        },
        // PTM fig 1-2 plan view, scale from the 17.89 m length (122 px/m at 400 dpi). The straight LE (17.0 deg, y 2.1-5.9 m) and the outboard TE
        // meet at the flap end y 3.0 m. Jane's prints 2.74 m and 1.12 m root and tip chords; the drawn tip chord agrees and the drawn TE and LE lines at y = 0 give the datum 3.05 m (Jane's root chord is measured at another station).
        design_vector: DesignVector {
            span_m: 13.35, // PTM fig 1-2: 43 ft 10 in over the winglets, static; Jane's 13.34 m
            root_chord_m: 3.05,
            break_chord_m: 2.03,
            tip_chord_m: 1.128,
            sweep_deg: 17.03,
            // ASSUMED: no published washout found.
            tip_twist_deg: -2.0,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: LENGTH_M,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        speed_schedule_source: crate::presets::SpeedScheduleSource::DefaultRule,
        airfoil_class_source: "Conventional (NACA 6A-series): the wing section is the NACA 64A109 mod (Lednicer, The Incomplete Guide to Airfoil Usage, 2026). The modification is the Gates Learjet leading edge of US Patent 4,240,597 (Ellis, Gertsen and Conley, filed 1978, granted 1980): a larger, lower-surface leading-edge radius over the forward 15 % of the 64A-109 chord, about 1.24 % of the root chord, for low stall speeds; the patent claims no drag-divergence gain. No source calls the main-wing section supercritical (the Longhorn winglet section is the NASA LS(1)0413, thinned to 8 %), so the 64A class stands.",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Jane's 2008-09 p 791: wings, gross 24.57 m2. PTM fig 19-2: MAC 80.09 in, LEMAC F.S. 365.085 in (TCDS III.5 gives the same MAC by derivation).
                reference_wing: crate::ReferenceWingConfig::published(24.57, Some(0.0254 * 80.09), Some(station_m(365.085))),
                root_datum_x_m: 8.119, // PTM fig 1-2 plan: the straight LE line at y = 0
                root_z_m: -0.575,      // PTM fig 1-2 front view: mid of the wing line; PTM ch 19 prints 2.5 deg dihedral
                break_z_m: -0.443,
                tip_z_m: -0.341,
                // ASSUMED, no published incidence found; total washout as design_vector.
                root_twist_deg: 2.0,
                break_twist_deg: 0.0,
                break_span_fraction: 0.4781, // flap end y 3.0 m of the 6.275 m panel
                kink_span_fraction: Some(0.4781),
                side_of_body_span_fraction: Some(0.1562), // fuselage half width 0.98 m at the drawn LE
                side_of_body_chord_ratio: Some(0.89),     // TE line at the side of body, 11.13 m
                // PTM fig 1-2 plan: inboard glove from y 1.75 m to the side of body (LE 0.32 m ahead of the straight line) and the inboard flap TE 0.08 m aft of the outboard line.
                edge_stations: vec![
                    crate::WingEdgeStation { span_fraction: 0.1562, le_offset_m: -0.32, te_offset_m: 0.0 },
                    crate::WingEdgeStation { span_fraction: 0.2789, le_offset_m: 0.0, te_offset_m: 0.03408 }, // the glove ends on the straight LE at y 1.75 m (PTM fig 1-2); a reading of -0.015 m tilted the edge line off the datum root
                    crate::WingEdgeStation { span_fraction: 0.4622, le_offset_m: 0.0, te_offset_m: 0.085 },
                    // PTM fig 1-2 plan: the TE holds the straight line to y 6.11 m, then flares into the winglet root (0.11 m aft at y 6.19 m).
                    crate::WingEdgeStation { span_fraction: 0.9732, le_offset_m: 0.0, te_offset_m: 0.0 },
                    crate::WingEdgeStation { span_fraction: 0.9871, le_offset_m: 0.0, te_offset_m: 0.112 },
                ],
                // PTM fig 1-2: the plan LE turns aft from y 6.0 m, LE 0.30 m behind the straight line where the side view shows the winglet start to rise.
                tip_rake_start_span_fraction: Some(0.956),
                tip_rake_tip_chord_m: Some(0.95), // plan tip LE read 0.13 m forward of the earlier 0.828 m fit
                // PTM fig 1-2 front view: winglet rises 1.15 m (side view reads 1.0 m) to the printed span at 15 deg cant; side and plan: tip LE 0.9 m aft of its root, tip chord 0.40 m (side 0.28 m, plan 0.44 m).
                wingtip_device: Some(crate::WingtipDevice {
                    kind: crate::WingtipDeviceKind::Sharklet,
                    height_m: 1.15,
                    down_height_m: 0.0,
                    cant_deg: 15.0,
                    root_chord_m: None,
                    // PTM fig 1-2 plan: root TE 0.21 m aft of the straight line, back on the winglet TE line at y 6.46 m.
                    root_te_flare: Some(crate::WingtipTeFlare { offset_m: 0.21, fade_length_m: 0.42 }),
                    tip_chord_m: 0.40,
                    le_sweep_deg: 42.0,
                    toe_out_deg: 0.0,
                    blend_radius_m: 0.12,
                    arc_start_deg: None,
                    path_stations: Vec::new(),
                    airfoil: None,
                    span_includes_device: true,
                    include_in_vlm: true,
                }),
                // Lednicer, The Incomplete Guide to Airfoil Usage (updated 4 April 2026): Learjet 60 wing root and
                // tip NACA 64A109 mod. Built as the 64A thickness form at 9 % on the a = 0.8 mean line at design cl
                // 0.1 (Abbott and von Doenhoff, Theory of Wing Sections, 1959, section 6.6). The "mod" is the
                // low-speed leading edge of US Patent 4,240,597 (radius about 1.24 % of the root chord, lower surface,
                // forward 15 % of chord); no coordinates are published, so the plain 64A109 is built, which carries
                // the cruise (wave-drag) class unchanged.
                root_airfoil: "naca64a109".to_owned(),
                tip_airfoil: "naca64a109".to_owned(),
                airfoil_class: crate::AirfoilClass::Conventional,
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // PTM fig 1-2 plan: HTP LE and TE lines, y 1.1-2.1 m (LE sweep 30 deg; PTM ch 1 prints 25 deg, chord line not stated). T-tail: root LE on the fin tip LE.
                hstab_offset_from_tail_m: LENGTH_M - 15.76,
                // PTM fig 1-2 side view, on the printed 14 ft 7 in height: tailplane lens centre 0.26 m under the bullet top. The front view draws it 0.13 m
                // higher, the same offset as its crown (0.16 m) against the side view; the side view carries the printed height, so it governs.
                hstab_z_m: 2.61,
                // PTM fig 1-2 plan view read on its vector paths (PDF p.11), scaled at 21.93 pt/m on the printed 58 ft 8 in
                // and 56 ft 2 in lengths of the side view (the wing span then measures 13.46 m against the printed 13.35 m):
                // LE (440.38, 244.30)-(465.26, 200.53) pt and TE (476.09, 244.30)-(485.99, 195.92) pt about the centre line
                // y 244.3 pt give 1.628 m at the centre line and 0.814 m at the printed 2.24 m half-span, the tip LE 1.273 m
                // aft of the root LE (the 1.29 m below). The earlier 1.68 and 0.85 m were read on the raster.
                hstab_root_chord_m: 1.63,
                hstab_tip_chord_m: 0.81,
                hstab_root_twist_deg: -1.0, // ASSUMED
                hstab_tip_twist_deg: -1.0,
                hstab_tip_le_m: (1.29, 2.24, 0.03), // printed span 14 ft 8 in (4.48 m)
                // PTM fig 1-2 side view: fin LE and TE lines from the aft crown line 2.42 m above ground, carried to the bullet top at the printed 14 ft 7 in height (4.44 m).
                vstab_offset_from_tail_m: LENGTH_M - 14.10,
                vstab_z_m: 0.855,
                vstab_root_chord_m: 2.93, // side view: drawn fin TE 0.13 m aft of the earlier 2.80 m line at the root
                vstab_tip_chord_m: 1.573,
                // Same view, outer edge of the drawn fin outline: LE x 14.71 m at z 1.5 m and 15.53 m at z 2.4 m, carried to the
                // top; the TE line on the same rise gives the 2.93 m root and 1.573 m tip chords.
                vstab_tip_le_m: (1.86, 0.0, 2.025),
                // Same view: the bullet on the fin top, nose at z 2.74 m, top z 2.88 m at x 16.65 m, aft end x 17.67 m at z 2.66 m, faired into the fin TE from z 2.50 m. Offsets from the fin lines.
                vstab_edge_stations: [(0.812, 0.0, 0.0), (0.849, 0.0, 0.224), (0.89, 0.0, 0.25), (0.93, 0.0, 0.14), (0.96, 0.15, 0.03), (0.98, 0.29, -0.09), (0.99, 0.43, -0.25), (1.0, 0.71, -0.61)]
                    .map(|(span_fraction, le_offset_m, te_offset_m)| crate::TailEdgeStation { span_fraction, le_offset_m, te_offset_m })
                    .to_vec(),
                // PTM fig 1-2 front view: fin 0.25 m thick at z 1.3 m on a 2.63 m chord, 9.7 %, thinner above; was the default 12 %.
                vstab_airfoil: Some("naca0009".to_owned()),
                // Same view: half-width 0.135 m at z 1.2, 0.098 at 1.7, 0.062 at 2.2, 0.041 at 2.5, 0.028 at 2.7 m over the side-view chords, t/c 10.0, 8.3, 6.1, 4.5, 3.2 %.
                vstab_thickness_stations: [(0.17, 1.11), (0.417, 0.92), (0.664, 0.673), (0.812, 0.498), (0.911, 0.35)]
                    .map(|(span_fraction, thickness_scale)| crate::WingThicknessStation { span_fraction, thickness_scale })
                    .to_vec(),
                // The fin continues above the tailplane, 0.27 m below the drawn fin top: the reference fin ends at that top, the end of these lines.
                vstab_reference_end_fraction: Some(1.0),
                // PTM fig 1-2 side view: the dorsal line comes out from behind the nacelle at x 13.3 m, 0.21 m above the crown, and joins the fin LE about z 1.5 m.
                vstab_dorsal_length_m: Some(0.80),
                vstab_dorsal_seat_rise_m: Some(0.21),
                vstab_dorsal_height_m: Some(0.445),
                // PTM fig 1-2: ventral delta fins. Side outline root (13.95, 0.25) to (16.95, 0.68), lower edge to
                // (16.3, -0.19), then (17.05, -0.08); plan outer apex (15.75, 1.04), trailing edge through (16.25, 0.92).
                // Inboard of y 0.47 the root is held inside the body: at y 0.10 all of it, the tail cone
                // being 0.28 m wide at the drawn root trailing edge. Only the part outside is skin.
                ventral_fin_stations: vec![
                    crate::VentralFinStation::drawn((14.10, 0.855), 0.10, (13.95, 0.25), (16.95, 0.68)),
                    crate::VentralFinStation::drawn((14.10, 0.855), 0.47, (13.95, 0.25), (17.00, 0.31)),
                    crate::VentralFinStation::drawn((14.10, 0.855), 0.75, (14.86, 0.07), (17.05, -0.08)),
                    crate::VentralFinStation::drawn((14.10, 0.855), 0.92, (15.41, -0.03), (16.30, -0.19)),
                    crate::VentralFinStation::drawn((14.10, 0.855), 1.04, (15.75, -0.10), (15.78, -0.105)),
                ],
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 1.96, // Jane's All the World's Aircraft 2008-09 p 825 fuselage max diameter; PTM fig 1-2 plan reads the same at 122 px/m
                height_m: Some(1.91), // PTM fig 1-2 side view, crown 2.53 m and keel 0.60 m above ground, less the line width
                nose_z_m: -0.63,  // PTM fig 1-2 side view: nose tip 0.93 m above ground
                // Jane's All the World's Aircraft 2008-09 cabin excl flight deck 5.38 m, ending at the F.S. 378.05 in aft bulkhead.
                cabin_start_x_m: 3.90,
                cabin_z_m: 0.0,
                // F.S. 378.05 in aft bulkhead to the tail-cone tip.
                tailcone_length_m: 7.84,
                tail_z_m: 0.615,
                // PTM fig 1-2 outlines (first fit, nose and aft law).
                nose_length_m: Some(3.80),
                nose_windshield_angle_deg: Some(30.0),
                nose_crown_end_fraction: Some(0.95),
                nose_radome_length_fraction: Some(0.12),
                nose_keel_exponent: Some(2.5),
                nose_plan_exponent: Some(1.8),
                nose_section_exponent: Some(2.0),
                belly_upsweep_length_m: Some(7.2),
                aft_belly_exponent: Some(1.5),
                tail_end_z_m: Some(0.615),
                tail_end_width_m: Some(0.30),
                tail_end_height_m: Some(0.36),
                aft_plan_length_m: Some(9.1),
                aft_plan_exponent: Some(1.0),
                aft_crown_length_m: Some(11.0),
                aft_crown_exponent: Some(1.2),
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                engine_name: "PW305A".to_owned(),
                // PTM fig 1-2 plan and front views: nacelle centres +-1.32 m.
                spanwise_positions_m: vec![1.32, -1.32],
                // On the rear fuselage: inlet 10.60 m aft of the nose (plan view, forward face read 0.07 m aft at 10.67), axis 2.29 m above ground (side view, top 2.83 m less the 0.54 m radius).
                aft_fuselage: Some(AftFuselageEngineInstallation {
                    inlet_ahead_of_tail_m: LENGTH_M - 10.60,
                    axis_z_m: 0.725,
                }),
                // The wing-relative fields say the same place at the nominal wing, for readers that do not know the mount.
                z_m: 1.219,
                inlet_x_offset_m: -2.15,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            // Jane's 2008-09 p 791 prints the cruise speeds without an altitude: long-range
            // 422 kt, normal 453 kt, high 466 kt. 422 kt is M 0.736 at the 12 500 m ISA cruise
            // altitude below (453 kt is M 0.790 there).
            cruise_mach: 0.736,
            cruise_altitude_m: 12_500.0,
            mtow_kg: MTOW_KG,
            // CS 25.337(b) below 50,000 lb: n = 2.1 + 24,000 / (W + 10,000), 2.82 at
            // the 23,500 lb MTOW, with the 1.5 factor of safety of CS 25.303. It
            // follows the design MTOW, so a lighter design keeps its own minimum.
            ultimate_load_factor: crate::requirements::cs25_ultimate_load_factor(MTOW_KG),
            ultimate_load_factor_from_mtow: true,
            aircraft_type: "passenger".to_owned(),
            max_wing_area_m2: 25.5,
            min_wing_loading_kg_m2: 300.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 8,
            passenger_mass_kg: 100.0,
            // TCDS III.10: cabin 260 lb + tail 300 lb baggage.
            cargo_payload_kg: 254.0,
            // Jane's p 791: max payload 1,011 kg (MZFW 7,711 kg less BOW 6,700 kg).
            max_structural_payload_kg: 1_011.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        // PTM ch 1: single-slotted flaps, no leading-edge device.
        performance: super::high_lift("conservative_simple_flaps"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lj60_states_one_weight_variant_and_rear_fuselage_engines() {
        let lj60 = lj60();
        assert_eq!(lj60.reference.mtow_kg, Some(lj60.requirements.mtow_kg));
        assert!(lj60.geometry.engine.aft_fuselage.is_some());
        assert_eq!(lj60.engine_spanwise_positions(), &[1.32, -1.32]);
        assert!((lj60.requirements.ultimate_load_factor - 4.224).abs() < 1e-3);
        let mut lighter = lj60.requirements.clone();
        lighter.mtow_kg = 9_500.0;
        assert!(lighter.design_ultimate_load_factor() > lj60.requirements.ultimate_load_factor);
    }

    #[test]
    fn lj60_declares_its_certified_operating_speed_limits_by_altitude() {
        const KT: f64 = 1_852.0 / 3_600.0;
        const FT: f64 = 0.3048;
        let profile = lj60().operational_mission_defaults().profile;
        let vmo_kt = |ft: f64| profile.vmo_cas_m_s_at_altitude(ft * FT) / KT;
        assert!((vmo_kt(5_000.0) - 300.0).abs() < 1.0e-9);
        assert!((vmo_kt(8_000.0) - 300.0).abs() < 1.0e-9);
        assert!((vmo_kt(15_000.0) - 340.0).abs() < 1.0e-9);
        assert!((vmo_kt(30_000.0) - 330.0).abs() < 1.0e-9);
        assert!((profile.mmo_at_altitude(30_000.0 * FT) - 0.81).abs() < 1.0e-12);
        assert!((profile.mmo_at_altitude(40_000.0 * FT) - 0.795).abs() < 1.0e-12);
        assert!((profile.mmo_at_altitude(45_000.0 * FT) - 0.78).abs() < 1.0e-12);
    }
}
