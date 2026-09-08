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

include!("op2_parts/part_01.rs");
include!("op2_parts/part_02.rs");
