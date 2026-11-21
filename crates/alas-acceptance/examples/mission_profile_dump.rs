// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Print the flown mission of a registered preset segment by segment: altitude,
//! range, true airspeed, Mach, throttle, thrust, flight-path angle and rate.
//!
//! `cargo run -p alas-acceptance --example mission_profile_dump -- <preset> [mission payload kg]`

#![allow(clippy::print_stdout, clippy::expect_used)]

use alas_acceptance::plausibility::case::{case_config, run_pipeline};
use alas_config::MtowSizing;

const FT: f64 = 0.3048;

fn main() {
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(run)
        .expect("spawn")
        .join()
        .expect("join");
}

fn run() {
    let _ = tracing::subscriber::set_global_default(LengtheningCounter);
    let preset = std::env::args().nth(1).expect("preset name");
    let mut config = case_config(&preset, None, MtowSizing::FixedRequirement).expect("config");
    // Experiment: one cruise level instead of the preset's three-level ladder.
    if std::env::var("ALAS_DUMP_CRUISE_LEGS").is_ok_and(|v| v == "1") {
        let altitude_m = config.requirements.cruise_altitude_m;
        alas_mission::configure_cruise_legs(&mut config.mission.profile, 1, altitude_m, 0.0);
    }
    let result = match std::env::args().nth(2).and_then(|p| p.parse::<f64>().ok()) {
        Some(payload_kg) => alas_pipeline::DesignPipeline::new(config.clone())
            .with_mission_payload_override_kg(payload_kg)
            .run(
                &alas_pipeline::PipelineOptions {
                    optimize: false,
                    compare_baseline: true,
                    parallel: true,
                    aerodynamic_solver: Default::default(),
                    optimization_solver: Default::default(),
                    output_dir: None,
                    save_plots: false,
                    seed: Some(42),
                    quiet: true,
                },
                &alas_pipeline::RunEnvironment::default(),
            )
            .expect("pipeline"),
        None => run_pipeline(&config).expect("pipeline"),
    };
    let Some(mission) = result.mission_result.as_ref() else {
        println!("no mission: {:?}", result.feasibility.native_mission_error);
        return;
    };
    if std::env::var("ALAS_DUMP_SOLUTIONS").is_ok() {
        for (s, solution) in mission.segments.iter().zip(&mission.solutions) {
            println!(
                "SOL {} converged {} throttle_limited {} idle_limited {} status {:?}",
                s.spec.tag,
                solution.converged,
                solution.throttle_limited,
                solution.idle_floor_limited,
                solution.status
            );
        }
    }
    println!("tag h0_ft h1_ft t_min r_km tas0 tas1 m0 m1 thr0 thr1 thrust0_kN thrust1_kN gamma_deg vs_fpm mass0 mass1");
    let (mut climb_s, mut cruise_s, mut descent_s, mut change_s) = (0.0, 0.0, 0.0, 0.0);
    let (mut climb_kg, mut climb_m, mut descent_m, mut top_m) = (0.0, 0.0, 0.0, 0.0_f64);
    for s in &mission.segments {
        let c = &s.conditions;
        let n = c.time_s.len();
        if n == 0 {
            continue;
        }
        let l = n - 1;
        let dt = c.time_s[l] - c.time_s[0];
        let dh = c.altitude_m[l] - c.altitude_m[0];
        let dr = c.aircraft_range_m[l] - c.aircraft_range_m[0];
        let gamma = if dr.abs() > 1.0 {
            (dh / dr).atan().to_degrees()
        } else {
            0.0
        };
        match s.spec.kind {
            alas_mission::segments::SegmentKind::Climb { .. } => {
                climb_s += dt;
                climb_kg += c.total_mass_kg[0] - c.total_mass_kg[l];
                climb_m += dr;
            }
            alas_mission::segments::SegmentKind::Descent { .. } => {
                descent_s += dt;
                descent_m += dr;
            }
            alas_mission::segments::SegmentKind::Cruise { .. } => {
                if s.spec.tag.starts_with("speed_change_to_") {
                    change_s += dt;
                } else {
                    cruise_s += dt;
                }
            }
        }
        top_m = top_m.max(c.altitude_m[l]);
        let vs = if dt > 0.0 { dh / dt / FT * 60.0 } else { 0.0 };
        println!(
            "{} {:.0} {:.0} {:.1} {:.1} {:.1} {:.1} {:.3} {:.3} {:.3} {:.3} {:.1} {:.1} {:.2} {:.0} {:.0} {:.0}",
            s.spec.tag,
            c.altitude_m[0] / FT,
            c.altitude_m[l] / FT,
            dt / 60.0,
            dr / 1e3,
            c.velocity_m_s[0],
            c.velocity_m_s[l],
            c.mach[0],
            c.mach[l],
            c.throttle[0],
            c.throttle[l],
            c.thrust[0].thrust_n / 1e3,
            c.thrust[l].thrust_n / 1e3,
            gamma,
            vs,
            c.total_mass_kg[0],
            c.total_mass_kg[l],
        );
    }
    if let Ok(prefix) = std::env::var("ALAS_DUMP_POINTS") {
        for s in mission
            .segments
            .iter()
            .filter(|s| s.spec.tag.starts_with(&prefix))
        {
            let c = &s.conditions;
            for i in 0..c.time_s.len() {
                println!(
                    "NPOINT {} t {:.1} h {:.0} ft v {:.2} thr {:.3} thrust {:.0} N vz {:.2} m/s mass {:.0}",
                    s.spec.tag, c.time_s[i], c.altitude_m[i] / FT, c.velocity_m_s[i], c.throttle[i], c.thrust[i].thrust_n, -c.velocity_vector_m_s[i][2], c.total_mass_kg[i]
                );
            }
        }
    }
    let report = result
        .baseline_analysis
        .as_ref()
        .or(result.optimized_report.as_ref());
    if let Some(d) = result.feasibility.fuel_loading.dispatch.as_ref() {
        println!(
            "PRICED {preset} outcome {} takeoff_kg {:.0} trip_fuel_kg {:.0} trip_min {:.1} native_trip_kg {:?}",
            d.outcome.as_str(),
            d.takeoff_mass_kg,
            d.plan.map_or(f64::NAN, |p| p.trip.kg),
            d.plan.map_or(f64::NAN, |p| p.trip_time_s / 60.0),
            d.native_check.map(|c| c.native_trip_fuel_kg),
        );
        if let (Some(report), Some(range_m)) = (report, d.route_distance_m) {
            let started = std::time::Instant::now();
            match alas_pipeline::fuel_model::report_route_mission_model(&result.config, report)
                .and_then(|m| m.fly_trip(d.takeoff_mass_kg, range_m).map_err(|e| e.to_string()))
            {
                Ok(f) => println!(
                    "PRICEDLEG {preset} fuel_kg {:.0} time_min {:.1} climb_min {:.1} climb_fuel_kg {:.0} climb_km {:.1} descent_km {:.1} cruise_alt_ft {:.0} final_alt_ft {:.0} rate_limited_steps {} min_climb_fpm {:.0} min_descent_fpm {:.0} fly_trip_ms {:.0} bands_ft {:?} cruise_tas_m_s {:?}",
                    f.leg.fuel_kg,
                    f.leg.time_s / 60.0,
                    f.climb_time_s / 60.0,
                    f.climb_fuel_kg,
                    f.climb_footprint_m / 1e3,
                    f.descent_footprint_m / 1e3,
                    f.cruise_altitude_m / FT,
                    f.final_cruise_altitude_m / FT,
                    f.rate_limited_steps,
                    f.minimum_climb_rate_m_s / FT * 60.0,
                    f.minimum_descent_rate_m_s / FT * 60.0,
                    started.elapsed().as_secs_f64() * 1e3,
                    f.climb_bands.iter().map(|b| (b / FT).round()).collect::<Vec<_>>(),
                    f.cruise_states.iter().map(|s| ((s.altitude_m / FT).round(), (s.tas_m_s * 10.0).round() / 10.0)).collect::<Vec<_>>(),
                ),
                Err(e) => println!("PRICEDLEG {preset} error {e}"),
            }
        }
    }
    for line in
        alas_acceptance::plausibility::takeoff_leg::leg_lines(&preset, mission, &result, report)
    {
        println!("{line}");
    }
    if let Some(line) = alas_acceptance::plausibility::takeoff_leg::reference_area_line(
        &preset, mission, &result, report,
    ) {
        println!("{line}");
    }
    if let Some(report) = report {
        use alas_acceptance::plausibility::mission_priced::{
            check_consistency, check_priced_profile, priced_flight, LegTotals,
        };
        use alas_acceptance::plausibility::{mission::MissionFacts, Findings};
        let native = MissionFacts::from_result(mission, result.config.requirements.cruise_mach);
        // Throttle 1.0 of the native flight against the priced deck's maximum-climb rating at the
        // same altitude and true airspeed (both read the same propulsion system).
        if let Ok(model) =
            alas_pipeline::fuel_model::report_route_mission_model(&result.config, report)
        {
            let mut shown = 0;
            for s in &mission.segments {
                let c = &s.conditions;
                for i in (0..c.time_s.len()).step_by(5) {
                    if (c.throttle[i] - 1.0).abs() > 5.0e-4 || shown >= 10 {
                        continue;
                    }
                    let rated = model
                        .propulsion
                        .flight_condition(
                            c.altitude_m[i],
                            c.velocity_m_s[i],
                            c.gravity_m_s2[i],
                            model.isa_deviation_c,
                        )
                        .ok()
                        .and_then(|f| {
                            model
                                .propulsion
                                .rated_point(f, alas_prop::system::PropulsionRating::MaximumClimb)
                                .ok()
                        });
                    if let Some(rated) = rated {
                        shown += 1;
                        println!(
                            "THRUSTCHECK {preset} {} h {:.0} ft tas {:.1} native {:.0} N priced max-climb {:.0} N ratio {:.4}",
                            s.spec.tag, c.altitude_m[i] / FT, c.velocity_m_s[i], c.thrust[i].thrust_n, rated.thrust_n, c.thrust[i].thrust_n / rated.thrust_n
                        );
                    }
                }
            }
        }
        // Native descent thrust against the priced flight-idle point at the same state.
        if let Ok(model) =
            alas_pipeline::fuel_model::report_route_mission_model(&result.config, report)
        {
            for s in &mission.segments {
                if !matches!(
                    s.spec.kind,
                    alas_mission::segments::SegmentKind::Descent { .. }
                ) {
                    continue;
                }
                let c = &s.conditions;
                let i = c.time_s.len() / 2;
                let idle = model
                    .propulsion
                    .flight_condition(
                        c.altitude_m[i],
                        c.velocity_m_s[i],
                        c.gravity_m_s2[i],
                        model.isa_deviation_c,
                    )
                    .ok()
                    .and_then(|f| model.propulsion.idle_point(f).ok());
                if let Some(idle) = idle {
                    println!(
                        "IDLECHECK {preset} {} h {:.0} ft tas {:.1} throttle {:.3} native {:.0} N native floor throttle {:.3} priced idle {:.0} N fuel native {:.3} priced idle {:.3} kg/s",
                        s.spec.tag, c.altitude_m[i] / FT, c.velocity_m_s[i], c.throttle[i], c.thrust[i].thrust_n, c.available_throttle_floor[i], idle.thrust_n, c.vehicle_mass_rate_kg_s[i].abs(), idle.fuel_flow_kg_s
                    );
                }
            }
        }
        match alas_pipeline::fuel_model::report_route_mission_model(&result.config, report)
            .and_then(|m| priced_flight(&m, &native))
            .map(|steps| MissionFacts::from_priced(&steps, native.design_mach))
        {
            Ok(priced) => {
                {
                    let top_m = native
                        .segments
                        .iter()
                        .map(|s| s.h0_m.max(s.h1_m))
                        .fold(0.0_f64, f64::max);
                    let edges: Vec<f64> = [top_m, 9144.0, 6096.0, 3048.0 + 10.0, 1524.0, 0.0]
                        .into_iter()
                        .chain(std::iter::empty())
                        .collect();
                    let mut nat: Vec<(f64, f64, f64, f64)> = Vec::new();
                    for s in &mission.segments {
                        if matches!(
                            s.spec.kind,
                            alas_mission::segments::SegmentKind::Descent { .. }
                        ) {
                            let c = &s.conditions;
                            for i in 0..c.time_s.len() {
                                nat.push((
                                    c.altitude_m[i],
                                    c.time_s[i],
                                    c.aircraft_range_m[i],
                                    c.total_mass_kg[i],
                                ));
                            }
                        }
                    }
                    let mut pr: Vec<(f64, f64, f64, f64)> = Vec::new();
                    if let Ok(leg) = alas_pipeline::fuel_model::report_route_mission_model(
                        &result.config,
                        report,
                    )
                    .and_then(|m| {
                        m.fly_trip(
                            result
                                .feasibility
                                .fuel_loading
                                .dispatch
                                .as_ref()
                                .map_or(0.0, |d| d.takeoff_mass_kg),
                            native.range_m(),
                        )
                        .map_err(|e| e.to_string())
                    }) {
                        for st in leg.steps.iter().filter(|st| {
                            matches!(
                                st.phase,
                                alas_opt::mdo::mission_model::StepPhase::Descent
                                    | alas_opt::mdo::mission_model::StepPhase::Landing
                            )
                        }) {
                            pr.push((st.altitude_m[0], st.time_s[0], st.range_m[0], st.mass_kg[0]));
                            pr.push((st.altitude_m[1], st.time_s[1], st.range_m[1], st.mass_kg[1]));
                        }
                    }
                    if std::env::var("ALAS_DUMP_POINTS").is_ok() {
                        if let Ok(leg) = alas_pipeline::fuel_model::report_route_mission_model(
                            &result.config,
                            report,
                        )
                        .and_then(|m| {
                            m.fly_trip(
                                result
                                    .feasibility
                                    .fuel_loading
                                    .dispatch
                                    .as_ref()
                                    .map_or(0.0, |d| d.takeoff_mass_kg),
                                native.range_m(),
                            )
                            .map_err(|e| e.to_string())
                        }) {
                            for st in leg.steps.iter().filter(|st| {
                                matches!(st.phase, alas_opt::mdo::mission_model::StepPhase::Descent)
                            }) {
                                let dt = st.time_s[1] - st.time_s[0];
                                println!(
                                    "PPOINT t {:.1} h {:.0}-{:.0} ft v {:.2} thrust {:.0} N drag {:.0} N vz {:.2} m/s mass {:.0}",
                                    st.time_s[0], st.altitude_m[0] / FT, st.altitude_m[1] / FT, st.tas_m_s[0], st.thrust_n, st.drag_n, (st.altitude_m[0] - st.altitude_m[1]) / dt, st.mass_kg[0]
                                );
                            }
                        }
                    }
                    let mut edges = edges;
                    edges[0] = nat
                        .first()
                        .map_or(top_m, |p| p.0)
                        .min(pr.first().map_or(top_m, |p| p.0))
                        - 1.0;
                    let (nb, pb) = (descent_bands(&nat, &edges), descent_bands(&pr, &edges));
                    for (i, (n, p)) in nb.iter().zip(&pb).enumerate() {
                        println!(
                            "DESCENTBAND {preset} {:.0}-{:.0} ft native dt {:.0} s dr {:.1} km fuel {:.0} kg | priced dt {:.0} s dr {:.1} km fuel {:.0} kg",
                            edges[i] / FT, edges[i + 1] / FT, n.0, n.1 / 1e3, n.2, p.0, p.1 / 1e3, p.2
                        );
                    }
                }
                let (n, p) = (LegTotals::of(&native), LegTotals::of(&priced));
                println!("CONSIST {preset} native {n:?}");
                println!("CONSIST {preset} priced {p:?}");
                for s in native
                    .segments
                    .iter()
                    .filter(|s| !s.speed_change && (s.t1_s - s.t0_s) > 0.0)
                {
                    println!(
                        "NSEG {} {:?} h {:.0}->{:.0} ft t {:.1}->{:.1} min r {:.1}->{:.1} km tas {:.1}->{:.1} mach {:.3} eas_low {:.0} kt ld {:.1} cl {:.3} fuel {:.1} kg",
                        s.tag, s.kind, s.h0_m / FT, s.h1_m / FT, s.t0_s / 60.0, s.t1_s / 60.0, s.r0_m / 1e3, s.r1_m / 1e3, s.tas0_ms, s.tas1_ms, s.max_mach, s.max_eas_low_alt_ms / 0.514_444, s.ld_mean, s.cl_mean, s.mass0_kg - s.mass1_kg
                    );
                }
                for s in &priced.segments {
                    println!(
                        "PSEG {} {:?} h {:.0}->{:.0} ft t {:.1}->{:.1} min r {:.1}->{:.1} km tas {:.1}->{:.1} mach {:.3} eas_low {:.0} kt ld {:.1} cl {:.3} fuel {:.1} kg",
                        s.tag, s.kind, s.h0_m / FT, s.h1_m / FT, s.t0_s / 60.0, s.t1_s / 60.0, s.r0_m / 1e3, s.r1_m / 1e3, s.tas0_ms, s.tas1_ms, s.max_mach, s.max_eas_low_alt_ms / 0.514_444, s.ld_mean, s.cl_mean, s.mass0_kg - s.mass1_kg
                    );
                }
                let mut found = Findings::new(preset.clone());
                check_consistency(&mut found, &n, &p);
                check_priced_profile(&mut found, &priced);
                for f in found.items() {
                    println!("FINDING {f}");
                }
            }
            Err(e) => println!("CONSIST {preset} error {e}"),
        }
    }
    println!(
        "NATIVELEG {preset} climb_fuel_kg {climb_kg:.0} climb_km {:.1} descent_km {:.1} top_ft {:.0}",
        climb_m / 1e3,
        descent_m / 1e3,
        top_m / FT,
    );
    println!(
        "LENGTHENED {preset} {}",
        LENGTHENED.load(std::sync::atomic::Ordering::Relaxed)
    );
    println!(
        "SUMMARY {preset} fuel_kg {:.0} block_min {:.1} climb_min {:.1} cruise_min {:.1} descent_min {:.1} speed_change_min {:.1} range_km {:.1}",
        mission.initial_mass_kg() - mission.final_mass_kg(),
        mission.block_time_s() / 60.0,
        climb_s / 60.0,
        cruise_s / 60.0,
        descent_s / 60.0,
        change_s / 60.0,
        mission
            .segments
            .last()
            .and_then(|s| s.conditions.aircraft_range_m.last())
            .copied()
            .unwrap_or(0.0)
            / 1e3,
    );
}

