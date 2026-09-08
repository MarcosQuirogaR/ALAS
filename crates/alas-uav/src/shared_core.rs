// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Adapter from preliminary UAV output to the production geometry and VLM core.
//!
//! The mixed optimizer owns preliminary sizing and its parabolic-polar checks.
//! This module does not replace those checks: it reconstructs the generated
//! aircraft with [`alas_geom`] primitives, independently runs
//! [`alas_aero::vlm`], and reports the disagreement. Airfoil identity and
//! operating point are explicit inputs because neither can be inferred from a
//! retail component catalogue.

include!("shared_core_parts/part_01.rs");
include!("shared_core_parts/part_02.rs");
