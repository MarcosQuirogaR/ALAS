// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airbus twin-aisle presets: the A340-300 here, the A350-900 and A380-800 in their own
//! modules.

mod a350;
mod a380;

pub use a350::a350_900;
pub use a380::a380_800;

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MissingDesignMissionDatum, MissionEvidenceApplicability,
    PartialDesignMissionEvidence, PartialMissionEvidenceKind, WingConfig,
};

/// Long-range quad with a conventional tail.
pub fn a340_300() -> AircraftPreset {
    AircraftPreset {
        name: "A340-300",
        display_name: "Airbus A340-300",
        class: crate::presets::AircraftClass::Transport,
        description: "Airbus A340-312 WV029 with CFM56-5C3-family engines.",
        identity: AircraftVariantIdentity {
            model: "A340-312",
            weight_variant: "WV029",
            engine_model: "CFM56-5C3/F",
            modification_state: "public WV029 planning baseline",
            tank_configuration: "three tanks",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Vol. I Table 1-1 (aerodrome reference code) applied to the
            // preset wingspan of 60.30 m (Airbus A340-300 Aircraft Characteristics):
            // 52 m <= b < 65 m is code E.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::E),
            mrw_kg: Some(260_900.0),
            mtow_kg: Some(260_000.0),
            mlw_kg: Some(188_000.0),
            mzfw_kg: Some(178_000.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("A340-300"),
            // Airbus A340-200/-300 Aircraft Characteristics, Rev Dec 01/25, sec. 2-1-1 p.4: usable fuel
            // 140,640 L = 110,402 kg at the page's own density 0.785 kg/L.
            usable_fuel_volume_l: Some(140_640.0),
            usable_fuel_mass_kg: Some(110_402.0),
            fuel_density_kg_l: Some(0.785),
            planning_seats: Some(335),
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: None,
                cruise: None,
                range_nmi: 5_000.0,
                payload_kg: Some(50_800.0),
                source: "Airbus A340-200/-300 Aircraft Characteristics Rev 33 (2025-12) section 3-2-1 Figure 3-2-1-991-013-A01 (payload/range ISA, CFM56-5C3, A340-300): maximum-payload corner read as 5,000 nmi at 112,000 lb (50.8 t); read uncertainty +-100 nmi, +-1,000 lb; chart MTOW not printed; reserves not stated",
                reserve: None,
            }),
            certified_max_seats: Some(375),
            certified_exit_layout: Some(crate::presets::A340_300_EXIT_LAYOUT),
            partial_design_mission_evidence: vec![PartialDesignMissionEvidence {
                kind: PartialMissionEvidenceKind::PayloadRangeChart,
                range: None,
                payload_kg: None,
                load_case: None,
                profile_assumptions: Some("ISA cruise at 39,000 ft and Mach 0.82 only"),
                reserve_assumptions: None,
                reserve_contract: None,
                applicability: "A340-300 CFM56-5C3 chart matches the model and engine family but selects no weight-variant design point",
                configuration_applicability: MissionEvidenceApplicability::ModelAndEngineFamily,
                missing: vec![
                    MissingDesignMissionDatum::Range,
                    MissingDesignMissionDatum::Payload,
                    MissingDesignMissionDatum::Profile,
                    MissingDesignMissionDatum::ReserveFuel,
                ],
                source: "Airbus A340-200/-300 Aircraft Characteristics Rev 33, 2025-12-01, section 3-2-1 p.4, Figure 3-2-1-991-013-A01",
            }],
            // Design mission, one provenance row per datum: the maximum-structural-payload corner
            // of the CFM56-5C3 chart, cruise printed in its legend. No reserve basis is printed.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::Manufacturer,
                    stated_conditions: Some("cruise conditions ISA, 39,000 ft, M = 0.82, at maximum take-off weight (the chart does not print the MTOW value); maximum-structural-payload corner read as 5,000 nmi (vector read 4,990 nmi); no reserve basis printed"),
                    relative_uncertainty: Some(100.0 / 5_000.0),
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "Airbus A340-200/-300 Aircraft Characteristics Rev Dec 01/25, section 3-2-1 p.4, Figure 3-2-1-991-013-A01 (A340-300, CFM56-5C3)",
                }),
                payload: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "maximum structural payload plateau 112,000 lb (50.8 t; vector read 111.8 klb) at the corner range",
                    "Airbus A340-200/-300 Aircraft Characteristics Rev Dec 01/25, section 3-2-1 Figure 3-2-1-991-013-A01",
                )),
                profile: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "ISA, cruise M = 0.82 at 39,000 ft; no climb or descent schedule is printed, so the model's configured schedules apply to those phases",
                    "Airbus A340-200/-300 Aircraft Characteristics Rev Dec 01/25, section 3-2-1 Figure 3-2-1-991-013-A01 legend 'CRUISE CONDITIONS ISA 39000 ft M = 0.82'",
                )),
                reserve_fuel: Some(crate::MissionDatumProvenance::regulatory_reserve(
                    "reference chart prints no reserve basis; the design mission applies the regulatory policy: contingency, alternate and 30 min final reserve fuel, at the alternate distance and holding height of the preset's configured fuel policy. The range comparison carries this reserve-basis uncertainty",
                )),
            },
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            aft_cg_nose_load: Some(crate::presets::gear_load::A340_300),
            reference_wing_area_m2: Some(361.6),
            sources: vec![
                "EASA.A.015 Issue 28, 2026-01-15, pp.32-36",
                "Airbus A340-200/-300 Aircraft Characteristics Rev 33, 2025-12-01, section 2-1-1",
            ],
            horizontal_tail_ice_protection: Some(crate::presets::tail_ice::A340_300),
            operating_speed_limits: Some(crate::presets::operating_limits::A340_300),
            high_speed_protection: Some(
                crate::presets::operating_limits::A340_HIGH_SPEED_PROTECTION,
            ),
            ..AircraftReferenceData::default()
        },
        engine_name: "CFM56-5C3/F",
        n_engines: 4,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 3,
            // AC 2-9-0: two 4-wheel wing bogies + 1 twin-wheel centreline gear.
            mlg_strut_bogie_wheels: Some(vec![4, 4, 2]),
            wheels_per_mlg_strut: 0,
            // Main tire: Airbus A340-200/-300 AC sec. 7-2-0 prints 1400x530R23; Michelin application chart printed p.63 gives 36 PR from 260.9 t ramp weight; rating from printed p.49.
            mlg_tire: "1400x530R23/36PR".to_owned(),
            nlg_tire: "1050x395R16/28PR".to_owned(),
            track_diameter_factor: 10.684 / 5.64,
            // AC 7-2-0 wheelbase/track; AC 2-2-0 nose-tip stations normalized.
            reference_wheelbase_m: Some(25.375),
            reference_track_m: Some(10.684),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(62.84),
            reference_nlg_x_fraction: Some(6.67 / 62.84),
            reference_mlg_x_fractions: Some(vec![
                32.05 / 62.84,
                32.05 / 62.84,
                33.04 / 62.84,
            ]),
            // Airbus A340-200/-300 AC Rev 33, Figure 2-3-0-991-005-A01 (PDF
            // p.44, ground clearances, aft CG): fuselage bottom ahead of the
            // wing (F2) 2.13 m; the 1.83 m BF is the belly fairing.
            fuselage_ground_clearance_m: Some(2.13),
            trimmable_stabilizer: true,
            // THS and elevator travel from the FAA type certificate data sheet A43NM (Rev 14,
            // p.7, A340-200/-300 "Control Surface Movements"): stabiliser +2/-14 deg,
            // elevator +15/-30 deg. The Airbus sign convention is positive = trailing edge
            // down (A380 sheet A58NM Rev 11 p.5 prints the THS nose-up limit as -10),
            // so the nose-up stop is 14 deg (nose-down 2 deg) and the elevator up travel
            // 30 deg (down 15 deg). The A330 sheet (A46NM Rev 42 p.35) prints the same.
            stabilizer_nose_up_travel_deg: Some(14.0),
            elevator_up_travel_deg: Some(30.0),
            // Published tail-strike attitude: 10.1 deg pitch to ground contact with
            // the main gear compressed (Airbus, "Avoiding Tail Strike", Operational
            // Liaison Meeting, FBW; NTSB docket attachment "Airbus Material -
            // Avoiding Tail Strike", PDF p.12). The generic tailcone loft
            // understates the geometric tail-down angle, so it is the floor.
            min_tip_back_deg: 10.1,
            // The rotation attitude the built fuselage must reach without touching is
            // the published gear-compressed tail-strike attitude above (10.1 deg),
            // compared with the model contour on its static ground plane.
            required_rotation_angle_deg: 10.1,
            required_rotation_basis: crate::landing_gear::RotationAngleBasis::PublishedStatic,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 60.30,
            // Planform from the vector silhouette of Airbus A340-200/-300 Aircraft Characteristics Rev 33 (Dec 01/25), FIGURE-2-2-0-991-007-A01
            // sheet 2 (63.69 m over 343.2 pt). Root chord: datum, inboard TE line (33.495 m at y 3.31, slope 0.098) carried to y = 0.
            root_chord_m: 12.08,
            // Kink at the TE break y = 9.25 m: LE 26.87 m, TE 34.275 m.
            break_chord_m: 7.408,
            // Winglet root: sheet 2 prints 2.5 m from the 39.1 m LE station.
            tip_chord_m: 2.5,
            // Sheet 2: one straight LE from y = 4.6 m to 28.8 m, slope 0.6247 (32.0 deg), no inboard crank. Airbus prints no sweep.
            sweep_deg: 32.0,
            tip_twist_deg: -2.0,
            wing_x_shift_m: -1.4,
            tail_scale: 1.0,
            // AC 2-2-0: 62.84 m is the printed body length to the tail-cone end; the published 63.66 m runs to the fin and HTP tip trailing points.
            fuselage_length_m: 62.84,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        speed_schedule_source: crate::presets::SpeedScheduleSource::SourcedLadder,
        airfoil_class_source: "Design-era declaration: the A340-300 (EIS 1993) shares the A330 wing, of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC(2)-0612 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values; the drawn planform stays the geometry.
                // S 361.6 m2: Airbus A340-200/-300 Aircraft Characteristics section 2-1-1 and Jane's 2008-09 (gross wing 361.60 m2); MAC 7.270 m: EASA TCDS (as recorded below);
                // LEMAC 28.083 m: section 7 pavement-load two-point statics (a derived value, 13 rows within +-0.014 m).
                // Recheck: the Figure 7-3-0-991-007-A01 sheet 2 most-aft-CG rows (WV023 to WV027, MRW, wing gear per strut
                // and centre gear, nose = MRW less both) on the Figure 2-2-0-991-007-A01 sheet 1 gear stations (NLG 6.67, MLG 32.05,
                // CLG 33.04 m) give 28.093 to 28.095 m. Open item: the drawn planform below puts its area-weighted LEMAC
                // at 27.78 m (built), 0.30 m ahead of this value, and no printed sheet 2 dimension moves the wing aft (its printed
                // 39.1 m winglet-root LE is 0.07 m ahead of the drawn line), so neither the drawing nor the statics value is moved.
                reference_wing: crate::ReferenceWingConfig::published(361.6, Some(7.27), Some(28.083)),
                // Sheet 2: the straight LE extended to y = 0 is 21.087 m aft of the nose (with wing_x_shift_m).
                root_datum_x_m: 22.487,
                // Ground shape: the sheet 1 front-view line (the flight shape below) less a tip rise linear in span, so the wing stays
                // on the body at the centreline. Tip from the Figure 2-3-0-991-005-A01 static clearances (aft CG): lower surface
                // 5.94 m above the ground (W2). The nacelle low points N1 1.28 m and N2 2.35 m take one offset per engine pair.
                root_z_m: -1.2850,
                break_z_m: -0.2909, // ground shape at the 9.25 m kink
                tip_z_m: 1.2296, // W2 clearance, now at the real tip (winglet root)
                root_twist_deg: 3.5,
                break_twist_deg: 1.5,
                break_span_fraction: 0.35,
                // Sheet 2 prints 10.5 m from the LE line to the TE at the fuselage side (drawn 22.82 to 33.50 m); ratio gives 10.56 m on the 2.9 m SoB line.
                side_of_body_chord_ratio: Some(0.874),
                kink_span_fraction: Some(9.25 / 28.95),
                // Sheet 2: TE slope 0.350 from the kink to y = 19.66 m, 0.402 from there to the tip; offset from the straight kink-to-tip chord.
                te_break_span_fraction: Some(19.66 / 28.95),
                te_break_chord_offset_m: Some(-0.268),
                // Sheet 2 silhouette: wing-body LE fillet from 21.79 m at the side of body back on the LE line by y = 4.6 m; inboard TE slope changes at y = 7.6 m.
                edge_stations: vec![
                    crate::WingEdgeStation { span_fraction: 0.10, le_offset_m: -1.106, te_offset_m: 0.0 },
                    crate::WingEdgeStation { span_fraction: 0.1155, le_offset_m: -0.475, te_offset_m: 0.0 },
                    crate::WingEdgeStation { span_fraction: 0.1385, le_offset_m: -0.05, te_offset_m: 0.0 },
                    // Sheet 2: the fillet is back on the straight LE line by y = 4.6 m (a reading of the line only to y = 7.6 m
                    // left the -0.05 m reading as the start of the longest straight run and tilted the wing line to 32.56 deg).
                    crate::WingEdgeStation { span_fraction: 4.6 / 28.95, le_offset_m: 0.0, te_offset_m: 0.0 },
                ],
                outboard_sweep_decrement_deg: 2.0,
                root_airfoil: "sc20612".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                airfoil_class: crate::AirfoilClass::Supercritical,
                                // Thickness profile: the NASA Common Research Model t/c 0.154 at the
                // centreline, 0.138 at the side of body, 0.105 at the yehudi break and 0.095 at the tip
                // (Vassberg, DeHaan, Rivers and Wahls, AIAA 2008-6919, Table 2) is the public transport-wing
                // distribution; the multipliers below act on the root-kink-tip section blend above.
                thickness_stations: vec![
                    crate::WingThicknessStation {
                        span_fraction: 0.0000,
                        thickness_scale: 1.2486, // t/c 0.1200 -> 0.1498
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.1000,
                        thickness_scale: 1.1189, // t/c 0.1200 -> 0.1342
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.3195, // the kink
                        thickness_scale: 0.8513, // t/c 0.1200 -> 0.1021
                    },
                    crate::WingThicknessStation {
                        span_fraction: 1.0000,
                        thickness_scale: 0.9278, // t/c 0.0996 -> 0.0924
                    },
                ],
                // Winglet of sheet 2 and sheet 1 (front view): root y = 28.95 m, tip y = 30.15 m (the printed span), tip LE 41.57 m, TE 42.22 m printed.
                wingtip_device: Some(crate::WingtipDevice {
                    kind: crate::WingtipDeviceKind::Winglet,
                    height_m: 1.452,
                    down_height_m: 0.0,
                    cant_deg: 39.57,
                    root_chord_m: None,
                    root_te_flare: None,
                    tip_chord_m: 0.65,
                    le_sweep_deg: 51.79,
                    toe_out_deg: 0.0,
                    blend_radius_m: 0.0,
                    arc_start_deg: None,
                    path_stations: Vec::new(),
                    airfoil: None,
                    span_includes_device: true,
                    include_in_vlm: true,
                }),
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                // Flight shape: the upper surface on the straight line of the sheet 1 front view (silhouette from y = 3.68 m to 28.87 m without a break, 4.40 deg).
                // The drawn and tabulated nacelle heights are the static ones: keep them in the flight shape.
                nacelles_hold_ground_height: true,
                flight_tip_rise_semispan_fraction: Some(0.0069), // the sheet 1 front-view upper line, 4.37 deg (0.0092 read 4.51 deg, 0.06 m high at y 28 m); the only rise of this wing, linear in span
                dihedral_stations: vec![
                    crate::WingDihedralStation { span_fraction: 0.10, z_m: -0.9589, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.20, z_m: -0.6319, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.50, z_m: 0.1118, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.65, z_m: 0.4482, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.80, z_m: 0.7789, flight_rise_m: None },
                ],
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0010".to_owned(), // sheet 1 front view over the plan and side chords: HTP t/c 0.10, fin 0.105
                // Figure 2-2-0-991-007-A01 sheet 2 (plan view): tailplane tip
                // leading edge 61.67 m aft of the nose and 6.43 m aft of the
                // root leading edge (55.24 m), tip trailing edge 63.69 m. The
                // root chord and height are not dimensioned.
                // Tail fit: tailplane straight-edge fit to the drawing, centreline values.
                hstab_offset_from_tail_m: 7.58,
                hstab_z_m: 1.46, // sheet 1 side and front views: side lower line and both front surfaces at the 10 % section
                hstab_root_chord_m: 5.36,
                hstab_tip_chord_m: 2.0,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                // Airbus A340 Aircraft Characteristics, general dimensions:
                // 19.4 m full horizontal-tail span.
                hstab_tip_le_m: (6.43, 9.7, 1.080), // tailplane dihedral matched to the front view; was z 0.8
                // Geometry round 2: rounded tip leading-edge corner read off sheet 2 (plan view); offsets aft of the straight LE.
                hstab_edge_stations: vec![
                    crate::TailEdgeStation { span_fraction: 0.91, le_offset_m: 0.0, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.944, le_offset_m: 0.14, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.97, le_offset_m: 0.42, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.985, le_offset_m: 0.85, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 1.0, le_offset_m: 1.15, te_offset_m: 0.0 },
                ],
                // Sheet 1 (side view, drawn at about 0.63 deg nose down; x and z on their own printed dimensions): fin root LE 52.42 m, root chord 7.78 m,
                // tip LE 8.09 m aft of the 52.47 m root, tip chord 3.1 m (60.56 to 63.66 m), 8.3 m of fin above the crown. Silhouette path: LE slope 0.99, TE slope 0.40,
                // both on the lines below; the builder carries the edges on down to the tail cone.
                vstab_offset_from_tail_m: 10.37,
                vstab_z_m: 3.02,
                vstab_root_chord_m: 7.79,
                vstab_tip_chord_m: 3.10, // printed, sheet 1
                vstab_tip_le_m: (8.09, 0.0, 8.30), // printed tip LE 60.56 m, 63.66 m at the trailing point
                // Sheet 1 dorsal fillet: leaves the crown at x 49.9 m, joins the fin LE about z 4.85 m.
                // Sheet 1 fill outline: rounded tip leading-edge corner.
                vstab_edge_stations: vec![
                    crate::TailEdgeStation { span_fraction: 0.96, le_offset_m: 0.0, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.98, le_offset_m: 0.1, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.99, le_offset_m: 0.25, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 1.0, le_offset_m: 0.55, te_offset_m: 0.0 },
                ],
                vstab_dorsal_length_m: Some(2.57),
                vstab_dorsal_height_m: Some(1.83),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 5.64,
                nose_z_m: 0.25,
                cabin_start_x_m: 5.5,
                cabin_z_m: 0.2,
                tailcone_length_m: 11.18,
                tail_z_m: 1.5,
                // FUSELAGE-G2 shaped nose and aft body. Outline traced from the manufacturer drawing
                // (side: same 2-2-0 p2; top: Airbus A340-200/-300 AC (Dec 01/25) 2-2-0 p3),
                // calibrated to the drawn length; the nose law (windshield, radome, keel and plan
                // exponents) is a least-squares fit to the traced upper, lower and plan lines (RMS 0.087 m),
                // the aft law (belly ramp, crown drop, plan taper, closing section) to the traced aft lines
                // (RMS 0.124 m). Cabin start, tailcone length and every payload and mass station are unchanged:
                // only the outer loft moves. Section exponent 2.0 (ellipse): no drawing measures it.
                // Data quality: aft crown read by eye (fin root hides it).
                nose_length_m: Some(10.53),
                nose_windshield_angle_deg: Some(22.3),
                nose_crown_end_fraction: Some(0.87),
                nose_radome_length_fraction: Some(0.1),
                nose_keel_exponent: Some(2.81),
                nose_plan_exponent: Some(1.43),
                nose_section_exponent: Some(2.0),
                belly_upsweep_length_m: Some(18.69),
                aft_belly_exponent: Some(1.23),
                tail_end_z_m: Some(2.54),
                tail_end_width_m: Some(0.22),
                tail_end_height_m: Some(0.58),
                aft_plan_length_m: Some(17.59),
                aft_plan_exponent: Some(1.26),
                aft_crown_length_m: Some(7.45),
                aft_crown_exponent: Some(1.0),
                // Cabin section: Airbus AC A340-200/-300 (Dec 2025) fig. 2-5-0-991-002 (vector): floor 2.90 m, hold ceiling 3.27 m below the crown, 1.71 m high, 3.18 m floor, 3.98 m between walls, 0.33 m chamfer; cabin-datum-evidence.md.
                cabin_section: Some(crate::CabinSectionConfig { cabin_widths: Some(crate::CabinWidthsConfig { armrest_m: 5.161, shoulder_m: 4.938 }), floor_below_crown_m: Some(2.9), hold: Some(crate::HoldSectionConfig { ceiling_below_crown_m: Some(3.27), height_m: 1.71, floor_width_m: 3.18, side_width_m: 3.98, chamfer_height_m: 0.33 }) }),
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // engines.json carries a V2 nacelle set (nacelle_shape_v2) that agrees with the measured length, diameter and inlet.
                nacelle_model: crate::NacelleModel::V2,
                // Figure 2-2-0-991-007-A01 sheet 1 (front view): engine
                // centrelines 18.74 m and 38.54 m apart.
                spanwise_positions_m: vec![9.37, -9.37, 19.27, -19.27],
                // Figure 2-3-0-991-005-A01 (aft CG): nacelle low points N1
                // 1.28 m and N2 2.35 m above the ground under the 1.1 m
                // nacelle radius.
                // Low point held with the drawn 1.175 m radius (nacelle change: -1.436 ->
                // -1.361), plus the wing change's re-fit (-1.436 -> -1.3492, +0.0868 m).
                z_m: -1.2742,
                z_per_engine_m: vec![-1.9794, -1.9794, -1.6735, -1.6735], // low points on N1 1.28 m and N2 2.35 m of Figure 2-3-0-991-005-A01 (aft CG). The sheet 1 front view stands the whole aircraft about 0.14 m lower on its ground line (its fuselage crown too), so it does not set this height; z_m above is what the propulsion installation reads
                // Sheet 2 inlets 22.39 m and 28.96 m aft of the nose; the mean
                // of the two offsets from the leading edge leaves each inlet
                // within 0.19 m.
                inlet_x_offset_m: 4.36, // mean of the pairs below
                inlet_x_per_engine_m: vec![4.55, 4.55, 4.17, 4.17], // sheet 2 inlets (printed 22.39 m and 28.96 m) against the 32.0 deg LE
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.82,
            cruise_altitude_m: 11887.2,
            mtow_kg: 260_000.0,
            max_wing_area_m2: 370.0,
            min_wing_loading_kg_m2: 500.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 335,
            cargo_payload_kg: 45_000.0,
            // Maximum zero-fuel weight 178.0 t less an operating empty weight
            // of about 129.4 t.
            max_structural_payload_kg: 48_600.0,
            // Superseded at registration: the profile declares the TCDS VMO/MMO, so VD is the
            // CS 25.335(b) value on VC = VMO (`crate::design_speeds`, `registry`).
            dive_speed_m_s: 200.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("advanced_highlift_widebody"),
    }
}
