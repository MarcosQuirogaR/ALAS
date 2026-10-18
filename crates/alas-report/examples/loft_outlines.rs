// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Study harness (never committed): model outlines of presets with the fuselage from a loft.
//!
//! usage: loft_outlines <out_dir> [--loft-json <path>] [--legacy] [--nacelle-v2] <preset>...
//!
//! Same JSON layout as the older geometry_outlines harness (x aft from the nose tip, y right,
//! z up, metres; fuselage.sections carry x, half_width, height, zc, z_top, z_bot, shape = n, k),
//! plus `loft`: the exact Rust evaluation of the loft on a dense axis (top, bot, a, zc, e, n, k)
//! for point-by-point comparison with the Python mirror, the build report and, per nacelle, the
//! exact V2 figures. `--loft-json` replaces the preset's fuselage by that file (a lane-D fit),
//! built at the preset's fuselage length; the rest of the aircraft is the preset's own.

#![allow(clippy::print_stdout)]

use std::path::PathBuf;

use alas_config::{parse_loft_spec, WingShape};
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::wing::Wing;
use alas_geom::builder::{AircraftBuilder, POLYLINE_TOLERANCE_M};
use alas_geom::loft::{Loft, LoftParams};
use serde_json::{json, Value};

fn wing_json(w: &Wing) -> Value {
    let xs: Vec<Value> = w
        .xsecs
        .iter()
        .map(|s| {
            json!({"le": s.xyz_le, "chord": s.chord, "twist_deg": s.twist,
                   "te": [s.xyz_le[0] + s.chord, s.xyz_le[1], s.xyz_le[2]],
                   "airfoil": s.airfoil.name, "airfoil_xz": s.airfoil.coordinates})
        })
        .collect();
    json!({"name": w.name, "symmetric": w.symmetric, "xsecs": xs})
}

fn sections_json(f: &Fuselage) -> Vec<Value> {
    f.xsecs
        .iter()
        .map(|s| {
            // the shoulder warp k of the lofted section at this station (0 without a loft)
            let k = f
                .loft
                .as_ref()
                .map_or(0.0, |loft| loft.section(s.xyz_c[0]).k);
            json!({"x": s.xyz_c[0], "y": s.xyz_c[1], "zc": s.xyz_c[2],
                   "half_width": s.width / 2.0, "height": s.height, "shape": s.shape,
                   "n": s.shape, "k": k,
                   "z_top": s.xyz_c[2] + s.height / 2.0, "z_bot": s.xyz_c[2] - s.height / 2.0})
        })
        .collect()
}

/// Dense axis: uniform plus a cubic cluster at the nose where the profile is a square root.
fn dense_axis(length: f64) -> Vec<f64> {
    let mut xs: Vec<f64> = (0..=2000).map(|i| length * i as f64 / 2000.0).collect();
    xs.extend((0..40).map(|i| length * 0.02 * (i as f64 / 40.0).powi(3)));
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    xs
}

fn loft_json(loft: &Loft) -> Value {
    let rows: Vec<Value> = dense_axis(loft.length_m())
        .into_iter()
        .map(|x| {
            let s = loft.section(x);
            json!({"x": x, "top": s.zc + s.e, "bot": s.zc - s.e, "a": s.a, "zc": s.zc,
                   "e": s.e, "n": s.n, "k": s.k})
        })
        .collect();
    json!({"length_m": loft.length_m(), "wetted_area_m2": loft.wetted_area(),
           "volume_m3": loft.volume(), "dense": rows})
}

/// Side-view (XZ) area of a fin: sum of |dz| * mean chord over its stations (one panel).
fn side_area(w: &Wing) -> f64 {
    w.xsecs
        .windows(2)
        .map(|p| (p[1].xyz_le[2] - p[0].xyz_le[2]).abs() * (p[0].chord + p[1].chord) / 2.0)
        .sum()
}

