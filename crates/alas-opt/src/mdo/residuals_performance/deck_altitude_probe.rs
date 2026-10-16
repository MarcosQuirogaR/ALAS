// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Diagnostic tables for the thrust of the empirical turbofan deck at cruise
//! altitude against independent lapse forms and against the registered
//! aircraft's own drag (lane fix-deck-altitude). Ignored tests; they print.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use crate::mdo::propulsion::PropulsionDeck;
use crate::mdo::sizing::run_candidate;
use alas_config::{ActiveEngineModel, AlasConfig};
use alas_prop::system::PropulsionRating;

const G0: f64 = 9.806_65;

/// Mattingly high-bypass maximum-thrust lapse `delta0 (1 - 0.49 sqrt(M))`
/// over the sea-level static thrust (coefficients as recalled, not re-read).
fn mattingly_ratio(altitude_m: f64, mach: f64) -> f64 {
    let a = alas_atmo::us1976_compute_values(altitude_m, 0.0);
    let delta = a.pressure_pa / 101_325.0;
    let delta0 = delta * (1.0 + 0.2 * mach * mach).powf(3.5);
    delta0 * (1.0 - 0.49 * mach.sqrt())
}

fn ft(feet: f64) -> f64 {
    feet * 0.3048
}

fn deck_ratio(deck: &PropulsionDeck, altitude_m: f64, mach: f64, rating: PropulsionRating) -> f64 {
    let a = alas_atmo::us1976_compute_values(altitude_m, 0.0);
    let flight = deck
        .flight_condition(altitude_m, mach * a.speed_of_sound_m_s, G0, 0.0)
        .unwrap();
    let static_flight = deck.flight_condition(0.0, 0.0, G0, 0.0).unwrap();
    let at = deck.rated_point(flight, rating).unwrap().thrust_n;
    let sls = deck
        .rated_point(static_flight, PropulsionRating::TakeoffGoAround)
        .unwrap()
        .thrust_n;
    at / sls
}

/// `cargo test -p alas-opt --release deck_altitude_table -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic table of all registered jets; slow"]
fn deck_altitude_table() {
    for registered in alas_config::presets::registry() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": registered.name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let engine = &config.geometry.engine;
        let Ok(ActiveEngineModel::Turbofan(p)) = engine.active_model() else {
            continue;
        };
        let req = &config.requirements;
        let deck = PropulsionDeck::from_engine(
            engine,
            req.cruise_mach,
            req.cruise_altitude_m,
            crate::mdo::propulsion::max_climb_rate_ft_min(
                config.mission.profile.initial_climb_rate_m_s,
            ),
        )
        .unwrap();
        let ref_mcl = deck_ratio(
            &deck,
            p.off_design.cruise_reference_altitude_m,
            p.off_design.cruise_reference_mach,
            PropulsionRating::MaximumClimb,
        );
        eprintln!(
            "ANCHOR {:10} MCL at ref {ref_mcl:.4} catalogue cruise {:.4} util {:.3}",
            registered.name,
            p.off_design.cruise_reference_thrust_n / (p.rated_thrust_kn * 1000.0),
            p.off_design.cruise_reference_thrust_n / (p.rated_thrust_kn * 1000.0) / ref_mcl,
        );
        eprintln!(
            "ENG {:10} bpr {:.1} (takeoff {:?}) ref FL{:.0} M{:.2} F_SLS {:.1} kN cruise-ref {:.1} kN tsfc {:.3} TR {:?}",
            registered.name,
            p.bypass_ratio,
            p.takeoff_bypass_ratio,
            p.off_design.cruise_reference_altitude_m / 0.3048 / 100.0,
            p.off_design.cruise_reference_mach,
            p.rated_thrust_kn,
            p.off_design.cruise_reference_thrust_n / 1000.0,
            p.cruise_tsfc_kg_kgf_hr,
            p.flat_rating_temperature_k,
        );
        for mach in [0.70, 0.78, 0.82, 0.86] {
            let mut line = format!("  M{mach:.2} MCL/crz/Matt:");
            for fl in [250.0, 290.0, 330.0, 350.0, 370.0, 390.0, 410.0] {
                let h = ft(fl * 100.0);
                let mcl = deck_ratio(&deck, h, mach, PropulsionRating::MaximumClimb);
                let crz = deck_ratio(&deck, h, mach, PropulsionRating::Cruise);
                line += &format!(
                    " FL{fl:.0} {mcl:.3}/{crz:.3}/{:.3} |",
                    mattingly_ratio(h, mach)
                );
            }
            eprintln!("{line}");
        }
    }
}

