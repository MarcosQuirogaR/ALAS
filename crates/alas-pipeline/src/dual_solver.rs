// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Independent VLM and AVL optimization branches.
//!
//! The native VLM objective remains the reference product path. The AVL
//! branch uses the same design vector, geometry builder, and hard feasibility
//! checks, but scores the admitted AVL Trefftz induced drag at the required
//! cruise lift. Keeping the branches in separate output namespaces means a
//! slow or unavailable external executable cannot corrupt the VLM result.

include!("dual_solver_parts/part_01.rs");
include!("dual_solver_parts/part_02.rs");
