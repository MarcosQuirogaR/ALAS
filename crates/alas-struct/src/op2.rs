// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reading NASTRAN's binary OP2 result file, natively.
//!
//! This has no Python counterpart to translate. The reference never parses OP2
//! itself: `alas/integration/nastran_runner.py` hands the file to pyNastran's
//! `OP2.read_op2` and reads values off the object it gets back. There is no Rust
//! pyNastran, and taking an external dependency on one does not exist, so the
//! reader is written here. What it must agree with is therefore pyNastran's
//! reader -- given the same bytes, recover the same numbers -- which is exactly
//! what `parity_op2.rs` holds it to, on files pyNastran itself wrote.
//!
//! It is scoped to the four tables those readers ask for, and no further:
//! SOL 101 real static displacements ([`Op2::displacements`]) and CQUAD4 corner
//! von Mises stress ([`Op2::cquad4_stress`]); SOL 103 eigenvectors with their
//! eigenvalues and mode cycles ([`Op2::eigenvectors`]); and SOL 111 complex
//! frequency-response displacements ([`Op2::complex_displacements`]). Every
//! other result table a general OP2 can carry is catalogued in
//! [`Op2::unread_result_tables`] rather than mis-parsed or silently discarded.
//!
//! The format, as pyNastran writes it and a real MSC.Nastran run does: a stream
//! of Fortran unformatted records, each `[len][body][len]` with 32-bit
//! little-endian length markers. A file header ends at the first `(-1, 0)`
//! marker pair. Each datablock opens with an 8-character ASCII name record
//! (`OUGV1`, `OPHIG`, `OES1X1`); inside it a 146-word record is the IDENT
//! (table-3) parameter block and one or more DATA records form table-4. MSC
//! splits a large table-4 at its internal record-size ceiling, so the word-count
//! records are used to join every chunk before decoding. The values this reader
//! wants live in table-3 (analysis code, subcase, element type, num-wide, and
//! the eigenvalue/frequency) and table-4 (the node line and the numbers).

use std::collections::BTreeMap;

/// One real vector result at one subcase: static displacements.
///
/// `data[i]` are the six components `[t1, t2, t3, r1, r2, r3]` at
/// `node_ids[i]`, in the file's own node order.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorTable {
    /// The grid ids, device code stripped, in file order.
    pub node_ids: Vec<i64>,
    /// Six components per node.
    pub data: Vec<[f64; 6]>,
}

/// The eigenvectors of one subcase, one entry per mode.
///
/// `modes`, `eigenvalues` and `mode_cycles` are parallel across modes;
/// `data[m]` is the mode-`m` shape over `node_ids`.
#[derive(Debug, Clone, PartialEq)]
pub struct EigenvectorTable {
    /// The 1-based mode numbers.
    pub modes: Vec<i64>,
    /// Each mode's eigenvalue (`rad^2/s^2`).
    pub eigenvalues: Vec<f64>,
    /// Each mode's frequency in Hz -- the reference's `mode_cycles`.
    pub mode_cycles: Vec<f64>,
    /// The grid ids, device code stripped.
    pub node_ids: Vec<i64>,
    /// `data[mode][node]` -- six components each.
    pub data: Vec<Vec<[f64; 6]>>,
}

/// Complex frequency-response displacements of one subcase, over frequencies.
///
/// `real[f]`/`imag[f]` are the response at `freqs[f]` over `node_ids`.
#[derive(Debug, Clone, PartialEq)]
pub struct ComplexVectorTable {
    /// The excitation frequencies, Hz.
    pub freqs: Vec<f64>,
    /// The grid ids, device code stripped.
    pub node_ids: Vec<i64>,
    /// `real[freq][node]` -- six real parts each.
    pub real: Vec<Vec<[f64; 6]>>,
    /// `imag[freq][node]` -- six imaginary parts each.
    pub imag: Vec<Vec<[f64; 6]>>,
}

/// CQUAD4 corner stress at one subcase, one row per element/node/fiber.
///
/// The reference reads only the von Mises column; the whole eight-component row
/// `[fiber_distance, oxx, oyy, txy, angle, major, minor, von_mises]` is kept so
/// the reader is faithful rather than pre-digested. `element_ids[i]` and
/// `node_ids[i]` name row `i` (node id `0` is the centroid).
#[derive(Debug, Clone, PartialEq)]
pub struct StressTable {
    /// The element id of each row.
    pub element_ids: Vec<i64>,
    /// The grid id of each row (`0` is the element centroid).
    pub node_ids: Vec<i64>,
    /// Eight stress components per row; index 7 is von Mises.
    pub data: Vec<[f64; 8]>,
}

