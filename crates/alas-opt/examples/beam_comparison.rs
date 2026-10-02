// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reproduce strength-only and in-loop wing structural inventories in SI.
//! Usage: cargo run -p alas-opt --example beam_comparison -- OUTPUT_JSON [BEFORE_JSON]
//! The deck mass is material in the generated FE model, not a solver result.
//! Optional historical shell accounting uses recorded rib counts and gauges;
//! original measurements remain unchanged. Both outputs use nominal presets.

#[path = "beam_comparison/diagnostics.rs"]
mod diagnostics;

use alas_config::{materials, presets, AlasConfig};
use alas_geom::{builder::AircraftBuilder, wing_structure::WingStructureGeometry};
use alas_mass::breakdown::{run_product_mass_analysis_with_groups, MassCoordinateModel};
use alas_opt::mdo::structural_feasibility::{assess_candidate, structural_design_mass_kg};
use alas_struct::analytical::{analyze_structure_with_wing_carried_mass, StructuralAnalysisReport};
use alas_struct::feasibility::{assess, LinearModelLimits, StructuralFeasibility};
use alas_struct::mesh::build_wing_mesh_bdf_product;
use alas_struct::sizing::{
    box_chord_band, size_for_linear_model, size_wingbox_with_scope, sizing_stations,
    WingFuelRelief, WingboxSizing,
};
use serde_json::{json, Value};
use std::{error::Error, fs, path::Path};

fn trapezoid(values: &[f64], stations: &[f64]) -> f64 {
    values
        .windows(2)
        .zip(stations.windows(2))
        .map(|(v, y)| 0.5 * (v[0] + v[1]) * (y[1] - y[0]))
        .sum()
}

