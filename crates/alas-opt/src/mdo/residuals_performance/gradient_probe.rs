// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Second-segment gradient decomposition of the registered presets, built
//! from the same pieces as the optimizer residual. Probe tables are ignored
//! tests; the decomposition identity is a unit test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use super::oei_deck::{second_segment_evidence, EngineOutModel, YawAirframe};
use crate::mdo::residuals_vmc::{evaluate_candidate, performance_with_vmc};
use crate::mdo::sizing::run_candidate;
use alas_config::AlasConfig;
use alas_perf::performance::{
    compute_v_speeds_at_masses, decompose_gradient, far25_oei_gradient, oei_cl_at_v2,
    oswald_factor, GradientInputs,
};

fn candidate(preset: &str) -> (AlasConfig, crate::mdo::sizing::SizingOutcome) {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": preset,
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get(preset).unwrap().design_vector;
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    (config, outcome)
}

/// `cargo test -p alas-opt --release gradient_table -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic table of all registered presets; slow"]
fn gradient_table() {
    for registered in alas_config::presets::registry() {
        let (config, outcome) = candidate(registered.name);
        let vmc = evaluate_candidate(&outcome, &config);
        let resolved = performance_with_vmc(&config, &vmc);
        let perf = &resolved.performance;
        let Some(departure) = outcome.departure else {
            continue;
        };
        let s_ref = outcome.plane.s_ref;
        let mass = outcome.sized.takeoff_mass_kg;
        let speeds = compute_v_speeds_at_masses(
            mass,
            mass,
            s_ref,
            departure,
            perf.cl_max_to,
            perf.cl_max_land,
            perf,
        );
        let drag = &outcome.sized.fuel_artifacts.drag;
        let atmosphere =
            alas_atmo::us1976_compute_values(departure.elevation_m, departure.isa_deviation_c);
        let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
        let model = EngineOutModel::new(
            &resolved,
            Some(YawAirframe {
                plane: &outcome.plane,
                cg_x_m: outcome.cg_x,
            }),
            s_ref,
            Some(outcome.static_thrust_kn * 1_000.0),
        )
        .ok()
        .map(|m| m.with_weight_kg(mass));
        let evidence =
            second_segment_evidence(model.as_ref(), perf, departure, &speeds, &|m, h, d| {
                drag.cd_at_atmosphere(0.0, m, h, d)
            });
        let n = outcome.n_engines;
        let cl = oei_cl_at_v2(perf.cl_max_to, evidence.condition.v2_over_vstall).unwrap();
        let cd0 =
            drag.cd_at_atmosphere(0.0, mach, departure.elevation_m, departure.isa_deviation_c);
        let cd = drag.cd_airport_base_at_atmosphere(
            cl,
            mach,
            departure.elevation_m,
            departure.isa_deviation_c,
        );
        let k = (cd - cd0) / (cl * cl);
        let ar = outcome.plane.b_ref * outcome.plane.b_ref / s_ref;
        let installed_tw = n as f64 * outcome.static_thrust_kn * 1_000.0 / (mass * 9.806_65);
        let d = decompose_gradient(&GradientInputs {
            n_engines: n,
            installed_tw_sls: installed_tw,
            thrust_ratio: evidence
                .condition
                .condition_to_sls_thrust_ratio
                .unwrap_or(f64::NAN),
            cl,
            cd0_clean: cd0,
            induced_factor: k,
            flap_delta_cd: perf.oei_climb_delta_cd,
            windmilling_cd: evidence.condition.windmilling_cd.unwrap_or(f64::NAN),
            trim_cd: evidence.condition.asymmetric_trim_cd.unwrap_or(f64::NAN),
        });
        // Break-even values of the terms the shortfall could be assigned to,
        // and the best-climb point (CL_opt = sqrt(CD0_total / k)).
        if let (Some(dd), Some(req_gradient)) = (d, far25_oei_gradient(n)) {
            let cd_fixed = cd0
                + perf.oei_climb_delta_cd
                + evidence.condition.windmilling_cd.unwrap_or(0.0)
                + evidence.condition.asymmetric_trim_cd.unwrap_or(0.0);
            let cl_opt = (cd_fixed / k).sqrt();
            let best = dd.thrust_term - (cd_fixed / cl_opt + k * cl_opt);
            let r_star = evidence
                .condition
                .condition_to_sls_thrust_ratio
                .unwrap_or(f64::NAN)
                + (req_gradient - dd.gradient) * n as f64 / ((n as f64 - 1.0) * installed_tw);
            eprintln!(
                "BRK  {:11} short {:+.4} | r* {:.3} | trim-CD to remove {:.4} (has {:.4}) | best-climb CL {:.2} (V2/VSR {:.2}) gradient {:.4} vs {:.3}",
                registered.name,
                req_gradient - dd.gradient,
                r_star,
                (req_gradient - dd.gradient) * cl,
                evidence.condition.asymmetric_trim_cd.unwrap_or(f64::NAN),
                cl_opt,
                (perf.cl_max_to / cl_opt).sqrt(),
                best,
                req_gradient,
            );
        }
        eprintln!(
            "GRAD {:11} N{} clmaxTO {:.2} v2/vsr {:.3} CL {:.3} A {:.2} TW {:.4} r {:.3} cd0 {:.4} k {:.4} e {:.3} flap {:.4} wm {:.4} trim {:.4} | {:?} req {:?}",
            registered.name,
            n,
            perf.cl_max_to,
            evidence.condition.v2_over_vstall,
            cl,
            ar,
            installed_tw,
            evidence.condition.condition_to_sls_thrust_ratio.unwrap_or(f64::NAN),
            cd0,
            k,
            oswald_factor(ar, k).unwrap_or(f64::NAN),
            perf.oei_climb_delta_cd,
            evidence.condition.windmilling_cd.unwrap_or(f64::NAN),
            evidence.condition.asymmetric_trim_cd.unwrap_or(f64::NAN),
            d.map(|d| (d.thrust_term, d.l_over_d, d.gradient, d.drag_over_lift)),
            far25_oei_gradient(n),
        );
    }
}

