// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-struct::op2` against pyNastran's own OP2 reader, via
//! `golden/generators/gen_struct_op2.py`.
//!
//! This module is native; there is no Python in the reference to translate, so
//! the thing it must agree with is the reader the reference *uses*: pyNastran.
//! Each fixture case carries a real `.op2` file pyNastran wrote (as hex) and the
//! values pyNastran's reader recovered from it; the test decodes the same bytes
//! through this crate's reader and holds the two to `Tier::Exact`.
//!
//! Exact is the right tier and it is reachable: the stored values are `f32` bit
//! patterns widened to `f64`, which both readers do identically and losslessly,
//! so the numbers are equal to the bit and not merely close. Node and element
//! ids, being integers copied from the file, are compared by equality outright.
//!
//! The three cases are the three solves whose results the runner reads: a SOL
//! 101 file carrying static displacements and CQUAD4 corner stress together, a
//! SOL 103 eigenvector file, and a SOL 111 complex frequency-response file.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;

use alas_struct::op2::{read_op2, Op2};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    sol101_static: Sol101,
    sol103_modal: Sol103,
    sol111_freq: Sol111,
}

#[derive(Debug, Deserialize)]
struct Sol101 {
    op2_hex: String,
    displacements: BTreeMap<i64, VectorRecord>,
    cquad4_stress: BTreeMap<i64, StressRecord>,
}

#[derive(Debug, Deserialize)]
struct VectorRecord {
    node_ids: Vec<i64>,
    data: Vec<[f64; 6]>,
}

#[derive(Debug, Deserialize)]
struct StressRecord {
    element_ids: Vec<i64>,
    node_ids: Vec<i64>,
    data: Vec<[f64; 8]>,
}

#[derive(Debug, Deserialize)]
struct Sol103 {
    op2_hex: String,
    eigenvectors: BTreeMap<i64, EigenvectorRecord>,
}

#[derive(Debug, Deserialize)]
struct EigenvectorRecord {
    modes: Vec<i64>,
    eigenvalues: Vec<f64>,
    mode_cycles: Vec<f64>,
    node_ids: Vec<i64>,
    data: Vec<Vec<[f64; 6]>>,
}

#[derive(Debug, Deserialize)]
struct Sol111 {
    op2_hex: String,
    complex_displacements: BTreeMap<i64, ComplexRecord>,
}

