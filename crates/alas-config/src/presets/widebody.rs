// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/presets.py (the twin-aisle entries)
// Reference: alas @ rust-port-baseline.

//! Four published twin-aisle types, from a 1970s trijet to a double-decker.
//!
//! The span of this group is the point of it. A design method calibrated on
//! one modern widebody will reproduce that widebody; whether it also
//! reproduces a DC-10 with an engine in its fin, and an A380 with two decks
//! and four engines, is what says whether the method generalizes. So the group
//! deliberately covers two, three and four engines, thirty years of structural
//! technology, and a factor of two in maximum takeoff weight.
//!
//! Every dimension here comes from the manufacturer's published specification
//! sheet, except the root and break chords, which those sheets do not give and
//! which are estimated from wing area, aspect ratio, taper and sweep.
//!
//! All four share the advanced high-lift assumptions: triple-slotted flaps
//! with leading-edge slats, which is what a twin-aisle transport has and what
//! makes its takeoff and landing speeds come out right.

use crate::{
    AircraftPreset, DesignRequirements, DesignVector, EmpennageConfig, EngineConfig,
    FuselageConfig, GeometryConfig, WingConfig,
};

/// How far below the wing reference plane the A380's engines hang, in metres.
///
/// Named rather than written in place because the lint below reads any 3.14 as
/// a mistyped pi. It is a ground-clearance calibration: set by the inboard
/// pair at about 0.35 m under the wing lower surface, which leaves the
/// outboard pair about 1.4 m.
// The value is a length in metres that happens to round to pi's first three
// digits; there is no circle anywhere near it.
#[allow(clippy::approx_constant)]
const A380_ENGINE_Z_M: f64 = -3.14;