/// The terms of one preset's second-segment balance, as the residual builds them.
fn preset_terms(preset: &str) -> (GradientInputs, f64, f64) {
    let (config, outcome) = candidate(preset);
    let vmc = evaluate_candidate(&outcome, &config);
    let resolved = performance_with_vmc(&config, &vmc);
    let departure = outcome.departure.unwrap();
    let s_ref = outcome.plane.s_ref;
    let mass = outcome.sized.takeoff_mass_kg;
    let model = EngineOutModel::new(
        &resolved,
        Some(YawAirframe {
            plane: &outcome.plane,
            cg_x_m: outcome.cg_x,
        }),
        s_ref,
        Some(outcome.static_thrust_kn * 1_000.0),
    )
    .ok()
    .map(|m| m.with_weight_kg(mass));
    let installed_tw =
        outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0 / (mass * 9.806_65);
    // The residual's own improved-climb resolution, so the speed is the same.
    let (resolved, _) = super::improved_climb::resolve_improved_climb(
        &resolved,
        &super::improved_climb::ImprovedClimbInputs {
            model: model.as_ref().unwrap(),
            departure,
            mass_kg: mass,
            s_ref_m2: s_ref,
            drag: &outcome.sized.fuel_artifacts.drag,
            installed_tw_sls: installed_tw,
        },
    );
    let perf = &resolved.performance;
    let speeds = compute_v_speeds_at_masses(
        mass,
        mass,
        s_ref,
        departure,
        perf.cl_max_to,
        perf.cl_max_land,
        perf,
    );
    let drag = &outcome.sized.fuel_artifacts.drag;
    let atmosphere =
        alas_atmo::us1976_compute_values(departure.elevation_m, departure.isa_deviation_c);
    let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
    let model = EngineOutModel::new(
        &resolved,
        Some(YawAirframe {
            plane: &outcome.plane,
            cg_x_m: outcome.cg_x,
        }),
        s_ref,
        Some(outcome.static_thrust_kn * 1_000.0),
    )
    .ok()
    .map(|m| m.with_weight_kg(mass));
    let evidence = second_segment_evidence(model.as_ref(), perf, departure, &speeds, &|m, h, d| {
        drag.cd_at_atmosphere(0.0, m, h, d)
    });
    let cl = oei_cl_at_v2(perf.cl_max_to, evidence.condition.v2_over_vstall).unwrap();
    let cd0 = drag.cd_at_atmosphere(0.0, mach, departure.elevation_m, departure.isa_deviation_c);
    let cd = drag.cd_airport_base_at_atmosphere(
        cl,
        mach,
        departure.elevation_m,
        departure.isa_deviation_c,
    );
    let installed_tw =
        outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0 / (mass * 9.806_65);
    let rows =
        super::performance_residuals(&outcome, &config, crate::mdo::ResidualRole::Constraint);
    let required_sls = rows
        .iter()
        .find(|row| row.id == "oei_second_segment")
        .unwrap()
        .limit;
    (
        GradientInputs {
            n_engines: outcome.n_engines,
            installed_tw_sls: installed_tw,
            thrust_ratio: evidence.condition.condition_to_sls_thrust_ratio.unwrap(),
            cl,
            cd0_clean: cd0,
            induced_factor: (cd - cd0) / (cl * cl),
            flap_delta_cd: perf.oei_climb_delta_cd,
            windmilling_cd: evidence.condition.windmilling_cd.unwrap(),
            trim_cd: evidence.condition.asymmetric_trim_cd.unwrap(),
        },
        required_sls,
        far25_oei_gradient(outcome.n_engines).unwrap(),
    )
}

