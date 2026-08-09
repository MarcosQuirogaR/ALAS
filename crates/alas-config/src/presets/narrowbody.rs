// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/presets.py (the single-aisle entries)
// Reference: alas @ rust-port-baseline.

//! Two published single-aisle types, which is where the global assumptions
//! stop fitting.
//!
//! The rest of this crate is calibrated around a modern twin-aisle transport,
//! and neither of these is one. Two corrections follow from that, and both are
//! stated on the presets rather than left to whoever runs them.
//!
//! Mass first. Structural, systems and furnishings mass does not scale
//! linearly with takeoff weight, so the widebody-calibrated Torenbeek
//! fractions under-predict a small aircraft's operating empty weight -- by
//! about 2.8 t on the A220-300, which is most of a revenue payload's worth of
//! error in the wrong direction. Its entry carries fractions of its own.
//!
//! Speeds second. Both types have full-span slats and slotted Fowler flaps,
//! which is a materially better high-lift system than the generic
//! "standard narrow-body" bucket describes; scored with the generic one their
//! rotation and takeoff-safety speeds come out fifteen to twenty knots high,
//! which sizes them out of runways they operate from every day.

use crate::{
    AircraftPreset, DesignRequirements, DesignVector, EmpennageConfig, EngineConfig,
    FuselageConfig, GeometryConfig, MassModelConfig, WingConfig,
};

/// Short and medium-range twin, the reference single-aisle.
pub fn a320_200() -> AircraftPreset {
    AircraftPreset {
        name: "A320-200",
        display_name: "Airbus A320-200",
        description: "Short/medium-range narrow-body twin with LEAP-1A engines.",
        engine_name: "LEAP-1A",
        n_engines: 2,
        design_vector: DesignVector {
            span_m: 35.80,
            root_chord_m: 6.10,
            break_chord_m: 3.80,
            tip_chord_m: 1.20,
            sweep_deg: 25.0,
            tip_twist_deg: -1.5,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: 37.57,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        geometry: GeometryConfig {
            wing: WingConfig {
                root_datum_x_m: 12.90,
                root_z_m: -1.2,
                break_z_m: -0.2,
                tip_z_m: 1.5,
                root_twist_deg: 3.0,
                break_twist_deg: 1.0,
                break_span_fraction: 0.37,
                outboard_sweep_decrement_deg: 1.5,
                root_airfoil: "sc20610".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 5.5,
                hstab_z_m: 0.8,
                hstab_root_chord_m: 4.0,
                hstab_tip_chord_m: 1.2,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (3.5, 6.0, 0.5),
                vstab_offset_from_tail_m: 6.5,
                vstab_z_m: 1.2,
                vstab_root_chord_m: 5.2,
                vstab_tip_chord_m: 1.8,
                vstab_tip_le_m: (5.0, 0.0, 5.8),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 3.95,
                nose_z_m: -0.3,
                cabin_start_x_m: 3.5,
                cabin_z_m: 0.1,
                tailcone_length_m: 7.5,
                tail_z_m: 1.0,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                spanwise_positions_m: vec![5.5, -5.5],
                // About 0.35 m of clearance under the wing lower surface,
                // which is what a large-fan engine on a low-slung single-aisle
                // has to live with.
                z_m: -1.71,
                inlet_x_offset_m: 2.5,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.78,
            cruise_altitude_m: 11278.0,
            mtow_kg: 78_000.0,
            max_wing_area_m2: 130.0,
            min_wing_loading_kg_m2: 500.0,
            num_passengers: 150,
            cargo_payload_kg: 18_000.0,
            // Maximum zero-fuel weight 62.5 t less an operating empty weight
            // of about 42.6 t.
            max_structural_payload_kg: 19_900.0,
            dive_speed_m_s: 180.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: super::high_lift("modern_narrowbody"),
    }
}

/// The smallest type in the registry, and the only one with its own mass model.
pub fn a220_300() -> AircraftPreset {
    AircraftPreset {
        name: "A220-300",
        display_name: "Airbus A220-300",
        description: "Short/medium-range narrow-body twin with PW1500G geared turbofans.",
        engine_name: "PW1500G",
        n_engines: 2,
        design_vector: DesignVector {
            span_m: 35.10,
            root_chord_m: 5.80,
            break_chord_m: 3.50,
            tip_chord_m: 1.10,
            sweep_deg: 25.0,
            tip_twist_deg: -1.5,
            wing_x_shift_m: 0.0,
            tail_scale: 1.0,
            fuselage_length_m: 38.70,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        geometry: GeometryConfig {
            wing: WingConfig {
                root_datum_x_m: 13.30,
                root_z_m: -1.0,
                break_z_m: -0.2,
                tip_z_m: 1.5,
                root_twist_deg: 3.0,
                break_twist_deg: 1.0,
                break_span_fraction: 0.37,
                outboard_sweep_decrement_deg: 1.5,
                root_airfoil: "SC2-0714".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 5.0,
                hstab_z_m: 0.7,
                hstab_root_chord_m: 3.8,
                hstab_tip_chord_m: 1.1,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (3.2, 5.5, 0.5),
                vstab_offset_from_tail_m: 6.0,
                vstab_z_m: 1.0,
                vstab_root_chord_m: 5.0,
                vstab_tip_chord_m: 1.6,
                vstab_tip_le_m: (4.5, 0.0, 5.5),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 3.50,
                nose_z_m: -0.2,
                cabin_start_x_m: 3.2,
                cabin_z_m: 0.1,
                tailcone_length_m: 7.0,
                tail_z_m: 0.8,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                spanwise_positions_m: vec![5.2, -5.2],
                z_m: -1.71,
                inlet_x_offset_m: 2.3,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.78,
            cruise_altitude_m: 11278.0,
            mtow_kg: 70_900.0,
            max_wing_area_m2: 120.0,
            min_wing_loading_kg_m2: 480.0,
            num_passengers: 130,
            cargo_payload_kg: 15_000.0,
            // Maximum zero-fuel weight 55.8 t less an operating empty weight
            // of 37.08 t.
            max_structural_payload_kg: 18_700.0,
            dive_speed_m_s: 175.0,
            ..DesignRequirements::default()
        },
        // The published operating empty weight is 37.08 t and the global
        // fractions predict about 34.3 t. Both raised fractions are what close
        // that gap; see the module documentation for why they are per-type.
        mass_model: Some(MassModelConfig {
            systems_mass_fraction: 0.13,
            furnishings_mass_fraction: 0.12,
            ..MassModelConfig::default()
        }),
        performance: super::high_lift("modern_narrowbody"),
    }
}
