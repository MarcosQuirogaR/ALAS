// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The influence-matrix factorization a [`super::VlmSystem`] solves with:
//! the ordinary LU, or the Schur-complement block elimination of
//! [`alas_math::block_lu`] over the panels that equal a stored base lattice
//! (everything except a rotating stabilizer). The block form differs from
//! the full LU only by rounding; it is enabled by
//! [`crate::vlm::speed::schur_lu`] and abandons itself near the pivot-ratio
//! gate, so the ordinary LU and its gate still decide there.

use std::sync::Arc;

use alas_math::block_lu::{BlockBase, BlockFactorization};
use alas_math::linalg::{LuFactorization, SolveDiagnostics};

use super::Panel;

/// The factorization of one assembled lattice.
#[derive(Debug)]
pub(in crate::vlm) enum AicFactorization {
    /// The ordinary partial-pivot LU of the whole matrix.
    Full(LuFactorization),
    /// The block elimination over a frozen panel set.
    Block(BlockFactorization),
}

impl AicFactorization {
    /// Solve `A x = b`, with the residual evidence of [`LuFactorization`].
    pub(in crate::vlm) fn solve_vector(&self, b: &[f64]) -> (Vec<f64>, SolveDiagnostics) {
        match self {
            Self::Full(lu) => lu.solve_vector(b),
            Self::Block(block) => block.solve_vector(b),
        }
    }

    /// True for the block elimination.
    #[cfg(test)]
    pub(in crate::vlm) fn is_block(&self) -> bool {
        matches!(self, Self::Block(_))
    }
}

/// A [`BlockBase`] with the panels that define its frozen block.
#[derive(Debug)]
pub(in crate::vlm) struct FrozenLattice {
    base: Arc<BlockBase>,
    frozen_panels: Vec<Panel>,
    n: usize,
}

impl FrozenLattice {
    /// Freeze the panels with `moving[i] == false` of `panels`, factoring
    /// their block of `aic` (row-major). `None` if that fails.
    pub(in crate::vlm) fn build(aic: &[f64], panels: &[Panel], moving: &[bool]) -> Option<Self> {
        let n = panels.len();
        let frozen: Vec<usize> = (0..n).filter(|&i| !moving[i]).collect();
        let others: Vec<usize> = (0..n).filter(|&i| moving[i]).collect();
        let frozen_panels = frozen.iter().map(|&i| panels[i].clone()).collect();
        let base = BlockBase::build(aic, n, frozen, others)?;
        Some(Self {
            base: Arc::new(base),
            frozen_panels,
            n,
        })
    }

    /// Whether the frozen panels of `panels` all equal the stored ones, and
    /// the moving set is small enough for the block update to pay off.
    pub(in crate::vlm) fn applies_to(&self, panels: &[Panel]) -> bool {
        panels.len() == self.n
            && 2 * self.base.moving_count() <= self.n
            && self
                .base
                .frozen()
                .iter()
                .zip(&self.frozen_panels)
                .all(|(&i, panel)| panels[i] == *panel)
    }

    /// Block-factor `aic`; `None` abandons the update (see the module doc).
    pub(in crate::vlm) fn factor(
        &self,
        aic: Arc<Vec<f64>>,
        max_pivot_ratio: f64,
    ) -> Option<BlockFactorization> {
        // Half the gate: the block pivots are not the full LU's.
        BlockFactorization::build(self.base.clone(), aic, 0.5 * max_pivot_ratio)
    }
}
