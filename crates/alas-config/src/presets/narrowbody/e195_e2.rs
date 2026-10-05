// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Embraer E195-E2 (type-certificate model ERJ 190-400) preset.
//!
//! Source map. The EASA TCDS EASA.IM.A.071 Issue 28 (3 Sep 2026), Section 5
//! (ERJ 190-400) is the certified source for dimensions, weights, MAC, the
//! datum, seating and exits. The Embraer E195-E2 specification sheet
//! (April 2025, `embraer.com/media/ue1bdfnq/e195-e2-spec-1.pdf`) supplies the
//! usable fuel, maximum payload, range and cabin counts. The only Embraer
//! airport-planning manual found is the E-Jets E2 APM 5824 Rev 8 (11 May
//! 2018), whose effectivity is the E190-E2 alone; where this preset reads it
//! the number is an E190-E2 value carried to the E195-E2 and is stated as
//! such. The E190-E2 to E195-E2 stretch is 5.366 m (TCDS length 36.237 m ->
//! 41.603 m); the EASA datum-to-wing-stub-front-spar distance grows from
//! 13,571 mm (ERJ 190-300) to 15,903 mm (ERJ 190-400), so, with the datum
//! fixed to the nose, the forward plug that moves wing, engines and main
//! gear aft is 2.332 m and the remaining 3.034 m is behind the wing.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CertifiedExitLayout,
    CertifiedExitPair, CgEnvelopeEvidence, DesignRequirements, DesignVector, EmpennageConfig,
    EngineConfig, FuselageConfig, GeometryConfig, LandingGearConfig, MissingDesignMissionDatum,
    MissionEvidenceApplicability, PartialDesignMissionEvidence, PartialMissionEvidenceKind,
    PublishedRange, SourcedPlanningCabin, SourcedSeatClass, WingConfig,
};

/// Forward plug between the E190-E2 and the E195-E2, m: the difference of the
/// two TCDS datum-to-front-spar distances, 15,903 mm - 13,571 mm (EASA.IM.A.071
/// Issue 28, Section 4 item III.15 and Section 5 item III.15).
const FORWARD_PLUG_M: f64 = 2.332;

/// EASA TCDS overall length of the ERJ 190-400, m.
const LENGTH_M: f64 = 41.603;

/// E190-E2 APM 5824 Rev 8 Figure 2.1: nose tip to nose-gear centreline, m.
const NOSE_GEAR_X_M: f64 = 4.55;

/// E190-E2 APM 5824 Rev 8 Figure 7.1 footprint: nose-gear to main-gear axle
/// line, m (Figure 2.1 side view prints 12.77 m to the main wheel centre).
const E190_E2_WHEELBASE_M: f64 = 12.61;

/// The ERJ 190-400's 146-seat maximum is the sum of two Type I and two Type III
/// pairs; the TCDS prints the sequence and the 146 limit.
pub const E195_E2_CERTIFIED_EXIT_LAYOUT: CertifiedExitLayout = CertifiedExitLayout {
    label: "I-III-III-I",
    pairs: &[
        CertifiedExitPair {
            exit_type: "I",
            station_m: None,
        },
        CertifiedExitPair {
            exit_type: "III",
            station_m: None,
        },
        CertifiedExitPair {
            exit_type: "III",
            station_m: None,
        },
        CertifiedExitPair {
            exit_type: "I",
            station_m: None,
        },
    ],
    station_body_length_m: None,
    source: "EASA.IM.A.071 Embraer ERJ-190 TCDS Issue 28, 2026-09-03, Section 5 (ERJ 190-400) III.19-20 p.41 (MPSC 146 for I-III-III-I; Type I fwd and aft main doors, two Type III overwing pairs, Type I fwd and aft service doors)",
};

