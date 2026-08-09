// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-perf::performance`'s speed/envelope surface against
//! `alas.physics.performance`, via `golden/generators/gen_perf_performance.py`:
//! the FAR-25 V-speed schedule, the field distances, the Breguet range and the
//! V-n diagram.
//!
//! Continuous quantities are closed-form `f64` arithmetic and are checked at
//! `Tier::Closed`, matching `docs/PORTING.md`; the field feasibility verdicts
//! are booleans and are asserted directly.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod perf_support;

use alas_perf::performance::{
    breguet_range_m, build_vn_diagram, compute_field_performance, compute_v_speeds,
};
use alas_testkit::{Comparison, Tier};
use perf_support::{
    compare_arrays, compare_v_speeds, perf_config_for, requirements_for, section, AirportInput,
    VSpeedsExpected,
};
use serde::Deserialize;
use serde_json::{Map, Value};

#[test]
fn compute_v_speeds_matches_python() {
    #[derive(Deserialize)]
    struct Case {
        name: String,
        mtow_kg: f64,
        wing_area_m2: f64,
        airport: AirportInput,
        cl_max_to: f64,
        cl_max_land: f64,
        config: Map<String, Value>,
        expected: VSpeedsExpected,
    }
    let mut c = Comparison::new("alas-perf::performance::compute_v_speeds", Tier::Closed);
    for case in section::<Case>("v_speeds") {
        let pc = perf_config_for(&case.config);
        let got = compute_v_speeds(
            case.mtow_kg,
            case.wing_area_m2,
            &case.airport.build(),
            case.cl_max_to,
            case.cl_max_land,
            &pc,
        );
        compare_v_speeds(&mut c, &case.name, &got, &case.expected);
    }
    c.finish();
}

#[test]
fn compute_field_performance_matches_python() {
    #[derive(Deserialize)]
    struct Expected {
        todr_m: f64,
        bfl_m: f64,
        asd_m: f64,
        ldr_m: f64,
        to_margin_m: f64,
        land_margin_m: f64,
        to_feasible: bool,
        land_feasible: bool,
        v_speeds: VSpeedsExpected,
    }
    #[derive(Deserialize)]
    struct Case {
        name: String,
        mtow_kg: f64,
        wing_area_m2: f64,
        airport: AirportInput,
        cl_max_to: f64,
        cl_max_land: f64,
        tw_sl: f64,
        k_land: f64,
        bfl_factor: f64,
        config: Map<String, Value>,
        expected: Expected,
    }
    let mut c = Comparison::new(
        "alas-perf::performance::compute_field_performance",
        Tier::Closed,
    );
    for case in section::<Case>("field_performance") {
        let pc = perf_config_for(&case.config);
        let got = compute_field_performance(
            case.mtow_kg,
            case.wing_area_m2,
            &case.airport.build(),
            case.cl_max_to,
            case.cl_max_land,
            case.tw_sl,
            case.k_land,
            case.bfl_factor,
            &pc,
        );
        let name = &case.name;
        c.scalar(&format!("{name}: todr_m"), got.todr_m, case.expected.todr_m);
        c.scalar(&format!("{name}: bfl_m"), got.bfl_m, case.expected.bfl_m);
        c.scalar(&format!("{name}: asd_m"), got.asd_m, case.expected.asd_m);
        c.scalar(&format!("{name}: ldr_m"), got.ldr_m, case.expected.ldr_m);
        c.scalar(
            &format!("{name}: to_margin_m"),
            got.to_margin_m(),
            case.expected.to_margin_m,
        );
        c.scalar(
            &format!("{name}: land_margin_m"),
            got.land_margin_m(),
            case.expected.land_margin_m,
        );
        assert_eq!(
            got.to_feasible(),
            case.expected.to_feasible,
            "{name}: to_feasible"
        );
        assert_eq!(
            got.land_feasible(),
            case.expected.land_feasible,
            "{name}: land_feasible"
        );
        compare_v_speeds(
            &mut c,
            &format!("{name}: v_speeds"),
            &got.v_speeds,
            &case.expected.v_speeds,
        );
    }
    c.finish();
}

#[test]
fn breguet_range_m_matches_python() {
    #[derive(Deserialize)]
    struct Case {
        tas_m_s: f64,
        l_over_d: f64,
        tsfc_si: f64,
        w_start_kg: f64,
        w_end_kg: f64,
        expected: f64,
    }
    let mut c = Comparison::new("alas-perf::performance::breguet_range_m", Tier::Closed);
    for (i, case) in section::<Case>("breguet_range").iter().enumerate() {
        c.scalar(
            &format!("case{i}"),
            breguet_range_m(
                case.tas_m_s,
                case.l_over_d,
                case.tsfc_si,
                case.w_start_kg,
                case.w_end_kg,
            ),
            case.expected,
        );
    }
    c.finish();
}

#[test]
fn build_vn_diagram_matches_python() {
    #[derive(Deserialize)]
    struct Expected {
        n_lim_pos: f64,
        n_lim_neg: f64,
        n_ult_pos: f64,
        n_ult_neg: f64,
        v_s_kt: f64,
        v_a_kt: f64,
        v_c_kt: f64,
        v_d_kt: f64,
        v_cruise_op_kt: f64,
        v_kt: Vec<f64>,
        n_stall_pos: Vec<f64>,
        n_stall_neg: Vec<f64>,
    }
    #[derive(Deserialize)]
    struct Case {
        name: String,
        s_ref: f64,
        req: Map<String, Value>,
        perf: Map<String, Value>,
        cruise_alt_m: f64,
        expected: Expected,
    }
    let mut c = Comparison::new("alas-perf::performance::build_vn_diagram", Tier::Closed);
    for case in section::<Case>("vn_diagram") {
        let req = requirements_for(&case.req);
        let pc = perf_config_for(&case.perf);
        let got = build_vn_diagram(case.s_ref, &req, &pc, case.cruise_alt_m);
        let name = &case.name;
        c.scalar(
            &format!("{name}: n_lim_pos"),
            got.n_lim_pos,
            case.expected.n_lim_pos,
        );
        c.scalar(
            &format!("{name}: n_lim_neg"),
            got.n_lim_neg,
            case.expected.n_lim_neg,
        );
        c.scalar(
            &format!("{name}: n_ult_pos"),
            got.n_ult_pos,
            case.expected.n_ult_pos,
        );
        c.scalar(
            &format!("{name}: n_ult_neg"),
            got.n_ult_neg,
            case.expected.n_ult_neg,
        );
        c.scalar(&format!("{name}: v_s_kt"), got.v_s_kt, case.expected.v_s_kt);
        c.scalar(&format!("{name}: v_a_kt"), got.v_a_kt, case.expected.v_a_kt);
        c.scalar(&format!("{name}: v_c_kt"), got.v_c_kt, case.expected.v_c_kt);
        c.scalar(&format!("{name}: v_d_kt"), got.v_d_kt, case.expected.v_d_kt);
        c.scalar(
            &format!("{name}: v_cruise_op_kt"),
            got.v_cruise_op_kt,
            case.expected.v_cruise_op_kt,
        );
        compare_arrays(
            &mut c,
            &format!("{name}: v_kt"),
            &got.v_kt,
            &case.expected.v_kt,
        );
        compare_arrays(
            &mut c,
            &format!("{name}: n_stall_pos"),
            &got.n_stall_pos,
            &case.expected.n_stall_pos,
        );
        compare_arrays(
            &mut c,
            &format!("{name}: n_stall_neg"),
            &got.n_stall_neg,
            &case.expected.n_stall_neg,
        );
    }
    c.finish();
}
