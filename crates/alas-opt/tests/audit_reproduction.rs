// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reproduction of the optimization-audit findings against the working tree.
//!
//! Each test prints the quantities the audit reported so the numbers can be
//! compared before and after the evaluator is changed; the assertions pin
//! the mechanism, not a tolerance.

// A failed unwrap is the assertion failing, and the reproduction prints the
// audited quantities so they can be read from the test log (module doc).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, MtowSizing, ObjectiveKind};
use alas_opt::objective::DesignObjective;
use alas_opt::{
    assess_candidate, run_sqp, ConstrainedEvaluator, ConstrainedPoint, DesignOptimizer,
    ObjectiveEvaluation, SqpSettings,
};
use alas_payload::apply_cabin_preset;

fn candidate_capacity(config: &AlasConfig, design: &DesignVector) -> i64 {
    let mut candidate = config.clone();
    apply_cabin_preset(&mut candidate, Some(design))
        .map(|_| candidate.requirements.num_passengers)
        .unwrap_or(-1)
}

/// Finding 1: the candidate cabin seats one count, the residual table and
/// the per-seat objective divide by another.
#[test]
fn capacity_scoring_mismatch() {
    let config = AlasConfig::default();
    let design = DesignVector::default();
    let capacity = candidate_capacity(&config, &design);
    let objective = DesignObjective::new(config.clone());
    let assessment = assess_candidate(&objective, &design.to_array()).unwrap();
    let shortfall = assessment
        .residuals
        .iter()
        .find(|residual| residual.id == "passenger_shortfall")
        .unwrap();
    println!(
        "capacity: candidate seats {capacity}, brief {}, residual actual {} limit {} raw {}, payload {} kg",
        config.requirements.num_passengers,
        shortfall.actual,
        shortfall.limit,
        shortfall.raw_residual,
        assessment.sized.payload_kg
    );
    let mut shorter = design;
    shorter.fuselage_length_m *= 0.90;
    let mut target = config.clone();
    target.requirements.num_passengers = 525;
    let capacity_short = candidate_capacity(&target, &shorter);
    let objective = DesignObjective::new(target);
    let assessment = assess_candidate(&objective, &shorter.to_array()).unwrap();
    let shortfall = assessment
        .residuals
        .iter()
        .find(|residual| residual.id == "passenger_shortfall")
        .unwrap();
    println!(
        "capacity (target 525, fuselage -10%): candidate seats {capacity_short}, residual actual {} limit {} raw {}, payload {} kg, hard ids {:?}",
        shortfall.actual,
        shortfall.limit,
        shortfall.raw_residual,
        assessment.sized.payload_kg,
        assessment.violated_hard_ids()
    );
}

/// Finding 7: the induced factor handed to the low-speed constraints carries
/// the cruise wave drag, so a wave-drag-only setting moves the OEI residual.
#[test]
fn wave_drag_contaminates_low_speed_induced_factor() {
    let mut config = AlasConfig::default();
    // Put the default cruise point past drag divergence so the Korn term is
    // active, then change only the Korn technology factor.
    config.requirements.cruise_mach = 0.86;
    let design = DesignVector::default();
    let residual = |config: &AlasConfig| {
        let objective = DesignObjective::new(config.clone());
        let assessment = assess_candidate(&objective, &design.to_array()).unwrap();
        let oei = assessment
            .residuals
            .iter()
            .find(|residual| residual.id == "oei_second_segment")
            .copied()
            .unwrap();
        (
            assessment.sized.lift_to_drag,
            oei.limit,
            assessment.sized.block_fuel_kg,
        )
    };
    let base = residual(&config);
    let mut lower_technology = config.clone();
    lower_technology.drag_model.korn_technology_factor -= 0.03;
    let changed = residual(&lower_technology);
    println!(
        "wave drag: L/D {:.4} -> {:.4}, required OEI T/W {:.6} -> {:.6}, fuel {:.1} -> {:.1} kg",
        base.0, changed.0, base.1, changed.1, base.2, changed.2
    );
    println!(
        "required OEI T/W moves with a cruise-only wave-drag setting: {}",
        (base.1 - changed.1).abs() > 1e-9
    );
}

