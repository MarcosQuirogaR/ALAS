// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/presets.py (the `AVE` entry)

//! AVE: the notional long-range twin this program was built around.
//!
//! The one entry in the registry that is not a real aeroplane. Its planform
//! is 777X-class and its numbers were chosen rather than read off a
//! specification sheet, which makes it the reference the rest of the program's
//! defaults are calibrated against: every unconfigured value elsewhere in this
//! crate produces something close to this aircraft.
//!
//! It lives apart from the published types because the provenance of its
//! numbers is different, and that difference matters when one of them looks
//! wrong: a disagreement against a real A320 is a bug, and a disagreement
//! against AVE is a decision somebody made.

use crate::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeEvidence,
    DesignRequirements, DesignVector, EmpennageConfig, EngineConfig, FuselageConfig,
    GeometryConfig, LandingGearConfig, WingConfig,
};

/// The reference twin.
pub fn ave() -> AircraftPreset {
    AircraftPreset {
        name: "AVE",
        display_name: "AVE (Reference Twin)",
        class: crate::presets::AircraftClass::Transport,
        description: "Long-range widebody twin reference aircraft based on 777X-class geometry.",
        identity: AircraftVariantIdentity {
            model: "AVE-v1",
            weight_variant: "notional design requirement",
            engine_model: "GE9X family conceptual installation",
            modification_state: "AVE-v1 design baseline",
            tank_configuration: "conceptual integral wing tanks",
        },
        reference: AircraftReferenceData {
            // ICAO Annex 14 Vol. I Table 1-1 (aerodrome reference code) applied to the
            // notional 777X-class wingspan of 71.75 m with the wingtips extended:
            // 65 m <= b < 80 m is code F. A folded-tip gate span is not modeled.
            aerodrome_reference_code: Some(crate::AerodromeReferenceCode::F),
            // Declared notional-cabin requirement (DesignRequirements default); not a published aircraft datum.
            planning_seats: Some(350),
            // AVE's body is the 777-9's (76.72 m against 76.73 m), so the
            // 777-9's doors and standard cabin stand in for its own: the
            // cabin is bounded by real door stations and seeded with a
            // published class mix. The 350-seat planning requirement above
            // is AVE's own and is left as declared.
            certified_exit_layout: Some(crate::presets::B777_9_EXIT_LAYOUT),
            planning_cabin: Some(crate::presets::B777_9_PLANNING_CABIN),
            // Notional long-range widebody class default: the declared AVE
            // design range, flown at the planning cabin. Not a chart read.
            design_point: Some(crate::PayloadRangeDesignPoint {
                long_range_cruise: None,
                cruise: None,
                range_nmi: 7_600.0,
                payload_kg: None,
                source: "AVE-v1 design requirements (notional); class default, no published payload/range chart",
                reserve: None,
            }),
            // The AVE is notional: its design requirement (the `requirements` block of
            // this preset and the default fuel policy) is the registered source of each datum.
            mission_provenance: crate::DesignMissionProvenanceSet {
                range: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::DesignRequirement,
                    stated_conditions: Some("7,600 nmi still-air design range flown with the planning cabin as payload"),
                    relative_uncertainty: None,
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "AVE-v1 design requirements (notional), reference.design_point",
                }),
                payload: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::DesignRequirement,
                    stated_conditions: Some("65,000 kg maximum structural payload requirement, notional, at the maximum payload of a 777-300ER; the cabin is the 777-9 standard two-class one"),
                    relative_uncertainty: None,
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "AVE-v1 design requirements (notional), requirements.max_structural_payload_kg",
                }),
                profile: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::DesignRequirement,
                    stated_conditions: Some("cruise Mach 0.84 at 11,887.2 m (FL390); the model's climb and descent schedules apply"),
                    relative_uncertainty: None,
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "AVE-v1 design requirements (notional), requirements.cruise_mach and cruise_altitude_m",
                }),
                reserve_fuel: Some(crate::MissionDatumProvenance {
                    tier: crate::MissionEvidenceTier::DesignRequirement,
                    stated_conditions: Some("EASA basic reserve policy of the default fuel policy: 5 % contingency (5 min minimum), 200 nmi alternate, 30 min final reserve at 1,500 ft; the reserve mass follows from the modelled trip"),
                    relative_uncertainty: None,
                    variant_match: crate::MissionEvidenceApplicability::ExactPreset,
                    source: "AVE-v1 design requirements (notional), default FuelPolicyConfig",
                }),
            },
            cg_evidence: CgEnvelopeEvidence::DesignRequirement,
            sources: vec!["notional design requirement; no published aircraft data"],
            operating_speed_limits: Some(super::operating_limits::AVE_DESIGN_RULE),
            ..AircraftReferenceData::default()
        },
        engine_name: "GE9X",
        n_engines: 2,
        // AVE is a notional twin with no real tyre: the generic tyre classes are its tyre.
        landing_gear: LandingGearConfig {
            // The AVE is a notional twin inspired by the Boeing 777-9: same 6.20 m fuselage width and GE9X engine.
            // Belly clearance 2.44 m from the 777-9 airport-planning ground clearances (D6-86073 Rev G, Sep 2025,
            // Table 2-2 and Figure 2-2, read on the page). Each row is a min-max pair over the loading attitudes, so
            // each row gives an interval for the belly clearance (model height of the row above the belly line,
            // level): C fuselage bottom 2.13-2.59 m (the direct row: Figure 2-2 ends its arrow at the belly), A fuselage top
            // 2.18-2.51 m (body 6.281 m high), F engine low point 2.43-2.45 m, K wing tip 2.41-2.67 m; these four intersect in
            // 2.43-2.45 m, 2.44 m is the middle. P horizontal-tail tip (2.87-3.32
            // m) and Q fin top (2.51-2.97 m) lie above it: the model's tail surfaces are lower than the 777-9's
            // (a geometry card, not absorbed in the plane). The table prints no attitude; the 777-9 is within 0.5 deg
            // of level, 0.1-0.2 m at the nacelle station. No scaling: width and engine are the 777-9's.
            fuselage_ground_clearance_m: Some(2.44),
            // No published tail-strike attitude backs this preset's aft-fuselage
            // geometry, so its model tail-down angle is unvalidated (the generic
            // tailcone loft understates it): keep the Raymer/Roskam 15 deg floor on
            // top of the tail-down criterion.
            min_tip_back_deg: 15.0,
            // Tail-scrape requirement: no published tail-strike attitude found (a reference design: no aircraft
            // publishes one), so the 10 deg default (an assumption) stands.
            trimmable_stabilizer: true,
            ..LandingGearConfig::default()
        },
        design_vector: DesignVector {
            span_m: 71.75,
            // With the 34 deg leading-edge sweep and the kink at 35 % semi-span,
            // the root-to-kink trailing edge runs aft (at most 90 deg to the
            // aft fuselage axis, the `root_to_kink_te_angle` limit) only when
            // the kink trailing edge is at or behind the root trailing edge:
            // The 16.0 m root and 8.0 m break chords give a root-to-kink
            // trailing-edge angle of 87.9 deg, inside the 90 deg limit.
            // Engineering choice for a notional aircraft; the chords are
            // set together with the wing area, span and tip chord.
            root_chord_m: 16.00,
            break_chord_m: 8.00,
            tip_chord_m: 1.60,
            sweep_deg: 34.0,
            tip_twist_deg: 0.0,
            // The range [-1.70, 2.64] m was established with the frozen
            // ReferenceCompatibility mass-coordinate path: outside it the
            // operating-empty, zero-fuel and takeoff centres of gravity stop
            // satisfying the nose-gear steering load limit for the auto-sized
            // 525-seat, 65 t cabin. 0.5 m is its midpoint. The product path
            // integrates a structural wingbox centroid instead; that model is
            // deliberately allowed to report a forward-CG finding until the
            // notional AVE requirement is re-sized from independent evidence.
            wing_x_shift_m: 0.5,
            tail_scale: 1.0,
            fuselage_length_m: 76.72,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        speed_schedule_source: crate::presets::SpeedScheduleSource::SourcedLadder,
        airfoil_class_source: "Clean-sheet default for a notional 777X-class transonic twin: current transonic transport wings carry supercritical sections. Drawn with NASA SC(2) sections, root SC2-0714 and tip SC(2)-0410 (C. D. Harris, NASA Supercritical Airfoils, NASA TP-2969, 1990).",
        geometry: GeometryConfig {
            wing: WingConfig {
                root_datum_x_m: 25.14,
                root_z_m: -2.1,
                break_z_m: -0.3,
                tip_z_m: 2.5,
                root_twist_deg: 4.0,
                break_twist_deg: 2.0,
                break_span_fraction: 0.35,
                kink_span_fraction: Some(0.35),
                outboard_sweep_decrement_deg: 2.0,
                root_airfoil: "SC2-0714".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                airfoil_class: crate::AirfoilClass::SupercriticalAdvanced,
                wing_profile_interpolation: crate::WingProfileInterpolation::C2Spline,
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                // drawn centreline leading-edge apex 64.87 m, 1.13 m aft
                // of the model, and leading-edge sweep 39.2 deg against 34.3 deg; the semispan (11.0 m), chords and
                // dihedral (grade C: tip rounding of the low-resolution mask) stay. Was offset 10.7 m, tip x 7.5 m.
                // Tail fit: tailplane straight-edge fit to the drawing, centreline values.
                hstab_offset_from_tail_m: 11.76,
                hstab_z_m: 1.2,
                hstab_root_chord_m: 6.98,
                hstab_tip_chord_m: 1.66,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (9.94, 12.19, 1.0), // tan(39.2 deg) x 11.0 m; was (7.5, 11.0, 1.0)
                // root line at the crown (3.3 m) with the
                // drawn crown leading edge 62.59 m (76.72 - 14.13); was root z 2.0 m and offset 12.2 m, which put the crown
                // leading edge at 65.68 m. Root chord 9.5 m stays (drawn crown chord 9.35 m, C). The fin was
                // set to the ACAP overall height 19.53 m above the ground plane of the time (-5.13 m), a fin top 14.40 m high and
                // 11.1 m above the crown (side-view reading, front view 10.1 m, C). Sweep 44.3 deg as drawn (dx 10.81 m). Was tip (9.0, 0.0, 9.8).
                // Tail fit: fin straight-edge fit to the drawing at the root line (rms 0.008/0.011 m).
                // The fit put the fin top at 3.3 + 10.51 = 13.81 m: on the 2.44 m belly clearance
                // the model stands 19.21 m high against the 777-9 overall height 19.53 m (D6-86073 Rev G Fig 2-1; Table 2-2 row Q 19.28-19.74 m),
                // and the horizontal-tail tip sits 0.42-0.88 m below its row P: tail geometry is a card, not absorbed in the ground plane.
                vstab_offset_from_tail_m: 13.63,
                vstab_z_m: 3.3,
                vstab_root_chord_m: 8.96,
                vstab_tip_chord_m: 2.65,
                vstab_tip_le_m: (10.3, 0.0, 10.51),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 6.2,
                nose_z_m: 0.25,
                cabin_start_x_m: 6.0,
                cabin_z_m: 0.2,
                tailcone_length_m: 14.0,
                tail_z_m: 1.8,
                // FUSELAGE-G2 shaped nose and aft body. Outline traced from the manufacturer drawing
                // (side: same; top: Boeing 777-9 D6-86073 Rev G),
                // calibrated to the drawn length; the nose law (windshield, radome, keel and plan
                // exponents) is a least-squares fit to the traced upper, lower and plan lines (RMS 0.087 m),
                // the aft law (belly ramp, crown drop, plan taper, closing section) to the traced aft lines
                // (RMS 0.109 m). Cabin start, tailcone length and every payload and mass station are unchanged:
                // only the outer loft moves. Section exponent 2.0 (ellipse): no drawing measures it.
                nose_length_m: Some(10.62),
                nose_windshield_angle_deg: Some(20.0),
                nose_crown_end_fraction: Some(1.0),
                nose_radome_length_fraction: Some(0.1),
                nose_keel_exponent: Some(3.01),
                nose_plan_exponent: Some(1.39),
                nose_section_exponent: Some(2.0),
                belly_upsweep_length_m: Some(20.59),
                aft_belly_exponent: Some(1.24),
                tail_end_z_m: Some(1.98),
                tail_end_width_m: Some(0.1),
                tail_end_height_m: Some(0.96),
                aft_plan_length_m: Some(17.66),
                aft_plan_exponent: Some(1.02),
                aft_crown_length_m: Some(11.64),
                aft_crown_exponent: Some(1.0),
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                // The engines follow the 777-9 drawing (ACAP Fig 2-1: centreline 10.64 m, mask
                // 10.81 m, mean 10.7 m; inlet 25.96 m from the nose; nacelle height as drawn, 1.62 m above the old
                // position). The 0.35 m clearance target is no longer a design rule. Was y 9.8 m, z -3.13 m, offset 4.2 m.
                spanwise_positions_m: vec![10.7, -10.7],
                // Measured down from the local wing surface: +1.479 m puts the nacelle centre at z -2.205 m (was -3.825 m at
                // y 9.8 m), 1.62 m higher as drawn, allowing for the wing heights at the new station and the C2 profile.
                // AVE side sheet best rigid shift dz -0.25 m (applied); dx +0.15 m side but 0.00 m top: offset kept. was z -1.651
                z_m: -1.901,
                inlet_x_offset_m: 6.9,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.84,
            cruise_altitude_m: 11887.2,
            mtow_kg: 358_670.0,
            max_wing_area_m2: 535.0,
            min_wing_loading_kg_m2: 485.0,
            // Notional, at the maximum payload of a 777-300ER.
            max_structural_payload_kg: 65_000.0,
            // The cabin is the 777-9 standard two-class one the reference
            // data declares, not a named airline profile.
            cabin_preset: "Custom".to_owned(),
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: super::high_lift("advanced_highlift_widebody"),
    }
}
