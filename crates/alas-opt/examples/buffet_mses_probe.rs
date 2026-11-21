// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Probe (round 4): buffet onset from MSES on the critical section of the
//! 3-D span load.
//!
//! 1. Wing-only VLM at the cruise lift coefficient (declared MTOW, cruise
//!    Mach and altitude) gives the strip section cl; the critical strip has
//!    the highest `cl_n = cl_local / cos^2 L_s` for eta in 0.3 to 0.8.
//! 2. `L_s` is the sweep of the line at 0.55 c (mean of the 0.5 c and 0.6 c
//!    lines) on the local planform panel; `M_n = M cos L_s`, the morphed
//!    section's ordinates are scaled by `1/cos L_s`, `Re_n = Re cos^2 L_s`.
//! 3. MSES is stepped in incidence (0.5 deg) until the upper-surface Mach
//!    ahead of the shock reaches 1.38 (Obert 2009 ch. 17 pp. 109-110, Figs.
//!    17.5-17.6). The nose suction peak (x/c < 0.15) is excluded and the
//!    shock is the steepest Mach drop at 0.25 <= x/c <= 0.9. The trailing-edge
//!    criteria of round 4 are dropped (Obert gives no threshold); set
//!    `BUFFET_ONLY_M` to skip them.
//! 4. `n = cl_n_onset / cl_n_cruise`, which equals `CL_onset / CL_cruise`
//!    because the VLM ratio cl_local/CL cancels.
//!
//! `buffet_mses_probe <preset> <mses_dir> [mach ...]`

#![allow(clippy::print_stderr, clippy::print_stdout, clippy::unwrap_used)]

use alas_aero::wing_analysis::{
    analyse, build_wing_model, AlphaSweep, AttitudeInput, FlightCondition, SpeedInput, SurfaceSet,
    WingAnalysisInputs,
};
use alas_config::AlasConfig;
use alas_geom::aircraft::wing::WingXSec;
use std::path::Path;

const M_LOCAL_ONSET: f64 = 1.38;

fn trapz(x: &[f64], y: &[f64]) -> f64 {
    x.windows(2)
        .zip(y.windows(2))
        .map(|(x, y)| 0.5 * (y[0] + y[1]) * (x[1] - x[0]))
        .sum()
}

fn panel_at(xsecs: &[WingXSec], y: f64) -> (&WingXSec, &WingXSec, f64) {
    let lifting: Vec<_> = xsecs
        .windows(2)
        .filter(|p| {
            let dy = (p[1].xyz_le[1] - p[0].xyz_le[1]).abs();
            let dz = (p[1].xyz_le[2] - p[0].xyz_le[2]).abs();
            dy > 0.0 && dz <= dy
        })
        .collect();
    let p = lifting
        .iter()
        .find(|p| y <= p[1].xyz_le[1].abs())
        .unwrap_or(&lifting[lifting.len() - 1]);
    let (ya, yb) = (p[0].xyz_le[1].abs(), p[1].xyz_le[1].abs());
    (&p[0], &p[1], ((y - ya) / (yb - ya)).clamp(0.0, 1.0))
}

