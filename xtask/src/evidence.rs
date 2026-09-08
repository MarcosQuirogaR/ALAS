// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bookkeeping checks for the evidence described by the porting ledger.
//!
//! These checks can establish that a declared fixture, manifest and generator
//! are present and connected to a literal parity consumer. They cannot show
//! that a fixture samples the right branch, or that a passing parity test is a
//! physically correct model; those remain review and test responsibilities.

include!("evidence_parts/part_01.rs");
include!("evidence_parts/part_02.rs");
