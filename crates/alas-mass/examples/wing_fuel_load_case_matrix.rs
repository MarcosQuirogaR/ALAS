// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the wing-bending design case is relieved by, per registered aircraft.
//!
//! One block per preset. It resolves the declared integral wing capacity, the
//! fuel the loading envelope guarantees is in the wings at the design gross
//! mass, and then sizes the same box three times on the same geometry and
//! materials — dry wing, design case, full tanks — so the sensitivity of the
//! sized box to the relieving fuel is measured rather than asserted.
//!
//! The A380-800 block additionally brackets the spanwise distribution of a
//! partial case: the declared shape scaled uniformly (what the product uses)
//! against an inboard-first and an outboard-first fill of the same mass.
//!
//! SI throughout: kg, m, N.m. Masses labelled `semi` are one semi-wing's.
//!
//! Run: `cargo run --release -p alas-mass --example wing_fuel_load_case_matrix`

#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig, DesignMode, DesignRequirements};
use alas_geom::builder::AircraftBuilder;
use alas_geom::wing_structure::WingStructureGeometry;
use alas_mass::wing_reconciliation::{declared_wing_fuel_case, DeclaredWingFuelCase};
use alas_struct::sizing::{
    box_chord_band, size_wingbox_with_wing_carried_mass, sizing_stations, WingboxSizing,
};

fn trapezoid(values: &[f64], stations: &[f64]) -> f64 {
    let mut acc = 0.0;
    for index in 0..values.len().saturating_sub(1) {
        acc += (stations[index + 1] - stations[index]) * (values[index + 1] + values[index]) / 2.0;
    }
    acc
}

/// Root bending moment of the sizing case, N.m, recovered from the reported
/// cap areas: `M0 = sum_spars A_cap0 F_allow h_eff0`, the equality the root is
/// sized to.
fn root_moment_nm(sizing: &WingboxSizing, f_allow_pa: f64) -> f64 {
    sizing
        .spars
        .iter()
        .map(|spar| spar.a_cap[0] * f_allow_pa * spar.h[0] * 0.85)
        .sum()
}

struct Probe {
    config: AlasConfig,
    requirements: DesignRequirements,
    wsg: WingStructureGeometry,
    stations: Vec<f64>,
    front: f64,
    rear: f64,
    engines: Vec<(f64, f64)>,
}

impl Probe {
    fn size(&self, fuel_kg_m: &[f64]) -> WingboxSizing {
        let structures = &self.config.structures;
        let material = |name: &str| {
            alas_config::materials::get(name).unwrap_or_else(|error| panic!("{error}"))
        };
        size_wingbox_with_wing_carried_mass(
            &self.wsg,
            structures,
            &self.requirements,
            material(&structures.skin_material),
            material(&structures.spar_web_material),
            material(&structures.spar_cap_material),
            material(&structures.rib_material),
            Some(fuel_kg_m),
            &self.engines,
        )
    }

    fn cap_allowable_pa(&self) -> f64 {
        alas_config::materials::get(&self.config.structures.spar_cap_material)
            .unwrap_or_else(|error| panic!("{error}"))
            .f_allow_pa
    }

    fn report(&self, label: &str, fuel_kg_m: &[f64], reference_kg: f64) {
        let sized = self.size(fuel_kg_m);
        let semi_fuel = trapezoid(fuel_kg_m, &self.stations);
        let moment = root_moment_nm(&sized, self.cap_allowable_pa());
        let delta = if reference_kg > 0.0 {
            format!(
                "{:+.2} %",
                100.0 * (sized.total_mass_kg / reference_kg - 1.0)
            )
        } else {
            "reference".to_owned()
        };
        println!(
            "    {label:<22} semi_fuel={semi_fuel:>10.1} kg  semi_box={:>9.1} kg  \
             complete_box={:>10.1} kg  M_root={moment:>11.4e} N.m  case={:<10} \
             min_MS={:+.3e}  {delta}",
            sized.total_mass_kg,
            2.0 * sized.total_mass_kg,
            sized.sizing_load_case,
            sized.minimum_margin_of_safety(),
        );
    }
}