/// Physical requirement at the design cruise point for every jet: thrust
/// required at the take-off weight, at the mid-cruise weight, against the
/// deck's maximum-climb thrust, and the model L/D there.
///
/// `cargo test -p alas-opt --release deck_requirement_table -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic table of all registered jets; slow"]
fn deck_requirement_table() {
    for registered in alas_config::presets::registry() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": registered.name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        if !matches!(
            config.geometry.engine.active_model(),
            Ok(ActiveEngineModel::Turbofan(_))
        ) {
            continue;
        }
        let design = alas_config::presets::get(registered.name)
            .unwrap()
            .design_vector;
        let outcome = run_candidate(&config, &design.to_array()).unwrap();
        let req = &config.requirements;
        let s_ref = outcome.plane.s_ref;
        let drag = &outcome.sized.fuel_artifacts.drag;
        let deck = PropulsionDeck::from_engine(
            &config.geometry.engine,
            req.cruise_mach,
            req.cruise_altitude_m,
            crate::mdo::propulsion::max_climb_rate_ft_min(
                config.mission.profile.initial_climb_rate_m_s,
            ),
        )
        .unwrap();
        let tow = outcome.sized.takeoff_mass_kg;
        let mid = outcome.sized.mid_cruise_mass_kg();
        let installed_tw =
            outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0 / (tow * G0);
        let r_mcl = deck_ratio(
            &deck,
            req.cruise_altitude_m,
            req.cruise_mach,
            PropulsionRating::Cruise,
        );
        let atmosphere = alas_atmo::Atmosphere::new(req.cruise_altitude_m);
        let q =
            0.5 * atmosphere.density() * (req.cruise_mach * atmosphere.speed_of_sound()).powi(2);
        let point = |mass: f64| {
            let cl = mass * G0 / (s_ref * q);
            let cd = drag.cd(cl, req.cruise_mach, req.cruise_altitude_m);
            (cl, cl / cd, cd / cl / r_mcl)
        };
        let (cl_t, ld_t, _) = point(tow);
        let (cl_m, ld_m, _) = point(mid);
        // Level-flight requirement on the take-off-weight axis, at maximum
        // cruise, from the production function.
        let engine_out_model = super::oei_deck::EngineOutModel::new(
            &config,
            None,
            s_ref,
            Some(outcome.static_thrust_kn * 1_000.0),
        )
        .unwrap();
        let level_at = |mass: f64| {
            super::cruise_deck::deck_cruise_requirements(
                &engine_out_model,
                drag,
                req,
                tow,
                mass,
                s_ref,
            )
            .map_or(f64::NAN, |d| d.level_tw)
        };
        let (req_t, req_m) = (level_at(tow), level_at(mid));
        // Maximum L/D of the model polar at this Mach and altitude.
        let mut ld_max = 0.0_f64;
        let mut cl = 0.2;
        while cl < 1.0 {
            let cd = drag.cd(cl, req.cruise_mach, req.cruise_altitude_m);
            ld_max = ld_max.max(cl / cd);
            cl += 0.005;
        }
        // Initial cruise level of the model: the highest flight level at
        // which the maximum-climb thrust gives 300 ft/min at 0.985 of the
        // take-off mass at the design Mach (the criterion of the mission's
        // initial-level search), and the highest level that holds level
        // flight at the mid-cruise mass.
        let engine_out = super::oei_deck::EngineOutModel::new(
            &config,
            None,
            s_ref,
            Some(outcome.static_thrust_kn * 1_000.0),
        )
        .unwrap();
        let (mut ica_fl, mut level_fl) = (0.0_f64, 0.0_f64);
        for fl in (250..=450).step_by(10) {
            let mut r = req.clone();
            r.cruise_altitude_m = f64::from(fl) * 100.0 * 0.3048;
            let Ok(d) = super::cruise_deck::deck_cruise_requirements(
                &engine_out,
                drag,
                &r,
                tow,
                mid,
                s_ref,
            ) else {
                continue;
            };
            if d.top_of_climb_tw <= installed_tw {
                ica_fl = f64::from(fl);
            }
            if d.level_tw <= installed_tw {
                level_fl = f64::from(fl);
            }
        }
        // The mission model's own initial cruise level at the take-off mass
        // over the design range (the 300 ft/min rule at the flown speed).
        if let Ok(model) =
            crate::mdo::candidate_mission_model(&config, &outcome.sized.fuel_artifacts)
        {
            let coordinates = |id: &str| {
                alas_config::airport_dataset::resolve(id)
                    .ok()
                    .and_then(|a| Some((a.latitude_deg.value?, a.longitude_deg.value?)))
            };
            let range_m = crate::mdo::range::mission_range_from_coordinates(
                config.optimizer.objective.design_range_nmi,
                coordinates(&config.departure_airport),
                coordinates(&config.arrival_airport),
            );
            match model.fly_trip(tow, range_m) {
                Ok(flown) => eprintln!(
                    "MISSION {:10} design FL{:.0}: initial level FL{:.0} adapted {} trip fuel {:.0} kg end mass {:.0} kg",
                    registered.name,
                    req.cruise_altitude_m / 0.3048 / 100.0,
                    flown.cruise_altitude_m / 0.3048 / 100.0,
                    flown.adapted,
                    flown.leg.fuel_kg,
                    flown.end_mass_kg,
                ),
                Err(e) => eprintln!("MISSION {:10} fly_trip: {e}", registered.name),
            }
        }
        eprintln!(
            "ICA {:10} design FL{:.0}: 300 fpm at TOC mass up to FL{ica_fl:.0}; level at mid-cruise up to FL{level_fl:.0}",
            registered.name,
            req.cruise_altitude_m / 0.3048 / 100.0,
        );
        eprintln!(
            "REQ {:10} FL{:.0} M{:.2} mid/TOW {:.3} r_MCL {:.3} inst {:.4} | TOW CL {:.3} L/D {:.1} req {:.4} | mid CL {:.3} L/D {:.1} req {:.4} | (L/D)max {:.1} wave(mid) {:.4}",
            registered.name,
            req.cruise_altitude_m / 0.3048 / 100.0,
            req.cruise_mach,
            mid / tow,
            r_mcl,
            installed_tw,
            cl_t,
            ld_t,
            req_t,
            cl_m,
            ld_m,
            req_m,
            ld_max,
            drag.wave_cd(cl_m, req.cruise_mach),
        );
    }
}

