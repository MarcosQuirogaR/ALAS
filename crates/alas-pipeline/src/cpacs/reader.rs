// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CPACS 3.5 XML reader and reference validation.
//!
//! The reader is a structural boundary. It keeps the values written by the
//! current exporter, including SI coordinates, transformations, symmetry and
//! UIDs, then validates references before returning the typed document.

include!("reader_parts/part_01.rs");
include!("reader_parts/part_02.rs");
