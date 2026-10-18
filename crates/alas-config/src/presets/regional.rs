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
pub fn atr72_600() -> AircraftPreset {
    let mut engine = EngineConfig {
        engine_name: "PW127M".to_owned(),
        ..EngineConfig::default()
    };
    // Materialize the technology binding before overriding installation
    // geometry. The embedded catalogue is the authoritative source for the
    // PW127M/568F power ratings; the preset owns where that system is mounted.
    engine.apply_engine_spec();
    // Outline read off the ATR 72-600 factsheet 2020 p.22 plan and side
    // renders (raster, +-0.2 m; low confidence): spinner tip to the aft
    // nacelle fairing is 6.2 m (the nacelle runs back over the wing to
    // 14.7 m aft of the nose, past the trailing edge), 1.25-1.3 m across
    // between 2.8 and 4.6 m, spinner cone over the first 0.7 m.
    engine.nacelle_profile = vec![
        (0.0, 0.20),
        (0.3, 0.42),
        (0.7, 0.62),
        (1.3, 0.84),
        (2.0, 0.97),
        (2.8, 1.0),
        (4.6, 1.0),
        (5.4, 0.72),
        (6.4, 0.28), // Step B: aft end 6.2 -> 6.4 m (side view zoom, drawn 14.96 m aft of the nose, C)
    ];
    engine.radius_scale_m = 0.65;
    // ATR 72-600 factsheet 2020 p.22 states an 8.10 m propeller-axis
    // separation; half of that is the single-engine spanwise station.
    engine.spanwise_positions_m = vec![4.05, -4.05];
    engine.z_m = -0.70;
    // ATR 72-600 factsheet 2020 p.22 side view: the spinner tip sits
    // 8.45 +/- 0.15 m aft of the nose (read against both the printed
    // 27.166 m length and the 10.77 m wheelbase, which disagree by 3 %),
    // 2.70 m ahead of the wing leading edge at the 4.05 m nacelle station.
    // Independent check: JCAB TCDS No. 75 Rev 3, ATR 72-212A item (17)(a),
    // engine oil H-arm 12.190 m, i.e. 9.828 m aft of the nose with the
    // 2.362 m datum of item (9). The propulsion mass station (40 % of the
    // 6.2 m nacelle from the spinner tip, 10.91 m) lands 1.08 m aft of it; the
    // earlier mid-length rule (11.53 m) landed 1.70 m aft of it, not 0.10 m as
    // this comment used to state (fix lane cg-atr-a400m 2026-10-14).
    // Step B: 2.70 -> 2.613 m so the spinner stays at 8.428 m with the root datum 11.0060 m and the 0.5 deg inboard LE at the nacelle station.
    engine.inlet_x_offset_m = 2.613;

    AircraftPreset {
        name: "ATR72-600",
        display_name: "ATR 72-600",
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
                range_nmi: 758.0,
                payload_kg: None,
                source: "ATR 72-600 Factsheet (PW127M/N edition, 2020-07) p.2: range with maximum passengers (72 seats) 758 nmi; reserves and payload mass not stated, so the planning cabin is the payload",
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
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            sources: vec![
                "ATR ATR 72-600 Airport Planning Manual, Issue 8, 2021, aircraft characteristics and limitations",
                "EASA Type Certificate Data Sheet EASA.A.084, ATR 42/72, Issue 14, 23 February 2026, section III.13.b \"ATR 72-212A models\" (Mod 6219 column: MRW 23,170 kg, MTOW 23,000 kg, MLW 22,350 kg, MZFW 21,000 kg) and Note VII.1 (Mod 5948 = New Avionic Suite / \"Glass Cockpit\", the commercial \"ATR 72-600\" designation, a separate modification from the weight variant)",
                "ATR 42 / ATR 72 Aircraft Recovery Manual, 1-10-01 p.19 Figure 1-2 (ATR 72-212A main dimensions: 1.728 m nose to nose wheel, 10.772 m wheelbase, 27.166 m length, 4.10 m track), 1-10-04 p.27 fuselage frame stations (drawing 9SMJ 062110 ZON 00110-004, frame 0 at STA 2362 mm), 4-00-02 Figure 4-1 tail-tipping CG limit H-arm 14.848 m = 54 percent MAC",
                "ATR Weight and Balance Manual, LIMITATIONS LIM.1 p.03, 15 JAN 2021, weight variant F2/75 (MAC 2.303 m; station 0 is 2.362 m forward of the fuselage nose; station 0 to reference chord leading edge 13.604 m)",
                "ATR ATR 72-600 Facts and Figures, product specification (accessed 2026-08-30)",
            ],
            ..AircraftReferenceData::default()
        },
        engine_name: "PW127M",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            wheels_per_mlg_strut: 2,
            // Approximate 4.1 m main-wheel track divided by 2.77 m fuselage width.
            track_diameter_factor: 4.1 / 2.77,
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
            // UK AAIB Bulletin AAIB-31376 (ATR 72-212A G-OASB, PW127M), p.3:
            // pre-flight full-and-free check "left elevator range of +12.7 deg
            // to -21.8 deg" (negative = nose-up). One airframe's measured
            // stop, not the AMM nominal travel; the stabiliser is fixed and
            // trimmed by elevator tabs (ATSB AO-2014-032, Fig. 4), so no
            // takeoff stabiliser setting is credited.
            elevator_up_travel_deg: Some(21.8),
            // Tail-strike attitude 8 deg: the ATR maintenance manual's tail skid "is
            // designed to avoid fuselage contact with the runway when the take-off or
            // landing attitude has an angle of 8 deg or greater" (quoted in UK AAIB
            // Bulletin 8/2006, ATR 72-212A D-ANFH, EW/C2005/09/04, p.3); the skid sits
            // between frames 38 and 39. The generic tailcone loft understates the
            // geometric tail-down angle, so this is the floor.
            min_tip_back_deg: 8.0,
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
            // Leading-edge sweep, an engineering estimate: no value is
            // published, and the factsheet three-view shows a nearly unswept
            // inboard leading edge and about 4 deg outboard of the nacelle,
            // which one straight leading edge cannot carry.
            // Step B (kinked LE, measure/ATR72-600, C +-0.6 deg): inboard LE 0.5 deg (drawn 0, drawn constant-chord centre section, rows 311-329 px flat) and 4.5 deg outboard of the
            // 0.32 kink through wing.outboard_le_sweep_deg; was one straight 3.0 deg LE (tip LE x 11.62 -> 11.64 m, drawn 11.71). Chords, kink and the 61 m^2 / 2.303 m MAC are unchanged.
            sweep_deg: 0.5, // 0.0 is refused by the design-space +-10 % sweep guardrail test; 0.5 deg is 0.04 m over the inboard panel
            tip_twist_deg: -2.0,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: 27.166,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        airfoil_class_source: "Not transonic: the 0.44 cruise Mach is below the wave-drag onset Mach, so the class does not enter its drag. Declared conventional, matching the drawn NACA 23018 root and 23012 tip sections.",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values (geometry freeze 2026-10-12, owner decision); the drawn planform stays the geometry.
                // S 61.0 m2: ATR 72-600 reference wing area; MAC 2.303 m and the MAC leading edge 13.604 m aft of station 0 (= 11.242 m aft of the nose,
                // station 0 2.362 m ahead of the nose): ATR Weight and Balance Manual LIM.1 p.03 (15 JAN 2021).
                reference_wing: crate::ReferenceWingConfig::published(61.0, Some(2.303), Some(11.242)),
                // ATR Weight and Balance Manual, LIMITATIONS LIM.1 p.03 (15
                // JAN 2021) and JCAB TCDS No. 75 Rev 3 items (9)-(10): the
                // reference-chord leading edge is 13.604 m aft of station 0,
                // which is 2.362 m forward of the nose, so 11.242 m aft of the
                // nose. This datum puts the built wing's MAC leading edge
                // (`Wing::aerodynamic_center(0.0)`) on that station.
                // Step B: 10.916 -> 11.0060 m so the MAC leading edge stays where the 3.0 deg straight LE put it (11.216 m) with the kinked LE (drawn root LE 11.05 m, 0.044 m away).
                root_datum_x_m: 11.0060,
                root_z_m: 1.85,
                break_z_m: 1.85,
                tip_z_m: 1.85,
                root_twist_deg: 2.0,
                break_twist_deg: 0.0,
                break_span_fraction: 0.32,
                kink_span_fraction: None,
                // The fuselage-side station carries the constant centre-section
                // chord. Meshing it as an explicit section keeps the lattice of
                // the neutral-point condition probes resolvable: without it
                // the high-lift probe's influence matrix reports a pivot ratio
                // of 1.2e4 and the critical neutral point silently falls back
                // to the clean one.
                side_of_body_chord_ratio: Some(1.0),
                outboard_sweep_decrement_deg: 0.0,
                outboard_le_sweep_deg: Some(4.5),
                root_airfoil: "naca23018".to_owned(),
                tip_airfoil: "naca23012".to_owned(),
                airfoil_class: crate::AirfoilClass::Conventional,
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // Approximate reconstruction of ATR's 2020 factsheet p.22,
                // not certified surface stations: +/-0.3 m digitization/model
                // uncertainty. Nose x=0, barrel centre z=0, z positive up.
                // The T-tail root and fin tip share (x,z)=(24.8,4.7) m.
                // geo/tailfit 2026-10-11: tailplane straight-edge fit to the drawing, centreline values; was off 27.166 - 25.0, chords 1.9/1.2, tip (0.75, 3.7, 0.0)
                hstab_offset_from_tail_m: 27.166 - 24.85,
                // Step B (measure/ATR72-600, C; DECISIONS.md T-tail rule): the fin top is raised +0.75 m (3.7 -> 4.45 m above the root line; drawn top 5.45 m over the barrel centre,
                // published height 7.65 m agrees) and the hstab moves with it, 4.7 -> 5.45 m, so the root stays on the fin tip. Drawn hstab is 0.72 m below the fin top; not modelled.
                // Root LE 24.8 -> 25.0 m and root chord 1.8 -> 1.9 m (equal to the fin tip chord; drawn 1.95 m, LE 24.8 m, TE 26.75 m): the builder test keeps the root on the fin tip in x and chord, so the drawn values are met within the +-0.2 m read. Tip chord 1.2 m unchanged; tip LE offset (0.6, 3.6) -> (0.75, 3.7) m (drawn 11.5 deg LE sweep vs 9.5, semispan 3.69 m).
                // geo/close 2026-10-12 (owner review, front view): the drawn tailplane sits 0.60 m below the drawn fin top (front sheet: model tailplane 0.5 m high; Step B already noted 0.72 m).
                // The fin top stays at the side-view height 4.45 m, so the tailplane root is no longer on the fin tip in z (x seat kept): the fin cap above the tailplane is drawn and cannot be held
                // by an attached fin tip. CONFLICT with the T-tail attachment rule recorded in geo-close-2026-10-12.md. was 5.45 (= vstab_z_m + fin tip z)
                hstab_z_m: 4.85,
                hstab_root_chord_m: 2.0,
                hstab_tip_chord_m: 1.34,
                hstab_root_twist_deg: -1.0,
                hstab_tip_twist_deg: -1.0,
                hstab_tip_le_m: (0.41, 3.7, 0.0),
                // Step B (side view fit incl. the dorsal fairing, +-0.5 m, C): root LE 22.3 -> 20.2 m (offset 6.966, was 27.166 - 22.3 = 4.866), root chord 3.8 -> 5.4 m (TE 25.6 m),
                // tip LE offset 2.5 -> 4.8 m (46 deg LE sweep, was 34; drawn 5.0 m / 48 deg, reduced so the fin tip LE sits at the hstab root LE 25.0 m), height 3.7 -> 4.45 m above the crown line. Tip chord 1.8 m kept. The fin root chord includes the dorsal fairing (not modelled).
                // FINAL MERGE (DECISIONS, ATR72-600 fin): the step B dorsal-inclusive silhouette (root LE 20.2 m, root chord 5.4 m) gave Sv 16.3 m2 (+55 %) and is REJECTED.
                // Fin = sourced vertical-tail area convention: root LE back to 22.3 m (offset 4.866, the pre-step-B value), Sv held at the pre-step-B built 10.4 m2 (published
                // ~10.5 m2 per the coordinator's earlier sourcing; no primary ATR fin area could be retrieved offline, flagged for verification) with the measured 4.45 m height:
                // root chord 3.8 -> 2.82 m (area (2.82 + 1.9) / 2 * 4.45 = 10.5 m2), tip LE x offset 4.8 -> 2.7 m so the tip LE stays at the hstab root LE 25.0 m. The drawn dorsal
                // shoulder is the separate geometry-only dorsal surface below (not in the fin area).
                // geo/tailfit 2026-10-11: fin straight-edge fit to the drawing at the root line (rms 0.011/0.020 m); was off 27.166 - 22.3, chords 2.82/1.9, tip (2.7, 0.0, 4.45)
                vstab_offset_from_tail_m: 27.166 - 22.36,
                vstab_z_m: 1.0,
                vstab_root_chord_m: 3.57,
                vstab_tip_chord_m: 2.42, // drawn 1.9 m; was 1.8
                vstab_tip_le_m: (2.49, 0.0, 4.45),
                // Step B: dorsal fairing shoulder (side view, C): from the crown at x 19.8 m rising to the fin LE at ~1.0 m above the root line. Geometry only (vstab_dorsal_in_aero false).
                // geo/tailfit 2026-10-11: dorsal re-fitted to the drawn LE; was x Some(19.8), height Some(1.0)
                vstab_dorsal_x_start_m: Some(18.82),
                vstab_dorsal_height_m: Some(1.8),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                // 2.865 m (geometry phase step A; old value 2.77): third-party
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
                nose_keel_exponent: Some(2.91),
                nose_plan_exponent: Some(1.8),
                nose_section_exponent: Some(2.0),
                belly_upsweep_length_m: Some(7.3),
                aft_belly_exponent: Some(1.42),
                tail_end_z_m: Some(0.64),
                tail_end_width_m: Some(0.38),
                tail_end_height_m: Some(0.5),
                aft_plan_length_m: Some(8.7),
                aft_plan_exponent: Some(1.0),
                aft_crown_length_m: Some(6.44),
                aft_crown_exponent: Some(1.0),
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
            // Preliminary-design VD consistent with the ATR transport-speed
            // envelope; retained as a modelling input, not an AFM limitation.
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
        assert!((h_x - (v_x + e.vstab_tip_le_m.0)).abs() < 1e-10);
        // geo/close: tailplane 0.6 m below the fin top (cap above), x seat only
        assert!((e.hstab_z_m - (e.vstab_z_m + e.vstab_tip_le_m.2 - 0.60)).abs() < 1e-10);
        // geo/tailfit: the drawn fin tip chord (2.42 m) exceeds the drawn tailplane root chord (2.0 m);
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
