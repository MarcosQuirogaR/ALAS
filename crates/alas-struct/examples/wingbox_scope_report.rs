// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What each registered aircraft's sized wingbox was sized to, and what it was
//! not sized to.
//!
//! One block per aircraft, sized twice on the same geometry and materials: once
//! on the enclosed-box-volume fuel estimate (the clean-sheet default), and once
//! on the aircraft's own declared integral wing capacity bounded by its declared
//! maximum zero-fuel mass. Each block prints the resolved
//! `alas_struct::scope::SizingScope`: the manoeuvre case that sized the box,
//! whether the relieving fuel was bounded or assumed, whether the relieved-load
//! fixed point settled, and every typed gap the solve declares.
//!
//! Nothing here estimates a missing quantity. An aircraft that declares no
//! zero-fuel limit is reported as sized on a full-tank assumption, and an
//! installation whose propeller and nacelle this crate cannot reach is reported
//! as relieved by neither.
//!
//! SI throughout: kg, m, N.m.
//!
//! Run: `cargo run --release -p alas-struct --example wingbox_scope_report`

#![allow(clippy::print_stdout)]

use alas_config::materials;
use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_geom::wing_structure::WingStructureGeometry;
use alas_struct::scope::{wing_mounted_relief, SizingScope, WingFuelDesignCase};
use alas_struct::sizing::{
    box_chord_band, size_wingbox_with_scope, sizing_stations, SizedWingbox, WingFuelRelief,
};
use alas_struct::tanks;

/// Trapezoidal integral of `values` over `stations`.
fn trapezoid(values: &[f64], stations: &[f64]) -> f64 {
    let mut acc = 0.0;
    for index in 0..values.len().saturating_sub(1) {
        acc += (stations[index + 1] - stations[index]) * (values[index + 1] + values[index]) / 2.0;
    }
    acc
}

/// The declared integral wing fuel, as a running mass on one semi-wing and a
/// whole-aircraft capacity.
///
/// Each cell's published volume is for both wings, so half of it is carried by
/// the modelled semi-wing; within the cell it is distributed in proportion to
/// the local enclosed box section, which is where the fuel physically is.
/// `None` when the aircraft declares no integral wing cell with a published
/// volume.
fn declared_wing_fuel(
    config: &AlasConfig,
    wsg: &WingStructureGeometry,
    y: &[f64],
    front: f64,
    rear: f64,
) -> Option<(Vec<f64>, f64)> {
    let layout = &config.fuel_tanks;
    // Jet A-1 at the density every registered aircraft's reference data states
    // for its published capacities.
    let density_kg_l = 0.8;
    let mut running = vec![0.0; y.len()];
    let mut capacity_kg = 0.0;
    for cell in [&layout.inner_wing, &layout.mid_wing, &layout.outer_wing] {
        if !cell.enabled {
            continue;
        }
        let Some(volume_l) = cell.published_usable_volume_l else {
            continue;
        };
        let cell_kg = volume_l * density_kg_l;
        let area: Vec<f64> = y
            .iter()
            .map(|&station| {
                let eta = station / wsg.semi_span;
                if eta < cell.span_start_fraction || eta > cell.span_end_fraction {
                    0.0
                } else {
                    tanks::box_section_area_m2(wsg, eta, front, rear)
                }
            })
            .collect();
        let integral = trapezoid(&area, y);
        if !integral.is_finite() || integral <= 0.0 {
            continue;
        }
        capacity_kg += cell_kg;
        for (mass, &a) in running.iter_mut().zip(&area) {
            *mass += 0.5 * cell_kg * a / integral;
        }
    }
    (capacity_kg > 0.0).then_some((running, capacity_kg))
}

/// The aircraft's declared maximum zero-fuel mass, or `None` where the registry
/// publishes none. There is no substitute and none is invented.
fn declared_mzfw_kg(name: &str) -> Option<f64> {
    presets::get(name).ok()?.reference.mzfw_kg
}

fn print_scope(label: &str, sized: &SizedWingbox) {
    let scope: &SizingScope = &sized.scope;
    println!(
        "  {label}: semi_box_kg={:.1} case={} n_ult={:.3} DG={:.0} kg",
        sized.sizing.total_mass_kg,
        scope.sizing_load_case,
        scope.ultimate_load_factor,
        scope.design_gross_mass_kg
    );
    println!(
        "    wing_fuel={:?} design_case_kg={:.1} bounded={} partial_fill={}",
        FuelKind::of(&scope.wing_fuel),
        scope.wing_fuel.design_case_kg(),
        scope.envelope_is_bounded(),
        scope.wing_fuel.is_partial_fill()
    );
    println!(
        "    relief_convergence={:?} unconservative_gap={}",
        scope.relief_convergence,
        scope.has_unconservative_gap()
    );
    for gap in scope.not_available() {
        println!("    NotAvailable[{:?}] {}", gap.direction, gap.quantity);
    }
}

