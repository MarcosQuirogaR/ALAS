// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Probe (ignored): where the registered A320 LTO field of about 2,109 m
//! (ISA sea level, report take-off mass) and the optimizer's field row of the
//! same aircraft (route aerodrome at 610 m, design gross mass 78,000 kg) part.
//! The same field method (`calculate_with_airframe`) is evaluated on a grid of
//! aerodrome and mass. Run with
//! `cargo test -p alas-report --test a320_field_gap_probe -- --ignored --nocapture`.
//!
//! The altitude probes (`a320_field_altitude_breakdown`, `a320_field_sensitivities`)
//! decompose the step from ISA sea level to 2,000 ft / the route aerodrome
//! (thrust ratio, ground runs, air legs, accelerate-stop, engine-out `V2`
//! gradient), and `the_altitude_step_of_the_a320_field_obeys_the_classical_scalings`
//! asserts the structure of that step (audit E1e MJ2).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use alas_config::airports::Airport;
use alas_config::AlasConfig;
use alas_opt::mdo::YawAirframe;
use alas_perf::performance::compute_v_speeds_at_masses;
use alas_pipeline::field_performance::{calculate_with_airframe, report_field_polar};
use alas_pipeline::field_reference::sea_level_static_thrust_n;
use alas_pipeline::full_analysis::FullAnalysis;
use alas_pipeline::report_config::{config_for_report_at, resolve_report_at};

