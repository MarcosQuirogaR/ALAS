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
            span_m: 34.10,
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
                root_z_m: -1.2,
                break_z_m: -0.62,
                tip_z_m: 0.33,
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
                hstab_offset_from_tail_m: 5.5,
                hstab_z_m: 0.8,
                // Figure 2-2-0-991-004-A01 sheet 2 (plan view, drawing read):
                // 1.24 m tailplane tip chord, tip leading edge 3.31 m aft of
                // the root's (28.0 deg); the root chord closes 31.0 m^2.
                hstab_root_chord_m: 3.740,
                hstab_tip_chord_m: 1.24,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (3.31, 6.225, 0.5),
                vstab_offset_from_tail_m: 6.5,
                // The 5.87 m fin height of sheet 1 is measured from the
                // fuselage top line, so the root line is the crown and the
                // tip stands 11.80 m above the ground in the Figure
                // 2-3-0-991-029-A01 state (VT 11.805 m; TCDS 11.76 m). The
                // chords close the 21.5 m^2 over those 5.87 m; the builder
                // carries the edges on down to the tail cone under the root.
                vstab_z_m: 2.17,
                vstab_root_chord_m: 5.444,
                vstab_tip_chord_m: 1.884,
                vstab_tip_le_m: (5.060, 0.0, 5.87),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 3.95,
                // Airbus A320 Aircraft Characteristics, general dimensions:
                // the external cross-section is taller than it is wide.
                height_m: Some(4.14),
                // Measured nose (v1.3.2): Airbus A320 AC (Jul 2026) fig 2-2-0-991-004-A01 sheets 1-2, PDF pp 44-45,
                // Outline read from the drawing, nose length = L(2 %) (full section within
                // 2 % of D_eff), tip height from the drawn mid-line at the tip. Laws fitted at fixed
                // length to the upper, lower and plan lines (RMS 0.02-0.04 D_eff); the section
                // exponent is not measurable from an airport-planning drawing and stays unset.
                nose_z_m: -0.63,
                cabin_start_x_m: 4.78,
                cabin_z_m: 0.1,
                tailcone_length_m: 7.5,
                tail_z_m: 1.0,
                // Solved so the tail-down angle about the main wheels on the static
                // ground plane is the published 11.7 deg pitch to ground contact with
                // the main gear compressed: Airbus, "Avoiding Tail Strike" (Operational Liaison Meeting, FBW; NTSB docket attachment "Airbus Material - Avoiding Tail Strike", PDF p.12).
                // The belly then starts to rise 26.3 m aft of the nose.
                belly_upsweep_length_m: Some(11.25),
                nose_windshield_angle_deg: Some(33.0),
                nose_crown_end_fraction: Some(1.0),
                nose_radome_length_fraction: Some(0.11),
                nose_keel_exponent: Some(2.31),
                nose_plan_exponent: Some(1.27),
                nose_section_exponent: None,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // EASA.A.064 Issue 12, engine axis from aircraft centreline.
                spanwise_positions_m: vec![5.755, -5.755],
                // Engine axis below the local wing leading edge. It puts the
                // 1.0 m-radius nacelle's low point 0.577 m above the ground, the
                // CFM56-5B nacelle clearance of Figure 2-3-0-991-029-A01 sheet 2
                // in the same MRW, aft-CG state as the dihedral above.
                z_m: -1.50,
                // Puts the nacelle front 11.19 m aft of the nose, the CFM56
                // dimension of Figure 2-2-0-991-004-A01 sheet 4: the 11.891 m
                // root datum plus the 2.945 m the 27.1 deg leading edge runs
                // aft by the engine station, less this offset.
                inlet_x_offset_m: 3.646,
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