/// The variant name alone, so the block stays readable without reprinting every
/// field the line above already carries.
#[derive(Debug)]
enum FuelKind {
    NoDeclaredCapacity,
    EnclosedBoxVolumeEstimate,
    TankLimited,
    ZeroFuelLimited,
    FullTanksAssumed,
}

impl FuelKind {
    fn of(case: &WingFuelDesignCase) -> Self {
        match case {
            WingFuelDesignCase::NoDeclaredCapacity => Self::NoDeclaredCapacity,
            WingFuelDesignCase::EnclosedBoxVolumeEstimate { .. } => Self::EnclosedBoxVolumeEstimate,
            WingFuelDesignCase::TankLimited { .. } => Self::TankLimited,
            WingFuelDesignCase::ZeroFuelLimited { .. } => Self::ZeroFuelLimited,
            WingFuelDesignCase::FullTanksAssumed { .. } => Self::FullTanksAssumed,
        }
    }
}

fn main() {
    for preset in presets::registry() {
        let name = preset.name;
        let Ok(config) = AlasConfig::from_value(&serde_json::json!({ "preset": name })) else {
            println!("{name}: configuration did not load");
            continue;
        };
        let design = &preset.design_vector;
        let Ok(plane) =
            AircraftBuilder::new(Some(config.geometry.clone())).build(Some(design), false)
        else {
            println!("{name}: geometry did not build");
            continue;
        };
        let Some(wing) = plane.wings.iter().find(|wing| wing.name == "Main Wing") else {
            println!("{name}: no main wing");
            continue;
        };
        let (Some(root), Some(tip)) = (wing.xsecs.first(), wing.xsecs.last()) else {
            continue;
        };
        let structures = &config.structures;
        let (fractions, full_span) = structures.resolved_spars();
        let Ok(wsg) = WingStructureGeometry::new(
            design,
            &config.geometry.wing,
            &root.airfoil,
            &tip.airfoil,
            &fractions,
            Some(&full_span),
        ) else {
            println!("{name}: wing structure did not build");
            continue;
        };
        let (Ok(skin), Ok(web), Ok(cap), Ok(rib)) = (
            materials::get(&structures.skin_material),
            materials::get(&structures.spar_web_material),
            materials::get(&structures.spar_cap_material),
            materials::get(&structures.rib_material),
        ) else {
            println!("{name}: materials did not resolve");
            continue;
        };
        let requirements = &config.requirements;
        let relief = wing_mounted_relief(&config.geometry.engine, &config.mass_model, requirements);
        let stations = sizing_stations(&wsg, structures);
        let (front, rear) = box_chord_band(&wsg);

        let mzfw = declared_mzfw_kg(name);
        println!("== {name} ==");
        println!(
            "  wing_mounted_semi={:?} total={:.1} kg  declared_mzfw_kg={:?}",
            relief.point_masses_kg,
            relief.total_mass_kg(),
            mzfw
        );

        let size = |fuel: &WingFuelRelief<'_>| {
            size_wingbox_with_scope(
                &wsg,
                structures,
                requirements,
                skin,
                web,
                cap,
                rib,
                fuel,
                &relief,
            )
        };

        print_scope("geometric", &size(&WingFuelRelief::EnclosedBoxVolume));

        match declared_wing_fuel(&config, &wsg, &stations, front, rear) {
            Some((running, capacity_kg)) => {
                let case = WingFuelDesignCase::declared(capacity_kg, requirements.mtow_kg, mzfw);
                // The declared spanwise shape, scaled to the design case. The
                // factor is exactly one whenever the wings cannot be avoided.
                let factor = if capacity_kg > 0.0 {
                    case.design_case_kg() / capacity_kg
                } else {
                    0.0
                };
                let scaled: Vec<f64> = running.iter().map(|&m| m * factor).collect();
                println!("  declared_capacity_kg={capacity_kg:.1} scale_to_case={factor:.4}");
                print_scope(
                    "declared ",
                    &size(&WingFuelRelief::Declared {
                        running_mass_kg_m: &scaled,
                        design_case: case,
                    }),
                );
            }
            None => println!("  declared : no integral wing cell publishes a usable volume"),
        }
    }
}
