// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Boeing 787-9 preset and the re-export of the McDonnell Douglas DC-10.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MissingDesignMissionDatum, MissionEvidenceApplicability,
    PartialDesignMissionEvidence, PartialMissionEvidenceKind, WingConfig,
};

mod dc10;
mod outer_shape;
mod wing_stations;

pub use dc10::dc_10;

/// Composite long-range twin, the type the global mass model is tuned on.
pub fn b787_9() -> AircraftPreset {
    AircraftPreset {
        name: "B787-9",
        display_name: "Boeing 787-9 Dreamliner",
        class: crate::presets::AircraftClass::Transport,
        description: "Long-range composite widebody twin with GEnx-1B engines.",
        identity: AircraftVariantIdentity {
            model: "787-9",
            weight_variant: "legacy 561,500 lb MTOW",
            engine_model: "GEnx-1B74/75 P2 family",
            modification_state: "Boeing Rev Q legacy-weight planning baseline",
            tank_configuration: "standard 33,399 US gal usable system",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Vol. I Table 1-1 (aerodrome reference code) applied to the
            // preset wingspan of 60.12 m (Boeing 787-9 airport-planning dimensions):
            // 52 m <= b < 65 m is code E.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::E),
            mrw_kg: Some(255_372.0),
            mtow_kg: Some(254_692.0),
            mlw_kg: Some(192_776.0),
            mlw_secondary_source: None,
            mzfw_kg: Some(181_436.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("B787-9"),
            usable_fuel_volume_l: Some(126_429.0),
            usable_fuel_mass_kg: Some(101_522.0),
            fuel_density_kg_l: Some(101_522.0 / 126_429.0),
            // Boeing (Wimpress) reference area 3,870 ft^2 = 359.535 m^2 (source in reference_wing below).
            reference_wing_area_m2: Some(359.535),
            planning_seats: Some(290),
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: Some("Boeing 787 Airplane Characteristics for Airport Planning Rev Q section 3.2.2: payload/range at long-range cruise; the section prints no Mach for the 787-9 chart"),
                cruise: None,
                range_nmi: 5_300.0,
                payload_kg: Some(52_586.0),
                source: "Boeing 787 Airplane Characteristics for Airport Planning Rev Q section 3.2.2 (payload/range long-range cruise, 787-9 typical engines): maximum-zero-fuel-weight corner read as 5,300 nmi (+-60 nmi); payload is the preset MZFW minus its reference OEW; reserves not stated",
                reserve: None,
            }),
            certified_max_seats: Some(420),
            certified_exit_layout: Some(crate::presets::B787_9_EXIT_LAYOUT),
            planning_cabin: Some(crate::presets::B787_9_PLANNING_CABIN),
            design_mission_evidence: crate::DesignMissionEvidence::Unverified,
            // Design mission, one provenance row per datum: the MZFW corner of the ACAP long-range
            // cruise chart at the 561,500 lb MTOW. 'Typical mission rules' prints no reserve terms.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::Manufacturer,
                    stated_conditions: Some("Mach 0.85 cruise, step climb at 2,000 ft increments, standard day, zero wind, typical mission rules (reserve terms not printed): the 561,500 lb MTOW line meets the 400,000 lb MZFW line at about 5,300 nmi"),
                    relative_uncertainty: Some(60.0 / 5_300.0),
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "Boeing 787 Airplane Characteristics for Airport Planning D6-58333 Rev Q (Oct 2025), section 3.2.2 p.3-3 (787-9, typical engines)",
                }),
                payload: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "maximum zero-fuel weight corner: the preset MZFW less its reference OEW, 52,586 kg",
                    "Boeing 787 Airplane Characteristics for Airport Planning D6-58333 Rev Q, sections 2.1.2 and 3.2.2",
                )),
                profile: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "standard day, zero wind, Mach 0.85 cruise with step climb at 2,000 ft increments; no climb or descent schedule is printed, so the model's configured schedules apply to those phases",
                    "Boeing 787 Airplane Characteristics for Airport Planning D6-58333 Rev Q, section 3.2.2 p.3-3 chart notes",
                )),
                reserve_fuel: Some(crate::MissionDatumProvenance::regulatory_reserve(
                    "reference chart prints no reserve basis ('typical mission rules' only); the design mission applies the regulatory policy: contingency, alternate and 30 min final reserve fuel, at the alternate distance and holding height of the preset's configured fuel policy. The range comparison carries this reserve-basis uncertainty",
                )),
            },
            partial_design_mission_evidence: vec![PartialDesignMissionEvidence {
                kind: PartialMissionEvidenceKind::PayloadRangeChart,
                range: None,
                payload_kg: None,
                load_case: None,
                profile_assumptions: Some("long-range cruise with typical engines only"),
                reserve_assumptions: None,
                reserve_contract: None,
                applicability: "787-9 chart is model-level planning evidence and selects no legacy-weight design point",
                configuration_applicability: MissionEvidenceApplicability::ModelOnly,
                missing: vec![
                    MissingDesignMissionDatum::Range,
                    MissingDesignMissionDatum::Payload,
                    MissingDesignMissionDatum::Profile,
                    MissingDesignMissionDatum::ReserveFuel,
                ],
                source: "Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 3.2.2 p.3-3",
            }],
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            planning_cg_envelope: None,
            usable_cg_range: None,
            aft_cg_nose_load: Some(crate::presets::gear_load::B787_9),
            horizontal_tail_ice_protection: None,
            sources: vec![
                "Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 2",
                "Boeing 787 ARFF composite-content diagram, p.3: https://www.boeing.com/content/dam/boeing/v2/airports/arff/787_composite_arff_data_2025.pdf",
                crate::preset_structures::TRANSPORT_CAP_SOURCE,
                "NASA/TP-20210023843, December 2022, Table I",
                "Boeing 787 ACAP D6-58333 Rev L, December 2015, p.2-3 (typical OEW only)",
            ],
            operating_speed_limits: None,
            high_speed_protection: Some(
                crate::presets::operating_limits::B787_HIGH_SPEED_PROTECTION,
            ),
        },
        engine_name: "GEnx-1B",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            // Main and nose tires: 787 ACAP sec. 7.2 prints 54 x 21.0 R23 38PR (main; rated load
            // is the 30 ply lower bound, Goodyear 2022 Sec. 4F) and 40 x 16.0 R16 26PR (nose).
            mlg_tire: "54x21.0R23/38PR".to_owned(),
            nlg_tire: "40x16.0R16/26PR".to_owned(),
            wheels_per_mlg_strut: 4,
            // Holds the 9.80 m track on the 5.77 m body width.
            track_diameter_factor: 9.8 / 5.77,
            // D6-58333 Rev P 2.2.2: nose-NLG 5.41 m, wheelbase 25.83 m, track 9.80 m.
            reference_wheelbase_m: Some(25.83),
            reference_track_m: Some(9.80),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(62.00),
            reference_nlg_x_fraction: Some(5.41 / 62.00),
            reference_mlg_x_fractions: Some(vec![31.24 / 62.00, 31.24 / 62.00]),
            // D6-58333 Rev Q section 2.3.2 (PDF p.25, ground clearances):
            // fuselage bottom ahead of the wing (D) 1.75 m minimum.
            fuselage_ground_clearance_m: Some(1.75),
            trimmable_stabilizer: true,
            // FAA TCDS T00021SE (Rev 40, p.24, 787-9 "Control Surface Movements"): horizontal
            // stabiliser +4.25/-12.75 deg, elevators +26.93/-32.54 deg, positive = trailing edge
            // down (nose-up stop 12.75, up travel 32.54).
            stabilizer_nose_up_travel_deg: Some(12.75),
            elevator_up_travel_deg: Some(32.54),
            // Tail-strike attitude 9.7 deg for the 787-9 "with wheels on the runway and
            // landing gear struts extended" (UK AAIB Bulletin 11/2020, AAIB-26410,
            // VH-ZND, quoting Boeing); the compressed-strut value was not found. The
            // generic tailcone loft understates the geometric tail-down angle, so
            // this is the floor.
            min_tip_back_deg: 9.7,
            // The same published 9.7 deg is the tail-scrape requirement; it is a
            // struts-extended attitude, so it is compared with the gear-extended
            // model attitude.
            required_rotation_angle_deg: 9.7,
            required_rotation_basis: crate::landing_gear::RotationAngleBasis::PublishedGearExtended,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 60.12,
            // Boeing 787 ACAP D6-58333 Rev Q 2.2.2 plan view, y measured from the drawn centreline:
            // trailing edge 33.71 m at the side of body (y = 2.885 m) and 34.27 m at the kink y = 11.11 m. The gross drawn
            // planform stays above the 359.535 m2 reference (geometry vs reference area).
            root_chord_m: 13.62, // datum only: the side-of-body chord is pinned below (side_of_body_chord_ratio)
            break_chord_m: 6.75, // leading edge 27.52 m, trailing edge 34.27 m on the drawn rounded kink
            tip_chord_m: 2.67, // printed extreme tip: leading edge 40.69 m, trailing edge 43.36 m; the breaks below follow the drawn edge
            // Leading-edge sweep of the D6-58333 Rev Q section 2.2.2 plan view, which prints no angle: the
            // straight leading edge from the drawn kink (27.52 m at y = 11.11 m) to the rake start reads
            // 34.74-34.82 deg on both wings, 34.63-34.73 deg from y = 5 m (scale +-0.05 deg), and it is
            // fitted as one 34.74 deg line through root_datum_x_m below. The 32.2 deg
            // outboard quarter-chord sweep quoted for the 787-9 (32 deg in NASA/TP-20210023843, December
            // 2022, Table I) is not a manufacturer value and is not used: through the drawn 6.75 m kink and
            // 2.67 m tip chords it converts to a 34.35 deg leading edge, 0.4 deg forward of the drawn one.
            // This leading edge builds a 32.60 deg outboard quarter chord.
            sweep_deg: 34.74, // Rev Q 2.2.2 plan view; was 34.74104381154634, the conversion of 32.2 deg c/4 through the earlier kink chord
            tip_twist_deg: -2.0,
            wing_x_shift_m: -1.5,
            tail_scale: 1.0,
            // EASA TCDS: 62.0014 m body length. The 62.81 m (206 ft 1 in) of
            // D6-58333 Rev Q section 2.2.2 ends at the tailplane tip trailing
            // edge, behind the tail cone.
            fuselage_length_m: 62.00,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        speed_schedule_source: crate::presets::SpeedScheduleSource::SourcedLadder,
        airfoil_class_source: "Design-era declaration: the 787-9 (787 EIS 2011, -9 2014) wing is of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC(2)-0612 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Published reference: S 359.535 = 3,870 ft^2, the Boeing (Wimpress) reference area of the PIANO
                // Boeing 787-8 (Dreamliner) sample analysis (Lissys, 2005; one reference wing for -8/-9: the TCDS MAC is the same).
                // No Boeing, EASA or FAA document prints a 787 wing area (ACAP D6-58331 2006, D6-58333 Rev O and Rev Q, TCDS
                // IM.A.115 Issue 30); the earlier 360.464 (3,880 ft^2) had no traceable source (-0.26 %).
                // MAC EASA.IM.A.115 Issue 30 item 16 (787-9 section, p. 37); LEMAC D6-58333 Rev Q 7.4.2 statics.
                reference_wing: crate::ReferenceWingConfig::published(359.535, Some(6.27126), Some(27.784)),
                // Rev Q 2.2.2 plan view: one straight 34.74 deg leading edge from y = 5 m to the rake start (27.52 m at the kink, its
                // extension 40.66 m at y = 30.06 m, against the printed 40.69 m tip leading edge). The published LEMAC 27.784 m
                // (7.4.2 statics) stays in reference_wing.
                root_datum_x_m: 21.314,
                root_z_m: -1.371, // datum of the height line: the panels inside the body are flat at the side-of-body height
                break_z_m: 0.063, // Rev Q 2.2.2 front view, upper and lower lines at y = 11 m, registered on the drawn fuselage section
                tip_z_m: 2.62, // Rev Q 2.2.2 front view, upper line at the tip
                root_twist_deg: 3.5,
                break_twist_deg: 1.5,
                break_span_fraction: 0.35,
                // Rev Q 2.2.2: the printed body width 18 ft 11 in (5.77 m) puts the side of body at y = 2.885 m of the
                // 30.06 m semispan; the plan view draws the wing meeting the body side line there (2.91 m at the drawing
                // scale, +-0.03 m). Was the default 0.10 (y 3.006 m) with a derived chord.
                side_of_body_span_fraction: Some(2.885 / 30.06),
                // Same plan view: the drawn inboard trailing edge (straight from y = 3.5 m to the rounded kink, both wings,
                // three fit windows) reaches 33.71 m (+-0.03 m) at y = 2.885 m. On the built 34.74 deg leading edge
                // (21.815 m there) that is a chord of 11.895 m over the 13.62 m root datum. The drawn leading edge lies
                // 0.10 m aft of the built one at the side of body (drawn chord 11.79 m); the trailing edge is matched.
                side_of_body_chord_ratio: Some(0.8734),
                // Rev Q 2.2.2 plan view: trailing-edge kink at y = 11.11 m of the 30.06 m semispan.
                kink_span_fraction: Some(0.3696),
                // Rev Q 2.2.2 plan view: outboard trailing edge straight (22.9 deg) from the kink to y = 22.5 m.
                te_break_span_fraction: Some(0.75),
                te_break_chord_offset_m: Some(-0.85),
                outboard_sweep_decrement_deg: 2.0,
                // Boeing publishes no 787 sections. The comparable public
                // M 0.85 widebody wing, the NASA Common Research Model
                // (Vassberg et al., AIAA 2008-6919, Table 2), runs from t/c
                // 0.154 at the centreline and 0.138 at the side of body to
                // 0.105 at the 37 % yehudi break and 0.095 at the tip: 0.115
                // area-weighted over the whole wing. This loft holds the
                // root section to the kink, so a 14 % root would carry 0.14
                // to the kink (0.133 overall); the 12 % section lands the
                // loft on the CRM mean, with the 10 % tip section close to
                // its 9.5 %.
                root_airfoil: "sc20612".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                airfoil_class: crate::AirfoilClass::SupercriticalAdvanced,
                                // Thickness profile: the NASA Common Research Model t/c 0.154 at the
                // centreline, 0.138 at the side of body, 0.105 at the yehudi break and 0.095 at the tip
                // (Vassberg, DeHaan, Rivers and Wahls, AIAA 2008-6919, Table 2) is the public transport-wing
                // distribution; the multipliers below act on the root-kink-tip section blend above.
                thickness_stations: vec![
                    crate::WingThicknessStation {
                        span_fraction: 0.0000,
                        thickness_scale: 1.2838, // t/c 0.1200 -> 0.1540
                    },
                    crate::WingThicknessStation {
                        span_fraction: 2.885 / 30.06, // on the side of body (was 0.1000)
                        thickness_scale: 1.1504, // t/c 0.1200 -> 0.1380
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.3696, // follows the kink (was 0.3538)
                        thickness_scale: 0.8753, // t/c 0.1200 -> 0.1050
                    },
                    crate::WingThicknessStation {
                        span_fraction: 1.0000,
                        thickness_scale: 0.9539, // t/c 0.0996 -> 0.0950
                    },
                ],
                // Raked tip, Rev Q 2.2.2 plan view: the leading edge leaves the straight line at y = 27.0 m and
                // curves to a tip chord of about 0.5 m; the trailing edge curves aft to the printed 43.36 m from y = 22.5 m. tip_chord_m
                // stays the chord of the straight edges, so taper, sweep and the reference wing do not see the rake. Offsets are from
                // the lines through the kink, the break, the rake start and the tip.
                tip_rake_tip_chord_m: Some(0.50),
                tip_rake_start_span_fraction: Some(0.898),
                edge_stations: vec![
                    crate::WingEdgeStation { span_fraction: 0.336, le_offset_m: 0.00, te_offset_m: -0.10 }, // rounded kink
                    crate::WingEdgeStation { span_fraction: 0.406, le_offset_m: 0.00, te_offset_m: -0.13 }, // rounded kink
                    crate::WingEdgeStation { span_fraction: 0.857, le_offset_m: 0.00, te_offset_m: -0.73 },
                    crate::WingEdgeStation { span_fraction: 0.898, le_offset_m: 0.00, te_offset_m: -1.01 },
                    crate::WingEdgeStation { span_fraction: 0.918, le_offset_m: -0.24, te_offset_m: -0.98 },
                    crate::WingEdgeStation { span_fraction: 0.945, le_offset_m: -0.40, te_offset_m: -0.84 },
                    crate::WingEdgeStation { span_fraction: 0.965, le_offset_m: -0.41, te_offset_m: -0.62 },
                    crate::WingEdgeStation { span_fraction: 0.985, le_offset_m: -0.33, te_offset_m: -0.33 },
                ],
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                // Rev Q 2.2.2 front view, upper and lower lines, both halves averaged, registered on the drawn fuselage section.
                dihedral_stations: vec![
                    // Moved from 0.1000 (z -0.87) to the side of body: 0.004 semispans off that loft
                    // section it would mesh a sliver panel. Same front-view line: -0.87 - 0.004025 x
                    // (0.353 / 0.1) = -0.8842 m.
                    crate::WingDihedralStation {
                        span_fraction: 2.885 / 30.06,
                        z_m: -0.8842,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.2000,
                        z_m: -0.517,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.3000,
                        z_m: -0.168,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.5000,
                        z_m: 0.609,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.6500,
                        z_m: 1.172,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.8000,
                        z_m: 1.70,
                        flight_rise_m: None,
                    },
                    // Moved from 0.9000 (z 2.08) to the rake start 0.898: a station 0.002 semispans from
                    // the rake-start section meshed a sliver panel the lattice cannot condition. Same
                    // front-view line: 2.08 - 0.002 x (2.08 - 1.70) / 0.1 = 2.0724 m.
                    crate::WingDihedralStation {
                        span_fraction: 0.898,
                        z_m: 2.0724,
                        flight_rise_m: None,
                    },
                ],
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                // Drawn tip edges read off the Boeing 787 Airplane Characteristics for Airport Planning tail views; analysis stays on
                // the trapezoid.
                // Rev Q 2.2.2 plan and side: the tip trailing corner reaches the printed 206 ft 1 in (62.81 m); the edge lies 0.05 m aft of the straight line outboard of the root.
                hstab_edge_stations: vec![
                    crate::TailEdgeStation { span_fraction: 0.30, le_offset_m: 0.0, te_offset_m: 0.02165 },
                    crate::TailEdgeStation { span_fraction: 0.97, le_offset_m: 0.0, te_offset_m: 0.07 },
                    crate::TailEdgeStation { span_fraction: 0.98, le_offset_m: 0.15, te_offset_m: 0.09 },
                    crate::TailEdgeStation { span_fraction: 0.99, le_offset_m: 0.3, te_offset_m: 0.12 },
                    crate::TailEdgeStation { span_fraction: 0.995, le_offset_m: 0.5, te_offset_m: 0.15 },
                    crate::TailEdgeStation { span_fraction: 1.0, le_offset_m: 1.0, te_offset_m: 0.19 },
                ],
                vstab_edge_stations: vec![
                    crate::TailEdgeStation {
                        span_fraction: 0.92,
                        le_offset_m: 0.0,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.97,
                        le_offset_m: 0.1,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.99,
                        le_offset_m: 0.3,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 1.0,
                        le_offset_m: 0.46,
                        te_offset_m: 0.0,
                    },
                ],
                // Rev Q 2.2.2 front view over the side chords: fin t/c 0.08-0.10, mean 0.09.
                vstab_airfoil: Some("naca0009".to_owned()),
                tail_airfoil: "naca0010".to_owned(), // Rev Q 2.2.2 front view: tailplane depth 0.46 m at y 3.2 m; 12 % drew both 0.1 m too deep
                // Rev Q 2.2.2: tailplane span 65 ft 0 in (19.81 m), tip trailing edge at the 206 ft 1 in (62.81 m) overall length,
                // root leading edge 52.61 m (62.00 - 9.39). Chords and sweep are not dimensioned: plan-view measures. The straight
                // leading edge to the tip runs at 40.9 deg; the drawn one reads 40.35 deg and lies within about 0.1 m of it over the exposed span.
                hstab_offset_from_tail_m: 9.39,
                hstab_z_m: 0.57, // Rev Q front view: both surfaces of the 10 % section, 0.10 m above the 0.47 m of the 12 % upper surface
                hstab_root_chord_m: 6.32,
                // Rev Q plan: 1.43 m is the chord of the straight edges at y = 9.905 m. Lines fitted through the drawn edges over
                // 0.35-0.90 of the semispan give 1.49-1.52 m there, because the drawn trailing edge lies aft of the straight one by
                // the hstab_edge_stations offsets (0.02 m at 0.30 to 0.07 m at 0.97, 0.072 m on the line extended to the tip):
                // 1.43 + 0.07 = 1.50 m on the same fitted lines.
                hstab_tip_chord_m: 1.43,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                // Rev Q plan: straight leading edge 52.61 + 8.58 = 61.19 m at the tip (drawn line 61.16-61.21 m); with the 1.43 m chord
                // and the 0.19 m tip offset the trailing corner sits at the printed 206 ft 1 in (62.81 m). Height between the front
                // view and the side view, which differ by 0.2 m at the tip.
                hstab_tip_le_m: (8.58, 9.905, 1.40),
                // Rev Q 2.2.2 side view: no fin station is dimensioned; root line at the crown of the 5.94 m body, top 11.62 m.
                vstab_offset_from_tail_m: 10.79, // Rev Q side view: fin edges read 0.43 m aft; tip trailing point at the printed 62.00 m
                vstab_z_m: 3.17,
                vstab_root_chord_m: 7.52, // Rev Q side view, trailing edge at the root line
                vstab_tip_chord_m: 2.28,
                // 45.1 deg leading edge; the drawn rounded leading-edge top corner is not expressible with straight edges.
                vstab_tip_le_m: (8.53, 0.0, 8.45),
                // Rev Q side view: dorsal fillet leaves the crown at x 49.75 m, joins the fin LE about z 4.48 m.
                vstab_dorsal_length_m: Some(1.46),
                vstab_dorsal_height_m: Some(1.31),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                // D6-58333 Rev Q section 2.2.2 (PDF p.22): body width 18 ft
                // 11 in and height 19 ft 6 in.
                diameter_m: 5.77,
                height_m: Some(5.94),
                nose_z_m: -0.71,
                // Generic nose-taper and tail-cone lengths of the outer mould
                // line: the ACAP prints no flight-deck or aft
                // pressure-bulkhead station. They shape the body only; the
                // passenger cabin is bounded by the section 2.7.1 door
                // stations (`B787_9_EXIT_LAYOUT`).
                cabin_start_x_m: 5.5,
                cabin_z_m: 0.2,
                tailcone_length_m: 12.0,
                tail_z_m: 1.5,
                ..outer_shape::b787_9()
            },
            engine: EngineConfig {
                // engines.json carries a V2 nacelle set (nacelle_shape_v2) that agrees with the measured length, diameter and inlet.
                nacelle_model: crate::NacelleModel::V2,
                // Boeing 787 ACAP D6-58333 Rev Q, October 2025, section
                // 2.2.2 (PDF p.22, General Dimensions: Model 787-9) plan
                // view: engine centreline 32 ft 6 in (9.91 m) from the
                // airplane centreline, nacelle inlet face 68 ft 3 in
                // (20.80 m) aft of the nose tip.
                spanwise_positions_m: vec![9.91, -9.91],
                // Offset from the wing height at the engine station. Rev Q 2.2.2: nacelle centre z -2.03 m, low point -3.95 m in the
                // body frame (side view -3.99 m, front view -3.94 m), 1.16 m below the keel; section 2.3.2 prints the 0.69 m
                // ground clearance (F) against 1.75 m at the keel (D), consistent on the drawn sloping ground line.
                z_m: -1.96,
                // Puts that inlet face 20.80 m aft of the nose: the leading edge runs to 26.69 m at y = 9.91 m.
                inlet_x_offset_m: 5.888,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.85,
            cruise_altitude_m: 11887.2,
            mtow_kg: 254_692.0,
            max_wing_area_m2: 385.0,
            min_wing_loading_kg_m2: 480.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 290,
            cargo_payload_kg: 55_000.0,
            // Maximum zero-fuel weight 181.4 t less an operating empty weight
            // of 128.8 t.
            max_structural_payload_kg: 52_586.0,
            // Superseded at registration: the profile declares the TCDS VMO/MMO, so VD is the
            // CS 25.335(b) value on VC = VMO (`crate::design_speeds`, `registry`).
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("advanced_highlift_widebody"),
    }
}