fn metrics(
    sizing: &WingboxSizing,
    response: &StructuralAnalysisReport,
    feasibility: &StructuralFeasibility,
) -> Value {
    let mass = &sizing.mass_breakdown_kg;
    json!({
        "primary_mass_kg": 2.0 * sizing.total_mass_kg,
        "primary_items_kg": {"caps": 2.0 * mass.spar_caps, "webs": 2.0 * mass.spar_webs,
            "covers": 2.0 * mass.skin, "ribs": 2.0 * mass.ribs},
        "root_ei_nm2": response.ei_nm2.first(),
        "stations_m": sizing.y_stations,
        "station_chords_m": sizing.chord,
        "root_cap_widths_m": sizing.spars.iter().map(|s| s.w_cap[0]).collect::<Vec<_>>(),
        "root_cap_thicknesses_m": sizing.spars.iter().map(|s| s.t_cap[0]).collect::<Vec<_>>(),
        "root_cap_areas_m2": sizing.spars.iter().map(|s| s.a_cap[0]).collect::<Vec<_>>(),
        "root_spar_heights_m": sizing.spars.iter().map(|s| s.h[0]).collect::<Vec<_>>(),
        "web_thicknesses_m": sizing.spars.iter().map(|s| s.t_web).collect::<Vec<_>>(),
        "cover_thickness_m": sizing.t_skin, "num_ribs": sizing.num_ribs,
        "passes": feasibility.passes(), "input_valid": feasibility.input_valid,
        "strength_utilization": feasibility.max_strength_utilization,
        "rib_spacing_ratio": feasibility.rib_spacing_ratio,
        "cap_packaging_ratio": feasibility.cap_packaging_ratio,
        "level_curvature_error": feasibility.max_linear_curvature_relative_error,
        "ultimate_curvature_error": feasibility.manoeuvre_curvature_relative_error,
        "ultimate_tip_deflection_ratio": feasibility.max_tip_deflection_ratio,
        "cases": response.load_cases.iter().map(|case| json!({
            "name": case.name, "load_factor": case.load_factor,
            "root_moment_nm": case.moment_nm.first(), "root_shear_n": case.shear_n.first(),
            "tip_deflection_m": case.tip_deflection_m,
            "root_cap_normal_stress_pa": case.spar_stress.iter().map(|s| s.stress_pa.first()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("missing OUTPUT_JSON argument")?;
    let historical_path = std::env::args().nth(2);
    let mut historical = historical_path
        .as_ref()
        .map(|path| fs::read(path).map(|bytes| serde_json::from_slice::<Value>(&bytes)))
        .transpose()?
        .transpose()?;
    let mut rows = Vec::new();
    for name in ["AVE", "A320-200", "B787-9", "A220-300"] {
        let config = AlasConfig::from_value(&json!({"preset": name}))?;
        let design = presets::get(name)?.design_vector;
        let plane =
            AircraftBuilder::new(Some(config.geometry.clone())).build(Some(&design), true)?;
        let wing = alas_mass::wing_reconciliation::main_wing(&plane).ok_or("missing wing")?;
        let root = wing.xsecs.first().ok_or("missing root")?;
        let tip = wing.xsecs.last().ok_or("missing tip")?;
        let cfg = &config.structures;
        let (fractions, full_span) = cfg.resolved_spars();
        let geometry = WingStructureGeometry::new(
            &design,
            &config.geometry.wing,
            &root.airfoil,
            &tip.airfoil,
            &fractions,
            Some(&full_span),
        )?;
        let mut req = config.requirements.clone();
        req.mtow_kg = structural_design_mass_kg(&config);
        let skin = materials::get(&cfg.skin_material)?;
        let web = materials::get(&cfg.spar_web_material)?;
        let cap = materials::get(&cfg.spar_cap_material)?;
        let rib = materials::get(&cfg.rib_material)?;
        let stations = sizing_stations(&geometry, cfg);
        let (front, rear) = box_chord_band(&geometry);
        let declared = alas_mass::wing_reconciliation::declared_wing_fuel_case(
            &config, &design, &req, &geometry, &stations, front, rear,
        );
        let fuel = declared
            .as_ref()
            .map(|case| case.running_mass_kg_m.clone())
            .unwrap_or_else(|| {
                alas_struct::tanks::integral_fuel_running_mass_kg_m(
                    &geometry, &stations, front, rear,
                )
            });
        let fuel_scope = declared
            .as_ref()
            .map_or(WingFuelRelief::EnclosedBoxVolume, |case| {
                WingFuelRelief::Declared {
                    running_mass_kg_m: &fuel,
                    design_case: alas_struct::scope::WingFuelDesignCase::declared(
                        case.capacity_kg,
                        case.design_gross_mass_kg,
                        case.max_zero_fuel_mass_kg,
                    ),
                }
            });
        let mounted = alas_struct::scope::wing_mounted_relief(
            &config.geometry.engine,
            &config.mass_model,
            &req,
        );
        let scoped = size_wingbox_with_scope(
            &geometry,
            cfg,
            &req,
            skin,
            web,
            cap,
            rib,
            &fuel_scope,
            &mounted,
        );
        let raw = scoped.sizing;
        let response = analyze_structure_with_wing_carried_mass(
            &geometry,
            &raw,
            cfg,
            &req,
            &config.geometry.engine,
            &config.mass_model,
            skin,
            web,
            cap,
            &fuel,
            &mounted.point_masses_kg,
        );
        let limits = LinearModelLimits {
            max_curvature_relative_error: cfg.max_linear_curvature_relative_error,
        };
        let raw_assessment = assess(&raw, &response, limits);
        let (raw_deck, _, _) = build_wing_mesh_bdf_product(
            &geometry,
            &raw,
            cfg,
            &config.geometry.engine,
            &config.mass_model,
            &req,
            skin,
            web,
            cap,
            rib,
        )?;
        let mut strength = metrics(&raw, &response, &raw_assessment);
        strength["fe_deck_primary_mass_kg"] =
            json!(raw_deck.primary_structural_mass_kg().map(|m| 2.0 * m));
        strength["fe_items_kg"] = diagnostics::fe_inventory(
            &raw_deck,
            strength["fe_deck_primary_mass_kg"]
                .as_f64()
                .ok_or("invalid FE mass")?,
        )
        .ok_or("invalid FE material accounting")?;
        strength["root_ei_items_nm2"] = diagnostics::root_ei(&raw, skin, web, cap);
        strength["equilibrium"] =
            diagnostics::beam_equilibrium(&response, req.gravity_m_s2, &mounted.point_masses_kg);
        if let Some(old_rows) = historical.as_mut().and_then(Value::as_array_mut) {
            if let Some(row) = old_rows
                .iter_mut()
                .find(|row| row["preset"].as_str() == Some(name))
            {
                for key in ["strength_only", "in_loop"] {
                    // Shell geometry depends on the original rib count and
                    // gauges, not the revised cap sizing. Original cap bar
                    // mass follows from recorded total less these shells.
                    let old = &mut row[key];
                    let mut section = raw.clone();
                    section.num_ribs = old["num_ribs"]
                        .as_i64()
                        .ok_or("missing historical rib count")?;
                    section.t_skin = old["cover_thickness_m"]
                        .as_f64()
                        .ok_or("missing historical cover gauge")?;
                    for (spar, gauge) in section.spars.iter_mut().zip(
                        old["web_thicknesses_m"]
                            .as_array()
                            .ok_or("missing historical web gauges")?,
                    ) {
                        spar.t_web = gauge.as_f64().ok_or("invalid historical web gauge")?;
                    }
                    let (deck, _, _) = build_wing_mesh_bdf_product(
                        &geometry,
                        &section,
                        cfg,
                        &config.geometry.engine,
                        &config.mass_model,
                        &req,
                        skin,
                        web,
                        cap,
                        rib,
                    )?;
                    old["fe_items_kg"] = diagnostics::fe_inventory(
                        &deck,
                        old["fe_deck_primary_mass_kg"]
                            .as_f64()
                            .ok_or("missing historical FE mass")?,
                    )
                    .ok_or("invalid historical shell accounting")?;
                    old["fe_items_provenance"] = json!("Shell geometry reconstructed from recorded rib count/gauges; caps are original total minus shells");
                }
            }
        }
        let linear = size_for_linear_model(
            &geometry,
            raw,
            cfg,
            &req,
            &config.geometry.engine,
            &config.mass_model,
            skin,
            web,
            cap,
            &fuel,
            &mounted.point_masses_kg,
            limits,
        );
        let (linear_deck, _, _) = build_wing_mesh_bdf_product(
            &geometry,
            &linear.sizing,
            cfg,
            &config.geometry.engine,
            &config.mass_model,
            &req,
            skin,
            web,
            cap,
            rib,
        )?;
        let mut in_loop = metrics(&linear.sizing, &linear.response, &linear.assessment);
        in_loop["fe_deck_primary_mass_kg"] =
            json!(linear_deck.primary_structural_mass_kg().map(|m| 2.0 * m));
        in_loop["fe_items_kg"] = diagnostics::fe_inventory(
            &linear_deck,
            in_loop["fe_deck_primary_mass_kg"]
                .as_f64()
                .ok_or("invalid FE mass")?,
        )
        .ok_or("invalid FE material accounting")?;
        in_loop["root_ei_items_nm2"] = diagnostics::root_ei(&linear.sizing, skin, web, cap);
        in_loop["equilibrium"] = diagnostics::beam_equilibrium(
            &linear.response,
            req.gravity_m_s2,
            &mounted.point_masses_kg,
        );
        in_loop["iterations"] = json!(linear.iterations);
        in_loop["converged"] = json!(linear.converged);
        let assessment = assess_candidate(&config, &design, &plane)?;
        let analysis_mass = config.analysis_mass_model(req.mtow_kg);
        let (masses, coords, _, _) = run_product_mass_analysis_with_groups(
            &plane,
            &req,
            &config.geometry,
            &config.cabin,
            &config.control_surfaces,
            Some(&analysis_mass),
            None,
            MassCoordinateModel::StructuralWingbox(cfg),
            &config.landing_gear,
        )?;
        let (product_coords, aircraft_cg) = alas_mass::product_stations::product_mass_coordinates(
            &config, &design, &plane, &masses, coords,
        )?;
        let product_stations = alas_mass::product_stations::product_component_stations(
            &config, &design, &plane, &masses,
        )?;
        let old_stations = alas_mass::stations::component_stations_with_gear(
            &plane,
            &config.geometry,
            &req,
            &config.mass_model,
            cfg,
            &config.landing_gear,
        )?;
        let oew_kg: f64 = alas_mass::breakdown::OEW_KEYS
            .iter()
            .map(|key| masses.get(key).unwrap_or(0.0))
            .sum();
        let wing_moment_change: [f64; 3] = std::array::from_fn(|axis| {
            masses.wing
                * (product_stations.wing.position_m[axis] - old_stations.wing.position_m[axis])
        });
        let box_centroid = alas_mass::wing_centroid::sized_wingbox_centroid(
            &geometry,
            &linear.sizing,
            root.xyz_le[0],
        )?;
        let primary = alas_mass::wingbox_feedback::SizedWingboxMass::full_wing(
            2.0 * linear.sizing.total_mass_kg,
            box_centroid.xyz_m,
        );
        let secondary =
            alas_mass::wing_reconciliation::clean_sheet_secondary(&config, wing, primary)?;
        let inventory = alas_mass::wing_reconciliation::reconcile(&config, &design, &plane, None)?;
        rows.push(json!({
            "preset": name, "design_gross_mass_kg": req.mtow_kg,
            "semispan_m": geometry.semi_span, "spar_fractions": fractions,
            "fuel_full_wing_kg": 2.0 * trapezoid(&fuel, &stations),
            "semiwing_mounted_masses_kg": mounted.point_masses_kg,
            "materials": {"skin": skin, "web": web, "cap": cap, "rib": rib},
            "flops_complete_wing_kg": masses.wing, "wing_centroid_m": product_coords.wing,
            "ledger_wing_centroid_m": product_stations.wing.position_m,
            "old_ledger_wing_centroid_m": old_stations.wing.position_m,
            "wing_first_moment_change_kg_m": wing_moment_change,
            "oew_mass_kg": oew_kg,
            "oew_cg_shift_from_ledger_wing_m": wing_moment_change.map(|moment| moment / oew_kg),
            "box_centroid_m": box_centroid.xyz_m,
            "secondary_centroid_m": secondary.centroid_m,
            "secondary_remainder_kg": masses.wing - primary.mass_kg,
            "secondary_position_weights": secondary.items.iter().map(|item| json!({
                "name": item.name, "fraction": item.mass_kg / secondary.total_kg,
                "centroid_m": item.centroid_m, "source": item.source,
            })).collect::<Vec<_>>(),
            "structural_inventory_complete": inventory.inventory_complete(),
            "cap_material_source": alas_config::preset_structures::cap_material_source(name),
            "aircraft_cg_m": aircraft_cg,
            "physical_mass_kg": masses.as_pairs().iter().map(|(_, mass)| mass.max(0.0)).sum::<f64>(),
            "effective_bending_allowables_pa": {
                "skin": alas_struct::allowables::bending_allowable_pa(skin),
                "web": alas_struct::allowables::bending_allowable_pa(web),
                "cap": alas_struct::allowables::bending_allowable_pa(cap),
            },
            "strength_only": strength, "in_loop": in_loop,
            "candidate_gate": {"passes": assessment.passes(),
                "primary_mass_kg": assessment.primary_mass_kg,
                "mesh_primary_mass_kg": assessment.mesh_primary_mass_kg,
                "ultimate_tip_deflection_ratio": assessment.max_tip_deflection_ratio},
        }));
    }
    if let Some(parent) = Path::new(&output)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, serde_json::to_vec_pretty(&rows)?)?;
    if let (Some(path), Some(values)) = (historical_path, historical) {
        fs::write(path, serde_json::to_vec_pretty(&values)?)?;
    }
    Ok(())
}
