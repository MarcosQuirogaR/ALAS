// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Study harness (never committed): reference quantities of a preset's built geometry.
//! usage: stepb_probe <preset>...   (JSON lines on stdout; x aft from the nose, metres)

#![allow(clippy::print_stdout)]

use alas_config::WingShape;
use alas_geom::builder::AircraftBuilder;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for preset in std::env::args().skip(1) {
        let pr = alas_config::presets::get(&preset)?;
        let dv = pr.design_vector;
        let (plane, _) = AircraftBuilder::new(Some(pr.geometry.clone())).build_reported(
            Some(&dv),
            true,
            WingShape::Flight,
        )?;
        let shift = pr.geometry.wing.root_datum_x_m + dv.wing_x_shift_m;
        let w = &plane.wings[0];
        let mac = w.mac_station();
        let h = &plane.wings[1];
        let hm = h.mac_station();
        let v = &plane.wings[2];
        let (s_v, qv, vmc) = {
            let (mut a_sum, mut qx, mut mc) = (0.0, 0.0, 0.0);
            for p in v.xsecs.windows(2) {
                let span = (p[1].xyz_le[2] - p[0].xyz_le[2]).hypot(p[1].xyz_le[1] - p[0].xyz_le[1]);
                let (c0, c1) = (p[0].chord, p[1].chord);
                let a = span * (c0 + c1) / 2.0;
                let q0 = p[0].xyz_le[0] + 0.25 * c0;
                let q1 = p[1].xyz_le[0] + 0.25 * c1;
                let lam = c1 / c0;
                let mac = (2.0 / 3.0) * c0 * (1.0 + lam + lam * lam) / (1.0 + lam);
                let yf = (1.0 + 2.0 * lam) / (3.0 * (1.0 + lam));
                a_sum += a;
                qx += a * (q0 + yf * (q1 - q0));
                mc += a * mac;
            }
            (a_sum, qx / a_sum, mc / a_sum)
        };
        let s_h = h.reference_area();
        let qw = mac.x_le_m + 0.25 * mac.chord_m;
        let qh = hm.x_le_m + 0.25 * hm.chord_m;
        let (lh, lv) = (qh - qw, qv - qw);
        let b = plane.b_ref;
        let nacelle_wetted: Vec<f64> = plane
            .fuselages
            .iter()
            .skip(1)
            .map(|f| f.area_wetted())
            .collect();
        println!(
            "{}",
            json!({
                "preset": preset,
                "s_ref": plane.s_ref, "c_ref": plane.c_ref, "b_ref": b,
                "mac_chord": mac.chord_m, "lemac_x": mac.x_le_m, "quarter_mac_x": qw,
                "x_wing_global": shift,
                "s_hstab": s_h, "s_vstab": s_v,
                "hstab_mac_chord": hm.chord_m, "vstab_mac_chord": vmc,
                "l_h_m": lh, "l_v_m": lv,
                "v_h": s_h * lh / (plane.s_ref * plane.c_ref),
                "v_v": s_v * lv / (plane.s_ref * b),
                "nacelle_wetted_trapezoid_m2": nacelle_wetted,
                "wing_count": plane.wings.len(),
            })
        );
    }
    Ok(())
}
