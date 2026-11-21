// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airbus A380-800 preset.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MissingDesignMissionDatum, MissionEvidenceApplicability,
    PartialDesignMissionEvidence, PartialMissionEvidenceKind, PublishedRange,
    PublishedReserveContract, WingConfig,
};

/// Double-deck quad, the largest airliner in the registry.
pub fn a380_800() -> AircraftPreset {
    AircraftPreset {
        name: "A380-800",
        display_name: "Airbus A380-800",
        class: crate::presets::AircraftClass::Transport,
        description: "Airbus A380-841 WV000 with Trent 970-84 engines.",
        identity: AircraftVariantIdentity {
            model: "A380-841",
            weight_variant: "WV000",
            engine_model: "Trent 970-84",
            modification_state: "WV000 public planning baseline",
            tank_configuration: "323,546 L tanks + 793 L usable system inventory",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Vol. I Table 1-1 (aerodrome reference code) applied to the
            // preset wingspan of 79.75 m (Airbus A380 Aircraft Characteristics):
            // 65 m <= b < 80 m is code F.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::F),
            mrw_kg: Some(562_000.0),
            mtow_kg: Some(560_000.0),
            mlw_kg: Some(386_000.0),
            mzfw_kg: Some(361_000.0),
            // EASA TCDS EASA.A.110 Issue 17, 2026-08-05, section 3.3 "Fluid
            // Capacities", p.14 of 20: 324,339 L usable and 1,086 L unusable
            // at the sheet's 0.800 kg/L (324,339 x 0.800 = 259,471 kg). The
            // aeroplane total stands here rather than the 323,546 L tank
            // total, because the 793 L difference is the same table's
            // "Systems" row - usable fuel in lines and engines, not in a tank
            // - and this block is the certified aircraft record that
            // docs/aircraft-parity.md compares the model against and that
            // supplies the FLOPS maximum fuel capacity, which the A380
            // declares nowhere else (`preset_flops::inputs_for`). The tanks
            // themselves now carry the same table's certified per-tank
            // volumes and sum to exactly 323,546 L, so the residual is that
            // system inventory and nothing else; the derivation and the
            // certified 0.00335 unusable fraction are in
            // `preset_fuel_tanks::layout_for`.
            // Airbus A380 Aircraft Characteristics, Rev Dec 01/23, sec. 2-1-1 p.2: usable fuel 323,546 L =
            // 253,983 kg at the page's own density 0.785 kg/L (the TCDS tank sum is the same 323,546 L; its
            // 0.800 kg/L convention and 793 L systems inventory are not mixed into this pair).
            usable_fuel_volume_l: Some(323_546.0),
            usable_fuel_mass_kg: Some(253_983.0),
            fuel_density_kg_l: Some(0.785),
            reference_wing_area_m2: Some(845.0),
            planning_seats: Some(555),
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: None,
                cruise: None,
                range_nmi: 6_535.0,
                payload_kg: Some(83_800.0),
                source: "Airbus A380 Aircraft Characteristics (2025-12) section 3-2-1 Figure 3-2-1-991-001-A01 (payload/range ISA, Trent 900): maximum-structural-payload corner read as 6,535 nmi at 83.8 t; read uncertainty +-60 nmi, +-0.5 t; reserves 5 % trip allowance, 30 min hold, 200 nmi diversion",
                reserve: Some(crate::presets::DesignPointReserve {
                    scheme: Some(crate::FuelScheme::StudyConvention),
                    contingency_trip_fraction: Some(0.05),
                    contingency_minimum_hold_min: Some(0.0),
                    alternate_distance_nmi: Some(200.0),
                    final_reserve_hold_min: Some(30.0),
                    flag_flight_time_fraction: None,
                    holding_altitude_ft: None,
                    source: "Airbus A380 Aircraft Characteristics section 3-2-1 Figure 3-2-1-991-001-A01: typical international flight profile, 5% trip fuel allowance, 30 min holding, 200 nm diversion; the holding height is not stated, so it is the configured policy's",
                }),
            }),
            certified_max_seats: Some(868),
            certified_exit_layout: Some(crate::presets::A380_800_EXIT_LAYOUT),
            partial_design_mission_evidence: vec![PartialDesignMissionEvidence {
                kind: PartialMissionEvidenceKind::PayloadRangeChart,
                range: None,
                payload_kg: None,
                load_case: None,
                profile_assumptions: Some(
                    "ISA, no wind, labeled typical international flight profile",
                ),
                reserve_assumptions: Some(
                    "200 nm diversion; 5% trip fuel allowance; 30 min holding",
                ),
                reserve_contract: Some(PublishedReserveContract {
                    diversion_range: Some(PublishedRange::NauticalMiles(200.0)),
                    trip_fuel_allowance_fraction: Some(0.05),
                    holding_time_minutes: Some(30.0),
                }),
                applicability: "A380-800 Trent 900 chart matches the model and engine family but selects no weight-variant design point",
                configuration_applicability: MissionEvidenceApplicability::ModelAndEngineFamily,
                missing: vec![
                    MissingDesignMissionDatum::Range,
                    MissingDesignMissionDatum::Payload,
                    MissingDesignMissionDatum::Profile,
                    MissingDesignMissionDatum::ReserveFuel,
                ],
                source: "Airbus A380 Aircraft Characteristics Rev 20, 2025-12-01, section 3-2-1 p.2, Figure 3-2-1-991-001-A01",
            }],
            // Design mission, one provenance row per datum: the maximum-structural-payload
            // corner of the A380-800 Trent 900 chart, whose notes print the reserve rule.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "ISA, no wind, typical international flight profile, 5 % trip fuel allowance, 30 min holding, 200 nm diversion; maximum-structural-payload corner of the Trent 900 chart read as 6,535 nmi (+-40 nmi)",
                    "Airbus A380 Aircraft Characteristics Rev 20, 2025-12-01, section 3-2-1 p.2, Figure 3-2-1-991-001-A01 (payload/range, ISA, Trent 900)",
                )),
                payload: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "maximum structural payload plateau of the same chart, 83.8 t (+-0.5 t), at the corner range",
                    "Airbus A380 Aircraft Characteristics Rev 20, 2025-12-01, section 3-2-1 p.2, Figure 3-2-1-991-001-A01",
                )),
                profile: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "ISA, no wind, typical international flight profile, long-range cruise Mach 0.85; the documents print no climb or descent schedule, so the model's configured schedules apply to those phases",
                    "Airbus A380 Aircraft Characteristics Rev 20, section 3-2-1 Figure 3-2-1-991-001-A01 (atmosphere and profile); Airbus A380 Facts and Figures, February 2022, p.3 (cruise Mach)",
                )),
                reserve_fuel: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "5 % trip fuel allowance, 30 min holding, 200 nm diversion; the holding height is not printed, so it is the configured policy's; the reserve mass follows from the modelled trip",
                    "Airbus A380 Aircraft Characteristics Rev 20, section 3-2-1 Figure 3-2-1-991-001-A01 notes",
                )),
            },
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            aft_cg_nose_load: Some(crate::presets::gear_load::A380_800),
            sources: vec![
                "EASA.A.110 Issue 17, 2026-08-05, pp.10-15",
                "Airbus A380 Aircraft Characteristics Rev 20, 2025-12-01, section 2-1-1",
                "Airbus A380 Facts and Figures, February 2022, p.3",
            ],
            high_speed_protection: Some(
                crate::presets::operating_limits::A380_HIGH_SPEED_PROTECTION,
            ),
            operating_speed_limits: Some(crate::presets::operating_limits::A380_800),
            ..AircraftReferenceData::default()
        },
        engine_name: "Trent 970-84",
        n_engines: 4,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 4,
            // AC 2-9-0: 4-wheel WLG + 6-wheel BLG bogies, kept as sourced.
            mlg_strut_bogie_wheels: Some(vec![4, 4, 6, 6]),
            wheels_per_mlg_strut: 0,
            // Main tire: Goodyear application chart PDF p.17 lists A380 main 1400x530R23 40 PR; Michelin Engineering Data printed p.49 rates it.
            mlg_tire: "1400x530R23/40PR".to_owned(),
            nlg_tire: "1270x455R22/32PR".to_owned(),
            track_diameter_factor: 14.34 / 7.14,
            // 14.34 m wing-gear track; 28.61 m NLG-WLG wheelbase, distinct from
            // the 31.88 m NLG-BLG body-gear wheelbase below.
            reference_wheelbase_m: Some(28.61),
            reference_body_wheelbase_m: Some(31.88),
            reference_track_m: Some(14.34),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(70.4),
            reference_nlg_x_fraction: Some(4.97 / 70.4),
            reference_mlg_x_fractions: Some(vec![
                33.58 / 70.4,
                33.58 / 70.4,
                36.85 / 70.4,
                36.85 / 70.4,
            ]),
            // Airbus A380 AC Rev 20, Figure 2-3-0-991-001-A01 (ground
            // clearances, MRW, aft CG 41 %MAC): fuselage F1 2.38 m, and the
            // F2/F3 fuselage tops 10.79 m less the 8.41 m body height.
            fuselage_ground_clearance_m: Some(2.38),
            // No published tail-strike attitude backs this preset's aft-fuselage
            // geometry, so its model tail-down angle is unvalidated (the generic
            // tailcone loft understates it): keep the Raymer/Roskam 15 deg floor on
            // top of the tail-down criterion.
            min_tip_back_deg: 15.0,
            // Tail-scrape requirement: no published tail-strike attitude found (A380 AC Rev 20 prints none; only a
            // roll-dependent value in an accident report), so the 10 deg default (an assumption) stands.
            trimmable_stabilizer: true,
            // FAA TCDS A58NM (Rev 11, pp.4-5, "Control Surface Movements"): THS +2/-10 deg
            // (maximum operational, computer limits; nose up printed negative), elevators +20/-30 deg,
            // positive = trailing edge down (nose-up limit 10, up travel 30).
            stabilizer_nose_up_travel_deg: Some(10.0),
            elevator_up_travel_deg: Some(30.0),
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 79.75,
            // Airbus A380 AC Rev 20, Fig. 2-2-0-991-001-A01 sheet 2 (plan view): a 3.98 m streamwise tip chord (leading
            // edge 46.97 m aft of the nose) and a 17.67 m chord at the side of the body.
            root_chord_m: 19.678256676269644, // datum: inboard drawn edges carried to y = 0 (19.7 m); not a section of the aircraft
            break_chord_m: 12.05, // same sheet, vector outline: leading edge 28.18 m, trailing edge 40.23 m at the kink; the published reference wing keeps 845 m^2 (was 11.62, whose short kink forced the trailing-edge bump)
            tip_chord_m: 3.98, // chord the straight edges reach at the tip (the extreme tip chord is the rake chord below)
            // Airbus A380 AC Rev 20, Fig. 2-2-0-991-001-A01 sheet 2, vector outline: outboard leading edge 28.18 m at the
            // kink to 46.96 m at the tip (printed 46.97 m). Its quarter-chord sweep is 33.3 deg
            // against the 33.5 deg of Airbus A380 Facts and Figures (2022).
            sweep_deg: 36.31,
            tip_twist_deg: -2.5,
            wing_x_shift_m: -7.5,
            tail_scale: 1.0,
            // AC 2-2-0: 70.40 m is the printed body length to the blunt tail cut; the published 72.72 m runs to the fin tip trailing point.
            fuselage_length_m: 70.4,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        speed_schedule_source: crate::presets::SpeedScheduleSource::SourcedLadder,
        airfoil_class_source: "Design-era declaration: the A380-800 (EIS 2007) wing is of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC(2)-0414 root and SC(2)-0610 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values; the drawn planform stays the geometry.
                // S 845 m2: Airbus A380 reference area (reference_wing_area_m2 of this preset); MAC 12.295 m and LEMAC 28.765 m: joint fit of 30 section 7
                // pavement-load rows (derived, not a TCDS figure).
                reference_wing: crate::ReferenceWingConfig::published(845.0, Some(12.295), Some(28.765)),
                // Airbus A380 AC Rev 20, Fig. 2-2-0-991-001-A01 sheet 2, vector outline: inboard leading edge 39.55 deg
                // through 20.80 m at y 5.39 m. The kink and outboard wing move 0.09 m aft onto the drawn lines. The
                // quarter-MAC point of the drawn planform differs from the published 31.839 m statics station: the reference wing above keeps the published values.
                inboard_le_sweep_deg: Some(39.55),
                root_datum_x_m: 23.85,
                // Ground shape: the unloaded wing of AC 2-2-0 sheet 1 (front view), which the AC 2-3-0 MRW table follows by the computed droop. Tip height: unloaded fence
                // bottom about 6.6 m above the drawn ground (W2 5.21 m in the MRW table is the loaded value).
                root_z_m: -2.5,
                break_z_m: -0.3399,
                tip_z_m: 1.4600,
                flight_tip_rise_semispan_fraction: Some(0.0005), // same front view: wing centre at the fence 1.48 m on the fuselage datum; the only rise of this wing, linear in span. The "over 4m" take-off flex of Airbus A380 Facts and Figures, February 2022, p.4 is ground droop plus 1 g: validation only (alas-struct flex_check, alas-mass ground_droop_validation), never lofted
                root_twist_deg: 4.5,
                break_twist_deg: 2.0,
                break_span_fraction: 0.33,
                // The plan view's 17.67 m chord at the side of the 7.14 m
                // wide body (Figure 2-2-0-991-001-A01 sheet 2).
                side_of_body_span_fraction: Some(3.57 / 39.875),
                side_of_body_chord_ratio: Some(0.8985), // 17.68 m (printed 17.67 m) on the drawn inboard leading-edge line, over the root datum; the fillet is the edge station below
                kink_span_fraction: Some(0.359_236_516_064_625_5),
                outboard_sweep_decrement_deg: 2.5,
                // Airbus publishes neither the A380 sections nor its twist, so
                // the camber distribution follows the NASA Common Research
                // Model, a public M 0.85 widebody wing designed for CL 0.5
                // (Vassberg, DeHaan, Rivers and Wahls, "Development of a
                // Common Research Model for Applied CFD Validation Studies",
                // AIAA 2008-6919; NASA CRM geometry page, max-camber and twist
                // figures). Its camber rises from about 0 % at the root to
                // 1.6 % outboard while the twist washes out 10.5 deg, and its
                // published eta 0.65 section (crm.eta65.unswept31.5deg) has a
                // thin-airfoil zero-lift angle of -4.4 deg streamwise. A
                // design-lift-0.7 root with a design-lift-0.4 tip inverted
                // that, adding 3.2 deg of camber washout to this preset's
                // twist. The NASA SC(2) family names its design lift in the
                // first two digits and its thickness in the last two (Harris,
                // NASA TP-2969, 1990): the root keeps 14 % and the tip 10 %,
                // and only their design lift is exchanged, giving thin-airfoil
                // zero-lift angles of -3.0 deg root and -4.1 deg tip.
                root_airfoil: "sc20414".to_owned(),
                tip_airfoil: "sc20610".to_owned(),
                airfoil_class: crate::AirfoilClass::SupercriticalAdvanced,
                // Thickness: root and side of body from the NASA CRM (Vassberg et al., AIAA 2008-6919, Table 2); outboard from the depth of the wing in
                // the AC 2-2-0 sheet 1 front view over the drawn chord at the same span (t/c 0.142 side of body, 0.120 y 7 m, about 0.088 kink to tip; the CRM kink value 0.105 was 0.2 m too deep).
                thickness_stations: vec![
                    crate::WingThicknessStation {
                        span_fraction: 0.0000,
                        thickness_scale: 1.0999, // t/c 0.1400 -> 0.1540
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.0895,
                        thickness_scale: 0.9856, // t/c 0.1400 -> 0.1380
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.1750,
                        thickness_scale: 0.8000, // t/c 0.1400 -> 0.1120
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.3592,
                        thickness_scale: 0.6070, // t/c 0.1400 -> 0.0850
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.6500,
                        thickness_scale: 0.7190, // t/c 0.1217 -> 0.0875
                    },
                    crate::WingThicknessStation {
                        span_fraction: 1.0000,
                        thickness_scale: 0.9230, // t/c 0.0997 -> 0.0920
                    },
                ],
                // Heights fitted to the centre of the wing depth in the AC 2-2-0 sheet 1 front view (upper line 6.3 deg inboard, 3.2 deg outboard of the inner engine).
                // The drawn and tabulated nacelle heights are the static ones: keep them in the flight shape.
                nacelles_hold_ground_height: true,
                dihedral_stations: vec![
                    crate::WingDihedralStation { span_fraction: 0.2000, z_m: -1.2694, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.3000, z_m: -0.6283, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.5000, z_m: 0.1678, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.6500, z_m: 0.5712, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.8000, z_m: 0.9139, flight_rise_m: None },
                ],
                // Airbus A380 AC Rev 20, Fig. 2-2-0-991-001-A01 sheet 2, vector outline. The leading edge leaves its line at y 38.27 m and curves aft to 49.00 m at the tip; the trailing edge
                // stays one line, so the extreme chord is 1.95 m. Offsets are from the lines through the stations.
                tip_rake_tip_chord_m: Some(1.95),
                tip_rake_start_span_fraction: Some(0.9596),
                // Same outline: the trailing edge breaks 15.1 -> 21.9 deg at y 11.53 m and 21.9 -> 23.6 deg at y 26.33 m, and runs straight between (the earlier 0.34 m offset
                // at 0.39 semispan read a flap-track fairing). -0.197 m puts the second break on the drawn line.
                te_break_span_fraction: Some(0.6603),
                te_break_chord_offset_m: Some(-0.197),
                // Same outline: leading-edge fillet from 18.54 m at the body side (y 3.57 m) to the inboard line at y 5.40 m, the inboard trailing-edge break, and the tip curve.
                edge_stations: vec![
                    crate::WingEdgeStation { span_fraction: 3.57 / 39.875, le_offset_m: -0.76, te_offset_m: 0.0 },
                    crate::WingEdgeStation { span_fraction: 0.1095, le_offset_m: -0.20, te_offset_m: -0.02809 },
                    crate::WingEdgeStation { span_fraction: 0.1354, le_offset_m: 0.0, te_offset_m: -0.06451 }, // fillet closes on the LE line at y 5.40 m; a reading of -0.01 m tilted the edge line off the datum root
                    crate::WingEdgeStation { span_fraction: 0.2891, le_offset_m: 0.0, te_offset_m: -0.28068 },
                    crate::WingEdgeStation { span_fraction: 0.9596, le_offset_m: 0.0, te_offset_m: -0.02343 }, // rake start: TE on the straight line from the second break to the tip
                    crate::WingEdgeStation { span_fraction: 0.9697, le_offset_m: -0.46, te_offset_m: -0.01757 },
                    crate::WingEdgeStation { span_fraction: 0.9800, le_offset_m: -0.74, te_offset_m: -0.01160 },
                    crate::WingEdgeStation { span_fraction: 0.9900, le_offset_m: -0.76, te_offset_m: -0.00580 },
                ],
                // C2 cubic spline through the dihedral, twist and thickness stations.
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                // Wingtip fence, AC 2-2-0 sheet 1 front view: 1.20 m above and 1.11 m below the wing at the tip, about 0.12 m thick (NACA 0006 on the 1.95 m tip chord); sheet 1 side view: leading edges swept back about 64 deg above and 57 deg below the tip (one field: 62 deg) to end chords 0.42 and 0.33 m, top trailing edge at the printed 52.07 m.
                // A fence has no lateral reach, so the 79.75 m span is the wing span.
                wingtip_device: Some(crate::WingtipDevice {
                    kind: crate::WingtipDeviceKind::Fence,
                    height_m: 1.20,
                    down_height_m: 1.11,
                    cant_deg: 0.0,
                    root_chord_m: None,
                    root_te_flare: None,
                    tip_chord_m: 0.45,
                    le_sweep_deg: 62.0,
                    toe_out_deg: 0.0,
                    blend_radius_m: 0.0,
                    arc_start_deg: None,
                    path_stations: Vec::new(),
                    airfoil: Some("naca0006".to_owned()),
                    span_includes_device: false,
                    include_in_vlm: true,
                }),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                // Drawn tip edges read off the Airbus A380 Aircraft Characteristics tail views; analysis stays on
                // the trapezoid.
                hstab_edge_stations: vec![
                    crate::TailEdgeStation {
                        span_fraction: 0.92,
                        le_offset_m: 0.0,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.94,
                        le_offset_m: 0.3,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.96,
                        le_offset_m: 1.0,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.98,
                        le_offset_m: 2.1,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 1.0,
                        le_offset_m: 3.45,
                        te_offset_m: 0.0,
                    },
                ],
                vstab_edge_stations: vec![
                    // AC 2-2-0 sheet 1 (side view): dorsal fillet, drawn leading edge 0.45 m ahead of the straight line at the root, 0.33 m at 0.8 m height, merging at 2 m
                    crate::TailEdgeStation { span_fraction: 0.01, le_offset_m: -0.45, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.055, le_offset_m: -0.33, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.14, le_offset_m: 0.0, te_offset_m: 0.0 },
                    crate::TailEdgeStation {
                        span_fraction: 0.96,
                        le_offset_m: 0.0,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.98,
                        le_offset_m: 0.2,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.99,
                        le_offset_m: 0.4,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.995,
                        le_offset_m: 0.68,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 1.0,
                        le_offset_m: 1.08,
                        te_offset_m: 0.0,
                    },
                ],
                tail_airfoil: "naca0010".to_owned(), // AC 2-2-0 sheet 1 front view: HTP and fin depth over chord about 0.10 (was naca0012)
                // Figure 2-2-0-991-001-A01 sheet 2 of 2, page 2-2-0 Page 3 (plan
                // view): tailplane tip leading edge 68.85 m aft of the nose,
                // 11.57 m aft of the root leading edge (57.28 m), and a printed
                // 3.72 m (12.2 ft) tip chord whose trailing edge closes the 72.57 m
                // plan length (68.85 + 3.72). Root chord and height are estimates.
                // Tail fit: tailplane straight-edge fit to the drawing, centreline values.
                hstab_offset_from_tail_m: 13.09,
                hstab_z_m: 1.05, // AC 2-2-0 sheet 1: side outboard lines and front surfaces together
                // Root chord 9.75 m: centreline value of the straight-edge fit to the sheet 2 tailplane (not dimensioned, an estimate). Tip chord: the printed 3.72 m above (an unsourced 3.80 m is not used).
                hstab_root_chord_m: 9.75,
                hstab_tip_chord_m: 3.72,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                // Tailplane span 30.37 m (99.64 ft), so a 15.185 m tip
                // station: Airbus A380 Aircraft Characteristics - Airport and
                // Maintenance Planning, Revision 20 Dec 01/25, Subject 2-2-0
                // General Aircraft Dimensions, FIGURE-2-2-0-991-001-A01 Sheet
                // 1 of 2, page 2-2-0 Page 2. In that front elevation the
                // dimension's extension lines terminate on the tailplane tips,
                // between the 79.75 m wing span above it and the 7.14 m
                // fuselage width below it; the figure is drawn to scale and
                // this preset already matches both of those. The previous
                // 12.5 m tip gave a 25.0 m tailplane, 17.7 % narrower than the
                // published surface. The trapezoidal area is
                // (9.75 + 3.72) x 15.185 = 204.54 m^2.
                hstab_tip_le_m: (11.57, 15.185, 1.70), // same centre line (6.2 deg)
                // Airbus A380 AC Rev 20, Fig. 2-2-0-991-001-A01 sheet 1 (side view), vector outline at the printed scale (72.73 m to the fin
                // tip trailing edge): root leading edge 53.94 m, root chord printed 12.06 m (12.07 m drawn), height printed 14.59 m, straight leading edge reaching 68.04 m, trailing edge
                // 72.75 m at the top. The drawn tip rounds off behind that corner and the builder has no fin tip shape, so the top leading edge sits 0.11 m aft of the corner. The fin top stands 24.12 m above ground in Fig. 2-3-0-991-001-A01.
                vstab_offset_from_tail_m: 16.46,
                vstab_z_m: 3.245,
                vstab_root_chord_m: 12.07,
                vstab_tip_chord_m: 4.60,
                vstab_tip_le_m: (14.21, 0.0, 14.59),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 7.14,
                // The one non-circular body in the registry: two decks make it
                // taller than it is wide.
                height_m: Some(8.41),
                nose_z_m: -1.02,
                cabin_start_x_m: 7.0,
                cabin_z_m: 0.3,
                tailcone_length_m: 12.67,
                tail_z_m: 2.0,
                // FUSELAGE-G2 shaped nose and aft body. Outline traced from the manufacturer drawing
                // (side: same 2-2-0 p2; top: Airbus A380 AC (Dec 01/25) 2-2-0 p3),
                // calibrated to the drawn length; the nose law (windshield, radome, keel and plan
                // exponents) is a least-squares fit to the traced upper, lower and plan lines (RMS 0.132 m),
                // the aft law (belly ramp, crown drop, plan taper, closing section) to the traced aft lines
                // (RMS 0.17 m). Cabin start, tailcone length and every payload and mass station are unchanged:
                // only the outer loft moves. Section exponent 2.0 (ellipse): no drawing measures it.
                nose_length_m: Some(13.16),
                nose_windshield_angle_deg: Some(33.1),
                nose_crown_end_fraction: Some(0.87),
                nose_radome_length_fraction: Some(0.1),
                nose_keel_exponent: Some(4.0),
                nose_plan_exponent: Some(1.89),
                nose_section_exponent: Some(2.0),
                belly_upsweep_length_m: Some(24.64),
                aft_belly_exponent: Some(1.4),
                tail_end_z_m: Some(2.21),
                tail_end_width_m: Some(0.1),
                tail_end_height_m: Some(0.47),
                aft_plan_length_m: Some(20.44),
                aft_plan_exponent: Some(1.38),
                aft_crown_length_m: Some(17.15),
                aft_crown_exponent: Some(1.08),
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // engines.json carries a V2 nacelle set (nacelle_shape_v2) that agrees with the measured length, diameter and inlet.
                nacelle_model: crate::NacelleModel::V2,
                // Figure 2-2-0-991-001-A01 sheet 1 (front view): engine
                // centrelines 29.6 m and 51.4 m apart.
                spanwise_positions_m: vec![14.8, -14.8, 25.7, -25.7],
                // Figure 2-3-0-991-001-A01 (MRW, aft CG): nacelle low points
                // 1.08 m (N1) and 1.90 m (N2) above the ground under the
                // 1.8 m nacelle radius, on the ground-shape wing above. One
                // offset serves both pairs, so they land at 1.14 m and
                // 1.84 m.
                // Low point held with the drawn 1.83 m radius (nacelle change: -0.872 ->
                // -0.842), plus the wing change's +0.022 m (thinner raked tip).
                // Front-view circle fit puts the outer pair (y 25.7 m) 0.70 m and the inner pair 0.10 m above the model; one z_m
                // serves both pairs, so raised 0.30 m (outer 0.4 m low, inner 0.2 m high; the published N1/N2 low points 1.08/1.90 m are a MRW aft-CG clearance, now 1.44/2.14 m static). was -0.820
                z_m: -0.520,
                // Inboard and outboard pairs fitted separately to the drawn UNLOADED nacelles (low points 1.14 m and 2.31 m above the drawn ground line; the loaded table is not the target). Order of spanwise_positions_m.
                z_per_engine_m: vec![-2.9063, -2.9063, -2.5869, -2.5869],
                inlet_x_offset_m: 6.41, // mean of the pairs below
                inlet_x_per_engine_m: vec![6.26, 6.26, 6.56, 6.56], // AC 2-2-0 sheet 2 (plan view): nacelle forward faces at the printed 22.23 m and 29.94 m (larger value = further forward)
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.85,
            cruise_altitude_m: 11887.2,
            mtow_kg: 560_000.0,
            max_wing_area_m2: 845.0,
            min_wing_loading_kg_m2: 450.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 555,
            cargo_payload_kg: 150_000.0,
            // Maximum zero-fuel weight about 361 t less an operating empty
            // weight of about 277 t.
            max_structural_payload_kg: 84_000.0,
            // Superseded at registration: CS 25.335(b)(1)(ii) from the secondary-source
            // VMO/MMO (`operating_limits`).
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("advanced_highlift_widebody"),
    }
}
