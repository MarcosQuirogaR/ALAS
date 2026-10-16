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
            usable_fuel_volume_l: Some(324_339.0),
            usable_fuel_mass_kg: Some(259_471.0),
            fuel_density_kg_l: Some(0.8),
            reference_wing_area_m2: Some(845.0),
            planning_seats: Some(555),
            design_point: Some(crate::PayloadRangeDesignPoint {
                range_nmi: 6_535.0,
                payload_kg: Some(83_800.0),
                source: "Airbus A380 Aircraft Characteristics (2025-12) section 3-2-1 Figure 3-2-1-991-001-A01 (payload/range ISA, Trent 900): maximum-structural-payload corner read as 6,535 nmi at 83.8 t; read uncertainty +-60 nmi, +-0.5 t; reserves 5 % trip allowance, 30 min hold, 200 nmi diversion",
            }),
            certified_max_seats: Some(868),
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
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            aft_cg_nose_load: Some(crate::presets::gear_load::A380_800),
            sources: vec![
                "EASA.A.110 Issue 17, 2026-08-05, pp.10-15",
                "Airbus A380 Aircraft Characteristics Rev 20, 2025-12-01, section 2-1-1",
                "Airbus A380 Facts and Figures, February 2022, p.3",
            ],
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
            track_diameter_factor: 14.34 / 7.14,
            // 14.34 m wing-gear track; 28.61 m NLG-WLG wheelbase, distinct from
            // the 31.88 m NLG-BLG body-gear wheelbase below.
            reference_wheelbase_m: Some(28.61),
            reference_body_wheelbase_m: Some(31.88),
            reference_track_m: Some(14.34),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(72.73),
            reference_nlg_x_fraction: Some(4.97 / 72.73),
            reference_mlg_x_fractions: Some(vec![
                33.58 / 72.73,
                33.58 / 72.73,
                36.85 / 72.73,
                36.85 / 72.73,
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
            takeoff_stabilizer_nose_up_deg: Some(crate::landing_gear::TRANSPORT_THS_TAKEOFF_NOSE_UP_DEG),
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 79.75,
            // Airbus A380 AC Rev 20, Figure 2-2-0-991-001-A01 sheet 2 (plan
            // view): a 3.98 m streamwise tip chord (leading edge 46.97 m aft
            // of the nose) and a 17.67 m chord at the side of the body. The
            // centreline and kink chords are the pair that, with those two
            // and the kink station below, closes the 845 m^2 reference area
            // and the 12.295 m weight-and-balance MAC of the section 7 fit.
            root_chord_m: 19.678256676269644, // geometry round 2: the drawn trailing edge runs straight through the side of body (17.67 m) to the centreline
            break_chord_m: 11.62016553386156, // geometry round 2 (wing lane): the sourced straight-edge tip chord is kept; this kink chord closes the published reference area behind the swept-back tip
            tip_chord_m: 3.98, // chord the straight edges reach at the tip (the extreme tip chord is the rake chord below)
            // Airbus A380 Facts and Figures (2022) gives 33.5 deg wing
            // sweep; Jane's identifies the quarter-chord convention. The
            // outboard taper converts that to this leading-edge angle, which
            // puts the tip leading edge 46.85 m aft of the nose against the
            // 46.97 m drawn.
            sweep_deg: 36.37689299819862, // geometry round 2 (wing lane): the published c/4 sweep re-converted through the closed kink chord (was 36.41988769551842)
            tip_twist_deg: -2.5,
            wing_x_shift_m: -7.5,
            tail_scale: 1.0,
            fuselage_length_m: 72.73,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        airfoil_class_source: "Design-era declaration: the A380-800 (EIS 2007) wing is of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC(2)-0414 root and SC(2)-0610 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Reference wing = the manufacturer's published values (geometry freeze 2026-10-12, owner decision); the drawn planform stays the geometry.
                // S 845 m2: Airbus A380 reference area (reference_wing_area_m2 of this preset); MAC 12.295 m and LEMAC 28.765 m: joint fit of 30 section 7
                // pavement-load rows (derived, not a TCDS figure).
                reference_wing: crate::ReferenceWingConfig::published(845.0, Some(12.295), Some(28.765)),
                // Quarter-MAC: joint fit of 30 section 7 pavement-load rows gives
                // LEMAC 28.765 m aft of nose, MAC 12.295 m, which the model
                // planform now reproduces; this datum puts its quarter-MAC
                // point on the manufacturer's 31.839 m.
                // Step B w1: inboard leading edge from the drawing (measure/A380-800, confidence B: LE fit y 4.5-12 m clear of the nacelles gives
                // 39.8 deg against 36.4 deg; the LE at the side of body is 0.97 m forward, the kink LE agrees to 0.07 m). The new field
                // inboard_le_sweep_deg cranks the LE at the kink; root datum moved by -14.3246 m x (tan 39.8 - tan 36.377) = -1.3827 m so the kink and the
                // whole outboard wing stay where they were. Was 25.0368 (round 2 value kept the published quarter-MAC station).
                inboard_le_sweep_deg: Some(39.8),
                root_datum_x_m: 23.6541, // step B w1: the outboard wing, the dimensioned tip and the engine inlets stay where they were; the quarter-MAC point moves to 31.441 m (was 31.839 m, the published statics station): geometry follows the drawing, the MAC frame offset is a phase-2 decision (re-pin alternative: 24.0524 with inlet offset 6.7666, which puts the whole outboard wing 0.40 m aft of the drawing)
                // Ground shape fitted to the Figure 2-3-0-991-001-A01 static
                // clearances (MRW, aft CG): the wing tip lower surface 5.21 m
                // above the ground (W2) and a flat inboard panel; the exact
                // fit to the engine clearances N1/N2 would put the kink 0.3 m
                // lower still. This drooped shape has 2.3 deg of dihedral.
                root_z_m: -2.5,
                break_z_m: -2.5,
                tip_z_m: 1.5050, // geo/a380fix r2 2026-10-12: UNLOADED fence bottom 6.59 m above ground (drawing, re-measured 6.58-6.60); 5.21 is the loaded MRW table value, obtained by the computed droop. was 0.1250 // geo/a380fix 2026-10-12: W2 is the fence bottom (AC 2-3-0 W1/W2 lines bound the tip fence), 5.21 m above ground; was -0.9704 (thin-tip lower surface at 5.21); flight tip unchanged through the rise below
                // was -0.9704, // geometry round 2: -0.876 moved by half the tip thickness x chord change so the published tip clearance W2 holds
                // Airbus A380 Facts and Figures (February 2022): "During
                // take-off the wing will flex upwards by over 4m". That 4 m
                // static-to-1 g tip rise over the 39.875 m semispan gives the
                // flight shape 8.0 deg of dihedral; "over" makes it a lower
                // bound.
                flight_tip_rise_semispan_fraction: Some(0.002733), // geo/a380fix r2: was 0.037341; flight tip held // geo/a380fix: was 0.064812; the ground tip rose 1.0954 m, so the flight tip stays where the drawing has it // geometry round 2: flight tip at the drawing height
                root_twist_deg: 4.5,
                break_twist_deg: 2.0,
                break_span_fraction: 0.33,
                // The plan view's 17.67 m chord at the side of the 7.14 m
                // wide body (Figure 2-2-0-991-001-A01 sheet 2).
                side_of_body_span_fraction: Some(3.57 / 39.875),
                side_of_body_chord_ratio: Some(0.8979453968251448), // 17.67 m over the collinear root chord (geometry round 2)
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
                airfoil_class: crate::AirfoilClass::Supercritical,
                                // Thickness profile (geometry round 2): the NASA Common Research Model t/c 0.154 at the
                // centreline, 0.138 at the side of body, 0.105 at the yehudi break and 0.095 at the tip
                // (Vassberg, DeHaan, Rivers and Wahls, AIAA 2008-6919, Table 2) is the public transport-wing
                // distribution; the multipliers below act on the root-kink-tip section blend above.
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
                        span_fraction: 0.3592,
                        thickness_scale: 0.7499, // t/c 0.1400 -> 0.1050
                    },
                    crate::WingThicknessStation {
                        span_fraction: 1.0000,
                        thickness_scale: 0.9529, // t/c 0.0997 -> 0.0950
                    },
                ],
                // Flight-shape dihedral (geometry round 2): the front-view drawing shows the wing bending up inboard (about 6 deg to the
                // engine pylon) and nearly flat outboard (about 3 deg); the ground shape above (static clearances) is kept and each
                // station carries the rise from it to the flight shape fitted to that drawing; the kink station carries the kink rise.
