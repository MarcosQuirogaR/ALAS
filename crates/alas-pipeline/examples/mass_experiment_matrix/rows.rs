// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CSV sections assembled from the fixed-case and mission-case JSON records.

use alas_config::design_variables::DesignVector;
use serde_json::Value;

use super::fixed::{reconstructed_case, reconstructed_cases};
use super::mission::{climb_variant_case, climb_variants};
use super::support::{csv_line, opt};

/// The A320/A220 climb-diagnosis rows and their JSON records.
pub(crate) fn climb_section(name: &str, design: &DesignVector) -> (Vec<String>, Vec<Value>) {
    let mut rows = Vec::new();
    let mut variants = Vec::new();
    for variant in climb_variants() {
        println!("   climb variant {}", variant.label);
        let result = climb_variant_case(name, design, &variant);
        let gs = |ptr: &str| {
            result
                .pointer(ptr)
                .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
                .unwrap_or_default()
        };
        rows.push(csv_line(&[
            name.to_owned(),
            variant.label.to_owned(),
            gs("/status"),
            gs("/takeoff_mass_kg"),
            gs("/zero_fuel_mass_kg"),
            gs("/block_fuel_kg"),
            gs("/profile/speed_reference"),
            gs("/profile/step_climb_1_air_speed_m_s"),
            gs("/thrust_kn_per_engine"),
            gs("/dispatch_status").chars().take(300).collect(),
            variant.note.to_owned(),
        ]));
        variants.push(result);
    }
    (rows, variants)
}

/// The reconstructed reference-case rows (header included) and their JSON
/// records, restricted to `selected` presets when a list is given.
pub(crate) fn reconstructed_section(selected: Option<&[String]>) -> (Vec<String>, Vec<Value>) {
    let mut rows = vec![csv_line(
        &[
            "label",
            "preset",
            "status",
            "declared_mtow_kg",
            "design_gross_mass_kg",
            "design_landing_mass_kg",
            "seated_pax",
            "unseated_pax",
            "flops_first",
            "flops_business",
            "flops_tourist",
            "flight_attendants",
            "wing_kg",
            "fuselage_kg",
            "gear_kg",
            "propulsion_kg",
            "systems_kg",
            "furnishings_kg",
            "oew_kg",
            "reference_oew_kg",
            "reference_status",
            "reference_tier",
            "oew_residual_kg",
            "oew_residual_pct",
            "declared_baseline_engine_mass_kg",
            "payload_kg",
            "actual_zfw_kg",
            "signed_fuel_closure_kg",
        ]
        .map(String::from),
    )];
    let mut reconstructed = Vec::new();
    for case in reconstructed_cases() {
        if let Some(list) = selected {
            if !list.iter().any(|name| name == case.preset) {
                continue;
            }
        }
        println!("== reconstructed {}", case.label);
        let row = reconstructed_case(&case);
        let g = |ptr: &str| row.pointer(ptr).and_then(Value::as_f64);
        let gs = |ptr: &str| {
            row.pointer(ptr)
                .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
                .unwrap_or_default()
        };
        rows.push(csv_line(&[
            case.label.to_owned(),
            case.preset.to_owned(),
            gs("/status"),
            opt(g("/declared_mtow_kg")),
            opt(g("/buildup/design_gross_mass_kg")),
            opt(g("/buildup/design_landing_mass_kg")),
            gs("/layout/seated_pax"),
            gs("/layout/unseated_pax"),
            gs("/buildup/class_split/0"),
            gs("/buildup/class_split/1"),
            gs("/buildup/class_split/2"),
            gs("/buildup/flight_attendants"),
            opt(g("/buildup/masses_kg/Wing")),
            opt(g("/buildup/masses_kg/Fuselage")),
            opt(g("/buildup/masses_kg/Gear")),
            opt(g("/buildup/masses_kg/Propulsion")),
            opt(g("/buildup/masses_kg/Systems")),
            opt(g("/buildup/masses_kg/Furnishings")),
            opt(g("/oew_kg")),
            opt(g("/reference_oew_kg")),
            gs("/reference_status"),
            gs("/reference_tier"),
            opt(g("/oew_residual_kg")),
            opt(g("/oew_residual_pct")),
            opt(g("/declared_baseline_engine_mass_kg")),
            opt(g("/buildup/masses_kg/Payload")),
            opt(g("/actual_zfw_kg")),
            opt(g("/signed_fuel_closure_kg")),
        ]));
        reconstructed.push(row);
    }
    (rows, reconstructed)
}
