// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A fillet is a blend body, not wing. The offsets of the stations that make it are data
//! for a drawn blend, so scaling them changes nothing that reads the wing.
//!
//! The offsets scaled are those of every edge station inboard of the start of its edge's
//! straight run, and those at the side of body. `f = 0` removes the fillet and `f = 1.5`
//! makes it half as large again. Metres, square metres, kilograms and degrees.

// A failed expect is the assertion failing on a registered preset. The
// printed lines are the per-quantity status, read with --nocapture.
#![allow(clippy::expect_used, clippy::print_stdout)]

use alas_config::presets::{preset_breakdown, registry, AircraftPreset};
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::calculate_flops_mass_buildup;
use alas_mass::tanks::resolve_product_layout;

/// Above this relative difference a quantity has moved. Far below anything physical and
/// above the rounding of a chain of arithmetic.
const MOVED: f64 = 1.0e-9;

/// One named group of numbers an analysis reads.
type Snapshot = Vec<(String, Vec<f64>)>;

/// The numbers in a `Debug` rendering, so a whole result can be compared without listing
/// its fields. A different shape (`None` for `Some`) shows as a different count.
fn debug_numbers(text: &str) -> Vec<f64> {
    let mut numbers = Vec::new();
    let mut token = String::new();
    for character in text.chars().chain(std::iter::once(' ')) {
        if character.is_ascii_digit() || matches!(character, '.' | '-' | 'e' | '+') {
            token.push(character);
        } else {
            if token
                .as_bytes()
                .first()
                .is_some_and(|b| b.is_ascii_digit() || *b == b'-')
            {
                if let Ok(number) = token.parse::<f64>() {
                    numbers.push(number);
                }
            }
            token.clear();
        }
    }
    numbers
}

fn sections(wing: &Wing) -> Vec<f64> {
    wing.xsecs
        .iter()
        .flat_map(|xsec| {
            [
                xsec.xyz_le[0],
                xsec.xyz_le[1],
                xsec.xyz_le[2],
                xsec.chord,
                xsec.twist,
            ]
        })
        .collect()
}

/// The fillet of `preset` with its offsets scaled by `factor`, `None` when it has none.
fn with_fillet_scaled(preset: &AircraftPreset, factor: f64) -> Option<AircraftPreset> {
    let wing = &preset.geometry.wing;
    let planform = wing.transport_planform(&preset.design_vector).ok()?;
    let lines = planform.inboard_edge_lines()?;
    let side_of_body = planform.side_of_body?;
    let semi_span_m = planform.tip.y_m;
    let mut scaled = preset.clone();
    let mut touched = false;
    for station in &mut scaled.geometry.wing.edge_stations {
        let y_m = station.span_fraction * semi_span_m;
        let at_side_of_body = (station.span_fraction - side_of_body.span_fraction).abs() < 1.0e-6;
        if (y_m < lines.leading.run_start_y_m - 1.0e-6 || at_side_of_body)
            && station.le_offset_m != 0.0
        {
            station.le_offset_m *= factor;
            touched = true;
        }
        if (y_m < lines.trailing.run_start_y_m - 1.0e-6 || at_side_of_body)
            && station.te_offset_m != 0.0
        {
            station.te_offset_m *= factor;
            touched = true;
        }
    }
    touched.then_some(scaled)
}

fn build(preset: &AircraftPreset) -> (AlasConfig, Airplane) {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))
        .expect("registered configuration");
    config.geometry = preset.geometry.clone();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    (config, plane)
}

/// Everything the wing's analyses read, apart from the lofted sections themselves.
fn snapshot(preset: &AircraftPreset, config: &AlasConfig, plane: &Airplane) -> Snapshot {
    let wing = plane.main_wing().expect("main wing");
    let mac_frame = plane.mac_frame().expect("MAC frame");
    let carried = wing.carried_box();
    let clean = preset
        .geometry
        .wing
        .transport_planform(&preset.design_vector)
        .expect("planform")
        .without_blends();
    let high_lift = preset_breakdown(preset);
    let layout = resolve_product_layout(config, &preset.design_vector, plane);
    let masses = calculate_flops_mass_buildup(
        plane,
        &config.requirements,
        &config.geometry,
        &config.control_surfaces,
        Some(&config.mass_model),
        &config.landing_gear,
        &config.cabin,
    );
    vec![
        (
            "S_ref, c_ref, b_ref".to_owned(),
            vec![plane.s_ref, plane.c_ref, plane.b_ref],
        ),
        (
            "wing reference area".to_owned(),
            vec![wing.reference_area(), wing.reference_mac()],
        ),
        (
            "%MAC frame".to_owned(),
            vec![mac_frame.chord_m, mac_frame.x_lemac_m],
        ),
        ("carried box".to_owned(), sections(&carried)),
        (
            "clean stations".to_owned(),
            clean
                .stations_with_rake()
                .iter()
                .flat_map(|station| [station.y_m, station.leading_edge_x_m, station.chord_m])
                .collect(),
        ),
        ("tanks".to_owned(), debug_numbers(&format!("{layout:?}"))),
        (
            "FLOPS inputs and masses (SFLAP included)".to_owned(),
            debug_numbers(&format!("{masses:?}")),
        ),
        (
            "registered CLmax".to_owned(),
            high_lift.map_or_else(Vec::new, |breakdown| {
                debug_numbers(&format!("{breakdown:?}"))
            }),
        ),
    ]
}

/// The first number of `after` that moved from `before`, with its relative difference.
fn moved(before: &[f64], after: &[f64]) -> Option<String> {
    if before.len() != after.len() {
        return Some(format!("{} numbers became {}", before.len(), after.len()));
    }
    before
        .iter()
        .zip(after)
        .map(|(a, b)| (a, b, (a - b).abs() / a.abs().max(b.abs()).max(1.0e-30)))
        .filter(|(a, b, relative)| *relative > MOVED && (*a - *b).abs() > 1.0e-12)
        .max_by(|left, right| left.2.total_cmp(&right.2))
        .map(|(a, b, relative)| format!("{a} -> {b} ({:+.4} %)", relative * 100.0))
}

#[test]
fn scaling_a_fillet_changes_nothing_that_reads_the_wing() {
    let mut failures = Vec::new();
    let mut with_fillet = 0;
    for preset in registry() {
        let (config, plane) = build(preset);
        let reference = snapshot(preset, &config, &plane);
        let reference_sections = sections(plane.main_wing().expect("main wing"));
        for factor in [0.0, 1.5] {
            let Some(scaled) = with_fillet_scaled(preset, factor) else {
                continue;
            };
            with_fillet += 1;
            let (scaled_config, scaled_plane) = build(&scaled);
            let scaled_sections = sections(scaled_plane.main_wing().expect("main wing"));
            let mut results = vec![(
                "lofted sections",
                moved(&reference_sections, &scaled_sections),
                reference_sections.len(),
            )];
            let after = snapshot(&scaled, &scaled_config, &scaled_plane);
            for ((name, before), (_, after)) in reference.iter().zip(&after) {
                results.push((name.as_str(), moved(before, after), before.len()));
            }
            for (name, result, count) in results {
                let status = result.as_deref().unwrap_or("unchanged");
                println!("{} f={factor}: {name} [{count}]: {status}", preset.name);
                if result.is_some() {
                    failures.push(format!("{} f={factor} {name}: {status}", preset.name));
                }
            }
        }
    }
    assert!(
        with_fillet > 0,
        "no registered preset has a fillet to scale"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
