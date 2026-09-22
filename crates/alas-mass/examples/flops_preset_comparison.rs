// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Emit the controlled pure-FLOPS comparison record for the eight registered
//! aircraft presets.
//!
//! The production column is evaluated first through the same complete FLOPS
//! product entry point used by mass analysis. The legacy column is a
//! separately selected comparison control and never supplies a production
//! fallback. The record keeps manufacturer/reference fields beside the
//! result, but does not turn a published OEW into an accuracy claim.
//!
//! Usage:
//!
//! ```text
//! cargo run -p alas-mass --example flops_preset_comparison -- \
//!     outputs/pure-flops-production/raw.json \
//!     path/to/mass_reference_anchors.json
//! ```

#![allow(clippy::print_stdout)]

use alas_config::{presets, AircraftPreset, AlasConfig, DesignMissionEvidence, MassArchitecture};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    calculate_component_masses, calculate_flops_mass_buildup, ComponentMassError, FlopsMassBuildup,
    MassBreakdown, ProductMassBuildup, OEW_KEYS,
};
use serde_json::{json, Map, Value};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn map_pairs<T>(pairs: impl IntoIterator<Item = (&'static str, T)>) -> Map<String, Value>
where
    T: serde::Serialize,
{
    pairs
        .into_iter()
        .map(|(name, value)| (name.to_owned(), json!(value)))
        .collect()
}

fn masses_json(masses: &MassBreakdown) -> Value {
    Value::Object(map_pairs(masses.as_pairs()))
}

fn oew_kg(masses: &MassBreakdown) -> f64 {
    OEW_KEYS.iter().filter_map(|name| masses.get(name)).sum()
}

fn evidence_row<'a>(evidence: Option<&'a Value>, name: &str) -> Option<&'a Value> {
    evidence?.get("presets")?.get(name)
}

fn evidence_anchor_value(evidence: Option<&Value>, name: &str, field: &str) -> Option<f64> {
    evidence_row(evidence, name)?
        .get("reference_anchors")?
        .get(field)?
        .get("value")?
        .as_f64()
}

fn selected_reference_value(
    evidence: Option<&Value>,
    name: &str,
    field: &str,
    preset_value: Option<f64>,
) -> Option<f64> {
    // A present evidence row with a null anchor is an intentional source gap;
    // do not resurrect a less-specific preset value behind it. The fallback
    // is used only when the revision-locked evidence artifact is unavailable.
    if evidence_row(evidence, name).is_some() {
        evidence_anchor_value(evidence, name, field)
    } else {
        preset_value
    }
}

fn ledger_json(
    masses: &MassBreakdown,
    mtow_kg: f64,
    preset: &AircraftPreset,
    evidence: Option<&Value>,
) -> Value {
    let oew = oew_kg(masses);
    let zfw = oew + masses.payload;
    let fuel_headroom = mtow_kg - zfw;
    let mzfw = selected_reference_value(evidence, preset.name, "mzfw_kg", preset.reference.mzfw_kg);
    let published_fuel = selected_reference_value(
        evidence,
        preset.name,
        "usable_fuel_mass_kg",
        preset.reference.usable_fuel_mass_kg,
    );
    json!({
        "masses_kg": masses_json(masses),
        "oew_kg": oew,
        "planning_payload_kg": masses.payload,
        "actual_zfw_kg": zfw,
        "mzfw_limit_kg": mzfw,
        "mzfw_margin_kg": mzfw.map(|limit| limit - zfw),
        "mtow_kg": mtow_kg,
        "signed_fuel_closure_kg": masses.signed_fuel_closure_kg(),
        "physical_fuel_mass_kg": masses.physical_fuel_mass_kg(),
        "signed_fuel_headroom_kg": fuel_headroom,
        "fuel_closure_error_kg": masses.signed_fuel_closure_kg() - fuel_headroom,
        "nonnegative_mass_headroom": fuel_headroom >= 0.0,
        "published_reference_usable_fuel_mass_kg": published_fuel,
    })
}

