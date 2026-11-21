// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ALAS neutral point against NASA TN D-3797.
//!
//! Aoyagi and Tolhurst, "Large-scale wind-tunnel tests of a subsonic transport
//! with aft engine nacelles and high tail", NASA TN D-3797, 1967 (Ames 40 x 80 ft
//! tunnel, q = 20 psf, Mach about 0.1). Geometry from Fig. 2a (page 12): wing
//! S = 249 ft2, AR 5.38, taper 0.23, quarter-chord sweep 35 deg, MAC 7.96 ft, low
//! wing 1.25 ft below the fuselage axis; fuselage 43.40 ft long, 4.06 ft diameter
//! (cylinder from 4.75 to 30.75 ft); T-tail 60 ft2, AR 3.23, taper 0.40, sweep 35 deg,
//! its quarter-MAC point 20.3 ft behind the moment reference 0.44 MAC, 10.92 ft above
//! the axis; two aft pods 6.00 ft long, 1.56 ft outer diameter at 2.5 ft from the
//! axis. Published: the tail-on neutral point 0.715 MAC (static margin 27.5 percent at
//! the 0.44 MAC reference, page text), tail off about 0.44, tail and nacelles off
//! about 0.40 (derivatives read from Figs. 10 and 18; unverified readings).
//! Fig. 11a (page 27): tail off, dCm/dCL about 0 about the
//! 0.44 MAC centre (aerodynamic centre about 0.44); tail on, i_t = 0, CL_alpha
//! about 0.0765 per deg (4.4 per rad) between alpha 4 and 12 deg.
//!
//! The wing is the published planform, not a trapezoid: page 3 and
//! Fig. 2b (page 13) give an unswept trailing-edge chord extension inboard of
//! 0.37 semispan (centre-line chord 12.59 ft = 151.08 in against 9.70 ft for
//! the basic 23 deg trailing edge), the 38.5 deg leading edge starts on the
//! centre line 14.75 ft behind the nose (Fig. 2a), and the drawn MAC sits at
//! 5.81 ft. Both surfaces are meshed spanwise as the product builder meshes a
//! preset (24 and 6 panels per semispan); the earlier trapezoid on one strip per
//! semispan agreed by a cancellation (the missing extension moved the
//! aerodynamic centre forward, the single strip moved it aft).
//!
//! `cargo run --release -p alas-pipeline --example sm1_validation_tnd3797`
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over a fixed published geometry: a failure aborts the run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
use alas_geom::aircraft::wing::{SpacingFunction, Wing, WingXSec};
use alas_stab::neutral_point::{neutral_point_conditions, NpConditionsInput};

const FT: f64 = 0.3048;

fn section(x: f64, y: f64, z: f64, chord: f64) -> WingXSec {
    WingXSec::new(
        [x * FT, y * FT, z * FT],
        chord * FT,
        0.0,
        Airfoil::from_name("naca0012").unwrap(),
    )
}

fn fuselage(name: &str, stations: &[(f64, f64, f64)]) -> Fuselage {
    Fuselage::new(
        name,
        stations
            .iter()
            .map(|&(x, y, r)| {
                FuselageXSec::new(
                    [x * FT, y * FT, 0.0],
                    Some(r * FT),
                    None,
                    None,
                    DEFAULT_SHAPE,
                )
                .unwrap()
            })
            .collect(),
    )
}

