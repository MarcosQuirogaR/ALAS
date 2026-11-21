// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The 45 deg swept box beam of NACA TN 1525 (Zender and Libove, 1948) built
//! through the product mesh generator and the native beam.
//!
//! Article (TN 1525, Fig. 3, p. 29): untapered 24S-T box, 45 deg sweep, ribs
//! normal to the spars, 7 in deep, spars 30 in apart, 0.050 in covers with
//! thirteen 3/4 x 3/4 x 1/16 in angle stringers each, 0.078 in spar webs,
//! 1 1/4 x 1 1/4 x 1/8 in angle flanges, front spar 118 3/4 in from the
//! carry-through bay. Load: 2.5 kips at the centre of each tip section.
//! E = 10,500 ksi, G = 4000 ksi (p. 2).
//!
//! Writes the solver deck and the native deflection to `ALAS_TN1525_OUT`.
//! With MSC installed (`ALAS_MSC_LAUNCHER`, `ALAS_MSC_SOLVER`) it solves the
//! deck and holds the FE spar deflections within 15 % of the measured ones
//! (Fig. 8(a), p. 34).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use std::fmt::Write as _;

use alas_config::{
    materials, DesignRequirements, DesignVector, EngineConfig, MassModelConfig, StructuresConfig,
    WingConfig,
};
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::wing_structure::WingStructureGeometry;
use alas_struct::analytical::point_load_deflection;
use alas_struct::mesh::build_wing_mesh_bdf_product;
use alas_struct::sizing::size_wingbox_reference_compatibility;

const IN: f64 = 0.0254;

/// Measured downward deflection, in, against distance along the spar from
/// bulkhead 6, in, digitised from TN 1525 Fig. 8(a), p. 34. The points at
/// bulkhead 6 are left out: there the carry-through rotation is the whole
/// deflection.
const MEASURED_FRONT: [(f64, f64); 4] =
    [(22.5, 0.240), (44.5, 0.503), (66.5, 0.794), (88.1, 1.116)];
const MEASURED_REAR: [(f64, f64); 4] = [(22.5, 0.166), (44.5, 0.416), (66.4, 0.726), (88.0, 1.052)];

/// Rotation of the carry-through bay, rad, TN 1525 App. A, eq. (A4), p. 14.
const CARRY_THROUGH_ROTATION: f64 = 0.00208;