/// Planform and tail-volume figures of the build (SI): built main-wing planform versus the
/// reference quantities the aircraft carries, tail areas and the volume coefficients
/// Vh = Sh lh / (S c), Vv = Sv lv / (S b) with the quarter-MAC arms.
fn metrics_json(plane: &alas_geom::aircraft::airplane::Airplane) -> Value {
    let main = &plane.wings[0];
    let hs = plane.wings.get(1);
    let vs = plane.wings.get(2);
    let ac = |w: &Wing| w.aerodynamic_center(0.25)[0];
    let sh = hs.map(Wing::projected_area);
    let sv = vs.map(side_area);
    let lh = hs.map(|w| ac(w) - ac(main));
    let lv = vs.map(|w| ac(w) - ac(main));
    let (s, c, b) = (plane.s_ref, plane.c_ref, plane.b_ref);
    json!({
        "built_main_projected_area_m2": main.projected_area(),
        "built_main_projected_span_m": main.projected_span(),
        "built_main_mac_m": main.mean_aerodynamic_chord(),
        "built_main_quarter_mac_x_m": ac(main),
        "s_ref_m2": s, "c_ref_m": c, "b_ref_m": b,
        "sh_m2": sh, "sv_m2": sv, "lh_m": lh, "lv_m": lv,
        "vh": sh.zip(lh).map(|(a, l)| a * l / (s * c)),
        "vv": sv.zip(lv).map(|(a, l)| a * l / (s * b)),
        "n_surfaces": plane.wings.len(),
    })
}

/// Largest `sqrt(w h)` and `pi/4 w h` over the stations (the legacy equivalent-diameter rule).
fn legacy_sections(f: &Fuselage) -> (f64, f64) {
    let wh = f
        .xsecs
        .iter()
        .map(|s| s.width * s.height)
        .fold(0.0, f64::max);
    (wh.sqrt(), std::f64::consts::FRAC_PI_4 * wh)
}

/// Main-gear station and ground plane the way the envelope code resolves them.
fn gear_frame(
    pr: &alas_config::presets::AircraftPreset,
    plane: &alas_geom::aircraft::airplane::Airplane,
) -> Option<(f64, f64)> {
    let f = plane.fuselages.first()?;
    let wing = plane.wings.first()?;
    let start = f.xsecs.first()?.xyz_c[0];
    let length = f.xsecs.last()?.xyz_c[0] - start;
    let mass_model = pr.mass_model.clone().unwrap_or_default();
    let nlg = start + length * mass_model.nlg_x_fraction;
    let mlg = wing.aerodynamic_center(0.0)[0]
        + mass_model.mlg_x_fraction_mac * wing.mean_aerodynamic_chord();
    let st = pr
        .landing_gear
        .resolved_station_positions(nlg, mlg, start, length);
    let ground = alas_mass::stations::ground_plane_z_m(f, &pr.geometry, &pr.landing_gear);
    Some((st.x_mlg_m, ground))
}