/// Finding 2: re-trim is triggered by centre-of-gravity shift only, so a
/// sized takeoff mass far below the ceiling keeps the ceiling's trim.
#[test]
fn trim_depends_only_on_cg_shift() {
    let config = AlasConfig::default();
    let design = DesignVector::default();
    let objective = DesignObjective::new(config.clone());
    let assessment = assess_candidate(&objective, &design.to_array()).unwrap();
    println!(
        "trim: ceiling {} kg, sized TOW {:.1} kg ({:.1} %), retrims {}, cg shift {:.4} % MAC, closed {}",
        config.requirements.mtow_kg,
        assessment.sized.takeoff_mass_kg,
        100.0 * assessment.sized.takeoff_mass_kg / config.requirements.mtow_kg,
        assessment.sized.retrim_count,
        assessment.sized.cg_shift_pct_mac,
        assessment.sized.sizing_closed
    );
    let mut tight = config.clone();
    tight.optimizer.objective.retrim_cg_tolerance_pct_mac = 1.0e-6;
    let objective = DesignObjective::new(tight);
    let retrimmed = assess_candidate(&objective, &design.to_array()).unwrap();
    println!(
        "trim (tolerance 1e-6): sized TOW {:.1} kg, retrims {}, fuel {:.1} vs {:.1} kg",
        retrimmed.sized.takeoff_mass_kg,
        retrimmed.sized.retrim_count,
        retrimmed.sized.block_fuel_kg,
        assessment.sized.block_fuel_kg
    );
}

/// Finding 8: below the climb/descent range credit the trip fuel is a fixed
/// fraction of the takeoff mass, independent of the polar.
#[test]
fn short_mission_loses_aerodynamic_sensitivity() {
    let mut config = AlasConfig::default();
    config.optimizer.objective.kind = ObjectiveKind::BlockFuel;
    config.optimizer.objective.mtow_sizing = MtowSizing::FixedRequirement;
    config.optimizer.objective.design_range_nmi = 100.0;
    let evaluate = |scale: f64| {
        let design = DesignVector {
            airfoil_thickness_scale: scale,
            ..DesignVector::default()
        };
        let objective = DesignObjective::new(config.clone());
        let assessment = assess_candidate(&objective, &design.to_array()).unwrap();
        (
            assessment.sized.lift_to_drag,
            assessment.sized.dispatch.plan.trip.kg,
            assessment.sized.takeoff_mass_kg,
        )
    };
    let thin = evaluate(0.9);
    let thick = evaluate(1.1);
    println!(
        "short mission: L/D {:.3} vs {:.3}, trip fuel {:.2} vs {:.2} kg, TOW {:.1} vs {:.1} kg, trip/TOW {:.9} vs {:.9}",
        thin.0,
        thick.0,
        thin.1,
        thick.1,
        thin.2,
        thick.2,
        thin.1 / thin.2,
        thick.1 / thick.2
    );
}

struct Analytic<F: FnMut(&[f64]) -> ConstrainedPoint>(F);

impl<F: FnMut(&[f64]) -> ConstrainedPoint> ConstrainedEvaluator for Analytic<F> {
    fn evaluate_batch(&mut self, designs: &[Vec<f64>]) -> Vec<ConstrainedPoint> {
        designs.iter().map(|x| (self.0)(x)).collect()
    }
}

/// Finding 9: a difference probe that fails on both sides leaves a zero
/// gradient, and the driver reports convergence on a sloped function.
#[test]
fn sqp_reports_convergence_after_failed_probes() {
    let run = |step: f64| {
        let mut evaluator = Analytic(|x: &[f64]| {
            if (x[0] - 0.5).abs() <= 0.0004 {
                ConstrainedPoint {
                    objective: x[0],
                    constraints: Vec::new(),
                    valid: true,
                    cost: x[0],
                }
            } else {
                ConstrainedPoint::invalid(1.0e3)
            }
        });
        let settings = SqpSettings {
            max_iterations: 20,
            finite_difference_step: step,
            constraint_tolerance: 1e-6,
            objective_tolerance: 1e-9,
            step_tolerance: 1e-4,
        };
        run_sqp(&[(0.0, 1.0)], &[0.5], &settings, &mut evaluator, None)
    };
    let coarse = run(0.001);
    let fine = run(0.0001);
    println!(
        "sqp: step 0.001 -> {} (converged {}), x {:?}, objective {}, evaluations {}",
        coarse.termination,
        coarse.converged,
        coarse.best_values,
        coarse.best.objective,
        coarse.evaluations
    );
    println!(
        "sqp: step 0.0001 -> {} (converged {}), x {:?}, objective {}, evaluations {}",
        fine.termination, fine.converged, fine.best_values, fine.best.objective, fine.evaluations
    );
}

