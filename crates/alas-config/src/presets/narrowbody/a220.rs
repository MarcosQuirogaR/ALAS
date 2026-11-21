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
        class: crate::presets::AircraftClass::Transport,
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
            // preset wingspan of 34.984 m (Airbus A220-300 airport-planning dimensions):
            // 24 m <= b < 36 m is code C.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::C),
            mrw_kg: Some(68_039.0),
            mtow_kg: Some(67_585.0),
            mlw_kg: Some(58_740.0),
            mzfw_kg: Some(55_792.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("A220-300"),
            // ACAP BD500-3AB48-13800-00 PDF p.26, Maximum fuel tank
            // capacity, BD-500-1A11 column: 5,790 USG (21 918 L), the 149,000 lb MTOW
            // basis of this preset. Mass = volume x fuel_density_kg_l (0.8089).
            usable_fuel_volume_l: Some(21_918.0),
            usable_fuel_mass_kg: Some(17_729.5),
            fuel_density_kg_l: Some(0.8089),
            planning_seats: Some(140),
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: None,
                cruise: None,
                range_nmi: 3_100.0,
                payload_kg: Some(13_957.0),
                source: "Airbus Canada A220-300 Airport Planning ACP Issue 001 (2015-10-29), DM BD500-A-J00-00-00-13AAB-030A-A Figure 1 page 3 (zero fuel weight vs range, ISA still air, MTOW 149,000 lb, MZFW 123,000 lb): '3100 NM RANGE' with '81750 lb OWE & 140 PAX', payload 30,770 lb = 13,957 kg; reserves not printed. The current ACP Issue 013 chart is drawn for the heavier 156,300 lb variant and is not used",
                reserve: None,
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
            // Design mission, one provenance row per datum: the 140-passenger design-range point
            // of ACP Issue 001, drawn for this 149,000 lb MTOW variant. No reserve basis is printed.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "ISA still air, MTOW 149,000 lb, MZFW 123,000 lb: '3100 NM RANGE' at the MTOW line with '81750 lb OWE & 140 PAX'; the chart prints no reserve basis",
                    "Airbus Canada A220-300 Airport Planning ACP Issue 001 (2015-10-29), DM BD500-A-J00-00-00-13AAB-030A-A Figure 1 (zero fuel weight vs range, ISA) page 3",
                )),
                payload: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "140 passengers at 81,750 lb OWE, ZFW 112,520 lb: payload 30,770 lb = 13,957 kg",
                    "Airbus Canada A220-300 Airport Planning ACP Issue 001, DM 13AAB Figure 1 page 3",
                )),
                profile: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "ISA still air; the chart prints no cruise Mach, climb or descent schedule, so the model's configured cruise Mach and speed schedules apply",
                    "Airbus Canada A220-300 Airport Planning ACP Issue 001, DM 13AAB Figure 1 page 3",
                )),
                reserve_fuel: Some(crate::MissionDatumProvenance::regulatory_reserve(
                    "reference chart prints no reserve basis; the design mission applies the regulatory policy: contingency, alternate and 30 min final reserve fuel, at the alternate distance and holding height of the preset's configured fuel policy. The range comparison carries this reserve-basis uncertainty",
                )),
            },
            cg_evidence: CgEnvelopeEvidence::PublicPlanning,
            reference_wing_area_m2: Some(112.3),
            planning_cg_envelope: Some(crate::presets::cg_envelope::A220_300_PLANNING_CG_ENVELOPE),
            usable_cg_range: Some(crate::presets::usable_cg_range::A220_300),
            aft_cg_nose_load: Some(crate::presets::gear_load::A220_300),
            sources: vec![
                "Airbus FAST 63 (2019), Flying the A220; Airbus A220 airframe features (July 2025), composite wing: https://www.aircraft.airbus.com/en/newsroom/stories/2025-07-the-clean-sheet-single-aisle-aircraft-at-the-vanguard-of-innovation",
                crate::preset_structures::TRANSPORT_CAP_SOURCE,
                "Airbus A220 Aircraft Recovery Publication BD500-3AB48-10400-00, May 2026, J06-20-01 p.14 and J08-41-03-01 p.2",
                "Airbus A220 ARP J07-40-00-06AAA-030A-A, 2019-10-22 p.2",
                "Airbus A220 Aircraft Characteristics - Airport and Maintenance Planning, A220-ACP-Issue013-00-27Nov2025, DM BD500-A-J06-10-00-00AAA-030A-A Rev 2023-11-01, pp.150-156 (nominal nose-tip drawing frame; dimensions vary with weight/CG)",
                "EASA.IM.A.570 BD-500 TCDS Issue 24, 2026-02-20, Section 2 BD-500-1A11 III.19 p.23 (baseline MPSC 145; Option C25631002 is required for 149)",
            ],
            operating_speed_limits: Some(crate::presets::operating_limits::A220_300),
            high_speed_protection: Some(
                crate::presets::operating_limits::A220_HIGH_SPEED_PROTECTION,
            ),
            ..AircraftReferenceData::default()
        },
        engine_name: "PW1500G",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            // Main tire: A220 ACP Table 29 prints H42 x 15.0 R21 26 PR. No data book rates it,
            // so the rated load is the dominated-envelope lower bound H40x14.5-19/26PR.
            mlg_tire: "H42x15.0R21/26PR".to_owned(),
            nlg_tire: "27x8.5R12/16PR".to_owned(),
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
            // Tail-scrape requirement: no published tail-strike attitude found (Airbus A220 ACP Issue 013, Airbus
            // Safety First, FCOM excerpts searched), so the 10 deg default (an assumption) stands.
            trimmable_stabilizer: true,
            // ACP Issue 013, A220-300 section 4.3, Tables 6 and 7 (ACN at the
            // most forward CG): 157,000 lb gross, 143,077 lb on the main gear,
            // so 13,923 lb (6,315 kg) on the nose gear at the maximum ramp
            // weight; 13,661-13,923 lb from 134,000 to 157,000 lb.
            nose_gear_max_static_load_kg: Some(13_923.0 * 0.453_592_37),
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 34.984, // ACP Issue 013 Table 3, locator E: 1377.32 in over the winglets, baseline span; was the rounded 35.10 m
            // Datum on the centreline: the unswept inboard TE (x 19.90 m, ACP Issue 013 Fig 1 sheet 2 plan view) minus the straight-LE datum 12.775 m.
            root_chord_m: 7.1250,
            break_chord_m: 3.5800, // TE corner: outboard TE line (x 19.93 m at y 6.32 m) less the LE line, plan view
            tip_chord_m: 1.2750, // Straight-edge tip chord: TE line x 21.97 m at y 12 m, 23.43 m at y 16 m, plan view
            // Leading-edge sweep, plan view of the A220 ACP Issue 013 (DM BD500-A-J06-10-00-00AAA-030A-A, Figure 1 sheet 2): a straight 29.5 deg
            // from the end of the root fillet (y 2.2 m) to the winglet. No quarter-chord value is published.
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
        speed_schedule_source: crate::presets::SpeedScheduleSource::SourcedLadder,
        airfoil_class_source: "Design-era declaration: the A220-300 (CSeries, EIS 2016) wing is of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC2-0714 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values; the drawn planform stays the geometry.
                // S 112.3 m2: Airbus A220 ACP Issue 013, DM BD500-A-J06-10-00-00AAA-030A-A Table 6 p.11, 'ESDU wing area' 1208.88 sq ft (incl. ailerons, flaps, spoilers and the area within the fuselage); MAC 3.781 m (148.86 in) and LEMAC 16.5353 m
                // (FS 818.998 in, FS0 168.0 in ahead of the nose): A220 Aircraft Recovery Publication BD500-3AB48-10400-00 (May 2026) J06-20-01 p.14; cg_envelope.rs.
                // The 112.3 m2 is an equivalent straight-tapered reference wing, not the drawn area: fitted to this S and MAC on span E with the drawn LE extended to the centreline (taper 0.156), it puts LEMAC within 0.012 m of the published one. The ACP plan view (Fig 1 sheet 2 p.8) draws 97.5 m2 exposed and 121.3 m2 under the Airbus side-of-body rule (Aerodynamic Design of Transport Aircraft (2009), ch. 24 p. 269, fig. 24.144); the preset builds 119.06 m2 under that rule.
                reference_wing: crate::ReferenceWingConfig::published(112.3, Some(3.781), Some(16.5353492)),
                root_datum_x_m: 12.7750, // LE line through x 14.01 m at y 2.2 m and 19.52 m at y 12 m (plan view); the 34.2 deg crank was the root fillet read as sweep
                root_z_m: -1.1300, // Datum on the line of the 0.10 and 0.20 stations; the carried section takes the 0.10 height
                break_z_m: -0.2500, // ACP Fig 1 front view, on the crown-registered upper-surface line (see dihedral stations)
                // ACP Fig 1 front view: winglet upper edge rises 1.28 m/m from the knee at y 16.4 m to the top at z 2.33 m (crown-registered).
                tip_z_m: 2.3000,
                root_twist_deg: 3.0,
                break_twist_deg: 1.0,
                break_span_fraction: 0.36, // follows the kink
                // Side of body at half the 3.50 m body width over the 17.55 m semispan.
                side_of_body_span_fraction: Some(0.1000),
                side_of_body_chord_ratio: Some(0.8606), // Before the fillet: TE x 19.90 m less the LE line 13.768 m = 6.132 m over the datum chord (plan view)
                inboard_le_sweep_deg: None, // Plan view: LE straight at the design sweep past the root fillet
                kink_span_fraction: Some(0.36), // Plan view: drawn TE corner, y 6.32 m
                outboard_sweep_decrement_deg: 1.5,
                root_airfoil: "SC2-0714".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                airfoil_class: crate::AirfoilClass::SupercriticalAdvanced,
                                // Thickness profile: the NASA Common Research Model t/c 0.154 at the
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
                        span_fraction: 0.3600, // follows the kink
                        thickness_scale: 0.7525, // t/c 0.1395 -> 0.1050
                    },
                    crate::WingThicknessStation {
                        span_fraction: 1.0000,
                        thickness_scale: 0.9539, // t/c 0.0996 -> 0.0950
                    },
                ],
                // Swept-back winglet in plan (ACP Fig 1 sheet 2): both edges leave the straight lines near y 16.1 m. TE steps aft to x 23.76 m at y 16.45 m, a notch to
                // x 23.73 m at y 16.63 m, then straight to the tip corner x 24.37 m. LE runs straight from x 22.56 m at y 16.63 m to the tip corner x 23.89 m. Tip chord 0.48 m.
                // The rake holds the tip TE on the straight line (x 23.95 m), so the tip section is moved aft by the tip edge station below.
                tip_rake_tip_chord_m: Some(0.48),
                tip_rake_start_span_fraction: Some(0.9200),
                // Offsets read off the plan view: root fillet (x 13.30 m at the side of body, 13.72 m at y 1.97 m) and winglet edges (see above).
                edge_stations: vec![
                    crate::WingEdgeStation { span_fraction: 0.1000, le_offset_m: -0.470, te_offset_m: 0.0 },
                    crate::WingEdgeStation { span_fraction: 0.1125, le_offset_m: -0.170, te_offset_m: 0.0 },
                    crate::WingEdgeStation { span_fraction: 0.1250, le_offset_m: 0.0, te_offset_m: 0.0 },
                    crate::WingEdgeStation { span_fraction: 0.9302, le_offset_m: -0.102, te_offset_m: 0.123 },
                    crate::WingEdgeStation { span_fraction: 0.9404, le_offset_m: -0.095, te_offset_m: 0.188 },
                    crate::WingEdgeStation { span_fraction: 0.9506, le_offset_m: 0.071, te_offset_m: 0.089 },
                    crate::WingEdgeStation { span_fraction: 1.0000, le_offset_m: 0.4235, te_offset_m: 0.4235 },
                ],
                // Spanwise dihedral: leading-edge heights fitted to ACP Fig 1 front view upper and lower edges with the view registered on the fuselage crown
                // (upper line 5.8 deg), which agrees with the side view; the wing sits about 0.4 m lower than the former fit to the fairing-biased registration.
                dihedral_stations: vec![
                    crate::WingDihedralStation {
                        span_fraction: 0.1000,
                        z_m: -0.8500,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.2000,
                        z_m: -0.5750,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.3000,
                        z_m: -0.3780,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.5000,
                        z_m: 0.0160,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.6500,
                        z_m: 0.3200,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.8000,
                        z_m: 0.5930,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.8800,
                        z_m: 0.7550,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        // The drawn tip edge station is at 0.9302; this front-view corner sat 0.0038 semispans
                        // (0.07 m) outboard of it, a sliver panel the lattice cannot condition. It takes the edge station, same height.
                        span_fraction: 0.9302,
                        z_m: 0.9100,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.9670,
                        z_m: 1.5600,
                        flight_rise_m: None,
                    },
                ],
                // C2 cubic spline through the dihedral, twist and thickness stations.
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                // Drawn tip corners read off the ACP Fig 1 plan and side views; analysis stays on the trapezoid.
                hstab_edge_stations: vec![
                    crate::TailEdgeStation { span_fraction: 0.98, le_offset_m: 0.0, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.99, le_offset_m: 0.03, te_offset_m: -0.03 },
                    crate::TailEdgeStation { span_fraction: 0.995, le_offset_m: 0.14, te_offset_m: -0.05 },
                    crate::TailEdgeStation { span_fraction: 1.0, le_offset_m: 0.29, te_offset_m: -0.10 },
                ],
                vstab_edge_stations: vec![
                    // Fin LE top corner, ACP Fig 1 sheet 1 side view: drawn LE aft of the straight LE line.
                    crate::TailEdgeStation { span_fraction: 0.94, le_offset_m: 0.0, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.96, le_offset_m: 0.03, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.97, le_offset_m: 0.08, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.98, le_offset_m: 0.17, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.99, le_offset_m: 0.35, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 1.0, le_offset_m: 0.48, te_offset_m: 0.0 },
                ],
                tail_airfoil: "naca0012".to_owned(),
                // ACP Issue 013: 29.13 m^2 tailplane area (DM J06-10 Table 6; the
                // 36.6 m^2 of DM 12AAB Table 6 does not fit this span and these
                // chords), 12.263 m span (locator D), tip trailing edge 37.82 m aft of
                // the nose (H) and 4.94 m aft of the root leading edge (V,
                // role read off the drawing), at the previous 0.2895 taper.
                // Apex moved 0.73 m forward with the tip leading edge (36.4 m) and
                // the chords held: the ACP plan view draws a 34.75 deg leading
                // edge, the former apex gave 30.5 deg. Area and span stand.
                // Tail fit: tailplane straight-edge fit to the drawing, centreline values.
                hstab_offset_from_tail_m: 6.31, // ACP Fig 1 plan view: tailplane 0.07 m forward of the former position (tip TE 37.82 m, locator H), the built TE ran 0.09 m aft of the drawn one
                hstab_z_m: 0.660, // ACP Fig 1 side view and crown-registered front view: upper surface z 1.24 m at 0.55 and 1.44 m at 0.85 semispan
                hstab_root_chord_m: 3.43,
                hstab_tip_chord_m: 1.28,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (4.27, 6.1315, 0.770), // Tailplane dihedral, front view
                // Fin measured on the A220 ACP BD500-3AB48-13800-00 Issue 013, DM
                // BD500-A-J06-10-00-00AAA-030A-A Figure 1 sheet 1 side view, page 7: straight
                // lines through the LE and TE stroke centres (rms 0.002 m), horizontal scale from locator A
                // (38.689 m, 10.09-10.10 pt/m), vertical from locator B (3.721 m, 11.03-11.17 pt/m: the sheet
                // is drawn about 1.10 taller than long). Mean of two readings (extension lines at 600 dpi,
                // outline strokes at 800 dpi), spread 0.05 m or less except the height (0.09 m, the two B
                // readings). Root line: the constant-section crown. LE 30.05 m aft of the nose at the crown,
                // swept 43.6 deg; TE swept 18.6 deg; drawn tip TE 37.73 m. Straight-edge trapezoid to the
                // crown 22.9 m^2, against 20.77 m^2 total vertical stabilizer area in DM J06-10 Table 6
                // (reference convention not printed; areas are checks only).
                vstab_offset_from_tail_m: 8.65, // body end 38.70 m less the LE 30.05 m; was 7.69, 8.61 before
                vstab_z_m: 1.9605,
                vstab_root_chord_m: 5.67, // drawn 5.66 / 5.68 m at the crown; was 5.09, 5.64 before
                vstab_tip_chord_m: 1.99, // straight-edge chord at the drawn top; was 1.80, 1.97 before
                vstab_tip_le_m: (5.69, 0.0, 5.97), // drawn top 5.92 / 6.02 m above the crown; was (5.08, 0, 5.89)
                // Dorsal fillet, geometry only (vstab_dorsal_in_aero stays false).
                vstab_dorsal_length_m: Some(1.61), // leaves the body line at x 28.44 m; was 1.66
                vstab_dorsal_height_m: Some(0.99), // meets the straight LE 0.99 m above the crown; was 1.03
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
                // Cabin section: floor depth measured on Airbus A220 ACP issue 013 fig. 6; hold polygon from the cargo compartment section A-A, ACP issue 013 DM 12AAB fig. 8 sheet 3 (PDF p130): 2.24 m between the walls, 1.05 m floor to ceiling, 0.84 m floor, lower corners on a 1.60 m radius, which drop 0.40 m from the wall foot to the floor edge (sqrt(1.60^2 - 0.42^2) - sqrt(1.60^2 - 1.12^2)); the chamfer is that drop. The forward-hold clearance section D-D (J14-20 fig. 3 sheet 3, PDF p199) read before dimensions the box envelope from the ceiling down the side wall (0.42, 0.46, 0.58, 0.71 m), not the floor depth; A-A times the printed 7.03 m forward length gives the 14.8 m3 compartment volume the ACP prints (J26 table 2, PDF p645). The ceiling takes the class floor-beam depth (the hold drawing carries no skin); cabin-datum-evidence.md.
                cabin_section: Some(crate::CabinSectionConfig { cabin_widths: Some(crate::CabinWidthsConfig { armrest_m: 3.277, shoulder_m: 3.135 }), floor_below_crown_m: Some(2.3), hold: Some(crate::HoldSectionConfig { ceiling_below_crown_m: None, height_m: 1.05, floor_width_m: 0.84, side_width_m: 2.24, chamfer_height_m: 0.40 }) }),
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // engines.json carries a V2 nacelle set (nacelle_shape_v2) that agrees with the measured length, diameter and inlet.
                nacelle_model: crate::NacelleModel::V2,
                // ACP Issue 013 locators P, Q and BB: engine centreline
                // P - (P - BB/2 - Q)/2 = 5.44 m outboard (derived, +-0.15 m).
                spanwise_positions_m: vec![5.44, -5.44],
                z_m: -1.270, // Relative to the wing LE height at the engine. Axis z -1.62 m: ACP Fig 1 front and side views draw -1.71 m, printed clearance G 0.582 m gives -1.57 m
                // Locator K, ACP Table 5: inlet lip 479.0 in = 12.17 m aft of the nose; plan view lip agrees (side view reads 12.35 m).
                inlet_x_offset_m: 3.6810, // LE at the engine 15.851 m less the lip station
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
            // Superseded at registration: the profile declares the TCDS VMO/MMO, so VD is the
            // CS 25.335(b) value on VC = VMO (`crate::design_speeds`, `registry`).
            dive_speed_m_s: 1.25 * 330.0 * 0.514_444,
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
