// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Regional transport presets.
//!
//! The ATR entry records `PW127M` as an identity and binds it to the typed
//! free-turbine/gearbox/568F-1 payload. Legacy flat turbofan fields remain in
//! [`crate::EngineConfig`] only as a migration mirror and are not active for
//! this preset.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MissingDesignMissionDatum, MissionEvidenceApplicability,
    PartialDesignMissionEvidence, PartialMissionEvidenceKind, PublishedRange, WingConfig,
};

/// ATR 72-212A marketed as the ATR 72-600, in the 23,000 kg configuration.
// Drawn-view stations are measured data; a fraction near 1/pi is not that constant.
#[allow(clippy::approx_constant)]
pub fn atr72_600() -> AircraftPreset {
    let mut engine = EngineConfig {
        engine_name: "PW127M".to_owned(),
        ..EngineConfig::default()
    };
    // Materialize the technology binding before overriding installation
    // geometry. The embedded catalogue is the authoritative source for the
    // PW127M/568F power ratings; the preset owns where that system is mounted.
    engine.apply_engine_spec();
    // Nacelle length and radius come from the PW127M engine catalogue entry: apply_engine_spec overwrites anything set here when a preset loads.
    // The 8.10 m printed on both ATR three-views runs between the propeller axes (the extension lines drop to the hubs).
    engine.spanwise_positions_m = vec![4.05, -4.05];
    // ATR 72 three view drawing (EWADE 2011 presentation, p.3), side view: nacelle centre z 1.135 m (top 1.78 m, bottom 0.49 m), below the wing plane at 1.47 m.
    // ATR Aircraft Recovery Manual 1-10-06 Fig. 1-28 agrees: propeller hub 3.09 m over the 1.965 m ground plane is z 1.125 m.
    // Measured from the local wing line, which is 1.487 m at the nacelle: axis at z 1.10 m (front view circle fit 1.09 m).
    engine.z_m = -0.387;
    // Same drawing: spinner tip 8.50 m aft of the nose (plan 8.54 m, side 8.43 m), ahead of the 11.05 m wing leading edge.
    // JCAB TCDS No. 75 Rev 3, ATR 72-212A item (17)(a) engine oil H-arm 12.190 m is 9.828 m aft of the nose, a coarse check.
    engine.inlet_x_offset_m = 2.55;

    AircraftPreset {
        name: "ATR72-600",
        display_name: "ATR 72-600",
        class: crate::presets::AircraftClass::Transport,
        description: "ATR 72-212A (ATR 72-600), 23 t variant, with PW127M engines and 568F-1 propellers.",
        identity: AircraftVariantIdentity {
            model: "ATR 72-212A",
            weight_variant: "23,000 kg MTOW",
            engine_model: "PW127M",
            // EASA TCDS A.084 Note VII.1: Mod 5948 is the New Avionic Suite
            // ("Glass Cockpit") change that defines the "ATR 72-600"
            // commercial designation for the ATR 72-212A; it is not a
            // weight variant. The 23,000 kg MTOW / 22,350 kg MLW /
            // 21,000 kg MZFW variant this preset declares is Mod 6219 (TCDS
            // A.084 section III.13.b, "ATR 72-212A models" table), a
            // separate, independently applicable modification; Mod 5948 alone is not
            // the weight-variant citation.
            modification_state: "ATR 72-600 commercial standard (Mod 5948, NAS/glass cockpit); 23,000 kg MTOW weight variant (Mod 6219)",
            tank_configuration: "standard integral wing tanks",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Vol. I Table 1-1 (aerodrome reference code) applied to the
            // preset wingspan of 27.05 m (ATR 72-600): 24 m <= b < 36 m is code C.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::C),
            // EASA TCDS A.084, Issue 14 (23 Feb 2026), section III.13.b
            // "ATR 72-212A models", Mod 6219 column: MRW 23,170 kg, MTOW
            // 23,000 kg, MLW 22,350 kg, MZFW 21,000 kg. The MTOW/MLW/MZFW
            // already matched this preset exactly; only MRW needed
            // taken from the TCDS in place of an uncited 23,150 kg.
            mrw_kg: Some(23_170.0),
            mtow_kg: Some(23_000.0),
            mlw_kg: Some(22_350.0),
            mzfw_kg: Some(21_000.0),
            // Factsheet typical in-service value; see `crate::oew_reference`.
            oew_kg: crate::oew_reference::preset_reference_oew_kg("ATR72-600"),
            usable_fuel_mass_kg: Some(5_000.0),
            reference_wing_area_m2: Some(61.0),
            planning_seats: Some(72),
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: Some("ATR 72-600 brochure 2026 p.2: 'Range with max pax at long-range cruise speed' (footnote (1)); no speed printed"),
                cruise: None,
                range_nmi: 758.0,
                payload_kg: Some(72.0 * 95.0),
                source: "ATR 72-600 Factsheet (PW127M/N edition, 2020-07) p.2: range with maximum passengers (72 seats) 758 nmi; the sheet prints no basis, which ATR states for the same quantity as 95 kg per passenger, EASA fuel reserves, 100 NM alternate, 10 min taxi (ATR 72-600 brochure 2026 p.2 footnote (1))",
                reserve: Some(crate::presets::DesignPointReserve {
                    scheme: None,
                    contingency_trip_fraction: None,
                    contingency_minimum_hold_min: None,
                    alternate_distance_nmi: Some(100.0),
                    final_reserve_hold_min: None,
                    flag_flight_time_fraction: None,
                    holding_altitude_ft: None,
                    source: "ATR 72-600 brochure 2026 p.2 footnote (1): EASA fuel reserves, 100 NM alternate; every other term is the configured (EASA basic) fuel policy's",
                }),
            }),
            certified_max_seats: Some(78),
            certified_exit_layout: Some(crate::presets::ATR72_600_EXIT_LAYOUT),
            planning_cabin: Some(crate::presets::ATR72_600_PLANNING_CABIN),
            partial_design_mission_evidence: vec![PartialDesignMissionEvidence {
                kind: PartialMissionEvidenceKind::AdvertisedRange,
                range: Some(PublishedRange::NauticalMiles(740.0)),
                payload_kg: None,
                load_case: None,
                profile_assumptions: None,
                reserve_assumptions: None,
                reserve_contract: None,
                applicability: "ATR advertises the 740 nm capability for the ATR 72-600 product; it does not define a payload, complete flight profile, or reserve-fuel mass for this preset",
                configuration_applicability: MissionEvidenceApplicability::ModelAndEngineFamily,
                missing: vec![
                    MissingDesignMissionDatum::Payload,
                    MissingDesignMissionDatum::Profile,
                    MissingDesignMissionDatum::ReserveFuel,
                ],
                source: "ATR ATR 72-600 Facts and Figures, product specification (accessed 2026-08-30)",
            }],
            // Design mission, one provenance row per datum: the factsheet's range with maximum
            // passengers; ATR prints the passenger mass and reserve basis of that figure in its
            // 2026 brochure, which is the regulatory EASA policy with a 100 NM alternate.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "range with maximum passengers (72 seats at 29 in), PW127M/N, 758 nmi; the factsheet prints no basis, which ATR states for the same quantity as 95 kg per passenger, EASA fuel reserves, 100 NM alternate, 10 min taxi, long-range cruise speed",
                    "ATR 72-600 factsheet (PW127M/N edition, 19T0916 V7, 2020) p.2; basis: ATR 72-600 brochure 2026 p.2 footnote (1)",
                )),
                payload: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "72 passengers at 95 kg: 6,840 kg",
                    "ATR 72-600 factsheet (2020) p.2 (72 seats); ATR 72-600 brochure 2026 p.2 footnote (1) (95 kg per passenger)",
                )),
                profile: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "long-range cruise speed, ISA; no climb or descent schedule is printed, so the model's configured cruise and speed schedules apply",
                    "ATR 72-600 brochure 2026 p.2 ('Range with max pax at long-range cruise speed' and footnote (1))",
                )),
                reserve_fuel: Some(crate::MissionDatumProvenance::regulatory_reserve(
                    "the 2020 factsheet prints no reserve basis; ATR states 'EASA fuel reserves - 100 NM alternate' for the same quantity (ATR 72-600 brochure 2026 p.2 footnote (1)), so the design mission applies the regulatory policy with a 100 nmi alternate. The range comparison carries this reserve-basis uncertainty",
                )),
            },
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            sources: vec![
                "ATR ATR 72-600 Airport Planning Manual, Issue 8, 2021, aircraft characteristics and limitations",
                "EASA Type Certificate Data Sheet EASA.A.084, ATR 42/72, Issue 14, 23 February 2026, section III.13.b \"ATR 72-212A models\" (Mod 6219 column: MRW 23,170 kg, MTOW 23,000 kg, MLW 22,350 kg, MZFW 21,000 kg) and Note VII.1 (Mod 5948 = New Avionic Suite / \"Glass Cockpit\", the commercial \"ATR 72-600\" designation, a separate modification from the weight variant)",
                "ATR 42 / ATR 72 Aircraft Recovery Manual, 1-10-01 p.19 Figure 1-2 (ATR 72-212A main dimensions: 1.728 m nose to nose wheel, 10.772 m wheelbase, 27.166 m length, 4.10 m track), 1-10-04 p.27 fuselage frame stations (drawing 9SMJ 062110 ZON 00110-004, frame 0 at STA 2362 mm), 4-00-02 Figure 4-1 tail-tipping CG limit H-arm 14.848 m = 54 percent MAC",
                "ATR Weight and Balance Manual, LIMITATIONS LIM.1 p.03, 15 JAN 2021, weight variant F2/75 (MAC 2.303 m; station 0 is 2.362 m forward of the fuselage nose; station 0 to reference chord leading edge 13.604 m)",
                "ATR ATR 72-600 Facts and Figures, product specification (accessed 2026-08-30)",
            ],
            horizontal_tail_ice_protection: Some(crate::presets::tail_ice::ATR72_600),
            operating_speed_limits: Some(crate::presets::operating_limits::ATR72_212A),
            ..AircraftReferenceData::default()
        },
        engine_name: "PW127M",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            wheels_per_mlg_strut: 2,
            // Main tire: UK CAA TCDS A.159 sec. 22: H 34 x 10.0 R16 for all ATR 72 models; Goodyear Data Book 2022 Sec. 4F PDF p.11.
            // A ~0.81 m main-tyre diameter (32x8.8R16) has no source on disk and conflicts with this declared
            // H34x10.0R16 (0.885 m). The 1.17 m class tyre is replaced by this certified tyre. Read the ATR
            // main-tyre size from the TCDS or the ATR 72 ARM wheel table before any change here.
            mlg_tire: "H34x10.0R16/14PR".to_owned(),
            nlg_tire: "450x190-5/10PR".to_owned(),
            // Printed 4.10 m main-wheel track over the 2.865 m fuselage width (was over 2.77 m, which built 4.24 m).
            track_diameter_factor: 4.1 / 2.865,
            // Longitudinal gear stations, measured, in the nose-tip frame.
            //
            // Source A, the stations. ATR 42 / ATR 72 *Aircraft Recovery
            // Manual* (ATR, 1 Allee Pierre Nadot, Blagnac; "Printed in
            // France"), 1-10-01 "Aircraft Dimensions", p.19 Figure 1-2
            // "ATR 72-212A Main Dimensions". The side elevation carries one
            // dimension chain along the ground line: 1,728 m (68,03 in) from
            // the fuselage nose to the nose-wheel contact, then 10,772 m
            // (424,09 in) from there to the main-wheel contact, drawn above an
            // overall length of 27,166 m (1069,53 in) taken from the same nose
            // tip. p.18 Figure 1-1 "ATR 42-500 Main Dimensions" prints the
            // identical leading 1,728 m (68,03 in) ahead of an 8,781 m
            // (345,70 in) wheelbase on a 22,67 m (892,52 in) aeroplane, which
            // is what a shared forward fuselage requires and is why the
            // 1,728 m reads as the nose-to-nose-gear leg of the chain.
            //
            // Source B, the frame. ATR *Weight and Balance Manual*,
            // LIMITATIONS LIM.1 "Certified Center of Gravity Envelope" p.03
            // (data module _9d8f0447, 15 JAN 2021, weight variant F2/75,
            // MTOW 22 800 kg): "The MAC is 2.303 m long. Station 0 is 2.362 m
            // forward of the fuselage nose. The distance from station 0 to
            // reference chord leading edge is 13.604 m." The recovery manual's
            // own frame table (1-10-04, drawing 9SMJ 062110 ZON 00110-004)
            // places frame 0 at STA 2362 mm, so STA[mm] = 1000 x x_nose[m] +
            // 2362 exactly and the two manuals share one frame.
            //
            // Independent check, nothing here was fitted to it. The recovery
            // manual's Figure 4-1 tail-tipping CG limit is H-arm 14.848 m
            // (54 % MAC) for the ATR 72 and 12.865 m (63 % MAC) for the ATR 42.
            // Converted with the same 2.362 m datum offset those are 12.486 m
            // and 10.503 m aft of the nose, against the 1.728 + 10.772 =
            // 12.500 m and 1.728 + 8.781 = 10.509 m main-wheel stations the
            // two dimension chains give: 14 mm and 6 mm forward of the main
            // wheels, the small margin a tip-back limit must carry.
            //
            // Stored as fractions of the drawing fuselage length so a shrink
            // or clean-sheet run re-applies them to the active fuselage rather
            // than freezing absolute metres
            // (`LandingGearConfig::resolved_station_positions`). These are
            // ground-contact stations, which is what the nose-gear load
            // balance needs; the ATR main gear is a trailing-arm unit, so its
            // axle and its contact station are not the same point and only the
            // contact station is dimensioned. Declaring the three fields is
            // what avoids the `StationError::MainGearStationNotMeasured`
            // refusal; the wing-mounted fallback (`x_mlg = mac_le + mlg_x_fraction_mac x
            // mac`) remains outside its domain for this sponson gear and is
            // not reached.
            reference_wheelbase_m: Some(10.772),
            reference_track_m: Some(4.10),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(27.166),
            reference_nlg_x_fraction: Some(1.728 / 27.166),
            reference_mlg_x_fractions: Some(vec![
                (1.728 + 10.772) / 27.166,
                (1.728 + 10.772) / 27.166,
            ]),
            // FAA TCDS A53EU (ATR 72-101 to -212A; FAA DRS title Rev 35, page header Rev 34), p. 28 of 34, "Control Surface
            // Movements": elevator ("Pitch") up 23 deg (+1/-0), down 13 deg (+1/-0.6); pitch trim by elevator tab
            // (+5 deg +-0.25, -1.5 deg +-0.1). The stabiliser is fixed (ATSB AO-2014-032, Fig. 4), so no takeoff stabiliser
            // setting is credited. Was 21.8 deg, one airframe's pre-flight full-and-free check (UK AAIB Bulletin AAIB-31376,
            // ATR 72-212A G-OASB, p.3: "left elevator range of +12.7 deg to -21.8 deg"), below the sheet's rigging range.
            elevator_up_travel_deg: Some(23.0),
            // Same TCDS page: pitch trim by elevator tab, +5 deg (+-0.25) and -1.5 deg (+-0.1);
            // the larger travel is read as the nose-up (trailing-edge-down) one, as a fixed
            // stabiliser needs its nose-up trim at forward CG and low speed. The pitch control
            // is cables from the control columns to the rear fuselage (ATSB AO-2014-032, second
            // interim report, pitch-system flexibility) with trim by tab, so the elevator is
            // reversible.
            elevator_trim_tab_nose_up_deg: Some(5.0),
            // The same tab is geared to its elevator: UK AAIB Bulletin 2/2017 (ATR 72-212A
            // G-COBO, EW/C2016/03/02), p. 17, "Pitch trim": the tabs are "linked mechanically
            // to their associated elevator" and move in the opposite direction, 0.5 deg of tab
            // per deg of elevator from the trim (null) position.
            elevator_balance_tab_gearing: Some(-0.5),
            // VMCA 98 kt CAS at sea level, OAT 15 C, 5 deg bank: AFM LIM.3 p. 15 (Takeoff
            // VMCA) as reviewed by AvioConsult, Critical Limited Review ATR Manuals on
            // Engine-out Operations (2022), pp. 31 and 35.
            published_vmca_m_s: Some(98.0 * 0.514_444),
            // Belly clearance from the points of ATR 42 / ATR 72 Aircraft Recovery Manual chapter 1-10-06 Figure 1-28
            // (vertical clearances, ATR 72 table), maximum ramp weight 21,530 kg. The table has no fuselage-bottom point (A..E are nose, door sills and tail points); the ground
            // plane is the one that puts the model on three tabulated heights: G propeller hub 3.09 m, H propeller tip 1.11 m (G - H =
            // 1.98 m = the 1.965 m propeller radius (3.93 m is the diameter), so H is the lowest blade tip), J wing tip 3.72 m.
            // Brought to a level fuselage (the table angle -1.183 deg, nose down, at CG 14 % MAC; -0.550 deg at CG 37 %) the model needs 0.684 / 0.695 / 0.575 m
            // (CG 14 %) and 0.669 / 0.684 / 0.548 m (CG 37 %), mean 0.64 m; spread +-0.07 m. The model nose bottom reads 0.8 m above its
            // belly where the real flat-bottomed ATR nose is level with it (B 0.54 m), which is why B is not used. Direction: the
            // wing-tip point J alone, the one point not tied to the propeller datum, needs 0.575 / 0.548 m, so the plane may sit up to
            // 0.09 m lower than the mean, which only lowers every clearance row (at 0.548 m the static propeller row is still
            // 0.88 m and the tyre-deflated one 0.45 m above their limits). The model tail-down angle on this static plane is
            // 6.9 deg (9.2 deg at the 0.35 D = 1.0 m plane it replaced; about 9.4 deg with the lift-off oleo extension). The
            // 8 deg below is a take-off/landing tail-skid design attitude, not a static gear-compressed contact angle, so the
            // static 6.9 deg is not measured against it and the plane is not set from either value.
            fuselage_ground_clearance_m: Some(0.64),
            // Tail-strike attitude 8 deg: the ATR maintenance manual's tail skid "is
            // designed to avoid fuselage contact with the runway when the take-off or
            // landing attitude has an angle of 8 deg or greater" (quoted in UK AAIB
            // Bulletin 8/2006, ATR 72-212A D-ANFH, EW/C2005/09/04, p.3); the skid sits
            // between frames 38 and 39. That is the take-off or landing attitude (main
            // gear extended or unloading) the skid protects, not a static,
            // gear-compressed fuselage-contact angle: in the same event the fuselage
            // touched at frames 36-38 (p.3) at 6.5 deg pitch and a 2.7 g touchdown (p.5).
            // The model's static tail-down angle (6.9 deg) is therefore no evidence of a
            // contour defect. The published 8 deg is kept as the tip-back floor.
            min_tip_back_deg: 8.0,
            // The same 8 deg is the rotation attitude the built fuselage must reach
            // without touching: the attitude the real aircraft reaches before its
            // skid does (AAIB Bulletin 8/2006 p.3, above). Gear state not stated.
            required_rotation_angle_deg: 8.0,
            required_rotation_basis: crate::landing_gear::RotationAngleBasis::PublishedStateUnspecified,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 27.05,
            // The factsheet 2020 p.22 plan view draws a constant-chord centre
            // section out past the nacelles and a straight-tapered outer
            // panel. With the kink at the 0.32 semispan station, these two
            // chords close the factsheet's 61 m^2 and the 2.303 m MAC of JCAB
            // TCDS No. 75 Rev 3, ATR 72-212A item (10), exactly.
            root_chord_m: 2.590_444,
            break_chord_m: 2.590_444,
            tip_chord_m: 1.604_089,
            // S: Jane's All the World's Aircraft 2008-09, ATR 72 entry, STRUCTURE: "sweepback on outer panels 2 deg 18 min at
            // quarter-chord". Outer-panel LE from the exact straight-panel identity
            // tan(LE) = tan(c/4) + (c_kink - c_tip) / (4 (y_tip - y_kink)), with the chords above and the 0.32 kink
            // (y 4.328 to 13.525 m): tan 2.30 deg + 0.98636 / 36.788 = 0.06698, 3.832 deg. The printed angle (to the minute)
            // is finer than the drawing: the ATR 72 three view (EWADE 2011, p.3) reads 4.5 deg from LE 11.24 / 11.45 / 11.67 m
            // at y 6.76 / 9.47 / 12.17 m, but a 0.06 m reading error over that 5.41 m base is 0.6 deg; 3.83 deg puts the
            // tip LE 0.11 m forward of that reading. Unswept to the kink (wing.inboard_le_sweep_deg), as the drawing shows.
            sweep_deg: 3.832,
            // Not published (Jane's 2008-2009 gives the root only): [E].
            tip_twist_deg: -2.0,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: 27.166,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        speed_schedule_source: crate::presets::SpeedScheduleSource::HandWritten,
        airfoil_class_source: "Not transonic: the 0.44 cruise Mach is below the wave-drag onset Mach, so the class does not enter its drag. Declared conventional, matching the drawn NACA 23018 root and 23012 tip sections.",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values; the drawn planform stays the geometry.
                // S 61.0 m2: ATR 72-600 reference wing area; MAC 2.303 m and the MAC leading edge 13.604 m aft of station 0 (= 11.242 m aft of the nose,
                // station 0 2.362 m ahead of the nose): ATR Weight and Balance Manual LIM.1 p.03 (15 JAN 2021).
                reference_wing: crate::ReferenceWingConfig::published(61.0, Some(2.303), Some(11.242)),
                // ATR Weight and Balance Manual, LIMITATIONS LIM.1 p.03 (15
                // JAN 2021) and JCAB TCDS No. 75 Rev 3 items (9)-(10): the
                // reference-chord leading edge is 13.604 m aft of station 0,
                // which is 2.362 m forward of the nose, so 11.242 m aft of the
                // nose. This datum puts the built wing's MAC leading edge
                // (`Wing::aerodynamic_center(0.0)`) on that station.
                // ATR 72 three view drawing (EWADE 2011 presentation, p.3): LE 11.05 m closes the plan LE and TE and gives the MAC LE 11.24 m (was 11.006 m).
                root_datum_x_m: 11.05,
                // Same drawing: front view upper surface rises 1.5 deg inboard and 1.4 deg outboard of the nacelle, tip top 0.165 m over the earlier flat-centre
                // fit; the side view holds the absolute height (centre top 1.72 m), so the rise is split: root lowered 0.02 m, break +0.02 m, tip +0.05 m.
                root_z_m: 1.45,
                break_z_m: 1.49,
                tip_z_m: 1.85,
                // Root incidence 2 deg: Jane's All the World's Aircraft
                // 2008-2009, ATR 42 entry, p. 300 (constant-chord,
                // no-dihedral centre section, 2 deg incidence at the root),
                // which the ATR 72 entry (p. 302, "As ATR 42") shares. The break and tip
                // incidences are not published there: estimates [E].
                root_twist_deg: 2.0,
                break_twist_deg: 0.0,
                break_span_fraction: 0.32,
                kink_span_fraction: None,
                // On the 1.4325 m fuselage half width; the inherited default 0.10 is inside the body.
                side_of_body_span_fraction: Some(0.106),
                // The fuselage-side station carries the constant centre-section
                // chord. Meshing it as an explicit section keeps the lattice of
                // the neutral-point condition probes resolvable: without it
                // the high-lift probe's influence matrix reports a pivot ratio
                // of 1.2e4 and the critical neutral point silently falls back
                // to the clean one.
                side_of_body_chord_ratio: Some(1.0),
                outboard_sweep_decrement_deg: 0.0,
                inboard_le_sweep_deg: Some(0.0),
                // Same drawing, plan view: the tip rounds from the LE line at 0.97 span to a 0.85 m chord on the straight TE, drawn as a rake.
                tip_rake_start_span_fraction: Some(0.97),
                tip_rake_tip_chord_m: Some(0.85),
                root_airfoil: "naca23018".to_owned(),
                tip_airfoil: "naca23012".to_owned(),
                airfoil_class: crate::AirfoilClass::Conventional,
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // ATR 72 three view drawing (EWADE 2011 presentation, p.3), plan and side views on the printed 27.166 m length;
                // nose x=0, barrel centre z=0, z up, +-0.08 m per raster pixel. The tailplane root LE sits on the fin tip LE.
                // Tailplane: root LE 24.90 m, TE 26.85 m at the centreline; tip LE 25.36 m, TE 26.52 m at y 3.66 m (plan, the
                // 7.25 m drawn tip to tip against 7.31 m published). The 8.10 m on the drawing is the propeller axes, not this span.
                hstab_offset_from_tail_m: 27.166 - 24.90,
                // Side view: tailplane lens centre 4.72 m, 0.75 m below the fin top (was 4.87 m, 0.15 m above the drawn lens); the front view upper surface agrees at 4.90 m.
                hstab_z_m: 4.72,
                hstab_root_chord_m: 1.95,
                hstab_tip_chord_m: 1.16,
                hstab_root_twist_deg: -1.0,
                hstab_tip_twist_deg: -1.0,
                hstab_tip_le_m: (0.46, 3.66, 0.0),
                // Same drawing, plan: the LE runs ahead of its straight line inboard of y 2.6 m, 0.115 m at the fuselage side, and is held there down to the root;
                // the tip LE corner rounds aft over the last 0.5 m of span, with the TE 0.05 m aft of its line near the tip.
                hstab_edge_stations: [(0.22, -0.115, 0.0), (0.71, 0.0, 0.0), (0.85, 0.0, 0.0), (0.90, 0.09, 0.03), (0.93, 0.13, 0.05), (0.95, 0.22, 0.05), (0.97, 0.43, 0.05), (1.0, 0.62, 0.0)]
                    .map(|(span_fraction, le_offset_m, te_offset_m)| crate::TailEdgeStation { span_fraction, le_offset_m, te_offset_m })
                    .to_vec(),
                // Fin, side view: the drawn LE is curved below z 3.5 m (x 21.4/22.1/23.1/23.5/24.2/24.7 m at z 2.1/2.4/3.1/3.3/4.1/4.9 m) and straight above;
                // the fin LE is a 35.6 deg line to the tip LE 24.98 m (within 0.15 m above z 3.3 m); TE line (25.90, 1.0) to (27.06, 5.47) m on the drawn TE (within 0.06 m); ink top z 5.47 m, 7.62 m over the drawn ground (7.65 m printed).
                vstab_offset_from_tail_m: 27.166 - 21.78,
                vstab_z_m: 1.0,
                vstab_root_chord_m: 4.12,
                vstab_tip_chord_m: 2.08,
                vstab_tip_le_m: (3.20, 0.0, 4.47),
                // Dorsal: side view, leaves the crown at x 17.3 m, joins the fin LE about z 2.25 m.
                vstab_dorsal_length_m: Some(4.48),
                vstab_dorsal_height_m: Some(1.25),
                // Drawn fin LE below the fitted line (fillet into the dorsal) and the small bow above it, read off the side view at z 2.06/2.42/3.05/3.58/4.71 m;
                // the last three stations round the top corners, z 5.16/5.36/5.47 m.
                vstab_edge_stations: [(0.237, -1.10, 0.0), (0.318, -0.71, 0.0), (0.458, -0.21, 0.0), (0.577, 0.18, 0.0), (0.83, 0.10, 0.0), (0.93, 0.15, 0.0), (0.975, 0.30, -0.14), (1.0, 0.62, -0.40)]
                    .map(|(span_fraction, le_offset_m, te_offset_m)| crate::TailEdgeStation { span_fraction, le_offset_m, te_offset_m })
                    .to_vec(),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                // 2.865 m (was 2.77): third-party
                // ATR 72-600 familiarisation handout "maximum cabin width"
                // 2865 mm, read as the external width (secondary; no primary
                // external width in EASA TCDS EASA.A.084, the 2020 factsheet or
                // the EWADE 2011 presentation), consistent with the 2.57 m
                // cabin width plus walls. The three-view plan draws 2.974 m.
                diameter_m: 2.865,
                // 2.71 m: factsheet 2020 side render (three-view side 2.70 m);
                // old value None (circular 2.77 m).
                height_m: Some(2.71),
                // Drawing nose tip (loft fit round 4; old value -0.65).
                nose_z_m: -0.52,
                cabin_start_x_m: 3.0,
                // Crown stays at 1.385 m: 0.03 + 2.71 / 2 (old 0.0 + 2.77 / 2).
                cabin_z_m: 0.03,
                tailcone_length_m: 6.0,
                // Drawing tail end (loft fit round 4; old value 1.0).
                tail_z_m: 0.64,
                // FUSELAGE-G2 shaped nose and aft body. Outline traced from the manufacturer drawing
                // (side: ATR 72-600 factsheet (2020) side view render (supplement); top: ATR 72-600 factsheet),
                // calibrated to the drawn length; the nose law (windshield, radome, keel and plan
                // exponents) is a least-squares fit to the traced upper, lower and plan lines (RMS 0.084 m),
                // the aft law (belly ramp, crown drop, plan taper, closing section) to the traced aft lines
                // (RMS 0.056 m). Cabin start, tailcone length and every payload and mass station are unchanged:
                // only the outer loft moves. Section exponent 2.0 (ellipse): no drawing measures it.
                // Data quality: photograph-quality side and top views; aft read by eye, nose upper/lower fitted to a noisy mask and the plan exponent set to 1.8 by eye on the cockpit-window bulge.
                nose_length_m: Some(3.04),
                nose_windshield_angle_deg: Some(38.6),
                nose_crown_end_fraction: Some(1.0),
                nose_radome_length_fraction: Some(0.21),
                // Needs data: the real ATR 72-600 nose is flat-bottomed (ARM point B 0.54 m above ground, ATR 42/72 ARM 1-10-06 Fig. 1-28, not on disk), while the
                // loft's nose bottom sits 0.8 m above the belly. The nose law here is fitted to the side view and has no lower-nose section data; a drawn nose-bottom outline (or a
                // sourced nose section table) is needed before the keel law can be re-fitted.
                nose_keel_exponent: Some(2.91),
                nose_plan_exponent: Some(1.8),
                nose_section_exponent: Some(2.0),
                belly_upsweep_length_m: Some(7.3),
                // Needs data: the aft lower contour (belly ramp, frames 38-39 where the tail skid sits) is not on disk; no drawing shows this loft wrong.
                // The 8 deg tip-back floor is a take-off/landing skid attitude (UK AAIB Bulletin 8/2006, D-ANFH, p.3), not the static gear-compressed angle
                // (model 6.9 deg), so the two are not a contour gap; a gear-compressed tail-strike attitude from a primary ATR or investigation source is
                // the comparison still wanted. Neither this exponent nor the 8 deg floor is a tuning lever.
                aft_belly_exponent: Some(1.42),
                tail_end_z_m: Some(0.64),
                tail_end_width_m: Some(0.38),
                tail_end_height_m: Some(0.5),
                // D: side view x 9.5 to 15.4 m, bottom z -1.7 m against the keel at -1.32 m;
                // front view lobes to y 1.82 m at the 2.865 m hull width. Parasite drag only, not built.
                gear_sponson_length_m: Some(5.9),
                gear_sponson_half_width_m: Some(1.82),
                gear_sponson_depth_m: Some(0.38),
                aft_plan_length_m: Some(8.7),
                aft_plan_exponent: Some(1.0),
                aft_crown_length_m: Some(6.44),
                aft_crown_exponent: Some(1.0),
                // Cabin section: floor 2.04 m below the inner cabin crown on ATR's cabin cross-section (EWADE 2011 p. 5) plus the loft's own 0.074 m of skin ((2.718 m outer height - 2.570 m inner circle) / 2); the class section leaves no underfloor hold; cabin-datum-evidence.md.
                // Needs data: the floor datum here (2.114 m below the crown) gives a main floor about 2.0 m above the registered plane, while the ATR 72 ARM
                // door-sill points D (1.67 m) and E (1.42 m) (not on disk) imply a floor 0.2 to 0.74 m lower depending on the door. Re-deriving the floor depth needs the
                // measured section floor line (S1/05 datum); the EWADE 2011 value above is not revised without it.
                cabin_section: Some(crate::CabinSectionConfig { cabin_widths: Some(crate::CabinWidthsConfig { armrest_m: 2.559, shoulder_m: 2.509 }), floor_below_crown_m: Some(2.114), hold: None }),
                ..FuselageConfig::default()
            },
            engine,
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.44,
            cruise_altitude_m: 5_180.0,
            mtow_kg: 23_000.0,
            max_wing_area_m2: 65.0,
            min_wing_loading_kg_m2: 330.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 72,
            cargo_payload_kg: 7_550.0,
            max_structural_payload_kg: 7_550.0,
            // Superseded at registration: the profile declares the TCDS VMO/MMO, so VD is the
            // CS 25.335(b) value on VC = VMO (`crate::design_speeds`, `registry`).
            dive_speed_m_s: 150.0,
            ..DesignRequirements::default()
        },
        // No subsystem fractions are inferred from the published OEW. A
        // turboprop/regional-airframe mass model must be calibrated separately.
        mass_model: None,
        performance: super::high_lift("conservative_simple_flaps"),
    }
}

