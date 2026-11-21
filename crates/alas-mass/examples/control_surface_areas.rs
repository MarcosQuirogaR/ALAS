// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Emit the built lifting-surface stations and the movable areas the declared
//! control-surface runs cut from them, for every registered preset.
//!
//! This is an audit fixture rather than a production entry point. The areas are
//! computed the way the control-surface figure draws them: each run is a chord
//! fraction over a span band of the built surface, both semispans where the
//! surface is mirrored.
//!
//! ```text
//! cargo run -p alas-mass --example control_surface_areas -- out.json [preset...]
//! ```

#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig};
use alas_geom::aircraft::wing::Wing;
use alas_geom::builder::AircraftBuilder;
use serde_json::{json, Value};
use std::{error::Error, fs};

/// `integral c ds` between two span values along one axis of the stations.
fn band_area(wing: &Wing, axis: usize, from: f64, to: f64) -> f64 {
    let mut area = 0.0;
    for pair in wing.xsecs.windows(2) {
        let (s0, s1) = (pair[0].xyz_le[axis], pair[1].xyz_le[axis]);
        let (lo, hi) = (s0.max(from), s1.min(to));
        if s1 > s0 && hi > lo {
            let chord =
                |s: f64| pair[0].chord + (s - s0) / (s1 - s0) * (pair[1].chord - pair[0].chord);
            area += 0.5 * (chord(lo) + chord(hi)) * (hi - lo);
        }
    }
    area
}

fn stations(wing: &Wing) -> Value {
    json!(wing
        .xsecs
        .iter()
        .map(|xsec| json!({
            "x_le": xsec.xyz_le[0],
            "y": xsec.xyz_le[1],
            "z": xsec.xyz_le[2],
            "chord": xsec.chord,
        }))
        .collect::<Vec<_>>())
}

fn row(name: &str) -> Result<Value, Box<dyn Error>> {
    let preset = presets::get(name).map_err(std::io::Error::other)?;
    let config = AlasConfig::from_value(&json!({"preset": name}))?;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)?;
    let cs = &config.control_surfaces;
    let wing = plane.wings.first().ok_or("no wing")?;
    let semi = wing.projected_span() / 2.0;
    let sides = if wing.symmetric { 2.0 } else { 1.0 };
    let on_wing = |chord: f64, start: f64, end: f64| {
        sides * chord * band_area(wing, 1, start * semi, end * semi)
    };
    let hstab = plane.wings.iter().find(|w| w.name.contains("Horizontal"));
    let vstab = plane.wings.iter().find(|w| w.name.contains("Vertical"));
    let elevator = hstab.map(|tail| {
        let semi = tail.projected_span() / 2.0;
        let sides = if tail.symmetric { 2.0 } else { 1.0 };
        sides
            * cs.elevator_chord_fraction
            * band_area(
                tail,
                1,
                cs.elevator_span_start_frac * semi,
                cs.elevator_span_end_frac * semi,
            )
    });
    let rudder = vstab.map(|fin| {
        let low = fin
            .xsecs
            .iter()
            .map(|x| x.xyz_le[2])
            .fold(f64::INFINITY, f64::min);
        let high = fin
            .xsecs
            .iter()
            .map(|x| x.xyz_le[2])
            .fold(f64::NEG_INFINITY, f64::max);
        cs.rudder_chord_fraction
            * band_area(
                fin,
                2,
                low + cs.rudder_span_start_frac * (high - low),
                low + cs.rudder_span_end_frac * (high - low),
            )
    });
    Ok(json!({
        "preset": name,
        "s_ref_m2": plane.s_ref,
        "wing_semi_m": semi,
        "wing": stations(wing),
        "wing_box": stations(wing.carried_box().as_ref()),
        "names": plane.wings.iter().map(|w| w.name.clone()).collect::<Vec<_>>(),
        "hstab": hstab.map(stations),
        "vstab": vstab.map(stations),
        "areas_m2": {
            "slat": on_wing(cs.slat_chord_fraction, cs.slat_span_start_frac, cs.slat_span_end_frac),
            "flap": cs
                .flap_bands()
                .into_iter()
                .map(|(start, end)| on_wing(cs.flap_chord_fraction, start, end))
                .sum::<f64>(),
            "aileron": on_wing(cs.aileron_chord_fraction, cs.aileron_span_start_frac, cs.aileron_span_end_frac)
                + on_wing(cs.inboard_aileron_chord_fraction, cs.inboard_aileron_span_start_frac, cs.inboard_aileron_span_end_frac),
            "spoiler": on_wing(cs.spoiler_chord_fraction, cs.spoiler_span_start_frac, cs.spoiler_span_end_frac),
            "elevator": elevator,
            "rudder": rudder,
        },
        "control_surfaces": cs,
        "cl_max_to": config.performance.cl_max_to,
        "cl_max_land": config.performance.cl_max_land,
        "cl_max_to_source": config.performance.cl_max_to_source,
    }))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let output = args
        .next()
        .ok_or("usage: control_surface_areas out.json [preset...]")?;
    let requested: Vec<String> = args.collect();
    let names: Vec<String> = if requested.is_empty() {
        presets::available()
            .into_iter()
            .map(str::to_owned)
            .collect()
    } else {
        requested
    };
    let rows: Vec<Value> = names
        .iter()
        .map(|name| row(name).unwrap_or_else(|e| json!({"preset": name, "reason": e.to_string()})))
        .collect();
    fs::write(&output, serde_json::to_string_pretty(&rows)? + "\n")?;
    println!("{output}");
    Ok(())
}
