// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! McDonnell Douglas DC-10 preset (kept apart from the Boeing 787-9 table to
//! bound the size of each preset file).

use super::outer_shape;
use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, WingConfig,
};

/// Trijet with a centerline engine, the oldest type in the registry.
pub fn dc_10() -> AircraftPreset {
    AircraftPreset {
        name: "DC-10",
        display_name: "McDonnell Douglas DC-10-30 (572k option)",
        description: "DC-10-30 ACAP 572,000 lb option with three CF6-50C engines.",
        identity: AircraftVariantIdentity {
            model: "DC-10-30 passenger",
            weight_variant: "ACAP 572,000 lb option",
            engine_model: "CF6-50C family",
            modification_state: "DAC-67803A Rev A footnoted 572k planning option",
            tank_configuration: "36,652 US gal with center-wing auxiliary tank",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Vol. I Table 1-1 (aerodrome reference code) applied to the
            // preset wingspan of 50.39 m (McDonnell Douglas DC-10-30): 36 m <= b < 52 m
            // is code D.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::D),
            // The ACAP 572,000 lb footnote restates no ramp limit or OEW; the
            // standard-row values (253,105 kg MRW) are not mixed in here.
            mrw_kg: None,
            mtow_kg: Some(259_454.0),
            mlw_kg: Some(190_962.0),
            // Boeing DC-10 ACAP (boeing_2023_dc10acap.pdf) PDF p.11, section 2.1, DC-10-30CF
            // passenger column: MZFW 391,000 lb (177,355 kg). The former 166,922 kg is
            // 368,000 lb, a value of the 590,000 lb takeoff-weight block of FAA TCDS A22WE.
            mzfw_kg: Some(177_355.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("DC-10"),
            usable_fuel_volume_l: Some(137_509.0),
            usable_fuel_mass_kg: Some(111_387.0),
            reference_wing_area_m2: Some(338.84),
            planning_seats: Some(255),
            design_point: Some(crate::PayloadRangeDesignPoint {
                range_nmi: 4_120.0,
                payload_kg: Some(45_993.0),
                source: "Boeing DC-10 Airplane Characteristics for Airport Planning section 3.2.1 p.54 (payload/range M0.82 step cruise, Series 30, 572,000 lb MTOGW curve): maximum-payload corner read as 4,120 nmi at 101,396 lb (45,993 kg); read uncertainty +-100 nmi; reserves per FAR 121.645 at a 200 nmi alternate",
            }),
            certified_max_seats: Some(399),
            certified_exit_layout: Some(crate::presets::DC_10_30_EXIT_LAYOUT),
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            // The ACAP publishes the Series 30 ground envelope at the
            // 583,000 lb maximum ramp weight; the 572,000 lb option lies
            // inside it.
            aft_cg_nose_load: Some(crate::presets::gear_load::DC_10_30),
            sources: vec![
                "Boeing DC/MD-10 ACAP DAC-67803A Rev A, Figure 2.1",
                "FAA TCDS A22WE Rev 13, 2018-04-30",
                "NASA CR-3119, April 1979",
            ],
            ..AircraftReferenceData::default()
        },
        engine_name: "CF6-50",
        n_engines: 3,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 3,
            // Two four-wheel wing bogies plus one two-wheel center gear.
            mlg_strut_bogie_wheels: Some(vec![4, 4, 2]),
            wheels_per_mlg_strut: 0,
            track_diameter_factor: 1.77,
            // DAC-67803A Rev A, Figure 7.2.2 (footprint, Series 30): nose
            // gear to wing-gear bogie centers 72 ft 4.6 in (22.06 m), center
            // gear axle 30 in (0.76 m) aft of them, wing-gear track 35 ft
            // (10.67 m). Figure 2.2 (Series 30 side view, printed length
            // 181 ft 7.2 in, 55.35 m) places the nose gear 8.1 m aft of the
            // nose tip, scaled against its own 22.07 m wheelbase dimension
            // (+-0.2 m, a drawing read).
            reference_wheelbase_m: Some(22.06),
            reference_body_wheelbase_m: Some(22.82),
            reference_track_m: Some(10.67),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(55.35),
            reference_nlg_x_fraction: Some(8.1 / 55.35),
            reference_mlg_x_fractions: Some(vec![30.16 / 55.35, 30.16 / 55.35, 30.92 / 55.35]),
            // No published tail-strike attitude backs this preset's aft-fuselage
            // geometry, so its model tail-down angle is unvalidated (the generic
            // tailcone loft understates it): keep the Raymer/Roskam 15 deg floor on
            // top of the tail-down criterion.
            min_tip_back_deg: 15.0,
            takeoff_stabilizer_nose_up_deg: Some(crate::landing_gear::TRANSPORT_THS_TAKEOFF_NOSE_UP_DEG),
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 50.39,
            // The Douglas reference wing: the straight-tapered trapezoid of
            // the 338.84 m2 TCDS area (IM.A.210, 3,647.5 ft2) over this span
            // whose MAC is the 7.51 m weight-and-balance chord (NASA
            // CR-3677, 1984: Re 6.95e6 at 19.7e6 /m on the 4.7 % model).
            // Area, span and MAC fix the taper at 0.2562; the 2.74 m tip
            // chord agrees with DAC-67803A Rev A Figure 2.2 (about 2.7 m,
            // drawing read). The inboard trailing-edge extension of the real
            // planform lies outside the reference wing and is not modelled.
            // Step B w2 (measure/DC-10, DAC-67803A Rev A Figure 2.2 p.14 at 400 dpi, B): the drawn inboard trailing edge
            // (the real planform's Yehudi extension) is now modelled: trailing edge x = 33.6 m at the side of body
            // (y = 2.97 m), 34.5 m at the kink (y = 9.44 m), straight to 41.85 m at the tip (chord 2.64 m drawn). With the
            // leading edge at the drawn 21.9 m at the side of body, these chords are root 13.61 m (drawn extrapolation
            // 13.7 m), kink 7.564 m, tip 2.64 m. The gross planform area is then about 360 m2, the 361 m2 of the drawing,
            // against the published 338.84 m2 (`reference_wing_area_m2`, the reference-wing convention, DECISIONS).
            // Was the 338.84 m2 trapezoid: 10.705_424_735_302_275, 7.918_672_469_211_44, 2.743_275_403_614_176.
            root_chord_m: 13.61,
            break_chord_m: 7.564,
            tip_chord_m: 2.64,
            // NASA CR-3119 (1979) specifies 35 deg at quarter chord; the
            // reference taper converts it to this leading-edge angle.
            sweep_deg: 37.926_177_529_939_146,
            tip_twist_deg: -2.0,
            wing_x_shift_m: -3.25,
            tail_scale: 1.0,
            // DAC-67803A Rev A Figure 2.2: printed 181 ft 7.2 in, the length
            // the gear station fractions above are normalized to.
            fuselage_length_m: 55.35,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        airfoil_class_source: "Design-era declaration: the DC-10 wing was designed in 1966-1968 (EIS 1971), before supercritical sections, first flown on the NASA F-8 SCW in 1971, reached transport service, so its technology is conventional (NACA 6-series era). The drawn root and tip stations are the DSMA-523A surrogate (UIUC Airfoil Coordinates Database; 11.0 % t/c), the nearest published member of the Douglas Santa Monica series the DC-10 used (DSMA-496, -521 and -522 at the root, -519 and -520 at the tip; Lednicer, UIUC Incomplete Guide to Airfoil Usage; coordinates unpublished); it fixes thickness and shape only, its aft loading is not credited in the Korn factor, and it does not change this declaration.",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Published reference (freeze 2026-10-12): S 338.84 (landing_reference.rs); drawn S 360.7, trapezoid 331.4; MAC and LEMAC not retrieved.
                reference_wing: crate::ReferenceWingConfig::published(338.84, None, None),
                // Wing-root leading edge at the centreline 19.33 m aft of the
                // nose (19.26 m on the 55.35 m drawing): the straight leading
                // edge of DAC-67803A Figure 2.2 meets the fuselage side at
                // 21.6 m. With the 3.25 m design-vector shift this places
                // LEMAC at 27.21 m, against 27.23 m from the Figure 7.4.2
                // nose-gear point (chart-derived, +-0.3 m).
                // Step B w2: +0.25 m (drawn leading edge 21.85 m at the side of body, 39.3 m at the tip, B: the model was
                // 0.21 m and 0.34 m forward). Was 22.58 m; the model quarter-MAC point moves 0.37 m forward with the
                // wider inboard planform (MAC 8.48 m against the 7.51 m weight-and-balance chord, open for phase 2).
                root_datum_x_m: 22.83,
                // Step B w2: the front view gives a straight mid-surface line, 5.0 deg from the side of body (z -1.2 m)
                // to the tip (0.75 m); the round-2 heights put the lower surface below the belly at the body. Straight
                // leading-edge line z = -1.5 + 0.0893 y (B). Was root -2.67, break -0.7528, tip 0.9391 (round 2); -1.6, -0.3, 2.0 earlier.
                root_z_m: -1.5000,
                break_z_m: -0.6569,
                tip_z_m: 0.7500,
                root_twist_deg: 4.0,
                break_twist_deg: 1.5,
                break_span_fraction: 0.3747, // step B w2: drawn yehudi trailing-edge kink y = 9.44 m (was 0.35, y = 8.82 m)
                // The reference trapezoid's own chord at the side of body,
                // pinned independently of the automatic clipping rule.
                // Step B w2: dropped (None = the chord on the root-to-kink trailing edge, 12.0 m at the default 0.10 semispan
                // station): the pinned 0.9256 ratio would have kinked the drawn straight inboard trailing edge.
                side_of_body_chord_ratio: None,
                kink_span_fraction: Some(0.3747), // step B w2; was 0.35
                outboard_sweep_decrement_deg: 2.0,
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline, // step B w2 (loft-wing report); was Linear
                // Section family: Douglas Santa Monica sections, DSMA-496,
                // -521 and -522 at the root and DSMA-519 and -520 at the tip
                // (D. Lednicer, "The Incomplete Guide to Airfoil Usage",
                // UIUC Airfoil Data Site, entry "Douglas DC-10-30"). Their
                // coordinates are not published. Thickness: 11.0 % average
                // t/c for the DC-10-10 and -30 (L. R. Jenkinson, P. Simpkin
                // and D. Rhodes, "Civil Jet Aircraft Design", Arnold, 1999,
                // Data A, Table 6). No published root, kink or tip t/c was
                // found, so the wing carries one section at that average:
                // DSMA-523A (UIUC Airfoil Coordinates Database, "McDonnell/
                // Douglas DSMA-523 transonic airfoil with sharp trailing
                // edge"), the nearest member of the same Douglas series in
                // the library, whose maximum t/c is 0.110.
                root_airfoil: "dsma523a".to_owned(),
                tip_airfoil: "dsma523a".to_owned(),
                // Korn class: the DSMA aft-loaded sections have no sourced
                // Korn kappa, so the conventional class (0.87) is the sourced
                // lower bound (Mason, ch. 7; Malone & Mason 1995).
                airfoil_class: crate::AirfoilClass::Conventional,
                // Thickness profile: the DC-10 final wing as printed in Obert, Aerodynamic Design
                // of Transport Aircraft (2009), Fig. 24.21 (book p. 208, PDF p. 227, from AIAA
                // 69-830; read on the page image): t/c 12.2 % at the root, 11.2 % at 20 % semispan,
                // 9.5 % at the 37 % break, 9.0 % at 60 % and 8.4 % at the tip. The single 11.0 %
                // average of Jenkinson Data A (which fixes the 11.0 % blend above) over-thickened
                // the outer wing; with it the Korn wave drag was 39 counts at M 0.82 and CL 0.49,
                // where Obert Fig. 24.24 (DC-10-10 flight test) shows 9-12 counts of total
                // compressibility drag rise at CL 0.3-0.4. Multipliers act on the 0.110 blend.
                thickness_stations: vec![
                    crate::WingThicknessStation {
                        span_fraction: 0.0000,
                        thickness_scale: 1.1091, // t/c 0.1100 -> 0.1220
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.2000,
                        thickness_scale: 1.0182, // t/c 0.1100 -> 0.1120
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.3747,
                        thickness_scale: 0.8636, // t/c 0.1100 -> 0.0950
                    },
                    crate::WingThicknessStation {
                        span_fraction: 0.6000,
                        thickness_scale: 0.8182, // t/c 0.1100 -> 0.0900
                    },
                    crate::WingThicknessStation {
                        span_fraction: 1.0000,
                        thickness_scale: 0.7636, // t/c 0.1100 -> 0.0840
                    },
                ],
                // DC-10 (superseded by the Obert Fig. 24.21 stations above): Jenkinson Data A gave the 11.0 % average only, so the
                // single DSMA-523A section stays; the CRM distribution was tried and made the structural relief iteration diverge.
                // Spanwise dihedral (geometry round 2): leading-edge heights at interior stations fitted to the upper edge of the
                // manufacturer front view (symmetrised drawing aligned on the fuselage crown, same measurement on the model).
                // Step B w2: stations on the straight line z = -1.5 + 0.0893 y (semispan 25.195 m); were -2.2986, -1.8274,
                // -0.9554, -0.1965, 0.1097, 0.4168 (round 2).
                dihedral_stations: vec![
                    crate::WingDihedralStation {
                        span_fraction: 0.1000,
                        z_m: -1.2750,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.2000,
                        z_m: -1.0500,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.3000,
                        z_m: -0.8250,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.5000,
                        z_m: -0.3750,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.6500,
                        z_m: -0.0376,
                        flight_rise_m: None,
                    },
                    crate::WingDihedralStation {
                        span_fraction: 0.8000,
                        z_m: 0.3000,
                        flight_rise_m: None,
                    },
                ],
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // DAC-67803A Rev A Figure 2.2 (Series 30 plan view, 5 ft
                // grid): tailplane span 71 ft 2 in (21.69 m, printed); chords
                // 8.93 m at the centreline and 3.07 m at the tip, leading edge
                // swept 40.9 deg, root leading edge 13.58 m forward of the
                // tail tip on the 55.35 m drawing (drawing reads, +-0.2 m).
                // Gross area 130.1 m2.
                // geo/close 2026-10-12: the tail-fit above read the DC-10 top scan with an isotropic 27.07 px/m although the scan is anisotropic (27.88 px/m along x): the model was drawn 3 % short
                // along x, so the fit pulled the tailplane 0.67 m forward and fattened its chord. Reverted to the values the fresh 400 dpi trace confirms (tip LE 51.05 vs 51.11, tip TE 54.17 vs 54.18):
                // offset 13.63, chords 8.93/3.07, tip (9.39, 10.845, 1.75). The tail-fit values were offset 12.96, chords 10.03/2.99, tip (9.12, 10.845, 1.75).
                hstab_offset_from_tail_m: 13.63,
                hstab_z_m: 0.87, // geometry round 2 (wing lane): upper surface at the root matched to the (hand-traced, +-0.2 m) front view; was 1.0
                hstab_root_chord_m: 8.93,
                hstab_tip_chord_m: 3.07,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (9.39, 10.845, 1.75), // tailplane dihedral 10.7 deg of the front view (hand-traced); was z 0.8
                // Fin read off the side view of DAC-67803A Rev A Figure 2.2 (300 dpi
                // scan, 27.8 px/m, drawing reads +-0.2 m; overlay study). The
                // drawn fin stands on the centre-engine nacelle: leading edge
                // 44.9 m aft of the nose and 4.3 m chord at the nacelle top,
                // tip leading edge 51.0 m, tip trailing edge 53.9 m, tip about
                // 12.4 m above the cabin axis (17.55 m overall height, EASA
                // IM.A.210). The edges are carried down at their drawn slopes
                // (44.5 deg leading edge, 37 deg trailing edge) to the fuselage
                // crown at 3.1 m: 5.0 m chord there. That panel is 36.7 m^2
                // above the crown, 53 m^2 to the fuselage axis. The fin stands
                // on the 14.15 m centre-engine pod of `engine.centre_engine`
                // (top 6.1 m at the leading edge), where the builder seats it:
                // 4.3 m chord there, as drawn. The former 10.5 m root chord and
                // 38 deg sweep were estimates with no drawn support.
                // geo/close 2026-10-12 (owner: DC-10 tail wrong): the tail-fit values (offset 15.09, chords 5.97/2.21, tip (10.6, 0, 9.3)) came from the same anisotropic-scale error plus the
                // grid lines/inset wing of the mask. Fresh 400 dpi trace (measure/DC-10, ov_side): LE 41.93 m at the crown, 44.5 m at the pod top, 50.7 m at the tip (12.4 m); TE 49.6 m at the crown,
                // 53.54 m at the tip; chord at the crown 7.7 m, tip 2.84 m. At the root line z = 3.1 m: LE 41.83 m (offset 13.52), chord 7.72 m, tip LE offset 8.87 m, tip chord 2.84 m.
                vstab_offset_from_tail_m: 13.52,
                vstab_z_m: 3.1,
                // Step B w2 (measure/DC-10, B, +-0.4 m): drawn chord at the crown 7.7 m (the leading edge is right to 0.1-0.2
                // m, the trailing edge was 2.8 m too far forward, drawn 24 deg TE sweep against 38 deg); was 5.0 m.
                vstab_root_chord_m: 7.72,
                vstab_tip_chord_m: 2.84,
                vstab_tip_le_m: (8.87, 0.0, 9.3),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 6.02,
                nose_z_m: 0.53,
                cabin_start_x_m: 5.5,
                cabin_z_m: 0.2,
                tailcone_length_m: 11.5,
                tail_z_m: 1.6,
                ..outer_shape::dc_10()
            },
            engine: EngineConfig {
                // Final merge: engines.json carries a V2 nacelle set (nacelle_shape_v2) that agrees with the measured length, diameter and inlet; was Legacy.
                nacelle_model: crate::NacelleModel::V2,
                engine_name: "CF6-50".to_owned(),
                // The third position is the centreline engine (No. 2), set by
                // `centre_engine` below, not by the underwing pair's `z_m` and
                // inlet offset. The underwing pair sits 26 ft 10 in (8.18 m)
                // from the centreline, DAC-67803A Rev A Figure 2.2 plan view.
                spanwise_positions_m: vec![8.18, -8.18, 0.0],
                z_m: -1.745, // step B w2: holds the nacelle centre at z -2.514 m with the new wing heights; was -1.659 (wing lane: -2.12 before)
                // Figure 2.2 (scan, +-0.2 m): inlet face 19.7 m aft of the nose
                // in plan and side view, 6.0 m ahead of the wing leading edge.
                inlet_x_offset_m: 6.25, // step B w2: holds the inlet face at 19.7 m with the leading edge 0.25 m aft; was 6.0
                // No. 2 engine, Series 30 Figure 2.2: a straight-through pod,
                // not an S-duct (only the outline is drawn, and it has no
                // bend): upper and lower lines parallel, both falling 0.8 m
                // over 12 m (axis inclined 4 deg), flat inlet face at 41.2 m,
                // aft bulkhead 53.1 m, 2 m tail cone to the tail. 2.85 m wide
                // (plan) by 3.0 m high (side), top 6.3 m at the face; the pod
                // stands on the crown and the fin stands on the pod.
                centre_engine: Some(crate::CentreEngineInstallation {
                    axis_z_m: 4.85,
                    axis_pitch_deg: 4.0,
                    width_m: 2.85,
                    height_m: 3.0,
                    // Step B w2: aft bulkhead at the drawn 52.4 m (B), station 11.2 m from the inlet face; was 11.9 m (53.1 m).
                    profile: vec![
                        (0.0, 0.93),
                        (0.35, 0.99),
                        (0.8, 1.0),
                        (11.2, 1.0),
                        (14.15, 0.25),
                    ],
                }),
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.82,
            cruise_altitude_m: 10668.0,
            mtow_kg: 259_454.0,
            max_wing_area_m2: 338.84,
            min_wing_loading_kg_m2: 500.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 255,
            cargo_payload_kg: 65_000.0,
            // Structural payload = MZFW 177,355 kg less the registry OEW basis 120,914 kg
            // (formerly 46,008 kg from the superseded 166,922 kg MZFW). OPEN for the owner:
            // the registry basis (266,191 lb OEW, 368,000 lb MZFW, DAC-67803A Rev A) does
            // not match Figure 2.1 on PDF p.11 (268,751 lb OEW, 391,000 lb MZFW, 122,249 lb
            // payload); the page-consistent 572k payload is 121,870 lb = 55,280 kg.
            max_structural_payload_kg: 56_441.0,
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("advanced_highlift_widebody"),
    }
}
