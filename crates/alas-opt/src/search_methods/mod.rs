// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one native optimizer kernel: [`lshade_de`], dispatched by
//! [`product_de`]. Every kernel receives normalized bounds and the same
//! scored-candidate contract, so changing the search algorithm cannot
//! silently change the aircraft physics being evaluated.

mod lshade_de;
pub(crate) mod product_de;

/// Objective and feasibility data attached to one evaluated design.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScoredPoint {
    pub(crate) values: Vec<f64>,
    pub(crate) cost: f64,
    pub(crate) valid: bool,
    pub(crate) constraint_violation: f64,
    pub(crate) objectives: [f64; 3],
}

impl ScoredPoint {
    /// Return a deterministic scalar winner while feasibility takes priority.
    pub(crate) fn feasibility_key(&self) -> (u8, OrderedF64, OrderedF64) {
        (
            u8::from(!self.valid),
            OrderedF64(self.constraint_violation),
            OrderedF64(self.cost),
        )
    }
}

/// Result retained by the common optimization-result adapter.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MethodOutcome {
    pub(crate) winner: ScoredPoint,
    pub(crate) pareto_front: Vec<ScoredPoint>,
}

/// Total-order wrapper used only for deterministic candidate sorting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OrderedF64(pub(crate) f64);

impl Eq for OrderedF64 {}

impl PartialOrd for OrderedF64 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedF64 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// Evaluate a whole generation's candidates in one deterministic call. A
/// batch is evaluated in the input order regardless of how many worker
/// threads the implementation spreads it across, which is what keeps a
/// seeded search result independent of `solver.workers` (see
/// `lshade_de`'s module documentation).
pub(crate) type EvaluateBatch<'a> = dyn FnMut(&[Vec<f64>]) -> Vec<ScoredPoint> + 'a;

pub(crate) fn clamp_to_bounds(values: &mut [f64], bounds: &[(f64, f64)]) {
    for (value, &(lower, upper)) in values.iter_mut().zip(bounds) {
        *value = value.clamp(lower, upper);
    }
}
