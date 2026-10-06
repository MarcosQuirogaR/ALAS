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
            root_chord_m: 17.934_637_703_217_59,
            break_chord_m: 11.738_539_338_676_212,
            tip_chord_m: 3.98,
            // Airbus A380 Facts and Figures (2022) gives 33.5 deg wing
            // sweep; Jane's identifies the quarter-chord convention. The
            // outboard taper converts that to this leading-edge angle, which
            // puts the tip leading edge 46.85 m aft of the nose against the
            // 46.97 m drawn.
            sweep_deg: 36.419_887_695_518_42,
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
                // Quarter-MAC: joint fit of 30 section 7 pavement-load rows gives
                // LEMAC 28.765 m aft of nose, MAC 12.295 m, which the model
                // planform now reproduces; this datum puts its quarter-MAC
                // point on the manufacturer's 31.839 m.
                root_datum_x_m: 24.930,
                // Ground shape fitted to the Figure 2-3-0-991-001-A01 static
                // clearances (MRW, aft CG): the wing tip lower surface 5.21 m
                // above the ground (W2) and a flat inboard panel; the exact
                // fit to the engine clearances N1/N2 would put the kink 0.3 m
                // lower still. This drooped shape has 2.3 deg of dihedral.
                root_z_m: -2.5,
                break_z_m: -2.5,
                tip_z_m: -0.876,
                // Airbus A380 Facts and Figures (February 2022): "During
                // take-off the wing will flex upwards by over 4m". That 4 m
                // static-to-1 g tip rise over the 39.875 m semispan gives the
                // flight shape 8.0 deg of dihedral; "over" makes it a lower
                // bound.
                flight_tip_rise_semispan_fraction: Some(4.0 / 39.875),
                root_twist_deg: 4.5,
                break_twist_deg: 2.0,
                break_span_fraction: 0.33,
                // The plan view's 17.67 m chord at the side of the 7.14 m
                // wide body (Figure 2-2-0-991-001-A01 sheet 2).
                side_of_body_span_fraction: Some(3.57 / 39.875),
                side_of_body_chord_ratio: Some(0.985_244_323_994_896_8),
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
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // Figure 2-2-0-991-001-A01 sheet 2 (plan view): tailplane tip
                // leading edge 68.85 m aft of the nose, 11.57 m aft of the
                // root leading edge (57.28 m), and a 3.72 m tip chord whose
                // trailing edge closes the 72.57 m plan length. The root
                // chord and height are not dimensioned and stay estimates.
                hstab_offset_from_tail_m: 15.45,
                hstab_z_m: 1.5,
                hstab_root_chord_m: 9.0,
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
                // (9.0 + 3.72) x 15.185 = 193.2 m^2.
                hstab_tip_le_m: (11.57, 15.185, 1.2),
                // Figure 2-2-0-991-001-A01 sheet 1 (side view): fin root
                // leading edge 53.94 m aft of the nose, a 14.08 m root chord
                // (role read off the drawing), tip leading edge 12.06 m aft
                // of the root and 14.59 m above it, tip trailing edge at
                // 70.4 m. The root height puts the fin tip at the 24.12 m of
                // Figure 2-3-0-991-001-A01 (VT, MRW, aft CG); the root line
                // then sits 1.26 m below the upper-deck crown, as the drawn
                // tail top line runs below it, and the builder carries the
                // edges on down to the tail cone under the root.
                vstab_offset_from_tail_m: 18.79,
                vstab_z_m: 3.245,
                vstab_root_chord_m: 14.08,
                vstab_tip_chord_m: 4.40,
                vstab_tip_le_m: (12.06, 0.0, 14.59),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 7.14,
                // The one non-circular body in the registry: two decks make it
                // taller than it is wide.
                height_m: Some(8.41),
                // Measured nose (v1.3.2): Airbus A380 AC (Dec 2025) fig 2-2-0-991-001-A01 sheets 1-2, PDF pp 34-35,
                // Outline read from the drawing, nose length = L(2 %) (full section within
                // 2 % of D_eff), tip height from the drawn mid-line (extrapolated where the
                // dimension line hides the tip). Laws fitted at fixed
                // length to the upper, lower and plan lines (RMS 0.02-0.04 D_eff); the section
                // exponent is not measurable from an airport-planning drawing and stays unset.
                nose_z_m: -1.45,
                cabin_start_x_m: 10.78,
                cabin_z_m: 0.3,
                tailcone_length_m: 15.0,
                tail_z_m: 2.0,
                nose_windshield_angle_deg: Some(36.0),
                nose_crown_end_fraction: Some(1.0),
                nose_radome_length_fraction: Some(0.1),
                nose_keel_exponent: Some(2.26),
                nose_plan_exponent: Some(1.52),
                nose_section_exponent: None,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // Figure 2-2-0-991-001-A01 sheet 1 (front view): engine
                // centrelines 29.6 m and 51.4 m apart.
                spanwise_positions_m: vec![14.8, -14.8, 25.7, -25.7],
                // Figure 2-3-0-991-001-A01 (MRW, aft CG): nacelle low points
                // 1.08 m (N1) and 1.90 m (N2) above the ground under the
                // 1.8 m nacelle radius, on the ground-shape wing above. One
                // offset serves both pairs, so they land at 1.14 m and
                // 1.84 m.
                z_m: -0.872,
                // Sheet 2 inlets 22.23 m and 29.94 m aft of the nose; the mean
                // of the two offsets from the leading edge leaves each inlet
                // within 0.17 m.
                inlet_x_offset_m: 6.285,
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
