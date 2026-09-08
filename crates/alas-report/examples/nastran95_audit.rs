// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Produce retained NASTRAN-95/MSC comparison artifacts for the structural
//! validation wingbox.
//!
//! The example is intentionally opt-in: it invokes external solvers and writes
//! a user-selected output tree. Set `ALAS_NASTRAN95_DIR` and, for the MSC side,
//! `ALAS_MSC_LAUNCHER` plus `ALAS_MSC_SOLVER`. An optional first argument is the
//! output directory; otherwise the repository's `tmp/nastran95_audit_20260822`
//! directory is used.

include!("nastran95_audit_parts/part_01.rs");
include!("nastran95_audit_parts/part_02.rs");
