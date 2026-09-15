// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Vortex-lattice mesh-resolution sensitivity sweep.
//!
//! `AnalysisConfig::spanwise_resolution` / `chordwise_resolution` mesh every
//! candidate the optimizer evaluates; `fine_spanwise_resolution` /
//! `fine_chordwise_resolution` mesh the once-per-run reported analysis. This
//! example measures what those four numbers actually buy, in three modes:
//!
//! * `probe <preset>`, one aircraft, a full factorial of meshes, reporting
//!   panel count, wall time, lift slope, zero-lift angle, neutral point,
//!   static margin and the trimmed cruise state at each.
//! * `camber <preset>`: the same mesh grid against the five camber design
//!   variables, measuring whether the mesh can see them at all.
//! * `objective <preset> <n>`: the optimizer's own scalar cost, at the
//!   nominal design vector and `n` perturbed neighbours, on each mesh.
//!
//! Output is CSV on stdout, one header per mode.

#![allow(clippy::print_stdout)]

use std::time::Instant;

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_opt::objective::DesignObjective;
use alas_opt::sampling::Rng;
use alas_stab::trim::{neutral_point, stability_and_trim};

/// Spanwise subdivision multipliers swept, in increasing cost.
const SPANWISE: &[i64] = &[1, 2, 3, 4, 6, 10];
/// Chordwise panel counts swept, in increasing cost.
const CHORDWISE: &[i64] = &[1, 2, 3, 4, 6, 8, 12, 16, 24, 32];
/// Meshes above this panel count are skipped: the influence matrix is dense
/// and its factorization is cubic, so the sweep would stop being a sweep.
const PANEL_CAP: usize = 4_000;

/// The meshes the objective sweep uses. A full objective evaluation is a
/// mission-sizing loop around several vortex-lattice solves, so this is the
/// affordable subset: the search resolutions a run could plausibly pay for,
/// plus a converged reference to rank against.
const OBJECTIVE_MESHES: &[(i64, i64)] = &[
    (1, 1),
    (1, 2),
    (1, 4),
    (1, 8),
    (1, 16),
    (2, 1),
    (2, 4),
    (2, 8),
    (2, 16),
];

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| "probe".to_owned());
    let preset = args.next().unwrap_or_else(|| "AVE".to_owned());
    let count: usize = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(12);

    match mode.as_str() {
        "probe" => probe(&preset),
        "camber" => camber(&preset),
        "objective" => objective(&preset, count),
        other => println!("unknown mode {other}"),
    }
}

/// The product configuration a preset name selects in the interface.
///
/// `ALAS_WING_SUBDIV` and `ALAS_TAIL_SUBDIV` override the builder's own
/// spanwise subdivision. That is the only clean spanwise refinement available:
/// it is `Linspace`, so it produces a uniformly finer strip distribution,
/// where the solver's `spanwise_resolution` subdivides an already-subdivided
/// wing again with `Cosspace` and leaves a sawtooth at every original
/// station. Sweeping it answers whether the default strip count is
/// spanwise-converged at all: a question the solver's own knob cannot ask.
fn config_for(preset: &str) -> Result<(AlasConfig, DesignVector), String> {
    let value = serde_json::json!({ "preset": preset });
    let mut config = AlasConfig::from_value(&value).map_err(|error| error.to_string())?;
    if let Some(n) = std::env::var("ALAS_WING_SUBDIV")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        config.geometry.wing.n_subdivisions = n;
    }
    if let Some(n) = std::env::var("ALAS_TAIL_SUBDIV")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        config.geometry.empennage.n_subdivisions = n;
    }
    let design = alas_config::presets::get(preset)
        .map(|entry| entry.design_vector)
        .map_err(|error| error.to_string())?;
    Ok((config, design))
}

/// Build one aircraft at `design`, engines included, as every analysis path
/// in the product does.
fn build(config: &AlasConfig, design: &DesignVector) -> Result<Airplane, String> {
    AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(design), true)
        .map_err(|error| error.to_string())
}

