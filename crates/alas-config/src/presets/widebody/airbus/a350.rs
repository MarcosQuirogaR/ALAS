// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airbus A350-941 preset, WV020 with Trent XWB-84 engines.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, WingConfig,
};

/// Composite long-range twin, drawn from the Airbus A350 Aircraft Characteristics.
pub fn a350_900() -> AircraftPreset {
    AircraftPreset {
        name: "A350-900",
        display_name: "Airbus A350-900",
        class: crate::presets::AircraftClass::Transport,
        description: "Airbus A350-941 WV020 with Trent XWB-84 engines.",
        identity: AircraftVariantIdentity {
            model: "A350-941",
            weight_variant: "WV020",
            engine_model: "Trent XWB-84",
            modification_state: "WV020 283 t planning baseline",
            tank_configuration: "166,488 L usable, mod 110211",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Vol. I Table 1-1: 52 m <= b < 65 m is code E (span 64.75 m).
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::E),
            // Airbus A350 AC Jul 01/26 section 2-1-0, WV020; EASA TCDS A.151 Issue 33 III.1.7.
            mrw_kg: Some(283_900.0),
            mtow_kg: Some(283_000.0),
            mlw_kg: Some(207_000.0),
            mzfw_kg: Some(195_700.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("A350-900"),
            // EASA TCDS A.151 Issue 33 III.2.2: mod 110211, usable, 0.785 kg/L.
            usable_fuel_volume_l: Some(166_488.0),
            usable_fuel_mass_kg: Some(130_693.0),
            fuel_density_kg_l: Some(0.785),
            // Poll and Schumann 2020 Part 2 Table 1 (Airbus definition); no Airbus or EASA figure on file.
            reference_wing_area_m2: Some(445.0),
            planning_seats: Some(325),
            // Airbus A350 AC Feb 01/26, 3-2-0 Page 2, figure 3-2-0-991-001-A01:
            // the 325-passenger line (95 kg each) meets the MTOW 280,000 kg
            // segment at 8,000 nmi, read on the figure's own grid (500 nmi per
            // 37 pt); both tank curves coincide there. The figure flies a
            // typical international flight profile and prints no reserve
            // terms, so the configured fuel policy applies.
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: None,
                cruise: None,
                range_nmi: 8_000.0,
                payload_kg: Some(325.0 * 95.0),
                source: "Airbus A350 Aircraft Characteristics, Feb 01/26, 3-2-0 Page 2, figure 3-2-0-991-001-A01: 325 passengers at 95 kg, 8,000 nmi at MTOW 280,000 kg, standard day, typical international flight profile; reserve terms not printed",
                reserve: None,
            }),
            // EASA TCDS A.151 Issue 33 III.2.5: A-A-A-A exit configuration.
            certified_max_seats: Some(440),
            certified_exit_layout: Some(crate::presets::A350_900_EXIT_LAYOUT),
            // Design mission, one provenance row per datum: the 325-passenger point of the AC
            // chart. Its 'typical international flight profile' is the reserve wording; the
            // regulation supplies the terms.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::Manufacturer,
                    stated_conditions: Some("standard day, typical international flight profile, 325 passengers at 95 kg including baggage, basic configuration with crew rest compartments: 8,000 nmi on the 280,000 kg MTOW curve (read 8,014 nmi at the 325-passenger line). The chart is drawn at 280,000 kg against this preset's 283,000 kg; the 3,000 kg of extra take-off mass is carried in the uncertainty"),
                    relative_uncertainty: Some(150.0 / 8_000.0),
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "Airbus A350 Aircraft Characteristics Rev Feb 01/26, section 3-2-0 p.2, Figure 3-2-0-991-001-A01 (A350-900 payload/range)",
                }),
                payload: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "325 passengers at 95 kg including baggage: 30,875 kg",
                    "Airbus A350 Aircraft Characteristics Rev Feb 01/26, section 3-2-0 p.2, Figure 3-2-0-991-001-A01",
                )),
                profile: Some(crate::MissionDatumProvenance::exact(
                    crate::MissionEvidenceTier::Manufacturer,
                    "standard day, typical international flight profile, cruise Mach 0.85; no climb or descent schedule is printed, so the model's configured schedules apply to those phases",
                    "Airbus A350 Aircraft Characteristics Rev Feb 01/26, section 3-2-0 Figure 3-2-0-991-001-A01; Airbus A350 Family Facts and Figures, June 2026, p.3 (cruise Mach 0.85)",
                )),
                reserve_fuel: Some(crate::MissionDatumProvenance::regulatory_reserve(
                    "reference chart prints no reserve terms; its 'typical international flight profile' is Airbus's wording for international reserves (Airbus A350 Aircraft Characteristics Rev Feb 01/26, Figure 3-2-0-991-001-A01), and the design mission applies the regulatory policy: contingency, alternate and 30 min final reserve fuel, at the alternate distance and holding height of the preset's configured fuel policy. The range comparison carries this reserve-basis uncertainty",
                )),
            },
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            sources: vec![
                "Airbus A350 Aircraft Characteristics, Jul 01/26, sections 2-1-0, 2-2-0, 2-3-0, 3-2-0, 7-2-0",
                "EASA.A.151 Issue 33, 2026-07-01, sections III.1.7, III.2.2, III.2.5",
                "EASA.E.111 Issue 20, 2026-03-30, sections III.2, III.5, III.6",
                "Airbus A350 Aircraft Characteristics, Rev 16 Feb 01/26, 2-2-0 Page 3 (plan view, measured; same printed values in Rev 17) and 3-2-0 Page 2, figure 3-2-0-991-001-A01",
                "Poll and Schumann 2020, Part 2, Table 1",
            ],
            high_speed_protection: Some(
                crate::presets::operating_limits::A350_HIGH_SPEED_PROTECTION,
            ),
            operating_speed_limits: Some(crate::presets::operating_limits::A350_900),
            ..AircraftReferenceData::default()
        },
        engine_name: "Trent XWB-84",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            // AC 7-2-0: main 1400x530R23 42PR (rating not catalogued, 40PR nearest), nose 1050x395R16 28PR.
            mlg_tire: "1400x530R23/40PR".to_owned(),
            nlg_tire: "1050x395R16/28PR".to_owned(),
            wheels_per_mlg_strut: 4,
            track_diameter_factor: 10.60 / 5.96,
            // AC 2-2-0 fig 991-001 sheet 1: nose-NLG 4.63 m, wheelbase 28.66 m, track 10.60 m.
            reference_wheelbase_m: Some(28.66),
            reference_track_m: Some(10.60),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(65.26),
            reference_nlg_x_fraction: Some(4.63 / 65.26),
            reference_mlg_x_fractions: Some(vec![33.29 / 65.26, 33.29 / 65.26]),
            // AC fig 2-3-0-991-001-A01 (MRW, forward CG): fuselage bottom at door 1, F1 2.40 m.
            fuselage_ground_clearance_m: Some(2.40),
            // Tail-scrape requirement: no published tail-strike attitude found (A350 AC prints none; the 8.6 deg
            // PFD limit in the AAIB A350-1000 report is the longer -1000's, with margin), so the 10 deg default
            // (an assumption) stands.
            trimmable_stabilizer: true,
            // FAA TCDS T00063IB (Rev 29, p.3, "Control Surface Movements"): stabiliser +0.8/-13.7 deg,
            // elevator +20/-30 deg, positive = trailing edge down (nose-up stop 13.7, up travel 30).
            stabilizer_nose_up_travel_deg: Some(13.7),
            elevator_up_travel_deg: Some(30.0),
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 64.75, // AC 2-2-0 fig 991-001 sheet 1: over the blended tips
            // AC sheet 2 plan view; edges read off the calibrated raster (25.6 px/m). Straight edges stop at the device root, y 31.47 m.
            root_chord_m: 16.13, // datum: inboard trailing edge carried to the centreline, x 35.9 m
            break_chord_m: 7.90, // plan view at y 10.85 m: leading edge 28.45 m, trailing edge 36.35 m
            tip_chord_m: 1.928, // straight edges at y 31.47 m: 42.88 m to 44.81 m (trailing edge as read)
            // One straight leading edge from the kink (28.447 m at y 10.847 m) through the printed rake start,
            // x 46.68 - 5.18 = 41.50 m at y 32.375 - 2.88 = 29.495 m (sheet 2), 0.2 mm off. Was 34.78 deg (0.10 m
            // forward of the printed point); a review scan read 34.79 deg, the raster fit 35.18 deg.
            sweep_deg: 34.989,
            tip_twist_deg: -2.0,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            // AC sheet 1: body 65.26 m to the tail-cone cut. 66.80 m ends at the tailplane tip, 66.61 m at the fin tip.
            fuselage_length_m: 65.26,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        speed_schedule_source: crate::presets::SpeedScheduleSource::DefaultRule,
        airfoil_class_source: "Design-era declaration: the A350 (EIS 2015) wing is of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC(2)-0612 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Published area only: no MAC or LEMAC on file. MAC and LEMAC are derived, no published MAC: they come from the reference
                // trapezoid (about 7.54 m). Searched 2026-11-19: EASA TCDS EASA.A.151 Issue 33 (01.07.2026), Section 1 item 1.6 (CG range by
                // reference to the AFM; no datum, MAC or LEMAC); FAA TCDS T00063IB Rev 29 p.2 (reference zero datum 5.34 m forward of the nose
                // section; no MAC or LEMAC); Airbus A350 Aircraft Characteristics Rev 16 Feb 01/26 (CG only in % MAC, e.g. 2-9-1, 2-14-1 and
                // 7-3-0; no MAC length or LEMAC station). Section 7-3-0 two-point statics would be a derived value and are not used here.
                reference_wing: crate::ReferenceWingConfig::published(445.0, None, None),
                root_datum_x_m: 19.77, // plan view: inboard leading-edge line at the centreline
                inboard_le_sweep_deg: Some(38.66), // datum to the kink; the edge station below draws the break
                // AC sheet 1 front view, upper line clear of the tailplane and nacelle, 0.0835 rise per metre, set on the fuselage section; fig 2-3-0-991-001-A01 W1 and F1 (jacked) agree at the tip. Mid-span raised 0.15 m: drawn upper dihedral 4.7 deg.
                root_z_m: -1.04,
                break_z_m: 0.06,
                tip_z_m: 2.38, // sheet 1 front view: upper line at the device root
                root_twist_deg: 3.5,
                break_twist_deg: 1.5,
                break_span_fraction: 0.35,
                side_of_body_span_fraction: Some(0.0953), // y 3.00 m: printed body half width 2.98 m
                side_of_body_chord_ratio: Some(0.8561), // chord 13.81 m: printed 13.47 m between its lines, outline 13.8 m
                kink_span_fraction: Some(0.3447), // plan view: trailing-edge break at y 10.85 m
                outboard_sweep_decrement_deg: 2.0,
                // Plan view: the leading edge cranks from 41.6 deg inboard to the straight kink-to-rake line, inboard of the trailing-edge kink; the crank is where the two lines meet (the pylon hides the drawn corner).
                edge_stations: vec![
                    crate::WingEdgeStation { span_fraction: 0.2270, le_offset_m: 0.37068, te_offset_m: 0.0 }, // LE crank on the straight kink-to-rake line, where the inboard line meets it (y 7.14 m)
                ],
                root_airfoil: "sc20612".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                airfoil_class: crate::AirfoilClass::SupercriticalAdvanced,
                // No Airbus sections public: NASA CRM t/c 0.154, 0.138, 0.105, 0.095 (Vassberg et al., AIAA 2008-6919, Table 2).
                thickness_stations: vec![
                    crate::WingThicknessStation { span_fraction: 0.0000, thickness_scale: 1.2838 },
                    crate::WingThicknessStation { span_fraction: 0.0953, thickness_scale: 1.1504 },
                    crate::WingThicknessStation { span_fraction: 0.3447, thickness_scale: 0.8753 },
                    crate::WingThicknessStation { span_fraction: 1.0000, thickness_scale: 0.9539 },
                ],
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                // AC sheet 2 front view: leading-edge heights under the drawn upper line.
                dihedral_stations: vec![
                    crate::WingDihedralStation { span_fraction: 0.4607, z_m: 0.43, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.6195, z_m: 0.93, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.7784, z_m: 1.39, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.8896, z_m: 1.64, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.9372, z_m: 1.77, flight_rise_m: None },
                    crate::WingDihedralStation { span_fraction: 0.9690, z_m: 1.93, flight_rise_m: None },
                ],
                // Plan view: the leading edge leaves the straight line at the printed rake start, 2.88 m inboard of the
                // 32.375 m half span (y 29.495 m of the 31.467 m planar semispan; the front-view station 0.9372 above
                // agrees), and reaches 43.06 m at the device root (chord 1.75 m).
                tip_rake_tip_chord_m: Some(1.75),
                tip_rake_start_span_fraction: Some(0.9373),
                // AC sheets 1-2: the blended tip reaches the 64.75 m span and 46.68 m at its trailing point. Radius, cant and height from the front
                // view (top z 4.1 m); sweep puts the tip on 46.68 m with a 0.25 m tip chord. Carried in the lattice as on the A320-200.
                wingtip_device: Some(crate::WingtipDevice {
                    kind: crate::WingtipDeviceKind::Sharklet,
                    height_m: 1.75, // sheet 1 front view: top z 4.1 m; planar span held
                    down_height_m: 0.0,
                    cant_deg: 15.0, // sheet 1 front view: the straight line above z 3.2 m
                    root_chord_m: None,
                    root_te_flare: None,
                    tip_chord_m: 0.25,
                    le_sweep_deg: 57.95, // the drawn tip x held on the shorter path of the arc below
                    toe_out_deg: 0.0,
                    blend_radius_m: 1.301, // reach 0.908 m, planar span held
                    arc_start_deg: Some(27.36), // slope of the last wing panel, so the root has no kink
                    path_stations: Vec::new(),
                    airfoil: None,
                    span_includes_device: true,
                    include_in_vlm: true,
                }),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                // Drawn tip edges read off the Airbus A350 Aircraft Characteristics tail views; analysis stays on
                // the trapezoid.
                // Sheet 2 plan: tip rounding only. The printed trapezoid (66.80 m tip, 2.46 m tip chord) is the trailing edge.
                // The leading edge leaves its straight line at 0.905 of the span.
                hstab_edge_stations: vec![
                    crate::TailEdgeStation {
                        span_fraction: 0.905,
                        le_offset_m: 0.0,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.94,
                        le_offset_m: 0.12,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.96,
                        le_offset_m: 0.3,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.98,
                        le_offset_m: 0.55,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 1.0,
                        le_offset_m: 1.0,
                        te_offset_m: 0.0,
                    },
                ],
                // Sheet 1 side view: tip rounding only. The printed chords and stations (tip trailing point 66.61 m) make the trailing edge one straight line.
                vstab_edge_stations: vec![
                    crate::TailEdgeStation { span_fraction: 0.97, le_offset_m: 0.0, te_offset_m: 0.0 },
                    crate::TailEdgeStation {
                        span_fraction: 0.98,
                        le_offset_m: 0.08,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 0.99,
                        le_offset_m: 0.28,
                        te_offset_m: 0.0,
                    },
                    crate::TailEdgeStation {
                        span_fraction: 1.0,
                        le_offset_m: 0.74,
                        te_offset_m: 0.0,
                    },
                ],
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 8.02, // AC sheet 2: apex 66.80 - 9.62 = 57.18 m, outline 57.28 m
                hstab_z_m: 1.17, // AC sheet 1 front view: upper line 1.64 m at 0.55 semispan
                hstab_root_chord_m: 6.40, // plan outline at the centreline; the printed 6.21 m ends at the body junction
                hstab_tip_chord_m: 2.46, // printed, sheet 2
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (7.12, 9.395, 1.01), // tip leading edge 66.80 - 2.46; printed span 18.79 m; front-view rise 0.089 per metre
                // AC sheet 1, fin datum line: leading edge 66.61 - 3.04 - 9.09 = 54.48 m, root chord 7.79 m, tip chord 3.04 m, height 9.42 m.
                vstab_offset_from_tail_m: 10.71, // leading edge 54.55 m: outline at the raster scale, 0.07 m aft of the printed station
                vstab_z_m: 2.36, // datum line: tip z 11.78 m on the side outline less the printed height
                vstab_root_chord_m: 7.79,
                vstab_tip_chord_m: 3.04,
                vstab_tip_le_m: (9.09, 0.0, 9.42),
                vstab_dorsal_length_m: Some(0.50), // side view: leaves the crown at x 54.05 m, joins the leading edge about z 3.36 m
                vstab_dorsal_height_m: Some(1.00),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                // AC 2-2-0 fig 991-001: width 5.96 m (sheet 2), height 6.09 m (sheet 1). Outer shape: the embedded loft.
                diameter_m: 5.96,
                height_m: Some(6.09),
                nose_z_m: -1.21, // side view: tip height, as the loft
                cabin_start_x_m: 5.5,
                cabin_z_m: 0.2,
                tailcone_length_m: 8.72, // cabin end 5.5 + 51.04 m (Airbus A350-900 web page), as the loft
                tail_z_m: 1.365, // side view: centre of the tail-cone cut, as the loft
                // Hold section: Airbus A350 AC Feb 01/26, 2-7-0 Page 13, figure 2-7-0-991-004-A01
                // (forward cargo compartment door), section A-A, read on the embedded drawing at
                // 145.05 px/m from the printed 1.76 m hold height; the same scale gives the outer
                // section 5.96 m wide and 6.08 m tall (AC 2-2-0: 5.96 and 6.09 m). Ceiling 3.69 m
                // below the outer crown, 1.76 m to the floor (printed), flat floor 3.20 m between
                // the frames, side walls 4.23 m apart, wall foot 0.55 m above the floor. AC 2-6-0
                // loads LD3 two abreast and 125 in (3.175 m) pallets across this hold.
                cabin_section: Some(crate::CabinSectionConfig {
                    floor_below_crown_m: None,
                    cabin_widths: None,
                    hold: Some(crate::HoldSectionConfig {
                        ceiling_below_crown_m: Some(3.69),
                        height_m: 1.76,
                        floor_width_m: 3.20,
                        side_width_m: 4.23,
                        chamfer_height_m: 0.55,
                    }),
                }),
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                nacelle_model: crate::NacelleModel::V2,
                // AC sheet 2: engine centreline 10.50 m, inlet face 21.97 m aft of the nose.
                spanwise_positions_m: vec![10.50, -10.50],
                z_m: -2.46, // AC fig 2-3-0-991-001-A01 N1 against F1: low point 1.64 m below the keel (MRW, forward CG)
                inlet_x_offset_m: 6.20, // inlet face on the printed 21.97 m
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.85,
            cruise_altitude_m: 11_887.2,
            mtow_kg: 283_000.0,
            max_wing_area_m2: 450.0,
            min_wing_loading_kg_m2: 480.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 325,
            cargo_payload_kg: 55_000.0,
            max_structural_payload_kg: 56_400.0,
            // Superseded at registration: CS 25.335(b)(1)(ii) from the secondary-source
            // VMO/MMO (`operating_limits`).
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("advanced_highlift_widebody"),
    }
}