flight_root_rise_m: Some(0.1536),
                // The drawn and tabulated nacelle heights are the static ones: keep them in the flight shape.
                nacelles_hold_ground_height: true,
                dihedral_stations: vec![
                    crate::WingDihedralStation {
                        span_fraction: 0.2000,
                        z_m: -2.5000,
                        flight_rise_m: Some(1.0046),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.3000,
                        z_m: -2.5000,
                        flight_rise_m: Some(1.6127),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.359236516064625,
                        z_m: -2.5000,
                        flight_rise_m: Some(1.8723),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.5000,
                        z_m: -2.1640,
                        flight_rise_m: Some(2.1668),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.6500,
                        z_m: -1.8059,
                        flight_rise_m: Some(2.2301),
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.8000,
                        z_m: -1.4478,
                        flight_rise_m: Some(2.2477),
                    },
                ],
                // Swept-back tip (geometry round 2): the leading and trailing edges follow the straight lines to the
                // design tip chord; outboard of the rake start the leading edge sweeps further aft so that the chord at the
                // extreme tip is 2.19 m (silhouette of the drawing 0.15 m inboard of the tip), the trailing edge staying one line.
                // The design tip chord closes the published reference area including the rake.
                tip_rake_tip_chord_m: Some(2.19),
                tip_rake_start_span_fraction: Some(0.9730),
                // Step B (wing profile splines, loft-wing-2026-10-11): C2 cubic spline through the dihedral, twist and thickness stations; was Linear (the default).
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                // Step B w1 (measure/A380-800, confidence C, two views): wingtip fence, drawn about 1.1 m above and 1.2 m below the tip in the front view and
                // as a planform sliver at y 39.8 m in the top view (AC 2-2-0 does not dimension it). Chord: the wing-tip chord at the root and 1.5 m at the top
                // (estimate; the 4.3 m sliver reads the tip fairing too). Vertical plate, no sweep or cant, NACA 0012; the 79.75 m span is the wing span (a fence has no lateral reach).
                wingtip_device: Some(crate::WingtipDevice {
                    kind: crate::WingtipDeviceKind::Fence,
                    height_m: 1.1,
                    down_height_m: 1.2,
                    cant_deg: 0.0,
                    root_chord_m: None,
                    tip_chord_m: 1.5,
                    le_sweep_deg: 0.0,
                    toe_out_deg: 0.0,
                    blend_radius_m: 0.0,
                    airfoil: Some("naca0012".to_owned()),
                    span_includes_device: false,
                    include_in_vlm: true,
                }),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // Figure 2-2-0-991-001-A01 sheet 2 (plan view): tailplane tip
                // leading edge 68.85 m aft of the nose, 11.57 m aft of the
                // root leading edge (57.28 m), and a 3.72 m tip chord whose
                // trailing edge closes the 72.57 m plan length. The root
                // chord and height are not dimensioned and stay estimates.
                // geo/tailfit 2026-10-11: tailplane straight-edge fit to the drawing, centreline values; was off 15.45, chords 10.0/3.72, tip (11.57, 15.185, 1.530)
                hstab_offset_from_tail_m: 15.42,
                hstab_z_m: 0.960, // geometry round 2 (wing lane): upper surface at the root matched to the front view; was 1.5
                // Step B: centreline root chord 10.0 m (was 9.0): measure/A380-800 htail root TE 67.2 m (top-view TE fit, extrapolated to y=0) less root LE 57.2 m; confidence C, tip chord and the dimensioned tip stations unchanged. Gross trapezoid area 193.2 -> 208.3 m^2 (unverified Airbus figure for the A380 tailplane is about 205 m^2, from memory).
                hstab_root_chord_m: 9.75,
                hstab_tip_chord_m: 3.67,
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
                // (9.0 + 3.72) x 15.185 = 193.2 m^2.
                hstab_tip_le_m: (11.57, 15.185, 1.530), // tailplane dihedral matched to the front view; was z 1.2
                // Figure 2-2-0-991-001-A01 sheet 1 (side view): fin root
                // leading edge 53.94 m aft of the nose, a 14.08 m root chord
                // (role read off the drawing), tip leading edge 12.06 m aft
                // of the root and 14.59 m above it, tip trailing edge at
                // 70.4 m. The root height puts the fin tip at the 24.12 m of
                // Figure 2-3-0-991-001-A01 (VT, MRW, aft CG); the root line
                // then sits 1.26 m below the upper-deck crown, as the drawn
                // tail top line runs below it, and the builder carries the
                // edges on down to the tail cone under the root.
                // geo/tailfit 2026-10-11: fin straight-edge fit to the drawing at the root line (rms 0.005/0.004 m); was off 18.79, chords 14.08/4.40, tip (12.06, 0.0, 14.59)
                vstab_offset_from_tail_m: 18.79,
                vstab_z_m: 3.245,
                vstab_root_chord_m: 12.04,
                vstab_tip_chord_m: 4.67,
                vstab_tip_le_m: (14.1, 0.0, 14.59),
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
                tailcone_length_m: 15.0,
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
                belly_upsweep_length_m: Some(26.97),
                aft_belly_exponent: Some(1.4),
                tail_end_z_m: Some(2.21),
                tail_end_width_m: Some(0.1),
                tail_end_height_m: Some(0.47),
                aft_plan_length_m: Some(22.77),
                aft_plan_exponent: Some(1.38),
                aft_crown_length_m: Some(19.48),
                aft_crown_exponent: Some(1.08),
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // Final merge: engines.json carries a V2 nacelle set (nacelle_shape_v2) that agrees with the measured length, diameter and inlet; was Legacy.
                nacelle_model: crate::NacelleModel::V2,
                // Figure 2-2-0-991-001-A01 sheet 1 (front view): engine
                // centrelines 29.6 m and 51.4 m apart.
                spanwise_positions_m: vec![14.8, -14.8, 25.7, -25.7],
                // Figure 2-3-0-991-001-A01 (MRW, aft CG): nacelle low points
                // 1.08 m (N1) and 1.90 m (N2) above the ground under the
                // 1.8 m nacelle radius, on the ground-shape wing above. One
                // offset serves both pairs, so they land at 1.14 m and
                // 1.84 m.
                // Low point held with the drawn 1.83 m radius (nacelle lane: -0.872 ->
                // -0.842), plus the wing lane's +0.022 m (thinner raked tip).
                // geo/close 2026-10-12 (owner: outer engine lower than the drawing): front-view circle fit puts the outer pair (y 25.7 m) 0.70 m and the inner pair 0.10 m above the model; one z_m
                // serves both pairs, so raised 0.30 m (outer 0.4 m low, inner 0.2 m high; the published N1/N2 low points 1.08/1.90 m are a MRW aft-CG clearance, now 1.44/2.14 m static). was -0.820
                z_m: -0.520,
                // geo/a380fix 2026-10-12 (owner): inboard and outboard pairs fitted separately to the drawn UNLOADED nacelles (low points 1.14 m and 2.31 m above the drawn ground line; the loaded table is not the target). Order of spanwise_positions_m.
                z_per_engine_m: vec![-0.706, -0.706, -0.217, -0.217],
                // Sheet 2 inlets 22.23 m and 29.94 m aft of the nose; the mean
                // of the two offsets from the leading edge leaves each inlet
                // within 0.17 m.
                // geo/close 2026-10-12: nacelle fronts 0.25 m aft of the model on the drawing (top best shift dx +0.40 / +0.15 for N1/N2, side +0.55); was 6.3683
                inlet_x_offset_m: 6.1183, // geometry round 2 (wing lane): unchanged by step B w1; was 6.285 before round 2 - geometry round 2 (wing lane): was 6.285; the wing datum and sweep moved by +0.0833 m at the engine, this keeps the inlet station
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
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("advanced_highlift_widebody"),
    }
}
