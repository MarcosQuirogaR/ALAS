// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! COMAC C919-100 standard-range preset with LEAP-1C28 engines.
//!
//! Source map and its limits. COMAC has published no airport-planning manual
//! this project could retrieve, the CAAC type-certificate data sheet is not
//! public, and no EASA type-certificate data sheet exists for the airframe.
//! The airframe numbers therefore come from the specification table of the
//! Wikipedia C919 article (English and Chinese editions, retrieved
//! 2026-10-05), a SECONDARY compilation whose weight rows cite COMAC's
//! 2023-01-11 delivery release (`english.comac.cc/news/latest/202301/11/
//! t20230111_7355061.shtml`) and Aviation Week (11 Feb 2020). Everything the
//! table does not print (planform chords, sweep, tail, gear stations, engine
//! station) is an ESTIMATE scaled from the Airbus A320-200 preset, the type
//! COMAC designed against, and is marked where it is used. The engine is
//! sourced to the ICAO Aircraft Engine Emissions Databank v32 (EASA, 2026),
//! UID 08P28CM150 (LEAP-1C28), the CAAC-certified LEAP-1C family sharing the
//! EASA.E.110 LEAP-1A/-1C type design with a 78 in fan.

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
            // Chinese Wikipedia table: full-payload range 2,200 nmi (STD),
            // 3,000 nmi (ER); the English table prints 4,139 km at standard
            // payload and COMAC's 4,075 km (2,200 nmi) basic range. No cabin,
            // reserve or profile is stated.
            design_point: Some(crate::PayloadRangeDesignPoint {
                range_nmi: 2_200.0,
                payload_kg: None,
                source: "Wikipedia, Comac C919, Specifications (Chinese edition: full-payload range 2,200 nmi STD / 3,000 nmi ER; English edition: 4,075 km basic range, 4,139 km at standard payload); secondary; no cabin, reserve or profile stated, so this is an advertised-range proxy, not a payload/range chart point",
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
        engine_name: "LEAP-1C",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            wheels_per_mlg_strut: 2,
            // ESTIMATE: no gear dimensions are published in any retained
            // source. The A320-200 track (7.59 m over a 3.95 m body) is
            // carried to the 3.96 m C919 body.
            track_diameter_factor: 7.59 / 3.95,
            // ESTIMATE: the A320-200 wheelbase (12.64 m, EASA.A.064) scaled by
            // the 38.9 / 37.57 length ratio.
            reference_wheelbase_m: Some(12.64 * 38.9 / 37.57),
            reference_track_m: Some(7.59),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(38.9),
            // ESTIMATE: A320-200 stations (nose gear 5.07 m, main gear 17.71 m
            // of 37.57 m) as length fractions.
            reference_nlg_x_fraction: Some(5.07 / 37.57),
            reference_mlg_x_fractions: Some(vec![17.71 / 37.57, 17.71 / 37.57]),
            // No published tail-strike attitude exists for the C919, so its
            // model tail-down angle is unvalidated (the generic tailcone loft
            // understates it): keep the Raymer/Roskam 15 deg floor on top of
            // the tail-down criterion.
            min_tip_back_deg: 15.0,
            takeoff_stabilizer_nose_up_deg: Some(crate::landing_gear::TRANSPORT_THS_TAKEOFF_NOSE_UP_DEG),
            ..LandingGearConfig::default()
        },
        // ESTIMATE of the whole planform. The planar wing excludes the
        // winglets: the 35.8 m of the table is over the winglets, so a
        // planar semi-span of 16.8 m (33.6 m, the figure the English article
        // gives for the span without winglets; the same article also prints
        // 35.4 m with winglets, which disagrees with the table) is used, as
        // the A320 preset does for its sharklets. The A320-200 planform
        // shape (kink at 0.379 of the semi-span, 27.1 deg leading edge,
        // unswept inboard trailing edge so the root-to-kink trailing-edge
        // angle is the 90 deg the layout residual allows, a 1.75 m tip chord
        // close to the A320's 1.64 m) is solved so the trapezoid closes the
        // 129.15 m^2 reference area: with the kink 6.372 m out the unswept
        // trailing edge fixes the kink chord at the root chord less 3.2625 m,
        // and a 7.150 m centreline chord (kink chord rounded up to 3.895 m so the edge never runs forward) then gives half area 64.58 m^2.
        design_vector: DesignVector {
            // Step B: 35.80 m is the span over the winglets (ACAP 02-02 Fig 1, printed, A); the planar wing is 33.6 m (device reach 1.10 m per side, below). Was span_m 33.6.
            span_m: 35.8,
            root_chord_m: 7.150,
            break_chord_m: 3.895,
            tip_chord_m: 1.75,
            sweep_deg: 27.1,
            tip_twist_deg: -1.5,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: 38.9,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        airfoil_class_source: "ESTIMATE: the C919 wing (first flight 2017) is a supercritical design; COMAC section data are not public. Drawn with NASA SC(2)-0610 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990), as for the A320 preset.",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values (geometry freeze 2026-10-12, owner decision); the drawn planform stays the geometry.
                // S 129.15 m2: secondary source only (Wikipedia specification table, see the sources list); no published MAC or LEMAC
                // (COMAC publishes none): both follow the drawn reference trapezoid.
                reference_wing: crate::ReferenceWingConfig::published(129.15, None, None),
                // Root-chord leading edge 12.60 m aft of the nose: the straight
                // leading edge of the C919 ACAP plan view (C919-SVV19-50009-00,
                // 02-02 Fig 1, vector, overlay study) run in to the centreline.
                // It replaces the A320 length-fraction estimate (12.31 m), which
                // put the wing 0.3 m forward of the drawing at every station.
                root_datum_x_m: 12.60,
                // ESTIMATE: A320-200 dihedral heights (low wing, 3.95 m body)
                // carried over.
                root_z_m: -1.0739, // geometry round 2 (wing lane); was -1.2
                break_z_m: -0.3997, // geometry round 2 (wing lane); was -0.62
                tip_z_m: 0.6992, // geometry round 2 (wing lane); was 0.33
                root_twist_deg: 3.0,
                break_twist_deg: 1.0,
                break_span_fraction: 0.379_3,
                kink_span_fraction: Some(0.379_3),
                // Half the 3.96 m body width over the 16.8 m semi-span.
                side_of_body_span_fraction: Some(0.117_86),
                // 6.136 m over the 7.150 m centreline chord: the chord where the
                // 27.1 deg leading edge leaves the body, keeping the inboard
                // trailing edge unswept as on the A320 preset.
                side_of_body_chord_ratio: Some(0.858_2),
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
                        span_fraction: 0.1179,
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
                // Spanwise dihedral (geometry round 2): leading-edge heights at interior stations fitted to the upper edge of the
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
                // Step B (loft-wing-2026-10-11): C2 cubic spline through the dihedral, twist and thickness stations; was Linear (default).
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                // Step B: C919 tip device (ACAP 02-02 Fig 1, measure/C919): outer tip reaches 17.75 m semi-span against the 16.65 m planar tip (reach 1.10 m, B) and the tip top stands
                // 2.19 m above the root upper surface, 0.93 m above the tip upper surface (C, one view, low confidence). A raked, strongly canted tip (47 deg from vertical, blend 0.3 m);
                // tip chord 0.5 m and LE sweep 55 deg are templates (C, line art does not resolve them). Reach with these numbers 1.115 m -> wing 33.57 m.
                wingtip_device: Some(crate::WingtipDevice {
                    kind: crate::WingtipDeviceKind::Winglet,
                    height_m: 0.93,
                    down_height_m: 0.0,
                    cant_deg: 47.0,
                    root_chord_m: None,
                    tip_chord_m: 0.5,
                    le_sweep_deg: 55.0,
                    toe_out_deg: 0.0,
                    blend_radius_m: 0.3,
                    airfoil: None,
                    span_includes_device: true,
                    include_in_vlm: true,
                }),
                ..WingConfig::default()
            },
            // ESTIMATE: the A320-200 empennage (31.0 m^2 tailplane, 21.5 m^2
            // fin) carried to the C919; no C919 tail dimensions are in a
            // retained source. The 11.95 m height of the table is about
            // 0.15 m above the A320's 11.76-11.80 m, consistent with the
            // carried fin.
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // Apex moved 0.69 m forward, tip leading edge held at the ACAP
                // drawing (36.7 m): LE sweep 32.5 deg drawn vs 27.8 deg carried.
                // geo/tailfit 2026-10-11: tailplane straight-edge fit to the drawing, centreline values; was off 6.19, chords 3.97/1.43, tip (4.04, 6.35, 0.612)
                hstab_offset_from_tail_m: 6.09,
                hstab_z_m: 0.800, // geometry round 2 (wing lane): upper surface at the root matched to the front view; was 0.8
                // Step B (C919 ACAP C919-SVV19-50009-00 02-02 Fig 1 top view, measure/C919, B; |delta| 0.13-0.2 m > 0.05 m reading error): drawn tailplane
                // 6.35 m semi-span, centreline chord 3.97 m, tip chord 1.43 m = 34.3 m^2 (was the carried A320 values 6.225 m / 3.740 m / 1.24 m = 31.0 m^2).
                hstab_root_chord_m: 3.8,
                hstab_tip_chord_m: 1.18,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                // Step B: root LE kept (offset 6.19 m); tip LE x 4.04 m = drawn 32.46 deg LE sweep (ACAP top view) over the 6.35 m semi-span, was 3.94 m at 6.225 m (32.33 deg);
                // tip z scaled with the semi-span to hold the 5.5 deg dihedral (0.600 -> 0.612, front view dihedral 4.79 vs 4.59 deg is within uncertainty).
                hstab_tip_le_m: (4.07, 6.35, 0.612),
                // Fin position fitted to the C919 ACAP side view (digitised): crown
                // leading edge 31.2 m, tip trailing edge 38.3 m on the 38.9 m
                // body; the A320 offset it was carried with put them 1.0-1.25 m
                // too far aft. The chords and height stay the carried ones
                // (drawing: 5.7 m root, 2.1 m tip chord, 5.9 m height).
                // Step B: crown LE at 31.16 m (ACAP side view LE extrapolated to the crown, measure/C919, C) = 38.9 - 7.74; was 7.65 (LE 31.25 m). The measurement table
                // lists 7.55 for -0.1 m, which has the wrong sign: the model LE was 0.10 m AFT of the drawing, so the offset grows.
                // geo/tailfit 2026-10-11: fin straight-edge fit to the drawing at the root line (rms 0.010/0.005 m); was off 7.74, chords 5.74/2.05, tip (5.05, 0.0, 5.93)
                vstab_offset_from_tail_m: 7.72,
                vstab_z_m: 2.17,
                // Step B (ACAP side view chords 5.60/4.79/3.89/3.01/2.24 m at 5/25/50/75/95 % height, measure/C919, B): crown chord 5.74 m (mean of the extrapolated 5.80 m and
                // the traced crown LE/TE 5.68 m; was the carried A320 5.444 m), tip chord 2.05 m (was 1.884), height 5.93 m (drawn 5.93-5.96, was 5.87), tip LE offset 5.05 m =
                // 5.93 m x tan(40.44 deg) of the drawn LE sweep (was 5.060 m at 5.87 m, 40.8 deg). Fin area 23.1 m^2 vs drawn 23.2 (was 21.5).
                vstab_root_chord_m: 5.54,
                vstab_tip_chord_m: 1.96,
                vstab_tip_le_m: (5.06, 0.0, 5.93),
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
                // Final merge: engines.json carries a V2 nacelle set (nacelle_shape_v2) that agrees with the measured length, diameter and inlet; was Legacy.
                nacelle_model: crate::NacelleModel::V2,
                // ESTIMATE: the A320-200 engine station (5.755 m of a 17.05 m
                // semi-span, 0.3376) scaled to the 16.8 m semi-span, 5.67 m,
                // with 0.1 m added for the larger 78 in fan nacelle.
                // geo/close 2026-10-12: drawn engine 0.30 m further outboard (top view best shift dy +0.35 m); was 5.77 (ESTIMATE)
                spanwise_positions_m: vec![6.07, -6.07],
                // ESTIMATE: the A320 axis 1.50 m below the local leading edge
                // lowered by the 0.15 m radius increase (1.15 m LEAP nacelle
                // against 1.0 m) so the nacelle keeps its ground clearance.
                // geo/close 2026-10-12: raised 0.35 m to the drawn height (side sheet best shift dz +0.35 m; the previous value was an ESTIMATE holding A320-like clearance); was -1.857
                z_m: -1.507, // geometry round 2 (wing lane): was -1.65; the wing leading edge at the engine moved +0.207 m, this keeps the nacelle height
                // ESTIMATE: the A320 offset 3.646 m plus 0.3 m for the
                // longer LEAP nacelle ahead of the wing leading edge.
                // Step B (C919 ACAP 02-02 Fig 1 top view, printed 12.08 m nose-to-inlet, measure/C919, B; model front was 11.603 m, 0.48 m forward, confirmed by the
                // before overlay): offset 3.95 -> 3.473 m puts the inlet at 12.08 m. Not tied to the LEAP-1C V2 shape (no radius change: drawn 2.72 m diameter is schematic).
                inlet_x_offset_m: 3.473,
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
            // ESTIMATE: no C919 dive speed is public; the A320 preset value
            // is carried.
            dive_speed_m_s: 180.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("modern_narrowbody"),
    }
}
