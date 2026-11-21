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
        class: crate::presets::AircraftClass::Transport,
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
            // Airbus A320 Aircraft Characteristics - Airport and Maintenance Planning, Rev Jun 01/24,
            // sec. 2-1-1 p.5, A320ceo CFM, no ACT: usable fuel 24,209 L at the page's own density 0.785 kg/L
            // (volume and density from the same page; the IAE page value is 23,859 L).
            usable_fuel_volume_l: Some(24_209.0),
            usable_fuel_mass_kg: Some(24_209.0 * 0.785),
            fuel_density_kg_l: Some(0.785),
            // Airbus A320 Aircraft Characteristics for Airport Planning, Rev 46, section 2-4-1 (typical two-class cabin).
            planning_seats: Some(150),
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: None,
                cruise: None,
                range_nmi: 2_120.0,
                payload_kg: Some(19_700.0),
                source: "Airbus A320 Aircraft Characteristics Jun 2024, section 3-2-1 Figure 3-2-1-991-016-A01 (payload/range ISA, A320-200, 78,000 kg curve, no ACT), maximum-payload corner read as 2,120 nmi at 19.7 t; read uncertainty +-40 nmi, +-0.3 t; reserves not stated on the figure",
                reserve: None,
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
            // Design mission, one provenance row per datum: the maximum-payload corner of the
            // 78,000 kg curve. The chart prints no reserve basis, so the regulation supplies it.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::Manufacturer,
                    stated_conditions: Some("ISA, MTOW 78,000 kg curve, no ACT: maximum-payload corner read as 2,120 nmi (vector read 2,114 nmi); the chart prints no reserve basis"),
                    relative_uncertainty: Some(40.0 / 2_120.0),
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "Airbus A320 Aircraft Characteristics Rev Jul 01/26, section 3-2-1 p.2, Figure 3-2-1-991-016-A01",
                }),
                payload: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "maximum-payload line of the 78,000 kg curve, 19.7 t (vector read 19.78 t), at the corner range",
                    "Airbus A320 Aircraft Characteristics Rev Jul 01/26, section 3-2-1 p.2, Figure 3-2-1-991-016-A01",
                )),
                profile: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "ISA; the chart prints no cruise Mach, climb or descent schedule, so the model's configured cruise Mach and speed schedules apply",
                    "Airbus A320 Aircraft Characteristics Rev Jul 01/26, section 3-2-1 Figure 3-2-1-991-016-A01 (payload/range - ISA conditions)",
                )),
                reserve_fuel: Some(crate::MissionDatumProvenance::regulatory_reserve(
                    "reference chart prints no reserve basis; the design mission applies the regulatory policy: contingency, alternate and 30 min final reserve fuel, at the alternate distance and holding height of the preset's configured fuel policy. The range comparison carries this reserve-basis uncertainty",
                )),
            },
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
            operating_speed_limits: Some(super::operating_limits::A320_200),
            horizontal_tail_ice_protection: Some(crate::presets::tail_ice::A320_200),
            high_speed_protection: Some(super::operating_limits::A320_HIGH_SPEED_PROTECTION),
            ..AircraftReferenceData::default()
        },
        engine_name: "CFM56-5B4/3",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            wheels_per_mlg_strut: 2,
            // Main tire: the A320 AC sec. 7-2-0 prints 46x17R20 without a ply rating; the
            // rated load is the 30 ply lower bound (Goodyear 2022 Sec. 4F). Nose 30x8.8R15.
            mlg_tire: "46x17.0R20".to_owned(),
            nlg_tire: "30x8.8R15/16PR".to_owned(),
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
            // VMCA 110 kt IAS at sea level (VMCG 111 kt CONF 1+F): FCOM limitations as
            // summarised in the theairlinepilots.com A320 limitations sheet (2023).
            published_vmca_m_s: Some(110.0 * 0.514_444),
            trimmable_stabilizer: true,
            // THS nose-up limit 11.2 deg: BEA final report on the XL Airways A320 D-AXLA
            // (2008), the stabiliser moved "from -4.4 to -11.2 corresponding to the electric
            // nose-up stop" (p.19, p.50). It is the electric stop, at or below the mechanical
            // one, so the clamp can only withhold download.
            stabilizer_nose_up_travel_deg: Some(11.2),
            // Published tail-strike attitude 11.7 deg, main gear compressed (Airbus, "Avoiding Tail Strike", FBW
            // Operational Liaison Meeting; NTSB docket attachment "Airbus Material - Avoiding Tail Strike", PDF p.12):
            // the tip-back floor (the generic tailcone loft understates the tail-down angle) and the rotation
            // attitude the built fuselage must reach on its static ground plane.
            min_tip_back_deg: 11.7,
            required_rotation_angle_deg: 11.7,
            required_rotation_basis: crate::landing_gear::RotationAngleBasis::PublishedStatic,
            ..LandingGearConfig::default()
        },
        // The planform is the planar wing of the Airbus plan view (Figure
        // 2-2-0-991-004-A01): 34.10 m span, the span the wing-tip-fence
        // sheet prints; the 35.80 m of the sharklet sheet is measured over
        // the sharklets (wingtip_device below). Further numbers come off that
        // drawing (AC Rev 46 Jul 01/26, sheet 4 plan view, vector): a 27.1 deg
        // leading edge, straight from the side of the body to the tip (there
        // is no inboard leading-edge crank), an unswept inboard trailing edge,
        // the 3.706 m kink chord and the 1.64 m chord printed at the 16.29 m
        // aileron-end station. The tip chord is the chord of those straight
        // edges at the 17.05 m tip; the reference wing is the published one,
        // so the drawn planform does not close
        // the 122.6 m^2 and 4.1935 m of EASA.A.064.
        //
        // The kink lands 6.467 m from the centreline, where the plan view
        // puts the trailing-edge break (6.464 m, vector read), and the
        // outboard quarter-chord sweep comes out 24.7 deg (specification
        // sheets quote 25.0 deg). The side-of-body chord on the edge lines is
        // 6.00 m, 0.07 m short of the 6.07 m printed there, which is measured
        // from the leading-edge fillet rather than from the swept leading edge.
        design_vector: DesignVector {
            // the published 35.80 m is the span OVER the sharklets (AC Rev 46 2-2-0 p4); the wing proper stays 34.10 m (fence sheet, 17.05 m semi-span) through
            // WingtipDevice::span_includes_device below (reach 0.850 m per side = (35.80 - 34.10) / 2 -> wing 34.100 m). Was span_m 34.10 with no sharklet.
            span_m: 35.80,
            root_chord_m: 6.875,
            // Sheet 4 plan view (PDF p47, vector, 37.57 m = 302.06 pt): the drawn chord at the trailing-edge break (y 6.464 m), from the straight LE to the
            // vertical inboard TE, is 3.706 m (29.80 pt); 3.70 m has no source, and 3.566 m closed an earlier area and MAC. With the
            // 0.8726 side-of-body ratio below the inboard TE runs 0.006 m aft from the body to the kink (89.93 deg to the axis).
            break_chord_m: 3.706,
            // Sheet 4 prints 1.64 m at the 16.29 m station (drawn there 1.63-1.69 m); straight edges from the 3.706 m kink chord through it reach 1.480 m at
            // the 17.05 m tip: 3.706 + (1.64 - 3.706) x 10.583 / 9.823. Was 1.58 m (no source) and before that 1.64 m placed at the tip.
            tip_chord_m: 1.480,
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
        speed_schedule_source: crate::presets::SpeedScheduleSource::HandWritten,
        airfoil_class_source: "Design-era declaration: the A320 wing (EIS 1988) is of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC(2)-0610 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values; the drawn planform stays the geometry.
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
                // AC Jul 01/26 Fig 2-2-0-991-004-A01 sheet 4 plan view, vector: one straight 27.2 deg LE from y 2.3 m to 16.3 m through the kink (no crank); kink LE held at x 15.27 m.
                root_datum_x_m: 11.958,
                // Ground shape: the sheet 3 front-view line (the flight shape below) less a tip rise linear in span, so the wing stays on
                // the body at the centreline. Tip from the Airbus sharklet ground clearances (Figure 2-3-0-991-029-A01 sheet 2, MRW, aft CG):
                // the sharklet bottom stands 4.009 m above the ground and the fuselage bottom 1.792 m, so the lower surface at the wing tip is
                // 2.217 m above the belly, which sits 1.97 m below the +0.1 m cabin axis; the tip leading edge is half a 10 % section above that.
                root_z_m: -0.9468,
                break_z_m: -0.4391,
                tip_z_m: 0.3262,
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
                // 6.00 m over the 6.875 m datum: LE on the straight line, TE unswept to the kink, as sheet 4 draws (TE x 18.99 m from body to kink).
                side_of_body_chord_ratio: Some(0.8726),
                // Sheet 4 plan view, vector: the LE fillet leaves the body near x 12.0 m and joins the straight LE near y 2.3 m (x at y: 2.15 m 12.89, 2.25 m 13.09). These stations
                // are the blend body, kept as data: the wing and the box in the body are on the edge lines, which give a 6.01 m side-of-body chord. The printed 6.07 m is taken from the dimension line on the fillet.
                edge_stations: vec![
                    crate::WingEdgeStation { span_fraction: 0.115_84, le_offset_m: -0.874, te_offset_m: 0.0 },
                    crate::WingEdgeStation { span_fraction: 0.1259, le_offset_m: -0.168, te_offset_m: 0.0 },
                    crate::WingEdgeStation { span_fraction: 0.136, le_offset_m: 0.0, te_offset_m: 0.0 }, // joins the straight LE at y 2.3 m (sheet 4); a reading of -0.01 m here tilted the edge line to 27.2 deg and the root panel to 26.85 deg
                ],
                // Sheet 4 plan view: the LE leaves the straight line near y 16.4 m into the sharklet (x 20.83 m at the 17.05 m wing tip); with the
                // TE on its straight line (x 22.163 m at the tip, dv.tip_chord_m 1.480) the tip chord is 1.336 m. Was 1.436 against the unsourced 1.58 m straight chord.
                tip_rake_tip_chord_m: Some(1.336),
                tip_rake_start_span_fraction: Some(0.9617),
                outboard_sweep_decrement_deg: 1.5,
                root_airfoil: "sc20610".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                airfoil_class: crate::AirfoilClass::Supercritical,
                                // Thickness profile: the NASA Common Research Model t/c 0.154 at the
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
                // Flight shape: the heights fitted to the sheet 3 front view. The airport-planning clearances are maximum-ramp-weight states.
                // The drawn and tabulated nacelle heights are the static ones: keep them in the flight shape.
                nacelles_hold_ground_height: true,
                flight_tip_rise_semispan_fraction: Some(0.02372), // tip clearance against the same line; the only rise of this wing, linear in span
                dihedral_stations: vec![
                    crate::WingDihedralStation { span_fraction: 0.2000, z_m: -0.7088, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.3000, z_m: -0.5493, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.5000, z_m: -0.3063, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.6500, z_m: -0.1376, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.8000, z_m: 0.0374, flight_rise_m: None },
                    // Sheet 3 front view: the straight line ends near y 16.4 m, then the sharklet blend rises 0.12 m by the 17.05 m tip (tip rise above).
                    // Moved from 0.9600 (z 0.2219) to the rake start 0.9617 (y 16.4 m, where the sheet ends the
                    // straight line): 0.0017 semispans between the two sections meshed a sliver panel. Same line:
                    // 0.2219 + 0.0017 x (0.2219 - 0.0374) / 0.16 = 0.2239 m.
                    crate::WingDihedralStation { span_fraction: 0.9617, z_m: 0.2239, flight_rise_m: None },
                ],
                // C2 cubic spline through the dihedral, twist and thickness stations.
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                // MOD160500 sharklet (preset identity, WV017). AC Rev 46 Jul 01/26 2-2-0 sheets 3-4: 35.80 m span, 2.43 m height, tip TE x 23.45 m printed; the wing proper
                // is the 34.10 m of sheet 1, so the reach is (35.80 - 34.10) / 2 = 0.850 m per side. Sheet 3 front view (PDF p46, vector, 35.80 m = 285.73 pt): the
                // straight part of the device midline leans 6.4 deg from the vertical (outer edge 5.4, inner edge 7.5 deg: the section thins to the tip); the
                // blend radius closes the printed reach with the printed height, 0.850 = R cos 6.4 + (2.43 - R (1 - sin 6.4)) tan 6.4 -> R 0.6458 m, and the arc
                // then ends within 0.01 m of the drawn midline (17.69 m at 0.57 m rise; fit rms 0.028 m over the drawn height; drawn rise from y 17.05 m 2.45 m).
                // Was cant 9.5 deg / blend 0.45 m (reach 0.788 m, wing 34.225 m), before that 7.5 deg / 0.6 m (reach 0.846 m). Toe-out 0 read from
                // the front view; tip chord 0.50 m read on the plan view (LE 22.95 m); LE sweep 37.07 deg puts the tip TE on the printed 23.45 m (path 2.810 m
                // from the 20.83 m tip LE). The sheet 3 side view draws the same TE at x 23.03 m and the sheet 4 plan view at 23.32 m (drawing inconsistency);
                // the printed plan value governs. include_in_vlm: drag and induced benefit together.
                wingtip_device: Some(crate::WingtipDevice {
                    kind: crate::WingtipDeviceKind::Sharklet,
                    height_m: 2.43,
                    down_height_m: 0.0,
                    cant_deg: 6.4,
                    root_chord_m: None,
                    root_te_flare: None,
                    tip_chord_m: 0.50,
                    le_sweep_deg: 37.07,
                    toe_out_deg: 0.0,
                    blend_radius_m: 0.6458,
                    arc_start_deg: None,
                    path_stations: Vec::new(),
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
                // Drawn tip edges read off the Airbus A320 Aircraft Characteristics tail views; analysis stays on
                // the trapezoid.
                hstab_edge_stations: vec![
                    crate::TailEdgeStation {
                        span_fraction: 0.965,
                        le_offset_m: 0.0,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.98,
                        le_offset_m: 0.05,
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
                vstab_edge_stations: vec![
                    crate::TailEdgeStation {
                        span_fraction: 0.96,
                        le_offset_m: 0.0,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.98,
                        le_offset_m: 0.12,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.99,
                        le_offset_m: 0.28,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 1.0,
                        le_offset_m: 0.5,
                        te_offset_m: 0.0,
                    },
                ],
                tail_airfoil: "naca0012".to_owned(),
                // Sheet 4 plan view, vector, straight edges to the centreline: LE x 31.696 m + 0.640 y, TE x 35.529 m + 0.2212 y (y 2 m to 6 m). The drawn tip rounding is not expressible.
                hstab_offset_from_tail_m: 5.874,
                hstab_z_m: 0.700, // sheet 3 front view: upper surface at eta 0.55-0.85 stood 0.10 m high; was 0.800
                hstab_root_chord_m: 3.833, // sheet 4 plan view, edges above at y = 0
                hstab_tip_chord_m: 1.185, // sheet 4 plan view: drawn tip chord 1.205 m, built read 1.260 m at 1.24 (+4.6 %)
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                // tip rise above the root 0.326 m (dihedral 5.77 deg) from the vector fit of AC Rev 46 2-2-0 p2 front view (PDF p44), A320-200 (B); was z 0.670 (rise 0.260 m, 4.61 deg), earlier 0.5.
                hstab_tip_le_m: (3.979, 6.225, 0.736),
                // Fin position fitted to the side view of Airbus A320 AC (Jul 01/26)
                // Fig 2-2-0-991-004-A01 (digitised, 67 px/m): straight-edge fit puts
                // the crown leading edge at 29.9 m, the crown trailing edge at
                // 35.4 m and the tip trailing edge at 36.7 m, i.e. the tip stays
                // inside the 37.57 m body; the former 6.5 m offset put all three
                // 1.1-1.3 m too far aft (tip trailing edge 0.4 m past the tail
                // cone). Chords, height and sweep are unchanged.
                // Sheet 3 side view, vector: LE x 32.245 m + 0.858 (z - 5), TE x 35.952 m + 0.2208 (z - 5), z 3.5-7.5 m; built LE/TE stood 0.11-0.13 m forward along the height: fin 0.11 m aft.
                vstab_offset_from_tail_m: 7.65,
                // The 5.87 m fin height of sheet 1 is measured from the
                // fuselage top line, so the root line is the crown and the
                // tip stands 11.80 m above the ground in the Figure
                // 2-3-0-991-029-A01 state (VT 11.805 m; TCDS 11.76 m). The
                // chords close the 21.5 m^2 over those 5.87 m; the builder
                // carries the edges on down to the tail cone under the root.
                vstab_z_m: 2.06, // side view: drawn tip z 7.945 m, built 8.057 m; root 0.11 m lower (inside the body), 5.87 m height kept
                vstab_root_chord_m: 5.51,
                vstab_tip_chord_m: 1.77,
                vstab_tip_le_m: (5.04, 0.0, 5.87),
                // Sheet 3 side view: the dorsal fillet leaves the crown at x 28.9 m and joins the LE about z 3.49 m. Geometry only.
                vstab_dorsal_length_m: Some(1.02),
                vstab_dorsal_height_m: Some(1.43),
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
                // Cabin section: floor depth below the outer crown measured on Airbus AC A320 (Jun 2024) fig. 2-5-0-991-005; the hold takes the class section (the AC draws no hold polygon); lining widths: Airbus's published maximum cabin width 3.70 m (aircraft.airbus.com, A320neo, accessed 2026-10-22) carried at the figure's widest line (the inner width is flat over 0.405-0.644 m above the floor, so the 0.635 m armrest reads 0.9996 of it: armrest 3.70 m, a measurement) and transferred to 1.034 m with the figure's own width ratio 0.9541 = 3.53 m (ratios do not depend on the figure's off scale; D29 pending the owner); cabin-datum-evidence.md.
                cabin_section: Some(crate::CabinSectionConfig { cabin_widths: Some(crate::CabinWidthsConfig { armrest_m: 3.70, shoulder_m: 3.53 }), floor_below_crown_m: Some(2.41), hold: None }),
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // engines.json carries a V2 nacelle set (nacelle_shape_v2) that agrees with the measured length, diameter and inlet.
                nacelle_model: crate::NacelleModel::V2,
                // EASA.A.064 Issue 12, engine axis from aircraft centreline.
                spanwise_positions_m: vec![5.755, -5.755],
                // Engine axis below the local wing leading edge (ground shape).
                // Sheet 3 front and side views draw the cowl 2.25 m tall (z -0.80 to -3.05 m) and 2.07 m wide; the circular catalogue section is centred on it (axis z -1.96 m).
                z_m: -1.28,
                z_per_engine_m: vec![-1.4697, -1.4697], // the same axis z -1.96 m from the ground shape; z_m above is what the propulsion installation reads
                // Inlet lip below the local wing LE. Sheets 3-4, vector: lip x 11.56 m in plan and 11.60 m in side, against the wing as drawn; the printed 11.19 m does not scale to either view.
                inlet_x_offset_m: 3.31,
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
            // Superseded at registration: the profile declares the TCDS VMO/MMO, so VD is the
            // CS 25.335(b) value on VC = VMO (`crate::design_speeds`, `registry`).
            dive_speed_m_s: 180.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: super::high_lift("modern_narrowbody"),
    }
}
