// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per-preset aft-body geometry behind the upsweep and base drag.
//!
//! `cargo run -p alas-aero --example drag1_aft_body_probe` prints one summary
//! row per preset; with `segments` it also prints the lofted aft body in
//! 0.25 m steps: half-width `a`, half-height `e`, centre, crown and keel
//! heights, and the side, crown and keel angles to the freestream (deg,
//! positive aft-facing) of each step. `root_dz_over_h` is the height of the
//! main wing's root leading edge above the body centre, over the body height
//! there: the fairing's wing position is low below -1/6, high above +1/6.

use alas_aero::analysis::AftBody;
use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;

/// Step of the segment table, m.
const STEP_M: f64 = 0.25;

// A command-line diagnostic writes its table to standard output.
#[allow(clippy::print_stdout)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let segments = std::env::args().any(|arg| arg == "segments");
    println!(
        "preset,aft_start_x_m,end_x_m,end_z_m,end_w_m,end_h_m,upsweep_deg,a_max_m2,\
         a_end_m2,a_sides_m2,a_crown_m2,a_base_m2,afterbody_l_over_d,small_base,root_dz_over_h"
    );
    let mut tables = Vec::new();
    for name in presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name}))?;
        let dv = presets::get(name)?.design_vector;
        let plane = AircraftBuilder::new(Some(config.geometry.clone())).build(Some(&dv), true)?;
        let Some(body) = plane.fuselages.first() else {
            continue;
        };
        let (Some(nose), Some(last)) = (body.xsecs.first(), body.xsecs.last()) else {
            continue;
        };
        // The same length and start the product buildup uses
        // (`AeroAnalysis::body_length` is `last.x - first.x`).
        let start = nose.xyz_c[0]
            + config
                .geometry
                .fuselage
                .aft_body_start_m(last.xyz_c[0] - nose.xyz_c[0]);
        let Some(aft) = AftBody::of(body, Some(start)) else {
            println!("{name},{start:.2},no aft body");
            continue;
        };
        let root_dz_over_h = match (
            plane.wings.first().and_then(|w| w.xsecs.first()),
            &body.loft,
        ) {
            (Some(root), Some(loft)) => {
                let section = loft.section(root.xyz_le[0]);
                (root.xyz_le[2] - section.zc) / (2.0 * section.e)
            }
            _ => f64::NAN,
        };
        println!(
            "{name},{start:.2},{:.2},{:.3},{:.3},{:.3},{:.2},{:.2},{:.4},{:.4},{:.4},{:.4},{:.2},{},{:.3}",
            last.xyz_c[0],
            last.xyz_c[2],
            last.width,
            last.height,
            aft.upsweep_rad.to_degrees(),
            aft.max_area_m2,
            aft.end_area_m2,
            aft.sides_area_m2,
            aft.crown_area_m2,
            aft.base_area_m2,
            aft.afterbody_fineness,
            aft.has_small_base(),
            root_dz_over_h
        );
        if let (true, Some(loft)) = (segments, body.loft.as_ref()) {
            let mut rows = vec![format!(
                "{name}: x_m,a_m,e_m,zc_m,crown_m,keel_m,side_deg,crown_deg,keel_deg"
            )];
            let end = loft.x_range().1;
            let mut x = start;
            while x < end - 1e-9 {
                let next = (x + STEP_M).min(end);
                let (p, q) = (loft.section(x), loft.section(next));
                let dx = next - x;
                let angle = |drop: f64| drop.atan2(dx).to_degrees();
                rows.push(format!(
                    "{name}: {x:.2},{:.3},{:.3},{:.3},{:.3},{:.3},{:.1},{:.1},{:.1}",
                    p.a,
                    p.e,
                    p.zc,
                    p.crown_z(),
                    p.keel_z(),
                    angle(p.a - q.a),
                    angle(p.crown_z() - q.crown_z()),
                    angle(q.keel_z() - p.keel_z()),
                ));
                x = next;
            }
            tables.push(rows);
        }
    }
    for line in tables.into_iter().flatten() {
        println!("{line}");
    }
    Ok(())
}
