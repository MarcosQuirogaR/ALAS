// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Scratch correlation probe: bound engine, planform closure and tail sizing.
#![allow(clippy::print_stdout, missing_docs)]
// Standalone diagnostic examples report to the console and assert their inputs.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::presets;

fn main() {
    println!(
        "{:<11} {:>8} {:>7} {:>8} {:>7} {:>7} {:>7} {:>8} {:>7} {:>7} {:>7}",
        "preset", "S_ref", "span", "MAC", "yMAC", "xLEMAC", "AR", "c/4out", "Sh", "Sv", "thrust"
    );
    for name in presets::available() {
        let p = presets::get(name).unwrap();
        let dv = &p.design_vector;
        let w = &p.geometry.wing;
        let pf = w.transport_planform(dv).unwrap();
        let s = dv.span_m / 2.0;
        let (yk, cr, cb, ct) = (
            pf.kink.y_m,
            pf.root.chord_m,
            pf.kink.chord_m,
            pf.tip.chord_m,
        );
        let chord = |y: f64| {
            if y <= yk {
                cr + (cb - cr) * y / yk
            } else {
                cb + (ct - cb) * (y - yk) / (s - yk)
            }
        };
        let n = 200_000;
        let (mut a, mut ic, mut iy) = (0.0, 0.0, 0.0);
        for i in 0..n {
            let y = (i as f64 + 0.5) * s / n as f64;
            let dy = s / n as f64;
            let c = chord(y);
            a += c * dy;
            ic += c * c * dy;
            iy += c * y * dy;
        }
        let (s_ref, mac, y_mac) = (2.0 * a, ic / a, iy / a);
        let x_lemac =
            w.root_datum_x_m + dv.wing_x_shift_m + y_mac * dv.sweep_deg.to_radians().tan();
        let c4 = ((pf.tip.leading_edge_x_m + 0.25 * ct - pf.kink.leading_edge_x_m - 0.25 * cb)
            / (s - yk))
            .atan()
            .to_degrees();
        let e = &p.geometry.empennage;
        let sh = (e.hstab_root_chord_m + e.hstab_tip_chord_m) * e.hstab_tip_le_m.1;
        let sv = (e.vstab_root_chord_m + e.vstab_tip_chord_m) / 2.0 * e.vstab_tip_le_m.2;
        let thrust = match p.geometry.engine.active_model() {
            Ok(alas_config::ActiveEngineModel::Turbofan(t)) => t.rated_thrust_kn,
            _ => 0.0,
        };
        println!(
            "{:<11} {:>8.2} {:>7.2} {:>8.4} {:>7.3} {:>7.2} {:>7.3} {:>8.2} {:>7.2} {:>7.2} {:>7.1}",
            name,
            s_ref,
            dv.span_m,
            mac,
            y_mac,
            x_lemac,
            dv.span_m * dv.span_m / s_ref,
            c4,
            sh,
            sv,
            thrust
        );
    }
}
