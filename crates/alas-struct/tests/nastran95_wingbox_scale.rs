// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Scale sensitivity of NASTRAN-95 on a product wingbox mesh.
//!
//! The closed-form box benchmark (`nastran95_closed_form.rs`) is exactly
//! scale-invariant, so the 25 percent stress swing the A320 wingbox showed
//! between deck length units of 10 and 100 per metre must come from something
//! the benchmark does not contain. This harness solves one product wingbox
//! mesh at each scale and attributes the stress differences to element zones.
//!
//! The solver comes from `ALAS_NASTRAN95_DIR` / `ALAS_NASTRAN95_RUNTIME`; when
//! unset the test skips, loudly. `ALAS_NASTRAN95_BENCH_DIR` (default
//! `C:/n95bench`) is the short work directory. Each solve takes about a minute.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

mod support;

use std::collections::BTreeSet;
use std::path::PathBuf;

use alas_config::{materials, DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_struct::nastran95::{
    build_static_deck_product_at_length_scale, displacement_of, read_displacement_tables,
    read_static_shell_stress_print, run_nastran95, Nastran95Solver, RunOutcome,
};
use alas_struct::{loads, mesh, sizing};

const SCALES: [f64; 4] = [1.0, 10.0, 100.0, 1000.0];

fn solver() -> Option<Nastran95Solver> {
    let dir = PathBuf::from(std::env::var("ALAS_NASTRAN95_DIR").ok()?);
    let runtime = std::env::var_os("ALAS_NASTRAN95_RUNTIME").map(PathBuf::from);
    let stage = std::env::var_os("ALAS_NASTRAN95_RF_STAGE")
        .map_or_else(|| PathBuf::from("C:/n95bench-rf"), PathBuf::from);
    Nastran95Solver::from_paths(&dir, runtime.as_deref(), Some(&stage), None)
}

/// Per scale and case: tip deflection (m) and every element's two-fiber maximum
/// von Mises (Pa), keyed by element id.
struct Solve {
    tip_m: Vec<f64>,
    von_mises_pa: Vec<std::collections::BTreeMap<i64, f64>>,
}

#[test]
fn wingbox_stress_scale_sensitivity_is_attributed_to_element_zones() {
    let Some(solver) = solver() else {
        println!("skipped: ALAS_NASTRAN95_DIR is not set or the solver is incomplete");
        return;
    };
    let config = StructuresConfig {
        num_ribs_override: Some(12),
        ..Default::default()
    };
    let requirements = DesignRequirements::default();
    let engine = EngineConfig {
        spanwise_positions_m: vec![10.234, -10.234],
        ..Default::default()
    };
    let mass_config = MassModelConfig::default();
    let material = materials::get("Al 7075-T6").unwrap();
    let geometry = support::build_geometry(&[0.15, 0.65], &[true, true]);
    let wingbox = sizing::size_wingbox_reference_compatibility(
        &geometry,
        &config,
        &requirements,
        material,
        material,
        material,
        material,
    );
    let (deck, _, node_index) = mesh::build_wing_mesh_bdf_product(
        &geometry,
        &wingbox,
        &config,
        &engine,
        &mass_config,
        &requirements,
        material,
        material,
        material,
        material,
    )
    .unwrap();
    let cases = loads::load_cases(&requirements, config.additional_safety_factor);
    let excluded: BTreeSet<i64> =
        deck.load_introduction_elements(&node_index.engine_nids, mesh::LOAD_INTRODUCTION_LAYERS);
    println!(
        "mesh: {} quads, {} excluded load-introduction elements",
        deck.quads().len(),
        excluded.len()
    );
    let base = PathBuf::from(
        std::env::var("ALAS_NASTRAN95_BENCH_DIR").unwrap_or_else(|_| "C:/n95bench".to_owned()),
    );
    for translations_only in [false] {
        println!(
            "=== RBE3 independent components: {} ===",
            if translations_only {
                "123 (translations only)"
            } else {
                "123 (product, translations only)"
            }
        );
        let mut solves = Vec::new();
        for &scale in &SCALES {
            let text = build_static_deck_product_at_length_scale(
                &deck,
                &node_index,
                &requirements,
                &config,
                scale,
            );
            let text = if translations_only {
                let reduced = translations_only_rbe3(&text);
                assert_ne!(reduced, text, "no CRBE3 card was reduced");
                reduced
            } else {
                text
            };
            let work = base.join(format!("W{scale:.0}"));
            let RunOutcome::Print(print) = run_nastran95(&solver, &text, &work, 1800.0) else {
                panic!("L = {scale}: the solve failed");
            };
            let tables = read_displacement_tables(&print);
            let tip_m = (0..cases.len())
                .map(|i| displacement_of(&tables, i, node_index.tip_nid).unwrap()[2] / scale)
                .collect::<Vec<_>>();
            let stress = read_static_shell_stress_print(&print, &deck, &cases);
            assert!(stress.error.is_none(), "{:?}", stress.error);
            let to_pa = scale * scale; // printed stress is in units of 1/L^2 Pa
            let von_mises_pa = stress
                .cases
                .iter()
                .map(|case| {
                    let mut by_element = std::collections::BTreeMap::new();
                    for sample in &case.samples {
                        let entry = by_element.entry(sample.element_id).or_insert(0.0_f64);
                        *entry = entry.max(sample.von_mises_pa * to_pa);
                    }
                    by_element
                })
                .collect();
            solves.push(Solve {
                tip_m,
                von_mises_pa,
            });
        }
        // The product scale (decimetre) is the reference the others are compared with.
        let reference = SCALES.iter().position(|&s| s == 10.0).unwrap();
        for (case_index, case) in cases.iter().enumerate() {
            println!("-- {} --", case.name);
            println!("L/m   tip[m]   max_all[MPa]  max_assessed[MPa]  worst |dVM| elem: id zone ratio-to-L10");
            let ref_vm = &solves[reference].von_mises_pa[case_index];
            for (si, &scale) in SCALES.iter().enumerate() {
                let vm = &solves[si].von_mises_pa[case_index];
                let max_all = vm.values().copied().fold(0.0_f64, f64::max);
                let max_assessed = vm
                    .iter()
                    .filter(|(eid, _)| !excluded.contains(eid))
                    .map(|(_, v)| *v)
                    .fold(0.0_f64, f64::max);
                // Largest relative change among elements stressed above 20 percent
                // of the reference maximum.
                let ref_max = ref_vm.values().copied().fold(0.0_f64, f64::max);
                let mut worst = (0_i64, 1.0_f64);
                for (eid, v) in vm {
                    let r = ref_vm[eid];
                    if r > 0.2 * ref_max && (v / r - 1.0).abs() > (worst.1 - 1.0).abs() {
                        worst = (*eid, v / r);
                    }
                }
                println!(
                    "{:<5} {:7.4} {:12.2} {:18.2}   {} {} {:.4}",
                    scale,
                    solves[si].tip_m[case_index],
                    max_all / 1e6,
                    max_assessed / 1e6,
                    worst.0,
                    if excluded.contains(&worst.0) {
                        "load-intro"
                    } else {
                        "assessed"
                    },
                    worst.1
                );
            }
        }
    }
}

/// The deck with every `CRBE3` independent-component group reduced from
/// `123456` to `123`: translations only. Fixed-field columns 49-56 hold the
/// independent components (name, eid, blank, refgrid, refc, weight, comp).
fn translations_only_rbe3(deck: &str) -> String {
    deck.lines()
        .map(|line| {
            if line.starts_with("CRBE3") && line.len() >= 56 && &line[48..54] == "123456" {
                format!("{}{:<8}{}", &line[..48], "123", &line[56..])
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
