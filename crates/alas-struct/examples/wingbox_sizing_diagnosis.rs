// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Why the strength-sized wingbox outweighs the empirical wing it belongs to.
//!
//! `structural_inventory_unverified` is raised whenever the strength-sized
//! *complete* box (two semi-wings) exceeds the frozen empirical complete wing.
//! This probe prints, per registered aircraft, the inputs that decide that
//! comparison: the sizing load case and its root shear/moment, the root cap
//! section, and the four semi-wing mass components, together with the two
//! chordwise-extent counterfactuals that identify where the box mass comes
//! from.
//!
//! Every quantity is SI: metres, kilograms, newtons, newton-metres, pascals.
//!
//! Run: `cargo run --release -p alas-struct --example wingbox_sizing_diagnosis`

#![allow(clippy::print_stdout)]

use alas_config::materials;
use alas_config::{presets, AlasConfig, StructuresConfig};
use alas_geom::builder::AircraftBuilder;
use alas_geom::wing_structure::WingStructureGeometry;
use alas_struct::loads::{self, WingInertiaRelief};
use alas_struct::sizing::{
    box_chord_band, box_running_mass_kg_m, size_wingbox, size_wingbox_with_wing_carried_mass,
};
use alas_struct::tanks;

/// Trapezoidal integral of `y` over `x`.
fn trapezoid(y: &[f64], x: &[f64]) -> f64 {
    let mut acc = 0.0;
    for i in 0..y.len().saturating_sub(1) {
        acc += (x[i + 1] - x[i]) * (y[i + 1] + y[i]) / 2.0;
    }
    acc
}

/// The running mass of the aircraft's declared integral wing tanks, kg/m on one
/// semi-wing, from their manufacturer-published usable volumes.
///
/// Each cell's published volume is for both wings, so half of it is carried by
/// the semi-wing modelled here. Within the cell it is distributed in proportion
/// to the local enclosed box section, which is where the fuel physically is.
/// `None` when the aircraft declares no integral wing cell with a published
/// volume, in which case the geometric estimate stands.
fn published_wing_fuel_running_mass_kg_m(
    config: &AlasConfig,
    wsg: &WingStructureGeometry,
    y: &[f64],
    front: f64,
    rear: f64,
) -> Option<Vec<f64>> {
    let layout = &config.fuel_tanks;
    let cells = [&layout.inner_wing, &layout.mid_wing, &layout.outer_wing];
    // Jet A-1 at the density every registered aircraft's reference data states
    // for its published capacities.
    let density_kg_l = 0.8;
    let mut running = vec![0.0; y.len()];
    let mut any = false;
    for cell in cells {
        if !cell.enabled {
            continue;
        }
        let Some(volume_l) = cell.published_usable_volume_l else {
            continue;
        };
        any = true;
        let semi_mass_kg = 0.5 * volume_l * density_kg_l;
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
        if integral <= 0.0 {
            continue;
        }
        for (mass, &a) in running.iter_mut().zip(&area) {
            *mass += semi_mass_kg * a / integral;
        }
    }
    any.then_some(running)
}

