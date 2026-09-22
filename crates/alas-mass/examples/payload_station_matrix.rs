// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where the lumped planning payload is placed, per registered aircraft.
//!
//! `alas_mass::stations`' payload fallback puts the payload's centroid at the
//! centre of an *occupied length* that always begins at the forward cabin
//! bulkhead, so a cabin the planning payload does not fill is loaded nose
//! first. This probe prints the cabin the model resolves, the occupied block,
//! where the two put the centroid, and what the difference is worth in centre
//! of gravity at the analysed take-off mass.
//!
//! SI: m, kg. `%MAC` uses the same mean aerodynamic chord the weight-and-
//! balance artifact reports.
//!
//! Run: `cargo run --release -p alas-mass --example payload_station_matrix`

#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;

fn main() {
    println!(
        "preset,cabin_start_m,cabin_len_m,occupied_len_m,fill_fraction,\
         payload_x_m,cabin_centre_x_m,delta_m,payload_kg,tow_kg,cg_shift_m"
    );
    for preset in presets::registry() {
        let Ok(config) = AlasConfig::from_value(&serde_json::json!({"preset": preset.name})) else {
            continue;
        };
        let Ok(plane) = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
        else {
            continue;
        };
        let Some(fuselage) = plane.fuselages.first() else {
            continue;
        };

        // The same arithmetic `stations::payload_fallback_station` applies.
        let length = fuselage
            .xsecs
            .last()
            .map(|section| section.xyz_c[0])
            .unwrap_or_default()
            - fuselage
                .xsecs
                .first()
                .map(|section| section.xyz_c[0])
                .unwrap_or_default();
        let cabin_start = config.geometry.fuselage.cabin_start_x_m;
        let cabin_len =
            (length - cabin_start - config.geometry.fuselage.tailcone_length_m).max(1.0);
        let payload_kg = config.requirements.payload_kg();
        let density = config.mass_model.cabin_payload_density_kg_m.max(1e-6);
        let occupied_len = cabin_len.min(payload_kg / density);
        let nose_first_x = cabin_start + 0.50 * occupied_len;
        let cabin_centre_x = cabin_start + 0.50 * cabin_len;
        let delta = cabin_centre_x - nose_first_x;
        let tow = config.requirements.mtow_kg;
        let cg_shift = delta * payload_kg / tow.max(1.0);
        println!(
            "{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.0},{:.0},{:.4}",
            preset.name,
            cabin_start,
            cabin_len,
            occupied_len,
            occupied_len / cabin_len,
            nose_first_x,
            cabin_centre_x,
            delta,
            payload_kg,
            tow,
            cg_shift,
        );
    }
}
