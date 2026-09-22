// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-perf::performance`'s matching-chart surface against
//! `alas.physics.performance`, via `golden/generators/gen_perf_performance.py`:
//! the density ratio, the four constraint curves, and the assembled chart.
//!
//! Every quantity is closed-form `f64` arithmetic: the atmospheric constants
//! reach through `Atmosphere::new` (the same fitted model `alas-prop::cycle`
//! checks at this tier), everything else is algebra over its result, so the
//! surface is checked at `Tier::Closed`, matching `docs/PORTING.md`. The
//! design point is an `Option` on both sides, so its `None` branch is a real
//! assertion rather than a number.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod perf_support;

use alas_config::airports::Airport;
use alas_perf::performance::{
    build_matching_chart, density_ratio, tw_cruise_constraint, tw_oei_climb_constraint,
    tw_takeoff_constraint, ws_landing_limit,
};
use alas_testkit::{Comparison, Tier};
use perf_support::{compare_arrays, section, AirportInput};
use serde::Deserialize;
use std::collections::BTreeMap;

#[test]
fn density_ratio_matches_python() {
    #[derive(Deserialize)]
    struct Case {
        elevation_m: f64,
        delta_isa_c: f64,
        sigma: f64,
    }
    let mut c = Comparison::new("alas-perf::performance::density_ratio", Tier::Closed);
    for case in section::<Case>("density_ratio") {
        c.scalar(
            &format!("sigma({}, {})", case.elevation_m, case.delta_isa_c),
            density_ratio(case.elevation_m, case.delta_isa_c),
            case.sigma,
        );
    }
    c.finish();
}

#[test]
fn tw_cruise_constraint_matches_python() {
    #[derive(Deserialize)]
    struct Case {
        ws_pa: Vec<f64>,
        cd0: f64,
        k: f64,
        cruise_mach: f64,
        cruise_altitude_m: f64,
        thrust_lapse: f64,
        expected: Vec<f64>,
    }
    let mut c = Comparison::new("alas-perf::performance::tw_cruise_constraint", Tier::Closed);
    for (i, case) in section::<Case>("tw_cruise").iter().enumerate() {
        let got = tw_cruise_constraint(
            &case.ws_pa,
            case.cd0,
            case.k,
            case.cruise_mach,
            case.cruise_altitude_m,
            case.thrust_lapse,
        );
        compare_arrays(&mut c, &format!("case{i}"), &got, &case.expected);
    }
    c.finish();
}

#[test]
fn tw_oei_climb_constraint_matches_python() {
    #[derive(Deserialize)]
    struct Case {
        cd0: f64,
        k: f64,
        n_engines: i64,
        oei_gradient: f64,
        cl_climb: f64,
        delta_cd_to_config: f64,
        expected: f64,
    }
    let mut c = Comparison::new(
        "alas-perf::performance::tw_oei_climb_constraint",
        Tier::Closed,
    );
    for case in section::<Case>("tw_oei_climb") {
        c.scalar(
            &format!("n_engines={}", case.n_engines),
            tw_oei_climb_constraint(
                case.cd0,
                case.k,
                case.n_engines,
                case.oei_gradient,
                case.cl_climb,
                case.delta_cd_to_config,
            ),
            case.expected,
        );
    }
    c.finish();
}

#[test]
fn tw_takeoff_constraint_matches_python() {
    #[derive(Deserialize)]
    struct Case {
        ws_pa: Vec<f64>,
        toda_m: f64,
        sigma: f64,
        cl_max_to: f64,
        expected: Vec<f64>,
    }
    let mut c = Comparison::new(
        "alas-perf::performance::tw_takeoff_constraint",
        Tier::Closed,
    );
    for (i, case) in section::<Case>("tw_takeoff").iter().enumerate() {
        let got = tw_takeoff_constraint(&case.ws_pa, case.toda_m, case.sigma, case.cl_max_to);
        compare_arrays(&mut c, &format!("case{i}"), &got, &case.expected);
    }
    c.finish();
}