fn main() {
    for preset in presets::registry() {
        let Ok(config) = AlasConfig::from_value(&serde_json::json!({"preset": preset.name})) else {
            println!("{}: configuration did not load", preset.name);
            continue;
        };
        let dv = &preset.design_vector;
        let Ok(plane) = AircraftBuilder::new(Some(config.geometry.clone())).build(Some(dv), false)
        else {
            println!("{}: geometry did not build", preset.name);
            continue;
        };
        let Some(wing) = plane.wings.iter().find(|w| w.name == "Main Wing") else {
            println!("{}: no main wing", preset.name);
            continue;
        };
        let (Some(root), Some(tip)) = (wing.xsecs.first(), wing.xsecs.last()) else {
            continue;
        };
        let structures: &StructuresConfig = &config.structures;
        let (spar_fractions, spar_full_span) = structures.resolved_spars();
        let Ok(wsg) = WingStructureGeometry::new(
            dv,
            &config.geometry.wing,
            &root.airfoil,
            &tip.airfoil,
            &spar_fractions,
            Some(&spar_full_span),
        ) else {
            continue;
        };
        let Ok(skin) = materials::get(&structures.skin_material) else {
            continue;
        };
        let Ok(web) = materials::get(&structures.spar_web_material) else {
            continue;
        };
        let Ok(cap) = materials::get(&structures.spar_cap_material) else {
            continue;
        };
        let Ok(rib) = materials::get(&structures.rib_material) else {
            continue;
        };

        let req = &config.requirements;
        let sizing = size_wingbox(&wsg, structures, req, skin, web, cap, rib);

        // Sizing loads, recomputed here on the same grid and with the same
        // relief the sizer converged to.
        let cases = loads::load_cases(req, structures.additional_safety_factor);
        let y = &sizing.y_stations;
        let (front, rear) = box_chord_band(&wsg);
        let box_width_fraction = rear - front;
        let fuel = tanks::integral_fuel_running_mass_kg_m(&wsg, y, front, rear);
        let structure = box_running_mass_kg_m(&sizing, skin, web, cap, front, rear);
        let relieved: Vec<f64> = fuel.iter().zip(&structure).map(|(&f, &s)| f + s).collect();
        let relief = WingInertiaRelief {
            running_mass_kg_m: relieved.clone(),
            point_masses_kg: Vec::new(),
        };
        let fuel_semi_kg = trapezoid(&fuel, y);
        let structure_semi_kg = trapezoid(&structure, y);

        let mut worst = (0usize, -1.0);
        for (index, case) in cases.iter().enumerate() {
            let q = loads::elliptic_distributed_load(y, wsg.semi_span, case.total_force_n);
            let q_net =
                loads::net_distributed_load(&q, case.load_factor, req.gravity_m_s2, &relieved);
            let (_, m) = loads::cantilever_shear_moment(y, &q_net);
            if m[0].abs() > worst.1 {
                worst = (index, m[0].abs());
            }
        }
        let case = cases[worst.0];
        let q_aero = loads::elliptic_distributed_load(y, wsg.semi_span, case.total_force_n);
        let (v_unrelieved, m_unrelieved) = loads::cantilever_shear_moment(y, &q_aero);
        let q = loads::net_distributed_load(&q_aero, case.load_factor, req.gravity_m_s2, &relieved);
        let (v, m) = loads::cantilever_shear_moment(y, &q);
        let _ = &relief;

        let semi = sizing.total_mass_kg;
        let complete = 2.0 * semi;
        println!("== {} ==", preset.name);
        println!(
            "  mtow_kg={:.0} n_ult={:.3} case={} semi_span_m={:.3} c_root_m={:.3} \
             spars={:?} box_width_frac={:.3}",
            req.mtow_kg,
            case.load_factor,
            case.name,
            wsg.semi_span,
            wsg.c_root,
            wsg.spar_fracs,
            box_width_fraction
        );
        println!(
            "  root_shear_N={:.4e} root_moment_Nm={:.4e} cap_f_allow_MPa={:.0} \
             cap_rho={:.0} skin_rho={:.0}",
            v[0],
            m[0],
            cap.f_allow_pa / 1.0e6,
            cap.rho_kg_m3,
            skin.rho_kg_m3
        );
        println!(
            "  unrelieved: shear_N={:.4e} moment_Nm={:.4e}  relief: \
             fuel_semi_kg={:.0} structure_semi_kg={:.0} moment_ratio={:.3}",
            v_unrelieved[0],
            m_unrelieved[0],
            fuel_semi_kg,
            structure_semi_kg,
            m[0] / m_unrelieved[0]
        );
        let root_cap_area: f64 = sizing.spars.iter().map(|s| s.a_cap[0]).sum();
        let root_h: Vec<f64> = sizing.spars.iter().map(|s| s.h[0]).collect();
        println!(
            "  root_h_m={:?} root_one_flange_cap_area_m2={:.5} num_ribs={} \
             rib_spacing_m={:.3} t_skin_m={:.4}",
            root_h
                .iter()
                .map(|h| (h * 1000.0).round() / 1000.0)
                .collect::<Vec<_>>(),
            root_cap_area,
            sizing.num_ribs,
            sizing.rib_spacing_m,
            sizing.t_skin
        );
        println!(
            "  semi_box_kg total={:.1}  caps={:.1} webs={:.1} skin={:.1} ribs={:.1}",
            semi,
            sizing.mass_breakdown_kg.spar_caps,
            sizing.mass_breakdown_kg.spar_webs,
            sizing.mass_breakdown_kg.skin,
            sizing.mass_breakdown_kg.ribs
        );
        println!("  complete_box_kg={complete:.1}");

        // What the queued call-site wiring would buy: the aircraft's own
        // published wing-tank capacity in place of the geometric estimate, and
        // its wing-mounted powerplant as relieving point masses.
        let engines = loads::engine_point_loads_n(&config.geometry.engine, &config.mass_model, req);
        let declared = published_wing_fuel_running_mass_kg_m(&config, &wsg, y, front, rear);
        let declared_semi_kg = declared.as_ref().map(|f| trapezoid(f, y));
        let wired = size_wingbox_with_wing_carried_mass(
            &wsg,
            structures,
            req,
            skin,
            web,
            cap,
            rib,
            declared.as_deref(),
            &engines,
        );
        println!(
            "  queued wiring: engines_semi={:?} published_fuel_semi_kg={:?} \
             -> semi={:.1} complete={:.1}",
            engines
                .iter()
                .map(|&(y_m, m)| ((y_m * 100.0).round() / 100.0, m.round()))
                .collect::<Vec<_>>(),
            declared_semi_kg.map(|v| v.round()),
            wired.total_mass_kg,
            2.0 * wired.total_mass_kg
        );
    }
}
