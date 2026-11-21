// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! COMAC C919-100 standard-range preset with LEAP-1C28 engines.
//!
//! Source map and its limits. The geometry comes from the COMAC C919 Aircraft
//! Characteristics for Airport Planning (ACAP), C919-SVV19-50009-00 Issue 003,
//! 02-02 Figure 1, a vector three-view (4.752 pt/m in plan and side, 4.894 pt/m in
//! front). Weights, cabin and range come from the specification table of the
//! Wikipedia C919 article (English and Chinese editions, retrieved 2026-10-05), a
//! SECONDARY compilation whose weight rows cite COMAC's 2023-01-11 delivery release
//! (`english.comac.cc/news/latest/202301/11/t20230111_7355061.shtml`) and Aviation
//! Week (11 Feb 2020). The CAAC type-certificate data sheet is not public and no
//! EASA type-certificate data sheet exists for the airframe. The engine is sourced
//! to the ICAO Aircraft Engine Emissions Databank v32 (EASA, 2026), UID 08P28CM150
//! (LEAP-1C28), the CAAC-certified LEAP-1C family sharing the EASA.E.110
//! LEAP-1A/-1C type design with a 78 in fan.

use super::exit_layouts::C919_CERTIFIED_EXIT_LAYOUT;
use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, MissingDesignMissionDatum, MissionEvidenceApplicability,
    PartialDesignMissionEvidence, PartialMissionEvidenceKind, PublishedRange, SourcedPlanningCabin,
    SourcedSeatClass, WingConfig,
};

/// The Wikipedia table's lower seat figure for the C919-100: 158 seats in two
/// classes, 8 business and 150 economy. Pitch is an ESTIMATE: no retained
/// source prints it (carrier cabins of this size are typically 30-32 in
/// economy and about 38 in business).
pub const C919_PLANNING_CABIN: SourcedPlanningCabin = SourcedPlanningCabin {
    classes: &[
        SourcedSeatClass {
            class: "Business",
            seats: 8,
            pitch_m: Some(38.0 * 0.0254),
            abreast: Some(4),
            width_m: None,
        },
        SourcedSeatClass {
            class: "Economy",
            seats: 150,
            pitch_m: Some(31.0 * 0.0254),
            abreast: Some(6),
            width_m: None,
        },
    ],
    source: "Wikipedia, Comac C919, Specifications table, seats 158 (8J + 150Y) to 192 (one-class high density), retrieved 2026-10-05 (secondary); seat pitch 38 in business and 31 in economy are ESTIMATES, not in any retained source",
};

