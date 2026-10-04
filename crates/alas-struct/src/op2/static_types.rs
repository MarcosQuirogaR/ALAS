// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The six real translation/rotation components of one static result table.
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