fn physics_json(
    pr: &alas_config::presets::AircraftPreset,
    legacy: &alas_geom::aircraft::airplane::Airplane,
    lofted: &alas_geom::aircraft::airplane::Airplane,
) -> Value {
    let lf = &legacy.fuselages[0];
    let nf = &lofted.fuselages[0];
    let length = |f: &Fuselage| {
        f.xsecs.last().map_or(0.0, |s| s.xyz_c[0]) - f.xsecs.first().map_or(0.0, |s| s.xyz_c[0])
    };
    let (d_eq_legacy, s_max_legacy) = legacy_sections(lf);
    let gear_legacy = gear_frame(pr, legacy);
    let gear_loft = gear_frame(pr, lofted);
    // legacy scrape: the envelope code's rule (stations aft of the gear, z_c - h/2)
    let legacy_scrape = |gear: Option<(f64, f64)>, f: &Fuselage| {
        gear.and_then(|(x, g)| {
            f.xsecs
                .iter()
                .filter(|s| s.xyz_c[0] > x)
                .map(|s| {
                    let dx = s.xyz_c[0] - x;
                    ((s.xyz_c[2] - s.height / 2.0 - g) / dx).atan().to_degrees()
                })
                .reduce(f64::min)
        })
    };
    let mut loft_part = Value::Null;
    if let Some(loft) = &nf.loft {
        let (x0, x1) = loft.x_range();
        let s_max = (0..=4000)
            .map(|i| loft.s_of_x(x0 + (x1 - x0) * i as f64 / 4000.0))
            .fold(0.0, f64::max);
        let d_eq = (4.0 * s_max / std::f64::consts::PI).sqrt();
        let scrape_own = gear_loft.and_then(|(x, g)| loft.tail_scrape_deg(x, g));
        let scrape_legacy_ground = gear_legacy.and_then(|(x, g)| loft.tail_scrape_deg(x, g));
        loft_part = json!({
            "length_m": loft.length_m(), "wetted_lofted_m2": loft.wetted_area(),
            "base_area_m2": loft.base_area(), "volume_m3": loft.volume(),
            "s_max_m2": s_max, "d_eq_m": d_eq, "fineness": loft.length_m() / d_eq,
            "tail_scrape_exact_deg": scrape_own, "tail_scrape_exact_at_legacy_gear_deg": scrape_legacy_ground,
            "tail_scrape_polyline_rule_deg": legacy_scrape(gear_loft, nf),
            "fairness": {
                "violations": loft.fairness().violations,
                "lines": loft.fairness().lines.iter().map(|l| json!({
                    "name": l.name, "inflections": l.inflections, "tv_ratio": l.tv_ratio,
                    "min_radius_hint_max_curvature": l.max_curvature, "knot_jump": l.knot_jump})).collect::<Vec<_>>(),
            },
        });
    }
    json!({
        "legacy": {
            "length_m": length(lf), "wetted_trapezoid_m2": lf.area_wetted(),
            "s_max_m2": s_max_legacy, "d_eq_m": d_eq_legacy, "fineness": length(lf) / d_eq_legacy,
            "tail_scrape_deg": legacy_scrape(gear_legacy, lf),
            "main_gear_x_m": gear_legacy.map(|g| g.0), "ground_z_m": gear_legacy.map(|g| g.1),
        },
        "loft": loft_part,
        "loft_build": {
            "wetted_trapezoid_of_polyline_m2": nf.area_wetted(),
            "main_gear_x_m": gear_loft.map(|g| g.0), "ground_z_m": gear_loft.map(|g| g.1),
        },
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let usage =
        "usage: loft_outlines <out_dir> [--loft-json <path>] [--legacy] [--nacelle-v2] <preset>...";
    let output = PathBuf::from(args.next().ok_or(usage)?);
    std::fs::create_dir_all(&output)?;
    let (mut override_json, mut v2, mut legacy, mut presets) =
        (None, None::<bool>, false, Vec::new());
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--loft-json" => override_json = Some(PathBuf::from(args.next().ok_or(usage)?)),
            "--nacelle-v2" => v2 = Some(true),
            "--nacelle-legacy" => v2 = Some(false),
            "--legacy" => legacy = true,
            _ => presets.push(arg),
        }
    }
    for preset in presets {
        let pr = alas_config::presets::get(&preset)?;
        let mut geometry = pr.geometry.clone();
        let preset_has_loft = geometry.fuselage.loft.is_some();
        if legacy {
            geometry.fuselage.loft = None;
        }
        let dv = pr.design_vector;
        let builder = nacelle_builder(AircraftBuilder::new(Some(geometry.clone())), v2);
        let (mut plane, report) = builder.build_reported(Some(&dv), true, WingShape::Flight)?;
        let mut loft_value = Value::Null;
        if let Some(path) = &override_json {
            let spec = parse_loft_spec(&std::fs::read_to_string(path)?)?;
            let loft = Loft::new(&spec, LoftParams::new(dv.fuselage_length_m))?;
            let sampled = loft.sample_xsecs_within(POLYLINE_TOLERANCE_M, 41, 400);
            let mut f = Fuselage::new("Fuselage", sampled.xsecs);
            f.loft = Some(std::sync::Arc::new(loft));
            plane.fuselages[0] = f;
            println!(
                "{preset}: loft {} -> {} stations, polyline deviation {:.2} mm",
                path.display(),
                plane.fuselages[0].xsecs.len(),
                1e3 * sampled.max_deviation_m
            );
        }
        if let Some(loft) = &plane.fuselages[0].loft {
            loft_value = loft_json(loft);
        }

        let physics =
            {
                let mut legacy_geometry = geometry.clone();
                legacy_geometry.fuselage.loft = None;
                let (legacy_plane, _) =
                    nacelle_builder(AircraftBuilder::new(Some(legacy_geometry)), v2)
                        .build_reported(Some(&dv), true, WingShape::Flight)?;
                physics_json(pr, &legacy_plane, &plane)
            };
        // main / hstab / vstab by build order; the dorsal fin and the wingtip device are named
        // surfaces appended after them and exported under their own keys so the overlay tools
        // can draw them (final evidence: devices and dorsals are part of the silhouette).
        let mut wings = serde_json::Map::new();
        for (w, key) in plane.wings.iter().zip(["main", "hstab", "vstab"]) {
            wings.insert(key.to_owned(), wing_json(w));
        }
        // geo/close 2026-10-12: the dorsal fillet is geometry-only (vstab_dorsal_in_aero false), so the product plane
        // does not carry it; build it for the overlay when configured and not already in the plane.
        if !plane.wings.iter().any(|w| w.name.contains("Dorsal")) {
            if let Some(w) = builder.build_dorsal_fin(&dv)? {
                wings.insert("dorsal".to_owned(), wing_json(&w));
            }
        }
        for w in plane.wings.iter().skip(3) {
            let key = if w.name.contains("Dorsal") {
                "dorsal"
            } else {
                "device"
            };
            wings.insert(key.to_owned(), wing_json(w));
        }
        let mut fuselage = Value::Null;
        let mut nacelles = Vec::new();
        for f in &plane.fuselages {
            let v = json!({"name": f.name, "sections": sections_json(f),
                           "area_wetted_trapezoid_m2": f.area_wetted(),
                           "area_wetted_lofted_m2": f.area_wetted_lofted()});
            let mut v = v;
            if let Some(loft) = &f.loft {
                // exact front silhouette of the loft (residual included): max over x of the half-width at height z
                let (x0, x1) = loft.x_range();
                let (zlo, zhi) = (-20.0_f64, 20.0_f64);
                let mut zs = Vec::new();
                let mut hw = Vec::new();
                let mut z = zlo;
                while z <= zhi {
                    let h = (0..=1500)
                        .map(|i| loft.half_width_at(x0 + (x1 - x0) * f64::from(i) / 1500.0, z))
                        .fold(0.0_f64, f64::max);
                    if h > 0.0 {
                        zs.push(z);
                        hw.push(h);
                    }
                    z += 0.02;
                }
                v["front_envelope"] = json!({"z": zs, "half_width": hw,
                    "has_residual": loft.spec().residual.is_some()});
            }
            if f.name == "Fuselage" {
                fuselage = v;
            } else {
                nacelles.push(v);
            }
        }
        let pf = geometry.wing.transport_planform(&dv)?;
        let station = |q: &alas_config::MainWingStation| json!({"chord": q.chord_m, "le_x": q.leading_edge_x_m, "y": q.y_m});
        let planform = json!({
            "x_wing_global_m": geometry.wing.root_datum_x_m + dv.wing_x_shift_m,
            "root": station(&pf.root), "kink": station(&pf.kink), "tip": station(&pf.tip),
            "side_of_body": pf.side_of_body.as_ref().map(station),
            "root_le_x": pf.root.leading_edge_x_m, "kink_le_x": pf.kink.leading_edge_x_m,
            "inboard_le_sweep_deg": pf.inboard_le_sweep_deg,
            "outboard_le_sweep_deg": pf.outboard_le_sweep_deg,
        });
        let out = json!({
            "preset": preset,
            "metrics": metrics_json(&plane),
            "axes": "x aft from nose tip, y right, z up (fuselage-geometry axes), metres",
            "nacelle_v2_override": v2, "nacelle_model": format!("{:?}", geometry.engine.nacelle_model),
            "loft_json_override": override_json.as_ref().map(|p| p.display().to_string()),
            "design_vector": serde_json::to_value(dv)?,
            "reference": {"s_ref": plane.s_ref, "c_ref": plane.c_ref, "b_ref": plane.b_ref},
            "wings": wings, "planform_stations_pre_shift": planform,
            "fuselage": fuselage, "nacelles": nacelles, "loft": loft_value,
            "physics": physics, "legacy_forced": legacy, "preset_has_loft": preset_has_loft,
            "build_report": {
                "warnings": report.warnings.iter()
                    .map(|w| json!({"code": w.code, "message": w.message})).collect::<Vec<_>>(),
                "loft": report.loft.as_ref().map(|l| json!({
                    "id": l.loft_id, "stations": l.stations,
                    "polyline_deviation_m": l.polyline_deviation_m,
                    "max_width_m": l.max_width_m, "max_height_m": l.max_height_m})),
            },
        });
        std::fs::write(
            output.join(format!("{preset}.json")),
            serde_json::to_string_pretty(&out)?,
        )?;
        println!("{preset}: ok");
    }
    Ok(())
}

/// Without an override the build follows the preset's `engine.nacelle_model`.
fn nacelle_builder(builder: AircraftBuilder, v2: Option<bool>) -> AircraftBuilder {
    match v2 {
        Some(on) => builder.with_nacelle_v2(on),
        None => builder,
    }
}
