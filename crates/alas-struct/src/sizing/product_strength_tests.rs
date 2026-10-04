// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use alas_config::{materials, presets, AlasConfig};
use alas_geom::{builder::AircraftBuilder, wing_structure::WingStructureGeometry};

fn probe(name: &str) -> (AlasConfig, WingStructureGeometry) {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
        .unwrap_or_else(|error| panic!("{name} loads: {error}"));
    let design = presets::registry()
        .iter()
        .find(|preset| preset.name == name)
        .map(|preset| preset.design_vector)
        .unwrap_or_else(|| panic!("{name} is registered"));
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), false)
        .unwrap_or_else(|error| panic!("{name} builds: {error}"));
    let wing = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .unwrap_or_else(|| panic!("{name} has a main wing"));
    let root = wing.xsecs.first().expect("a root section");
    let tip = wing.xsecs.last().expect("a tip section");
    let (fractions, full_span) = config.structures.resolved_spars();
    let geometry = WingStructureGeometry::new(
        &design,
        &config.geometry.wing,
        &root.airfoil,
        &tip.airfoil,
        &fractions,
        Some(&full_span),
    )
    .unwrap_or_else(|error| panic!("{name} wing builds: {error}"));
    (config, geometry)
}

fn size_probe(config: &AlasConfig, geometry: &WingStructureGeometry) -> WingboxSizing {
    let cfg = &config.structures;
    super::super::size_wingbox(
        geometry,
        cfg,
        &config.requirements,
        materials::get(&cfg.skin_material).expect("skin material"),
        materials::get(&cfg.spar_web_material).expect("web material"),
        materials::get(&cfg.spar_cap_material).expect("cap material"),
        materials::get(&cfg.rib_material).expect("rib material"),
    )
}

fn assert_sizing_bits_equal(actual: &WingboxSizing, expected: &WingboxSizing, label: &str) {
    let equal_bits = |left: &[f64], right: &[f64]| {
        for (index, (left, right)) in left.iter().zip(right).enumerate() {
            assert_eq!(left.to_bits(), right.to_bits(), "{label}, element {index}");
        }
    };
    equal_bits(&actual.y_stations, &expected.y_stations);
    equal_bits(&actual.eta_stations, &expected.eta_stations);
    equal_bits(&actual.chord, &expected.chord);
    equal_bits(&actual.spar_fracs, &expected.spar_fracs);
    equal_bits(
        &[
            actual.t_skin,
            actual.rib_spacing_m,
            actual.total_mass_kg,
            actual.mass_breakdown_kg.spar_caps,
            actual.mass_breakdown_kg.spar_webs,
            actual.mass_breakdown_kg.skin,
            actual.mass_breakdown_kg.ribs,
        ],
        &[
            expected.t_skin,
            expected.rib_spacing_m,
            expected.total_mass_kg,
            expected.mass_breakdown_kg.spar_caps,
            expected.mass_breakdown_kg.spar_webs,
            expected.mass_breakdown_kg.skin,
            expected.mass_breakdown_kg.ribs,
        ],
    );
    for (actual, expected) in actual.spars.iter().zip(&expected.spars) {
        equal_bits(
            &[actual.chord_fraction, actual.t_web],
            &[expected.chord_fraction, expected.t_web],
        );
        equal_bits(&actual.h, &expected.h);
        equal_bits(&actual.w_cap, &expected.w_cap);
        equal_bits(&actual.t_cap, &expected.t_cap);
        equal_bits(&actual.a_cap, &expected.a_cap);
        equal_bits(&actual.frac_moment, &expected.frac_moment);
        equal_bits(&actual.margin_of_safety, &expected.margin_of_safety);
    }
    assert_eq!(actual, expected, "{label}");
}

#[test]
fn narrowed_cap_search_replays_the_legacy_section_bit_for_bit() {
    for name in [
        "A320-200",
        "ATR72-600",
        "AVE",
        "B787-9",
        "A380-800",
        "A340-300",
        "A220-300",
        "DC-10",
    ] {
        let (config, geometry) = probe(name);
        let cfg = &config.structures;
        let skin = materials::get(&cfg.skin_material).expect("skin material");
        let web = materials::get(&cfg.spar_web_material).expect("web material");
        let cap = materials::get(&cfg.spar_cap_material).expect("cap material");
        let sizing = size_probe(&config, &geometry);
        for load_factor in [0.0, 0.01, 0.5, 1.0, 2.0, 100.0] {
            let load = crate::loads::elliptic_distributed_load(
                &sizing.y_stations,
                geometry.semi_span,
                load_factor * config.requirements.mtow_kg * config.requirements.gravity_m_s2,
            );
            let (shears, moments) =
                crate::loads::cantilever_shear_moment(&sizing.y_stations, &load);
            let mut expected = sizing.clone();
            for spar in &mut expected.spars {
                spar.t_web = cfg.t_web_min_m;
            }
            let mut actual = expected.clone();
            super::reference::size(&mut expected, &moments, &shears, cfg, skin, web, cap);
            super::size(&mut actual, &moments, &shears, cfg, skin, web, cap);
            assert_sizing_bits_equal(
                &actual,
                &expected,
                &format!("{name}, load factor {load_factor}"),
            );
        }
    }
}