/// Low-altitude maximum-climb ratio of the A340-300 deck (BPR 6.6) at the
/// heights and Mach numbers of Jenkinson's charts.
#[test]
#[ignore = "diagnostic table; prints"]
fn deck_low_altitude_table() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "A340-300"})).unwrap();
    let req = &config.requirements;
    let deck = PropulsionDeck::from_engine(
        &config.geometry.engine,
        req.cruise_mach,
        req.cruise_altitude_m,
        crate::mdo::propulsion::max_climb_rate_ft_min(
            config.mission.profile.initial_climb_rate_m_s,
        ),
    )
    .unwrap();
    for feet in [0.0, 10_000.0, 15_000.0, 25_000.0, 30_000.0] {
        let mut line = format!("LOW {feet:6.0} ft:");
        for mach in [0.4, 0.6, 0.8] {
            let r = deck_ratio(&deck, ft(feet), mach, PropulsionRating::MaximumClimb);
            line += &format!(" M{mach:.1} {r:.3} |");
        }
        eprintln!("{line}");
    }
}

/// Residual-climb profile of the registered jets against altitude, with the
/// thrust and drag behind it (fix lane a220-atr).
///
/// `cargo test -p alas-opt --release toc_profile_table -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic table; prints"]
fn toc_profile_table() {
    let names =
        std::env::var("ALAS_PROBE_PRESETS").unwrap_or_else(|_| "A220-300,A320-200".to_owned());
    for name in names.split(',') {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let outcome = run_candidate(&config, &design.to_array()).unwrap();
        let req = &config.requirements;
        let s_ref = outcome.plane.s_ref;
        let drag = &outcome.sized.fuel_artifacts.drag;
        let tow = outcome.sized.takeoff_mass_kg;
        let model =
            crate::mdo::candidate_mission_model(&config, &outcome.sized.fuel_artifacts).unwrap();
        let plan_h = outcome
            .sized
            .fuel_artifacts
            .frozen_plan
            .as_ref()
            .map(|p| p.cruise_altitude_m);
        eprintln!(
            "TOCP {name} tow {tow:.0} S {s_ref:.2} design FL{:.0} M{:.2} plan level {:?} m installed {:.2} kN x{}",
            req.cruise_altitude_m / 0.3048 / 100.0,
            req.cruise_mach,
            plan_h,
            outcome.static_thrust_kn,
            outcome.n_engines,
        );
        let deck = PropulsionDeck::from_engine(
            &config.geometry.engine,
            req.cruise_mach,
            req.cruise_altitude_m,
            crate::mdo::propulsion::max_climb_rate_ft_min(
                config.mission.profile.initial_climb_rate_m_s,
            ),
        )
        .unwrap();
        for fl in (150..=410).step_by(20) {
            let h = ft(f64::from(fl) * 100.0);
            let tas = model.cruise_true_airspeed_m_s(h).unwrap();
            let a = alas_atmo::us1976_compute_values(h, 0.0);
            let mach = tas / a.speed_of_sound_m_s;
            let mass = 0.985 * tow;
            let q = 0.5 * a.density_kg_m3 * tas * tas;
            let cl = mass * G0 / (s_ref * q);
            let cd = drag.cd(cl, mach, h);
            let drag_n = cd * q * s_ref;
            let r = deck_ratio(&deck, h, mach, PropulsionRating::MaximumClimb);
            let thrust_n = r * outcome.n_engines as f64 * outcome.static_thrust_kn * 1000.0;
            let frozen = outcome.sized.fuel_artifacts.frozen_plan.as_ref().unwrap();
            let rate = model
                .top_of_climb_rate_m_s(tow, h, frozen.range_m, frozen.climb_rate_scales)
                .unwrap_or(f64::NAN);
            eprintln!(
                "TOCP {name} FL{fl} M{mach:.3} TAS {tas:.1} CL {cl:.3} L/D {:.2} D {:.1} kN T {:.1} kN r_MCL {r:.3} ROC at the plan speeds {rate:.2} m/s (Mach-constant hand {:.2})",
                cl / cd,
                drag_n / 1000.0,
                thrust_n / 1000.0,
                (thrust_n - drag_n) * tas / (mass * G0),
            );
        }
    }
}

