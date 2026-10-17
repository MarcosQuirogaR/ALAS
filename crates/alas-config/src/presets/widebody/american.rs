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

pub use dc10::dc_10;

/// Composite long-range twin, the type the global mass model is tuned on.
pub fn b787_9() -> AircraftPreset {
    AircraftPreset {
        name: "B787-9",
        display_name: "Boeing 787-9 Dreamliner",
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
            mzfw_kg: Some(181_436.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("B787-9"),
            usable_fuel_volume_l: Some(126_429.0),
            usable_fuel_mass_kg: Some(101_522.0),
            fuel_density_kg_l: Some(101_522.0 / 126_429.0),
            reference_wing_area_m2: Some(360.464),
            planning_seats: Some(290),
            design_point: Some(crate::PayloadRangeDesignPoint {
                range_nmi: 5_300.0,
                payload_kg: Some(52_586.0),
                source: "Boeing 787 Airplane Characteristics for Airport Planning Rev Q section 3.2.2 (payload/range long-range cruise, 787-9 typical engines): maximum-zero-fuel-weight corner read as 5,300 nmi (+-60 nmi); payload is the preset MZFW minus its reference OEW; reserves not stated",
            }),
            certified_max_seats: Some(420),
            certified_exit_layout: Some(crate::presets::B787_9_EXIT_LAYOUT),
            planning_cabin: Some(crate::presets::B787_9_PLANNING_CABIN),
            design_mission_evidence: crate::DesignMissionEvidence::Unverified,
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
            aft_cg_nose_load: Some(crate::presets::gear_load::B787_9),
            sources: vec![
                "Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 2",
                "Boeing 787 ARFF composite-content diagram, p.3: https://www.boeing.com/content/dam/boeing/v2/airports/arff/787_composite_arff_data_2025.pdf",
                crate::preset_structures::TRANSPORT_CAP_SOURCE,
                "NASA/TP-20210023843, December 2022, Table I",
                "Boeing 787 ACAP D6-58333 Rev L, December 2015, p.2-3 (typical OEW only)",
            ],
        },
        engine_name: "GEnx-1B",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
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
            takeoff_stabilizer_nose_up_deg: Some(crate::landing_gear::TRANSPORT_THS_TAKEOFF_NOSE_UP_DEG),
            // Tail-strike attitude 9.7 deg for the 787-9 "with wheels on the runway and
            // landing gear struts extended" (UK AAIB Bulletin 11/2020, AAIB-26410,
            // VH-ZND, quoting Boeing); the compressed-strut value was not found. The
            // generic tailcone loft understates the geometric tail-down angle, so
            // this is the floor.
            min_tip_back_deg: 9.7,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 60.12,
            // Step B (w2), D6-58333 Rev Q 2.2.2 p.22 plan view (native raster, 1 px = 0.06 m, +-0.1 m):
            // the drawn trailing edge runs from x = 33.44 m at the centreline with 4.2 deg inboard sweep to the
            // kink at y = 11.11 m (x = 34.25 m); with the leading edge held at the root datum below these are the
            // chords that put both edges on the drawing. Root chord was 12.6 m (round 2), break chord 6.554 m.
            // The gross planform area is now about 373 m2 against the published 360.464 m2: the drawn planform is
            // wider inboard than the area the published reference closes (geometry vs reference area, DECISIONS).
            root_chord_m: 13.3663, // step B w2; was 12.6
            break_chord_m: 6.4692, // step B w2; was 6.55428 (no longer closes 360.464 m2, see above)
            tip_chord_m: 1.6, // chord the straight edges reach at the tip; no rake any more (extreme chord 1.6 m, published 2.67 m: the drawn trailing edge kicks aft over the last 2.5 m, not representable with straight edges)
            // The 32.2 deg quoted for the 787-9 (32 deg in NASA/TP-20210023843,
            // December 2022, Table I) is the outboard quarter-chord sweep:
            // the outboard taper converts it to this leading-edge angle, and
            // the straight 34.7 deg leading edge of the D6-58333 Rev Q
            // section 2.2.2 plan view confirms it.
            sweep_deg: 34.74104381154634, // geometry round 2 (wing lane): the published c/4 sweep re-converted through the closed kink chord (was 34.7140083405483)
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
        airfoil_class_source: "Design-era declaration: the 787-9 (787 EIS 2011, -9 2014) wing is of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC(2)-0612 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Published reference (freeze 2026-10-12): S 360.464 (787-9 gross area); MAC EASA.IM.A.115 item 16; LEMAC D6-58333 7.4.2 statics.
                reference_wing: crate::ReferenceWingConfig::published(360.464, Some(6.27126), Some(27.784)),
                // Quarter-MAC: D6-58333 7.4.2 %MAC/main-gear-load two-point statics
                // give LEMAC 27.784 m aft of nose (TCDS MAC 6.271 m; model MAC 20% longer).
                // Step B w2: NOT moved to the drawn 21.33 m (the Rev Q plan view puts the whole leading edge 0.24 m, B,
                // forward of the model): the LEMAC statics pin is a published-derived value that the CG and gear loads
                // rest on and beats a drawing trace (DECISIONS); with the new chords the model quarter-MAC lies 0.06 m
                // ahead of the round-2 station, inside the +-0.1 m reading error. Open owner decision.
                root_datum_x_m: 21.5737, // geometry round 2 (wing lane): was 21.552; keeps the published quarter-MAC station with the swept-back tip, the closed kink chord and the re-converted sweep
                root_z_m: -1.5210, // geometry round 2 (wing lane); was -1.8
                break_z_m: -0.0866, // geometry round 2 (wing lane); was -0.2
                tip_z_m: 2.65, // step B w2: front view tip upper edge 2.72 m vs model 2.45 m (B, -0.27 m, the drawn tip curls up over the last 1.2 m); was 2.3813 (round 2), 2.5 earlier
                root_twist_deg: 3.5,
                break_twist_deg: 1.5,
                break_span_fraction: 0.35,
                // Step B w2: drawn trailing-edge kink at y = 11.11 m of the 30.06 m semispan (B), was 0.353_771_245_388_011_8 (y = 10.63 m).
                kink_span_fraction: Some(0.3696),
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
                airfoil_class: crate::AirfoilClass::Supercritical,
                                // Thickness profile (geometry round 2): the NASA Common Research Model t/c 0.154 at the
                // centreline, 0.138 at the side of body, 0.105 at the yehudi break and 0.095 at the tip
                // (Vassberg, DeHaan, Rivers and Wahls, AIAA 2008-6919, Table 2) is the public transport-wing
                // distribution; the multipliers below act on the root-kink-tip section blend above.
                thickness_stations: vec![
                    crate::WingThicknessStation {
                        span_fraction: 0.0000,
                        thickness_scale: 1.2838, // t/c 0.1200 -> 0.1540
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.1000,
                        thickness_scale: 1.1504, // t/c 0.1200 -> 0.1380
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.3696, // step B w2: follows the kink (was 0.3538)
                        thickness_scale: 0.8753, // t/c 0.1200 -> 0.1050
                    },
                    crate::WingThicknessStation {
                        span_fraction: 1.0000,
                        thickness_scale: 0.9539, // t/c 0.0996 -> 0.0950
                    },
                ],
                // Step B w2: the swept-back tip of round 2 (extreme chord 1.01 m, rake start 0.908) is removed. The Rev Q plan
                // view draws ONE straight leading edge to the extreme tip (x = 40.69 m published, 133 ft 6 in), so the rake put
                // the model tip leading edge 0.82 m aft of it; the drawn trailing edge kicks aft to 43.36 m, which straight
                // edges cannot follow (tip chord 1.6 m against 2.67 m published, still wrong). Was tip_rake_tip_chord_m
                // Some(1.01), tip_rake_start_span_fraction Some(0.9080).
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline, // step B w2 (loft-wing report); was Linear
                // Spanwise dihedral (geometry round 2): leading-edge heights at interior stations fitted to the upper edge of the
                // manufacturer front view (symmetrised drawing aligned on the fuselage crown, same measurement on the model).
                dihedral_stations: vec![
                    crate::WingDihedralStation {
                        span_fraction: 0.1000,
                        z_m: -1.0687,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.2000,
                        z_m: -0.6374,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.3000,
                        z_m: -0.2780,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.5000,
                        z_m: 0.4787,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.6500,
                        z_m: 1.0222,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.8000,
                        z_m: 1.6221,
                        flight_rise_m: None,
                    },
                ],
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // D6-58333 Rev Q section 2.2.2 (PDF p.22): tailplane span
                // 65 ft 0 in (19.81 m) with its tip trailing edge at the
                // 206 ft 1 in (62.81 m) overall length; this root station
                // puts it there. Chords and sweep are not dimensioned.
                // Apex moved 2.76 m forward with the tip leading edge held (61.0 m,
                // tip trailing edge at the 62.81 m length): the Rev Q plan view
                // draws a 40 deg leading edge, the former apex gave 31 deg.
                // Step B w2 (Rev Q 2.2.2 plan view, B): centreline apex 52.78 m (62.00 - 9.22); was 9.75 (52.25 m). Replaces
                // the pass-2 reading 54.9 m, which was the body-junction x.
                // geo/tailfit 2026-10-11: tailplane straight-edge fit to the drawing, centreline values; was off 9.22, chords 6.27/1.65, tip (8.38, 9.905, 1.190)
                hstab_offset_from_tail_m: 9.56,
                hstab_z_m: 0.570, // geometry round 2 (wing lane): upper surface at the root matched to the front view; was 1.0
                hstab_root_chord_m: 6.57, // step B w2: centreline trailing edge 59.05 m (B); was 6.5
                hstab_tip_chord_m: 1.75, // step B w2: tip trailing edge on the published 62.81 m (B); was 1.8
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (8.47, 9.905, 1.190), // tailplane dihedral matched to the front view; was z 0.8; step B w2: tip leading edge 61.16 m, sweep 40.2 deg (B), was dx 8.76 (41.5 deg)
                // No fin station is dimensioned: the offset holds the fin root
                // leading edge at 50.81 m on the 62.00 m body. The root line
                // is the crown of the 5.94 m body, the tip at the 16.81 m
                // minimum fin-top height of section 2.3.2 (PDF p.25, N), and
                // the builder carries the edges on down to the tail cone
                // under the root. Chords are not dimensioned.
                // Fitted to the Rev Q side view (digitised, 16.5 px/m): crown
                // trailing edge 58.2 m and tip trailing edge 61.6 m on the 62.0 m
                // body, so the former 52.31 m root leading edge sat about
                // 1.5 m too far aft (tip trailing edge past the tail cone).
                // geo/tailfit 2026-10-11: fin straight-edge fit to the drawing at the root line (rms 0.037/0.026 m); was off 10.99, chords 7.5/2.4, tip (8.60, 0.0, 8.55)
                vstab_offset_from_tail_m: 11.22, // step B w2: crown leading edge 51.01 m (62.00 - 10.99), 0.2 m aft of the old 50.81 m (B); was 11.19
                vstab_z_m: 3.17,
                vstab_root_chord_m: 7.46,
                vstab_tip_chord_m: 2.28,
                // Step B w2: fin top 8.55 m above the crown (11.72 m in the body frame; B) and 45.1 deg leading-edge sweep
                // (dx 8.60 m); the published 17.02 m is measured to the drawing's ground line, which falls 0.79 deg toward
                // the tail, and is consistent with 8.55 m. Was (8.5, 0.0, 9.12), 43.0 deg.
                vstab_tip_le_m: (8.53, 0.0, 8.55),
                // Step B w2: dorsal fillet drawn in the Rev Q side view (B): the leading edge leaves the crown at x = 49.3 m and
                // joins the straight leading edge at x = 52.3 m, z = 4.4 m (1.23 m above the crown); not in the aerodynamics.
                // geo/tailfit 2026-10-11: dorsal re-fitted to the drawn LE; was x Some(49.3), height Some(1.23)
                vstab_dorsal_x_start_m: Some(50.1),
                vstab_dorsal_height_m: Some(0.97),
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
                // Final merge: engines.json carries a V2 nacelle set (nacelle_shape_v2) that agrees with the measured length, diameter and inlet; was Legacy.
                nacelle_model: crate::NacelleModel::V2,
                // Boeing 787 ACAP D6-58333 Rev Q, October 2025, section
                // 2.2.2 (PDF p.22, General Dimensions: Model 787-9) plan
                // view: engine centreline 32 ft 6 in (9.91 m) from the
                // airplane centreline, nacelle inlet face 68 ft 3 in
                // (20.80 m) aft of the nose tip.
                spanwise_positions_m: vec![9.91, -9.91],
                // Section 2.3.2 (PDF p.25): GEnx nacelle low point (F) 0.69 m
                // above the ground under the 1.70 m nacelle radius.
                // Low point held with the drawn 1.83 m radius (nacelle lane: -1.821 ->
                // -1.691), plus the wing lane's -0.137 m for the moved wing leading edge.
                z_m: -1.7153, // step B w2: +0.113 m so that the nacelle low point stays at the published 0.69 m clearance (z -3.83 m) with the 3.83 m maximum diameter and the C2 wing heights; was -1.828 for D 3.66 m
                // Puts that inlet face 20.80 m aft of the nose: the 20.052 m
                // root leading edge (root datum plus wing shift) plus the
                // 6.866 m the planform leading edge runs aft by y = 9.91 m,
                // less this offset.
                inlet_x_offset_m: 6.1466, // geometry round 2 (wing lane): was 6.118; the wing datum and sweep moved by +0.0286 m at the engine, this keeps the inlet station
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
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("advanced_highlift_widebody"),
    }
}
