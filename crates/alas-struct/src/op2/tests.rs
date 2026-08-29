// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

fn fortran_record(body: &[u8]) -> Vec<u8> {
    let mut record = Vec::with_capacity(body.len() + 8);
    record.extend_from_slice(&(body.len() as i32).to_le_bytes());
    record.extend_from_slice(body);
    record.extend_from_slice(&(body.len() as i32).to_le_bytes());
    record
}

fn word_record(word: i32) -> Vec<u8> {
    fortran_record(&word.to_le_bytes())
}

fn ident(analysis: i32, subcase: i32, element_type: i32, num_wide: i32) -> Vec<u8> {
    let mut words = vec![0i32; IDENT_WORDS];
    words[0] = analysis * 10 + 1;
    words[2] = element_type;
    words[3] = subcase;
    words[9] = num_wide;
    words.into_iter().flat_map(i32::to_le_bytes).collect()
}

fn vector_row(node_id: i32, t3: f32) -> Vec<u8> {
    let mut row = Vec::with_capacity(8 * 4);
    row.extend_from_slice(&(node_id * 10 + 1).to_le_bytes());
    row.extend_from_slice(&1i32.to_le_bytes());
    for value in [0.0, 0.0, t3, 0.0, 0.0, 0.0] {
        row.extend_from_slice(&value.to_le_bytes());
    }
    row
}

fn complex_vector_row(node_id: i32, magnitudes: [f32; 6], phases: [f32; 6]) -> Vec<u8> {
    let mut row = Vec::with_capacity(14 * 4);
    row.extend_from_slice(&(node_id * 10 + 1).to_le_bytes());
    row.extend_from_slice(&1i32.to_le_bytes());
    for value in magnitudes.into_iter().chain(phases) {
        row.extend_from_slice(&value.to_le_bytes());
    }
    row
}

fn append_table(records: &mut Vec<Vec<u8>>, table3: &[u8], chunks: &[Vec<u8>], marker: i32) {
    records.push(word_record(IDENT_WORDS as i32));
    records.push(fortran_record(table3));
    records.push(word_record(marker));
    records.push(word_record(1));
    records.push(word_record(0));
    for chunk in chunks {
        records.push(word_record((chunk.len() / 4) as i32));
        records.push(fortran_record(chunk));
    }
    records.push(word_record(marker - 1));
}

fn op2_stream(records: Vec<Vec<u8>>) -> Vec<u8> {
    records.into_iter().flatten().collect()
}

#[test]
fn a_name_record_is_uppercase_ascii_padded_to_eight_bytes() {
    assert_eq!(record_is_name(b"OUGV1   ").as_deref(), Some("OUGV1"));
    assert_eq!(record_is_name(b"OPHIG   ").as_deref(), Some("OPHIG"));
    assert_eq!(record_is_name(b"OES1X1  ").as_deref(), Some("OES1X1"));
    assert_eq!(record_is_name(b"XXXXXXXX").as_deref(), Some("XXXXXXXX"));
    assert_eq!(record_is_name(b"OUGV1"), None);
    assert_eq!(record_is_name(b"ougv1   "), None);
}

#[test]
fn a_short_record_reports_rather_than_panics() {
    assert_eq!(read_i32(&[1, 2, 3], 0), Err(Op2Error::RecordTooShort));
    assert_eq!(word_f32(&[0; 4], 2), Err(Op2Error::RecordTooShort));
}

#[test]
fn a_broken_length_word_is_a_framing_error() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&4i32.to_le_bytes());
    bytes.extend_from_slice(&0i32.to_le_bytes());
    bytes.extend_from_slice(&5i32.to_le_bytes());
    assert_eq!(
        split_records(&bytes),
        Err(Op2Error::RecordLengthMismatch { offset: 0 })
    );
}

#[test]
fn an_unknown_vector_width_is_refused_rather_than_misread() {
    let data = vec![0u8; 40];
    assert!(matches!(
        read_vector(&data, 10),
        Err(Op2Error::UnsupportedVectorWidth { num_wide: 10 })
    ));
}

#[test]
fn msc_table4_chunks_are_joined_and_force_vectors_are_reported_not_displacements() {
    let mut records = vec![fortran_record(b"OUG1    ")];
    append_table(
        &mut records,
        &ident(1, 1, 0, 8),
        &[vector_row(10, 0.25), vector_row(20, 0.50)],
        -4,
    );
    records.push(fortran_record(b"OPG1    "));
    append_table(
        &mut records,
        &ident(1, 1, 0, 8),
        &[vector_row(10, 99.0)],
        -8,
    );

    let parsed = read_op2(&op2_stream(records)).unwrap();
    let table = parsed.displacements.get(&1).unwrap();
    assert_eq!(table.node_ids, [10, 20]);
    assert_eq!(table.data[0][2], 0.25);
    assert_eq!(table.data[1][2], 0.50);
    assert_eq!(
        parsed.unread_result_tables,
        [UnreadResultTable::Vector {
            name: "OPG1".to_owned(),
            subcase: 1,
            num_wide: 8,
        }]
    );
}

