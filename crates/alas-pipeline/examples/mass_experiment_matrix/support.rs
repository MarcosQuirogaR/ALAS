// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared helpers: CSV formatting, JSON views of a buildup and dispatch text.

use std::sync::atomic::{AtomicBool, Ordering};

use alas_config::oew_reference;
use alas_config::AlasConfig;
use alas_mass::breakdown::{FlopsMassBuildup, MassBreakdown, OEW_KEYS};
use alas_mass::dispatch::DispatchStatus;
use serde_json::{json, Map, Value};

/// Design-range points (nmi) of the fixed-aircraft mission sweep, besides the
/// operational route and the declared FLOPS design range.
pub(crate) const SWEEP_RANGES_NMI: &[f64] = &[500.0, 1_000.0, 2_000.0];

/// `--engine-fallback`: evaluate every case with the FLOPS equation 76
/// engine correlation instead of the registered certified dry engine mass,
/// so a before/after pair comes from one binary and one tree.
pub(crate) static ENGINE_FALLBACK: AtomicBool = AtomicBool::new(false);

/// Apply the run-wide options to a freshly loaded configuration.
pub(crate) fn apply_run_options(config: &mut AlasConfig) {
    if ENGINE_FALLBACK.load(Ordering::Relaxed) {
        config.mass_model.flops_structure.baseline_engine_mass_kg = None;
    }
}

pub(crate) fn oew_kg(masses: &MassBreakdown) -> f64 {
    OEW_KEYS.iter().filter_map(|name| masses.get(name)).sum()
}

pub(crate) fn masses_json(masses: &MassBreakdown) -> Value {
    let mut map = Map::new();
    for (name, value) in masses.as_pairs() {
        map.insert(name.to_owned(), json!(value));
    }
    Value::Object(map)
}

pub(crate) fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_owned()
    }
}

pub(crate) fn csv_line(fields: &[String]) -> String {
    fields
        .iter()
        .map(|field| csv_escape(field))
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn fmt(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.6}")
    } else {
        String::new()
    }
}

pub(crate) fn opt(value: Option<f64>) -> String {
    value.map_or_else(String::new, fmt)
}

pub(crate) fn dispatch_text(status: &DispatchStatus) -> String {
    match status {
        DispatchStatus::Converged => "converged".to_owned(),
        DispatchStatus::MtowLimited { shortfall_kg } => {
            format!("mtow_limited shortfall {shortfall_kg:.1} kg")
        }
        DispatchStatus::TankLimited { shortfall_kg } => {
            format!("tank_limited shortfall {shortfall_kg:.1} kg")
        }
        DispatchStatus::NotConverged { last_change_kg } => {
            format!("not_converged last change {last_change_kg:.1} kg")
        }
        DispatchStatus::ModelFailed(reason) => format!("model_failed: {reason}"),
    }
}

/// Everything a buildup says about the sizing basis it was evaluated on.
pub(crate) fn buildup_json(buildup: &FlopsMassBuildup) -> Value {
    let structure = &buildup.airframe.structure_inputs;
    let propulsion = buildup.airframe.propulsion;
    let groups = &buildup.systems_and_operating_items;
    json!({
        "masses_kg": masses_json(&buildup.masses),
        "oew_kg": oew_kg(&buildup.masses),
        "design_gross_mass_kg": structure.wing.design_gross_mass_kg,
        "design_landing_mass_kg": structure.design_landing_mass_kg,
        "ultimate_load_factor": structure.wing.ultimate_load_factor,
        "systems_design_gross_mass_kg": buildup.inputs.design_gross_mass_kg,
        "sources": {
            "landing_mass": buildup.airframe.sources.landing_mass,
            "main_gear_length": buildup.airframe.sources.main_gear_length,
            "nose_gear_length": buildup.airframe.sources.nose_gear_length,
            "baseline_engine_mass": buildup.airframe.sources.baseline_engine_mass,
        },
        "class_split": [
            buildup.inputs.first_class_passenger_count,
            buildup.inputs.business_class_passenger_count,
            buildup.inputs.tourist_class_passenger_count,
        ],
        "flight_attendants": buildup.inputs.flight_attendant_count,
        "design_range_nmi": buildup.inputs.design_range_nmi,
        "maximum_fuel_capacity_kg": buildup.inputs.maximum_fuel_capacity_kg,
        "containerized_cargo_kg": buildup.inputs.containerized_cargo_kg,
        "structure": buildup.airframe.structure.map(|s| json!({
            "wing_kg": s.wing.total_kg,
            "wing_bending_kg": s.wing.bending_material_kg,
            "wing_shear_control_kg": s.wing.shear_and_control_kg,
            "wing_misc_kg": s.wing.miscellaneous_kg,
            "horizontal_tail_kg": s.horizontal_tail_kg,
            "vertical_tail_kg": s.vertical_tail_kg,
            "fuselage_kg": s.fuselage_kg,
            "main_gear_kg": s.main_gear_kg,
            "nose_gear_kg": s.nose_gear_kg,
            "nacelle_kg": s.nacelle_kg,
            "paint_kg": s.paint_kg,
        })),
        "propulsion": propulsion.map(|p| json!({
            "engine_each_kg": p.engine_each_kg,
            "engines_kg": p.engines_kg,
            "thrust_reversers_kg": p.thrust_reversers_kg,
            "engine_controls_kg": p.engine_controls_kg,
            "starters_kg": p.starters_kg,
            "fuel_system_kg": p.fuel_system_kg,
            "total_without_nacelles_kg": p.total_kg,
        })),
        "systems": {
            "surface_controls_kg": groups.systems.surface_controls_kg,
            "apu_kg": groups.systems.apu_kg,
            "instruments_kg": groups.systems.instruments_kg,
            "hydraulics_kg": groups.systems.hydraulics_kg,
            "electrical_kg": groups.systems.electrical_kg,
            "avionics_kg": groups.systems.avionics_kg,
            "furnishings_kg": groups.systems.furnishings_kg,
            "air_conditioning_kg": groups.systems.air_conditioning_kg,
            "anti_ice_kg": groups.systems.anti_ice_kg,
        },
        "operating_items": {
            "cabin_crew_and_baggage_kg": groups.operating_items.cabin_crew_and_baggage_kg,
            "flight_crew_and_baggage_kg": groups.operating_items.flight_crew_and_baggage_kg,
            "unusable_fuel_kg": groups.operating_items.unusable_fuel_kg,
            "engine_oil_kg": groups.operating_items.engine_oil_kg,
            "passenger_service_kg": groups.operating_items.passenger_service_kg,
            "cargo_containers_kg": groups.operating_items.cargo_containers_kg,
            "total_kg": groups.operating_items.total_kg,
        },
    })
}

/// The registry record of a preset as the JSON every artifact carries, with
/// the residual of `oew` against the comparable value when there is one.
pub(crate) fn oew_reference_json(name: &str, oew: f64) -> Value {
    let Some(record) = oew_reference::get(name) else {
        return json!({ "status": "no_registry_record" });
    };
    let mut value = record.to_json();
    if let Value::Object(map) = &mut value {
        map.insert(
            "oew_residual_kg".to_owned(),
            json!(record.reference_oew_kg.map(|r| oew - r)),
        );
        map.insert(
            "oew_residual_pct".to_owned(),
            json!(record.reference_oew_kg.map(|r| 100.0 * (oew - r) / r)),
        );
    }
    value
}
