// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wing-alone lattice aerodynamic centre and lift slope against the spanwise
//! panel count.
//!
//! A flat trapezoid (two cross-sections, root and tip) of aspect ratio `A`,
//! taper `lambda`, quarter-chord sweep `L`: the lattice's x_ac as a fraction of
//! the trapezoid's MAC from its leading edge, and CL_alpha per rad, at
//! spanwise resolutions 1 to 64 (chordwise 8). Mach 0.08, sea level.
//! Check against Torenbeek (1982) Fig. E-10 (printed p. 478, lifting-surface
//! charts of Ref. E-31, read on the page): A 6 / taper 0.5 / 0 deg about 0.24,
//! A 6 / 0.3 / 35 deg about 0.325, A 8 / 0.3 / 35 deg about 0.36, A 6 / 0.2 /
//! 50 deg about 0.45; the lattice converges to 0.24 / 0.31 / 0.33 / 0.41 and
//! sits 0.10-0.15 aft of them on one strip per semispan.
//! With preset names as arguments it also prints the built preset's geometry
//! and its main-wing-alone AC at the builder mesh and with each strip split 2
//! and 4 times (B747-400 33.2 / 33.1 / 33.3 %MAC: converged).
//!
//! `cargo run --release -p alas-pipeline --example fix2_np_wing_ac_probe`
#![allow(clippy::print_stdout, missing_docs)]
// Diagnostic over fixed analytic planforms: a failure aborts the run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{Wing, WingXSec};

fn trapezoid(ar: f64, taper: f64, sweep_c4_deg: f64) -> (Airplane, f64, f64) {
    let s = 10.0_f64;
    let b = (s * ar).sqrt();
    let cr = 2.0 * s / (b * (1.0 + taper));
    let ct = taper * cr;
    let half = b / 2.0;
    let tip_le = half * sweep_c4_deg.to_radians().tan() + 0.25 * (cr - ct);
    let mac = 2.0 / 3.0 * cr * (1.0 + taper + taper * taper) / (1.0 + taper);
    let y_mac = b / 6.0 * (1.0 + 2.0 * taper) / (1.0 + taper);
    let le_mac = y_mac * tip_le / half;
    let foil = Airfoil::from_name("naca0012").unwrap();
    let wing = Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], cr, 0.0, foil.clone()),
            WingXSec::new([tip_le, half, 0.0], ct, 0.0, foil),
        ],
        true,
    );
    let plane = Airplane {
        name: "trapezoid".to_owned(),
        xyz_ref: [le_mac + 0.25 * mac, 0.0, 0.0],
        wings: vec![wing],
        fuselages: vec![],
        s_ref: s,
        c_ref: mac,
        b_ref: b,
    };
    (plane, le_mac, mac)
}

fn main() {
    for name in std::env::args().skip(1) {
        preset_wing_convergence(&name);
    }
    let atm = Atmosphere::new(0.0);
    let v = 0.08 * atm.speed_of_sound();
    let op = |a: f64| OperatingPoint::new(atm, v, a, 0.0, 0.0, 0.0, 0.0);
    for (ar, taper, sweep) in [
        (6.0, 0.5, 0.0),
        (6.0, 0.2, 50.0),
        (6.0, 0.3, 35.0),
        (8.0, 0.3, 35.0),
        (7.0, 0.3, 35.0),
        (7.0, 0.25, 37.5),
    ] {
        let (plane, le_mac, mac) = trapezoid(ar, taper, sweep);
        print!("A {ar} taper {taper} sweep {sweep}:");
        for ns in [1usize, 2, 4, 8, 16, 32, 64] {
            let sys = VlmSystem::assemble(&plane, ns, 8).unwrap();
            let lo = sys.solve(&op(0.0)).unwrap();
            let hi = sys.solve(&op(2.0)).unwrap();
            let cla = (hi.cl_lift - lo.cl_lift) / 2.0_f64.to_radians();
            let dcm = hi.cm_pitch - lo.cm_pitch;
            let dcl = hi.cl_lift - lo.cl_lift;
            let x_ac = plane.xyz_ref[0] - dcm / dcl * mac;
            print!("  ns {ns}: xac {:.3} CLa {:.3}", (x_ac - le_mac) / mac, cla);
        }
        println!();
    }
}

/// Main-wing-alone lattice AC and slope of a built preset at the builder's own
/// strips and with each strip split 2 and 4 times (cosine), plus strip counts.
pub fn preset_wing_convergence(name: &str) {
    use alas_config::{presets, AlasConfig};
    use alas_geom::aircraft::wing::SpacingFunction;
    use alas_pipeline::full_analysis::FullAnalysis;
    let preset = presets::get(name).unwrap();
    let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .unwrap();
    let plane = &report.airplane;
    let frame = plane.mac_frame().unwrap();
    let atm = Atmosphere::new(0.0);
    let v = 0.2 * atm.speed_of_sound();
    let op = |a: f64| OperatingPoint::new(atm, v, a, 0.0, 0.0, 0.0, 0.0);
    println!(
        "  {name}: S {:.1} m2 b {:.2} m c_ref {:.3} m; MAC frame LEMAC {:.3} m chord {:.3} m; xyz_ref {:.3}",
        plane.s_ref, plane.b_ref, plane.c_ref, frame.x_lemac_m, frame.chord_m, plane.xyz_ref[0]
    );
    for w in &plane.wings {
        let first = &w.xsecs[0];
        let last = &w.xsecs[w.xsecs.len() - 1];
        println!(
            "  {name}: wing '{}' {} xsecs, area {:.1} m2, root LE x {:.2} y {:.2} z {:.2} c {:.2}, tip LE x {:.2} y {:.2} c {:.2}, AC(0.25) x {:.2}",
            w.name,
            w.xsecs.len(),
            w.area(),
            first.xyz_le[0],
            first.xyz_le[1],
            first.xyz_le[2],
            first.chord,
            last.xyz_le[0],
            last.xyz_le[1],
            last.chord,
            w.aerodynamic_center(0.25)[0]
        );
    }
    for f in &plane.fuselages {
        let x0 = f.xsecs.first().unwrap().xyz_c[0];
        let x1 = f.xsecs.last().unwrap().xyz_c[0];
        let wmax = f.xsecs.iter().map(|s| s.width).fold(0.0_f64, f64::max);
        println!(
            "  {name}: body '{}' x {x0:.2}..{x1:.2} m, max width {wmax:.2} m",
            f.name
        );
    }
    let main = plane.wings.iter().find(|w| w.name == "Main Wing").unwrap();
    print!("  {name} main wing alone (MAC {:.3} m):", frame.chord_m);
    for k in [1usize, 2, 4] {
        let mut alone = plane.clone();
        alone.fuselages.clear();
        alone.wings = vec![if k == 1 {
            main.clone()
        } else {
            main.subdivide_sections(k, SpacingFunction::Cosspace)
                .unwrap()
        }];
        let sys = VlmSystem::assemble(&alone, 1, 8).unwrap();
        let lo = sys.solve(&op(0.0)).unwrap();
        let hi = sys.solve(&op(2.0)).unwrap();
        let cla = (hi.cl_lift - lo.cl_lift) / 2.0_f64.to_radians();
        let x_ac = alone.xyz_ref[0]
            - (hi.cm_pitch - lo.cm_pitch) / (hi.cl_lift - lo.cl_lift) * alone.c_ref;
        print!(
            "  x{k}: xac {:.1} %MAC CLa {:.3}",
            100.0 * (x_ac - frame.x_lemac_m) / frame.chord_m,
            cla
        );
    }
    println!();
}