/// Engine-scale sweep of one registered jet: the catalogue cruise point is
/// declared an engine datum at `k x 0.93 x F_chart_MCL(ref)` (a what-if, not
/// data), and the hard deck rows are printed (fix lane a220-atr).
///
/// `ALAS_PROBE_PRESETS=A220-300 cargo test -p alas-opt --release k_sweep -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic sweep; prints"]
fn k_sweep() {
    let name = std::env::var("ALAS_PROBE_PRESETS").unwrap_or_else(|_| "A220-300".to_owned());
    let ks: Vec<f64> = std::env::var("ALAS_PROBE_K")
        .unwrap_or_else(|_| "1.0,1.03,1.05,1.07,1.10,1.15,1.17,1.20".to_owned())
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    let base = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
    let design = alas_config::presets::get(&name).unwrap().design_vector;
    let spec = base.geometry.engine.turbofan.clone().unwrap();
    let req = &base.requirements;
    let deck = PropulsionDeck::from_engine(
        &base.geometry.engine,
        req.cruise_mach,
        req.cruise_altitude_m,
        crate::mdo::propulsion::max_climb_rate_ft_min(base.mission.profile.initial_climb_rate_m_s),
    )
    .unwrap();
    let r_ref = deck_ratio(
        &deck,
        spec.off_design.cruise_reference_altitude_m,
        spec.off_design.cruise_reference_mach,
        PropulsionRating::MaximumClimb,
    );
    let chart_mcl_n = r_ref * spec.rated_thrust_kn * 1000.0;
    eprintln!(
        "KSW {name}: chart MCL at ref {r_ref:.4} x {:.2} kN = {:.1} N; MCR = {:.1} N; catalogue cruise {:.1} N (u_MCR {:.3})",
        spec.rated_thrust_kn,
        chart_mcl_n,
        0.93 * chart_mcl_n,
        spec.off_design.cruise_reference_thrust_n,
        spec.off_design.cruise_reference_thrust_n / (0.93 * chart_mcl_n),
    );
    for k in ks {
        let mut config = base.clone();
        let turbofan = config.geometry.engine.turbofan.as_mut().unwrap();
        turbofan.off_design.evidence = "direct-openap".to_owned();
        turbofan.off_design.source = "what-if probe, not data".to_owned();
        turbofan.off_design.cruise_reference_thrust_n = k * 0.93 * chart_mcl_n;
        match crate::assess_product_candidate(&config, &design) {
            Ok(a) => {
                let level = a
                    .sized
                    .fuel_artifacts
                    .frozen_plan
                    .as_ref()
                    .map(|p| p.cruise_altitude_m / 0.3048 / 100.0);
                let rows: Vec<String> = a
                    .residuals
                    .iter()
                    .filter(|r| matches!(r.id, "cruise_thrust" | "toc_rate_of_climb"))
                    .map(|r| {
                        format!(
                            "{} {:.4}/{:.4}{}",
                            r.id,
                            r.actual,
                            r.limit,
                            if r.violated() { " VIOL" } else { "" }
                        )
                    })
                    .collect();
                eprintln!(
                    "KSW {name} k {k:.3} initial level FL{:.0} block fuel {:.0} kg | {} | hard {:?}",
                    level.unwrap_or(f64::NAN),
                    a.sized.block_fuel_kg,
                    rows.join(" | "),
                    a.violated_hard_ids(),
                );
            }
            Err(e) => eprintln!("KSW {name} k {k:.3}: {e}"),
        }
    }
}