/// Finding 10: NSGA-II returns a point worse than the best scalar cost it
/// evaluated.
#[test]
fn nsga2_loses_the_scalar_incumbent() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.method = "nsga2".to_owned();
    config.optimizer.solver.max_iterations = 5;
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.seed = Some(42);
    // Fixed coordinates still have to be inside the selected design-mode
    // envelope. Zero was valid for several dimensionless bumps in the old
    // evaluator-only harness, but it is outside the chord bounds and now
    // correctly fails before NSGA-II starts. Pin each coordinate at its
    // declared nominal value and leave span free for this probe.
    let mut bounds: Vec<_> = alas_config::DESIGN_VARIABLE_SPECS
        .iter()
        .map(|spec| (spec.default, spec.default))
        .collect();
    bounds[0] = (60.0, 80.0);
    let mut best_seen = f64::INFINITY;
    let mut evaluator = |design: &DesignVector| {
        // Bowl in span with a span "objective" that pulls the front away.
        let x = (design.span_m - 60.0) / 20.0;
        let cost = 0.5 + 8.0 * (x - 0.5).powi(2);
        best_seen = best_seen.min(cost);
        ObjectiveEvaluation {
            cost,
            valid: true,
            l_over_d: 18.0,
            span_m: design.span_m,
            alpha_deg: 2.0,
            area_m2: 400.0,
            trim_ih_deg: 0.0,
            reject_reason: String::new(),
        }
    };
    let start = DesignVector {
        span_m: 70.0,
        ..DesignVector::default()
    };
    let result = DesignOptimizer::new(config)
        .run_with_evaluator(Some(&bounds), Some(&start), &mut evaluator, None)
        .unwrap();
    println!(
        "nsga2: best scalar seen {best_seen:.6}, returned {:.6} at span {:.4}, front size {}",
        result.best_cost,
        result.best_design.span_m,
        result.pareto_front.len()
    );
}

/// Method-selection evidence: evaluation cost, and the objective's
/// smoothness along one variable at three finite-difference scales.
#[test]
fn evaluation_cost_and_smoothness_probe() {
    use std::fmt::Write as _;
    use std::time::Instant;
    let config = AlasConfig::default();
    let objective = DesignObjective::new(config.clone());
    let design = DesignVector::default();
    let started = Instant::now();
    let repeats = 20;
    for _ in 0..repeats {
        let _ = assess_candidate(&objective, &design.to_array());
    }
    let per_eval_ms = started.elapsed().as_secs_f64() * 1_000.0 / repeats as f64;
    println!("cost: {per_eval_ms:.1} ms per candidate evaluation (default design, in-loop VLM)");

    let mut csv = String::from("variable,value,fuel_kg,tow_kg,ld,feasible,retrims,hard_ids\n");
    let sweeps: [(&str, usize, f64, f64); 3] = [
        ("span_m", 0, 66.0, 76.0),
        ("sweep_deg", 4, 30.0, 38.0),
        ("fuselage_length_m", 8, 72.0, 80.0),
    ];
    for (name, index, lower, upper) in sweeps {
        let points = 81;
        let mut fuels = Vec::with_capacity(points);
        for step in 0..points {
            let value = lower + (upper - lower) * step as f64 / (points - 1) as f64;
            let mut x = design.to_array();
            x[index] = value;
            match assess_candidate(&objective, &x) {
                Ok(assessment) => {
                    fuels.push(assessment.sized.block_fuel_kg);
                    let _ = writeln!(
                        csv,
                        "{name},{value:.6},{:.6},{:.6},{:.6},{},{},{}",
                        assessment.sized.block_fuel_kg,
                        assessment.sized.takeoff_mass_kg,
                        assessment.sized.lift_to_drag,
                        assessment.hard_feasible,
                        assessment.sized.retrim_count,
                        assessment.violated_hard_ids().join("|")
                    );
                }
                Err(reason) => {
                    fuels.push(f64::NAN);
                    let _ = writeln!(csv, "{name},{value:.6},nan,nan,nan,false,0,{reason}");
                }
            }
        }
        // Second differences relative to the local scale flag jumps.
        let mut jumps = 0;
        let mut worst = 0.0_f64;
        for window in fuels.windows(3) {
            let second = window[0] - 2.0 * window[1] + window[2];
            if !second.is_finite() {
                continue;
            }
            worst = worst.max(second.abs());
            if second.abs() > 20.0 {
                jumps += 1;
            }
        }
        println!(
            "smoothness {name}: {points} points, worst |second difference| {worst:.2} kg, {jumps} windows above 20 kg"
        );
        for h_fraction in [1.0e-2, 1.0e-3, 1.0e-4, 1.0e-5] {
            let h = h_fraction * (upper - lower);
            let mut plus = design.to_array();
            let mut minus = design.to_array();
            plus[index] += h;
            minus[index] -= h;
            let f = |x: &[f64]| {
                assess_candidate(&objective, x)
                    .map(|a| a.sized.block_fuel_kg)
                    .unwrap_or(f64::NAN)
            };
            let derivative = (f(&plus) - f(&minus)) / (2.0 * h);
            println!(
                "  central difference d(fuel)/d({name}) at h={h:.2e}: {derivative:.4} kg/unit"
            );
        }
    }
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.agent/probes/smoothness");
    std::fs::create_dir_all(&root).ok();
    std::fs::write(root.join("sweeps.csv"), csv).ok();
}