#[test]
#[ignore = "writes the NACA TN 1525 article to ALAS_TN1525_OUT; solves it with ALAS_MSC_LAUNCHER and ALAS_MSC_SOLVER"]
fn naca_tn1525_swept_box_article() {
    let out = std::path::PathBuf::from(std::env::var("ALAS_TN1525_OUT").expect("ALAS_TN1525_OUT"));
    std::fs::create_dir_all(&out).unwrap();

    // Planform: streamwise chord c, spars at f and r; the box is 30 in wide
    // normal to the spars, so (r - f) c cos 45 = 30 in.
    let sweep = 45.0_f64.to_radians();
    let chord = 1.5;
    let front = 0.10;
    let rear = front + 30.0 * IN / sweep.cos() / chord;
    // The tip rib starts at the leading-edge tip and runs normal to the spars;
    // it meets the front spar f c / 2 inboard. Front spar from the root plane
    // (bulkhead 8) to the tip: 118.75 in.
    let y_front_tip = 118.75 * IN * sweep.cos();
    let semi_span = y_front_tip + front * chord * sweep.sin() * sweep.cos();

    // Rectangular box section: skin mid-surface 7 - 0.05 in deep.
    let half = 0.5 * (7.0 - 0.05) * IN / chord;
    let n = 81;
    let shape = |x: f64| half * (x / 0.03).min(1.0).min((1.0 - x) / 0.03);
    let xs: Vec<f64> = (0..n).map(|i| i as f64 / (n - 1) as f64).collect();
    let mut coordinates: Vec<(f64, f64)> = xs.iter().rev().map(|&x| (x, shape(x))).collect();
    coordinates.extend(xs.iter().skip(1).map(|&x| (x, -shape(x))));
    let section = Airfoil::from_coordinates("tn1525-box", coordinates);

    let dv = DesignVector {
        span_m: 2.0 * semi_span,
        root_chord_m: chord,
        break_chord_m: chord,
        tip_chord_m: chord,
        sweep_deg: 45.0,
        tip_twist_deg: 0.0,
        ..DesignVector::default()
    };
    let wing_cfg = WingConfig {
        kink_span_fraction: None,
        side_of_body_span_fraction: None,
        outboard_sweep_decrement_deg: 0.0,
        outboard_le_sweep_deg: None,
        root_twist_deg: 0.0,
        break_twist_deg: 0.0,
        root_z_m: 0.0,
        break_z_m: 0.0,
        tip_z_m: 0.0,
        flight_tip_rise_semispan_fraction: None,
        flight_root_rise_m: None,
        dihedral_stations: Vec::new(),
        wingtip_device: None,
        ..WingConfig::default()
    };
    let geometry = WingStructureGeometry::new(
        &dv,
        &wing_cfg,
        &section,
        &section,
        &[front, rear],
        Some(&[true, true]),
    )
    .expect("article geometry");
    assert!(
        (geometry.semi_span - semi_span).abs() < 1e-9,
        "{}",
        geometry.semi_span
    );

    // 24S-T at the report's moduli; steel bulkheads.
    let mut alloy = materials::get("Al 2024-T3").unwrap().clone();
    alloy.e_pa = 10_500.0e3 * 6894.757;
    alloy.nu = 10_500.0 / (2.0 * 4000.0) - 1.0;
    let steel = materials::get("Steel 300M").unwrap();

    // Bulkheads 22 in apart along the spars.
    let rib_count = ((semi_span / (22.0 * IN * sweep.cos())).round() as i64) + 1;
    let cfg = StructuresConfig {
        num_ribs_override: Some(rib_count),
        mesh_chordwise_points: 30,
        t_rib_m: IN / 16.0,
        ..Default::default()
    };
    let req = DesignRequirements::default();
    let mut sizing =
        size_wingbox_reference_compatibility(&geometry, &cfg, &req, &alloy, &alloy, &alloy, steel);
    // The product idealisation: each cover's flanges and stringers lumped
    // into its two spar caps, 1 1/4 in wide.
    let angle = |leg: f64, t: f64| (2.0 * leg - t) * t;
    let corner = (angle(1.25, 0.125) + 6.5 * angle(0.75, 1.0 / 16.0)) * IN * IN;
    let width = 1.25 * IN;
    sizing.t_skin = 0.050 * IN;
    let stations = sizing.y_stations.len();
    sizing.axis_cos = vec![sweep.cos(); stations];
    for spar in &mut sizing.spars {
        spar.w_cap = vec![width; stations];
        spar.t_cap = vec![corner / width; stations];
        spar.a_cap = vec![corner; stations];
        spar.t_web = vec![0.078 * IN; stations];
        spar.lower_extra_area = vec![0.0; stations];
    }

    let (deck, report, index) = build_wing_mesh_bdf_product(
        &geometry,
        &sizing,
        &cfg,
        &EngineConfig::default(),
        &MassModelConfig::default(),
        &req,
        &alloy,
        &alloy,
        &alloy,
        steel,
    )
    .expect("article mesh");

    // 2.5 kips at the tip section, a quarter at each spar-cap corner.
    let load = 2.5e3 * 4.448_222;
    let tips: Vec<i64> = index
        .spar_upper_nids
        .iter()
        .chain(&index.spar_lower_nids)
        .map(|line| *line.last().unwrap())
        .collect();
    let mut text = String::new();
    writeln!(
        text,
        "SOL 101\nCEND\nTITLE = NACA TN 1525 swept box, tip bending"
    )
    .unwrap();
    writeln!(
        text,
        "SUBCASE 1\n  LOAD = 1\n  SPC = 1\n  DISPLACEMENT = ALL"
    )
    .unwrap();
    writeln!(
        text,
        "  STRESS(VONMISES) = ALL\n  FORCE = ALL\n  SPCFORCE = ALL"
    )
    .unwrap();
    writeln!(text, "BEGIN BULK").unwrap();
    for &nid in &tips {
        writeln!(
            text,
            "FORCE,1,{nid},0,{:.6e},0.0,0.0,-1.0",
            load / tips.len() as f64
        )
        .unwrap();
    }
    text.push_str(&deck.write_bulk_msc());
    if !text.trim_end().ends_with("ENDDATA") {
        writeln!(text, "ENDDATA").unwrap();
    }
    std::fs::write(out.join("tn1525.bdf"), text).unwrap();

    // Spar lines and the native deflection at each spar grid, for the
    // comparison script.
    let y_load = tips.iter().map(|&n| deck.node_y(n)).sum::<f64>() / tips.len() as f64;
    let native = point_load_deflection(&sizing, &alloy, &alloy, &alloy, y_load, load);
    let mut csv = String::from("line,nid,x,y,z,native_w\n");
    for (name, line) in [
        ("front", &index.spar_upper_nids[0]),
        ("rear", &index.spar_upper_nids[1]),
    ] {
        for &nid in line {
            let p = deck.grid_xyz(nid).unwrap();
            let w = interp(&sizing.y_stations, &native, p[1]);
            writeln!(csv, "{name},{nid},{},{},{},{w}", p[0], p[1], p[2]).unwrap();
        }
    }
    std::fs::write(out.join("native.csv"), csv).unwrap();
    println!(
        "TN1525 semi_span {semi_span:.4} y_load {y_load:.4} ribs {rib_count} rbe3 {} warp_max {:.4} corner_area_in2 {:.4}",
        report.rbe3_count,
        report.warping_max,
        corner / IN / IN
    );
    // Solve and compare with the measured deflections when MSC is installed.
    let (Some(launcher), Some(solver)) = (
        std::env::var_os("ALAS_MSC_LAUNCHER"),
        std::env::var_os("ALAS_MSC_SOLVER"),
    ) else {
        return;
    };
    let bdf = out.join("tn1525.bdf");
    let outcome = alas_struct::nastran::run_nastran_with_solver(
        &bdf,
        std::path::Path::new(&launcher),
        Some(std::path::Path::new(&solver)),
        600.0,
    );
    assert!(outcome.ok, "MSC failed: {}", outcome.detail);
    let f06 = std::fs::read_to_string(bdf.with_extension("f06")).expect("MSC wrote an .f06");
    let mut uz = std::collections::HashMap::new();
    for row in f06.lines() {
        let s: Vec<&str> = row.split_whitespace().collect();
        if s.len() == 8 && s[1] == "G" {
            if let (Ok(nid), Ok(w)) = (s[0].parse::<i64>(), s[4].parse::<f64>()) {
                uz.insert(nid, w);
            }
        }
    }
    let sin45 = std::f64::consts::FRAC_1_SQRT_2;
    let mut worst: f64 = 0.0;
    for (name, line, offset, measured) in [
        ("front", &index.spar_upper_nids[0], 30.0, &MEASURED_FRONT),
        ("rear", &index.spar_upper_nids[1], 0.0, &MEASURED_REAR),
    ] {
        // Distance along the spar from bulkhead 6, in, and the downward
        // deflection, in, at every spar grid.
        let pts: Vec<(f64, f64)> = line
            .iter()
            .map(|&nid| {
                (
                    deck.node_y(nid) / sin45 / IN - offset,
                    -uz.get(&nid).copied().unwrap_or(f64::NAN) / IN,
                )
            })
            .collect();
        for &(x, w) in measured.iter() {
            // The articles are clamped on the root plane (bulkhead 8): remove
            // the carry-through-bay rotation, App. A, eq. (A4), p. 14.
            let rigid = CARRY_THROUGH_ROTATION * sin45 * (x + offset);
            let reference = w - rigid;
            let fe = interp(
                &pts.iter().map(|p| p.0).collect::<Vec<_>>(),
                &pts.iter().map(|p| p.1).collect::<Vec<_>>(),
                x,
            );
            let error = fe / reference - 1.0;
            println!(
                "TN1525 {name} x {x:5.1} in: measured {reference:.3} in, FE {fe:.3} in ({:+.1} %)",
                error * 100.0
            );
            worst = worst.max(error.abs());
        }
    }
    assert!(
        worst <= 0.15,
        "FE deflection departs {:.1} % from TN 1525 Fig. 8(a)",
        worst * 100.0
    );
}

fn interp(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    for k in 0..xs.len() - 1 {
        if x <= xs[k + 1] {
            let t = ((x - xs[k]) / (xs[k + 1] - xs[k])).clamp(0.0, 1.0);
            return ys[k] + t * (ys[k + 1] - ys[k]);
        }
    }
    *ys.last().unwrap()
}
