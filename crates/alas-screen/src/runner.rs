// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/analysis/airfoil_screening.py
// Reference: alas @ rust-port-baseline.

//! Screening runner orchestrating Stage 1 (2-D), Stage 2 (3-D), and Stage 3 (MSES).

include!("runner_parts/part_01.rs");
include!("runner_parts/part_02.rs");
include!("runner_parts/part_03.rs");