/// Counts the planner's lengthening events (`speed_change.rs::lengthen`).
struct LengtheningCounter;

static LENGTHENED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

struct MessageContains(bool, String);

impl tracing::field::Visit for MessageContains {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" && format!("{value:?}").contains("lengthening a level") {
            self.0 = true;
        }
        if field.name() == "leg" {
            self.1 = format!("{value:?}");
        }
    }
}

impl tracing::Subscriber for LengtheningCounter {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut visitor = MessageContains(false, String::new());
        event.record(&mut visitor);
        if visitor.0 {
            println!("LENGTHENED_LEG {}", visitor.1);
            LENGTHENED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

/// Time, ground distance and fuel of `samples` (altitude m, time s, range m, mass kg; descending
/// altitude) between altitude `edges`, by first crossing with linear interpolation.
fn descent_bands(samples: &[(f64, f64, f64, f64)], edges: &[f64]) -> Vec<(f64, f64, f64)> {
    let at = |altitude_m: f64| -> (f64, f64, f64) {
        for pair in samples.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if a.0 >= altitude_m && b.0 <= altitude_m {
                let w = if (a.0 - b.0).abs() < 1.0e-9 {
                    0.0
                } else {
                    (a.0 - altitude_m) / (a.0 - b.0)
                };
                return (
                    a.1 + w * (b.1 - a.1),
                    a.2 + w * (b.2 - a.2),
                    a.3 + w * (b.3 - a.3),
                );
            }
        }
        let last = samples.last().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
        (last.1, last.2, last.3)
    };
    edges
        .windows(2)
        .map(|pair| {
            let (upper, lower) = (at(pair[0]), at(pair[1]));
            (lower.0 - upper.0, lower.1 - upper.1, upper.2 - lower.2)
        })
        .collect()
}
