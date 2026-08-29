// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/presets.py (the `AVE` entry)
// Reference: alas @ rust-port-baseline.

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
        description: "Long-range widebody twin reference aircraft based on 777X-class geometry.",
        identity: AircraftVariantIdentity {
            model: "AVE-v1",
            weight_variant: "notional design requirement",
            engine_model: "GE9X family conceptual installation",
            modification_state: "AVE-v1 design baseline",
            tank_configuration: "conceptual integral wing tanks",
        },
        reference: AircraftReferenceData {
            cg_evidence: CgEnvelopeEvidence::DesignRequirement,
            sources: vec!["docs/PRESET_PHYSICAL_AUDIT.md#variant-identity"],
            ..AircraftReferenceData::default()
        },
        engine_name: "GE9X",
        n_engines: 2,
        landing_gear: LandingGearConfig::default(),
        design_vector: DesignVector {
            span_m: 71.75,
            root_chord_m: 16.50,
            break_chord_m: 7.80,
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
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 10.7,
                hstab_z_m: 1.2,
                hstab_root_chord_m: 8.0,
                hstab_tip_chord_m: 2.2,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (7.5, 11.0, 1.0),
                vstab_offset_from_tail_m: 12.2,
                vstab_z_m: 2.0,
                vstab_root_chord_m: 9.5,
                vstab_tip_chord_m: 3.2,
                vstab_tip_le_m: (9.0, 0.0, 9.8),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 6.2,
                nose_z_m: -0.5,
                cabin_start_x_m: 6.0,
                cabin_z_m: 0.2,
                tailcone_length_m: 14.0,
                tail_z_m: 1.8,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                spanwise_positions_m: vec![9.8, -9.8],
                // Measured down from the local wing surface, and calibrated to
                // leave about 0.35 m of clearance under the lower surface.
                z_m: -3.13,
                inlet_x_offset_m: 4.2,
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
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: super::high_lift("advanced_highlift_widebody"),
    }
}