#[test]
fn narrowed_cap_search_preserves_floor_widening_and_packaging_limits() {
    let (config, geometry) = probe("A320-200");
    let cfg = &config.structures;
    let skin = materials::get(&cfg.skin_material).expect("skin material");
    let web = materials::get(&cfg.spar_web_material).expect("web material");
    let cap = materials::get(&cfg.spar_cap_material).expect("cap material");
    let sizing = size_probe(&config, &geometry);
    for chord in [0.1, 2.0, 10.0] {
        for height in [0.001, 0.1, 1.0] {
            for minimum_gauge in [0.0, 1.0e-6, 0.005, 0.1] {
                for moment in [0.0, 1.0, 1.0e3, 1.0e6, 1.0e12] {
                    let mut cfg = cfg.clone();
                    cfg.t_skin_min_m = minimum_gauge;
                    let mut expected = sizing.clone();
                    expected.chord[0] = chord;
                    expected.t_skin = minimum_gauge;
                    for (index, spar) in expected.spars.iter_mut().enumerate() {
                        spar.h[0] = height * 0.5_f64.powi(index as i32);
                    }
                    let mut actual = expected.clone();
                    super::reference::size_station_caps(
                        &mut expected,
                        0,
                        moment,
                        &cfg,
                        skin,
                        web,
                        cap,
                    );
                    super::size_station_caps(&mut actual, 0, moment, &cfg, skin, web, cap);
                    assert_sizing_bits_equal(
                        &actual,
                        &expected,
                        &format!(
                        "chord={chord}, height={height}, floor={minimum_gauge}, moment={moment}"),
                    );
                }
            }
        }
    }
}

#[test]
fn exhausted_cap_bracket_retains_the_last_evaluated_section() {
    let (config, geometry) = probe("A320-200");
    let mut cfg = config.structures.clone();
    cfg.t_skin_min_m = 0.0;
    let skin = materials::get(&cfg.skin_material).expect("skin material");
    let web = materials::get(&cfg.spar_web_material).expect("web material");
    let cap = materials::get(&cfg.spar_cap_material).expect("cap material");
    let mut expected = size_probe(&config, &geometry);
    expected.chord[0] = 10_000.0;
    expected.t_skin = 0.0;
    for spar in &mut expected.spars {
        spar.h[0] = 10_000.0;
    }
    let mut actual = expected.clone();
    super::reference::size_station_caps(&mut expected, 0, 1.0e40, &cfg, skin, web, cap);
    super::size_station_caps(&mut actual, 0, 1.0e40, &cfg, skin, web, cap);
    assert_sizing_bits_equal(&actual, &expected, "exhausted doubling bracket");
    let area = 1.0e-12 * 2.0_f64.powi(63);
    let (width, thickness) = super::station_cap_dimensions(area, 10_000.0, 10_000.0, 0.0);
    assert_eq!(actual.spars[0].w_cap[0].to_bits(), width.to_bits());
    assert_eq!(actual.spars[0].t_cap[0].to_bits(), thickness.to_bits());
}

#[test]
#[ignore = "single-thread release timing probe; run explicitly with --nocapture"]
// Explicitly requested timing probe owns its console output.
#[allow(clippy::print_stdout)]
fn wingbox_sizing_microtiming() {
    for name in ["A320-200", "ATR72-600", "AVE", "B787-9"] {
        let (config, geometry) = probe(name);
        std::hint::black_box(size_probe(&config, &geometry));
        let mut best = f64::INFINITY;
        for _ in 0..3 {
            let start = std::time::Instant::now();
            for _ in 0..5 {
                std::hint::black_box(size_probe(&config, &geometry));
            }
            best = best.min(start.elapsed().as_secs_f64() / 5.0);
        }
        let sizing = size_probe(&config, &geometry);
        println!(
            "{name}: {:.6} ms, total_mass_kg={:.17}",
            best * 1000.0,
            sizing.total_mass_kg
        );
    }
}

#[test]
#[ignore = "single-thread release timing probe; run explicitly with --nocapture"]
// Explicitly requested timing probe owns its console output.
#[allow(clippy::print_stdout)]
fn product_strength_microtiming() {
    for name in ["A320-200", "ATR72-600", "AVE", "B787-9"] {
        let (config, geometry) = probe(name);
        let cfg = &config.structures;
        let skin = materials::get(&cfg.skin_material).expect("skin material");
        let web = materials::get(&cfg.spar_web_material).expect("web material");
        let cap = materials::get(&cfg.spar_cap_material).expect("cap material");
        let sizing = size_probe(&config, &geometry);
        let load = crate::loads::elliptic_distributed_load(
            &sizing.y_stations,
            geometry.semi_span,
            config.requirements.mtow_kg
                * config.requirements.gravity_m_s2
                * config.requirements.ultimate_load_factor
                * cfg.additional_safety_factor,
        );
        let (shears, moments) = crate::loads::cantilever_shear_moment(&sizing.y_stations, &load);
        for legacy in [true, false] {
            let mut candidate = sizing.clone();
            let mut best = f64::INFINITY;
            for _ in 0..3 {
                let start = std::time::Instant::now();
                for _ in 0..5 {
                    for spar in &mut candidate.spars {
                        spar.t_web = cfg.t_web_min_m;
                    }
                    if legacy {
                        super::reference::size(
                            &mut candidate,
                            &moments,
                            &shears,
                            cfg,
                            skin,
                            web,
                            cap,
                        );
                    } else {
                        super::size(&mut candidate, &moments, &shears, cfg, skin, web, cap);
                    }
                    std::hint::black_box(&candidate);
                }
                best = best.min(start.elapsed().as_secs_f64() / 5.0);
            }
            println!(
                "{name}: legacy={legacy}, strength_pass_ms={:.6}",
                best * 1000.0
            );
        }
    }
}