/// Long-range quad with a conventional tail.
pub fn a340_300() -> AircraftPreset {
    AircraftPreset {
        name: "A340-300",
        display_name: "Airbus A340-300",
        description: "Long-range quad-engine widebody with CFM56-5C engines.",
        engine_name: "CFM56-5C",
        n_engines: 4,
        design_vector: DesignVector {
            span_m: 60.30,
            root_chord_m: 12.00,
            break_chord_m: 6.50,
            tip_chord_m: 1.80,
            sweep_deg: 30.0,
            tip_twist_deg: -2.0,
            wing_x_shift_m: -1.4,
            tail_scale: 1.0,
            fuselage_length_m: 63.69,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        geometry: GeometryConfig {
            wing: WingConfig {
                root_datum_x_m: 22.0,
                root_z_m: -1.8,
                break_z_m: -0.3,
                tip_z_m: 2.0,
                root_twist_deg: 3.5,
                break_twist_deg: 1.5,
                break_span_fraction: 0.35,
                outboard_sweep_decrement_deg: 2.0,
                root_airfoil: "sc20612".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 9.0,
                hstab_z_m: 1.0,
                hstab_root_chord_m: 6.5,
                hstab_tip_chord_m: 1.8,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (6.0, 9.0, 0.8),
                vstab_offset_from_tail_m: 10.5,
                vstab_z_m: 1.8,
                vstab_root_chord_m: 8.0,
                vstab_tip_chord_m: 2.8,
                vstab_tip_le_m: (7.5, 0.0, 8.5),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 5.64,
                nose_z_m: -0.4,
                cabin_start_x_m: 5.5,
                cabin_z_m: 0.2,
                tailcone_length_m: 12.0,
                tail_z_m: 1.5,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                spanwise_positions_m: vec![7.5, -7.5, 14.0, -14.0],
                // Calibrated for the inboard pair at 0.35 m of clearance. The
                // outboard pair sits under a thinner, higher part of the wing
                // and clears by about 1.4 m at the same offset.
                z_m: -1.94,
                inlet_x_offset_m: 3.0,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.82,
            cruise_altitude_m: 11887.2,
            mtow_kg: 275_000.0,
            max_wing_area_m2: 370.0,
            min_wing_loading_kg_m2: 500.0,
            num_passengers: 290,
            cargo_payload_kg: 45_000.0,
            // Maximum zero-fuel weight 178.0 t less an operating empty weight
            // of about 129.4 t.
            max_structural_payload_kg: 48_600.0,
            dive_speed_m_s: 200.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: super::high_lift("advanced_highlift_widebody"),
    }
}

/// Double-deck quad, the largest airliner in the registry.
pub fn a380_800() -> AircraftPreset {
    AircraftPreset {
        name: "A380-800",
        display_name: "Airbus A380-800",
        description: "Double-deck super-jumbo with Trent 900 engines.",
        engine_name: "Trent 900",
        n_engines: 4,
        design_vector: DesignVector {
            span_m: 79.75,
            root_chord_m: 23.00,
            break_chord_m: 11.30,
            tip_chord_m: 3.50,
            sweep_deg: 33.5,
            tip_twist_deg: -2.5,
            wing_x_shift_m: -7.5,
            tail_scale: 1.0,
            fuselage_length_m: 72.72,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        geometry: GeometryConfig {
            wing: WingConfig {
                root_datum_x_m: 26.70,
                root_z_m: -2.5,
                break_z_m: -0.4,
                tip_z_m: 3.0,
                root_twist_deg: 4.5,
                break_twist_deg: 2.0,
                break_span_fraction: 0.33,
                outboard_sweep_decrement_deg: 2.5,
                root_airfoil: "SC2-0714".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 11.0,
                hstab_z_m: 1.5,
                hstab_root_chord_m: 9.0,
                hstab_tip_chord_m: 2.5,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (8.5, 12.5, 1.2),
                vstab_offset_from_tail_m: 13.0,
                vstab_z_m: 2.5,
                vstab_root_chord_m: 11.0,
                vstab_tip_chord_m: 3.5,
                vstab_tip_le_m: (10.0, 0.0, 11.0),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 7.14,
                // The one non-circular body in the registry: two decks make it
                // taller than it is wide.
                height_m: Some(8.41),
                nose_z_m: -0.6,
                cabin_start_x_m: 7.0,
                cabin_z_m: 0.3,
                tailcone_length_m: 15.0,
                tail_z_m: 2.0,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                spanwise_positions_m: vec![10.0, -10.0, 18.5, -18.5],
                z_m: A380_ENGINE_Z_M,
                inlet_x_offset_m: 4.5,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.85,
            cruise_altitude_m: 11887.2,
            mtow_kg: 560_000.0,
            max_wing_area_m2: 855.0,
            min_wing_loading_kg_m2: 450.0,
            num_passengers: 525,
            cargo_payload_kg: 150_000.0,
            // Maximum zero-fuel weight about 361 t less an operating empty
            // weight of about 277 t.
            max_structural_payload_kg: 84_000.0,
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: super::high_lift("advanced_highlift_widebody"),
    }
}

/// Composite long-range twin, the type the global mass model is tuned on.
pub fn b787_9() -> AircraftPreset {
    AircraftPreset {
        name: "B787-9",
        display_name: "Boeing 787-9 Dreamliner",
        description: "Long-range composite widebody twin with GEnx-1B engines.",
        engine_name: "GEnx-1B",
        n_engines: 2,
        design_vector: DesignVector {
            span_m: 60.12,
            root_chord_m: 12.60,
            break_chord_m: 6.50,
            tip_chord_m: 1.60,
            sweep_deg: 32.2,
            tip_twist_deg: -2.0,
            wing_x_shift_m: -1.5,
            tail_scale: 1.0,
            fuselage_length_m: 62.81,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        geometry: GeometryConfig {
            wing: WingConfig {
                root_datum_x_m: 21.00,
                root_z_m: -1.8,
                break_z_m: -0.2,
                tip_z_m: 2.5,
                root_twist_deg: 3.5,
                break_twist_deg: 1.5,
                break_span_fraction: 0.35,
                outboard_sweep_decrement_deg: 2.0,
                root_airfoil: "sc20614".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 9.5,
                hstab_z_m: 1.0,
                hstab_root_chord_m: 6.5,
                hstab_tip_chord_m: 1.8,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (6.0, 9.5, 0.8),
                vstab_offset_from_tail_m: 10.5,
                vstab_z_m: 1.8,
                vstab_root_chord_m: 8.0,
                vstab_tip_chord_m: 2.8,
                vstab_tip_le_m: (7.5, 0.0, 8.5),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 5.94,
                nose_z_m: -0.4,
                cabin_start_x_m: 5.5,
                cabin_z_m: 0.2,
                tailcone_length_m: 12.0,
                tail_z_m: 1.5,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                spanwise_positions_m: vec![9.5, -9.5],
                z_m: -2.55,
                inlet_x_offset_m: 3.5,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.85,
            cruise_altitude_m: 11887.2,
            mtow_kg: 254_000.0,
            max_wing_area_m2: 385.0,
            min_wing_loading_kg_m2: 480.0,
            num_passengers: 290,
            cargo_payload_kg: 55_000.0,
            // Maximum zero-fuel weight 181.4 t less an operating empty weight
            // of 128.8 t.
            max_structural_payload_kg: 52_600.0,
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: super::high_lift("advanced_highlift_widebody"),
    }
}

/// Trijet with a centerline engine, the oldest type in the registry.
pub fn dc_10() -> AircraftPreset {
    AircraftPreset {
        name: "DC-10",
        display_name: "McDonnell Douglas DC-10",
        description: "Classic long-range trijet widebody with underwing and \
                      tail-mounted CF6-50 engines.",
        engine_name: "CF6-50",
        n_engines: 3,
        design_vector: DesignVector {
            span_m: 50.41,
            root_chord_m: 12.80,
            break_chord_m: 7.80,
            tip_chord_m: 1.80,
            sweep_deg: 35.0,
            tip_twist_deg: -2.0,
            wing_x_shift_m: -3.25,
            tail_scale: 1.0,
            fuselage_length_m: 55.35,
            tail_x_shift_m: 0.0,
            airfoil_thickness_scale: 1.0,
            airfoil_camber_scale: 1.0,
            ..DesignVector::default()
        },
        geometry: GeometryConfig {
            wing: WingConfig {
                root_datum_x_m: 21.40,
                root_z_m: -1.6,
                break_z_m: -0.3,
                tip_z_m: 2.0,
                root_twist_deg: 4.0,
                break_twist_deg: 1.5,
                break_span_fraction: 0.35,
                outboard_sweep_decrement_deg: 2.0,
                root_airfoil: "sc20612".to_owned(),
                tip_airfoil: "sc20410".to_owned(),
                ..WingConfig::default()
            },
            empennage: EmpennageConfig {
                tail_airfoil: "naca0012".to_owned(),
                hstab_offset_from_tail_m: 8.5,
                hstab_z_m: 1.0,
                hstab_root_chord_m: 7.2,
                hstab_tip_chord_m: 2.0,
                hstab_root_twist_deg: -2.0,
                hstab_tip_twist_deg: -2.0,
                hstab_tip_le_m: (5.8, 8.5, 0.8),
                vstab_offset_from_tail_m: 10.0,
                vstab_z_m: 2.2,
                vstab_root_chord_m: 10.5,
                vstab_tip_chord_m: 3.8,
                vstab_tip_le_m: (7.5, 0.0, 9.5),
                ..EmpennageConfig::default()
            },
            fuselage: FuselageConfig {
                diameter_m: 6.02,
                nose_z_m: -0.4,
                cabin_start_x_m: 5.5,
                cabin_z_m: 0.2,
                tailcone_length_m: 11.5,
                tail_z_m: 1.6,
                ..FuselageConfig::default()
            },
            engine: EngineConfig {
                engine_name: "CF6-50".to_owned(),
                // The third position is the centerline engine, which sits on
                // the fin rather than under the wing. The vertical offset
                // below is the underwing pair's, so nothing here places the
                // tail engine correctly; whatever draws it has to know.
                spanwise_positions_m: vec![8.8, -8.8, 0.0],
                z_m: -2.12,
                inlet_x_offset_m: 3.5,
                ..EngineConfig::default()
            },
            ..GeometryConfig::default()
        },
        requirements: DesignRequirements {
            cruise_mach: 0.82,
            cruise_altitude_m: 10668.0,
            mtow_kg: 259_450.0,
            max_wing_area_m2: 338.8,
            min_wing_loading_kg_m2: 500.0,
            num_passengers: 250,
            cargo_payload_kg: 65_000.0,
            // DC-10-30: maximum zero-fuel weight about 182 t less an operating
            // empty weight of about 121.2 t.
            max_structural_payload_kg: 48_000.0,
            dive_speed_m_s: 210.0,
            ..DesignRequirements::default()
        },
        mass_model: None,
        performance: super::high_lift("advanced_highlift_widebody"),
    }
}