/// Diagnose the aerodynamic branch jump seen in the span and sweep sweeps.
#[test]
fn branch_jump_diagnosis() {
    let config = AlasConfig::default();
    let mut objective = DesignObjective::new(config);
    let cases = [
        ("span", 0usize, 69.5),
        ("span", 0, 69.62),
        ("span", 0, 69.75),
        ("span", 0, 69.9),
        ("sweep", 4, 33.1),
        ("sweep", 4, 33.2),
        ("sweep", 4, 33.3),
        ("sweep", 4, 33.4),
    ];
    for (name, index, value) in cases {
        let mut x = DesignVector::default().to_array();
        x[index] = value;
        let cost = objective.evaluate(&x);
        let h = &objective.history;
        let last = h.n_evaluations() - 1;
        println!(
            "{name}={value}: cost {cost:.4} valid {} L/D {:.3} alpha {:.3} ih {:.3} area {:.2} tow {:.0} fuel {:.0} reason {}",
            h.valid[last], h.l_over_d[last], h.alpha_deg[last], h.trim_ih_deg[last], h.area_m2[last], h.takeoff_mass_kg[last], h.block_fuel_kg[last], h.reject_reason[last]
        );
    }
}

/// Whether the trim solve at the takeoff-mass ceiling converged on the bad
/// branch, and how the in-loop lattice's induced drag scales with lift.
#[test]
fn trim_convergence_and_induced_drag_probe() {
    use alas_aero::analysis::{AeroAnalysis, TrimPoint};
    use alas_atmo::Atmosphere;
    use alas_geom::builder::AircraftBuilder;
    use alas_stab::trim::stability_and_trim;
    let config = AlasConfig::default();
    let req = &config.requirements;
    let atmo = Atmosphere::new(req.cruise_altitude_m);
    let velocity = req.cruise_mach * atmo.speed_of_sound();
    let q = 0.5 * atmo.density() * velocity * velocity;
    for span in [69.62, 69.75, 71.75] {
        let design = DesignVector {
            span_m: span,
            ..DesignVector::default()
        };
        let mut plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .unwrap();
        // Reference point at the wing quarter-chord neighbourhood; the exact
        // CG only shifts the pitching-moment lever, which is what the trim
        // solves.
        plane.xyz_ref[0] = 30.0;
        for mass in [358_670.0, 289_000.0] {
            let cl_target = mass * req.gravity_m_s2 / (q * plane.s_ref);
            let trim = stability_and_trim(
                &plane,
                &config.analysis,
                cl_target,
                req.cruise_mach,
                req.cruise_altitude_m,
            )
            .unwrap();
            let aero = AeroAnalysis::new(
                &plane,
                design.sweep_deg,
                Some(config.geometry.clone()),
                Some(config.drag_model.clone()),
                Some(config.analysis.clone()),
            );
            let point = TrimPoint {
                trim_alpha_deg: trim.trim_alpha_deg,
                trim_ih_deg: trim.trim_ih_deg,
                cl_alpha: trim.cl_alpha,
            };
            let perf = aero
                .trimmed_performance(&point, req.cruise_mach, req.cruise_altitude_m)
                .unwrap();
            let cd0 = aero.parasite_drag(
                req.cruise_mach,
                req.cruise_altitude_m,
                cl_target,
                None,
                None,
            );
            println!(
                "span {span} mass {mass}: CL target {cl_target:.4} converged {} alpha {:.3} ih {:.3} sm {:.4} | trimmed CL {:.4} CD {:.5} cd0 {:.5} k_eff {:.4} L/D {:.2} cm {:.4}",
                trim.converged, trim.trim_alpha_deg, trim.trim_ih_deg, trim.static_margin, perf.cl, perf.cd, cd0, (perf.cd - cd0) / (perf.cl * perf.cl), perf.l_over_d, perf.cm_residual
            );
        }
        // Induced-drag scaling of the in-loop lattice versus the fine one.
        for (label, spanwise, chordwise) in [("coarse", 1, 1), ("fine", 2, 8)] {
            let mut analysis = config.analysis.clone();
            analysis.spanwise_resolution = spanwise;
            analysis.chordwise_resolution = chordwise;
            analysis.sweep_alpha_min_deg = 0.0;
            analysis.sweep_alpha_max_deg = 6.0;
            analysis.sweep_n_points = 4;
            let aero = AeroAnalysis::new(
                &plane,
                design.sweep_deg,
                Some(config.geometry.clone()),
                Some(config.drag_model.clone()),
                Some(analysis),
            );
            let sweep = aero
                .run_sweep(req.cruise_mach, req.cruise_altitude_m)
                .unwrap();
            let line: Vec<String> = sweep
                .cl
                .iter()
                .zip(&sweep.cd_induced)
                .map(|(cl, cdi)| format!("CL {cl:.3} CDi {cdi:.5} k {:.4}", cdi / (cl * cl)))
                .collect();
            println!("  {label} lattice span {span}: {}", line.join(" | "));
        }
    }
}

