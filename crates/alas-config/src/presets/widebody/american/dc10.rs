// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! McDonnell Douglas DC-10 preset (kept apart from the Boeing 787-9 table to
//! bound the size of each preset file).

use super::{outer_shape, wing_stations};
use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, WingConfig,
};

/// Trijet with a centerline engine, the oldest type in the registry.
pub fn dc_10() -> AircraftPreset {
    AircraftPreset {
        name: "DC-10",
        display_name: "McDonnell Douglas DC-10-30 (572k option)",
        class: crate::presets::AircraftClass::Transport,
        description: "DC-10-30 ACAP 572,000 lb option with three CF6-50C1 engines.",
        identity: AircraftVariantIdentity {
            model: "DC-10-30 passenger",
            weight_variant: "ACAP 572,000 lb option",
            engine_model: "CF6-50C1",
            modification_state: "DAC-67803A Rev A footnoted 572k planning option",
            tank_configuration: "36,522 US gal: wing tanks 1 and 3, tank 2, center-wing tank, under-wing auxiliary tank",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Vol. I Table 1-1 (aerodrome reference code) applied to the
            // preset wingspan of 50.39 m (McDonnell Douglas DC-10-30): 36 m <= b < 52 m
            // is code D.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::D),
            // The ACAP 572,000 lb footnote restates no ramp limit or OEW; the
            // standard-row values (253,105 kg MRW) are not mixed in here.
            mrw_kg: None,
            mtow_kg: Some(259_454.0),
            mlw_kg: Some(190_962.0),
            // Boeing DC-10 Airplane Characteristics for Airport Planning, section 2.1
            // (Model DC-10 Series 10, 30 and 40), Series 30 passenger column: MZFW
            // 368,000 lb (166,922 kg), the same column as the 266,191 lb OEW of the
            // registry basis. The 391,000 lb of the following table belongs to the
            // convertible 30CF, a different weight variant.
            mzfw_kg: Some(166_922.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("DC-10"),
            // Boeing, DC-10 Airplane Characteristics for Airport Planning, sec 5.4.4 "Ground Service
            // Connection Data, Series 30/30CF/40/40CF", printed p.111: 36,522 US gal usable
            // x 3.785411784 L/gal = 138,250 L (was 137,509 L, no primary source found).
            usable_fuel_volume_l: Some(138_250.0),
            // 138,250 L x 0.804 kg/L = 111,153 kg: the app's standard fuel density (`MassConfig::fuel_density_kg_m3`
            // = 804 kg/m3, alas-config mass.rs). The ACAP page prints no fuel weight, so no other density is sourced.
            usable_fuel_mass_kg: Some(111_153.0),
            reference_wing_area_m2: Some(338.84),
            planning_seats: Some(255),
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: None,
                cruise: None,
                range_nmi: 4_120.0,
                payload_kg: Some(45_993.0),
                source: "Boeing DC-10 Airplane Characteristics for Airport Planning section 3.2.1 p.54 (payload/range M0.82 step cruise, Series 30, 572,000 lb MTOGW curve): maximum-payload corner read as 4,120 nmi at 101,396 lb (45,993 kg); read uncertainty +-100 nmi; reserves per FAR 121.645 at a 200 nmi alternate",
                reserve: Some(crate::presets::DesignPointReserve {
                    scheme: Some(crate::FuelScheme::FaaFlagSupplemental),
                    contingency_trip_fraction: None,
                    contingency_minimum_hold_min: None,
                    alternate_distance_nmi: Some(200.0),
                    final_reserve_hold_min: Some(30.0),
                    flag_flight_time_fraction: Some(0.10),
                    holding_altitude_ft: Some(1_500.0),
                    source: "Boeing DC-10 Airplane Characteristics for Airport Planning section 3.2.1, printed p 47 (PDF p 54), Series 30 note 1: fuel reserves based on FAR 121.645 at 200 n mi; the terms are the rule's own (14 CFR 121.645(b): 10 percent of the flight time, the alternate, 30 min holding at 1,500 ft)",
                }),
            }),
            // Design mission, one provenance row per datum (ACAP section 3.2.1,
            // Series 30, 572,000 lb MTOGW curve). The ACAP prints no climb or descent
            // schedule.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::Manufacturer,
                    stated_conditions: Some("Series 30, 572,000 lb MTOGW curve, M0.82 step cruise, CF6-50C1, FAR 121.645 reserves at a 200 nmi alternate; range read from the chart at the maximum-payload corner"),
                    relative_uncertainty: Some(100.0 / 4_120.0),
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "Boeing DC-10 Airplane Characteristics for Airport Planning (DAC-67803A Rev A) section 3.2.1, printed p.47 (PDF p.54); corner read as 4,120 nmi, +-100 nmi",
                }),
                payload: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::Manufacturer,
                    stated_conditions: Some("maximum-payload corner of the 572,000 lb curve: 101,396 lb (45,993 kg)"),
                    relative_uncertainty: None,
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "Boeing DC-10 Airplane Characteristics for Airport Planning section 3.2.1, printed p.47 (PDF p.54)",
                }),
                profile: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::Manufacturer,
                    stated_conditions: Some("M0.82 step cruise; the ACAP prints no climb or descent schedule, so the model's configured schedules apply to those phases"),
                    relative_uncertainty: None,
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "Boeing DC-10 Airplane Characteristics for Airport Planning section 3.2.1, printed p.47 (PDF p.54)",
                }),
                reserve_fuel: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::Manufacturer,
                    stated_conditions: Some("fuel reserves based on FAR 121.645 at a 200 nmi alternate (14 CFR 121.645(b): 10 % of flight time, the alternate, 30 min holding at 1,500 ft); the reserve mass follows from the modelled trip"),
                    relative_uncertainty: None,
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "Boeing DC-10 Airplane Characteristics for Airport Planning section 3.2.1, printed p.47 (PDF p.54), Series 30 note 1",
                }),
            },
            certified_max_seats: Some(399),
            certified_exit_layout: Some(crate::presets::DC_10_30_EXIT_LAYOUT),
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            usable_cg_range: Some(crate::presets::usable_cg_range::DC10_30),
            // The ACAP publishes the Series 30 ground envelope at the
            // 583,000 lb maximum ramp weight; the 572,000 lb option lies
            // inside it.
            aft_cg_nose_load: Some(crate::presets::gear_load::DC_10_30),
            sources: vec![
                "Boeing DC/MD-10 ACAP DAC-67803A Rev A, Figure 2.1",
                "FAA TCDS A22WE Rev 13, 2018-04-30",
                "NASA CR-3119, April 1979",
            ],
            operating_speed_limits: Some(crate::presets::operating_limits::DC10_30),
            ..AircraftReferenceData::default()
        },
        engine_name: "CF6-50",
        n_engines: 3,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 3,
            // Two four-wheel wing bogies plus one two-wheel center gear.
            mlg_strut_bogie_wheels: Some(vec![4, 4, 2]),
            wheels_per_mlg_strut: 0,
            // Main and nose tires: 52x20.5-23 and 40x15.5-16 (application charts, DC-10-30/40); the ply is not
            // printed in the retrieved DC-10 documents, so the rated load is the lowest charted ply (26).
            nlg_tire: "40x15.5-16".to_owned(),
            mlg_tire: "52x20.5-23".to_owned(),
            track_diameter_factor: 1.77,
            // DAC-67803A Rev A, Figure 7.2.2 (footprint, Series 30): nose
            // gear to wing-gear bogie centers 72 ft 4.6 in (22.06 m), center
            // gear axle 30 in (0.76 m) aft of them, wing-gear track 35 ft
            // (10.67 m). Nose tip to nose gear 8.49 m (27.84 ft). Figure 2.2 itself prints 27 ft 10.5 in
            // (8.50 m) for that distance; its drawn gear legs stand 0.3-0.4 m ahead of their own dimensions (nose
            // leg 8.12 m, wing bogie 30.29 m at 55.935 px/m on the 600 dpi render), which is where the old value came
            // from. The Series 30 turning-radius table of the Boeing DC/MD-10 ACAP D6-58327 (2023
            // reissue, sec. 4.2, printed p. 71) agrees: its nose-gear radius R3 puts the
            // nose gear 72.94 ft ahead of the turning line (R3 sin(theta), 25-68 deg,
            // spread 0.07 ft) and its nose radius R5 puts the nose tip 100.78 ft ahead of
            // it (sqrt(R5^2 - (R3 cos theta)^2), spread 0.07 ft); the Series 10 table
            // gives 8.46-8.52 m. The turning line (22.23 m aft of the nose gear) lies 0.03 m from
            // the static-load centroid of the main group (22.20 m, wing bogies and center gear
            // above weighted by the section 7.3.2 loads). Was 8.1 m, a Figure 2.2 drawing read
            // scaled against its own 22.07 m wheelbase dimension (+-0.2 m). Stations
            // normalized to the fuselage length; the wheelbases above are kept, so the
            // main gear moves with the nose gear.
            reference_wheelbase_m: Some(22.06),
            reference_body_wheelbase_m: Some(22.82),
            reference_track_m: Some(10.67),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(51.97),
            reference_nlg_x_fraction: Some(8.49 / 51.97),
            reference_mlg_x_fractions: Some(vec![30.55 / 51.97, 30.55 / 51.97, 31.31 / 51.97]),
            // Boeing DC-10 ACAP D6-58327 (2023 reissue, p.19) sec. 2.3 Ground Clearances, Series 30/40 table,
            // item D (fuselage bottom under the forward hold; the top of the fuselage A 8.28 m less the 6.02 m diameter gives
            // the same 2.26 m): 2.26 m at nominal maximum ramp weight, 2.24 m critical minimum (7 ft 5 in / 7 ft 4 in).
            fuselage_ground_clearance_m: Some(2.26),
            // No published tail-strike attitude backs this preset's aft-fuselage
            // geometry, so its model tail-down angle is unvalidated (the generic
            // tailcone loft understates it): keep the Raymer/Roskam 15 deg floor on
            // top of the tail-down criterion.
            min_tip_back_deg: 15.0,
            // Tail-scrape requirement: no published tail-strike attitude found (ACAP D6-58327 prints none; the ~10
            // deg compressed value in NTSB-derived material is the longer MD-11's), so the 10 deg default (an
            // assumption) stands.
            trimmable_stabilizer: true,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 50.39,
            // Published reference wing: 338.84 m2 TCDS area (IM.A.210, 3,647.5 ft2), MAC 7.51 m (NASA CR-3677), taper 0.2562.
            // Drawn planform, DAC-67803A Rev A Figure 2.2 plan view (printed p. 8, PDF p. 14). The ACAP prints no chord and no area, so the
            // chords are drawing reads: native 300 dpi scan, stroke centres, on the drawing's own 5 ft grid (27.93 px/m along x,
            // printed 181 ft 7.2 in overall length to 0.1 %; 27.55 px/m along y, mean drawn semispan 0.2 % inside the printed 165 ft 4 in),
            // both halves averaged (scan leans 0.2 deg; halves differ by up to 0.04 m in chord). LE x = 19.30 + 0.795 y, inboard (Yehudi)
            // TE x = 33.22 + 0.144 y, outboard TE x = 30.17 + 0.469 y; drawn kink y = 9.40 m, +-0.06 m (mean of the two drawn corners, free of
            // the centreline row; the 9.26 / 9.55 m halves took row 997.5, the wing-LE crossing, 4 px off the TE crossing). Chords at y = 0, at
            // the kink and at the printed semispan, +-0.05 m. Closure: gross planform (with the tip rake below)
            // 369.5 m2, against 367.5 m2 from the printed aspect ratio 6.91 (Jenkinson, Simpkin and Rhodes, Civil Jet Aircraft Design, 1999,
            // Table 18.1, p. 399, "DC10", series not named) on the printed span (+0.6 %); outboard straight edges carried to the centreline
            // 341.0 m2, against the published 338.84 m2 trapezoid (+0.6 %). The built LE keeps `sweep_deg` and `root_datum_x_m` (19.344 +
            // 0.790 y), so the built straight-edge tip sits 0.08 m forward of the read lines (LE 39.25 vs 39.33 m, TE 41.91 vs 41.99 m), inside
            // the read spread. Were 13.974 / 7.611 / 2.80 m (gross 368.9 m2), 14.24 / 7.564 / 2.64 m, root 13.61 earlier.
            root_chord_m: 13.92, // Fig. 2.2: inboard lines carried to y = 0, a datum only; was 13.974
            break_chord_m: 7.81, // Fig. 2.2: chord at the kink y = 9.40 m on the outboard trailing edge; was 7.76 (y 9.55 m), 7.611
            tip_chord_m: 2.66, // Fig. 2.2: straight edges carried to the printed semispan 25.195 m; was 2.80
            // Fig. 2.2: leading-edge slope 0.790 from the fuselage side to the tip (the grid read gives 0.795 +-0.009
            // between the halves, inside that spread). NASA CR-3119 and Obert (Aerodynamic Design of Transport Aircraft, 2009, p. 207)
            // print 35 deg at quarter chord, a rounded nominal: the drawn edges and chords above build 35.3 deg outboard. Was 37.926.
            sweep_deg: 38.31,
            tip_twist_deg: -2.0,
            wing_x_shift_m: -3.25,
            tail_scale: 1.0,
            // DAC-67803A Rev A Figure 2.2: printed fuselage length 170 ft 6 in, the end of the
            // hull under the centre-engine pod, which the gear station fractions above are
            // normalized to. The printed overall 181 ft 7.2 in (55.35 m) is the pod nozzle.
            fuselage_length_m: 51.97,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        speed_schedule_source: crate::presets::SpeedScheduleSource::SourcedLadder,
        airfoil_class_source: "Design-era declaration: the DC-10 wing was designed in 1966-1968 (EIS 1971), before supercritical sections, first flown on the NASA F-8 SCW in 1971, reached transport service, so its technology is conventional: peaky sections, the basic one with very little rear loading (Obert, Aerodynamic Design of Transport Aircraft, 2009, p. 207). Its Douglas Santa Monica sections (DSMA-496, -521 and -522 at the root, -519 and -520 at the tip; Lednicer, UIUC Incomplete Guide to Airfoil Usage) are unpublished, so root and tip draw the basic wing section Obert prints (Fig. 24.23, p. 208, from AIAA paper 69-830), digitised as DC10-BASIC; the thickness stations set the t/c.",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Published reference: S 338.84 (landing_reference.rs); drawn gross S 369.5 (the Fig. 2.2 chords above),
                // outboard straight edges carried to the centreline 341.0.
                // MAC 7.51 m and LEMAC 27.253 m aft of the nose tip, from the NASA CR-3677 wind-tunnel models. The 3.25 % high-speed
                // model (Fig. 3: nose at model station 19.728 cm, moment reference c/4 of the MAC at 114.404 cm) and the 4.7 %
                // high-lift model (Fig. 29: 28.532 cm and 165.448 cm) both put c/4 of the MAC 29.131 m aft of the nose at full
                // scale (1146.9 in); MAC from the high-lift run's Reynolds number (p. 49) and scale (p. 48). LEMAC = 29.131 - 7.51 / 4.
                // Check against the Boeing DC/MD-10 ACAP DAC-67803A Rev A, Fig. 7.4.2 (printed p. 141, Series 30/30CF/40/40CF): its
                // two scales give %MAC = 46.9 - 3.04 (100 - % weight on the main gear group) (reproducing the printed worked example,
                // 91.3 % -> 20.5 %MAC, and the aft boundary, 94.3 % -> 29.5 %MAC), so the main-gear group sits at 46.9 %MAC (+-0.5).
                // With the group's load centroid 30.69 m aft of the nose (the gear stations above) that puts c/4 at 29.04 m (MAC 7.51 m)
                // to 29.10 m (the chart's own 7.26 m), 0.03-0.09 m from the tunnel models. The measured tail-off aerodynamic centre
                // of the tunnel model (CR-3677 Fig. 5, M 0.6-0.7: Cm-CL +0.005 about c/4) is 0.245 MAC. Was LEMAC 26.82 m and the
                // reference trapezoid's 7.42 m MAC: the chart applied to the gear centroid of the 8.1 m nose gear (30.30 m), 0.43 m
                // forward of the tunnel frame.
                reference_wing: crate::ReferenceWingConfig::published(338.84, Some(7.51), Some(27.253)),
                // Fig. 2.2: leading-edge line reaches y = 0 at 19.344 m, with the 3.25 m design-vector shift. The original read
                // (28.13 px/m, +-0.1 m) gave 19.594 m; the 600 dpi re-read on the stroke centres (0.44 px rms over 90 rows) puts the
                // leading edge at 19.36 m at the centreline on the nose-tip-to-tail-tip scale (21.69-21.75 m at the fuselage side,
                // against 21.97 m built) and 19.27 m on the scale of the dimension extension lines, so the wing moves 0.25 m
                // forward (the mid of 0.22-0.28 m, the nearer scale). The CR-3677 reference trapezoid (S 338.84 m2, span 50.39 m,
                // MAC 7.51 m at LEMAC 27.253 m, leading-edge slope 0.790) gives 19.265 m, 0.08 m further forward. The
                // read on the drawing's 5 ft grid (both halves averaged, see the chords) gives 19.30 m (halves 19.27 and 19.34 m),
                // 0.04 m from this value, so it stands. Was 22.844 (19.594 m); before that 23.16.
                root_datum_x_m: 22.594,
                // Fig. 2.2 front view: leading-edge heights set the built upper surface on the drawn upper line, which rises
                // slowly inboard (about 1 deg) and about 3.3 deg outboard of the nacelle. Was root -1.5, break -0.6569, tip 0.75.
                root_z_m: -1.5000,
                break_z_m: -0.677,
                tip_z_m: 0.45,
                root_twist_deg: 4.0,
                break_twist_deg: 1.5,
                break_span_fraction: 0.373, // Fig. 2.2: Yehudi trailing-edge kink y = 9.40 m; was 0.379 (y 9.55 m), 0.3747
                // Fuselage half width 3.01 m over the 25.195 m semispan; the inherited 0.10 sat 0.49 m inside the body.
                side_of_body_span_fraction: Some(0.1195),
                side_of_body_chord_ratio: None,
                kink_span_fraction: Some(0.373), // Fig. 2.2 as above; was 0.379, 0.3747
                // Fig. 2.2 plan, the half that ends on the printed span: LE rounds 0.40 m aft over the last 0.2 m, TE corner 0.12 m forward. The other half is drawn 0.3 m longer, sharp.
                // Rake tip chord = tip chord - 0.40 - 0.12 (the -0.12 edge offsets below move both edges forward): 2.66 - 0.52 = 2.14 m.
                tip_rake_start_span_fraction: Some(0.992),
                tip_rake_tip_chord_m: Some(2.14), // was 2.28 (from tip_chord_m 2.80)
                edge_stations: vec![crate::WingEdgeStation { span_fraction: 1.0, le_offset_m: -0.12, te_offset_m: -0.12 }],
                outboard_sweep_decrement_deg: 2.0,
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                // Section family: Douglas Santa Monica sections, DSMA-496,
                // -521 and -522 at the root and DSMA-519 and -520 at the tip
                // (D. Lednicer, "The Incomplete Guide to Airfoil Usage",
                // UIUC Airfoil Data Site, entry "Douglas DC-10-30"), whose
                // coordinates are not published. Obert, Aerodynamic Design of
                // Transport Aircraft (2009), p. 207, prints them as peaky
                // sections, the basic one with "very little rear loading",
                // and draws that basic section (Fig. 24.23, p. 208, from
                // AIAA 69-830). Root and tip draw it as digitised,
                // DC10-BASIC (alas-geom data/digitised_sections.txt; printed
                // t/c 10.4 %, digitised 10.1 %, thin-airfoil c_m,c/4
                // -0.013); the thickness stations below set its t/c. Was
                // the conventional-class surrogate pair (b737a root, b737d
                // outboard; c_m,c/4 -0.013 and -0.022), and before that
                // DSMA-523A at root and tip (UIUC, "McDonnell/Douglas
                // DSMA-523 transonic airfoil with sharp trailing edge"), an
                // aft-loaded section (c_m,c/4 -0.149, maximum camber near
                // 80 % chord) that put the clean wing-body Cm_ac near -0.17,
                // against -0.03 measured tail-off on the 747 of the same
                // class (NASA CR-147641, Fig. 48).
                root_airfoil: "DC10-BASIC".to_owned(),
                tip_airfoil: "DC10-BASIC".to_owned(),
                // Korn class: conventional, 0.87 (Mason, ch. 7; Malone & Mason 1995).
                airfoil_class: crate::AirfoilClass::Peaky,
                // Thickness (Obert, Aerodynamic Design of Transport Aircraft, 2009, Fig. 24.21) and dihedral (Fig. 2.2 front view)
                // stations, with their sources: `wing_stations`.
                thickness_stations: wing_stations::dc_10_thickness(),
                dihedral_stations: wing_stations::dc_10_dihedral(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                // Drawn tip edges read off the DC-10 Airplane Characteristics for Airport Planning tail views; analysis stays on
                // the trapezoid. Plan view: the drawn tip corner is rounded, trailing edge pulled forward 0.4 m at the tip (+-0.1 m).
                hstab_edge_stations: vec![
                    crate::TailEdgeStation {
                        span_fraction: 0.965,
                        le_offset_m: 0.0,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.985,
                        le_offset_m: 0.1,
                        te_offset_m: -0.15,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 1.0,
                        le_offset_m: 0.35,
                        te_offset_m: -0.4,
                    },
                ],
                vstab_edge_stations: vec![
                    crate::TailEdgeStation {
                        span_fraction: 0.97,
                        le_offset_m: 0.0,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.99,
                        le_offset_m: 0.12,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 1.0,
                        le_offset_m: 0.3,
                        te_offset_m: 0.0,
                    },
                ],
                tail_airfoil: "naca0012".to_owned(),
                // Fig. 2.2: printed tailplane span 71 ft 2 in (21.69 m). Plan view reads, root edges at the crossing of the half lines:
                // leading edge x = 42.32 + 0.829 y, +-0.07 m (grid x scale: 42.26 + 0.828 y), c/4 sweep 35.2 deg, the wing's 35 deg as Obert (Aerodynamic
                // Design of Transport Aircraft, 2009, p. 409) states for the DC-10; was 42.55 + 0.852 y, 0.23-0.48 m aft (the wing's scale), which put the built TE as far aft of the drawn one. Chords: Fig. 2.2 plan view, the read of the wing chords (5 ft grid,
                // stroke centres, both halves averaged): trailing edge x = 50.77 + 0.334 y, chord 8.45 m at y = 0 and 3.08 m at
                // the printed semispan 10.845 m (halves 8.52 / 8.39 m and 3.07 / 3.09 m), area 125.0 m2, taper 0.364, aspect ratio
                // 3.76 on the printed span. The drawing is the primary source; the published planform, area 124.30 m2 (1,338 ft2),
                // taper 0.344, aspect ratio 3.78 (Jenkinson, Simpkin and Rhodes, Civil Jet Aircraft Design, 1999, Aircraft Data
                // File (accompanying Data A), Table 6, DC10-30), is 0.6 % smaller in area. Were root 8.53 m and tip 2.93 m from that published area and taper; earlier
                // drawing reads gave 8.93 / 3.07 m (an earlier trace, 130.1 m2), 8.88 / 2.82 m (126.9 m2) and 8.41 / 2.89 m
                // (122.6 m2). The drawn tip rounds off about 0.25 m inside the printed span. Front view: upper line 1.83 m at 0.55 and 2.27 m at 0.85 semispan. Were offset 13.63, chords 8.93 and 3.07, tip (9.39, 10.845, 1.55), z 0.87: 0.8 m forward and 0.3 m high.
                hstab_offset_from_tail_m: 9.65, // Root leading edge 42.32 m aft of the nose; was 9.42 (42.55 m), 12.80 from the 55.35 m overall length
                hstab_z_m: 0.40,
                hstab_root_chord_m: 8.45,
                hstab_tip_chord_m: 3.08,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (8.99, 10.845, 1.85), // tip LE 51.31 m; was 9.24
                // Fin, Fig. 2.2 side view (+-0.1 m): tip 12.4 m above the cabin axis, tip leading edge 50.74 m, tip trailing edge
                // 53.54 m; edges carried at their drawn slopes down to the root line z = 3.1 m through the centre-engine pod. The built fin
                // stands on the pod (the builder seats it on `engine.centre_engine`); the reference planform stays carried to the fuselage.
                // Were offset 13.52, chords 8.46 and 2.84, tip x 8.87.
                vstab_offset_from_tail_m: 10.53, // Plan and side views: fin root edges 0.25 m forward of the former position (tip fixed at 50.74 m); was 13.91 from the 55.35 m overall length
                vstab_z_m: 3.1,
                vstab_root_chord_m: 8.40,
                vstab_tip_chord_m: 2.80,
                vstab_tip_le_m: (9.30, 0.0, 9.3),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 6.02,
                nose_z_m: 0.53,
                cabin_start_x_m: 5.5,
                cabin_z_m: 0.2,
                tailcone_length_m: 8.12, // Cabin end held at 43.85 m; was 11.5 from the 55.35 m overall length
                tail_z_m: 1.6,
                ..outer_shape::dc_10()
            },
            engine: EngineConfig {
                // engines.json carries a V2 nacelle set (nacelle_shape_v2) that agrees with the measured length, diameter and inlet.
                nacelle_model: crate::NacelleModel::V2,
                engine_name: "CF6-50".to_owned(),
                // The third position is the centreline engine (No. 2), set by
                // `centre_engine` below, not by the underwing pair's `z_m` and
                // inlet offset. The underwing pair sits 26 ft 10 in (8.18 m)
                // from the centreline, DAC-67803A Rev A Figure 2.2 plan view.
                spanwise_positions_m: vec![8.18, -8.18, 0.0],
                z_m: -1.872, // Fig. 2.2 side view upper line and front view height 2.97 m; lower line then 1.02 m above ground, inside the printed 0.86 to 1.09 m of Section 2.3 clearance N; was -2.065
                // Fig. 2.2: inlet face 20.3 m in plan and 19.8 m in side view (the views disagree), held at 20.2 m, nearer the plan; was 6.25.
                inlet_x_offset_m: 5.85,
                // No. 2 engine, Series 30 Figure 2.2: a straight-through pod,
                // not an S-duct (only the outline is drawn, and it has no
                // bend): upper and lower lines parallel, both falling 0.8 m
                // over 12 m (axis inclined 4 deg), rounded inlet face at 41.0 m,
                // aft bulkhead at 52.3 m where the outline steps in, tail cone to
                // the printed overall length, 3.38 m aft of the hull end. 2.8 m wide
                // (plan) by 3.0 m high (side), top 6.3 m at the face; struts carry
                // the pod over the crown and the fin stands on the pod, with a
                // fixed fairing behind it over the tail cone.
                centre_engine: Some(crate::CentreEngineInstallation {
                    axis_z_m: 4.85,
                    axis_pitch_deg: 4.0,
                    width_m: 2.80, // Fig. 2.2 plan and front views
                    height_m: 3.0,
                    // Fig. 2.2 side view, plan view for the width behind the step: inlet face 2.0 m tall with corners of about 0.5 m radius,
                    // step at 52.3 m to 2.05 m wide by 2.0 m tall, straight cone to 0.8 m at 54.46 m, nozzle piece to the end. Was (0.0, 0.93),
                    // (0.35, 0.99), (0.8, 1.0), (11.25, 1.0), (13.9, 0.25) from an inlet face at 41.45 m, the mean of a skewed plan view and the side view.
                    profile: vec![
                        (0.0, 0.66),
                        (0.05, 0.76),
                        (0.15, 0.85),
                        (0.3, 0.92),
                        (0.5, 0.96),
                        (1.0, 0.985),
                        (2.0, 1.0),
                        (7.0, 1.0),
                        (11.25, 0.94),
                        (11.29, 0.70),
                        (13.46, 0.28),
                        (14.1, 0.22),
                        (14.35, 0.01),
                    ],
                    nozzle_aft_of_fuselage_m: 3.38,
                    // Fig. 2.2 side view: struts at 42.4 and 49.7 m, about 0.28 m chord at the pod and 0.15 m at the crown (+-0.1 m, the ink is 0.1 m wide).
                    struts: vec![
                        crate::CentrePodStrut {
                            station_m: 1.4,
                            chord_at_pod_m: 0.28,
                            chord_at_fuselage_m: 0.15,
                        },
                        crate::CentrePodStrut {
                            station_m: 8.7,
                            chord_at_pod_m: 0.28,
                            chord_at_fuselage_m: 0.15,
                        },
                    ],
                    // Fig. 2.2: the line from the rudder base runs on to 54.75 m, then a vertical edge down to the nozzle; plan view a thin spine.
                    fin_aft_fairing: Some(crate::FinAftFairing {
                        top_start_z_m: 6.33,
                        top_end_station_m: 13.75,
                        top_end_z_m: 5.88,
                    }),
                }),
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.82,
            cruise_altitude_m: 10668.0,
            mtow_kg: 259_454.0,
            max_wing_area_m2: 338.84,
            min_wing_loading_kg_m2: 500.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 255,
            cargo_payload_kg: 65_000.0,
            // Structural payload = MZFW 166,922 kg less the registry OEW basis 120,914 kg
            // (ACAP section 2.1, Series 30 passenger: 101,809 lb less the 379 lb the
            // 572,000 lb option adds to OEW, 101,430 lb).
            max_structural_payload_kg: 46_008.0,
            // Superseded at registration: the profile declares the TCDS VMO/MMO, so VD is the
            // CS 25.335(b) value on VC = VMO (`crate::design_speeds`, `registry`).
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("advanced_highlift_widebody"),
    }
}