/// The decomposition is the assessor's balance written term by term: at the
/// installed thrust, the available gradient exceeds the requirement by exactly
/// `(N-1)/N r (T/W_installed - T/W_required_SLS)`. Hand relation, checked on
/// a narrow-body twin that passes (A320-200) and on the wide-body twin that
/// misses (B787-9).
#[test]
fn the_decomposition_matches_the_residuals_required_thrust_for_the_a320_and_the_b787() {
    // With the Jenkinson take-off charts (V2 thrust ratio 0.83 at BPR 9.6)
    // the B787-9 twin meets at its installed thrust with 9 % to spare; the
    // missing side is the same twin with 12 % less installed T/W.
    for (preset, tw_scale, passes) in [
        ("A320-200", 1.0, true),
        ("B787-9", 1.0, true),
        ("B787-9", 0.88, false),
    ] {
        let (mut inputs, required_sls, gamma) = preset_terms(preset);
        inputs.installed_tw_sls *= tw_scale;
        let d = decompose_gradient(&inputs).unwrap();
        let n = inputs.n_engines as f64;
        let hand = (n - 1.0) / n * inputs.thrust_ratio * (inputs.installed_tw_sls - required_sls);
        assert!(
            (d.gradient - gamma - hand).abs() < 1e-9,
            "{preset}: gradient {} vs requirement {gamma} + {hand}",
            d.gradient
        );
        assert_eq!(d.gradient >= gamma, passes, "{preset}: {d:?}");
        // The per-lift terms add up to the total drag over lift.
        let sum: f64 = d.drag_over_lift.iter().sum();
        assert!((sum - d.cd_total / inputs.cl).abs() < 1e-12);
        // Plausibility of the pieces against handbook ranges: deck thrust
        // ratio at V2 and the L/D at V2 with one engine out.
        assert!((0.65..0.85).contains(&inputs.thrust_ratio), "{preset}");
        assert!(
            (9.0..15.0).contains(&d.l_over_d),
            "{preset}: {}",
            d.l_over_d
        );
    }
}
/// Deck-based cruise-altitude requirements against the flat lapse:
/// `cargo test -p alas-opt --release cruise_table -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic table of all registered presets; slow"]
fn cruise_table() {
    use super::cruise_deck::deck_cruise_requirements;
    for registered in alas_config::presets::registry() {
        let (config, outcome) = candidate(registered.name);
        let req = &config.requirements;
        let s_ref = outcome.plane.s_ref;
        let mass = outcome.sized.takeoff_mass_kg;
        let model = EngineOutModel::new(
            &config,
            None,
            s_ref,
            Some(outcome.static_thrust_kn * 1_000.0),
        )
        .unwrap();
        let installed =
            outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0 / (mass * 9.806_65);
        let drag = &outcome.sized.fuel_artifacts.drag;
        let deck = deck_cruise_requirements(
            &model,
            drag,
            req,
            mass,
            outcome.sized.mid_cruise_mass_kg(),
            s_ref,
        );
        let atmosphere = alas_atmo::Atmosphere::new(req.cruise_altitude_m);
        let q =
            0.5 * atmosphere.density() * (req.cruise_mach * atmosphere.speed_of_sound()).powi(2);
        let cl = mass * 9.806_65 / (s_ref * q);
        let lapse_tw = drag.cd(cl, req.cruise_mach, req.cruise_altitude_m)
            / cl
            / config.performance.thrust_lapse;
        eprintln!(
            "CRZ {:11} h {:6.0} M {:.2} CL {:.3} installed {:.4} lapse {:.3} -> {:.4} | deck {:?}",
            registered.name,
            req.cruise_altitude_m,
            req.cruise_mach,
            cl,
            installed,
            config.performance.thrust_lapse,
            lapse_tw,
            deck.map(|d| (d.thrust_ratio, d.level_tw, d.top_of_climb_tw)),
        );
    }
}