/// The two underfloor Class C holds as `(name, forward station, aft station,
/// volume)`, metres aft of the nose tip and cubic metres.
///
/// Volumes are certified: EASA.IM.A.071 Issue 28 Section 5 III.21, 14.77 m3
/// forward and 15.20 m3 aft. The stations are ESTIMATES. The E190-E2 APM
/// Figure 2.4 draws its holds as 6.36 m (5.03-11.39 m) and 7.89 m
/// (19.70-27.59 m) long (vector drawing, scale from the printed 22.55 m
/// between the outer hold ends); the forward hold ends at the wing front
/// spar and so moves aft with the 2.332 m forward plug, and the aft hold,
/// bounded by the aft pressure bulkhead, moves with the whole 5.366 m
/// stretch. The same datum method reproduces the E190 -> E195 forward hold
/// growth of the first-generation TCDS (datum 14,443 -> 15,256 mm = 0.81 m,
/// hold 12.5 -> 13.8 m3).
pub const E195_E2_HOLD_COMPARTMENTS: [(&str, f64, f64, f64); 2] = [
    ("Forward hold", 5.03, 11.39 + FORWARD_PLUG_M, 14.77),
    (
        "Aft hold",
        19.70 + FORWARD_PLUG_M,
        27.59 + (LENGTH_M - 36.237),
        15.20,
    ),
];

/// The E195-E2 lower holds as cabin-config compartments.
pub fn e195_e2_hold_compartments() -> Vec<crate::HoldCompartmentConfig> {
    E195_E2_HOLD_COMPARTMENTS
        .iter()
        .map(
            |&(name, x_start_m, x_end_m, volume_m3)| crate::HoldCompartmentConfig {
                name: name.to_owned(),
                x_start_m,
                x_end_m,
                volume_m3: Some(volume_m3),
                max_net_kg: None,
                deck: crate::HoldDeck::Lower,
            },
        )
        .collect()
}

/// Embraer's single-class E195-E2 with the 31 in pitch it advertises.
pub const E195_E2_PLANNING_CABIN: SourcedPlanningCabin = SourcedPlanningCabin {
    classes: &[SourcedSeatClass {
        class: "Economy",
        seats: 132,
        pitch_m: Some(31.0 * 0.0254),
        // Two-two seating: the E-Jet cabin is four abreast.
        abreast: Some(4),
        width_m: None,
    }],
    source: "Embraer E195-E2 specification sheet, April 2025, page 1 (single class configuration 132 seats @ 31 in; the same sheet prints 146 seats @ 28 in and 120 seats three-class 12 @ 36 in / 24 @ 34 in / 84 @ 31 in)",
};

