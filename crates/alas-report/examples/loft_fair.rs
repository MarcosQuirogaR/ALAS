// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Study harness (never committed): Rust validator and sampled fairness of loft JSON files.
//!
//! usage: loft_fair <file.json>...   -> one JSON object per line.

#![allow(clippy::print_stdout)]

use alas_config::parse_loft_spec;
use alas_geom::loft::{Loft, LoftParams};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path)?;
        let out = match parse_loft_spec(&text) {
            Err(e) => json!({"file": path, "error": format!("{e}")}),
            Ok(spec) => match Loft::new(&spec, LoftParams::new(spec.reference_length_m)) {
                Err(e) => json!({"file": path, "error": format!("{e}")}),
                Ok(loft) => {
                    let f = loft.fairness();
                    let (x0, x1) = loft.x_range();
                    let sx: Vec<_> = (0..=600)
                        .map(|i| {
                            let x = x0 + (x1 - x0) * f64::from(i) / 600.0;
                            [x, loft.ds_dx(x), loft.crown_z(x), loft.keel_z(x)]
                        })
                        .collect();
                    json!({"file": path, "sx": sx, "violations": f.violations,
                           "min_exponent": f.min_exponent, "min_squared_extent": f.min_squared_extent,
                           "area_maxima": f.area_maxima,
                           "lines": f.lines.iter().map(|l| json!({
                               "name": l.name, "inflections": l.inflections, "extrema": l.curvature_extrema,
                               "tv_ratio": l.tv_ratio, "energy": l.energy, "max_curvature": l.max_curvature,
                               "knot_jump": l.knot_jump})).collect::<Vec<_>>()})
                }
            },
        };
        println!("{out}");
    }
    Ok(())
}
