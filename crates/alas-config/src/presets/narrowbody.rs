// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/presets.py (the single-aisle entries)

//! Two published single-aisle types, which is where the global assumptions
//! stop fitting.
//!
//! The rest of this crate is calibrated around a modern twin-aisle transport,
//! and neither of these is one. Two corrections follow from that, and both are
//! stated on the presets rather than left to whoever runs them.
//!
//! Mass first. Structural, systems and furnishings mass does not scale
//! linearly with takeoff weight, so the widebody-calibrated Torenbeek
//! fractions under-predict a small aircraft's operating empty weight, by
//! about 2.8 t on the A220-300, which is most of a revenue payload's worth of
//! error in the wrong direction. Its entry carries fractions of its own.
//!
//! Speeds second. Both types have full-span slats and slotted Fowler flaps,
//! which is a materially better high-lift system than the generic
//! "standard narrow-body" bucket describes; scored with the generic one their
//! rotation and takeoff-safety speeds come out fifteen to twenty knots high,
//! which sizes them out of runways they operate from every day.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MissingDesignMissionDatum, MissionEvidenceApplicability,
    PartialDesignMissionEvidence, PartialMissionEvidenceKind, PublishedMissionLoadCase, WingConfig,
};

mod a220;
mod c919;
mod e195_e2;
mod exit_layouts;

pub use a220::a220_300;
pub use c919::c919;
pub use e195_e2::{e195_e2, e195_e2_hold_compartments};
use exit_layouts::A320_200_CERTIFIED_EXIT_LAYOUT;