/// Cost and smoothness of the in-loop evaluation at finer lattice
/// resolutions.
#[test]
fn fine_lattice_cost_and_smoothness() {
    use std::time::Instant;
    for (spanwise, chordwise) in [(1, 1), (2, 4), (2, 8)] {
        let mut config = AlasConfig::default();
        config.analysis.spanwise_resolution = spanwise;
        config.analysis.chordwise_resolution = chordwise;
        let objective = DesignObjective::new(config);
        let design = DesignVector::default();
        let started = Instant::now();
        let repeats = 5;
        for _ in 0..repeats {
            let _ = assess_candidate(&objective, &design.to_array());
        }
        let per_eval_ms = started.elapsed().as_secs_f64() * 1_000.0 / repeats as f64;
        let mut jumps = Vec::new();
        let mut previous: Option<(f64, f64)> = None;
        let points = 41;
        for step in 0..points {
            let span = 66.0 + 10.0 * step as f64 / (points - 1) as f64;
            let mut x = design.to_array();
            x[0] = span;
            let fuel = assess_candidate(&objective, &x)
                .map(|a| (a.sized.block_fuel_kg, a.sized.lift_to_drag))
                .unwrap_or((f64::NAN, f64::NAN));
            if let Some((prev_fuel, _)) = previous {
                if (fuel.0 - prev_fuel).abs() > 300.0 {
                    jumps.push(format!("{:.2}m:{:.0}->{:.0}kg", span, prev_fuel, fuel.0));
                }
            }
            previous = Some(fuel);
        }
        println!(
            "lattice {spanwise}x{chordwise}: {per_eval_ms:.1} ms per evaluation; span-sweep jumps >300 kg: {}",
            if jumps.is_empty() { "none".to_owned() } else { jumps.join(", ") }
        );
    }
}

/// Where the planform changes discontinuously with span.
#[test]
fn planform_discontinuity_probe() {
    use alas_geom::builder::AircraftBuilder;
    let config = AlasConfig::default();
    for span in [69.62, 69.68, 69.72, 69.75] {
        let design = DesignVector {
            span_m: span,
            ..DesignVector::default()
        };
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .unwrap();
        let wing = &plane.wings[0];
        let sections: Vec<String> = wing
            .xsecs
            .iter()
            .map(|x| {
                format!(
                    "(y {:.3} x {:.3} c {:.3} tw {:.2})",
                    x.xyz_le[1], x.xyz_le[0], x.chord, x.twist
                )
            })
            .collect();
        println!(
            "span {span}: s_ref {:.3} b_ref {:.3} c_ref {:.3} sections {} {}",
            plane.s_ref,
            plane.b_ref,
            plane.c_ref,
            wing.xsecs.len(),
            sections.join(" ")
        );
    }
}
