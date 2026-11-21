// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Spar-box fuel volume of every preset against its published usable fuel.
#![allow(clippy::print_stdout, clippy::expect_used, clippy::unwrap_used)]

use alas_config::{presets, AlasConfig, FuelTankLayoutConfig, WingTankConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::tanks::{FuelTankLayout, TankKind};

fn main() {
    println!("preset,sob_frac,box_geo_L,box_usable_L,layout_wing_geo_usable_L,layout_wing_published_L,layout_total_geo_usable_L,published_total_L,wing_cells");
    for name in presets::available() {
        let preset = presets::get(name).expect("preset");
        let config =
            AlasConfig::from_value(&serde_json::json!({ "preset": name })).expect("config");
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("plane");
        let density = preset
            .reference
            .fuel_density_kg_l
            .map_or(800.0, |d| d * 1000.0);
        let published_l = preset.reference.usable_fuel_volume_l.or_else(|| {
            preset
                .reference
                .usable_fuel_mass_kg
                .map(|kg| kg / density * 1000.0)
        });
        // Whole box, side of body to tip, one cell.
        let sob = config
            .geometry
            .wing
            .side_of_body_span_fraction
            .unwrap_or(f64::NAN);
        let mut config = config;
        if let Ok(s) = std::env::var("SPARS") {
            let v: Vec<f64> = s.split(',').map(|x| x.parse().unwrap()).collect();
            config.structures.spar_chord_fractions = v;
        }
        let start_root = std::env::var("ROOT").is_ok();
        let mut whole = config.fuel_tanks.clone();
        whole.mid_wing.enabled = false;
        whole.outer_wing.enabled = false;
        whole.center.enabled = false;
        whole.trim.enabled = false;
        whole.auxiliary.enabled = false;
        whole.calibrate_to_published_capacity = false;
        whole.inner_wing = WingTankConfig {
            enabled: true,
            span_start_fraction: if start_root {
                0.0
            } else if sob.is_finite() {
                sob
            } else {
                0.1
            },
            span_end_fraction: 1.0,
            usable_fraction: 0.92,
            published_usable_volume_l: None,
            ..WingTankConfig::default()
        };
        let w = FuelTankLayout::resolve(
            &plane,
            &config.geometry,
            &config.structures,
            &whole,
            &config.fuel_policy,
            density,
            None,
        );
        let (geo, usable) = match &w {
            Ok(l) => (
                l.tanks().iter().map(|t| t.geometric_volume_m3).sum::<f64>() * 1e3,
                l.tanks().iter().map(|t| t.usable_volume_m3).sum::<f64>() * 1e3,
            ),
            Err(_) => (f64::NAN, f64::NAN),
        };
        // Registered layout, published stripped.
        let mut g: FuelTankLayoutConfig = config.fuel_tanks.clone();
        let wing_pub: f64 = [&g.inner_wing, &g.mid_wing, &g.outer_wing]
            .iter()
            .filter(|c| c.enabled)
            .map(|c| c.published_usable_volume_l.unwrap_or(f64::NAN))
            .sum();
        let cells = [&g.inner_wing, &g.mid_wing, &g.outer_wing]
            .iter()
            .filter(|c| c.enabled)
            .count();
        for c in [&mut g.inner_wing, &mut g.mid_wing, &mut g.outer_wing] {
            c.published_usable_volume_l = None;
            c.feed.published_usable_volume_l = None;
        }
        g.center.published_usable_volume_l = None;
        g.trim.published_usable_volume_l = None;
        g.calibrate_to_published_capacity = false;
        let r = FuelTankLayout::resolve(
            &plane,
            &config.geometry,
            &config.structures,
            &g,
            &config.fuel_policy,
            density,
            None,
        );
        let (wing_l, tot_l) = match &r {
            Ok(l) => (
                l.tanks()
                    .iter()
                    .filter(|t| {
                        matches!(
                            t.kind,
                            TankKind::WingInner
                                | TankKind::WingMid
                                | TankKind::WingOuter
                                | TankKind::WingFeed
                        )
                    })
                    .map(|t| t.usable_volume_m3)
                    .sum::<f64>()
                    * 1e3,
                l.tanks().iter().map(|t| t.usable_volume_m3).sum::<f64>() * 1e3,
            ),
            Err(_) => (f64::NAN, f64::NAN),
        };
        println!(
            "{name},{sob:.4},{geo:.0},{usable:.0},{wing_l:.0},{wing_pub:.0},{tot_l:.0},{},{cells}",
            published_l.map_or("NA".into(), |v| format!("{v:.0}"))
        );
    }
}