/// Short and medium-range twin, the reference single-aisle.
pub fn a320_200() -> AircraftPreset {
    AircraftPreset {
        name: "A320-200",
        display_name: "Airbus A320-200",
        description: "Airbus A320-214 WV017 with CFM56-5B4/3 engines and sharklets.",
        identity: AircraftVariantIdentity {
            model: "A320-214",
            weight_variant: "WV017",
            engine_model: "CFM56-5B4/3",
            modification_state: "MOD160500 sharklets; MOD37147 Tech Insertion",
            tank_configuration: "three tanks; MOD37331 + MOD160001",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Vol. I Table 1-1 (aerodrome reference code) applied to the
            // published sharklet wingspan of 35.80 m (Airbus A320 Aircraft Characteristics,
            // section 2-2-0): 24 m <= b < 36 m is code C.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::C),
            mrw_kg: Some(78_400.0),
            mtow_kg: Some(78_000.0),
            mlw_kg: Some(66_000.0),
            mzfw_kg: Some(62_500.0),
            // No configuration-matched OEW is published for this aircraft;
            // the registry (`crate::oew_reference`) records the anchors.
            oew_kg: crate::oew_reference::preset_reference_oew_kg("A320-200"),
            usable_fuel_volume_l: Some(24_167.0),
            usable_fuel_mass_kg: Some(19_334.0),
            fuel_density_kg_l: Some(0.8),
            // Airbus A320 Aircraft Characteristics for Airport Planning, Rev 46, section 2-4-1 (typical two-class cabin).
            planning_seats: Some(150),
            design_point: Some(crate::PayloadRangeDesignPoint {
                range_nmi: 2_120.0,
                payload_kg: Some(19_700.0),
                source: "Airbus A320 Aircraft Characteristics Jun 2024, section 3-2-1 Figure 3-2-1-991-016-A01 (payload/range ISA, A320-200, 78,000 kg curve, no ACT), maximum-payload corner read as 2,120 nmi at 19.7 t; read uncertainty +-40 nmi, +-0.3 t; reserves not stated on the figure",
            }),
            certified_max_seats: Some(180),
            certified_exit_layout: Some(A320_200_CERTIFIED_EXIT_LAYOUT),
            partial_design_mission_evidence: vec![PartialDesignMissionEvidence {
                kind: PartialMissionEvidenceKind::PayloadRangeChart,
                range: None,
                payload_kg: None,
                load_case: Some(PublishedMissionLoadCase::TakeoffMassesKg(vec![
                    73_500.0, 78_000.0,
                ])),
                profile_assumptions: Some("ISA conditions only"),
                reserve_assumptions: None,
                reserve_contract: None,
                applicability: "A320-200 sharklet chart includes 73,500 kg and 78,000 kg base/one-ACT curves; Airbus marks the curves informational and selects no WV017 design point",
                configuration_applicability: MissionEvidenceApplicability::ModelOnly,
                missing: vec![
                    MissingDesignMissionDatum::Range,
                    MissingDesignMissionDatum::Payload,
                    MissingDesignMissionDatum::Profile,
                    MissingDesignMissionDatum::ReserveFuel,
                ],
                source: "Airbus A320 Aircraft Characteristics Rev 46, 2026-07-01, section 3-2-1 p.3, Figure 3-2-1-991-017-A01",
            }],
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            aft_cg_nose_load: Some(crate::presets::gear_load::A320_200),
            reference_wing_area_m2: Some(122.6),
            sources: vec![
                "Airbus A320 Aircraft Characteristics Rev 46, 2026-07-01, section 2-1-1 p.2",
                "Airbus A320 Aircraft Characteristics, Jun 01/24, section 2-2-0 Figure 2-2-0-991-004-A01 (sheets 1-2, wing tip fence: 34.10 m span; sheets 3-4, sharklet: 35.80 m span over the sharklets, 37.57 m length, 3.95 m body width, 12.45 m tailplane span, 5.87 m fin height, 6.07 m wing root chord at the body including the leading-edge fillet, 1.64 m chord at the 16.29 m aileron-end station; sheet 2 draws the 1.50 m chord at the 17.05 m wing tip; plan view read for a 27.1 deg straight leading edge, an unswept inboard trailing edge and the trailing-edge kink 6.42-6.46 m from the centreline; its 16.29 m dimension is spanwise, from the centreline to the trailing edge at the outboard end of the aileron, not a nose-to-MAC station)",
                "Airbus A320 Aircraft Characteristics, Jun 01/24, section 2-3-0 Figure 2-3-0-991-029-A01 sheet 2 (sharklet ground clearances at MRW 78,400 kg, aft CG 36.8 %MAC: sharklet bottom W2 4.009 m, fuselage bottom aft F2 1.792 m, CFM56-5B nacelle low point N1 0.577 m)",
                "Airbus A320 Aircraft Characteristics, Jun 01/24, section 7-3-0 Figure 7-3-0-991-010-A01 (static nose-gear loads at 17% MAC and main-gear loads at 38.7-43% MAC per weight variant; two-point statics with NLG 5.07 m and wheelbase 12.64 m place the leading edge of MAC 15.24-15.33 m aft of the nose)",
                "Airbus A320 Aircraft Characteristics, section 2-3-0 ground-clearance figures (45,000 kg empty weight for maintenance; 17% / 36.8% MAC CG conditions; no OEW)",
                "EASA.A.064 Issue 62, pp.37-48",
                "EASA.A.064 Issue 12, section 1 items 15-16 (datum 2.540 m forward of nose; MAC 4.1935 m)",
                "EASA.E.003 Issue 06, pp.10-11",
            ],
            ..AircraftReferenceData::default()
        },
        engine_name: "CFM56-5B4/3",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            wheels_per_mlg_strut: 2,
            track_diameter_factor: 7.59 / 3.95,
            // EASA A.064 gives a 12.64 m NLG-to-MLG wheelbase and 7.59 m
            // main-gear track. The Airbus aircraft-characteristics drawing
            // also provides nose-tip-referenced stations; they are stored as
            // normalized geometry anchors so shrink/optimization scales them
            // with the active fuselage instead of freezing absolute metres.
            reference_wheelbase_m: Some(12.64),
            reference_track_m: Some(7.59),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(37.57),
            reference_nlg_x_fraction: Some(5.07 / 37.57),
            reference_mlg_x_fractions: Some(vec![17.71 / 37.57, 17.71 / 37.57]),
            // Airbus A320 AC Jun 01/24, Figure 2-3-0-991-004-A01 sheet 2:
            // fuselage "bottom aft" (F2) 1.792 m above ground at MRW 78.4 t,
            // aft CG 36.8 %MAC (1.762-1.843 m across the tabulated states).
            fuselage_ground_clearance_m: Some(1.79),
            takeoff_stabilizer_nose_up_deg: Some(crate::landing_gear::TRANSPORT_THS_TAKEOFF_NOSE_UP_DEG),
            // Published tail-strike attitude: 11.7 deg pitch to ground contact with
            // the main gear compressed (Airbus, "Avoiding Tail Strike", Operational
            // Liaison Meeting, FBW; NTSB docket attachment "Airbus Material -
            // Avoiding Tail Strike", PDF p.12). The generic tailcone loft
            // understates the geometric tail-down angle, so it is the floor.
            min_tip_back_deg: 11.7,
            ..LandingGearConfig::default()
        },
        // The planform is the planar wing of the Airbus plan view (Figure
        // 2-2-0-991-004-A01): 34.10 m span, the span the wing-tip-fence
        // sheet prints; the 35.80 m of the sharklet sheet is measured over
        // the sharklets, a wingtip device this geometry does not model, so
        // their mass and drag are excluded. Three further numbers come off
        // that drawing: the 1.64 m chord, a 27.1 deg leading edge, straight
        // from the side of the body to the tip (there is no inboard
        // leading-edge crank), and an unswept inboard trailing edge. The
        // 1.64 m is drawn at the 16.29 m aileron-end station, not at the tip,
        // where sheet 2 draws 1.50 m; it stands here as the tip chord of the
        // equivalent trapezoid, whose trailing edge at the tip lies 0.18 m
        // aft of the drawn one. With them the centreline, kink and kink
        // station are the one set that closes the 122.6 m^2 reference area
        // and the 4.1935 m mean aerodynamic chord of EASA.A.064 together.
        //
        // Two numbers nothing was tuned to check the fit: the kink lands
        // 6.467 m from the centreline, where the plan view puts the
        // trailing-edge break (6.42-6.46 m), and the outboard quarter-chord
        // sweep comes out 25.0 deg, the value specification sheets quote.
        // The root chord the fit gives at the side of the body is 5.86 m,
        // 0.21 m short of the 6.07 m printed there, which is measured from
        // the leading-edge fillet rather than from the swept leading edge.
        design_vector: DesignVector {
            // Step B: the published 35.80 m is the span OVER the sharklets (AC Rev 46 2-2-0 p4); the wing proper stays 34.10 m (fence sheet, 17.05 m semi-span) through
            // WingtipDevice::span_includes_device below (reach 0.846 m per side -> wing 34.108 m). Was span_m 34.10 with no sharklet.
            span_m: 35.80,
            root_chord_m: 6.875,
            break_chord_m: 3.566,
            tip_chord_m: 1.64,
            sweep_deg: 27.1,
            tip_twist_deg: -1.5,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: 37.57,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        airfoil_class_source: "Design-era declaration: the A320 wing (EIS 1988) is of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC(2)-0610 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values (geometry freeze 2026-10-12, owner decision); the drawn planform stays the geometry.
                // S 122.6 m2: Airbus A320 family reference area (reference_wing_area_m2 of this preset); MAC 4.1935 m: EASA TCDS EASA.A.064 section 1 item 16.
                // LEMAC 15.2615 m aft of the nose tip (model frame): the load-sheet H-arm 17.8015 m less the TCDS datum 2.540 m ahead of the nose (the
                // TCDS itself prints no LEMAC; two-point statics give 15.24-15.33 m and the drawn MAC leading edge is 15.260 m).
                reference_wing: crate::ReferenceWingConfig::published(122.6, Some(4.1935), Some(15.2615)),
                // Places the leading edge of MAC 15.26 m aft of the nose, inside
                // the 15.24-15.33 m that two-point statics give on the Airbus
                // section 7-3-0 gear loads, and equal to the commonly quoted
                // load-sheet H-arm 17.8015 m less the EASA.A.064 datum 2.540 m
                // forward of the nose (that H-arm is not from a primary
                // document). (The earlier 16.29 m anchor was a spanwise plan-view
                // dimension, which had put the main gear at 34 %MAC, ahead of
                // the 36.8 %MAC ground condition; it now sits at 58 %MAC.)
                root_datum_x_m: 11.891,
                // One dihedral across both panels, from the Airbus sharklet
                // ground clearances (Figure 2-3-0-991-029-A01 sheet 2, MRW,
                // aft CG): the sharklet bottom stands 4.009 m above the ground
                // and the fuselage bottom 1.792 m, so the lower surface at the
                // wing tip is 2.217 m above the belly, which sits 1.97 m below
                // the +0.1 m cabin axis; the tip leading edge is half a 10 %
                // section above that. From the centreline root 1.2 m below the
                // axis this is 5.1 deg.
                root_z_m: -1.2000, // static ground shape (clearance fit) kept; the drawing fit is the flight shape (geometry round 2)
                break_z_m: -0.6200, // ground shape kept
                tip_z_m: 0.3262, // ground shape kept; 0.3300 moved by half the tip thickness x chord change so the published tip clearance holds
                root_twist_deg: 3.0,
                break_twist_deg: 1.0,
                break_span_fraction: 0.379_3,
                kink_span_fraction: Some(0.379_3),
                // The side-of-body station is the side of this body: half of
                // the 3.95 m fuselage width over the 17.05 m semi-span. It is
                // where the exposed wing starts, and moving it off the body
                // would make the exposed-panel aerodynamics answer for a
                // chord the aeroplane does not have there.
                side_of_body_span_fraction: Some(0.115_84),
                // 5.864 m over the 6.875 m centreline chord: the root-to-kink
                // chord where the wing leaves the body, stated so the exposed
                // root keeps its own lofted section. From there the inboard
                // trailing edge runs straight and unswept to the kink, as on
                // the aeroplane, on the exposed and on the centreline edge.
                side_of_body_chord_ratio: Some(0.852_98),
                outboard_sweep_decrement_deg: 1.5,
                root_airfoil: "sc20610".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                airfoil_class: crate::AirfoilClass::Supercritical,
                                // Thickness profile (geometry round 2): the NASA Common Research Model t/c 0.154 at the
                // centreline, 0.138 at the side of body, 0.105 at the yehudi break and 0.095 at the tip
                // (Vassberg, DeHaan, Rivers and Wahls, AIAA 2008-6919, Table 2) is the public transport-wing
                // distribution; the multipliers below act on the root-kink-tip section blend above.
                thickness_stations: vec![
                    crate::WingThicknessStation {
                        span_fraction: 0.0000,
                        thickness_scale: 1.5444, // t/c 0.0997 -> 0.1540
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.1158,
                        thickness_scale: 1.3840, // t/c 0.0997 -> 0.1380
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.3793,
                        thickness_scale: 1.0530, // t/c 0.0997 -> 0.1050
                    },
                    crate::WingThicknessStation {
                        span_fraction: 1.0000,
                        thickness_scale: 0.9539, // t/c 0.0996 -> 0.0950
                    },
                ],
                // Flight-shape dihedral (geometry round 2): the front-view drawing shows the wing higher than the static ground
                // shape fitted to the published clearances (the airport-planning figures are maximum-ramp-weight states), so the
                // ground heights stay and each station carries the rise to the shape fitted to the drawing.
                flight_root_rise_m: Some(0.2232),
                // The drawn and tabulated nacelle heights are the static ones: keep them in the flight shape.
                nacelles_hold_ground_height: true,
                flight_tip_rise_semispan_fraction: Some(0.016681),
                dihedral_stations: vec![
                    crate::WingDihedralStation {
                        span_fraction: 0.2000,
                        z_m: -0.8942,
                        flight_rise_m: Some(0.2363),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.3000,
                        z_m: -0.7413,
                        flight_rise_m: Some(0.2834),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.3793,
                        z_m: -0.6200,
                        flight_rise_m: Some(0.3043),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.5000,
                        z_m: -0.4360,
                        flight_rise_m: Some(0.3020),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.6500,
                        z_m: -0.2073,
                        flight_rise_m: Some(0.2976),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.8000,
                        z_m: 0.0213,
                        flight_rise_m: Some(0.2947),
                    },
                ],
                // Step B (loft-wing-2026-10-11): C2 cubic spline through the dihedral, twist and thickness stations; was Linear (default).
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                // Step B: MOD160500 sharklet (preset identity, WV017; coordinator decision in measure/DECISIONS.md). AC Rev 46 2-2-0 p4-5 (PDF p46-47): 35.80 m overall span (A), 2.43 m height (B,
                // raster dimension line), tip TE station 23.45 m (B). The blend radius 0.6 m, cant 7.5 deg (reach 0.846 m = (35.80 - 34.10) / 2), tip chord 0.55 m and toe-out 0 are NOT dimensioned
                // on the sheets (C templates); the 43 deg LE sweep is derived so that the tip TE reaches 23.45 m (1.24 m behind the 22.21 m wing-tip TE). include_in_vlm: drag and induced benefit together.
                wingtip_device: Some(crate::WingtipDevice {
                    kind: crate::WingtipDeviceKind::Sharklet,
                    height_m: 2.43,
                    down_height_m: 0.0,
                    cant_deg: 7.5,
                    root_chord_m: None,
                    tip_chord_m: 0.55,
                    le_sweep_deg: 43.0,
                    toe_out_deg: 0.0,
                    blend_radius_m: 0.6,
                    airfoil: None,
                    span_includes_device: true,
                    include_in_vlm: true,
                }),
                ..WingConfig::default()
            },
            // Both surfaces are sized to their published span and area: a
            // 12.45 m tailplane closing 31.0 m^2, and a 5.87 m fin closing
            // 21.5 m^2 at the leading-edge sweep and taper the previous entry
            // already carried. The two spans are dimensioned on the Airbus
            // front view; the two areas are the established published values,
            // not read off that drawing.
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // Tailplane apex moved forward 0.56 m with the tip leading edge held
                // (35.5 m, as drawn): the drawn leading edge is swept 32.8 deg, the
                // former apex gave 28 deg. Root/tip chords, span and area stand.
                // Fig 2-2-0-991-004-A01 sheet 2 vs the same fit (overlay study).
                // geo/tailfit 2026-10-11: tailplane straight-edge fit to the drawing, centreline values; was off 6.06, chords 3.740/1.24, tip (4.02, 6.225, 0.736)
                hstab_offset_from_tail_m: 6.04,
                hstab_z_m: 0.800, // geometry round 2 (wing lane): upper surface at the root matched to the front view; was 0.8
                // Figure 2-2-0-991-004-A01 sheet 2 (plan view, drawing read):
                // 1.24 m tailplane tip chord, tip leading edge 3.31 m aft of
                // the root's (28.0 deg); the root chord closes 31.0 m^2.
                hstab_root_chord_m: 3.8,
                hstab_tip_chord_m: 1.18,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                // Step B: tip rise above the root 0.326 m (dihedral 5.77 deg) from the vector fit of AC Rev 46 2-2-0 p2 front view (PDF p44), measure/A320-200 (B); was z 0.670 (rise 0.260 m, 4.61 deg), earlier 0.5.
                hstab_tip_le_m: (4.01, 6.225, 0.736),
                // Fin position fitted to the side view of Airbus A320 AC (Jul 01/26)
                // Fig 2-2-0-991-004-A01 (digitised, 67 px/m): straight-edge fit puts
                // the crown leading edge at 29.9 m, the crown trailing edge at
                // 35.4 m and the tip trailing edge at 36.7 m, i.e. the tip stays
                // inside the 37.57 m body; the former 6.5 m offset put all three
                // 1.1-1.3 m too far aft (tip trailing edge 0.4 m past the tail
                // cone). Chords, height and sweep are unchanged.
                // geo/tailfit 2026-10-11: fin straight-edge fit to the drawing at the root line (rms 0.007/0.004 m); was off 7.7, chords 5.444/1.884, tip (5.060, 0.0, 5.87)
                vstab_offset_from_tail_m: 7.66,
                // The 5.87 m fin height of sheet 1 is measured from the
                // fuselage top line, so the root line is the crown and the
                // tip stands 11.80 m above the ground in the Figure
                // 2-3-0-991-029-A01 state (VT 11.805 m; TCDS 11.76 m). The
                // chords close the 21.5 m^2 over those 5.87 m; the builder
                // carries the edges on down to the tail cone under the root.
                vstab_z_m: 2.17,
                vstab_root_chord_m: 5.51,
                vstab_tip_chord_m: 1.75,
                vstab_tip_le_m: (5.06, 0.0, 5.87),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 3.95,
                // Airbus A320 Aircraft Characteristics, general dimensions:
                // the external cross-section is taller than it is wide.
                height_m: Some(4.14),
                nose_z_m: -0.57,
                cabin_start_x_m: 3.5,
                cabin_z_m: 0.1,
                tailcone_length_m: 7.5,
                tail_z_m: 1.0,
                // FUSELAGE-G2 shaped nose and aft body. Outline traced from the manufacturer drawing
                // (side: Airbus A320 AC (Jul 01/26) 2-2-0 p2; top: Airbus A320 AC (Jul 01/26) 2-2-0 p3, A320-200 wing-tip fence),
                // calibrated to the drawn length; the nose law (windshield, radome, keel and plan
                // exponents) is a least-squares fit to the traced upper, lower and plan lines (RMS 0.035 m),
                // the aft law (belly ramp, crown drop, plan taper, closing section) to the traced aft lines
                // (RMS 0.031 m). Cabin start, tailcone length and every payload and mass station are unchanged:
                // only the outer loft moves. Section exponent 2.0 (ellipse): no drawing measures it.
                nose_length_m: Some(5.71),
                nose_windshield_angle_deg: Some(32.4),
                nose_crown_end_fraction: Some(1.0),
                nose_radome_length_fraction: Some(0.1),
                nose_keel_exponent: Some(2.99),
                nose_plan_exponent: Some(1.54),
                nose_section_exponent: Some(2.0),
                belly_upsweep_length_m: Some(13.11),
                aft_belly_exponent: Some(1.42),
                tail_end_z_m: Some(1.26),
                tail_end_width_m: Some(0.63),
                tail_end_height_m: Some(0.67),
                aft_plan_length_m: Some(9.55),
                aft_plan_exponent: Some(1.09),
                aft_crown_length_m: Some(4.25),
                aft_crown_exponent: Some(1.0),
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // Final merge: engines.json carries a V2 nacelle set (nacelle_shape_v2) that agrees with the measured length, diameter and inlet; was Legacy.
                nacelle_model: crate::NacelleModel::V2,
                // EASA.A.064 Issue 12, engine axis from aircraft centreline.
                spanwise_positions_m: vec![5.755, -5.755],
                // Engine axis below the local wing leading edge. It puts the
                // 1.0 m-radius nacelle's low point 0.577 m above the ground, the
                // CFM56-5B nacelle clearance of Figure 2-3-0-991-029-A01 sheet 2
                // in the same MRW, aft-CG state as the dihedral above.
                // (-1.48: low point held with the drawn 1.02 m radius, was 1.0.)
                // geo/close 2026-10-12: raised 0.15 m to the drawn height (overlay on the CFM56 side sheet: best rigid shift dz +0.15 m, dx +0.35..0.5 m); was -1.48 (published MRW aft-CG clearance, a
                // compressed-gear minimum, not the static drawn height).
                z_m: -1.33,
                // Puts the nacelle front 11.19 m aft of the nose, the CFM56
                // dimension of Figure 2-2-0-991-004-A01 sheet 4: the 11.891 m
                // root datum plus the 2.945 m the 27.1 deg leading edge runs
                // aft by the engine station, less this offset.
                // geo/close 2026-10-12: drawn nacelle lies 0.40 m aft of the model on both sheets (best rigid shift top dx +0.35, side +0.50); the V2 nacelle front is not the dimensioned 11.19 m
                // reference point. was 3.646
                inlet_x_offset_m: 3.246,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.78,
            cruise_altitude_m: 11278.0,
            mtow_kg: 78_000.0,
            max_wing_area_m2: 130.0,
            min_wing_loading_kg_m2: 500.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 150,
            cargo_payload_kg: 18_000.0,
            // Declared input: maximum zero-fuel weight 62,500 kg less a
            // 41,244 kg empty weight that is absent from the current Airbus
            // document (see `crate::oew_reference`); retained as declared.
            max_structural_payload_kg: 21_256.0,
            dive_speed_m_s: 180.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: super::high_lift("modern_narrowbody"),
    }
}
