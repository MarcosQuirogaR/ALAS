// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reading NASTRAN's binary OP2 result file, natively.
//!
//! This has no Python counterpart to translate. The reference never parses OP2
//! itself: `alas/integration/nastran_runner.py` hands the file to pyNastran's
//! `OP2.read_op2` and reads values off the object it gets back. There is no Rust
//! pyNastran, and taking an external dependency on one does not exist, so the
//! reader is written here. What it must agree with is therefore pyNastran's
//! reader (given the same bytes, recover the same numbers) which is exactly
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

mod records;
mod static_types;
mod tables;
#[cfg(test)]
mod tests;
mod words;

use records::*;
pub use static_types::VectorTable;
use tables::*;
use words::*;

use std::collections::BTreeMap;

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
    /// Each mode's frequency in Hz: the reference's `mode_cycles`.
    pub mode_cycles: Vec<f64>,
    /// The grid ids, device code stripped.
    pub node_ids: Vec<i64>,
    /// `data[mode][node]`, six components each.
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
    /// `real[freq][node]`, six real parts each.
    pub real: Vec<Vec<[f64; 6]>>,
    /// `imag[freq][node]`, six imaginary parts each.
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
    /// Repeated complete static IDENT/DATA tables for one subcase. Legacy
    /// readers retain their last-table behavior; product acceptance rejects
    /// this ambiguous output. Table-4 chunks of one IDENT are joined earlier.
    pub duplicate_static_subcases: Vec<i64>,
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
    /// A record was shorter than the field being read out of it: a table
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
