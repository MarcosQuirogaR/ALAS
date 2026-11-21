// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rotated-normal tail incidence against the exact lattice rebuild, and the
//! product trim with and without the rotated-normal stage.
//!
//! Usage: normal_rotation_probe OUTPUT_JSON [PRESET ...]
//!
//! Per preset (fine mesh of the full-fidelity screening controls):
//! 1. `model_error`: for tail incidence changes `d` from the built one, the
//!    CL and Cm of the exact rebuild and of the rotated-normal solve on the
//!    lattice built at the original incidence.
//! 2. `trims`: `stability_and_trim_with_cache` with the rotated-normal stage
//!    off and on, each on a fresh cache and each run twice (the second run
//!    is timed, the first warms the allocator): trimmed values, wall time
//!    and lattice assemblies.

#![allow(clippy::print_stdout)]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::{speed, VlmGeometryCache, VlmSystem};
use alas_atmo::Atmosphere;
use alas_config::{AlasConfig, TailSizing};
use alas_geom::aircraft::airplane::Airplane;
use alas_opt::ScreeningFidelity;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::{error::Error, fs, time::Instant};

const HSTAB: &str = "Horizontal Stabilizer";

fn tail_at(plane: &Airplane, incidence_deg: f64) -> Airplane {
    let mut out = plane.clone();
    for wing in out.wings.iter_mut().filter(|w| w.name == HSTAB) {
        for xsec in &mut wing.xsecs {
            xsec.twist = incidence_deg;
        }
        wing.twist_about_pitch_axis = true;
    }
    out
}

fn counters() -> Value {
    let report = alas_math::stage_profile::report();
    let get = |key: &str| {
        report
            .counters
            .iter()
            .find(|c| c.0 == key)
            .map_or(0, |c| c.1)
    };
    json!({
        "assemblies": get("vlm_assemblies"), "cold": get("vlm_cold_assemblies"),
        "solves": get("vlm_solves"),
    })
}

fn row(preset: &str) -> Result<Value, Box<dyn Error>> {
    let config = AlasConfig::from_value(&json!({"preset": preset}))?;
    let mut config = ScreeningFidelity::full().configure(&config);
    let mut dv = alas_config::presets::get(preset)?.design_vector;
    let sizing = TailSizing::of(&config.geometry.empennage, &dv);
    let plane = alas_opt::mdo::rebuild_airplane(&mut config, &mut dv, &sizing)?;
    let analysis = &config.analysis;
    let req = &config.requirements;
    let atmosphere = Atmosphere::new(req.cruise_altitude_m);
    let velocity = req.cruise_mach * atmosphere.speed_of_sound();
    let (sp, ch) = (
        analysis.spanwise_resolution as usize,
        analysis.chordwise_resolution as usize,
    );
    let tail_indices: Vec<usize> = plane
        .wings
        .iter()
        .enumerate()
        .filter(|(_, w)| w.name == HSTAB)
        .map(|(i, _)| i)
        .collect();
    let i0 = plane
        .wings
        .iter()
        .find(|w| w.name == HSTAB)
        .and_then(|w| w.xsecs.first())
        .map_or(0.0, |x| x.twist);

    // 1. Model error against the exact rebuild.
    let reference_plane = tail_at(&plane, i0);
    let reference = VlmSystem::assemble(&reference_plane, sp, ch)?;
    let mut model_rows = Vec::new();
    for alpha in [2.0, 5.0] {
        let op = OperatingPoint::new(atmosphere, velocity, alpha, 0.0, 0.0, 0.0, 0.0);
        let base = reference.solve(&op)?;
        for d in [-4.0_f64, -2.0, -1.0, -0.1, 0.1, 1.0, 2.0, 4.0] {
            let rotated = reference.solve_with_surface_pitch(&op, &tail_indices, d.to_radians())?;
            let rebuilt_plane = tail_at(&plane, i0 + d);
            let rebuilt = VlmSystem::assemble(&rebuilt_plane, sp, ch)?.solve(&op)?;
            let dcl_exact = rebuilt.cl_lift - base.cl_lift;
            let dcm_exact = rebuilt.cm_pitch - base.cm_pitch;
            model_rows.push(json!({
                "alpha_deg": alpha, "d_incidence_deg": d,
                "cl_exact": rebuilt.cl_lift, "cl_model": rotated.cl_lift,
                "cm_exact": rebuilt.cm_pitch, "cm_model": rotated.cm_pitch,
                "cl_rel_err": (rotated.cl_lift - rebuilt.cl_lift) / rebuilt.cl_lift,
                "cm_abs_err": rotated.cm_pitch - rebuilt.cm_pitch,
                "cm_err_over_cm_change": (rotated.cm_pitch - rebuilt.cm_pitch) / dcm_exact,
                "cl_err_over_cl_change": (rotated.cl_lift - rebuilt.cl_lift) / dcl_exact,
            }));
        }
    }

    // 2. The product trim, rotated-normal stage off and on.
    alas_math::stage_profile::enable();
    let cl_target = 0.5;
    let mut trims = Vec::new();
    for on in [false, true] {
        speed::set_normal_rotation_trim(Some(on));
        let mut last = Value::Null;
        for rep in 0..2 {
            alas_math::stage_profile::reset();
            let cache = Arc::new(Mutex::new(VlmGeometryCache::default()));
            let started = Instant::now();
            let trim = alas_stab::trim::stability_and_trim_with_cache(
                &plane,
                analysis,
                cl_target,
                req.cruise_mach,
                req.cruise_altitude_m,
                &cache,
            )?;
            let wall = started.elapsed().as_secs_f64();
            if rep == 1 {
                last = json!({
                    "rotated_normal_stage": on, "wall_s": wall, "counters": counters(),
                    "converged": trim.converged, "x_np": trim.x_np,
                    "static_margin": trim.static_margin,
                    "cl_alpha": trim.cl_alpha, "cm_alpha": trim.cm_alpha,
                    "trim_alpha_deg": trim.trim_alpha_deg, "trim_ih_deg": trim.trim_ih_deg,
                    "cl_ih": trim.cl_ih, "cm_ih": trim.cm_ih,
                    "compressible": trim.compressible.map(|c| json!({
                        "converged": c.converged, "x_np": c.x_np,
                        "trim_alpha_deg": c.trim_alpha_deg, "trim_ih_deg": c.trim_ih_deg,
                        "cl_ih": c.cl_ih, "cm_ih": c.cm_ih,
                        "jacobian_at_trim": c.jacobian_at_trim,
                    })),
                });
            }
        }
        trims.push(last);
    }
    speed::set_normal_rotation_trim(None);
    Ok(json!({
        "preset": preset, "panels": reference.panel_count(),
        "built_tail_incidence_deg": i0,
        "model_error": model_rows, "trims": trims,
    }))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        return Err("usage: normal_rotation_probe OUTPUT_JSON [PRESET ...]".into());
    }
    let presets: Vec<String> = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        ["A320-200", "B747-400", "ATR72-600", "A400M"]
            .map(str::to_owned)
            .to_vec()
    };
    let mut rows = Vec::new();
    for preset in &presets {
        rows.push(row(preset)?);
    }
    fs::write(&args[1], serde_json::to_vec_pretty(&json!({"rows": rows}))?)?;
    Ok(())
}
