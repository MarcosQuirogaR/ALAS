// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Little-endian word access.

use super::*;

pub(super) fn read_six(record: &[u8], word_index: usize) -> Result<[f64; 6], Op2Error> {
    let mut out = [0.0f64; 6];
    for (offset, slot) in out.iter_mut().enumerate() {
        *slot = f64::from(word_f32(record, word_index + offset)?);
    }
    Ok(out)
}

/// The `word_index`-th 4-byte word of a record as an `i32`.
pub(super) fn word_i32(record: &[u8], word_index: usize) -> Result<i32, Op2Error> {
    read_i32(record, word_index * 4)
}

/// The `word_index`-th 4-byte word of a record as an `f32`.
pub(super) fn word_f32(record: &[u8], word_index: usize) -> Result<f32, Op2Error> {
    read_f32(record, word_index * 4)
}

/// A one-word record read as an `i32`, or `None` if it is not exactly one word:
/// the test for the marker records that punctuate the stream.
pub(super) fn as_i32(record: &[u8]) -> Option<i32> {
    match record {
        &[a, b, c, d] => Some(i32::from_le_bytes([a, b, c, d])),
        _ => None,
    }
}

/// A little-endian `i32` at a byte offset, or an error if the bytes are short.
pub(super) fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, Op2Error> {
    match bytes.get(offset..offset + 4) {
        Some(&[a, b, c, d]) => Ok(i32::from_le_bytes([a, b, c, d])),
        _ => Err(Op2Error::RecordTooShort),
    }
}

/// A little-endian `f32` at a byte offset, or an error if the bytes are short.
fn read_f32(bytes: &[u8], offset: usize) -> Result<f32, Op2Error> {
    match bytes.get(offset..offset + 4) {
        Some(&[a, b, c, d]) => Ok(f32::from_le_bytes([a, b, c, d])),
        _ => Err(Op2Error::RecordTooShort),
    }
}
