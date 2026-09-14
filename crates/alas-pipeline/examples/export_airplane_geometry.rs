// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Dump one built aircraft's lifting geometry as JSON, for cross-checking the
//! native vortex lattice against AeroSandbox on the *same* wing.
//!
//! `AircraftBuilder::build` already applies the geometry configuration's own
//! spanwise subdivision, so what this writes is exactly the `Airplane` the
//! VLM meshes -- cross-section stations, chords, twists and the resolved
//! airfoil coordinates. Rebuilding that in Python from a preset name instead
//! would reintroduce every difference the comparison is meant to isolate.
//!
//! Usage: `export_airplane_geometry <preset> <output.json>`

#![allow(clippy::print_stdout)]

use std::collections::BTreeMap;

use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;
use serde_json::{json, Value};

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let preset = args.next().unwrap_or_else(|| "AVE".to_owned());
    let output = args
        .next()
        .unwrap_or_else(|| format!("{preset}-geometry.json"));

    let config =
        AlasConfig::from_value(&json!({ "preset": preset })).map_err(|error| error.to_string())?;
    let design = alas_config::presets::get(&preset)
        .map(|entry| entry.design_vector)
        .map_err(|error| error.to_string())?;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .map_err(|error| error.to_string())?;

    // Airfoil coordinate sets repeat across cross-sections; emit each once and
    // reference it by index so the file stays a readable size.
    let mut airfoils: Vec<Value> = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();

    let wings: Vec<Value> = plane
        .wings
        .iter()
        .map(|wing| {
            let xsecs: Vec<Value> = wing
                .xsecs
                .iter()
                .map(|xsec| {
                    // Name alone is not identity: two cross-sections can share
                    // a name after a blend or a design-vector reshape, so the
                    // coordinates themselves key the table.
                    let key = format!("{}|{:?}", xsec.airfoil.name, xsec.airfoil.coordinates);
                    let index = *seen.entry(key).or_insert_with(|| {
                        airfoils.push(json!({
                            "name": xsec.airfoil.name,
                            "coordinates": xsec.airfoil.coordinates
                                .iter()
                                .map(|&(x, y)| [x, y])
                                .collect::<Vec<_>>(),
                        }));
                        airfoils.len() - 1
                    });
                    json!({
                        "xyz_le": xsec.xyz_le,
                        "chord": xsec.chord,
                        "twist": xsec.twist,
                        "airfoil": index,
                    })
                })
                .collect();
            json!({
                "name": wing.name,
                "symmetric": wing.symmetric,
                "xsecs": xsecs,
            })
        })
        .collect();

    let payload = json!({
        "preset": preset,
        "name": plane.name,
        "xyz_ref": plane.xyz_ref,
        "s_ref": plane.s_ref,
        "c_ref": plane.c_ref,
        "b_ref": plane.b_ref,
        "cruise_mach": config.requirements.cruise_mach,
        "cruise_altitude_m": config.requirements.cruise_altitude_m,
        "airfoils": airfoils,
        "wings": wings,
    });

    let text = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?;
    std::fs::write(&output, text).map_err(|error| error.to_string())?;
    println!(
        "{preset}: {} wings, {} cross-sections, {} airfoil sets -> {output}",
        plane.wings.len(),
        plane.wings.iter().map(|w| w.xsecs.len()).sum::<usize>(),
        airfoils.len(),
    );
    Ok(())
}