/// The meshes swept, coarsest first.
///
/// Both multipliers scale the panel count exactly linearly, a spanwise
/// subdivision by `r` turns each surface's `n - 1` lofted sections into
/// `r (n - 1)`, and the chordwise resolution is a literal panel count, so
/// `strips * span * chord` predicts the mesh size before it is built, and
/// meshes above [`PANEL_CAP`] are dropped without paying for the assembly.
///
/// `ALAS_SWEEP_SPAN` and `ALAS_SWEEP_CHORD` override the swept lists with
/// comma-separated values, and `ALAS_SWEEP_PANEL_CAP` the cap, so one
/// specific mesh can be interrogated without editing the table.
fn meshes(strips: usize) -> Vec<(i64, i64)> {
    let axis = |name: &str, fallback: &[i64]| -> Vec<i64> {
        std::env::var(name).ok().map_or_else(
            || fallback.to_vec(),
            |raw| {
                raw.split(',')
                    .filter_map(|part| part.trim().parse().ok())
                    .collect()
            },
        )
    };
    let cap = std::env::var("ALAS_SWEEP_PANEL_CAP")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(PANEL_CAP);
    let mut out = Vec::new();
    for span in axis("ALAS_SWEEP_SPAN", SPANWISE) {
        for chord in axis("ALAS_SWEEP_CHORD", CHORDWISE) {
            let panels = strips * span.max(1) as usize * chord.max(1) as usize;
            if panels <= cap {
                out.push((span, chord));
            }
        }
    }
    out
}

