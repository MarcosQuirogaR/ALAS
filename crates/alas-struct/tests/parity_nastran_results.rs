// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-struct::nastran`'s result readers and run report against
//! `alas/integration/nastran_runner.py`, via
//! `golden/generators/gen_struct_nastran_results.py`.
//!
//! The decks this module writes are compared in `parity_nastran.rs`. This is
//! the other direction -- what it reads back -- and it is checked on OP2 files
//! pyNastran wrote, because reading back otherwise needs a solve and there is
//! no NASTRAN install here to produce one. Each scenario in the fixture exists
//! to reach a branch that a complete, healthy result would never reach: a
//! subcase the solve skipped, a stress table that is not there, a monitor grid
//! missing from the result, a modal solve whose every mode is rigid-body.
//!
//! Two tiers, for the reason the payload and routing rows carry two. What is
//! copied out of the file is `exact`: the statuses, which labels a result
//! carries and in what order, how many modes survived the filter, and the
//! frequencies, tip deflections and span stations, none of which any arithmetic
//! touches on the way through. What is computed is `closed`: a normalized mode
//! shape is a division, a response magnitude is a `hypot`, and the two RMS
//! figures are a square root over a trapezoidal integral. A mode shape asserted
//! bitwise would be claiming something the division does not support, and a
//! frequency compared loosely would let a reader that dropped a mode pass.
//!
//! The run report is `exact` throughout, and deliberately so: it is text, and
//! the thing most likely to go wrong in it -- that a NASTRAN print file is
//! paginated with form feeds, which Python breaks lines at and Rust does not --
//! shows up only as the wrong characters, never as a tolerance.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;

use alas_struct::loads::LoadCase;
use alas_struct::mesh::MeshNodeIndex;
use alas_struct::nastran::{
    fatal_lines, monitor_set, read_modes, read_static, read_vibration, tail, LabelledValues,
    SpanStations,
};
use alas_struct::op2::{read_op2, Op2};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    r#static: Vec<StaticCase>,
    modes: Vec<ModesCase>,
    vibration: Vec<VibrationCase>,
    text: TextCases,
}

#[derive(Debug, Deserialize)]
struct NodeIndex {
    root_nid: i64,
    tip_nid: i64,
    kink_nid: i64,
    spar_upper_nids: Vec<Vec<i64>>,
    spar_lower_nids: Vec<Vec<i64>>,
    engine_nids: Vec<i64>,
}