/// Improved-climb V2/VSR and the take-off field length per preset (ISA sea
/// level, MTOW), with and without the improved climb:
/// `cargo test -p alas-opt --release improved_climb_table -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic table of all registered presets; slow"]
fn improved_climb_table() {
    use super::improved_climb::{resolve_improved_climb, ImprovedClimbInputs};
    use alas_config::airports::Airport;
    use alas_perf::performance::compute_field_performance_at_masses;
    for registered in alas_config::presets::registry() {
        let (config, outcome) = candidate(registered.name);
        let vmc = evaluate_candidate(&outcome, &config);
        let resolved = performance_with_vmc(&config, &vmc);
        let Some(departure) = outcome.departure else {
            continue;
        };
        let s_ref = outcome.plane.s_ref;
        let mass = outcome.sized.takeoff_mass_kg;
        let model = EngineOutModel::new(
            &resolved,
            Some(YawAirframe {
                plane: &outcome.plane,
                cg_x_m: outcome.cg_x,
            }),
            s_ref,
            Some(outcome.static_thrust_kn * 1_000.0),
        )
        .ok()
        .map(|m| m.with_weight_kg(mass));
        let installed_tw =
            outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0 / (mass * 9.806_65);
        let (improved, selection) = resolve_improved_climb(
            &resolved,
            &ImprovedClimbInputs {
                model: model.as_ref().unwrap(),
                departure,
                mass_kg: mass,
                s_ref_m2: s_ref,
                drag: &outcome.sized.fuel_artifacts.drag,
                installed_tw_sls: installed_tw,
            },
        );
        let sea_level = Airport::custom("ISA sea level", 0.0, 10_000.0, 10_000.0, 0.0, 0.0, 0.0);
        let todr = |c: &AlasConfig| {
            let p = &c.performance;
            compute_field_performance_at_masses(
                mass,
                mass,
                s_ref,
                &sea_level,
                p.cl_max_to,
                p.cl_max_land,
                installed_tw,
                p.k_land,
                1.0,
                p,
            )
            .todr_m
        };
        eprintln!(
            "IMP {:11} {:?} | schedule V2/VSR {:.3} -> {:.3} | TOFL {:.0} -> {:.0} m (x{:.3})",
            registered.name,
            selection.map(|s| (
                s.improved,
                s.meets,
                s.required_at_schedule,
                s.required,
                installed_tw
            )),
            improved.performance.schedule_takeoff_speed_ratios().v2,
            improved.performance.takeoff_speed_ratios().v2,
            todr(&resolved),
            todr(&improved),
            improved.performance.improved_climb_field_factor(),
        );
    }
}

/// Why the plan level and the criterion disagree: rates around the plan level.
#[test]
#[ignore = "diagnostic"]
fn toc_probe() {
    use crate::mdo::sizing::candidate_model::candidate_mission_model;
    for name in ["AVE", "B747-400", "A340-300"] {
        let (config, outcome) = candidate(name);
        let artifacts = &outcome.sized.fuel_artifacts;
        let plan = artifacts.frozen_plan.as_ref().unwrap();
        let model = candidate_mission_model(&config, artifacts).unwrap();
        eprintln!(
            "TOC {name}: plan h {:.1} m, plan mass {:.0}, sized mass {:.0}, design h {:.0}, policy {:?}, steps {:?}",
            plan.cruise_altitude_m,
            plan.takeoff_mass_kg,
            outcome.sized.takeoff_mass_kg,
            config.requirements.cruise_altitude_m,
            plan.policy,
            plan.step_positions
        );
        for dh in [-1000.0, -300.0, -100.0, -10.0, 0.0, 10.0, 100.0, 300.0] {
            let h = plan.cruise_altitude_m + dh;
            eprintln!(
                "   dh {dh:+7.0}: rate {:?} (sized mass {:?})",
                model
                    .top_of_climb_rate_m_s(
                        plan.takeoff_mass_kg,
                        h,
                        plan.range_m,
                        plan.climb_rate_scales
                    )
                    .ok(),
                model
                    .top_of_climb_rate_m_s(
                        outcome.sized.takeoff_mass_kg,
                        h,
                        plan.range_m,
                        plan.climb_rate_scales
                    )
                    .ok()
            );
        }
    }
}