/// A result table whose layout is not part of this reader's requested output.
///
/// The data are intentionally not decoded, but this typed record prevents a
/// solver-success path from being mistaken for complete OP2 coverage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnreadResultTable {
    /// A vector-shaped result other than displacement, eigenvector, or FRF.
    Vector {
        /// Eight-character OP2 table name with trailing spaces removed.
        name: String,
        /// Result subcase identifier.
        subcase: i64,
        /// Words per grid record declared by table-3.
        num_wide: i64,
    },
    /// An element-stress layout other than CQUAD4-144 corner stress.
    ElementStress {
        /// Eight-character OP2 table name with trailing spaces removed.
        name: String,
        /// Result subcase identifier.
        subcase: i64,
        /// NASTRAN element type from table-3.
        element_type: i32,
        /// Words per element record declared by table-3.
        num_wide: i32,
    },
}

/// Everything this reader recovers from one OP2 file.
///
/// Each map is keyed by subcase id, matching the dictionaries the reference
/// indexes (`op2.displacements[sid]`, `op2.eigenvectors[1]`, ...). A file that
/// carries none of the four in-scope tables yields empty maps; the output
/// tables it did carry are nevertheless visible in `unread_result_tables`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Op2 {
    /// Real static displacements, by subcase.
    pub displacements: BTreeMap<i64, VectorTable>,
    /// Eigenvectors, by subcase.
    pub eigenvectors: BTreeMap<i64, EigenvectorTable>,
    /// Complex frequency-response displacements, by subcase.
    pub complex_displacements: BTreeMap<i64, ComplexVectorTable>,
    /// CQUAD4 corner stress, by subcase.
    pub cquad4_stress: BTreeMap<i64, StressTable>,
    /// Result tables intentionally not decoded by this scoped reader.
    pub unread_result_tables: Vec<UnreadResultTable>,
}

/// What can go wrong reading a file that is not the OP2 this reader expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Op2Error {
    /// A Fortran record's trailing length word did not match its leading one,
    /// so the file's record framing is not what this reader parses.
    #[error("OP2 record framing is broken: length words disagree at byte {offset}")]
    RecordLengthMismatch {
        /// Byte offset of the record whose two length words disagree.
        offset: usize,
    },
    /// A record was shorter than the field being read out of it -- a table
    /// whose declared width overran its own data.
    #[error("OP2 record is too short for the field being read")]
    RecordTooShort,
    /// A table-3 declared a per-node width this reader does not know how to
    /// walk (it reads only the 8-word real and 14-word complex vector layouts).
    #[error("unsupported OP2 vector width {num_wide} (expected 8 or 14)")]
    UnsupportedVectorWidth {
        /// The `num_wide` the IDENT block declared.
        num_wide: i64,
    },
}

/// Parse an OP2 file's bytes into the tables this program reads.
///
/// Recognised datablocks are decoded; unrecognised ones are stepped over. The
/// only errors are structural: framing that is not this format's, or a table
/// whose declared shape overruns its data.
pub fn read_op2(bytes: &[u8]) -> Result<Op2, Op2Error> {
    let records = split_records(bytes)?;
    parse(&records)
}

/// Split the byte stream into Fortran record bodies.
///
/// Each record is `[len][body][len]` with 32-bit little-endian length words.
/// Reading stops cleanly at the first length word that cannot begin a record
/// (non-positive, or overrunning the file) -- that is how the trailing
/// end-of-file marker and any pad are reached. A record whose two length words
/// disagree is corruption, and is reported.
fn split_records(bytes: &[u8]) -> Result<Vec<&[u8]>, Op2Error> {
    let mut records = Vec::new();
    let mut p = 0usize;
    while p + 8 <= bytes.len() {
        let len = read_i32(bytes, p)?;
        if len <= 0 {
            break;
        }
        let len = len as usize;
        let end = match p.checked_add(8).and_then(|v| v.checked_add(len)) {
            Some(end) if end <= bytes.len() => end,
            _ => break,
        };
        let trailing = read_i32(bytes, p + 4 + len)?;
        if trailing != len as i32 {
            return Err(Op2Error::RecordLengthMismatch { offset: p });
        }
        records.push(&bytes[p + 4..p + 4 + len]);
        p = end;
    }
    Ok(records)
}