#[test]
fn ws_landing_limit_matches_python() {
    #[derive(Deserialize)]
    struct Case {
        lda_m: f64,
        sigma: f64,
        cl_max_land: f64,
        k_factor: f64,
        expected: f64,
    }
    let mut c = Comparison::new("alas-perf::performance::ws_landing_limit", Tier::Closed);
    for (i, case) in section::<Case>("ws_landing_limit").iter().enumerate() {
        c.scalar(
            &format!("case{i}"),
            ws_landing_limit(case.lda_m, case.sigma, case.cl_max_land, case.k_factor),
            case.expected,
        );
    }
    c.finish();
}

#[test]
fn build_matching_chart_matches_python() {
    #[derive(Deserialize)]
    struct Expected {
        ws_pa: Vec<f64>,
        tw_cruise: Vec<f64>,
        tw_oei_climb: f64,
        tw_takeoff: BTreeMap<String, Vec<f64>>,
        ws_land_limits: BTreeMap<String, f64>,
        design_ws_pa: Option<f64>,
        design_tw: Option<f64>,
    }
    #[derive(Deserialize)]
    struct Case {
        name: String,
        cd0: f64,
        k: f64,
        cruise_mach: f64,
        cruise_altitude_m: f64,
        mtow_kg: f64,
        wing_area_m2: f64,
        n_engines: i64,
        airports: Vec<AirportInput>,
        cl_max_to: Option<f64>,
        cl_max_land: Option<f64>,
        thrust_lapse: Option<f64>,
        oei_gradient: Option<f64>,
        k_land: Option<f64>,
        oei_climb_cl: Option<f64>,
        oei_climb_delta_cd: Option<f64>,
        tw_design: Option<f64>,
        n_ws_points: i64,
        ws_min_pa: Option<f64>,
        ws_max_pa: Option<f64>,
        expected: Expected,
    }
    let mut c = Comparison::new("alas-perf::performance::build_matching_chart", Tier::Closed);
    for case in section::<Case>("matching_chart") {
        let airports: Vec<Airport> = case.airports.iter().map(AirportInput::build).collect();
        let got = build_matching_chart(
            case.cd0,
            case.k,
            case.cruise_mach,
            case.cruise_altitude_m,
            case.mtow_kg,
            case.wing_area_m2,
            case.n_engines,
            &airports,
            case.cl_max_to,
            case.cl_max_land,
            case.thrust_lapse,
            case.oei_gradient,
            case.k_land,
            case.oei_climb_cl,
            case.oei_climb_delta_cd,
            case.tw_design,
            case.n_ws_points,
            case.ws_min_pa,
            case.ws_max_pa,
        );
        let name = &case.name;
        compare_arrays(
            &mut c,
            &format!("{name}: ws_pa"),
            &got.ws_pa,
            &case.expected.ws_pa,
        );
        compare_arrays(
            &mut c,
            &format!("{name}: tw_cruise"),
            &got.tw_cruise,
            &case.expected.tw_cruise,
        );
        c.scalar(
            &format!("{name}: tw_oei_climb"),
            got.tw_oei_climb,
            case.expected.tw_oei_climb,
        );
        assert_eq!(
            got.design_ws_pa.is_some(),
            case.expected.design_ws_pa.is_some(),
            "{name}: design_ws_pa presence"
        );
        if let (Some(g), Some(w)) = (got.design_ws_pa, case.expected.design_ws_pa) {
            c.scalar(&format!("{name}: design_ws_pa"), g, w);
        }
        assert_eq!(got.design_tw, case.expected.design_tw, "{name}: design_tw");
        assert_eq!(
            got.tw_takeoff.len(),
            case.expected.tw_takeoff.len(),
            "{name}: aerodrome count"
        );
        for (apt_name, curve) in &got.tw_takeoff {
            let want = case
                .expected
                .tw_takeoff
                .get(apt_name)
                .unwrap_or_else(|| panic!("{name}: no expected take-off curve for {apt_name}"));
            compare_arrays(
                &mut c,
                &format!("{name}: tw_takeoff[{apt_name}]"),
                curve,
                want,
            );
        }
        for (apt_name, limit) in &got.ws_land_limits {
            let want = case
                .expected
                .ws_land_limits
                .get(apt_name)
                .unwrap_or_else(|| panic!("{name}: no expected landing limit for {apt_name}"));
            c.scalar(
                &format!("{name}: ws_land_limits[{apt_name}]"),
                *limit,
                *want,
            );
        }
    }
    c.finish();
}