#[cfg(test)]
// Tests assert on the gear anchor fields of the ATR preset this module
// builds, so a failed expect is the assertion failing, not a library
// invariant being broken.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn atr_conceptual_t_tail_attaches_and_stays_inside_drawing_envelope() {
        let atr = atr72_600();
        let e = &atr.geometry.empennage;
        let length = 27.166;
        let h_x = length - e.hstab_offset_from_tail_m;
        let v_x = length - e.vstab_offset_from_tail_m;
        // Builder's nominal tail_scale=1, tail_x_shift=0 coordinates.
        assert!((h_x - (v_x + e.vstab_tip_le_m.0)).abs() < 0.10);
        // Tailplane 0.75 m below the fin top (cap above), x seat only
        assert!((e.hstab_z_m - (e.vstab_z_m + e.vstab_tip_le_m.2 - 0.75)).abs() < 1e-10);
        // tail fit: the drawn fin tip chord (2.16 m) exceeds the drawn tailplane root chord (1.95 m);
        // the T-tail rule only seats the tailplane root LE on the fin tip LE, so the chords are free.
        assert!(e.hstab_root_chord_m > 0.0 && e.vstab_tip_chord_m > 0.0);
        assert!((e.hstab_z_m - 4.85).abs() < 0.3);
        for trailing_edge in [
            h_x + e.hstab_root_chord_m,
            h_x + e.hstab_tip_le_m.0 + e.hstab_tip_chord_m,
            v_x + e.vstab_root_chord_m,
            v_x + e.vstab_tip_le_m.0 + e.vstab_tip_chord_m,
        ] {
            // the drawn fin tip TE overhangs the nominal body length by about 0.1 m
            assert!(trailing_edge.is_finite() && trailing_edge < length + 0.2);
        }
        assert!(e.hstab_tip_le_m.1 > 0.0);
        assert_eq!(e.hstab_tip_le_m.2, 0.0);
    }

    #[test]
    fn atr_identity_and_certified_mass_limits_are_not_mixed() {
        let atr = atr72_600();
        assert_eq!(atr.identity.model, "ATR 72-212A");
        assert_eq!(atr.identity.engine_model, "PW127M");
        assert_eq!(atr.reference.mtow_kg, Some(23_000.0));
        assert_eq!(atr.reference.mlw_kg, Some(22_350.0));
        assert_eq!(atr.reference.mzfw_kg, Some(21_000.0));
        assert_eq!(atr.requirements.mtow_kg, 23_000.0);
    }

    #[test]
    fn atr_pins_the_measured_longitudinal_gear_station_anchor() {
        // ATR 42 / ATR 72 Aircraft Recovery Manual, 1-10-01 p.19 Figure 1-2
        // "ATR 72-212A Main Dimensions": one ground-line dimension chain reads
        // 1,728 m (68,03 in) nose to nose wheel, then 10,772 m (424,09 in) to
        // the main wheel, under a 27,166 m (1069,53 in) overall length taken
        // from the same nose tip. The three anchor fields are declared together
        // or not at all; a partial triple is rejected by
        // `LandingGearConfig::validate`, and an estimate in any one of them
        // would be indistinguishable downstream from this measurement.
        let atr = atr72_600();
        let gear = &atr.landing_gear;
        assert_eq!(
            gear.reference_station_frame.as_deref(),
            Some("nose_tip_drawing_reference"),
        );

        let length_m = gear
            .reference_station_fuselage_length_m
            .expect("ATR drawing fuselage length");
        assert!(
            (length_m - 27.166).abs() < 1e-9,
            "drawing fuselage length {length_m} m",
        );

        let nlg_fraction = gear.reference_nlg_x_fraction.expect("ATR NLG fraction");
        let nlg_x_m = length_m * nlg_fraction;
        assert!(
            (nlg_x_m - 1.728).abs() < 1e-9,
            "x_nlg {nlg_x_m} m from nose"
        );

        let mlg_fractions = gear
            .reference_mlg_x_fractions
            .as_deref()
            .expect("ATR MLG fractions");
        assert_eq!(mlg_fractions.len(), 2, "two sponson main-gear legs");
        for fraction in mlg_fractions {
            let mlg_x_m = length_m * fraction;
            assert!(
                (mlg_x_m - 12.500).abs() < 1e-9,
                "x_mlg {mlg_x_m} m from nose"
            );
        }

        // The wheelbase the two anchors imply is the 10.772 m the same figure
        // prints, and the 10.77 m two independent ATR three-views carry.
        let wheelbase_m = length_m * (mlg_fractions[0] - nlg_fraction);
        assert!(
            (wheelbase_m - 10.772).abs() < 1e-9,
            "implied wheelbase {wheelbase_m} m",
        );
        assert_eq!(gear.reference_wheelbase_m, Some(10.772));
        assert_eq!(gear.reference_track_m, Some(4.10));
    }

    /// The ATR Weight and Balance Manual datum, kept as an executable
    /// conversion rather than prose: LIMITATIONS LIM.1 p.03 (15 JAN 2021)
    /// states "Station 0 is 2.362 m forward of the fuselage nose" and puts the
    /// reference chord leading edge 13.604 m aft of station 0, and the recovery
    /// manual's frame table (1-10-04 p.27, drawing 9SMJ 062110 ZON 00110-004)
    /// puts frame 0 at STA 2362 mm. So the nose-tip frame used by the anchors
    /// above and the ATR station frame differ by exactly 2.362 m, and the
    /// manual's own tail-tipping limit lands just forward of the main wheels.
    #[test]
    fn atr_nose_tip_anchor_frame_matches_the_published_atr_station_frame() {
        const STATION_ZERO_AHEAD_OF_NOSE_M: f64 = 2.362;
        const TAIL_TIPPING_H_ARM_M: f64 = 14.848;

        let atr = atr72_600();
        let gear = &atr.landing_gear;
        let length_m = gear
            .reference_station_fuselage_length_m
            .expect("ATR drawing fuselage length");
        let mlg_x_m = length_m
            * gear
                .reference_mlg_x_fractions
                .as_deref()
                .expect("ATR MLG fractions")[0];

        let mlg_station_m = mlg_x_m + STATION_ZERO_AHEAD_OF_NOSE_M;
        let margin_m = mlg_station_m - TAIL_TIPPING_H_ARM_M;
        assert!(
            (0.0..0.05).contains(&margin_m),
            "tail-tipping limit must sit just forward of the main wheels, got {margin_m} m",
        );
    }

    #[test]
    fn atr_wing_root_sits_above_the_fuselage_crown() {
        // The geometric fact that keeps the wing-mounted fallback out of its
        // domain here, pinned at its source: the wing root chord plane is
        // above the fuselage outer surface, so no wing-root gear bay
        // exists. z is measured up in the geometry frame, m.
        let atr = atr72_600();
        let crown_z_m =
            atr.geometry.fuselage.cabin_z_m + atr.geometry.fuselage.effective_height_m() / 2.0;
        assert!(
            atr.geometry.wing.root_z_m > crown_z_m,
            "ATR wing root at {} m is not above the fuselage crown at {crown_z_m} m",
            atr.geometry.wing.root_z_m
        );
    }

    #[test]
    fn atr_mounts_a_symmetric_pw127m_pair() {
        let atr = atr72_600();
        assert_eq!(atr.engine_name, "PW127M");
        assert_eq!(atr.geometry.engine.engine_name, "PW127M");
        assert_eq!(atr.engine_spanwise_positions(), &[4.05, -4.05]);
    }
}