/// Sweep, degrees, of the line at `fraction` of the local chord on the panel.
fn line_sweep(a: &WingXSec, b: &WingXSec, fraction: f64) -> f64 {
    let dy = (b.xyz_le[1] - a.xyz_le[1]).abs();
    let dx = (b.xyz_le[0] + fraction * b.chord) - (a.xyz_le[0] + fraction * a.chord);
    (dx / dy).atan().to_degrees()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let preset = &args[0];
    let mses_dir = Path::new(&args[1]);
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset })).unwrap();
    let design = alas_config::presets::get(preset).unwrap().design_vector;
    let req = &config.requirements;
    let machs: Vec<f64> = if args.len() > 2 {
        args[2..].iter().map(|s| s.parse().unwrap()).collect()
    } else {
        vec![req.cruise_mach]
    };
    let model = build_wing_model(&config.geometry, &design, SurfaceSet::WingOnly, None).unwrap();
    let s_ref = model.reference().area_m2;
    let atmosphere = alas_atmo::Atmosphere::new(req.cruise_altitude_m);
    let cruise_mach = req.cruise_mach;
    let v_cruise = cruise_mach * atmosphere.speed_of_sound();
    let q = 0.5 * atmosphere.density() * v_cruise * v_cruise;
    let cl_cruise = req.mtow_kg * req.gravity_m_s2 / (q * s_ref);

    let inputs = WingAnalysisInputs {
        surfaces: SurfaceSet::WingOnly,
        condition: FlightCondition {
            altitude_m: req.cruise_altitude_m,
            speed: SpeedInput::Mach(cruise_mach),
            attitude: AttitudeInput::LiftCoefficient(cl_cruise),
        },
        moment_reference_m: model.reference().moment_reference_m,
        spanwise_resolution: 1,
        chordwise_resolution: 8,
        sweep: AlphaSweep {
            min_deg: 0.0,
            max_deg: 2.0,
            points: 2,
        },
    };
    let outcome = analyse(&model, &inputs, &|| false).unwrap();
    let xsecs = &model.airplane().wings[0].xsecs;
    if std::env::var("BUFFET_DUMP").is_ok() {
        println!("CLcr={cl_cruise:.4} sref={s_ref:.2}");
        for s in &outcome.span_load {
            if s.y_m > 0.0 {
                println!(
                    "SPAN eta={:.3} y={:.2} cl={:.4} ratio={:.3}",
                    s.y_over_semispan,
                    s.y_m,
                    s.section_cl,
                    s.section_cl / cl_cruise
                );
            }
        }
        for eta in [0.2_f64, 0.4, 0.55, 0.674, 0.8] {
            let y = eta * model.airplane().wings[0].xsecs.last().unwrap().xyz_le[1].abs();
            let (a, b, t) = panel_at(xsecs, y);
            let sec = WingXSec::between(a, b, t).unwrap();
            let c = &sec.airfoil.coordinates;
            let (tc, cam) = tc_camber(c);
            println!(
                "SEC eta={eta} chord={:.2} twist={:.2} tc={tc:.4} camber={cam:.4} npts={}",
                sec.chord,
                sec.twist,
                c.len()
            );
        }
    }
    // Critical strip: highest cl_n for eta 0.3 to 0.8 (starboard strips).
    let mut best: Option<(f64, f64, f64, f64, f64)> = None; // cl_n, eta, y, cl_local, sweep
    for s in &outcome.span_load {
        let eta = s.y_over_semispan.abs();
        if !(0.3..=0.8).contains(&eta) {
            continue;
        }
        let (a, b, _) = panel_at(xsecs, s.y_m.abs());
        let sweep = 0.5 * (line_sweep(a, b, 0.5) + line_sweep(a, b, 0.6));
        let cl_n = s.section_cl / sweep.to_radians().cos().powi(2);
        if best.is_none_or(|x| cl_n > x.0) {
            best = Some((cl_n, eta, s.y_m.abs(), s.section_cl, sweep));
        }
    }
    let (cl_n_cruise, eta, y, cl_local, sweep_deg) = best.unwrap();
    let (a, b, t) = panel_at(xsecs, y);
    let section = WingXSec::between(a, b, t).unwrap();
    let cos = sweep_deg.to_radians().cos();
    println!(
        "PRESET {preset} CLcr(MTOW)={cl_cruise:.4} eta*={eta:.3} cl_local={cl_local:.4} ratio={:.3} sweep_s={sweep_deg:.2} cl_n_cruise={cl_n_cruise:.4} chord={:.2} Mcr={cruise_mach}",
        cl_local / cl_cruise,
        section.chord
    );
    let normal = alas_geom::aircraft::airfoil::Airfoil {
        name: section.airfoil.name.clone(),
        coordinates: section
            .airfoil
            .coordinates
            .iter()
            .map(|&(x, y)| (x, y / cos))
            .collect(),
    };
    let only_m = std::env::var("BUFFET_ONLY_M").is_ok();
    let mut mses = config.mses.clone();
    mses.enabled = true;
    mses.mses_dir = mses_dir.to_string_lossy().into_owned();

    for &mach in &machs {
        let v = mach * atmosphere.speed_of_sound();
        let m_n = mach * cos;
        let re_n = v / atmosphere.kinematic_viscosity() * section.chord * cos * cos;
        println!("MACH {mach} Mn={m_n:.4} Re_n={re_n:.3e}");
        let mut points: Vec<(f64, f64, f64, f64)> = Vec::new(); // alpha, cl_n, cpTE, Mmax
        let mut alpha = -1.0;
        let verdict = loop {
            let started = std::time::Instant::now();
            let r = alas_aero::mses::run_mses_pressure_distribution(
                &normal,
                m_n,
                re_n,
                alpha,
                &mses,
                mses_dir,
                Some(&[0.0]),
            );
            let seconds = started.elapsed().as_secs_f64();
            if r.x_upper.is_empty() || r.x_lower.is_empty() {
                println!("ALPHA {alpha:.2} FAILED t={seconds:.0}s {:?}", r.error);
                break "no onset before MSES non-convergence".to_owned();
            }
            let cl = trapz(&r.x_lower, &r.cp_lower) - trapz(&r.x_upper, &r.cp_upper);
            let cp_te = r.cp_upper[r.cp_upper.len() - 1];
            if std::env::var("BUFFET_DUMP").is_ok() {
                let s: Vec<String> = r
                    .x_upper
                    .iter()
                    .zip(r.mach_upper.iter())
                    .step_by(4)
                    .map(|(x, m)| format!("{x:.3}:{m:.2}"))
                    .collect();
                let k: Vec<String> = (0..r.x_upper.len())
                    .filter(|i| r.x_upper[*i] > 0.66 && r.x_upper[*i] < 0.80)
                    .map(|i| {
                        format!(
                            "{i}:{:.3}:{:.2}:{:.2}",
                            r.x_upper[i], r.cp_upper[i], r.mach_upper[i]
                        )
                    })
                    .collect();
                println!("RAW n={} {}", r.x_upper.len(), k.join(" "));
                println!("MACHDIST alpha={alpha:.2} {}", s.join(" "));
            }
            let (m_max, x_shock) = pre_shock_mach(&r.x_upper, &r.mach_upper);
            println!(
                "ALPHA {:.2} cl_n={cl:.4} (x{:.3} of cruise) cpTE={cp_te:.4} Mpre={m_max:.3} xs={x_shock:.2} t={seconds:.0}s",
                r.alpha_deg,
                cl / cl_n_cruise
            );
            let previous = points.last().copied();
            points.push((alpha, cl, cp_te, m_max));
            let mut found: Vec<(f64, &str)> = Vec::new();
            if let Some(p) = previous {
                if p.3 < M_LOCAL_ONSET && m_max >= M_LOCAL_ONSET {
                    let f = (M_LOCAL_ONSET - p.3) / (m_max - p.3);
                    found.push((p.1 + f * (cl - p.1), "Mlocal>=1.38"));
                }
                if !only_m && p.2 >= 0.0 && cp_te < 0.0 {
                    let f = p.2 / (p.2 - cp_te);
                    found.push((p.1 + f * (cl - p.1), "CpTE<0"));
                }
            } else if m_max >= M_LOCAL_ONSET {
                found.push((cl, "Mlocal>=1.38 at first point"));
            }
            if !only_m && points.len() >= 3 {
                let (p2, p1) = (points[points.len() - 3], points[points.len() - 2]);
                let s_prev = (p1.2 - p2.2) / (p1.1 - p2.1);
                let s_now = (cp_te - p1.2) / (cl - p1.1);
                if s_prev < 0.0 && s_now > 0.0 {
                    found.push((p1.1, "dCpTE/dcl sign change"));
                }
            }
            if let Some(&(cl_on, which)) = found.iter().min_by(|a, b| a.0.total_cmp(&b.0)) {
                let all: Vec<String> = found.iter().map(|(c, w)| format!("{w}@{c:.3}")).collect();
                break format!(
                    "ONSET {which} cl_n={cl_on:.4} n={:.3} (candidates {})",
                    cl_on / cl_n_cruise,
                    all.join(", ")
                );
            }
            if cl > 2.0 * cl_n_cruise {
                break "no onset up to 2x cruise cl_n".to_owned();
            }
            alpha += 0.5;
        };
        println!("RESULT {preset} M={mach} {verdict}");
    }
}