/// The E195-E2 at the 62,500 kg weight variant, with PW1923G engines.
pub fn e195_e2() -> AircraftPreset {
    AircraftPreset {
        name: "E195-E2",
        display_name: "Embraer E195-E2",
        description: "ERJ 190-400 (marketed E195-E2) at 62,500 kg MTOW with PW1923G engines.",
        identity: AircraftVariantIdentity {
            model: "ERJ 190-400 (E195-E2)",
            weight_variant: "62,500 kg MTOW (SB 190E2-00-0058)",
            engine_model: "PW1923G",
            modification_state: "EASA.IM.A.071 Issue 28 planning configuration",
            tank_configuration: "standard wing integral tanks, no centre tank",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Vol. I Table 1-1 applied to the TCDS span of
            // 35.124 m: 24 m <= b < 36 m is code C.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::C),
            // EASA.IM.A.071 Issue 28 Section 5 III.13: taxi and ramp 62,700 kg
            // (138,229 lb) with SB 190E2-00-0058, take-off 62,500 kg, landing
            // 54,000 kg (119,049 lb), zero fuel 51,850 kg (114,309 lb).
            mrw_kg: Some(62_700.0),
            mtow_kg: Some(62_500.0),
            mlw_kg: Some(54_000.0),
            mzfw_kg: Some(51_850.0),
            oew_kg: crate::oew_reference::preset_reference_oew_kg("E195-E2"),
            // Embraer E195-E2 specification sheet: maximum usable fuel
            // 13,690 kg at the 0.803 kg/l it prints; 13,690 / 0.803 = 17,048.6 l.
            usable_fuel_volume_l: Some(17_048.57),
            usable_fuel_mass_kg: Some(13_690.0),
            fuel_density_kg_l: Some(0.803),
            // Embraer specification sheet single-class 132 seats at 31 in.
            planning_seats: Some(132),
            // The 3,000 nm of the specification sheet is flown with a full
            // passenger load of an unstated cabin, so the planning cabin is
            // the design payload (`payload_kg: None`), not a chart corner.
            design_point: Some(crate::PayloadRangeDesignPoint {
                range_nmi: 3_000.0,
                payload_kg: None,
                source: "Embraer E195-E2 specification sheet, April 2025, page 1 (Range Full PAX, LRC, typical reserves, 100 nm alternate: 3,000 nm / 5,556 km); the sheet states neither the cabin layout nor the takeoff weight, so this is an advertised-range proxy, not a payload/range chart point",
            }),
            certified_max_seats: Some(146),
            certified_exit_layout: Some(E195_E2_CERTIFIED_EXIT_LAYOUT),
            planning_cabin: Some(E195_E2_PLANNING_CABIN),
            partial_design_mission_evidence: vec![PartialDesignMissionEvidence {
                kind: PartialMissionEvidenceKind::AdvertisedRange,
                range: Some(PublishedRange::NauticalMiles(3_000.0)),
                payload_kg: None,
                load_case: None,
                profile_assumptions: Some("long-range cruise (LRC), ISA, sea-level start"),
                reserve_assumptions: Some("typical reserves, 100 nm alternate"),
                reserve_contract: None,
                applicability: "Embraer advertises 3,000 nm with full passengers; the cabin layout, payload and takeoff weight behind that figure are not stated",
                configuration_applicability: MissionEvidenceApplicability::ModelAndEngineFamily,
                missing: vec![
                    MissingDesignMissionDatum::Payload,
                    MissingDesignMissionDatum::Profile,
                    MissingDesignMissionDatum::ReserveFuel,
                ],
                source: "Embraer E195-E2 specification sheet, April 2025, page 1",
            }],
            // TCDS Section 5 III.14: "Centre of Gravity: See Airplane Flight
            // Manual"; no planning envelope is public for the E195-E2.
            cg_evidence: CgEnvelopeEvidence::AfmRequired,
            reference_wing_area_m2: Some(103.0),
            sources: vec![
                "EASA.IM.A.071 Embraer ERJ-190 TCDS Issue 28, 2026-09-03, Section 5 (ERJ 190-400) III.4 length 41.603 m, span 35.124 m, height 10.71 m, wing area 103 m2; III.11 maximum operating altitude 12,497 m (41,000 ft); III.13 weights; III.15 datum 15,903 mm ahead of the wing stub front spar; III.16 MAC 3.665 m; III.19-22 seating, exits, holds and tyres (nose 27x8.5R12, main H42x16.0R20)",
                "EASA.IM.A.071 Issue 28, Section 4 (ERJ 190-300) III.15 datum 13,571 mm ahead of the wing stub front spar and III.4 length 36.237 m, span 33.72 m",
                "Embraer E195-E2 specification sheet, April 2025 (https://www.embraer.com/media/ue1bdfnq/e195-e2-spec-1.pdf), page 1: MTOW 62,500 kg, MLW 54,000 kg, maximum payload 16,150 kg, usable fuel 13,690 kg at 0.803 kg/l, maximum cruise M 0.82, service ceiling 41,000 ft, 3,000 nm range",
                "Embraer E-Jets E2 Airport Planning Manual APM 5824 Rev 8, 2018-05-11 (effectivity E190-E2 only): Figure 2.1 (36.33 m length, nose gear 4.55 m aft of the nose, 6.73 m track, plan view read for the planform, engine station and tailplane), Figure 7.1 (12.61 m wheelbase), section 2.2.4-2.2.5 (tailplane 23.25 m2, 9.84 m span; fin 16.20 m2, 5.27 m span); the E190-E2 values are carried to the E195-E2 with the 2.332 m forward plug",
                "ICAO Aircraft Engine Emissions Databank v32 (EASA, 2026) UID 04P20PW204 and EASA.IM.E.090 Pratt & Whitney PW1500G series TCDS Issue 11, 2026-09-21 (PW1923G rating 10,593 daN)",
            ],
            ..AircraftReferenceData::default()
        },
        engine_name: "PW1900G",
        n_engines: 2,
        landing_gear: LandingGearConfig {
            n_nlg_wheels: 2,
            n_mlg_struts: 2,
            wheels_per_mlg_strut: 2,
            // 6.73 m track (APM Figure 2.1 and 7.1, E190-E2; the gear is the
            // same E2 gear) over the 3.02 m body width.
            track_diameter_factor: 6.73 / 3.02,
            // Wheelbase and stations are the E190-E2 APM values moved by the
            // 2.332 m forward plug: the nose gear (4.55 m from the nose tip)
            // stays ahead of the plug, the main gear moves aft with the wing.
            // ESTIMATE: no E195-E2 airport planning manual was found, and the
            // datum-based plug assumes the datum is fixed to the nose.
            reference_wheelbase_m: Some(E190_E2_WHEELBASE_M + FORWARD_PLUG_M),
            reference_track_m: Some(6.73),
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(LENGTH_M),
            reference_nlg_x_fraction: Some(NOSE_GEAR_X_M / LENGTH_M),
            reference_mlg_x_fractions: Some(vec![
                (NOSE_GEAR_X_M + E190_E2_WHEELBASE_M + FORWARD_PLUG_M) / LENGTH_M,
                (NOSE_GEAR_X_M + E190_E2_WHEELBASE_M + FORWARD_PLUG_M) / LENGTH_M,
            ]),
            mlg_strut_bogie_wheels: Some(vec![2, 2]),
            // No published tail-strike attitude exists for the E195-E2 (the
            // E190-E2 APM prints 12.0-12.4 deg tail-skid clearance for the
            // shorter body), so its model tail-down angle is unvalidated
            // (the generic tailcone loft understates it): keep the
            // Raymer/Roskam 15 deg floor on top of the tail-down criterion.
            min_tip_back_deg: 15.0,
            takeoff_stabilizer_nose_up_deg: Some(crate::landing_gear::TRANSPORT_THS_TAKEOFF_NOSE_UP_DEG),
            ..LandingGearConfig::default()
        },
        // Planform fitted to the TCDS reference area 103 m^2 and MAC 3.665 m
        // at the TCDS 35.124 m span, on the 28.9 deg straight leading edge
        // read from the vector plan view of the E190-E2 APM Figure 2.1 (a
        // leading edge that runs straight from the side of the body to the
        // tip, an unswept inboard trailing edge kinking at 7.3 m from the
        // centreline and a 1.0 m raked tip chord): centreline chord 6.00 m,
        // kink chord 2.87 m (0.478 of the root, drawn 0.478), tip chord
        // 0.85 m, kink at 0.416 of the semi-span (7.3 m). The drawn
        // E190-E2 chords are about 10 % larger than those that close the
        // published 103 m^2, which the TCDS reference area, not the outline,
        // governs. Outboard quarter-chord sweep 26.7 deg (derived). ESTIMATE:
        // the E195-E2's 1.4 m longer raked tip is not drawn in any document
        // found, so the outer panel shares the E190-E2 sweep and kink.
        design_vector: DesignVector {
            span_m: 35.124,
            root_chord_m: 6.003,
            break_chord_m: 2.869,
            tip_chord_m: 0.854,
            sweep_deg: 28.9,
            tip_twist_deg: -1.5,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: 41.60,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        airfoil_class_source: "Design-era declaration: the E2 wing (certified 2018) is of the supercritical-section generation; manufacturer section data are not public. Drawn with NASA SC2-0714 root and SC(2)-0410 tip sections (Harris, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                // Root-chord leading edge 11.55 m aft of the nose on the
                // E190-E2 plan view (APM Figure 2.1: 28.9 deg leading edge
                // through 12.38 m at the side of the body) plus the 2.332 m
                // forward plug: 13.882 m. The model leading edge of the MAC
                // then sits 17.36 m aft of the nose (quarter MAC 18.27 m).
                // ESTIMATE: derived from the E190-E2 drawing and the datum
                // difference; the E195-E2 drawing was not found.
                root_datum_x_m: 13.882,
                // Low wing. ESTIMATE of the root and kink heights from the
                // A320 convention for a 3.35 m body; the tip lower surface
                // stands 3.81 m above the ground (E190-E2 APM Table 2.2,
                // (G), 56,600 kg) against a body axis read at 3.75 m.
                root_z_m: -1.2,
                break_z_m: -0.65,
                tip_z_m: 0.35,
                root_twist_deg: 3.0,
                break_twist_deg: 1.0,
                break_span_fraction: 0.416,
                kink_span_fraction: Some(0.416),
                // Half the 3.02 m body width over the 17.562 m semi-span.
                side_of_body_span_fraction: Some(0.086),
                outboard_sweep_decrement_deg: 1.5,
                root_airfoil: "SC2-0714".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                airfoil_class: crate::AirfoilClass::Supercritical,
                ..WingConfig::default()
            },
            // The E190-E2 tail carried to the E195-E2 unchanged (ESTIMATE:
            // no E195-E2 drawing found). Tailplane 23.25 m^2 on 9.84 m span
            // (APM 2.2.4), 34.5 deg leading edge, 1.07 m tip chord and the
            // tip leading edge 3.38 m aft of the root's, read off the vector
            // plan view of Figure 2.1; root leading edge 5.30 m forward of the
            // tail tip. Fin 16.20 m^2 on 5.27 m height (APM 2.2.5), root
            // chord 4.76 m and tip chord 1.36 m at a 40.5 deg leading edge
            // (tip leading edge 4.50 m aft of the root's), root leading edge
            // 6.56 m forward of the tail tip, read off the side view.
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 5.30,
                hstab_z_m: 1.15,
                hstab_root_chord_m: 3.65,
                hstab_tip_chord_m: 1.07,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (3.38, 4.92, 0.5),
                vstab_offset_from_tail_m: 6.56,
                vstab_z_m: 1.70,
                vstab_root_chord_m: 4.76,
                vstab_tip_chord_m: 1.36,
                vstab_tip_le_m: (4.50, 0.0, 5.27),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                // Width 3.02 m from the half-width of the E190-E2 APM plan
                // view (Figure 2.1, 1.51 m); height 3.35 m is the published
                // E-Jet envelope (ESTIMATE, the APM side view reads 3.4 m).
                diameter_m: 3.02,
                height_m: Some(3.35),
                nose_z_m: -0.3,
                cabin_start_x_m: 3.4,
                cabin_z_m: 0.1,
                // ESTIMATE: 19 % of the length, the A220 and A320 ratio.
                tailcone_length_m: 7.9,
                tail_z_m: 1.4,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // Nacelle centreline +-4.84 m (E190-E2 APM Figure 2.1 plan
                // view: nacelle outline 3.62-6.07 m from the centreline).
                spanwise_positions_m: vec![4.84, -4.84],
                // Nacelle low point 0.38 m above the ground (E190-E2 APM
                // Table 2.2, (F)) with the 1.08 m catalogue radius, against a
                // body axis read at 3.75 m and the local wing leading edge.
                z_m: -1.45,
                // Puts the nacelle front 12.88 m aft of the nose: the 13.882 m
                // root datum plus the 2.668 m the 28.9 deg leading edge runs
                // aft by the engine station, less this offset (the E190-E2
                // inlet lip at 10.55 m plus the 2.332 m forward plug).
                inlet_x_offset_m: 3.668,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            // ESTIMATE: Embraer's brochure long-range cruise of M 0.78 at
            // 35,000 ft; the 2025 sheet prints only the M 0.82 maximum.
            cruise_mach: 0.78,
            cruise_altitude_m: 10_668.0,
            mtow_kg: 62_500.0,
            max_wing_area_m2: 110.0,
            min_wing_loading_kg_m2: 500.0,
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: true,
            num_passengers: 132,
            cargo_payload_kg: 13_000.0,
            // Embraer specification sheet maximum payload (also MZFW 51,850 kg
            // less the 35,700 kg basic operating weight it implies).
            max_structural_payload_kg: 16_150.0,
            // ESTIMATE: Vmo/Mmo are in the AFM; 0.82 Mmo at 35,000 ft is
            // about 240 m/s TAS, 175 m/s is the A220 preset's dive-speed class.
            dive_speed_m_s: 175.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: crate::presets::high_lift("modern_narrowbody"),
    }
}