#[test]
#[ignore = "probe: prints, asserts nothing"]
fn a320_field_by_aerodrome_and_mass() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let report = FullAnalysis::new(config.clone())
        .run(&design, true)
        .unwrap();
    let route = config.departure_airport_record().unwrap();
    let sea_level = Airport::custom("ISA sea level", 0.0, 10_000.0, 10_000.0, 0.0, 0.0, 0.0);
    let area_m2 = report.airplane.s_ref;
    let landing_cap_kg = config.requirements.mtow_kg * 0.85;
    eprintln!(
        "PROBE registered mtow_kg={} route=({} m elevation, ISA {}, TODA {} m) s_ref={area_m2:.2}",
        config.requirements.mtow_kg, route.elevation_m, route.isa_deviation_c, route.toda_m
    );
    for mass_kg in [60_000.0, 66_000.0, 73_500.0, 75_571.0, 78_000.0] {
        let landing_kg = landing_cap_kg.min(0.9 * mass_kg);
        for airport in [&sea_level, route] {
            let resolved =
                config_for_report_at(&config, &report, airport, mass_kg, landing_kg, area_m2);
            let cfg = &*resolved;
            let perf = &cfg.performance;
            let speeds = compute_v_speeds_at_masses(
                mass_kg,
                landing_kg,
                area_m2,
                airport,
                perf.cl_max_to,
                perf.cl_max_land,
                perf,
            );
            let (thrust_n, _) = sea_level_static_thrust_n(cfg, speeds.v_r_ms).unwrap();
            let tw = thrust_n / (mass_kg * cfg.requirements.gravity_m_s2);
            let atmosphere =
                alas_atmo::us1976_compute_values(airport.elevation_m, airport.isa_deviation_c);
            let polar = report_field_polar(
                cfg,
                &report,
                speeds.v2_ms / atmosphere.speed_of_sound_m_s,
                airport,
            )
            .unwrap();
            let field = calculate_with_airframe(
                cfg,
                Some(YawAirframe {
                    plane: &report.airplane,
                    cg_x_m: report.physical_cg[0],
                }),
                airport,
                area_m2,
                mass_kg,
                landing_kg,
                tw,
                &polar,
            )
            .unwrap();
            let solved = field.takeoff_field.as_ref().unwrap();
            let governs = if solved.tofl_m > solved.balanced_m * 1.0001 {
                "1.15 x all-engine"
            } else {
                "engine-out balanced"
            };
            eprintln!(
                "PROBE mass={mass_kg:.0} kg aerodrome={:<14} elev={:.0} m: TOFL={:.0} m ({governs}), BFL={:.0}, V2={:.1} m/s, V1={:.1} m/s, T/W={tw:.4}",
                airport.name, airport.elevation_m, solved.tofl_m, solved.balanced_m, speeds.v2_ms, solved.v1_m_s
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Altitude sensitivity of the jet field (audit E1e MJ2).
//
// The same field integration at ISA sea level, at 2,000 ft ISA (609.6 m, the
// Airbus aircraft-characteristics chart's second curve) and at the route
// aerodrome, with the deck's take-off thrust ratio, the all-engine and
// engine-out ground runs, the air distances, the accelerate-stop distance and
// the engine-out V2 gradient against the CS 25.121(b) 0.024.
// ---------------------------------------------------------------------------

use alas_perf::performance::{
    field_breakdown, takeoff_field, FieldBreakdown, TakeoffField, TakeoffFieldInputs,
};

/// The field at one aerodrome with its decomposition.
struct FieldCase {
    solved: TakeoffField,
    parts: FieldBreakdown,
    /// `calculate_with_airframe` (the pipeline's own path) for the cross-check.
    pipeline_tofl_m: f64,
    sigma: f64,
    thrust_ratio_static: f64,
    thrust_ratio_v2: f64,
    v2_tas_m_s: f64,
    v2_eas_m_s: f64,
    /// `V2 / VSR` of the schedule the aerodrome flies (improved climb included).
    v2_over_vsr: f64,
    cl_v2: f64,
    drag_terms: String,
    improved: String,
}

/// Evaluate the field at `airport`: `gear` scales the gear drag, `v2_factor`
/// the schedule `V2`, `failed` the failed-engine (windmilling plus trim) drag;
/// all three are 1.0 for the model as it is; `ground_effect` is 1.0 for the
/// ground-effect reduction of the lift-dependent drag and 0.0 for free air.
fn field_case(
    config: &AlasConfig,
    report: &alas_pipeline::full_analysis::AnalysisReport,
    airport: &Airport,
    mass_kg: f64,
    (gear, v2_factor, failed, ground_effect): (f64, f64, f64, f64),
) -> FieldCase {
    use alas_opt::mdo::{
        longest_on_ground_planes, with_jet_field_inputs, EngineOutCondition, EngineOutModel,
        EngineOutRating, JetFieldInputs, V2ClimbOutPolar,
    };

    let area_m2 = report.airplane.s_ref;
    let landing_kg = (config.requirements.mtow_kg * 0.85).min(0.9 * mass_kg);
    let resolution = resolve_report_at(config, report, Some(airport), mass_kg, landing_kg, area_m2);
    let improved = resolution.improved_climb.map_or_else(
        || "improved climb not evaluated".to_owned(),
        |s| {
            format!(
                "improved climb needed {} meets {} at V2/VSR {:.3}",
                s.improved, s.meets, s.v2_over_vsr
            )
        },
    );
    let resolved = resolution.config;
    let cfg = &*resolved;
    let perf = &cfg.performance;
    let mut speeds = compute_v_speeds_at_masses(
        mass_kg,
        landing_kg,
        area_m2,
        airport,
        perf.cl_max_to,
        perf.cl_max_land,
        perf,
    );
    speeds.v2_ms *= v2_factor;
    let (static_n, _) = sea_level_static_thrust_n(cfg, speeds.v_r_ms).unwrap();
    let atmosphere = alas_atmo::us1976_compute_values(airport.elevation_m, airport.isa_deviation_c);
    let polar = report_field_polar(
        cfg,
        report,
        speeds.v2_ms / atmosphere.speed_of_sound_m_s,
        airport,
    )
    .unwrap();
    let (cd0, k) = (polar.cd0, polar.k);
    let yaw = || YawAirframe {
        plane: &report.airplane,
        cg_x_m: report.physical_cg[0],
    };
    let pipeline_tofl_m = calculate_with_airframe(
        cfg,
        Some(yaw()),
        airport,
        area_m2,
        mass_kg,
        landing_kg,
        static_n / (mass_kg * cfg.requirements.gravity_m_s2),
        &polar,
    )
    .unwrap()
    .takeoff_field
    .unwrap()
    .tofl_m;
    let model = EngineOutModel::new(cfg, Some(yaw()), area_m2, None)
        .unwrap()
        .with_weight_kg(mass_kg);
    let at = |tas_m_s| EngineOutCondition {
        altitude_m: airport.elevation_m,
        isa_deviation_c: airport.isa_deviation_c,
        tas_m_s,
    };
    let point = model
        .point(at(speeds.v2_ms), EngineOutRating::TakeoffGoAround, cd0)
        .unwrap();
    let trim = point.asymmetric_trim_cd.unwrap();
    let ratio = |v: f64| {
        model
            .thrust(at(v), EngineOutRating::TakeoffGoAround, false)
            .unwrap()
            .1
    };
    // The pipeline's own inputs (`with_jet_field_inputs`) flown on every
    // ground plane of the lift-off lift, the longer field kept
    // (`longest_on_ground_planes`), as `calculate_with_airframe` does; the
    // knobs scale the gear and failed-engine drag and drop the ground effect
    // by struct update. In the air legs `failed` scales the failed engine's
    // zero-lift share of the climb-out locus (a trim the locus carries in its
    // span efficiency stays in the lift-dependent part).
    let plane = &report.airplane;
    let climb_out = Some(V2ClimbOutPolar::of(
        cfg,
        alas_opt::mdo::climb_out_aspect_ratio(cfg, plane.b_ref, plane.s_ref),
    ));
    let fly = |liftoff| {
        let jet = JetFieldInputs {
            perf,
            engine_out: &model,
            airport,
            wing_area_m2: area_m2,
            mass_kg,
            static_thrust_n: static_n,
            polar: &polar,
            speeds,
            climb_out,
            liftoff,
        };
        with_jet_field_inputs(&jet, |field| {
            let air = field.airborne_drag;
            let scaled_air = |cl: f64, engine_failed: bool| match air {
                Some(air) if engine_failed => {
                    let (healthy, _) = air(cl, false);
                    let (zero, lift) = air(cl, true);
                    (healthy + failed * (zero - healthy), lift)
                }
                Some(air) => air(cl, false),
                None => (f64::NAN, f64::NAN),
            };
            let field = TakeoffFieldInputs {
                gear_delta_cd: gear * field.gear_delta_cd,
                failed_engine_cd: failed * field.failed_engine_cd,
                ground_effect: field.ground_effect.filter(|_| ground_effect > 0.5),
                airborne_drag: air.map(|_| &scaled_air as &dyn Fn(f64, bool) -> (f64, f64)),
                ..*field
            };
            let solved = takeoff_field(&field)?;
            Ok((solved, field_breakdown(&field, &solved)?))
        })
    };
    let (solved, parts) =
        longest_on_ground_planes(Some(plane), cfg, fly, |case| case.0.tofl_m).unwrap();
    let sigma = atmosphere.density_kg_m3 / 1.225;
    let cl_v2 = mass_kg * 9.80665
        / (0.5 * atmosphere.density_kg_m3 * speeds.v2_ms * speeds.v2_ms * area_m2);
    // F2: the field's parabola against the drag table the second-segment
    // assessor evaluates at the same CL.
    let table_cd = report
        .fuel
        .artifacts(cfg, &report.design)
        .unwrap()
        .drag
        .cd_airport_base_at_atmosphere(
            cl_v2,
            speeds.v2_ms / atmosphere.speed_of_sound_m_s,
            airport.elevation_m,
            airport.isa_deviation_c,
            alas_opt::mdo::drag_table::high_lift_lift_shift(perf.cl_max_to, perf.cl_max_clean),
        );
    let parabola_cd = cd0 + k * cl_v2 * cl_v2;
    let table_note = format!(
        "table CD {table_cd:.4} vs field parabola {parabola_cd:.4} at CL {cl_v2:.3} (gradient {:+.4})",
        (parabola_cd - table_cd) / cl_v2
    );
    FieldCase {
        solved,
        parts,
        pipeline_tofl_m,
        sigma,
        thrust_ratio_static: ratio(0.0),
        thrust_ratio_v2: ratio(speeds.v2_ms),
        v2_tas_m_s: speeds.v2_ms,
        v2_eas_m_s: speeds.v2_ms * sigma.sqrt(),
        v2_over_vsr: speeds.v2_ms / speeds.v_stall_to_ms,
        cl_v2,
        improved: format!("{improved}; {table_note}"),
        drag_terms: format!(
            "field cd0 {cd0:.4} + flap {:.4} + gear {:.4} + windmilling {:.4} + trim {trim:.4} + k {k:.4} CL^2",
            perf.oei_climb_delta_cd, perf.takeoff_gear_delta_cd, point.windmilling_cd
        ),
    }
}

fn a320_report() -> (AlasConfig, alas_pipeline::full_analysis::AnalysisReport) {
    preset_report("A320-200")
}

fn preset_report(name: &str) -> (AlasConfig, alas_pipeline::full_analysis::AnalysisReport) {
    let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
    let design = alas_config::presets::get(name).unwrap().design_vector;
    let report = FullAnalysis::new(config.clone())
        .run(&design, true)
        .unwrap();
    (config, report)
}

fn isa_aerodrome(name: &str, elevation_m: f64) -> Airport {
    Airport::custom(name, elevation_m, 10_000.0, 10_000.0, 0.0, 0.0, 0.0)
}

/// Prints the decomposition at ISA sea level, ISA 2,000 ft and the route
/// aerodrome for `PROBE_MASS_KG` (default 78,000 kg). Run with
/// `cargo test -p alas-report --test a320_field_gap_probe a320_field_altitude -- --ignored --nocapture`.
#[test]
#[ignore = "probe: prints, asserts nothing"]
fn a320_field_altitude_breakdown() {
    let (config, report) = a320_report();
    let route = config.departure_airport_record().unwrap().clone();
    let mass_kg: f64 = std::env::var("PROBE_MASS_KG")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(78_000.0);
    let mut sea_level_tofl = None;
    for airport in [
        isa_aerodrome("ISA sea level", 0.0),
        isa_aerodrome("ISA 2000 ft", 609.6),
        route,
    ] {
        let c = field_case(&config, &report, &airport, mass_kg, (1.0, 1.0, 1.0, 1.0));
        let (s, b) = (&c.solved, &c.parts);
        eprintln!(
            "ALT {:<14} elev={:.1} m sigma={:.4} mass={mass_kg:.0} kg (pipeline TOFL {:.1} m, probe TOFL {:.1} m)",
            airport.name, airport.elevation_m, c.sigma, c.pipeline_tofl_m, s.tofl_m
        );
        eprintln!(
            "ALT   drag build at V2: {}; CL(V2) {:.3}; {}",
            c.drag_terms, c.cl_v2, c.improved
        );
        eprintln!(
            "ALT   deck take-off thrust ratio over SLS static: V=0 {:.4}, V2 {:.4}; V2 {:.2} m/s TAS = {:.2} EAS; VR {:.2}, VLOF {:.2}",
            c.thrust_ratio_static, c.thrust_ratio_v2, c.v2_tas_m_s, c.v2_eas_m_s, s.vr_m_s, b.all_engine_vlof_m_s
        );
        eprintln!(
            "ALT   all-engine: roll {:.0} + rotation {:.0} + air {:.0} = {:.0} m (x1.15 = {:.0}); gradient at V2 {:.4}",
            b.all_engine_roll_m, b.all_engine_rotation_m, b.all_engine_air_m, s.all_engine_m, 1.15 * s.all_engine_m, b.all_engine_gradient_v2
        );
        eprintln!(
            "ALT   engine-out continued: AEO roll to VEF {:.0} + OEI roll {:.0} + rotation {:.0} + air {:.0} = {:.0} m; OEI V2 gradient (gear down) {:.4} against 0.024",
            b.continued_roll_all_engine_m, b.continued_roll_one_engine_m, b.continued_rotation_m, b.continued_air_m, s.continued_m, b.engine_out_gradient_v2
        );
        eprintln!(
            "ALT   accelerate-stop {:.0} m (roll {:.0} + {:.0}, braking {:.0}, allowance {:.0}); V1 {:.1} VEF {:.1} m/s; BFL {:.0}; TOFL {:.0} m",
            s.accelerate_stop_m, b.stop_roll_all_engine_m, b.stop_roll_one_engine_m, b.stop_braking_m, b.stop_allowance_m, s.v1_m_s, s.vef_m_s, s.balanced_m, s.tofl_m
        );
        match sea_level_tofl {
            None => sea_level_tofl = Some(s.tofl_m),
            Some(sl) => eprintln!(
                "ALT   TOFL change vs sea level {:+.1} %",
                100.0 * (s.tofl_m / sl - 1.0)
            ),
        }
    }
}

/// What drives the altitude sensitivity: the gear drag, `V2` and the
/// failed-engine drag scaled one at a time. Run with
/// `cargo test -p alas-report --test a320_field_gap_probe a320_field_sens -- --ignored --nocapture`.
#[test]
#[ignore = "probe: prints, asserts nothing"]
fn a320_field_sensitivities() {
    let (config, report) = a320_report();
    let (sea_level, ft2000) = (
        isa_aerodrome("ISA sea level", 0.0),
        isa_aerodrome("ISA 2000 ft", 609.6),
    );
    for variant in [
        (1.0, 1.0, 1.0, 1.0),
        (1.0, 1.0, 1.0, 0.0),
        (0.5, 1.0, 1.0, 1.0),
        (0.0, 1.0, 1.0, 1.0),
        (1.0, 1.05, 1.0, 1.0),
        (1.0, 1.0, 0.5, 1.0),
        (1.0, 1.0, 0.0, 1.0),
        (1.0, 1.0, 1.0, 0.0),
    ] {
        let sl = field_case(&config, &report, &sea_level, 78_000.0, variant);
        let alt = field_case(&config, &report, &ft2000, 78_000.0, variant);
        eprintln!(
            "SENS gear x{:.1} V2 x{:.2} failed-engine drag x{:.1} ground effect {:.0}: SL TOFL {:.0} m (OEI air {:.0}, gradient {:.4}, 1.15xAEO {:.0}); 2000 ft TOFL {:.0} m (OEI air {:.0}, gradient {:.4}, 1.15xAEO {:.0}); change {:+.1} %",
            variant.0, variant.1, variant.2, variant.3,
            sl.solved.tofl_m, sl.parts.continued_air_m, sl.parts.engine_out_gradient_v2, 1.15 * sl.solved.all_engine_m,
            alt.solved.tofl_m, alt.parts.continued_air_m, alt.parts.engine_out_gradient_v2, 1.15 * alt.solved.all_engine_m,
            100.0 * (alt.solved.tofl_m / sl.solved.tofl_m - 1.0)
        );
    }
}

/// The altitude step of the model decomposes into the classical scalings
/// (audit E1e MJ2): no second density or pressure lapse, no true airspeed
/// where an equivalent one belongs, and an engine-out air leg that follows
/// `1 / gradient`. Asserts the structure, not the reference: the Airbus
/// aircraft-characteristics chart (3-3-1, CFM56, ISA, 78 t) reads 2,081 m at
/// sea level and 2,285 m at 2,000 ft (+9.8 %); the model's engine-out leg is
/// what makes its step larger at that weight (see the probe above).
#[test]
fn the_altitude_step_of_the_a320_field_obeys_the_classical_scalings() {
    let (config, report) = a320_report();
    let sea = field_case(
        &config,
        &report,
        &isa_aerodrome("ISA sea level", 0.0),
        78_000.0,
        (1.0, 1.0, 1.0, 1.0),
    );
    let alt = field_case(
        &config,
        &report,
        &isa_aerodrome("ISA 2000 ft", 609.6),
        78_000.0,
        (1.0, 1.0, 1.0, 1.0),
    );
    // The probe's integration is the pipeline's own.
    for case in [&sea, &alt] {
        assert!(
            (case.solved.tofl_m / case.pipeline_tofl_m - 1.0).abs() < 1e-9,
            "{} against {}",
            case.solved.tofl_m,
            case.pipeline_tofl_m
        );
    }
    // V2 / VSR is resolved at each aerodrome (audit E1f F1: 2,000 ft may need
    // the improved-climb speed that sea level does not), and VSR is the same
    // equivalent airspeed: the true airspeed rises as 1/sqrt(sigma).
    assert!(
        ((alt.v2_eas_m_s / alt.v2_over_vsr) / (sea.v2_eas_m_s / sea.v2_over_vsr) - 1.0).abs()
            < 0.002
    );
    assert!(
        (alt.v2_tas_m_s / sea.v2_tas_m_s * alt.sigma.sqrt() * sea.v2_over_vsr / alt.v2_over_vsr
            - 1.0)
            .abs()
            < 0.002
    );
    // One thrust lapse: the static deck ratio sits between delta^1.05 and
    // delta^0.7 of the pressure ratio at 609.6 m (delta = 0.9293), and is not
    // applied twice (a second delta lapse would put it near 0.86).
    let delta = 0.9293_f64;
    assert!(
        alt.thrust_ratio_static > delta.powf(1.05) && alt.thrust_ratio_static < delta.powf(0.7),
        "static thrust ratio {}",
        alt.thrust_ratio_static
    );
    // All-engine path: roll + rotation + air grows by the classical
    // 1/(sigma T/T0) with the lapse of the deck, within the planning band,
    // once the V2 schedule of each aerodrome is taken out: the distances to
    // a speed go as its square, and 2,000 ft flies the improved-climb V2/VSR
    // that sea level does not (a real A320 at 78 t does not
    // need it there). Measured with the product parasite
    // items on and off: raw ratio
    // 1.1849 on against 1.1553 off; V2/VSR 1.1300 at sea level both ways and
    // 1.1756 on against 1.1636 off at 2,000 ft (the higher CD0 lowers the
    // engine-out gradient, 0.01961 -> 0.01924, so the improved-climb speed
    // rises); with the schedule taken out, 1.0948 on and 1.0895 off.
    // The distance scales with V2 squared and V2 / VSR is resolved at each
    // aerodrome: with the tail at its dynamic-pressure ratio in the lattice the A320 at 2,000 ft needs the improved-climb speed (V2 / VSR 1.172
    // against the 1.13 floor at sea level), a regime switch of 3.5 % in V2 and
    // 7 % in distance that is not the lapse. The classical band is on the ratio
    // with that speed step divided out (1.180 raw, 1.098 normalised).
    // The regime is asserted, not assumed: the sea-level V2 / VSR is the 1.13
    // floor and the 2,000 ft one is the improved-climb speed above it (1.172 with
    // the tail at its ratio and the neutral-point downwash; raw ratio
    // 1.180), the A320 at 78 t sitting at the second-segment limit there.
    // vlm-physics (r8): the better engine-out gradient (0.0205 at 2,000 ft)
    // leaves V2 / VSR at 1.148, still above the floor. The floor is the
    // CS-25.107(b)(1) minimum V2 of 1.13 VSR for turbofans, so the regime claim
    // is "above 1.13", not a band around one measured value.
    assert_eq!(sea.v2_over_vsr, 1.13);
    assert!(
        alt.v2_over_vsr > 1.13 && alt.v2_over_vsr < 1.20,
        "{}",
        alt.v2_over_vsr
    );
    let speed_step = (alt.v2_over_vsr / sea.v2_over_vsr).powi(2);
    let aeo = alt.solved.all_engine_m / sea.solved.all_engine_m / speed_step;
    assert!(
        (1.08..1.16).contains(&aeo),
        "all-engine ratio {aeo} over the V2 schedule step {speed_step}"
    );
    // The engine-out air leg is h / mean gradient: its growth equals the
    // inverse growth of the engine-out gradient at V2 to within the mean of
    // the lift-off and V2 values.
    let air = alt.parts.continued_air_m / sea.parts.continued_air_m;
    let gradient = sea.parts.engine_out_gradient_v2 / alt.parts.engine_out_gradient_v2;
    assert!(
        (air / gradient - 1.0).abs() < 0.12,
        "air leg x{air:.3} against 1/gradient x{gradient:.3}"
    );
    // Both gradients are positive (a field exists).
    assert!(alt.parts.engine_out_gradient_v2 > 0.0);
}

/// The Airbus A320 aircraft-characteristics chart (3-3-1 Page 2, CFM56, ISA,
/// curves 0 and 2 (1000 ft) read from the rendered page, +-15 m): runway
/// length at ISA sea level and at 2,000 ft by take-off mass.
const AIRBUS_A320_FIELD: [(f64, f64, f64); 4] = [
    (66_000.0, 1_356.0, 1_508.0),
    (70_000.0, 1_571.0, 1_730.0),
    (74_000.0, 1_807.0, 1_966.0),
    (78_000.0, 2_081.0, 2_285.0),
];

/// The model's ISA sea level to ISA 2,000 ft field step at the four chart
/// masses against the Airbus chart, with the improved-climb resolution at each
/// aerodrome. Run with
/// `cargo test -p alas-report --test a320_field_gap_probe a320_field_step -- --ignored --nocapture`.
#[test]
#[ignore = "probe: prints, asserts nothing"]
fn a320_field_step_table() {
    let (config, report) = a320_report();
    let (sea_level, ft2000) = (
        isa_aerodrome("ISA sea level", 0.0),
        isa_aerodrome("ISA 2000 ft", 609.6),
    );
    for (mass_kg, airbus_sl, airbus_alt) in AIRBUS_A320_FIELD {
        let sl = field_case(&config, &report, &sea_level, mass_kg, (1.0, 1.0, 1.0, 1.0));
        let alt = field_case(&config, &report, &ft2000, mass_kg, (1.0, 1.0, 1.0, 1.0));
        eprintln!(
            "STEP {mass_kg:.0} kg: SL {:.0} m (Airbus {airbus_sl:.0}, {:+.1} %), 2000 ft {:.0} m (Airbus {airbus_alt:.0}, {:+.1} %); step {:+.1} % (Airbus {:+.1} %); OEI gradient {:.4} -> {:.4}; 1.15xAEO {:.0} -> {:.0}; BFL {:.0} -> {:.0}; [{}] [{}]",
            sl.solved.tofl_m,
            100.0 * (sl.solved.tofl_m / airbus_sl - 1.0),
            alt.solved.tofl_m,
            100.0 * (alt.solved.tofl_m / airbus_alt - 1.0),
            100.0 * (alt.solved.tofl_m / sl.solved.tofl_m - 1.0),
            100.0 * (airbus_alt / airbus_sl - 1.0),
            sl.parts.engine_out_gradient_v2,
            alt.parts.engine_out_gradient_v2,
            1.15 * sl.solved.all_engine_m,
            1.15 * alt.solved.all_engine_m,
            sl.solved.balanced_m,
            alt.solved.balanced_m,
            sl.improved,
            alt.improved
        );
    }
}

/// Second family (E7 independent check): the E195-E2 airport-planning manual
/// (Embraer E-Jets E2 APM 5824, Rev 19, Section 03, Fig. 3.11, p. 14 of 34,
/// PW1921G, T/O-1, ISA, flap 4 envelope) runway length at ISA 0 ft and 2,000 ft
/// by take-off weight, curves read from the rendered page by pixel
/// extraction (+-15 m).
const EMBRAER_E195E2_FIELD: [(f64, f64, f64); 3] = [
    (50_000.0, 1_179.0, 1_290.0),
    (55_000.0, 1_412.0, 1_551.0),
    (60_000.0, 1_679.0, 1_847.0),
];

/// The model's field step on the E195-E2 preset against that chart. Run with
/// `cargo test -p alas-report --test a320_field_gap_probe e195e2_field_step -- --ignored --nocapture`.
#[test]
#[ignore = "probe: prints, asserts nothing"]
fn e195e2_field_step_table() {
    let (config, report) = preset_report("E195-E2");
    let (sea_level, ft2000) = (
        isa_aerodrome("ISA sea level", 0.0),
        isa_aerodrome("ISA 2000 ft", 609.6),
    );
    for (mass_kg, chart_sl, chart_alt) in EMBRAER_E195E2_FIELD {
        let sl = field_case(&config, &report, &sea_level, mass_kg, (1.0, 1.0, 1.0, 1.0));
        let alt = field_case(&config, &report, &ft2000, mass_kg, (1.0, 1.0, 1.0, 1.0));
        eprintln!(
            "E2STEP {mass_kg:.0} kg: SL {:.0} m (chart {chart_sl:.0}, {:+.1} %), 2000 ft {:.0} m (chart {chart_alt:.0}, {:+.1} %); step {:+.1} % (chart {:+.1} %); OEI gradient {:.4} -> {:.4}; 1.15xAEO {:.0} -> {:.0}; [{}] [{}]",
            sl.solved.tofl_m,
            100.0 * (sl.solved.tofl_m / chart_sl - 1.0),
            alt.solved.tofl_m,
            100.0 * (alt.solved.tofl_m / chart_alt - 1.0),
            100.0 * (alt.solved.tofl_m / sl.solved.tofl_m - 1.0),
            100.0 * (chart_alt / chart_sl - 1.0),
            sl.parts.engine_out_gradient_v2,
            alt.parts.engine_out_gradient_v2,
            1.15 * sl.solved.all_engine_m,
            1.15 * alt.solved.all_engine_m,
            sl.improved,
            alt.improved
        );
    }
}
