// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The factored influence matrices of recently assembled lattices, for
//! [`super::VlmSystem::assemble`] and its reference-compatibility twin.
//!
//! One candidate evaluation assembles the same lattice from several places
//! that do not share a system: the stretched cruise lattice for the clean
//! and the aeroelastic neutral points and the trimmed performance, the wing
//! alone for each buffet strip candidate, the isolated-tail subsets of every
//! neutral-point probe. The fill and the factorization are functions of the
//! panels alone (and of whether the reference inter-surface core was
//! applied, which changes the panels before they are compared), so an
//! assembly whose panels equal a held lattice's, field for field and bit for
//! bit, takes that lattice's factorization, near-field kernel and
//! superposition basis instead of recomputing them: the same numbers, by
//! construction. Only lattices this
//! path factored from scratch are held; the incremental and block-updated
//! factorizations of [`super::VlmGeometryCache`] differ from a fresh one by
//! rounding and never enter.
//!
//! The memo is shared by every thread: a hit returns exactly what a miss
//! would compute, so which thread filled an entry cannot change a result.
//! It is bounded by the bytes its factorizations hold.

use std::sync::{Arc, Mutex, OnceLock};

use super::schur::AicFactorization;
use super::superposition::BasisSlot;
use super::Panel;

/// The near-field kernel slot shared by the systems of one lattice.
pub(super) type KernelSlot = Arc<OnceLock<Option<Arc<Vec<[f64; 3]>>>>>;

/// Bytes the memo may hold, counted per lattice as five `n x n` arrays of
/// `f64`: the matrix kept for the residual evidence, its LU, and the
/// near-field kernel's three components per entry.
const BUDGET_BYTES: usize = 768 << 20;

struct Entry {
    reference_core: bool,
    panels: Vec<Panel>,
    factorization: Arc<AicFactorization>,
    kernel: KernelSlot,
    basis: BasisSlot,
}

impl Entry {
    fn bytes(&self) -> usize {
        let n = self.panels.len();
        5 * n * n * std::mem::size_of::<f64>()
    }
}

fn memo() -> &'static Mutex<Vec<Entry>> {
    static MEMO: OnceLock<Mutex<Vec<Entry>>> = OnceLock::new();
    MEMO.get_or_init(|| Mutex::new(Vec::new()))
}

/// The factorization, kernel and basis slots of a held lattice equal to `panels`,
/// most recently used last.
pub(super) fn get(
    panels: &[Panel],
    reference_core: bool,
) -> Option<(Arc<AicFactorization>, KernelSlot, BasisSlot)> {
    let mut held = memo().lock().ok()?;
    let index = held.iter().position(|entry| {
        entry.reference_core == reference_core
            && entry.panels.len() == panels.len()
            && entry.panels.as_slice() == panels
    })?;
    let entry = held.remove(index);
    let found = (
        entry.factorization.clone(),
        entry.kernel.clone(),
        entry.basis.clone(),
    );
    held.push(entry);
    Some(found)
}

/// Hold a lattice factored from scratch, evicting the least recently used
/// ones beyond the budget. A lattice larger than the whole budget is not
/// held.
pub(super) fn put(
    panels: &[Panel],
    reference_core: bool,
    factorization: &Arc<AicFactorization>,
    kernel: &KernelSlot,
    basis: &BasisSlot,
) {
    let entry = Entry {
        reference_core,
        panels: panels.to_vec(),
        factorization: factorization.clone(),
        kernel: kernel.clone(),
        basis: basis.clone(),
    };
    if entry.bytes() > BUDGET_BYTES {
        return;
    }
    let Ok(mut held) = memo().lock() else {
        return;
    };
    let mut total: usize = held.iter().map(Entry::bytes).sum::<usize>() + entry.bytes();
    while total > BUDGET_BYTES && !held.is_empty() {
        total -= held.remove(0).bytes();
    }
    held.push(entry);
}
