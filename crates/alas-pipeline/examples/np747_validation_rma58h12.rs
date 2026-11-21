// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ALAS neutral point against NACA RM A58H12.
//!
//! Hickey and Aoyagi, "Large-scale wind-tunnel tests of a jet-transport-type
//! model with leading- and trailing-edge high-lift devices", NACA RM A58H12,
//! 1958 (Ames 40 x 80 ft tunnel, Mach about 0.08, R 2.8-8.1 million): a low,
//! 35 deg swept wing with four pylon-mounted pods, the 747 class at model scale.
//! Geometry read on Fig. 2 (page 10): wing S = 302.0 ft2, AR 7.0, taper 0.3,
//! quarter-chord sweep 35 deg, MAC 7.15 ft drawn at 9.95 ft from the plane of
//! symmetry, dihedral 6 deg, 2.07 ft below the fuselage axis; horizontal tail
//! 62.8 ft2, AR 3.7, taper 0.375, sweep 35 deg, its quarter-MAC point 21.46 ft
//! behind the moment centre, 1.08 ft above the axis; nacelle centrelines 9.2 and
//! 16.10 ft from the plane of symmetry. The moment centre is the wing
//! quarter-chord line at the drawn MAC station (0.25 MAC, page 2). Fuselage a
//! 4 x 5 ft ellipse with a straight-taper cone (page 4). Not dimensioned, so
//! scaled from the drawing (26.1 px/ft at 300 dpi on the 23.00 ft semispan,
//! checked on the 21.46 ft dimension): fuselage length 49.5 ft, moment centre
//! 23.2 ft from the nose, cone from 25.7 ft, nacelles 1.85 ft wide and 6.6 ft
//! long with inlets at 14.65 and 20.45 ft.
//!
//! Both lifting surfaces are meshed spanwise as the product builder meshes a
//! preset (24 and 6 panels per semispan, `FIX2_WING_PANELS` / `FIX2_TAIL_PANELS`
//! override them). The earlier version of this probe solved one spanwise strip
//! per semispan (two cross-sections and no builder subdivision), which put the
//! wing-alone lattice aerodynamic centre at 0.452 MAC instead of the converged
//! 0.32 (and raised CL_alpha by 13 %): that, not the wing-body physics, was the
//! 22-27 point residual reported for the 747 neutral point.
//!
//! Measured (plot readings, not tabulated; tail on unless stated, page 4; wall
//! corrections applied, page 4): Fig. 13a (page 27), plain leading edge, flaps
//! up, nacelles on (page 5 text), tail on, R 4.8-8.1 million: dCm/dCL -0.204
//! between CL 0.30 and 0.94 (squares, R 4.8 million, read at 260 dpi), so the
//! neutral point is 0.454 MAC (-0.15 and 0.40 at 2.8 million); CL_alpha 0.0761
//! to 0.0776 per deg between alpha 0.2 and 10.5 deg, 4.4 per rad (the 3.7 per
//! rad quoted before was a misreading). Fig. 11b (page 23), flaps 50 deg on
//! 0.09-0.63 semispan, nacelles off, CL 1.1-1.6: tail off dCm/dCL about +0.04
//! (aerodynamic centre about 0.21 MAC; high lift, near the break), tail on
//! about -0.25 (neutral point about 0.50 MAC). Fig. 12a (page 24, flaps 30 and
//! 50 deg): nacelles on and off give nearly parallel Cm-CL lines.
//!
//! `cargo run --release -p alas-pipeline --example np747_validation_rma58h12`
#![allow(clippy::print_stdout, missing_docs)]
// Diagnostic over a fixed published geometry: a failure aborts the run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
use alas_geom::aircraft::wing::{SpacingFunction, Wing, WingXSec};
use alas_stab::neutral_point::{neutral_point_conditions, NpConditionsInput};

#[path = "support/np_terms.rs"]
mod np_terms;

const FT: f64 = 0.3048;
/// Published wing MAC, ft, and the moment centre as a MAC fraction.
const MAC: f64 = 7.15;
const X_REF_MAC: f64 = 0.25;

fn section(x: f64, y: f64, z: f64, chord: f64, twist: f64) -> WingXSec {
    WingXSec::new(
        [x * FT, y * FT, z * FT],
        chord * FT,
        twist,
        Airfoil::from_name("naca0012").unwrap(),
    )
}