fn probe(preset_name: &str) -> Option<(Probe, DeclaredWingFuelCase, alas_config::DesignVector)> {
    let preset = presets::get(preset_name).ok()?;
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset_name })).ok()?;
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    let design = preset.design_vector;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .ok()?;
    let wing = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .or_else(|| plane.wings.first())?;
    let (fractions, full_span) = config.structures.resolved_spars();
    let wsg = WingStructureGeometry::new(
        &design,
        &config.geometry.wing,
        &wing.xsecs.first()?.airfoil,
        &wing.xsecs.last()?.airfoil,
        &fractions,
        Some(&full_span),
    )
    .ok()?;
    let stations = sizing_stations(&wsg, &config.structures);
    let (front, rear) = box_chord_band(&wsg);
    // The same design-gross-mass requirements `sized_primary_wing` builds the
    // loads from, so the probe and the product read one design weight.
    let mut requirements = config.requirements.clone();
    requirements.mtow_kg = alas_mass::wing_reconciliation::design_gross_mass_kg(&config);
    let engines = alas_struct::loads::engine_point_loads_n(
        &config.geometry.engine,
        &config.mass_model,
        &requirements,
    );
    let case = declared_wing_fuel_case(
        &config,
        &design,
        &requirements,
        &wsg,
        &stations,
        front,
        rear,
    )?;
    Some((
        Probe {
            config,
            requirements,
            wsg,
            stations,
            front,
            rear,
            engines,
        },
        case,
        design,
    ))
}

/// The same total mass packed from the inboard edge of the declared wet band
/// outward (`inboard`) or from the outboard edge inward, at the declared
/// running-mass shape's own local density ceiling.
fn packed(shape: &[f64], stations: &[f64], total_kg: f64, inboard: bool) -> Vec<f64> {
    let mut packedv = vec![0.0; shape.len()];
    let order: Vec<usize> = if inboard {
        (0..shape.len()).collect()
    } else {
        (0..shape.len()).rev().collect()
    };
    let mut remaining = total_kg;
    for &index in &order {
        if shape[index] <= 0.0 {
            continue;
        }
        // Mass this station's own trapezoid half-segments would carry at the
        // declared density.
        let mut width = 0.0;
        if index > 0 {
            width += 0.5 * (stations[index] - stations[index - 1]);
        }
        if index + 1 < stations.len() {
            width += 0.5 * (stations[index + 1] - stations[index]);
        }
        let capacity = shape[index] * width;
        if capacity <= 0.0 {
            continue;
        }
        let take = capacity.min(remaining);
        packedv[index] = shape[index] * (take / capacity);
        remaining -= take;
        if remaining <= 0.0 {
            break;
        }
    }
    packedv
}

fn main() {
    println!("Wing-bending design case: declared capacity against the fuel the loading");
    println!("envelope guarantees at the design gross mass. SI: kg, m, N.m.");
    for preset in presets::registry() {
        println!();
        println!("== {} ==", preset.name);
        let Some((probe, case, _design)) = probe(preset.name) else {
            println!("  no declared integral wing cell, or geometry did not build");
            continue;
        };
        let mzfw = case
            .max_zero_fuel_mass_kg
            .map_or("undeclared".to_owned(), |kg| format!("{kg:.0} kg"));
        println!(
            "  DG={:.0} kg  MZFW={mzfw}  wing_capacity={:.1} kg  design_case={:.1} kg  \
             factor={:.4}  zero_fuel_limited={}",
            case.design_gross_mass_kg,
            case.capacity_kg,
            case.design_case_kg,
            case.design_case_kg / case.capacity_kg,
            case.zero_fuel_limited(),
        );
        let dry = vec![0.0; probe.stations.len()];
        let full = {
            let mut requirements = probe.requirements.clone();
            // The same aircraft at a mass its wings cannot be avoided at: the
            // case reverts to the full declared capacity on the same cells.
            requirements.mtow_kg = case
                .max_zero_fuel_mass_kg
                .unwrap_or(case.design_gross_mass_kg)
                + 2.0 * case.capacity_kg;
            declared_wing_fuel_case(
                &probe.config,
                &_design,
                &requirements,
                &probe.wsg,
                &probe.stations,
                probe.front,
                probe.rear,
            )
            .map_or_else(|| case.running_mass_kg_m.clone(), |c| c.running_mass_kg_m)
        };
        let geometric = alas_struct::tanks::integral_fuel_running_mass_kg_m(
            &probe.wsg,
            &probe.stations,
            probe.front,
            probe.rear,
        );

        let reference = probe.size(&case.running_mass_kg_m).total_mass_kg;
        probe.report("design case (product)", &case.running_mass_kg_m, 0.0);
        probe.report("full tanks (previous)", &full, reference);
        probe.report("dry wing", &dry, reference);
        probe.report("geometric estimate", &geometric, reference);

        if case.zero_fuel_limited() {
            let inboard = packed(&full, &probe.stations, 0.5 * case.design_case_kg, true);
            let outboard = packed(&full, &probe.stations, 0.5 * case.design_case_kg, false);
            println!("    -- distribution bracket for the same design-case mass --");
            probe.report("  inboard-first fill", &inboard, reference);
            probe.report("  outboard-first fill", &outboard, reference);
        }
    }
}