/// Improved climb on the B787-9 candidate with an installed T/W that the best-climb
/// speed can reach (0.265, between the 0.2583 required at the best speed and the 0.2721 at
/// the 1.13 VSR schedule; re-pinned by lane field lengths, 2026-10-17, by probe: the
/// CS 25.107 schedule and the capped BPR 8 take-off chart move both): the lowest
/// meeting speed is resolved into the configuration, raises V2 and charges the jet
/// field length by the square of the speed ratio.
#[test]
fn an_attainable_gradient_resolves_the_lowest_speed_and_charges_the_field() {
    use super::improved_climb::{resolve_improved_climb, ImprovedClimbInputs};
    use alas_config::airports::Airport;
    use alas_perf::performance::compute_field_performance_at_masses;
    let (config, outcome) = candidate("B787-9");
    let vmc = evaluate_candidate(&outcome, &config);
    let resolved = performance_with_vmc(&config, &vmc);
    let s_ref = outcome.plane.s_ref;
    let mass = outcome.sized.takeoff_mass_kg;
    let model = EngineOutModel::new(
        &resolved,
        Some(YawAirframe {
            plane: &outcome.plane,
            cg_x_m: outcome.cg_x,
        }),
        s_ref,
        Some(outcome.static_thrust_kn * 1_000.0),
    )
    .unwrap()
    .with_weight_kg(mass);
    let inputs = |installed_tw_sls: f64| ImprovedClimbInputs {
        model: &model,
        departure: outcome.departure.unwrap(),
        mass_kg: mass,
        s_ref_m2: s_ref,
        drag: &outcome.sized.fuel_artifacts.drag,
        installed_tw_sls,
    };
    // 0.2650 is reachable only above the schedule speed.
    let (improved, selection) = resolve_improved_climb(&resolved, &inputs(0.2650));
    let selection = selection.unwrap();
    assert!(selection.improved && selection.meets, "{selection:?}");
    assert!(selection.required <= 0.2650 && selection.required > 0.2649);
    let x = selection.v2_over_vsr;
    let schedule = selection.schedule_v2_over_vsr;
    assert!((schedule - 1.13).abs() < 1e-12, "{schedule}");
    assert!(x > schedule && x < 1.42, "{x}");
    let perf = &improved.performance;
    assert_eq!(perf.improved_climb_v2_over_vsr, Some(x));
    assert!((perf.takeoff_speed_ratios().v2 - x).abs() < 1e-12);
    assert!((perf.schedule_takeoff_speed_ratios().v2 - schedule).abs() < 1e-12);
    assert!((perf.improved_climb_field_factor() - (x / schedule).powi(2)).abs() < 1e-12);
    // The jet field length is the schedule length times that factor.
    let sea_level = Airport::custom("ISA sea level", 0.0, 10_000.0, 10_000.0, 0.0, 0.0, 0.0);
    let todr = |c: &AlasConfig| {
        let p = &c.performance;
        compute_field_performance_at_masses(
            mass,
            mass,
            s_ref,
            &sea_level,
            p.cl_max_to,
            p.cl_max_land,
            0.28,
            p.k_land,
            1.0,
            p,
        )
        .todr_m
    };
    assert!((todr(&improved) / todr(&resolved) - (x / schedule).powi(2)).abs() < 1e-9);
    // The schedule that already meets, the switch and the best-climb miss.
    let (kept, selection) = resolve_improved_climb(&resolved, &inputs(0.40));
    assert!(!selection.unwrap().improved);
    assert!(kept.performance.improved_climb_v2_over_vsr.is_none());
    let mut off = resolved.clone().into_owned();
    off.performance.improved_climb_v2 = false;
    let (same, selection) = resolve_improved_climb(&off, &inputs(0.2650));
    assert!(selection.is_none() && same.performance.improved_climb_v2_over_vsr.is_none());
    let (missed, selection) = resolve_improved_climb(&resolved, &inputs(0.2350));
    let selection = selection.unwrap();
    assert!(selection.improved && !selection.meets);
    assert!(missed.performance.improved_climb_v2_over_vsr.is_none());
}
