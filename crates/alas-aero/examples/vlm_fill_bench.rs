// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Timing and bit-digest harness for the rotating-stabilizer lattice update.
//!
//! Builds a wing + horizontal tail + fin lattice of about 700 panels, then
//! does what a trim and a drag table do: rotate the stabilizer through a
//! sequence of incidences, assemble through one `VlmGeometryCache` each time,
//! and solve. Prints the wall time per assembly (split by the stage profile)
//! and an FNV-1a digest of every solve result, so two builds can be compared
//! bit for bit.
//!
//! Usage: vlm_fill_bench [ASSEMBLIES] [CHORDWISE] [SPANWISE] [compare]
//!
//! `compare` runs every step through two caches, the ordinary LU and the
//! Schur update (`vlm::speed`), and prints the largest relative difference
//! of the coefficients, circulations and Trefftz induced drag.

#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::{VlmGeometryCache, VlmSystem};
use alas_atmo::Atmosphere;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{Wing, WingXSec};
use alas_math::stage_profile;
use std::time::Instant;

fn airplane() -> Airplane {
    let section =
        |xyz, chord| WingXSec::new(xyz, chord, 0.0, Airfoil::from_name("naca0012").unwrap());
    let main = Wing::new(
        "Main Wing",
        (0..=6)
            .map(|k| {
                let y = f64::from(k) * 5.0;
                section(
                    [1.2 * y.min(30.0) / 6.0, y, 0.1 * y],
                    6.0 - 0.6 * f64::from(k),
                )
            })
            .collect(),
        true,
    );
    let htail = Wing::new(
        "Horizontal Stabilizer",
        vec![
            section([20.0, 0.0, 1.0], 3.0),
            section([23.0, 6.0, 1.2], 1.6),
        ],
        true,
    );
    let fin = Wing::new(
        "Vertical Stabilizer",
        vec![
            section([19.0, 0.0, 1.0], 4.0),
            section([23.0, 0.0, 6.0], 2.0),
        ],
        false,
    );
    Airplane {
        name: "fill bench".into(),
        xyz_ref: [6.0, 0.0, 0.0],
        s_ref: main.reference_area(),
        b_ref: main.reference_span(),
        c_ref: main.mean_aerodynamic_chord(),
        wings: vec![main, htail, fin],
        fuselages: vec![],
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let count: usize = args.get(1).map_or(60, |s| s.parse().unwrap());
    let chordwise: usize = args.get(2).map_or(10, |s| s.parse().unwrap());
    let spanwise: usize = args.get(3).map_or(6, |s| s.parse().unwrap());
    if args.get(4).map(String::as_str) == Some("compare") {
        compare(count, chordwise, spanwise);
        return;
    }
    stage_profile::enable();
    let mut plane = airplane();
    let mut cache = VlmGeometryCache::default();
    let mut digest: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |text: &str| {
        for &byte in text.as_bytes() {
            digest = (digest ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    // Warm-up assembly: the cold build is not what is being measured.
    VlmSystem::assemble_cached(&plane, spanwise, chordwise, &mut cache).unwrap();
    stage_profile::reset();
    let start = Instant::now();
    let mut panels = 0;
    for step in 0..count {
        let incidence = -6.0 + 12.0 * (step as f64 * 0.618_033_988_75).fract();
        for section in &mut plane.wings[1].xsecs {
            section.twist = incidence;
        }
        plane.xyz_ref[0] = 6.0 + 0.003 * step as f64;
        let system = VlmSystem::assemble_cached(&plane, spanwise, chordwise, &mut cache).unwrap();
        panels = system.panel_count();
        for alpha in [0.5, 4.0] {
            let point =
                OperatingPoint::new(Atmosphere::new(9000.0), 200.0, alpha, 1.0, 0.0, 0.0, 0.0);
            let result = system.solve(&point).unwrap();
            feed(&format!("{result:?}"));
            feed(&format!(
                "{:?}",
                system.trefftz_induced_drag_coefficient(&result, &point)
            ));
        }
    }
    let wall = start.elapsed().as_secs_f64();
    let report = stage_profile::report();
    println!("panels {panels} assemblies {count}");
    println!(
        "wall per assembly+2 solves {:.3} ms",
        1e3 * wall / count as f64
    );
    for (name, seconds, _, _) in &report.stages {
        if *seconds > 0.0 {
            println!(
                "  {name:<24} {:.3} ms/assembly",
                1e3 * seconds / count as f64
            );
        }
    }
    println!("digest {digest:016x}");
}

/// Largest relative difference between the ordinary-LU and Schur results.
fn compare(count: usize, chordwise: usize, spanwise: usize) {
    let mut plane = airplane();
    let mut full_cache = VlmGeometryCache::with_schur_update(false);
    let mut block_cache = VlmGeometryCache::with_schur_update(true);
    let mut worst = [0.0_f64; 8];
    let relative = |a: f64, b: f64| (a - b).abs() / b.abs().max(1.0e-12);
    for step in 0..count {
        let incidence = -6.0 + 12.0 * (step as f64 * 0.618_033_988_75).fract();
        for section in &mut plane.wings[1].xsecs {
            section.twist = incidence;
        }
        let full =
            VlmSystem::assemble_cached(&plane, spanwise, chordwise, &mut full_cache).unwrap();
        let block =
            VlmSystem::assemble_cached(&plane, spanwise, chordwise, &mut block_cache).unwrap();
        for alpha in [0.5, 4.0] {
            let point =
                OperatingPoint::new(Atmosphere::new(9000.0), 200.0, alpha, 1.0, 0.0, 0.0, 0.0);
            let (a, b) = (full.solve(&point).unwrap(), block.solve(&point).unwrap());
            let scale = a
                .vortex_strengths
                .iter()
                .fold(0.0_f64, |m, g| m.max(g.abs()));
            let gamma = a
                .vortex_strengths
                .iter()
                .zip(&b.vortex_strengths)
                .fold(0.0_f64, |m, (x, y)| m.max((x - y).abs() / scale));
            let cdi_a = full.trefftz_induced_drag_coefficient(&a, &point).unwrap();
            let cdi_b = block.trefftz_induced_drag_coefficient(&b, &point).unwrap();
            let values = [
                relative(b.cl_lift, a.cl_lift),
                relative(b.cd_drag, a.cd_drag),
                relative(b.cm_pitch, a.cm_pitch),
                relative(cdi_b, cdi_a),
                gamma,
                relative(b.cy_side, a.cy_side),
                relative(b.cl_roll, a.cl_roll),
                relative(b.cn_yaw, a.cn_yaw),
            ];
            for (w, v) in worst.iter_mut().zip(values) {
                *w = w.max(v);
            }
        }
    }
    println!(
        "max relative difference over {count} assemblies: CL {:.2e} CD {:.2e} Cm {:.2e} CDi {:.2e} gamma/max {:.2e} CY {:.2e} Cl {:.2e} Cn {:.2e}",
        worst[0], worst[1], worst[2], worst[3], worst[4], worst[5], worst[6], worst[7]
    );
}