/// The C919-100 STD at 75,100 kg with LEAP-1C28 engines.
pub fn c919() -> AircraftPreset {
    AircraftPreset {
        name: "C919",
        display_name: "COMAC C919",
        class: crate::presets::AircraftClass::Transport,
        description: "COMAC C919-100 standard-range at 75,100 kg MTOW with LEAP-1C28 engines.",
        identity: AircraftVariantIdentity {
            model: "C919-100 STD",
            weight_variant: "75,100 kg MTOW (standard range)",
            engine_model: "LEAP-1C28",
            modification_state: "production aircraft as described by the public COMAC specification; modification state not published",
            tank_configuration: "wing integral tanks, 19,560 kg usable (24,917 L)",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Vol. I Table 1-1 applied to the 35.8 m span over
            // the winglets (Wikipedia specification table): 24 m <= b < 36 m
            // is code C.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::C),
            // Wikipedia specification table (secondary; weight rows cite the
            // COMAC 2023-01-11 delivery release): STD MTOW 75,100 kg, MLW
            // 67,800 kg (Chinese edition). The maximum taxi weight is not
            // printed for the STD and is left unset.
            mrw_kg: None,
            mtow_kg: Some(75_100.0),
            mlw_kg: Some(67_800.0),
            mlw_secondary_source: Some(
                "Wikipedia, Comac C919, Specifications (Chinese edition), weight rows citing the COMAC 2023-01-11 delivery release; no type certificate or airport-planning document publishes the MLW",
            ),
            // DERIVED, not published: OEW 45,700 kg + maximum payload
            // 18,900 kg (both from the same secondary table).
            mzfw_kg: Some(64_600.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("C919"),
            // Wikipedia table: fuel capacity 24,917 L; the Chinese edition
            // gives 19,560 kg maximum fuel, a density of 0.785 kg/L.
            usable_fuel_volume_l: Some(24_917.0),
            usable_fuel_mass_kg: Some(19_560.0),
            fuel_density_kg_l: Some(19_560.0 / 24_917.0),
            // Lower seat figure of the table's 158 (8J + 150Y).
            planning_seats: Some(158),
            // COMAC ACAP DM 03-02 Table 1, C919 STD standard-payload point.
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: None,
                cruise: None,
                range_nmi: 2_235.0,
                payload_kg: Some(15_010.0),
                source: "COMAC C919 ACAP C919-SVV19-50009-00 (R9, 2025-10-17), DM 03-02 Payload/Range Table 1: C919 STD standard payload 15,010 kg (158 x 95 kg) at 2,235 nmi, ISA, no wind, MTOW 75,100 kg; Figure 1 conditions box: cruise Mach 0.785 at 11,277 m (37,000 ft), reserve fuel 200 nm + 30 min + 5 % main-leg fuel",
                reserve: Some(crate::presets::DesignPointReserve {
                    scheme: Some(crate::FuelScheme::StudyConvention),
                    contingency_trip_fraction: Some(0.05),
                    contingency_minimum_hold_min: Some(0.0),
                    alternate_distance_nmi: Some(200.0),
                    final_reserve_hold_min: Some(30.0),
                    flag_flight_time_fraction: None,
                    holding_altitude_ft: None,
                    source: "COMAC C919 ACAP C919-SVV19-50009-00 (R9), DM 03-02 Figure 1 conditions box: 'RESERVE FUEL: 200NM+30MIN+5% MAIN LEG FUEL'; the holding height is not printed, so it is the configured policy's",
                }),
            }),
            // COMAC C919-SVV19-50009-00 ACAP 02-01 Table 1: maximum seating capacity 174 (certificated or anticipated for certification).
            certified_max_seats: Some(174),
            certified_exit_layout: Some(C919_CERTIFIED_EXIT_LAYOUT),
            planning_cabin: Some(C919_PLANNING_CABIN),
            partial_design_mission_evidence: vec![PartialDesignMissionEvidence {
                kind: PartialMissionEvidenceKind::AdvertisedRange,
                range: Some(PublishedRange::NauticalMiles(2_200.0)),
                payload_kg: None,
                load_case: None,
                profile_assumptions: None,
                reserve_assumptions: None,
                reserve_contract: None,
                applicability: "COMAC advertises 4,075 km (2,200 nmi) for the standard-range variant; payload, profile and reserves behind it are not stated",
                configuration_applicability: MissionEvidenceApplicability::ModelAndEngineFamily,
                missing: vec![
                    MissingDesignMissionDatum::Payload,
                    MissingDesignMissionDatum::Profile,
                    MissingDesignMissionDatum::ReserveFuel,
                ],
                source: "Wikipedia, Comac C919, Specifications and Variants (secondary, citing COMAC), retrieved 2026-10-05",
            }],
            // No public CG envelope or weight-and-balance document.
            // Design mission, one provenance row per datum: the ACAP standard-payload point
            // of the C919 STD (Table 1), whose figure box prints the cruise and reserve rule.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "C919 STD, ISA, no wind, MTOW 75,100 kg, OEW 45,700 kg, cruise Mach 0.785 at 11,277 m (37,000 ft), reserve fuel 200 nm + 30 min + 5 % main-leg fuel: 2,235 nmi at the 15,010 kg standard payload",
                    "COMAC C919 ACAP C919-SVV19-50009-00 (R9, 2025-10-17), DM 03-02 Payload/Range, Table 1 and Figure 1 conditions box",
                )),
                payload: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "standard payload 15,010 kg = 158 passengers (mixed-type maximum seating, DM 02-01) at 95 kg",
                    "COMAC C919 ACAP C919-SVV19-50009-00 (R9), DM 03-02 Table 1; DM 02-01 page 2 (seating)",
                )),
                profile: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "ISA, cruise Mach 0.785 at 11,277 m (37,000 ft); the ACAP prints no climb or descent schedule, so the model's configured schedules apply to those phases",
                    "COMAC C919 ACAP C919-SVV19-50009-00 (R9), DM 03-02 Figure 1 conditions box",
                )),
                reserve_fuel: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "200 nm alternate + 30 min hold + 5 % of main-leg fuel; the holding height is not printed, so it is the configured policy's; the reserve mass follows from the modelled trip",
                    "COMAC C919 ACAP C919-SVV19-50009-00 (R9), DM 03-02 Figure 1 conditions box",
                )),
            },
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            // Wikipedia specification table: wing area 129.15 m^2
            // (secondary; other press figures near 129.8 m^2).
            reference_wing_area_m2: Some(129.15),
            sources: vec![
                "Wikipedia, Comac C919 (English, https://en.wikipedia.org/wiki/Comac_C919), Specifications: length 38.9 m, span 35.8 m over winglets, height 11.95 m, fuselage 3.96 m wide x 4.166 m high, wing area 129.15 m2, cargo 45.2 m3, OEW 45,700 kg, maximum payload 18,900 kg, MTOW 75,100 kg (STD), fuel 24,917 L, LEAP-1C28 29,220 lbf, cruise M 0.785, ceiling 39,800 ft, fan 78 in; retrieved 2026-10-05; SECONDARY",
                "Wikipedia, COMAC C919 Chinese-language article (zhwiki), specification table: MLW 67,800 kg, maximum fuel 19,560 kg, full-payload range 2,200 nmi (STD), LEAP-1C28 129.98 kN; retrieved 2026-10-05; SECONDARY",
                "COMAC press release 2023-01-11, english.comac.cc/news/latest/202301/11/t20230111_7355061.shtml and Aviation Week 2020-02-11 (the sources cited by the table's weight rows; not retrieved directly)",
                "ICAO Aircraft Engine Emissions Databank v32 (EASA, 2026) UID 08P28CM150 LEAP-1C28: rated thrust 129.976 kN, bypass ratio 10.559, pressure ratio 35.464, fuel flow T/O 0.9469 / C/O 0.7763 / App 0.2622 / Idle 0.0931 kg/s",
            ],
            ..AircraftReferenceData::default()
        },
        // Needs data: C919 nacelle clearance against COMAC item J (model 0.37 m below); the ground and pylon rule and the item J dimension are not on disk, so no re-hang is applied.
        engine_name: "LEAP-1C",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            wheels_per_mlg_strut: 2,
            // Main and nose tires: COMAC C919 ACAP Table 1 prints 46x17R20/30PR and 30x8.8R15/16PR.
            mlg_tire: "46x17.0R20/30PR".to_owned(),
            nlg_tire: "30x8.8R15/16PR".to_owned(),
            // COMAC C919 ACAP 02-02 Fig 1: printed track 7.62 m over the printed 3.96 m body, printed wheelbase
            // 13.47 m. Stations read at the side-view gear axes: nose gear 5.28 m, main gear 18.75 m aft of the nose.
            track_diameter_factor: 7.62 / 3.96,
            reference_wheelbase_m: Some(13.47),
            reference_track_m: Some(7.62),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(38.9),
            reference_nlg_x_fraction: Some(5.28 / 38.9),
            reference_mlg_x_fractions: Some(vec![18.75 / 38.9, 18.75 / 38.9]),
            // COMAC C919 Aircraft Characteristics for Airport Planning C919-SVV19-50009-00, sec. 02-03 Table 1
            // (STD): bottom of fuselage (K) 1.45 m forward CG / 1.46 m aft CG at MTW,
            // 1.54 / 1.56 m at OEW; the MTW aft-CG value is used (compressed struts, as the Airbus tables are read).
            fuselage_ground_clearance_m: Some(1.46),
            // No published tail-strike attitude exists for the C919, so its
            // model tail-down angle is unvalidated (the generic tailcone loft
            // understates it): keep the Raymer/Roskam 15 deg floor on top of
            // the tail-down criterion.
            min_tip_back_deg: 15.0,
            // Tail-scrape requirement: no published tail-strike attitude found (COMAC ACAP C919-SVV19-50009-00
            // prints no tail angle), so the 10 deg default (an assumption) stands.
            trimmable_stabilizer: true,
            ..LandingGearConfig::default()
        },
        // Planform: straight edges of the ACAP 02-02 Fig 1 plan view (outline strokes read at 4.752 pt/m). The leading edge is
        // one line, x = 12.664 + 0.5186 y, from the side of body to the device. The inboard trailing edge sits at x = 19.80 m; the
        // outboard one is x = 17.838 + 0.3057 y for y > 9 m; a fillet joins them. The kink chord is the drawn chord at the kink, on
        // the fillet, so the straight trapezoid closes at 129.4 m^2 against the 129.15 m^2 published.
        design_vector: DesignVector {
            // Printed span over the devices, ACAP 02-02 Fig 1.
            span_m: 35.8,
            // Plan view: 12.664 to 19.80 m on the centreline; 15.993 to 19.985 m at the kink; 21.369 to 22.970 m at y = 16.786 m.
            root_chord_m: 7.136,
            break_chord_m: 3.992,
            tip_chord_m: 1.601,
            // Plan view: leading-edge slope 0.5186 from y = 2.5 to 16.5 m.
            sweep_deg: 27.41,
            tip_twist_deg: -1.5,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: 38.9,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        speed_schedule_source: crate::presets::SpeedScheduleSource::SourcedLadder,
        airfoil_class_source: "ESTIMATE: the C919 wing (first flight 2017) is a supercritical design; COMAC section data are not public. Drawn with NASA SC(2)-0610 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990), as for the A320 preset.",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values; the drawn planform stays the geometry.
                // S 129.15 m2: secondary source only (Wikipedia specification table, see the sources list); no published MAC or LEMAC
                // (COMAC publishes none): both follow the drawn reference trapezoid.
                reference_wing: crate::ReferenceWingConfig::published(129.15, None, None),
                // Plan view: the straight leading edge run in to the centreline.
                root_datum_x_m: 12.664,
                // Heights fitted to the upper edge of the ACAP 02-02 Fig 1 front view, read on the fuselage crown.
                root_z_m: -1.0739,
                break_z_m: -0.3997,
                tip_z_m: 0.7392, // front view: upper surface z = 0.846 m at y = 16.8 m
                root_twist_deg: 3.0,
                break_twist_deg: 1.0,
                // Plan view: the two trailing-edge lines meet at y = 6.42 m of the 16.786 m wing semispan.
                break_span_fraction: 0.382_5,
                kink_span_fraction: Some(0.382_5),
                // Half the printed 3.96 m body width over the 16.786 m wing semispan.
                side_of_body_span_fraction: Some(0.117_96),
                // Plan view: 13.691 to 19.80 m at the side of body, 6.109 m over the 7.136 m root chord.
                side_of_body_chord_ratio: Some(0.856_1),
                outboard_sweep_decrement_deg: 1.5,
                root_airfoil: "sc20610".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                airfoil_class: crate::AirfoilClass::SupercriticalAdvanced,
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
                        span_fraction: 0.1180,
                        thickness_scale: 1.3840, // t/c 0.0997 -> 0.1380
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.3825,
                        thickness_scale: 1.0530, // t/c 0.0997 -> 0.1050
                    },
                    crate::WingThicknessStation {
                        span_fraction: 1.0000,
                        thickness_scale: 0.9539, // t/c 0.0996 -> 0.0950
                    },
                ],
                // Spanwise dihedral: leading-edge heights at interior stations fitted to the upper edge of the
                // manufacturer front view (symmetrised drawing aligned on the fuselage crown, same measurement on the model).
                dihedral_stations: vec![
                    crate::WingDihedralStation {
                        span_fraction: 0.2000,
                        z_m: -0.7572,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.3000,
                        z_m: -0.5497,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.5000,
                        z_m: -0.2003,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.6500,
                        z_m: 0.0930,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.8000,
                        z_m: 0.3672,
                        flight_rise_m: None,
                    },
                ],
                // C2 cubic spline through the dihedral, twist and thickness stations.
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                // Tip device of the ACAP 02-02 Fig 1 views, a continuous arc. Front view: rises from y = 16.8 m to 17.9 m, top about
                // z = 1.65 m, fitted by a 1.0 m blend into a 30 deg cant. Plan view: leading edge fitted by 49.5 deg over the arclength
                // (centre of the drawn line, tip x 23.2 m; was 51 deg); chord 0.42 m at the tip.
                wingtip_device: Some(crate::WingtipDevice {
                    kind: crate::WingtipDeviceKind::Winglet,
                    height_m: 0.93,
                    down_height_m: 0.0,
                    cant_deg: 30.0,
                    root_chord_m: None,
                    root_te_flare: None,
                    tip_chord_m: 0.42,
                    le_sweep_deg: 49.5,
                    toe_out_deg: 0.0,
                    blend_radius_m: 1.0,
                    arc_start_deg: None,
                    path_stations: Vec::new(),
                    airfoil: None,
                    span_includes_device: true,
                    include_in_vlm: true,
                }),
                // Plan view. Leading-edge fillet at the body: x = 13.48 m at the side of body, back on the line at y = 2.2 m.
                // Trailing-edge fillet about the kink: offsets from the side-of-body-to-kink and kink-to-tip lines to the drawn
                // edge, y = 2.2, 4.6, 5.6, 7.0, 8.0, 9.5 and 12.5 m.
                edge_stations: vec![
                    crate::WingEdgeStation { span_fraction: 0.11796, le_offset_m: -0.21, te_offset_m: 0.0 },
                    crate::WingEdgeStation { span_fraction: 0.1311, le_offset_m: 0.0, te_offset_m: -0.009 },
                    crate::WingEdgeStation { span_fraction: 0.2740, le_offset_m: 0.0, te_offset_m: -0.099 },
                    crate::WingEdgeStation { span_fraction: 0.3336, le_offset_m: 0.0, te_offset_m: -0.075 },
                    crate::WingEdgeStation { span_fraction: 0.4170, le_offset_m: 0.0, te_offset_m: -0.072 },
                    crate::WingEdgeStation { span_fraction: 0.4766, le_offset_m: 0.0, te_offset_m: -0.140 },
                    crate::WingEdgeStation { span_fraction: 0.5660, le_offset_m: 0.0, te_offset_m: -0.11606 },
                    crate::WingEdgeStation { span_fraction: 0.7447, le_offset_m: 0.0, te_offset_m: -0.06827 },
                ],
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // ACAP 02-02 Fig 1 plan view: leading edge x = 32.85 + 0.634 y, trailing edge x = 36.64 + 0.23 y, tip at the printed
                // 12.60 m span. Front view: mean surface rises 0.67 m from root to tip (6 deg).
                hstab_offset_from_tail_m: 6.14, // round 2: built plan outline was 0.09 m aft of the drawn leading edge at every station
                hstab_z_m: 0.785,
                hstab_root_chord_m: 3.75, // round 2: trailing edge 0.045 m forward of the first fit (drawn trailing edge x = 36.64 + 0.23 y)
                hstab_tip_chord_m: 1.205, // round 2: drawn tip chord 1.21 m, ACAP 02-02 Fig 1 plan view
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (3.991, 6.295, 0.67),
                // ACAP 02-02 Fig 1 side view: leading edge x = 32.38 + 0.85 (z - 3.5), trailing edge x = 37.25 + 0.25 (z - 4), from the
                // crown z = 2.18 m to the printed fin height 5.96 m.
                vstab_offset_from_tail_m: 7.64,
                vstab_z_m: 2.18,
                vstab_root_chord_m: 5.535,
                vstab_tip_chord_m: 1.96,
                vstab_tip_le_m: (5.065, 0.0, 5.96),
                // Rounded tip leading-edge corner, ACAP 02-02 Fig 1 side view: leaves the straight edge at z 7.9 m, runs to x 37.3 m at the tip.
                vstab_edge_stations: vec![
                    crate::TailEdgeStation { span_fraction: 0.96, le_offset_m: 0.0, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.968, le_offset_m: 0.08, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.9765, le_offset_m: 0.16, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.985, le_offset_m: 0.36, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 0.993, le_offset_m: 0.71, te_offset_m: 0.0 },
                    crate::TailEdgeStation { span_fraction: 1.0, le_offset_m: 1.05, te_offset_m: 0.0 },
                ],
                // Side view: fillet ahead of the fin leading edge, leaves the crown at x = 31.0 m and joins the fin line at z = 3.0 m.
                vstab_dorsal_length_m: Some(0.61), // side view: leaves the crown at x 30.65 m, joins the LE about z 2.92 m
                vstab_dorsal_shoulder: Some(0.15),
                vstab_dorsal_height_m: Some(0.74),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                // Wikipedia specification table: 3.96 m wide, 4.166 m high.
                diameter_m: 3.96,
                height_m: Some(4.166),
                nose_z_m: -0.68,
                cabin_start_x_m: 3.5,
                cabin_z_m: 0.1,
                // ESTIMATE: the A320-200 tailcone (7.5 m of 37.57 m) scaled to
                // the 38.9 m length.
                tailcone_length_m: 7.77,
                tail_z_m: 1.0,
                // FUSELAGE-G2 shaped nose and aft body. Outline traced from the manufacturer drawing
                // (side: same; top: COMAC C919 ACAP C919-SVV19-50009-00 Issue 003),
                // calibrated to the drawn length; the nose law (windshield, radome, keel and plan
                // exponents) is a least-squares fit to the traced upper, lower and plan lines (RMS 0.092 m),
                // the aft law (belly ramp, crown drop, plan taper, closing section) to the traced aft lines
                // (RMS 0.049 m). Cabin start, tailcone length and every payload and mass station are unchanged:
                // only the outer loft moves. Section exponent 2.0 (ellipse): no drawing measures it.
                nose_length_m: Some(4.44),
                nose_windshield_angle_deg: Some(36.3),
                nose_crown_end_fraction: Some(1.0),
                nose_radome_length_fraction: Some(0.1),
                nose_keel_exponent: Some(3.38),
                nose_plan_exponent: Some(1.64),
                nose_section_exponent: Some(2.0),
                belly_upsweep_length_m: Some(13.69),
                aft_belly_exponent: Some(1.58),
                tail_end_z_m: Some(1.21),
                tail_end_width_m: Some(0.5),
                tail_end_height_m: Some(0.48),
                aft_plan_length_m: Some(10.94),
                aft_plan_exponent: Some(1.18),
                aft_crown_length_m: Some(11.27),
                aft_crown_exponent: Some(2.2),
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // engines.json carries a V2 nacelle set (nacelle_shape_v2) that agrees with the measured length, diameter and inlet.
                nacelle_model: crate::NacelleModel::V2,
                // ACAP 02-02 Fig 1 plan view: mid of the nacelle sides.
                spanwise_positions_m: vec![6.07, -6.07],
                // Side view: nacelle top z = -0.62 m and bottom z = -3.11 m, axis z = -1.865 m; wing leading edge z = -0.431 m there.
                z_m: -1.434,
                // Printed nose-to-inlet 12.08 m; wing leading edge x = 15.812 m at the engine.
                inlet_x_offset_m: 3.732,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            // Wikipedia table: cruise M 0.785 (37,000 ft in the table's
            // conversion; 39,000 ft in the article text), so 37,000 ft is
            // an ESTIMATE of the design cruise altitude.
            cruise_mach: 0.785,
            cruise_altitude_m: 37_000.0 * 0.3048,
            mtow_kg: 75_100.0,
            max_wing_area_m2: 135.0,
            min_wing_loading_kg_m2: 500.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 158,
            // ESTIMATE: 158 passengers at about 95 kg with baggage.
            cargo_payload_kg: 15_000.0,
            // Wikipedia table: maximum payload 18,900 kg.
            max_structural_payload_kg: 18_900.0,
            // HAND-SET: no C919 VMO is published at any rank (CAAC TCDS TC0066A is not
            // public); only MMO 0.82 (SASAC, 2015-11-16, design specification) is. The A320
            // preset's former value is carried until the AFM limitations are available.
            dive_speed_m_s: 180.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        // S: the reference field length is stated with the packs on: COMAC C919 ACAP
        // C919-SVV19-50009-00 R9, DM 03-01 Table 1 (Aeroplane reference field length, SL, ISA,
        // max take-off thrust, A/C on, anti-ice off, MTOW, Config 2).
        performance: crate::presets::high_lift("modern_narrowbody").map(|performance| {
            crate::PerformanceConfig {
                takeoff_packs_on: true,
                ..performance
            }
        }),
    }
}
