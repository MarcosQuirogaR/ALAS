// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The per-Mach columns a table held before it is tabulated again on a
//! refined grid.
//!
//! A refinement pass keeps every node and inserts cell midpoints. Each column
//! entry is a function of its own Mach number alone, so a kept node's entries
//! are copied rather than recomputed: the copy is the value a recomputation
//! returns, to the bit, and it already passed the finiteness checks.

use super::{DragAreaColumn, TrimmedDragTable};

/// What [`KeptNodes::take`] emptied out of a table's per-Mach columns.
pub(super) struct KeptNodes {
    mach: Vec<f64>,
    wave: Vec<f64>,
    wave_slope: Vec<f64>,
    factor: Vec<Vec<f64>>,
    areas: Vec<Vec<f64>>,
}

impl KeptNodes {
    /// Set `table`'s nodes to `mach` and empty its per-Mach columns, keeping
    /// what they held.
    pub(super) fn take(table: &mut TrimmedDragTable, mach: &[f64]) -> Self {
        Self {
            mach: std::mem::replace(&mut table.mach, mach.to_vec()),
            wave: std::mem::take(&mut table.wave),
            wave_slope: std::mem::take(&mut table.wave_slope),
            factor: table
                .parasite
                .iter_mut()
                .map(|component| std::mem::take(&mut component.factor))
                .collect(),
            areas: DragAreaColumn::take_all(&mut table.drag_areas),
        }
    }

    /// Append the kept entries of node `m` to `table`'s columns; `false`,
    /// with nothing appended, when `m` was not a node.
    pub(super) fn push(&self, table: &mut TrimmedDragTable, m: f64) -> bool {
        let Some(old) = self.mach.iter().position(|p| p.to_bits() == m.to_bits()) else {
            return false;
        };
        let n_cl = table.cl.len();
        DragAreaColumn::push_previous(&mut table.drag_areas, &self.areas, old);
        for (component, factor) in table.parasite.iter_mut().zip(&self.factor) {
            component.factor.push(factor[old]);
        }
        table
            .wave
            .extend_from_slice(&self.wave[old * n_cl..(old + 1) * n_cl]);
        table
            .wave_slope
            .extend_from_slice(&self.wave_slope[old * n_cl..(old + 1) * n_cl]);
        true
    }
}