/// Max thickness and max camber (fractions of chord) of a closed airfoil
/// listing (upper then lower, either order) by interpolating both surfaces.
fn tc_camber(c: &[(f64, f64)]) -> (f64, f64) {
    let i_le = c
        .iter()
        .enumerate()
        .min_by(|a, b| a.1 .0.total_cmp(&b.1 .0))
        .unwrap()
        .0;
    let s1: Vec<(f64, f64)> = c[..=i_le].iter().rev().copied().collect(); // LE -> first TE
    let s2: Vec<(f64, f64)> = c[i_le..].to_vec();
    let at = |s: &[(f64, f64)], x: f64| {
        for w in s.windows(2) {
            if (w[0].0 <= x && x <= w[1].0) || (w[1].0 <= x && x <= w[0].0) {
                let d = w[1].0 - w[0].0;
                return if d.abs() < 1e-12 {
                    w[0].1
                } else {
                    w[0].1 + (x - w[0].0) / d * (w[1].1 - w[0].1)
                };
            }
        }
        s[0].1
    };
    let (mut tc, mut cam) = (0.0_f64, 0.0_f64);
    for k in 1..100 {
        let x = k as f64 / 100.0;
        let (a, b) = (at(&s1, x), at(&s2, x));
        tc = tc.max((a - b).abs());
        cam = cam.max((0.5 * (a + b)).abs());
    }
    (tc, cam)
}

/// Maximum local Mach number in front of the upper-surface shock, and the
/// shock position (Obert 2009 ch. 17 p. 109: M_local = 1.38 is the maximum
/// Mach number ahead of the shock at buffet onset). The leading-edge suction
/// peak (x/c < 0.15) is excluded. The shock is the steepest Mach drop over
/// two samples (>= 0.15) from a supersonic point at 0.25 <= x/c <= 0.9; with
/// no shock the maximum aft of 0.15 c is returned with x_shock = 1.
fn pre_shock_mach(x: &[f64], m: &[f64]) -> (f64, f64) {
    let mut shock: Option<(usize, f64)> = None;
    for i in 0..x.len().saturating_sub(2) {
        if x[i] < 0.25 || x[i] > 0.9 || m[i] < 1.0 {
            continue;
        }
        let drop = m[i] - m[i + 2];
        if drop >= 0.15 && shock.is_none_or(|s| drop > s.1) {
            shock = Some((i, drop));
        }
    }
    let end = shock.map_or(x.len() - 1, |s| s.0);
    let m_pre = (0..=end)
        .filter(|&i| x[i] >= 0.15)
        .map(|i| m[i])
        .fold(0.0, f64::max);
    (m_pre, shock.map_or(1.0, |s| x[s.0]))
}