#[derive(Debug, Deserialize)]
struct ComplexRecord {
    freqs: Vec<f64>,
    node_ids: Vec<i64>,
    real: Vec<Vec<[f64; 6]>>,
    imag: Vec<Vec<[f64; 6]>>,
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

/// Compare a list of six-component rows over a node line, elementwise.
fn compare_rows(cmp: &mut Comparison, label: &str, actual: &[[f64; 6]], expected: &[[f64; 6]]) {
    cmp.exact(
        &format!("{label} row count"),
        &actual.len(),
        &expected.len(),
    );
    for (node, (got, want)) in actual.iter().zip(expected).enumerate() {
        for component in 0..6 {
            cmp.scalar(
                &format!("{label}[node {node}][{component}]"),
                got[component],
                want[component],
            );
        }
    }
}

#[test]
fn sol101_static_displacements_and_corner_stress_match_pynastran() {
    let fixture: Fixture = alas_testkit::load("struct", "op2");
    let op2 = read(&fixture.sol101_static.op2_hex);
    let mut cmp = Comparison::new("alas-struct::op2 SOL 101", Tier::Exact);

    let disp_subcases: Vec<i64> = op2.displacements.keys().copied().collect();
    let want_disp: Vec<i64> = fixture
        .sol101_static
        .displacements
        .keys()
        .copied()
        .collect();
    cmp.exact("displacement subcases", &disp_subcases, &want_disp);
    for (sid, record) in &fixture.sol101_static.displacements {
        let table = op2
            .displacements
            .get(sid)
            .unwrap_or_else(|| panic!("missing displacement subcase {sid}"));
        cmp.exact(
            &format!("disp[{sid}] node ids"),
            &table.node_ids,
            &record.node_ids,
        );
        compare_rows(&mut cmp, &format!("disp[{sid}]"), &table.data, &record.data);
    }

    let stress_subcases: Vec<i64> = op2.cquad4_stress.keys().copied().collect();
    let want_stress: Vec<i64> = fixture
        .sol101_static
        .cquad4_stress
        .keys()
        .copied()
        .collect();
    cmp.exact("stress subcases", &stress_subcases, &want_stress);
    for (sid, record) in &fixture.sol101_static.cquad4_stress {
        let table = op2
            .cquad4_stress
            .get(sid)
            .unwrap_or_else(|| panic!("missing stress subcase {sid}"));
        cmp.exact(
            &format!("stress[{sid}] element ids"),
            &table.element_ids,
            &record.element_ids,
        );
        cmp.exact(
            &format!("stress[{sid}] node ids"),
            &table.node_ids,
            &record.node_ids,
        );
        cmp.exact(
            &format!("stress[{sid}] row count"),
            &table.data.len(),
            &record.data.len(),
        );
        for (row, (got, want)) in table.data.iter().zip(&record.data).enumerate() {
            for component in 0..8 {
                cmp.scalar(
                    &format!("stress[{sid}][row {row}][{component}]"),
                    got[component],
                    want[component],
                );
            }
        }
    }
    cmp.finish();
}

#[test]
fn sol103_eigenvectors_carry_their_frequencies_and_shapes() {
    let fixture: Fixture = alas_testkit::load("struct", "op2");
    let op2 = read(&fixture.sol103_modal.op2_hex);
    let mut cmp = Comparison::new("alas-struct::op2 SOL 103", Tier::Exact);

    let subcases: Vec<i64> = op2.eigenvectors.keys().copied().collect();
    let want: Vec<i64> = fixture.sol103_modal.eigenvectors.keys().copied().collect();
    cmp.exact("eigenvector subcases", &subcases, &want);
    for (sid, record) in &fixture.sol103_modal.eigenvectors {
        let table = op2
            .eigenvectors
            .get(sid)
            .unwrap_or_else(|| panic!("missing eigenvector subcase {sid}"));
        cmp.exact(&format!("eig[{sid}] modes"), &table.modes, &record.modes);
        cmp.exact(
            &format!("eig[{sid}] node ids"),
            &table.node_ids,
            &record.node_ids,
        );
        cmp.slice(
            &format!("eig[{sid}] eigenvalues"),
            &table.eigenvalues,
            &record.eigenvalues,
        );
        cmp.slice(
            &format!("eig[{sid}] mode cycles"),
            &table.mode_cycles,
            &record.mode_cycles,
        );
        cmp.exact(
            &format!("eig[{sid}] mode count"),
            &table.data.len(),
            &record.data.len(),
        );
        for (mode, (got, want)) in table.data.iter().zip(&record.data).enumerate() {
            compare_rows(&mut cmp, &format!("eig[{sid}][mode {mode}]"), got, want);
        }
    }
    cmp.finish();
}

#[test]
fn sol111_complex_frequency_response_recovers_real_and_imaginary_parts() {
    let fixture: Fixture = alas_testkit::load("struct", "op2");
    let op2 = read(&fixture.sol111_freq.op2_hex);
    let mut cmp = Comparison::new("alas-struct::op2 SOL 111", Tier::Exact);

    let subcases: Vec<i64> = op2.complex_displacements.keys().copied().collect();
    let want: Vec<i64> = fixture
        .sol111_freq
        .complex_displacements
        .keys()
        .copied()
        .collect();
    cmp.exact("complex subcases", &subcases, &want);
    for (sid, record) in &fixture.sol111_freq.complex_displacements {
        let table = op2
            .complex_displacements
            .get(sid)
            .unwrap_or_else(|| panic!("missing complex subcase {sid}"));
        cmp.exact(
            &format!("cdisp[{sid}] node ids"),
            &table.node_ids,
            &record.node_ids,
        );
        cmp.slice(&format!("cdisp[{sid}] freqs"), &table.freqs, &record.freqs);
        cmp.exact(
            &format!("cdisp[{sid}] real freq count"),
            &table.real.len(),
            &record.real.len(),
        );
        for (freq, (got, want)) in table.real.iter().zip(&record.real).enumerate() {
            compare_rows(
                &mut cmp,
                &format!("cdisp[{sid}][freq {freq}] re"),
                got,
                want,
            );
        }
        for (freq, (got, want)) in table.imag.iter().zip(&record.imag).enumerate() {
            compare_rows(
                &mut cmp,
                &format!("cdisp[{sid}][freq {freq}] im"),
                got,
                want,
            );
        }
    }
    cmp.finish();
}