fn reference_json(preset: &AircraftPreset, evidence: Option<&Value>) -> Value {
    let reference = &preset.reference;
    let mass_reference = |field: &str, value: Option<f64>| {
        selected_reference_value(evidence, preset.name, field, value)
    };
    let mission = match &reference.design_mission_evidence {
        DesignMissionEvidence::SourceBacked(mission) => json!({
            "status": "source_backed",
            "source": mission.source,
        }),
        DesignMissionEvidence::Unverified => json!({
            "status": "unverified",
            "source": Value::Null,
        }),
    };
    let partial_missions = reference
        .partial_design_mission_evidence
        .iter()
        .map(|entry| {
            json!({
                "kind": format!("{:?}", entry.kind),
                "range": format!("{:?}", entry.range),
                "payload_kg": entry.payload_kg,
                "load_case": format!("{:?}", entry.load_case),
                "profile_assumptions": entry.profile_assumptions,
                "reserve_assumptions": entry.reserve_assumptions,
                "applicability": entry.applicability,
                "configuration_applicability": format!("{:?}", entry.configuration_applicability),
                "missing": entry.missing.iter().map(|datum| format!("{:?}", datum)).collect::<Vec<_>>(),
                "source": entry.source,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "actual_aircraft_data": preset.name != "AVE",
        "data_status": if preset.name == "AVE" {
            "no_actual_aircraft_data"
        } else {
            "manufacturer_or_certification_reference_values_when_present"
        },
        "identity": {
            "model": preset.identity.model,
            "weight_variant": preset.identity.weight_variant,
            "engine_model": preset.identity.engine_model,
            "modification_state": preset.identity.modification_state,
            "tank_configuration": preset.identity.tank_configuration,
        },
        "mrw_kg": mass_reference("mrw_kg", reference.mrw_kg),
        "mtow_kg": mass_reference("mtow_kg", reference.mtow_kg),
        "mlw_kg": mass_reference("mlw_kg", reference.mlw_kg),
        "mzfw_kg": mass_reference("mzfw_kg", reference.mzfw_kg),
        // The operating empty mass anchor is read from the one OEW reference
        // registry, never from the older evidence artifact.
        "oew_kg": reference.oew_kg,
        "oew_reference": alas_config::oew_reference::get(preset.name)
            .map(|record| record.to_json())
            .unwrap_or(Value::Null),
        "usable_fuel_volume_l": mass_reference("usable_fuel_volume_l", reference.usable_fuel_volume_l),
        "usable_fuel_mass_kg": mass_reference("usable_fuel_mass_kg", reference.usable_fuel_mass_kg),
        "fuel_density_kg_l": mass_reference("fuel_density_kg_l", reference.fuel_density_kg_l),
        "reference_wing_area_m2": reference.reference_wing_area_m2,
        "planning_seats": reference.planning_seats,
        "certified_max_seats": reference.certified_max_seats,
        "design_mission": mission,
        "partial_design_mission_evidence": partial_missions,
        "cg_evidence": format!("{:?}", reference.cg_evidence),
        "source_documents": reference.sources,
        "mass_reference_source": if evidence_row(evidence, preset.name).is_some() {
            "out/evidence/data/pure-flops-evidence/mass_reference_anchors.json"
        } else {
            "registered preset reference metadata"
        },
        "mass_reference_evidence": evidence_row(evidence, preset.name)
            .and_then(|row| row.get("reference_anchors"))
            .cloned()
            .unwrap_or(Value::Null),
    })
}

fn resolved_inputs_json(build: &FlopsMassBuildup) -> Value {
    let inputs = build.inputs;
    json!({
        "maximum_mach": inputs.maximum_mach,
        "design_range_nmi": inputs.design_range_nmi,
        "design_gross_mass_kg": inputs.design_gross_mass_kg,
        "wing_area_m2": inputs.wing_area_m2,
        "movable_surface_area_m2": inputs.movable_surface_area_m2,
        "wing_span_m": inputs.wing_span_m,
        "quarter_chord_sweep_deg": inputs.quarter_chord_sweep_deg,
        "fuselage_length_m": inputs.fuselage_length_m,
        "fuselage_width_m": inputs.fuselage_width_m,
        "fuselage_depth_m": inputs.fuselage_depth_m,
        "fuselage_count": inputs.fuselage_count,
        "passenger_compartment_length_m": inputs.passenger_compartment_length_m,
        "first_class_passenger_count": inputs.first_class_passenger_count,
        "business_class_passenger_count": inputs.business_class_passenger_count,
        "tourist_class_passenger_count": inputs.tourist_class_passenger_count,
        "flight_crew_count": inputs.flight_crew_count,
        "flight_attendant_count": inputs.flight_attendant_count,
        "galley_crew_count": inputs.galley_crew_count,
        "wing_mounted_engine_count": inputs.wing_mounted_engine_count,
        "fuselage_mounted_engine_count": inputs.fuselage_mounted_engine_count,
        "engine_count": inputs.engine_count,
        "rated_thrust_per_engine_n": inputs.rated_thrust_per_engine_n,
        "nacelle_diameter_m": inputs.nacelle_diameter_m,
        "hydraulic_pressure_pa": inputs.hydraulic_pressure_pa,
        "variable_sweep_penalty": inputs.variable_sweep_penalty,
        "maximum_fuel_capacity_kg": inputs.maximum_fuel_capacity_kg,
        "fuel_tank_count": inputs.fuel_tank_count,
        "containerized_cargo_kg": inputs.containerized_cargo_kg,
        // The three declarations that decide how the cabin equipment, the
        // occupant operating items and the container tare are priced. They are
        // resolved by one rule for a preset and for a configuration built
        // without one; a matrix that does not print them cannot show that.
        "cabin_equipment_method": inputs.cabin_equipment_method.as_str(),
        "cargo_loading": inputs.cargo_loading.as_str(),
        "haul_class": inputs.haul_class.as_str(),
        "containerized_baggage_kg": inputs.containerized_baggage_kg,
    })
}

fn groups_json(build: &FlopsMassBuildup) -> Value {
    let systems = build.systems_and_operating_items.systems;
    let operating = build.systems_and_operating_items.operating_items;
    let structure = build.airframe.structure;
    let propulsion = build.airframe.propulsion;
    json!({
        "systems": {
            "surface_controls_kg": systems.surface_controls_kg,
            "apu_kg": systems.apu_kg,
            "instruments_kg": systems.instruments_kg,
            "hydraulics_kg": systems.hydraulics_kg,
            "electrical_kg": systems.electrical_kg,
            "avionics_kg": systems.avionics_kg,
            "furnishings_kg": systems.furnishings_kg,
            "air_conditioning_kg": systems.air_conditioning_kg,
            "anti_ice_kg": systems.anti_ice_kg,
            "total_kg": systems.total_kg,
        },
        "operating_items": {
            "cabin_crew_and_baggage_kg": operating.cabin_crew_and_baggage_kg,
            "flight_crew_and_baggage_kg": operating.flight_crew_and_baggage_kg,
            "unusable_fuel_kg": operating.unusable_fuel_kg,
            "engine_oil_kg": operating.engine_oil_kg,
            "passenger_service_kg": operating.passenger_service_kg,
            // Reported outside operating empty mass: Boeing D6-58333 Rev Q
            // section 2.1 and FAA AC 120-27F both exclude unit load devices,
            // and AC 120-85B tracks them with the load. FLOPS is the outlier
            // in carrying `WCON` inside `WOPIT`, and its own convention is
            // kept reproducible on the line below.
            "cargo_containers_outside_oew_kg": operating.cargo_containers_kg,
            "total_kg": operating.total_kg,
            "flops_wopit_total_with_cargo_containers_kg":
                operating.total_with_cargo_containers_kg,
        },
        "airframe": {
            "structure": structure.map(|group| json!({
                "wing": {
                    "bending_factor": group.wing.bending_factor,
                    "inertia_relief_factor": group.wing.inertia_relief_factor,
                    "bending_material_kg": group.wing.bending_material_kg,
                    "shear_and_control_kg": group.wing.shear_and_control_kg,
                    "miscellaneous_kg": group.wing.miscellaneous_kg,
                    "total_kg": group.wing.total_kg,
                },
                "horizontal_tail_kg": group.horizontal_tail_kg,
                "vertical_tail_kg": group.vertical_tail_kg,
                "fuselage_kg": group.fuselage_kg,
                "main_gear_kg": group.main_gear_kg,
                "nose_gear_kg": group.nose_gear_kg,
                "nacelle_kg": group.nacelle_kg,
                "paint_kg": group.paint_kg,
                "total_kg": group.total_kg,
            })),
            "propulsion": propulsion.map(|group| json!({
                "total_nacelles": group.total_nacelles,
                "baseline_engine_mass_kg": group.baseline_engine_mass_kg,
                "engine_core_each_kg": group.engine_core_each_kg,
                "inlet_each_kg": group.inlet_each_kg,
                "nozzle_each_kg": group.nozzle_each_kg,
                "engine_each_kg": group.engine_each_kg,
                "engine_cores_kg": group.engine_cores_kg,
                "inlets_kg": group.inlets_kg,
                "nozzles_kg": group.nozzles_kg,
                "engines_kg": group.engines_kg,
                "thrust_reversers_kg": group.thrust_reversers_kg,
                "engine_controls_kg": group.engine_controls_kg,
                "starters_kg": group.starters_kg,
                "misc_kg": group.misc_kg,
                "fuel_system_kg": group.fuel_system_kg,
                // Outside the published FLOPS boundary: NASA/TM-2017-219627
                // Vol. I has no pylon equation at all, so this line is a
                // declared addition and has to be visible as one rather than
                // disappearing into the group total.
                "pylons_kg": group.pylons_kg,
                "total_kg": group.total_kg,
            })),
        },
        "nacelle_kg": build.nacelle_kg(),
        "propulsion_without_nacelles_kg": build.propulsion_without_nacelles_kg(),
    })
}

fn partial_json(partial: &alas_mass::flops_transport::PartialFlopsTransportBreakdown) -> Value {
    json!({
        "surface_controls_kg": partial.surface_controls_kg,
        "apu_kg": partial.apu_kg,
        "instruments_kg": partial.instruments_kg,
        "hydraulics_kg": partial.hydraulics_kg,
        "electrical_kg": partial.electrical_kg,
        "avionics_kg": partial.avionics_kg,
        "furnishings_kg": partial.furnishings_kg,
        "air_conditioning_kg": partial.air_conditioning_kg,
        "anti_ice_kg": partial.anti_ice_kg,
        "cabin_crew_and_baggage_kg": partial.cabin_crew_and_baggage_kg,
        "flight_crew_and_baggage_kg": partial.flight_crew_and_baggage_kg,
        "unusable_fuel_kg": partial.unusable_fuel_kg,
        "engine_oil_kg": partial.engine_oil_kg,
        "passenger_service_kg": partial.passenger_service_kg,
        "cargo_containers_kg": partial.cargo_containers_kg,
    })
}

fn production_json(
    plane: &alas_geom::aircraft::airplane::Airplane,
    config: &AlasConfig,
    preset: &AircraftPreset,
    evidence: Option<&Value>,
) -> Result<Value, Box<dyn Error>> {
    let result = calculate_flops_mass_buildup(
        plane,
        &config.requirements,
        &config.geometry,
        &config.control_surfaces,
        Some(&config.mass_model),
        &config.landing_gear,
        &config.cabin,
    );
    match result {
        Ok(ProductMassBuildup::PureFlops(build)) => Ok(json!({
            "status": "evaluated_declared_inputs",
            "mass_architecture": config.mass_model.mass_architecture.as_str(),
            "physical_validation": "not_performed",
            "masses_kg": masses_json(&build.masses),
            "ledger": ledger_json(&build.masses, config.requirements.mtow_kg, preset, evidence),
            "resolved_inputs": resolved_inputs_json(&build),
            "provenance": serde_json::to_value(&build.provenance)?,
            "groups": groups_json(&build),
        })),
        Ok(ProductMassBuildup::LegacyComparison(_)) => Ok(json!({
            "status": "invalid_production_result",
            "mass_architecture": config.mass_model.mass_architecture.as_str(),
            "error": "production entry point returned the legacy comparison variant",
        })),
        Err(ComponentMassError::FlopsUnverified { reasons, partial }) => Ok(json!({
            "status": "unsupported_or_unverified",
            "mass_architecture": config.mass_model.mass_architecture.as_str(),
            "physical_validation": "not_performed",
            "masses_kg": Value::Null,
            "ledger": Value::Null,
            "blockers": reasons.iter().map(|reason| reason.as_str()).collect::<Vec<_>>(),
            "partial_components": partial_json(&partial),
        })),
        Err(error) => Ok(json!({
            "status": "error",
            "mass_architecture": config.mass_model.mass_architecture.as_str(),
            "masses_kg": Value::Null,
            "ledger": Value::Null,
            "error": error.to_string(),
        })),
    }
}

fn legacy_json(
    plane: &alas_geom::aircraft::airplane::Airplane,
    config: &AlasConfig,
    preset: &AircraftPreset,
    evidence: Option<&Value>,
) -> Value {
    let mut legacy_config = config.clone();
    legacy_config.mass_model.mass_architecture =
        MassArchitecture::LegacyReferenceCompatibleComparison;
    legacy_config.mass_model.apply_architecture();
    let masses = calculate_component_masses(
        plane,
        &legacy_config.requirements,
        &legacy_config.geometry,
        Some(&legacy_config.mass_model),
    );
    json!({
        "status": "evaluated_comparison_control",
        "mass_architecture": MassArchitecture::LegacyReferenceCompatibleComparison.as_str(),
        "physical_validation": "not_performed",
        "masses_kg": masses_json(&masses),
        "ledger": ledger_json(&masses, legacy_config.requirements.mtow_kg, preset, evidence),
    })
}

fn aircraft_row(name: &str, evidence: Option<&Value>) -> Result<Value, Box<dyn Error>> {
    let preset = presets::get(name).map_err(std::io::Error::other)?;
    let config = AlasConfig::from_value(&json!({"preset": name}))?;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)?;
    let production = production_json(&plane, &config, preset, evidence)?;
    let legacy = legacy_json(&plane, &config, preset, evidence);
    Ok(json!({
        "preset": name,
        "display_name": preset.display_name,
        "description": preset.description,
        "identity": {
            "model": preset.identity.model,
            "weight_variant": preset.identity.weight_variant,
            "engine_model": preset.identity.engine_model,
            "modification_state": preset.identity.modification_state,
            "tank_configuration": preset.identity.tank_configuration,
        },
        "reference": reference_json(preset, evidence),
        "production": production,
        "legacy_comparison": legacy,
    }))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let output = args.next().map(PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../outputs/pure-flops-production/raw.json")
    });
    let evidence_path = args.next().map(PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../out/evidence/data/pure-flops-evidence/mass_reference_anchors.json")
    });
    if args.next().is_some() {
        return Err(
            "usage: flops_preset_comparison [output.json] [mass_reference_anchors.json]".into(),
        );
    }
    let evidence = if evidence_path.is_file() {
        Some(serde_json::from_reader(fs::File::open(&evidence_path)?)?)
    } else {
        None
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let rows = presets::available()
        .into_iter()
        .map(|name| aircraft_row(name, evidence.as_ref()))
        .collect::<Result<Vec<_>, _>>()?;
    let generated_epoch = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let report = json!({
        "schema_version": 2,
            "generated_by": "cargo run -p alas-mass --example flops_preset_comparison [output.json] [mass_reference_anchors.json]",
        "generated_utc_epoch_s": generated_epoch,
        "model": {
            "production_architecture": MassArchitecture::PureFlopsTransportV1.as_str(),
            "legacy_architecture": MassArchitecture::LegacyReferenceCompatibleComparison.as_str(),
            "purpose": "same-preset-geometry component mass comparison",
            "validation_scope": "equation parity, conservation and architecture wiring; not physical aircraft validation",
            "no_accuracy_fit": true,
        },
        "sources": {
            "nasa_tm": "https://ntrs.nasa.gov/citations/20170005851",
            "aviary_pinned_commit": "https://github.com/OpenMDAO/Aviary/tree/c7affbbe54dcbeded7373eae05f771882e2bb28a",
            "local_source_copy": "out/evidence/data/flops-reference-20260911/",
            "reference_manifest": "docs/flops-mass-sources.json",
            "mass_reference_anchors": evidence.as_ref().map(|_| evidence_path.display().to_string()),
            "aircraft_input_evidence": "out/evidence/data/pure-flops-evidence/aircraft_inputs.json",
            "aircraft_source_manifest": "out/evidence/data/pure-flops-evidence/source_manifest.json",
        },
        "summary": {
            "aircraft_count": rows.len(),
            "production_evaluated_count": rows.iter().filter(|row| row["production"]["status"] == "evaluated_declared_inputs").count(),
            "unsupported_or_unverified_count": rows.iter().filter(|row| row["production"]["status"] == "unsupported_or_unverified").count(),
            "notes": [
                "AVE is not an actual aircraft dataset; its reference fields are explicitly contextual or absent.",
                "Published manufacturer values are reference anchors for the named variant and do not establish model accuracy.",
                "ATR72-600 is expected to remain unsupported because the pinned NASA transport memorandum has no propeller or shaft-power mass equation.",
            ],
        },
        "aircraft": rows,
    });
    fs::write(&output, serde_json::to_string_pretty(&report)? + "\n")?;
    println!("{}", output.display());
    Ok(())
}