#[test]
fn msc_oes_reports_bar_and_triangle_records_before_reading_cquad4_corners() {
    let mut records = vec![fortran_record(b"OES1X1  ")];
    append_table(&mut records, &ident(1, 1, 34, 16), &[vec![0; 16 * 4]], -4);

    let mut quad_words = vec![0u8; 87 * 4];
    quad_words[0..4].copy_from_slice(&101i32.to_le_bytes());
    let mut word = 2usize;
    for node in [4i32, 11, 12, 13, 14] {
        quad_words[word * 4..word * 4 + 4].copy_from_slice(&node.to_le_bytes());
        word += 1;
        for fiber in 0..2 {
            for component in 0..8 {
                let value = (fiber * 10 + component) as f32;
                quad_words[word * 4..word * 4 + 4].copy_from_slice(&value.to_le_bytes());
                word += 1;
            }
        }
    }
    append_table(
        &mut records,
        &ident(1, 1, 144, 87),
        &[quad_words.clone(), quad_words],
        -8,
    );
    append_table(&mut records, &ident(1, 1, 74, 17), &[vec![0; 17 * 4]], -12);

    let parsed = read_op2(&op2_stream(records)).unwrap();
    let table = parsed.cquad4_stress.get(&1).unwrap();
    assert_eq!(table.element_ids, [10; 20]);
    assert_eq!(
        table.node_ids,
        [0, 0, 11, 11, 12, 12, 13, 13, 14, 14, 0, 0, 11, 11, 12, 12, 13, 13, 14, 14,]
    );
    assert_eq!(table.data.len(), 20);
    assert_eq!(table.data[0][7], 7.0);
    assert_eq!(table.data[1][7], 17.0);
    assert_eq!(
        parsed.unread_result_tables,
        [
            UnreadResultTable::ElementStress {
                name: "OES1X1".to_owned(),
                subcase: 1,
                element_type: 34,
                num_wide: 16,
            },
            UnreadResultTable::ElementStress {
                name: "OES1X1".to_owned(),
                subcase: 1,
                element_type: 74,
                num_wide: 17,
            },
        ]
    );
}

#[test]
fn msc_modal_frequency_is_recovered_from_the_eigenvalue_when_its_word_is_zero() {
    let mut table3 = ident(2, 1, 0, 8);
    table3[4 * 4..5 * 4].copy_from_slice(&1i32.to_le_bytes());
    table3[5 * 4..6 * 4].copy_from_slice(&((2.0 * std::f32::consts::PI).powi(2)).to_le_bytes());
    let mut records = vec![fortran_record(b"OUG1    ")];
    append_table(&mut records, &table3, &[vector_row(10, 1.0)], -4);

    let parsed = read_op2(&op2_stream(records)).unwrap();
    let modes = parsed.eigenvectors.get(&1).unwrap();
    assert!((modes.mode_cycles[0] - 1.0).abs() < 1e-7);
}

#[test]
fn msc_phase_output_is_converted_to_real_and_imaginary_components() {
    let mut table3 = ident(5, 1, 0, 14);
    table3[4 * 4..5 * 4].copy_from_slice(&5.0f32.to_le_bytes());
    table3[8 * 4..9 * 4].copy_from_slice(&3i32.to_le_bytes());
    let data = complex_vector_row(
        536,
        [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        [0.0, 90.0, 180.0, -90.0, 45.0, -45.0],
    );
    let mut records = vec![fortran_record(b"OUG1    ")];
    append_table(&mut records, &table3, &[data], -4);

    let parsed = read_op2(&op2_stream(records)).unwrap();
    let response = parsed.complex_displacements.get(&1).unwrap();
    assert_eq!(response.freqs, [5.0]);
    assert!((response.real[0][0][0] - 1.0).abs() < 1e-12);
    assert!(response.real[0][0][1].abs() < 1e-12);
    assert!((response.imag[0][0][1] - 2.0).abs() < 1e-12);
    assert!((response.real[0][0][2] + 3.0).abs() < 1e-12);
    assert!((response.imag[0][0][3] + 4.0).abs() < 1e-12);
    assert!((response.real[0][0][4].hypot(response.imag[0][0][4]) - 5.0).abs() < 1e-12);
}