/// A trapezoid of area `s`, aspect ratio `ar`, taper `taper`, quarter-chord sweep
/// 35 deg: `(span, root chord, tip chord, tip leading-edge offset, MAC, y_mac)`.
fn trapezoid(s: f64, ar: f64, taper: f64) -> (f64, f64, f64, f64, f64, f64) {
    let b = (s * ar).sqrt();
    let cr = 2.0 * s / (b * (1.0 + taper));
    let ct = taper * cr;
    let tip_le = b / 2.0 * 35.0_f64.to_radians().tan() + 0.25 * (cr - ct);
    let mac = 2.0 / 3.0 * cr * (1.0 + taper + taper * taper) / (1.0 + taper);
    let y_mac = b / 6.0 * (1.0 + 2.0 * taper) / (1.0 + taper);
    (b, cr, ct, tip_le, mac, y_mac)
}

/// Spanwise panels per surface, as the product builder meshes a preset
/// (`geometry.wing.n_subdivisions` 24, `geometry.empennage.n_subdivisions` 6);
/// `FIX2_WING_PANELS` / `FIX2_TAIL_PANELS` override them.
fn panels_from(var: &str, default: usize) -> usize {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
fn spanwise_panels() -> usize {
    panels_from("FIX2_WING_PANELS", 24)
}
fn tail_panels() -> usize {
    panels_from("FIX2_TAIL_PANELS", 6)
}

/// The model; `with_tail` and `with_nacelles` select the configurations.
/// Returns the airplane and the leading edge of the published MAC, ft.
fn model(with_tail: bool, with_nacelles: bool) -> (Airplane, f64) {
    let (b, cr, ct, tip_le, mac, y_mac) = trapezoid(302.0, 7.0, 0.3);
    let half = b / 2.0;
    let _ = (mac, y_mac);
    // Fig. 2 (page 10): the moment centre is the quarter-chord line at the
    // drawn MAC station, 9.95 ft from the plane of symmetry (dimensioned; the
    // straight-taper formula would put it at 9.43 ft), and the 21.46 ft tail arm
    // runs from it. Scaled off the drawing (26.1 px/ft on the 23.00 ft
    // semispan) it is 23.2 ft behind the nose.
    let x_moment_centre = 23.2;
    let x_c4_root = x_moment_centre - 9.95 * 35.0_f64.to_radians().tan();
    let x_apex = x_c4_root - 0.25 * cr;
    let z_root = -2.07;
    let z_tip = z_root + half * 6.0_f64.to_radians().tan();
    let wing = Wing::new(
        "Main Wing",
        vec![
            section(x_apex, 0.0, z_root, cr, 2.0),
            section(x_apex + tip_le, half, z_tip, ct, 2.0),
        ],
        true,
    )
    .mesh_spanwise(spanwise_panels(), SpacingFunction::Linspace)
    .unwrap();
    // The published MAC (7.15 ft) frame: the moment centre is its 0.25 point.
    let le_mac = x_moment_centre - X_REF_MAC * MAC;
    let x_c4 = x_moment_centre;
    let mut wings = vec![wing];
    if with_tail {
        let (bt, crt, ctt, tip_t, mac_t, y_t) = trapezoid(62.8, 3.7, 0.375);
        let x_root = x_c4 + 21.46 - 0.25 * mac_t - y_t * tip_t / (bt / 2.0);
        wings.push(
            Wing::new(
                "Horizontal Stabilizer",
                vec![
                    section(x_root, 0.0, 1.08, crt, 0.0),
                    section(x_root + tip_t, bt / 2.0, 1.08, ctt, 0.0),
                ],
                true,
            )
            .mesh_spanwise(tail_panels(), SpacingFunction::Linspace)
            .unwrap(),
        );
    }
    // (station, scale of the 4 x 5 ft ellipse), ft.
    let body = [
        (0.0, 0.05),
        (1.0, 0.45),
        (2.5, 0.68),
        (5.0, 0.88),
        (8.0, 1.0),
        (25.7, 1.0),
        (33.0, 0.78),
        (40.0, 0.55),
        (46.0, 0.30),
        (49.5, 0.08),
    ];
    let xsec = |x: f64, y: f64, z: f64, w: f64, h: f64| {
        FuselageXSec::new(
            [x * FT, y * FT, z * FT],
            None,
            Some(w * FT),
            Some(h * FT),
            DEFAULT_SHAPE,
        )
        .unwrap()
    };
    let mut fuselages = vec![Fuselage::new(
        "Fuselage",
        body.iter()
            .map(|&(x, k)| xsec(x, 0.0, 0.0, 4.0 * k, 5.0 * k))
            .collect(),
    )];
    if with_nacelles {
        for (y, x_inlet) in [(9.2, 14.65), (16.10, 20.45)] {
            for side in [-1.0, 1.0] {
                fuselages.push(Fuselage::new(
                    "Nacelle",
                    [(0.0, 0.85), (1.5, 1.0), (6.6, 0.6)]
                        .iter()
                        .map(|&(dx, k)| xsec(x_inlet + dx, side * y, -3.0, 1.85 * k, 1.85 * k))
                        .collect(),
                ));
            }
        }
    }
    let plane = Airplane {
        name: "NACA RM A58H12".to_owned(),
        xyz_ref: [(le_mac + X_REF_MAC * MAC) * FT, 0.0, 0.0],
        wings,
        fuselages,
        s_ref: 302.0 * FT * FT,
        c_ref: MAC * FT,
        b_ref: b * FT,
    };
    (plane, le_mac)
}

fn main() {
    let analysis = AlasConfig::default().analysis;
    let mut fine = analysis.clone();
    fine.spanwise_resolution = fine.fine_spanwise_resolution;
    fine.chordwise_resolution = fine.fine_chordwise_resolution;
    let input = NpConditionsInput {
        low_speed_mach: 0.08,
        low_speed_altitude_m: 0.0,
        cruise_mach: 0.08,
        cruise_altitude_m: 0.0,
        include_flexibility: false,
        wing_stiffness: None,
        tail_flexibility: None,
        high_lift: None,
        propeller: None,
        jet_inlets: None,
        max_operating: None,
    };
    println!("NACA RM A58H12, flaps up (measured, plot readings: tail on, nacelles on NP 0.454 MAC and CLa about 4.4/rad, Fig. 13a)");
    for eta in [0.90, 0.78] {
        let mut tuned = fine.clone();
        tuned.tail_efficiency = Some(eta);
        for (label, tail, nacelles) in [
            ("tail on, nacelles on  ", true, true),
            ("tail on, nacelles off ", true, false),
            ("tail off, nacelles on ", false, true),
            ("tail off, nacelles off", false, false),
        ] {
            if !tail && eta != 0.78 {
                continue;
            }
            let (plane, le_mac) = model(tail, nacelles);
            let np = neutral_point_conditions(&plane, &tuned, &input).unwrap();
            let d = &np.diagnostics;
            let to_mac = |x_m: f64| (x_m / FT - le_mac) / MAC;
            let dx = |x_m: f64| x_m / FT / MAC;
            println!(
                "  eta {eta:.3} {label}: NP {:.3} MAC (wing alone AC {:.3}, surfaces {:.3}, Multhopp {:+.3}, D2 x share {:+.3}, D1 {:+.3}, Raymer {:+.3}, nacelles {:+.3}); CLa {:.2}/rad, wing share {:.2}, downwash {:.2}",
                to_mac(np.clean_low_speed),
                to_mac(d.x_wing_alone_ac),
                to_mac(d.x_np_surfaces),
                dx(d.fuselage.multhopp_shift_m),
                dx(d.wing_lift_fraction * d.fuselage.torenbeek_d2_shift_m),
                dx(d.fuselage.torenbeek_d1_shift_m),
                dx(d.fuselage.gilruth_shift_m),
                dx(d.nacelle_shift_m),
                d.cl_alpha_total,
                d.wing_lift_fraction,
                d.deps_dalpha_vlm
            );
        }
    }
    // The term-by-term decomposition at the product eta, in the
    // published MAC frame (same terms as `np747_term_probe`).
    let (plane, le_mac) = model(true, true);
    let terms = np_terms::decompose(&plane, &fine, 0.08, le_mac * FT, MAC * FT);
    println!("{}", np_terms::line("RM A58H12 decomposition", &terms));
}