/// Drag build-up of the registered jets at the design cruise point and at the
/// top-of-climb point (fix lane a220-atr).
///
/// `cargo test -p alas-opt --release drag_split_table -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic table; prints"]
fn drag_split_table() {
    let names =
        std::env::var("ALAS_PROBE_PRESETS").unwrap_or_else(|_| "A220-300,A320-200".to_owned());
    for name in names.split(',') {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let outcome = run_candidate(&config, &design.to_array()).unwrap();
        let req = &config.requirements;
        let s_ref = outcome.plane.s_ref;
        let b_ref = outcome.plane.b_ref;
        let drawn = outcome.plane.wings[0].projected_area();
        eprintln!(
            "SPLIT {name} S_ref {s_ref:.2} drawn main-wing projected area {drawn:.2} ({:+.1} %) b {b_ref:.2} AR_ref {:.2} OEW {:.0} kg (reference {:?})",
            100.0 * (drawn / s_ref - 1.0),
            b_ref * b_ref / s_ref,
            outcome.sized.operating_empty_mass_kg,
            alas_config::oew_reference::preset_reference_oew_kg(name),
        );
        let drag = &outcome.sized.fuel_artifacts.drag;
        let table = drag.table().unwrap();
        let tow = outcome.sized.takeoff_mass_kg;
        let mid = outcome.sized.mid_cruise_mass_kg();
        for (label, mass, fl, mach) in [
            (
                "design mid-cruise",
                mid,
                req.cruise_altitude_m / 0.3048 / 100.0,
                req.cruise_mach,
            ),
            (
                "TOC 0.985 TOW at design",
                0.985 * tow,
                req.cruise_altitude_m / 0.3048 / 100.0,
                req.cruise_mach,
            ),
            ("TOC 0.985 TOW FL250", 0.985 * tow, 250.0, req.cruise_mach),
        ] {
            let h = ft(fl * 100.0);
            let a = alas_atmo::us1976_compute_values(h, 0.0);
            let q = 0.5 * a.density_kg_m3 * (mach * a.speed_of_sound_m_s).powi(2);
            let cl = mass * G0 / (s_ref * q);
            let cd = drag.cd(cl, mach, h);
            let cdi = table.induced_cd(cl);
            let cdw = table.wave_cd(cl, mach);
            eprintln!(
                "SPLIT {name} {label} FL{fl:.0} M{mach:.2} m {mass:.0}: CL {cl:.3} CD {:.1} counts = induced(trim incl) {:.1} + wave {:.1} + parasite&viscous {:.1}; L/D {:.2}; e_eff {:.3}",
                cd * 1e4, cdi * 1e4, cdw * 1e4, (cd - cdi - cdw) * 1e4, cl / cd, table.effective_oswald(cl),
            );
        }
        let mut best = (0.0, 0.0);
        let mut cl = 0.2;
        while cl < 1.0 {
            let ld = cl / drag.cd(cl, req.cruise_mach, req.cruise_altitude_m);
            if ld > best.1 {
                best = (cl, ld);
            }
            cl += 0.005;
        }
        eprintln!(
            "SPLIT {name} (L/D)max {:.2} at CL {:.3} (design altitude, M{:.2})",
            best.1, best.0, req.cruise_mach
        );
    }
}
