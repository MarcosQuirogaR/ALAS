// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airbus A220-300 preset.

use super::exit_layouts::A220_300_CERTIFIED_EXIT_LAYOUT;
use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MassModelConfig, MissingDesignMissionDatum,
    MissionEvidenceApplicability, PartialDesignMissionEvidence, PartialMissionEvidenceKind,
    PublishedMissionLoadCase, PublishedRange, WingConfig,
};

/// The smallest type in the registry, and the only one with its own mass model.
pub fn a220_300() -> AircraftPreset {
    AircraftPreset {
        name: "A220-300",
        display_name: "Airbus A220-300",
        description: "BD-500-1A11 legacy-weight A220-300 with PW1521G-3 engines.",
        identity: AircraftVariantIdentity {
            model: "BD-500-1A11",
            weight_variant: "legacy 149,000 lb MTOW",
            engine_model: "PW1521G-3",
            modification_state: "S/N 55001-59999 planning configuration",
            tank_configuration: "standard integral tanks",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Vol. I Table 1-1 (aerodrome reference code) applied to the
            // preset wingspan of 35.10 m (Airbus A220-300 airport-planning dimensions):
            // 24 m <= b < 36 m is code C.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::C),
            mrw_kg: Some(68_039.0),
            mtow_kg: Some(67_585.0),
            mlw_kg: Some(58_740.0),
            mzfw_kg: Some(55_792.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("A220-300"),
            usable_fuel_volume_l: Some(21_504.92),
            usable_fuel_mass_kg: Some(17_395.27),
            fuel_density_kg_l: Some(0.8089),
            planning_seats: Some(140),
            design_point: Some(crate::PayloadRangeDesignPoint {
                range_nmi: 2_150.0,
                payload_kg: Some(18_643.0),
                source: "Airbus A220 Airport Planning (ACP Issue 013, 2025-11) p.239 Figure 1 zero-fuel weight vs range, ISA, applicability S/N 55001-59999: maximum-zero-fuel-weight corner read as 2,150 nmi (+-25 nmi), drawn for a 156,300 lb MTOW variant, so a heavier weight variant than this 149,000 lb preset; payload is the preset MZFW minus its reference OEW",
            }),
            // EASA's BD-500 type-certificate data sheet sets 145 as the
            // baseline maximum passenger seating capacity for the selected
            // legacy S/N 55001-59999 configuration.  The optional 149-seat
            // arrangement is a different exit installation and is therefore
            // deliberately not folded into this preset.
            certified_max_seats: Some(145),
            certified_exit_layout: Some(A220_300_CERTIFIED_EXIT_LAYOUT),
            partial_design_mission_evidence: vec![
                PartialDesignMissionEvidence {
                    kind: PartialMissionEvidenceKind::AdvertisedRange,
                    range: Some(PublishedRange::NauticalMiles(3_400.0)),
                    payload_kg: None,
                    load_case: Some(PublishedMissionLoadCase::TakeoffMassesKg(vec![70_900.0])),
                    profile_assumptions: None,
                    reserve_assumptions: None,
                    reserve_contract: None,
                    applicability: "Airbus advertises up to 3,400 nm for the up-to-70.9 t product; that capability claim does not select payload or apply exactly to the preset's legacy 67,585 kg weight variant",
                    configuration_applicability: MissionEvidenceApplicability::DifferentWeightVariant,
                    missing: vec![
                        MissingDesignMissionDatum::Payload,
                        MissingDesignMissionDatum::Profile,
                        MissingDesignMissionDatum::ReserveFuel,
                    ],
                    source: "Airbus A220 Digital Pamphlet FAI V5.2, July 2022, p.1",
                },
                PartialDesignMissionEvidence {
                    kind: PartialMissionEvidenceKind::PayloadRangeChart,
                    range: None,
                    payload_kg: None,
                    load_case: Some(PublishedMissionLoadCase::ZeroFuelWeightRangeEnvelope),
                    profile_assumptions: Some("ISA conditions only"),
                    reserve_assumptions: None,
                    reserve_contract: None,
                    applicability: "BD-500-1A11 S/N 55001-59999 ZFW/range chart; Airbus marks the publication superseded and the chart selects no legacy-weight design point",
                    configuration_applicability: MissionEvidenceApplicability::ExactPreset,
                    missing: vec![
                        MissingDesignMissionDatum::Range,
                        MissingDesignMissionDatum::Payload,
                        MissingDesignMissionDatum::Profile,
                        MissingDesignMissionDatum::ReserveFuel,
                    ],
                    source: "Airbus A220-300 APP Issue 031, 2023-10-19, data module BD500-A-J00-00-00-13AAB-030A-A pp.2-3, Figure 1",
                },
            ],
            cg_evidence: CgEnvelopeEvidence::PublicPlanning,
            reference_wing_area_m2: Some(112.3),
            planning_cg_envelope: Some(crate::presets::cg_envelope::A220_300_PLANNING_CG_ENVELOPE),
            aft_cg_nose_load: Some(crate::presets::gear_load::A220_300),
            sources: vec![
                "Airbus FAST 63 (2019), Flying the A220; Airbus A220 airframe features (July 2025), composite wing: https://www.aircraft.airbus.com/en/newsroom/stories/2025-07-the-clean-sheet-single-aisle-aircraft-at-the-vanguard-of-innovation",
                crate::preset_structures::TRANSPORT_CAP_SOURCE,
                "Airbus A220 Aircraft Recovery Publication BD500-3AB48-10400-00, May 2026, J06-20-01 p.14 and J08-41-03-01 p.2",
                "Airbus A220 ARP J07-40-00-06AAA-030A-A, 2019-10-22 p.2",
                "Airbus A220 Aircraft Characteristics - Airport and Maintenance Planning, A220-ACP-Issue013-00-27Nov2025, DM BD500-A-J06-10-00-00AAA-030A-A Rev 2023-11-01, pp.150-156 (nominal nose-tip drawing frame; dimensions vary with weight/CG)",
                "EASA.IM.A.570 BD-500 TCDS Issue 24, 2026-02-20, Section 2 BD-500-1A11 III.19 p.23 (baseline MPSC 145; Option C25631002 is required for 149)",
            ],
            ..AircraftReferenceData::default()
        },
        engine_name: "PW1500G",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            wheels_per_mlg_strut: 2,
            track_diameter_factor: 6.731 / 3.50,
            // Airbus' ACP side/ground drawing gives the A220-300 nominal
            // longitudinal anchors for S/N 55001-59999.  The dimensions
            // originate at the geometric nose-tip extension, so retain that
            // frame explicitly and normalize the stations before applying
            // them to a resized active fuselage.  These are drawing/group
            // centres, not certified WBM/AFM datum or attachment points.
            reference_wheelbase_m: Some(15.23238),
            reference_track_m: Some(6.731),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(38.68928),
            reference_nlg_x_fraction: Some(3.401568 / 38.68928),
            reference_mlg_x_fractions: Some(vec![
                18.633948 / 38.68928,
                18.633948 / 38.68928,
            ]),
            mlg_strut_bogie_wheels: Some(vec![2, 2]),
            // ACP Issue 013 clearance table: fuselage top 5.385 m minimum
            // less the 3.721 m body height (Figure 1, locator B).
            fuselage_ground_clearance_m: Some(1.664),
            // No published tail-strike attitude backs this preset's aft-fuselage
            // geometry, so its model tail-down angle is unvalidated (the generic
            // tailcone loft understates it): keep the Raymer/Roskam 15 deg floor on
            // top of the tail-down criterion.
            min_tip_back_deg: 15.0,
            takeoff_stabilizer_nose_up_deg: Some(crate::landing_gear::TRANSPORT_THS_TAKEOFF_NOSE_UP_DEG),
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 35.10,
            // geo/close 2026-10-12 (owner: TE meets the fuselage at 90 deg): centreline chord 5.80 -> 7.334 m so the inboard TE is a straight, unswept line to the kink (A320 convention: centreline-extended); with the SOB station below
            root_chord_m: 7.0710, // geo/close pass 2: TE x 19.85 m (drawn inboard TE) minus root LE 12.779 m; pass 1 was 7.334
            break_chord_m: 3.7000, // geo/close 2026-10-12: kink moved inboard to the drawn TE corner (0.3827 -> 0.34 span, TE x 19.85 m on the same outboard TE line); was 3.5336599999999994 // geometry round 2 (wing lane): the sourced straight-edge tip chord is kept; this kink chord closes the published reference area behind the swept-back tip
            tip_chord_m: 1.1, // chord the straight edges reach at the tip (the extreme tip chord is the rake chord below)
            // Leading-edge sweep read off the plan view of the A220 ACP
            // Issue 013 (DM BD500-A-J06-10-00-00AAA-030A-A, Figure 1 sheet
            // 2): a straight 29.5 deg from the side of the body to the
            // winglet on both wings. No quarter-chord value is published;
            // with this taper the outboard quarter chord is 27.0 deg.
            sweep_deg: 29.5,
            tip_twist_deg: -1.5,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: 38.70,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        airfoil_class_source: "Design-era declaration: the A220-300 (CSeries, EIS 2016) wing is of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC2-0714 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values (geometry freeze 2026-10-12, owner decision); the drawn planform stays the geometry.
                // S 112.3 m2: Airbus A220 ACP Table 6 'ESDU wing area' (incl. ailerons, flaps, spoilers and the area within the fuselage); MAC 3.781 m (148.86 in) and LEMAC 16.5353 m
                // (FS 818.998 in, FS0 168.0 in ahead of the nose): A220 Aircraft Recovery Publication BD500-3AB48-10400-00 (May 2026) J06-20-01 p.14; cg_envelope.rs.
                reference_wing: crate::ReferenceWingConfig::published(112.3, Some(3.781), Some(16.5353492)),
                // Puts the model's quarter-MAC point on the published one
                // (LEMAC 16.535 m + 0.25 x 3.781 m reference chord, the
                // planning MAC reference below); the model MAC is 0.8 %
                // shorter, so its own LEMAC sits 0.09 m aft of 16.535 m.
                root_datum_x_m: 12.7791, // geometry round 2 (wing lane): was 12.76; keeps the published quarter-MAC station with the swept-back tip, the closed kink chord and the re-converted sweep
                root_z_m: -1.0651, // geometry round 2 (wing lane); was -1.0
                break_z_m: -0.3461, // geo/close 2026-10-12: height of the C2 dihedral line at the new kink 0.34; was -0.2598 at 0.3827 // geometry round 2 (wing lane); was -0.2
                // Step B (ACP front view: tip top 5.81 m above ground vs ACP Table 7 L 5.85-5.95 m; upper edge rises ~1.2 m/m over the outer 1.15 m, measure/A220-300, B):
                // the raked outer panel climbs 1.2 m above the rake-start station (new 0.934 station below), so the tip LE z = 0.937 + 1.213 = 2.15 m; was 1.1320 (round 2), 1.5 earlier.
                tip_z_m: 2.15,
                root_twist_deg: 3.0,
                break_twist_deg: 1.0,
                break_span_fraction: 0.34, // geo/close 2026-10-12; was 0.37
                // geo/close 2026-10-12: side of body at half the 3.50 m body width over the 17.55 m semispan; chord 6.341 m there (TE x = kink TE) = 0.8647 of the 7.334 m centreline chord; was None
                side_of_body_span_fraction: Some(0.1000),
                side_of_body_chord_ratio: Some(0.8595), // pass 2 (SOB chord 6.078 m over 7.071 m)
                kink_span_fraction: Some(0.34), // geo/close 2026-10-12: drawn TE corner (top sheet: the unswept inboard TE meets the outboard TE line 0.76 m inboard of the old kink); was 0.382_736_255_076_680_5
                outboard_sweep_decrement_deg: 1.5,
                root_airfoil: "SC2-0714".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                airfoil_class: crate::AirfoilClass::Supercritical,
                                // Thickness profile (geometry round 2): the NASA Common Research Model t/c 0.154 at the
                // centreline, 0.138 at the side of body, 0.105 at the yehudi break and 0.095 at the tip
                // (Vassberg, DeHaan, Rivers and Wahls, AIAA 2008-6919, Table 2) is the public transport-wing
                // distribution; the multipliers below act on the root-kink-tip section blend above.
                thickness_stations: vec![
                    crate::WingThicknessStation {
                        span_fraction: 0.0000,
                        thickness_scale: 1.1037, // t/c 0.1395 -> 0.1540
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.1000,
                        thickness_scale: 0.9890, // t/c 0.1395 -> 0.1380
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.3400, // geo/close 2026-10-12: follows the kink; was 0.3827
                        thickness_scale: 0.7525, // t/c 0.1395 -> 0.1050
                    },
                    crate::WingThicknessStation {
                        span_fraction: 1.0000,
                        thickness_scale: 0.9539, // t/c 0.0996 -> 0.0950
                    },
                ],
                // Swept-back tip (geometry round 2): the leading and trailing edges follow the straight lines to the
                // design tip chord; outboard of the rake start the leading edge sweeps further aft so that the chord at the
                // extreme tip is 0.59 m (silhouette of the drawing 0.15 m inboard of the tip), the trailing edge staying one line.
                // The design tip chord closes the published reference area including the rake.
                tip_rake_tip_chord_m: Some(0.59),
                tip_rake_start_span_fraction: Some(0.9340),
                // Spanwise dihedral (geometry round 2): leading-edge heights at interior stations fitted to the upper edge of the
                // manufacturer front view (symmetrised drawing aligned on the fuselage crown, same measurement on the model).
                dihedral_stations: vec![
                    crate::WingDihedralStation {
                        span_fraction: 0.1000,
                        z_m: -0.8561,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.2000,
                        z_m: -0.6440,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.3000,
                        z_m: -0.4257,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.5000,
                        z_m: -0.0276,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.6500,
                        z_m: 0.2480,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.8000,
                        z_m: 0.5416,
                        flight_rise_m: None,
                    },
                    // Step B: rake-start station (0.934 = tip_rake_start_span_fraction), on the former straight line 0.8 -> tip (0.5416 -> 1.1320), so the inner wing keeps its round-2 shape.
                    // geo/close 2026-10-12 (owner: odd dihedral near the tip in the front view): the C2 spline through the single 0.8 / 0.934 / 1.0 knee overshot below the chord line
                    // (lower-surface dip at ~0.9 span). Stations on the same two straight lines (0.8 -> 0.934 and 0.934 -> tip, 46 deg) pin the spline; knee height and tip unchanged.
                    crate::WingDihedralStation {
                        span_fraction: 0.8800,
                        z_m: 0.7778,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.9340,
                        z_m: 0.9371,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.9670,
                        z_m: 1.5436,
                        flight_rise_m: None,
                    },
                ],
                // Step B (loft-wing-2026-10-11): C2 cubic spline through the dihedral, twist and thickness stations; was Linear (default).
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // ACP Issue 013: 36.6 m^2 tailplane area (Table 6), 12.263 m
                // span (locator D, derived), tip trailing edge 37.82 m aft of
                // the nose (H) and 4.94 m aft of the root leading edge (V,
                // role read off the drawing), at the previous 0.2895 taper.
                // Apex moved 0.73 m forward with the tip leading edge (36.4 m) and
                // the chords held: the ACP plan view draws a 34.75 deg leading
                // edge, the former apex gave 30.5 deg. Area and span stand.
                // geo/tailfit 2026-10-11: tailplane straight-edge fit to the drawing, centreline values; was off 6.55, chords 4.629/1.340, tip (4.26, 6.1315, 0.710)
                hstab_offset_from_tail_m: 6.4,
                hstab_z_m: 0.620, // geometry round 2 (wing lane): upper surface at the root matched to the front view; was 0.7
                hstab_root_chord_m: 3.43,
                hstab_tip_chord_m: 1.28,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (4.27, 6.1315, 0.710), // tailplane dihedral matched to the front view; was z 0.5
                // Fin placed on the ACP Issue 013 Fig 1 sheet 1 side view
                // (digitised, 84 px/m, overlay study): leading edge 30.1 m aft
                // of the nose at the crown and swept 41 deg (the former 4.5 m
                // tip offset gave 36 deg and a leading edge 2.6 m too far aft),
                // tip chord 2.0 m, tip trailing edge 37.4 m (drawn 37.7 m)
                // inside the 38.7 m tail cone, tip 6.193 m above the crown
                // (11.578 m, locator C, over the 5.385 m fuselage top of the
                // clearance table). The crown chord stays area-calibrated:
                // 4.31 m closes the 28.2 m^2 of Table 6 (edges extended to the
                // fuselage axis) on these edges. The straight edges drawn would
                // enclose about 35 m^2 to the axis (crown chord 5.7 m), 25 %
                // above that table value; the published area is kept and the
                // discrepancy (drawing silhouette vs Table 6) is open.
                // geo/tailfit 2026-10-11: fin straight-edge fit to the drawing at the root line (rms 0.007/0.005 m); was off 8.64, chords 4.31/2.0, tip (5.66, 0.0, 6.193)
                vstab_offset_from_tail_m: 8.61,
                vstab_z_m: 1.9605,
                vstab_root_chord_m: 5.64,
                // geo/close 2026-10-12: fin top at the DRAWN height. The tail-fit kept the ACP 6.193 m (11.578 m locator C minus 5.385 m); the side sheet is stretched 1/0.9029 vertically
                // (fuselage), so the drawn fin top is 5.89 m above the crown in the true frame (5.88 in the tail-fit report). Same straight LE/TE lines, cut lower: tip chord 1.78 -> 1.97 (TE slope), tip LE 5.97 -> 5.68; was 1.78 / (5.97, 0, 6.19)
                vstab_tip_chord_m: 1.97,
                // Step B (ACP side view LE fit slope 0.958 m/m above the crown, z-corrected; drawn tip TE 37.7 m, measure/A220-300, B): tip LE offset 5.38 -> 5.66 m (LE sweep 41 -> 42.4 deg
                // over the published 6.193 m; tip TE 37.4 -> 37.7 m as drawn). A shear: chords and area unchanged. Root chord 4.31 m KEPT (drawn 5.66 m): ACP Table 6 VTP 28.2 m^2
                // incl. rudder is the area convention and the drawn root is not reconcilable with it (exposed 23.7 m^2, to axis ~34 m^2), left as an open item.
                vstab_tip_le_m: (5.68, 0.0, 5.89),
                // Step B: dorsal fillet (ACP side view line fit, measure/A220-300, C: LE runs from 28.7 m at the crown to the fin LE (30.09 m) 0.48 m above it; one view, rough). Geometry only (vstab_dorsal_in_aero stays false).
                // geo/tailfit 2026-10-11: dorsal re-fitted to the drawn LE; was x Some(28.7), height Some(0.48)
                vstab_dorsal_x_start_m: Some(29.01),
                vstab_dorsal_height_m: Some(0.8),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 3.50,
                // ACP Issue 013 Figure 1 / table 5, locator B: 146.5 in body
                // height.
                height_m: Some(3.721),
                nose_z_m: -1.01,
                cabin_start_x_m: 3.2,
                cabin_z_m: 0.1,
                tailcone_length_m: 7.0,
                tail_z_m: 0.8,
                // FUSELAGE-G2 shaped nose and aft body. Outline traced from the manufacturer drawing
                // (side: same, Fig.1 sheet 1; top: Airbus A220 ACP BD500-3AB48-13800-00 Fig.1 sheet 2),
                // calibrated to the drawn length; the nose law (windshield, radome, keel and plan
                // exponents) is a least-squares fit to the traced upper, lower and plan lines (RMS 0.204 m),
                // the aft law (belly ramp, crown drop, plan taper, closing section) to the traced aft lines
                // (RMS 0.186 m). Cabin start, tailcone length and every payload and mass station are unchanged:
                // only the outer loft moves. Section exponent 2.0 (ellipse): no drawing measures it.
                nose_length_m: Some(6.01),
                nose_windshield_angle_deg: Some(31.5),
                nose_crown_end_fraction: Some(1.0),
                nose_radome_length_fraction: Some(0.1),
                nose_keel_exponent: Some(4.0),
                nose_plan_exponent: Some(1.62),
                nose_section_exponent: Some(2.0),
                belly_upsweep_length_m: Some(10.3),
                aft_belly_exponent: Some(1.15),
                tail_end_z_m: Some(0.99),
                tail_end_width_m: Some(0.55),
                tail_end_height_m: Some(0.87),
                aft_plan_length_m: Some(10.68),
                aft_plan_exponent: Some(1.52),
                aft_crown_length_m: Some(11.33),
                aft_crown_exponent: Some(2.17),
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // Final merge: engines.json carries a V2 nacelle set (nacelle_shape_v2) that agrees with the measured length, diameter and inlet; was Legacy.
                nacelle_model: crate::NacelleModel::V2,
                // ACP Issue 013 locators P, Q and BB: engine centreline
                // P - (P - BB/2 - Q)/2 = 5.44 m outboard (derived, +-0.15 m).
                spanwise_positions_m: vec![5.44, -5.44],
                // ACP nacelle clearance 22.9 in (0.582 m) minimum under the
                // 1.08 m nacelle radius.
                // Low point held at the ACP clearance with the drawn 1.225 m
                // nacelle radius (nacelle lane: -1.410 -> -1.265), plus the wing lane's
                // +0.054 m for the wing leading edge at the engine (integration).
                // Step B (ACP front view circle centre -1.70 m, side view -1.71 m, measure/A220-300, B): centre 0.083 m lower (-1.617 -> -1.700 m), giving the nacelle clearance 0.645 -> ~0.56 m
                // vs ACP G 0.582 m min and 0.56 m drawn; was -1.211 (ACP-pinned with the older wing LE). New value -1.211 - 0.07 = -1.281 (between drawn centre and ACP clearance).
                z_m: -1.281,
                // Step B: KEPT. Vector side and top silhouettes put the lip at 11.1 m (measure/A220-300, B) but ACP Table 5 K = 479.0 in = 12.17 m and the definition of K in Fig 1 sheet 2 is not
                // readable from the extracted text; unresolved, published value kept (a move to 2.64 m would be needed if K is not the lip).
                // Locator K table value: inlet 12.17 m aft of the nose (the
                // drawn K is not to scale).
                // geo/close 2026-10-12: KEPT. The ACP sheet 2 (top) overlay puts the drawn lip at K = 12.17 m (best rigid shift 0.00 m), so K is the lip as drawn there; sheet 1 (side) draws the
                // nacelle front ~0.55 m further forward (the two sheets of the drawing disagree; side nacelle mismatch stays 0.85 m2 <= 1.0).
                // geo/close 2026-10-12 owner review: side sheet wants the nacelle further forward; 0.15 m forward of the published offset is the top/side compromise (top 0.00 vs side -0.55 m optimum)
                inlet_x_offset_m: 3.8371, // geometry round 2 (wing lane): was 3.668; the wing datum and sweep moved by +0.0191 m at the engine, this keeps the inlet station
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.78,
            cruise_altitude_m: 11278.0,
            mtow_kg: 67_585.0,
            max_wing_area_m2: 120.0,
            min_wing_loading_kg_m2: 480.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 140,
            cargo_payload_kg: 15_000.0,
            // Airbus recovery publication: MZFW 55,792 kg less OEW 37,149 kg.
            max_structural_payload_kg: 18_643.0,
            dive_speed_m_s: 175.0,
            ..DesignRequirements::default()
        },
        // Compatibility-only outcome calibration for the frozen
        // ReferenceCompatibleFractions method: the published operating empty
        // weight is 37.08 t while the global fractions predict about 34.3 t.
        // These raised fractions close that gap; they are not source-backed
        // A220 avionics or furnishings subsystem masses. A physical method
        // must replace them only after its architecture inputs are declared.
        mass_model: Some(MassModelConfig {
            systems_mass_fraction: 0.13,
            furnishings_mass_fraction: 0.12,
            ..MassModelConfig::default()
        }),
        performance: crate::presets::high_lift("modern_narrowbody"),
    }
}
