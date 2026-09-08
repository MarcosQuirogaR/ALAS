// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// Read a CQUAD4-144 corner-stress DATA record: 87 words per element --
/// `[eid*10+device, 'CEN/', then five nodes of (gid, two fibers x eight)]`.
fn read_cquad4_corner(table4: &[u8], table: &mut StressTable) -> Result<(), Op2Error> {
    let total = table4.len() / 4;
    if total % CQUAD4_CORNER_WORDS != 0 {
        return Err(Op2Error::RecordTooShort);
    }
    let mut p = 0usize;
    while p < total {
        let eid = i64::from(word_i32(table4, p)? / 10);
        p += 2; // element id (with device code) and the 'CEN/' word
        for node_index in 0..CORNER_NODES {
            let raw_gid = i64::from(word_i32(table4, p)?);
            // MSC writes the CQUAD4 corner count (`4`) in the centroid slot;
            // pyNastran normalizes that logical location to grid id zero. Its
            // synthesized OP2 writes a literal zero, so this correction is
            // two-sided and preserves the frozen fixture.
            let gid = if node_index == 0 { 0 } else { raw_gid };
            p += 1;
            for _fiber in 0..2 {
                let mut row = [0.0f64; 8];
                for (component, slot) in row.iter_mut().enumerate() {
                    *slot = f64::from(word_f32(table4, p + component)?);
                }
                p += 8;
                table.element_ids.push(eid);
                table.node_ids.push(gid);
                table.data.push(row);
            }
        }
    }
    Ok(())
}

/// The nodes a CQUAD4 corner record reports: the centroid then four corners.
const CORNER_NODES: usize = 5;

/// CQUAD4-144 corner stress words per element.
const CQUAD4_CORNER_WORDS: usize = 87;

/// Six consecutive `f32` words widened to `f64`, from `word_index`.
fn read_six(record: &[u8], word_index: usize) -> Result<[f64; 6], Op2Error> {
    let mut out = [0.0f64; 6];
    for (offset, slot) in out.iter_mut().enumerate() {
        *slot = f64::from(word_f32(record, word_index + offset)?);
    }
    Ok(out)
}

/// The `word_index`-th 4-byte word of a record as an `i32`.
fn word_i32(record: &[u8], word_index: usize) -> Result<i32, Op2Error> {
    read_i32(record, word_index * 4)
}

/// The `word_index`-th 4-byte word of a record as an `f32`.
fn word_f32(record: &[u8], word_index: usize) -> Result<f32, Op2Error> {
    read_f32(record, word_index * 4)
}

/// A one-word record read as an `i32`, or `None` if it is not exactly one word
/// -- the test for the marker records that punctuate the stream.
fn as_i32(record: &[u8]) -> Option<i32> {
    match record {
        &[a, b, c, d] => Some(i32::from_le_bytes([a, b, c, d])),
        _ => None,
    }
}

/// A little-endian `i32` at a byte offset, or an error if the bytes are short.
fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, Op2Error> {
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

#[cfg(test)]
#[path = "../op2/tests.rs"]
mod tests;