impl NodeIndex {
    fn to_mesh_index(&self) -> MeshNodeIndex {
        MeshNodeIndex {
            root_nid: self.root_nid,
            tip_nid: self.tip_nid,
            kink_nid: self.kink_nid,
            spar_upper_nids: self.spar_upper_nids.clone(),
            spar_lower_nids: self.spar_lower_nids.clone(),
            engine_nids: self.engine_nids.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct StaticCase {
    label: String,
    op2_hex: String,
    case_names: Vec<String>,
    node_index: NodeIndex,
    expected: StaticExpected,
}

#[derive(Debug, Deserialize)]
struct StaticExpected {
    status: String,
    tip_deflection_m: BTreeMap<String, f64>,
    root_von_mises_max_pa: BTreeMap<String, f64>,
}

#[derive(Debug, Deserialize)]
struct ModesCase {
    label: String,
    op2_hex: String,
    node_index: NodeIndex,
    grid_y: BTreeMap<String, f64>,
    expected: ModesExpected,
}

#[derive(Debug, Deserialize)]
struct ModesExpected {
    status: String,
    frequencies_hz: Vec<f64>,
    mode_shape_y_m: Option<Vec<f64>>,
    mode_shapes: Vec<Vec<f64>>,
}

#[derive(Debug, Deserialize)]
struct VibrationCase {
    label: String,
    sine_op2_hex: Option<String>,
    random_op2_hex: Option<String>,
    modal_freqs: Vec<f64>,
    node_index: NodeIndex,
    damping_ratio: f64,
    psd_base_g2_per_hz: f64,
    expected: VibrationExpected,
}

#[derive(Debug, Deserialize)]
struct VibrationExpected {
    status: String,
    frf_freq_hz: Option<Vec<f64>>,
    frf_tip_abs_m_per_n: Option<Vec<f64>>,
    peak_frf: f64,
    peak_freq_hz: f64,
    miles_rms_m: BTreeMap<String, f64>,
    nastran_rms_m: BTreeMap<String, f64>,
}

#[derive(Debug, Deserialize)]
struct TextCases {
    tail: Vec<TailCase>,
    fatal_scan: Vec<FatalCase>,
}

#[derive(Debug, Deserialize)]
struct TailCase {
    label: String,
    text: String,
    n_lines: usize,
    expected: String,
}

#[derive(Debug, Deserialize)]
struct FatalCase {
    label: String,
    text: String,
    expected_lines: Vec<String>,
    expected_reported: Vec<String>,
}

/// The fixture's grid table, standing in for the mesh the reference reads span
/// stations off.
struct GridTable(BTreeMap<i64, f64>);

impl SpanStations for GridTable {
    fn span_station(&self, nid: i64) -> f64 {
        self.0.get(&nid).copied().unwrap_or(f64::NAN)
    }
}

/// Decode the hex the fixture stores the raw `.op2` bytes as.
fn decode_hex(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    assert!(bytes.len() % 2 == 0, "hex payload has an odd length");
    bytes
        .chunks_exact(2)
        .map(|pair| {
            let hi = (pair[0] as char).to_digit(16).unwrap();
            let lo = (pair[1] as char).to_digit(16).unwrap();
            (hi * 16 + lo) as u8
        })
        .collect()
}

fn read(hex: &str) -> Op2 {
    read_op2(&decode_hex(hex)).expect("the fixture's .op2 bytes should parse")
}

/// Which labels a result carries, compared as a set.
///
/// A reader that silently dropped a monitor or invented one is not a tolerance
/// question, so this is `exact` wherever it is called from. The fixture is
/// written with sorted keys, so the *set* is what is comparable here; the order
/// the reader records them in is a separate claim, and `results.rs`'s own unit
/// tests make it.
fn compare_labels(
    cmp: &mut Comparison,
    label: &str,
    actual: &LabelledValues,
    expected: &BTreeMap<String, f64>,
) {
    let mut names = actual.labels();
    names.sort_unstable();
    let wanted: Vec<&str> = expected.keys().map(String::as_str).collect();
    cmp.exact(&format!("{label} labels"), &names, &wanted);
}

/// Compare a label-keyed result whose values are copied rather than computed.
fn compare_labelled_exact(
    cmp: &mut Comparison,
    label: &str,
    actual: &LabelledValues,
    expected: &BTreeMap<String, f64>,
) {
    compare_labels(cmp, label, actual, expected);
    for (name, want) in expected {
        cmp.exact(&format!("{label}[{name}]"), &actual.get(name), &Some(*want));
    }
}

/// Compare a label-keyed result whose values are arithmetic.
fn compare_labelled(
    exact: &mut Comparison,
    numeric: &mut Comparison,
    label: &str,
    actual: &LabelledValues,
    expected: &BTreeMap<String, f64>,
) {
    compare_labels(exact, label, actual, expected);
    for (name, want) in expected {
        let Some(got) = actual.get(name) else {
            exact.exact(&format!("{label}[{name}] present"), &false, &true);
            continue;
        };
        numeric.scalar(&format!("{label}[{name}]"), got, *want);
    }
}

#[test]
fn static_results_match_the_reference_across_partial_and_complete_solves() {
    let fixture: Fixture = alas_testkit::load("struct", "nastran_results");
    let mut exact = Comparison::new("alas-struct::nastran::read_static", Tier::Exact);

    for case in &fixture.r#static {
        let label = &case.label;
        let op2 = read(&case.op2_hex);
        let index = case.node_index.to_mesh_index();
        let cases: Vec<LoadCase> = case
            .case_names
            .iter()
            .map(|name| LoadCase {
                // The reader uses only the name; the two numbers are carried so the
                // call reads as the reference's does.
                name: load_case_name(name),
                load_factor: 1.0,
                total_force_n: 1.0,
            })
            .collect();
        let result = read_static(&op2, &index, &cases);

        exact.exact(
            &format!("[{label}] status"),
            &result.status.as_str().to_owned(),
            &case.expected.status,
        );
        // Both of these are copied out of the file -- one component of a
        // displacement row, and the largest absolute value of a stress column.
        // Neither passes through arithmetic, so both are reachable at `exact`.
        compare_labelled_exact(
            &mut exact,
            &format!("[{label}] tip deflection"),
            &result.tip_deflection_m,
            &case.expected.tip_deflection_m,
        );
        compare_labelled_exact(
            &mut exact,
            &format!("[{label}] root von Mises"),
            &result.root_von_mises_max_pa,
            &case.expected.root_von_mises_max_pa,
        );
    }
    exact.finish();
}

#[test]
fn modal_results_match_the_reference_including_the_rigid_body_filter() {
    let fixture: Fixture = alas_testkit::load("struct", "nastran_results");
    let mut exact = Comparison::new("alas-struct::nastran::read_modes", Tier::Exact);
    let mut closed = Comparison::new("alas-struct::nastran::read_modes shapes", Tier::Closed);

    for case in &fixture.modes {
        let label = &case.label;
        let op2 = read(&case.op2_hex);
        let index = case.node_index.to_mesh_index();
        let stations = GridTable(
            case.grid_y
                .iter()
                .map(|(nid, y)| (nid.parse().expect("a grid id"), *y))
                .collect(),
        );
        let result = read_modes(&op2, &stations, &index);

        exact.exact(
            &format!("[{label}] status"),
            &result.status.as_str().to_owned(),
            &case.expected.status,
        );
        // A frequency is a value the solver wrote and this reader selected. If
        // it moves at all, a mode was dropped or kept wrongly.
        exact.exact(
            &format!("[{label}] frequencies"),
            &result.frequencies_hz,
            &case.expected.frequencies_hz,
        );
        // Whether a shape was built at all is the branch, and it is discrete.
        exact.exact(
            &format!("[{label}] span stations present"),
            &result.mode_shape_y_m.is_some(),
            &case.expected.mode_shape_y_m.is_some(),
        );
        if let (Some(got), Some(want)) = (&result.mode_shape_y_m, &case.expected.mode_shape_y_m) {
            exact.exact(&format!("[{label}] span stations"), got, want);
        }
        exact.exact(
            &format!("[{label}] shape count"),
            &result.mode_shapes.len(),
            &case.expected.mode_shapes.len(),
        );
        for (mode, (got, want)) in result
            .mode_shapes
            .iter()
            .zip(&case.expected.mode_shapes)
            .enumerate()
        {
            closed.slice(&format!("[{label}] shape[mode {mode}]"), got, want);
        }
    }
    exact.finish();
    closed.finish();
}

#[test]
fn vibration_results_match_the_reference_with_either_solve_absent() {
    let fixture: Fixture = alas_testkit::load("struct", "nastran_results");
    let mut exact = Comparison::new("alas-struct::nastran::read_vibration", Tier::Exact);
    let mut closed = Comparison::new("alas-struct::nastran::read_vibration values", Tier::Closed);

    for case in &fixture.vibration {
        let label = &case.label;
        let sine = case.sine_op2_hex.as_deref().map(read);
        let random = case.random_op2_hex.as_deref().map(read);
        let index = case.node_index.to_mesh_index();
        let result = read_vibration(
            sine.as_ref(),
            random.as_ref(),
            &case.modal_freqs,
            monitor_set(&index),
            case.damping_ratio,
            case.psd_base_g2_per_hz,
        );

        exact.exact(
            &format!("[{label}] status"),
            &result.status.as_str().to_owned(),
            &case.expected.status,
        );
        // Whether there is an FRF at all is the branch a missing tip grid takes.
        exact.exact(
            &format!("[{label}] frf present"),
            &result.frf_freq_hz.is_some(),
            &case.expected.frf_freq_hz.is_some(),
        );
        if let (Some(got), Some(want)) = (&result.frf_freq_hz, &case.expected.frf_freq_hz) {
            // The swept frequencies are copied from the file, not computed.
            exact.exact(&format!("[{label}] frf frequencies"), got, want);
        }
        if let (Some(got), Some(want)) = (
            &result.frf_tip_abs_m_per_n,
            &case.expected.frf_tip_abs_m_per_n,
        ) {
            closed.slice(&format!("[{label}] frf magnitude"), got, want);
        }
        closed.scalar(
            &format!("[{label}] peak frf"),
            result.peak_frf,
            case.expected.peak_frf,
        );
        // Which frequency the peak landed on is a selection, not a value: a
        // reader that picked the wrong sample must not pass on closeness.
        exact.exact(
            &format!("[{label}] peak frequency"),
            &result.peak_freq_hz,
            &case.expected.peak_freq_hz,
        );
        compare_labelled(
            &mut exact,
            &mut closed,
            &format!("[{label}] Miles RMS"),
            &result.miles_rms_m,
            &case.expected.miles_rms_m,
        );
        compare_labelled(
            &mut exact,
            &mut closed,
            &format!("[{label}] NASTRAN RMS"),
            &result.nastran_rms_m,
            &case.expected.nastran_rms_m,
        );
    }
    exact.finish();
    closed.finish();
}

#[test]
fn the_run_report_quotes_the_same_text_the_reference_does() {
    let fixture: Fixture = alas_testkit::load("struct", "nastran_results");
    let mut cmp = Comparison::new("alas-struct::nastran run report", Tier::Exact);

    for case in &fixture.text.tail {
        cmp.exact(
            &format!("tail[{}, {} lines]", case.label, case.n_lines),
            &tail(&case.text, case.n_lines),
            &case.expected,
        );
    }
    for case in &fixture.text.fatal_scan {
        let found = fatal_lines(&case.text);
        cmp.exact(
            &format!("fatal lines[{}]", case.label),
            &found,
            &case.expected_lines,
        );
        let reported: Vec<String> = found.iter().take(5).cloned().collect();
        cmp.exact(
            &format!("fatal lines reported[{}]", case.label),
            &reported,
            &case.expected_reported,
        );
    }
    cmp.finish();
}

/// A load case's name is `&'static str` upstream and here, so the fixture's
/// owned string is mapped back onto the three the reference defines. A fixture
/// naming a fourth would be a fixture out of step with `structural_loads`.
fn load_case_name(name: &str) -> &'static str {
    match name {
        "pull-up" => "pull-up",
        "push-down" => "push-down",
        "level" => "level",
        other => panic!("the fixture names a load case the reference does not define: {other}"),
    }
}
