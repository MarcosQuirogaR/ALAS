// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Every leading- and trailing-edge station of the main wing and tails of every
//! preset, with the sweep either side and its change (diagnostic, CSV on stdout).
#![allow(clippy::print_stdout, clippy::unwrap_used, missing_docs)]

use alas_config::presets;

fn sweep(dy: f64, dx: f64) -> f64 {
    dx.atan2(dy).to_degrees()
}

fn emit(
    preset: &str,
    surface: &str,
    kinds: &[String],
    etas: &[f64],
    ys: &[f64],
    le: &[f64],
    te: &[f64],
) {
    for (edge, xs) in [("LE", le), ("TE", te)] {
        for i in 0..xs.len() {
            let sin = (i > 0).then(|| sweep(ys[i] - ys[i - 1], xs[i] - xs[i - 1]));
            let sout = (i + 1 < xs.len()).then(|| sweep(ys[i + 1] - ys[i], xs[i + 1] - xs[i]));
            let d = match (sin, sout) {
                (Some(a), Some(b)) => format!("{:.4}", b - a),
                _ => String::new(),
            };
            let f = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.4}"));
            println!(
                "{preset},{surface},{edge},{},{:.5},{:.4},{:.4},{},{},{d}",
                kinds[i],
                etas[i],
                ys[i],
                xs[i],
                f(sin),
                f(sout)
            );
        }
    }
}

fn main() {
    println!("preset,surface,edge,station,eta,y_m,x_m,sweep_in_deg,sweep_out_deg,dsweep_deg");
    for p in presets::registry() {
        let Ok(pf) = p.geometry.wing.transport_planform(&p.design_vector) else {
            continue;
        };
        let st = pf.stations_with_rake();
        let kinds: Vec<String> = st.iter().map(|s| format!("{:?}", s.kind)).collect();
        let etas: Vec<f64> = st.iter().map(|s| s.span_fraction).collect();
        let ys: Vec<f64> = st.iter().map(|s| s.y_m).collect();
        let le: Vec<f64> = st.iter().map(|s| s.leading_edge_x_m).collect();
        let te: Vec<f64> = st.iter().map(|s| s.leading_edge_x_m + s.chord_m).collect();
        emit(p.name, "wing", &kinds, &etas, &ys, &le, &te);
        let e = &p.geometry.empennage;
        for (surface, run, tip_x, c0, c1, stations) in [
            (
                "htail",
                e.hstab_tip_le_m.1.abs(),
                e.hstab_tip_le_m.0,
                e.hstab_root_chord_m,
                e.hstab_tip_chord_m,
                &e.hstab_edge_stations,
            ),
            (
                "vtail",
                e.vstab_tip_le_m.2.abs(),
                e.vstab_tip_le_m.0,
                e.vstab_root_chord_m,
                e.vstab_tip_chord_m,
                &e.vstab_edge_stations,
            ),
        ] {
            if run <= 0.0 {
                continue;
            }
            let mut pts: Vec<(f64, f64, f64)> = vec![(0.0, 0.0, 0.0)];
            for s in stations.iter() {
                pts.push((s.span_fraction, s.le_offset_m, s.te_offset_m));
            }
            if pts.last().unwrap().0 < 1.0 {
                pts.push((1.0, 0.0, 0.0));
            }
            let kinds: Vec<String> = pts
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    if i == 0 {
                        "Root".into()
                    } else if i + 1 == pts.len() {
                        "Tip".into()
                    } else {
                        "TailEdge".into()
                    }
                })
                .collect();
            let etas: Vec<f64> = pts.iter().map(|p| p.0).collect();
            let ys: Vec<f64> = pts.iter().map(|p| p.0 * run).collect();
            let le: Vec<f64> = pts.iter().map(|p| p.0 * tip_x + p.1).collect();
            let te: Vec<f64> = pts
                .iter()
                .map(|p| p.0 * tip_x + p.1 + c0 + p.0 * (c1 - c0) + p.2 - p.1)
                .collect();
            emit(p.name, surface, &kinds, &etas, &ys, &le, &te);
        }
    }
}