/// The panel count of the unmultiplied mesh: one chordwise panel on every
/// spanwise strip the builder's own subdivision already produced.
fn base_strips(plane: &Airplane) -> Result<usize, String> {
    let system = VlmSystem::assemble(plane, 1, 1).map_err(|error| format!("{error:?}"))?;
    let atmo = Atmosphere::new(0.0);
    let op = OperatingPoint::new(atmo, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let result = system.solve(&op).map_err(|error| format!("{error:?}"))?;
    Ok(result.vortex_strengths.len())
}

/// One aircraft, every mesh: cost, lift slope, zero-lift angle, neutral
/// point, static margin, trimmed cruise state.
fn probe(preset: &str) {
    let (config, design) = match config_for(preset) {
        Ok(pair) => pair,
        Err(error) => {
            println!("preset error,{error}");
            return;
        }
    };
    let plane = match build(&config, &design) {
        Ok(plane) => plane,
        Err(error) => {
            println!("build error,{error}");
            return;
        }
    };

    let req = &config.requirements;
    let atmo = Atmosphere::new(req.cruise_altitude_m);
    let velocity = req.cruise_mach * atmo.speed_of_sound();
    let q = 0.5 * atmo.density() * velocity * velocity;
    let cl_target = req.required_cruise_cl(q, plane.s_ref);

    let strips = match base_strips(&plane) {
        Ok(strips) => strips,
        Err(error) => {
            println!("base mesh error,{error}");
            return;
        }
    };

    println!(
        "preset,span_res,chord_res,panels,assemble_ms,solve_ms,\
         cl_alpha_per_deg,alpha_zero_lift_deg,cm_alpha_per_deg,cd_i_at_2deg,\
         span_efficiency,x_np_m,static_margin,trim_alpha_deg,trim_ih_deg,\
         trim_converged,trimmed_cl,trimmed_cd_induced,induced_factor_k,\
         trimmed_e,trimmed_cd0,trimmed_cd_wave,trimmed_l_over_d,\
         cl_target,s_ref_m2,b_ref_m,c_ref_m"
    );

    for (span, chord) in meshes(strips) {
        let mut analysis = config.analysis.clone();
        analysis.spanwise_resolution = span;
        analysis.chordwise_resolution = chord;

        let started = Instant::now();
        let system = match VlmSystem::assemble(&plane, span.max(1) as usize, chord.max(1) as usize)
        {
            Ok(system) => system,
            Err(error) => {
                println!("{preset},{span},{chord},,,,,,,,,,,,,assemble error {error:?}");
                continue;
            }
        };
        let assemble_ms = started.elapsed().as_secs_f64() * 1e3;

        let low = OperatingPoint::new(atmo, velocity, 0.0, 0.0, 0.0, 0.0, 0.0);
        let high = OperatingPoint::new(atmo, velocity, 2.0, 0.0, 0.0, 0.0, 0.0);
        let started = Instant::now();
        let (r_low, r_high) = match (system.solve(&low), system.solve(&high)) {
            (Ok(a), Ok(b)) => (a, b),
            _ => {
                println!("{preset},{span},{chord},,,,,,,,,,,,,solve error");
                continue;
            }
        };
        let solve_ms = started.elapsed().as_secs_f64() * 1e3 / 2.0;
        let panels = r_low.vortex_strengths.len();

        let cl_alpha = (r_high.cl_lift - r_low.cl_lift) / 2.0;
        let cm_alpha = (r_high.cm_pitch - r_low.cm_pitch) / 2.0;
        let alpha_zero_lift = if cl_alpha.abs() > 1e-9 {
            -r_low.cl_lift / cl_alpha
        } else {
            f64::NAN
        };
        let aspect_ratio = plane.b_ref * plane.b_ref / plane.s_ref;
        let span_efficiency = if r_high.cd_drag.abs() > 1e-12 {
            r_high.cl_lift * r_high.cl_lift / (std::f64::consts::PI * aspect_ratio * r_high.cd_drag)
        } else {
            f64::NAN
        };

        let (x_np, static_margin) = match neutral_point(&plane, &analysis) {
            Ok((x_np, sm, _)) => (x_np, sm),
            Err(_) => (f64::NAN, f64::NAN),
        };
        let trim = stability_and_trim(
            &plane,
            &analysis,
            cl_target,
            req.cruise_mach,
            req.cruise_altitude_m,
        );
        let (trim_alpha, trim_ih, converged) = match &trim {
            Ok(trim) => (trim.trim_alpha_deg, trim.trim_ih_deg, trim.converged),
            Err(_) => (f64::NAN, f64::NAN, false),
        };

        // The trimmed cruise point is what the optimizer's mission model
        // actually flies on: `k = CD_i / CL^2` and the parasite term become
        // the drag polar of every mission segment.
        let aero = alas_aero::analysis::AeroAnalysis::new(
            &plane,
            design.sweep_deg,
            Some(config.geometry.clone()),
            Some(config.drag_model.clone()),
            Some(analysis.clone()),
        );
        let trimmed = trim.ok().and_then(|trim| {
            let point = alas_aero::analysis::TrimPoint {
                trim_alpha_deg: trim.trim_alpha_deg,
                trim_ih_deg: trim.trim_ih_deg,
                cl_alpha: trim.cl_alpha,
            };
            aero.trimmed_performance(&point, req.cruise_mach, req.cruise_altitude_m)
                .ok()
        });
        let (t_cl, t_cdi, t_k, t_e, t_cd0, t_wave, t_ld) = match trimmed {
            Some(perf) => {
                let k = perf.cd_induced / (perf.cl * perf.cl);
                let e = 1.0 / (std::f64::consts::PI * aspect_ratio * k);
                (
                    perf.cl,
                    perf.cd_induced,
                    k,
                    e,
                    perf.cd_parasite,
                    perf.cd_wave,
                    perf.l_over_d,
                )
            }
            None => (
                f64::NAN,
                f64::NAN,
                f64::NAN,
                f64::NAN,
                f64::NAN,
                f64::NAN,
                f64::NAN,
            ),
        };

        println!(
            "{preset},{span},{chord},{panels},{assemble_ms:.2},{solve_ms:.2},\
             {cl_alpha:.6},{alpha_zero_lift:.4},{cm_alpha:.6},{cd_i:.6},\
             {span_efficiency:.4},{x_np:.4},{static_margin:.5},{trim_alpha:.4},\
             {trim_ih:.4},{converged},{t_cl:.5},{t_cdi:.6},{t_k:.6},{t_e:.4},\
             {t_cd0:.6},{t_wave:.6},{t_ld:.4},\
             {cl_target:.5},{s_ref:.3},{b_ref:.3},{c_ref:.4}",
            cd_i = r_high.cd_drag,
            s_ref = plane.s_ref,
            b_ref = plane.b_ref,
            c_ref = plane.c_ref,
        );
    }
}

/// The five design variables that move the camber line, at their bounds.
fn camber_cases(nominal: &DesignVector) -> Vec<(&'static str, DesignVector)> {
    let mut cases = vec![("nominal", *nominal)];
    let mut low = *nominal;
    low.airfoil_camber_scale = 0.7;
    cases.push(("camber_scale_0.70", low));
    let mut high = *nominal;
    high.airfoil_camber_scale = 1.4;
    cases.push(("camber_scale_1.40", high));
    let mut front = *nominal;
    front.bump_upper_front = -0.005;
    cases.push(("bump_upper_front_-0.005", front));
    let mut rear = *nominal;
    rear.bump_upper_rear = -0.005;
    cases.push(("bump_upper_rear_-0.005", rear));
    let mut lower = *nominal;
    lower.bump_lower_rear = 0.003;
    cases.push(("bump_lower_rear_+0.003", lower));
    let mut twist = *nominal;
    twist.tip_twist_deg = nominal.tip_twist_deg - 3.0;
    cases.push(("tip_twist_-3deg", twist));
    cases
}

/// Whether a mesh can see the camber design variables at all: lift at a
/// fixed attitude, for each camber perturbation, on each mesh.
fn camber(preset: &str) {
    let (config, design) = match config_for(preset) {
        Ok(pair) => pair,
        Err(error) => {
            println!("preset error,{error}");
            return;
        }
    };
    let req = &config.requirements;
    let atmo = Atmosphere::new(req.cruise_altitude_m);
    let velocity = req.cruise_mach * atmo.speed_of_sound();

    println!("preset,case,span_res,chord_res,panels,cl_at_2deg,cm_at_2deg,cd_i_at_2deg");
    for (label, candidate) in camber_cases(&design) {
        let plane = match build(&config, &candidate) {
            Ok(plane) => plane,
            Err(error) => {
                println!("{preset},{label},,,,build error {error}");
                continue;
            }
        };
        let Ok(strips) = base_strips(&plane) else {
            continue;
        };
        for (span, chord) in meshes(strips) {
            let system =
                match VlmSystem::assemble(&plane, span.max(1) as usize, chord.max(1) as usize) {
                    Ok(system) => system,
                    Err(_) => continue,
                };
            let op = OperatingPoint::new(atmo, velocity, 2.0, 0.0, 0.0, 0.0, 0.0);
            let Ok(result) = system.solve(&op) else {
                continue;
            };
            let panels = result.vortex_strengths.len();
            println!(
                "{preset},{label},{span},{chord},{panels},{cl:.6},{cm:.6},{cd:.6}",
                cl = result.cl_lift,
                cm = result.cm_pitch,
                cd = result.cd_drag,
            );
        }
    }
}

/// A design vector perturbed within a fraction of each variable's own range,
/// clamped to its bounds: the neighbourhood a converging search samples.
fn perturb(nominal: &DesignVector, rng: &mut Rng, fraction: f64) -> DesignVector {
    let bounds = DesignVector::bounds();
    let values: Vec<f64> = nominal
        .to_array()
        .iter()
        .zip(&bounds)
        .map(|(&value, &(lower, upper))| {
            let half = 0.5 * fraction * (upper - lower);
            (value + rng.uniform(-half, half)).clamp(lower, upper)
        })
        .collect();
    DesignVector::from_array(&values).unwrap_or(*nominal)
}

/// The optimizer's own scalar cost at each mesh, for the nominal design and
/// `count` neighbours of it.
fn objective(preset: &str, count: usize) {
    let (config, design) = match config_for(preset) {
        Ok(pair) => pair,
        Err(error) => {
            println!("preset error,{error}");
            return;
        }
    };

    let mut rng = Rng::seed(20_260_911);
    let mut candidates = vec![("nominal".to_owned(), design)];
    for index in 0..count {
        candidates.push((
            format!("sample_{index:02}"),
            perturb(&design, &mut rng, 0.10),
        ));
    }

    println!(
        "preset,candidate,span_res,chord_res,eval_ms,cost,valid,reject_reason,\
         l_over_d,alpha_deg,trim_ih_deg,span_m,area_m2,objective_value,\
         takeoff_mass_kg,block_fuel_kg,hard_violation,soft_violation"
    );

    for (span, chord) in OBJECTIVE_MESHES.iter().copied() {
        let mut config = config.clone();
        config.analysis.spanwise_resolution = span;
        config.analysis.chordwise_resolution = chord;
        config.mission.enabled = false;
        for (label, candidate) in &candidates {
            let mut objective = DesignObjective::new(config.clone());
            let started = Instant::now();
            let cost = objective.evaluate(&candidate.to_array());
            let eval_ms = started.elapsed().as_secs_f64() * 1e3;
            let history = &objective.history;
            let last = history.cost.len().saturating_sub(1);
            if history.cost.is_empty() {
                println!("{preset},{label},{span},{chord},{eval_ms:.1},{cost},,no history");
                continue;
            }
            println!(
                "{preset},{label},{span},{chord},{eval_ms:.1},{cost:.6},{valid},{reason},\
                 {l_over_d:.4},{alpha:.4},{ih:.4},{span_m:.3},{area:.3},{objective_value:.6},\
                 {mass:.1},{fuel:.1},{hard:.6},{soft:.6}",
                valid = history.valid[last],
                reason = history.reject_reason[last],
                l_over_d = history.l_over_d[last],
                alpha = history.alpha_deg[last],
                ih = history.trim_ih_deg[last],
                span_m = history.span_m[last],
                area = history.area_m2[last],
                objective_value = history.objective_value[last],
                mass = history.takeoff_mass_kg[last],
                fuel = history.block_fuel_kg[last],
                hard = history.hard_violation[last],
                soft = history.soft_violation[last],
            );
        }
    }
}