/// The model; `with_tail` and `with_nacelles` select the tested configurations.
fn model(with_tail: bool, with_nacelles: bool) -> (Airplane, f64, f64) {
    // Fig. 2a (page 12) and Fig. 2b (page 13, inches): leading edge swept
    // 38.5 deg from the apex on the centre line 14.75 ft from the nose; centre-
    // line chord 12.59 ft (151.08 in) with the unswept trailing-edge extension
    // out to eta 0.37 (6.73 ft, 80.76 in), where it meets the basic 23 deg
    // trailing edge; tip chord 2.91 ft (34.92 in) at 18.31 ft; dihedral 3 deg.
    // The drawn MAC (7.96 ft) sits at 5.81 ft, where the local chord equals it;
    // the 0.44 MAC moment centre is 0.44 x 7.96 ft behind that station's
    // leading edge (scaled off the drawing it lands within 0.05 ft of it).
    let b = 2.0 * 18.31_f64;
    let half = b / 2.0;
    let tan_le = 38.5_f64.to_radians().tan();
    let x_apex = 14.75;
    let (y_kink, c_root, c_tip) = (6.73_f64, 12.59_f64, 2.91_f64);
    let c_kink = c_root - y_kink * tan_le;
    let wing_z = -1.25;
    let dihedral = 3.0_f64.to_radians().tan();
    let wing = Wing::new(
        "Main Wing",
        vec![
            section(x_apex, 0.0, wing_z, c_root),
            section(
                x_apex + y_kink * tan_le,
                y_kink,
                wing_z + y_kink * dihedral,
                c_kink,
            ),
            section(
                x_apex + half * tan_le,
                half,
                wing_z + half * dihedral,
                c_tip,
            ),
        ],
        true,
    )
    .mesh_spanwise(24, SpacingFunction::Linspace)
    .unwrap();
    let le_mac = x_apex + 5.81 * tan_le;
    // Moment reference 0.44 of the published MAC (7.96 ft) behind its leading edge.
    let x_ref = le_mac + 0.44 * 7.96;
    // Reference wing area and span of the source table.
    let s = 249.0_f64;
    let mut wings = vec![wing];
    if with_tail {
        let (st, art, tt) = (60.0_f64, 3.23_f64, 0.40_f64);
        let bt = (st * art).sqrt();
        let cr = 2.0 * st / (bt * (1.0 + tt));
        let ct = tt * cr;
        let sweep = 35.0_f64.to_radians();
        let tip_off = bt / 2.0 * sweep.tan() + 0.25 * (cr - ct);
        let mac_t = 2.0 / 3.0 * cr * (1.0 + tt + tt * tt) / (1.0 + tt);
        let y_t = bt / 6.0 * (1.0 + 2.0 * tt) / (1.0 + tt);
        // Quarter-MAC point 20.3 ft behind the moment reference.
        let le_mac_t = x_ref + 20.3 - 0.25 * mac_t;
        let x_root_le = le_mac_t - y_t * (tip_off / (bt / 2.0));
        wings.push(
            Wing::new(
                "Horizontal Stabilizer",
                vec![
                    section(x_root_le, 0.0, 10.92, cr),
                    section(x_root_le + tip_off, bt / 2.0, 10.92, ct),
                ],
                true,
            )
            .mesh_spanwise(6, SpacingFunction::Linspace)
            .unwrap(),
        );
    }
    let fus = fuselage(
        "Fuselage",
        &[
            (0.0, 0.0, 0.05),
            (0.6, 0.0, 0.9),
            (1.6, 0.0, 1.55),
            (3.0, 0.0, 1.9),
            (4.75, 0.0, 2.03),
            (30.75, 0.0, 2.03),
            (36.0, 0.0, 1.5),
            (40.0, 0.0, 0.8),
            (43.4, 0.0, 0.25),
        ],
    );
    let mut fuselages = vec![fus];
    if with_nacelles {
        for side in [-1.0, 1.0] {
            fuselages.push(fuselage(
                "Nacelle",
                &[
                    (30.2, side * 2.5, 0.62),
                    (33.2, side * 2.5, 0.78),
                    (36.2, side * 2.5, 0.55),
                ],
            ));
        }
    }
    let plane = Airplane {
        name: "TN D-3797".to_owned(),
        xyz_ref: [x_ref * FT, 0.0, 0.0],
        wings,
        fuselages,
        s_ref: s * FT * FT,
        c_ref: 7.96 * FT,
        b_ref: b * FT,
    };
    (plane, x_ref, le_mac)
}

fn main() {
    let analysis = AlasConfig::default().analysis;
    let mut fine = analysis.clone();
    fine.spanwise_resolution = fine.fine_spanwise_resolution;
    fine.chordwise_resolution = fine.fine_chordwise_resolution;
    let input = NpConditionsInput {
        low_speed_mach: 0.1,
        low_speed_altitude_m: 0.0,
        cruise_mach: 0.1,
        cruise_altitude_m: 0.0,
        include_flexibility: false,
        wing_stiffness: None,
        tail_flexibility: None,
        high_lift: None,
        propeller: None,
        jet_inlets: None,
        max_operating: None,
    };
    println!("NASA TN D-3797 (published NP: tail on 0.715 from the stated 27.5 % margin; tail off about 0.44; tail and nacelles off about 0.40)");
    // Tail-on neutral point against the tail dynamic-pressure efficiency.
    for eta in [0.90, 0.78] {
        let mut tuned = fine.clone();
        tuned.tail_efficiency = Some(eta);
        let (plane, x_ref, _) = model(true, true);
        let np = neutral_point_conditions(&plane, &tuned, &input).unwrap();
        println!(
            "  eta_tail {eta:.3}: tail on NP {:.3} MAC (published 0.715)",
            0.44 + (np.clean_low_speed / FT - x_ref) / 7.96
        );
    }
    for (label, tail, nacelles) in [
        ("tail on, nacelles on ", true, true),
        ("tail off, nacelles on", false, true),
        ("tail off, nacelles off", false, false),
        ("tail on, nacelles off", true, false),
    ] {
        let (plane, x_ref, _) = model(tail, nacelles);
        let np = neutral_point_conditions(&plane, &fine, &input).unwrap();
        let d = &np.diagnostics;
        let to_mac = |x_m: f64| 0.44 + (x_m / FT - x_ref) / 7.96;
        println!(
            "  {label}: NP {:.3} MAC (wing alone AC {:.3}, surfaces {:.3}, Multhopp {:+.3}, D1 {:+.3}, D2 x share {:+.3}, Raymer {:+.3}, nacelles {:+.3}); CLa {:.2}/rad, wing share {:.2}, downwash {:.2}",
            to_mac(np.clean_low_speed),
            to_mac(d.x_wing_alone_ac),
            to_mac(d.x_np_surfaces),
            d.fuselage.multhopp_shift_m / FT / 7.96,
            d.fuselage.torenbeek_d1_shift_m / FT / 7.96,
            d.wing_lift_fraction * d.fuselage.torenbeek_d2_shift_m / FT / 7.96,
            d.fuselage.gilruth_shift_m / FT / 7.96,
            d.nacelle_shift_m / FT / 7.96,
            d.cl_alpha_total,
            d.wing_lift_fraction,
            d.deps_dalpha_vlm
        );
    }
}