/// The datablock name records this reader routes on -- eigenvectors arrive in
/// `OPHIG`, everything else in a name starting `OUG` (displacement family) or
/// `OES` (element stress).
fn record_is_name(record: &[u8]) -> Option<String> {
    if record.len() != 8 {
        return None;
    }
    if !record
        .iter()
        .all(|&c| c == b' ' || c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return None;
    }
    if !record[0].is_ascii_uppercase() {
        return None;
    }
    Some(String::from_utf8_lossy(record).trim_end().to_string())
}

/// Walk the records, joining every DATA chunk belonging to one 146-word IDENT
/// and routing the completed table by its datablock name and analysis code.
fn parse(records: &[&[u8]]) -> Result<Op2, Op2Error> {
    let mut op2 = Op2::default();
    let mut i = skip_header(records);
    let mut name = String::new();
    let mut pending: Option<PendingTable<'_>> = None;
    let mut declared_words: Option<usize> = None;
    while i < records.len() {
        let record = records[i];
        i += 1;
        if let Some(found) = record_is_name(record) {
            flush_pending(&mut op2, &mut pending)?;
            name = found;
            declared_words = None;
            continue;
        }
        if let Some(word) = as_i32(record) {
            if word < 0 && pending.as_ref().is_some_and(|table| !table.data.is_empty()) {
                flush_pending(&mut op2, &mut pending)?;
            }
            declared_words = (word > 1).then_some(word as usize);
            continue;
        }
        let words = record.len() / 4;
        if declared_words == Some(words) {
            declared_words = None;
            if let Some(table) = pending.as_mut() {
                table.data.extend_from_slice(record);
            } else if words == IDENT_WORDS {
                pending = Some(PendingTable {
                    name: name.clone(),
                    ident: record,
                    data: Vec::new(),
                });
            }
        }
    }
    flush_pending(&mut op2, &mut pending)?;
    Ok(op2)
}

/// One table-3 and all table-4 record bodies that follow it.
struct PendingTable<'a> {
    name: String,
    ident: &'a [u8],
    data: Vec<u8>,
}

fn flush_pending(op2: &mut Op2, pending: &mut Option<PendingTable<'_>>) -> Result<(), Op2Error> {
    let Some(table) = pending.take() else {
        return Ok(());
    };
    if table.data.is_empty() {
        return Ok(());
    }
    store(op2, &table.name, table.ident, &table.data)
}

/// The number of 4-byte words in a table-3 IDENT record.
const IDENT_WORDS: usize = 146;

/// Skip the file header: the records up to and including the first `(-1, 0)`
/// marker pair, after which the first datablock begins.
fn skip_header(records: &[&[u8]]) -> usize {
    for i in 0..records.len().saturating_sub(1) {
        if as_i32(records[i]) == Some(-1) && as_i32(records[i + 1]) == Some(0) {
            return i + 2;
        }
    }
    0
}

/// Route one (IDENT, DATA) pair into the right table.
fn store(op2: &mut Op2, name: &str, table3: &[u8], table4: &[u8]) -> Result<(), Op2Error> {
    let analysis = word_i32(table3, 0)? / 10; // approach code = analysis*10 + device
    let subcase = i64::from(word_i32(table3, 3)?);
    if name.starts_with("OES") {
        // MSC groups several element layouts in OES1X1. Only element type 144
        // is CQUAD4 corner stress (87 words); CBAR type 34 and CTRIA3 type 74
        // must be skipped rather than decoded as quads.
        let element_type = word_i32(table3, 2)?;
        let num_wide = word_i32(table3, 9)?;
        if element_type != 144 || num_wide != 87 {
            op2.unread_result_tables
                .push(UnreadResultTable::ElementStress {
                    name: name.to_owned(),
                    subcase,
                    element_type,
                    num_wide,
                });
            return Ok(());
        }
        let table = op2
            .cquad4_stress
            .entry(subcase)
            .or_insert_with(|| StressTable {
                element_ids: Vec::new(),
                node_ids: Vec::new(),
                data: Vec::new(),
            });
        read_cquad4_corner(table4, table)?;
        return Ok(());
    }
    // OQG carries SPC forces and OPG carries applied loads in the same vector
    // shape. Treating either as displacement would overwrite OUG in a real MSC
    // file because OPG is written after OUG.
    if !name.starts_with("OUG") && name != "OPHIG" {
        op2.unread_result_tables.push(UnreadResultTable::Vector {
            name: name.to_owned(),
            subcase,
            num_wide: i64::from(word_i32(table3, 9)?),
        });
        return Ok(());
    }
    let num_wide = i64::from(word_i32(table3, 9)?);
    let mut nodes = read_vector(table4, num_wide)?;
    match analysis {
        2 => {
            // Eigenvectors: one IDENT/DATA pair per mode, all one subcase.
            let mode = i64::from(word_i32(table3, 4)?);
            let eigenvalue = f64::from(word_f32(table3, 5)?);
            let written_cycle = f64::from(word_f32(table3, 6)?);
            // pyNastran-authored fixtures carry mode_cycle in word 7. MSC
            // 2026.1 leaves that word zero while writing the eigenvalue, so use
            // the defining relation lambda = (2*pi*f)^2 only for the absent
            // product field. A negative eigenvalue has no real cyclic
            // frequency and remains NaN rather than being silently absoluted.
            let cycle = if written_cycle.is_finite() && written_cycle > 0.0 {
                written_cycle
            } else if eigenvalue > 0.0 {
                eigenvalue.sqrt() / std::f64::consts::TAU
            } else {
                f64::NAN
            };
            let table = op2
                .eigenvectors
                .entry(subcase)
                .or_insert_with(|| EigenvectorTable {
                    modes: Vec::new(),
                    eigenvalues: Vec::new(),
                    mode_cycles: Vec::new(),
                    node_ids: nodes.node_ids.clone(),
                    data: Vec::new(),
                });
            table.modes.push(mode);
            table.eigenvalues.push(eigenvalue);
            table.mode_cycles.push(cycle);
            table.data.push(nodes.real);
        }
        5 => {
            // Complex frequency response: one pair per frequency, one subcase.
            // FORMAT=3 stores magnitudes followed by phase angles in degrees;
            // MSC writes this when case control requests PHASE. The synthetic
            // pyNastran fixture uses FORMAT=2 real/imaginary pairs instead.
            if word_i32(table3, 8)? == 3 {
                magnitude_phase_to_rectangular(&mut nodes);
            }
            let freq = f64::from(word_f32(table3, 4)?);
            let table =
                op2.complex_displacements
                    .entry(subcase)
                    .or_insert_with(|| ComplexVectorTable {
                        freqs: Vec::new(),
                        node_ids: nodes.node_ids.clone(),
                        real: Vec::new(),
                        imag: Vec::new(),
                    });
            table.freqs.push(freq);
            table.real.push(nodes.real);
            table.imag.push(nodes.imag);
        }
        _ => {
            op2.displacements.insert(
                subcase,
                VectorTable {
                    node_ids: nodes.node_ids,
                    data: nodes.real,
                },
            );
        }
    }
    Ok(())
}

/// Convert a FORMAT=3 complex vector from magnitude/degrees to Cartesian form.
fn magnitude_phase_to_rectangular(rows: &mut VectorRows) {
    for (magnitudes, phases) in rows.real.iter_mut().zip(&mut rows.imag) {
        for (magnitude, phase_degrees) in magnitudes.iter_mut().zip(phases.iter_mut()) {
            let phase_radians = phase_degrees.to_radians();
            let real = *magnitude * phase_radians.cos();
            let imaginary = *magnitude * phase_radians.sin();
            *magnitude = real;
            *phase_degrees = imaginary;
        }
    }
}

/// The node line and per-node components read out of one DATA record.
struct VectorRows {
    node_ids: Vec<i64>,
    real: Vec<[f64; 6]>,
    imag: Vec<[f64; 6]>,
}

/// Read a displacement/eigenvector/complex DATA record: `num_wide` words per
/// node -- `[nid*10+device, gridtype, six real (+ six imaginary)]`.
fn read_vector(table4: &[u8], num_wide: i64) -> Result<VectorRows, Op2Error> {
    let per = match num_wide {
        8 | 14 => num_wide as usize,
        other => return Err(Op2Error::UnsupportedVectorWidth { num_wide: other }),
    };
    let total = table4.len() / 4;
    if total % per != 0 {
        return Err(Op2Error::RecordTooShort);
    }
    let count = total / per;
    let mut rows = VectorRows {
        node_ids: Vec::with_capacity(count),
        real: Vec::with_capacity(count),
        imag: Vec::with_capacity(count),
    };
    for k in 0..count {
        let base = k * per;
        rows.node_ids.push(i64::from(word_i32(table4, base)? / 10));
        rows.real.push(read_six(table4, base + 2)?);
        if per == 14 {
            rows.imag.push(read_six(table4, base + 8)?);
        }
    }
    Ok(rows)
}

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
#[path = "op2/tests.rs"]
mod tests;
