// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native optimizer kernels that share the product objective contract.
//!
//! Frozen Python parity remains in `differential_evolution`; these methods are
//! explicit product alternatives. Every kernel receives normalized bounds and
//! the same scored-candidate callback, so changing the search algorithm cannot
//! silently change the aircraft physics being evaluated.

// The saved `optimizer.solver.method` names that once selected these
// population methods still load, but no longer choose a distinct search
// algorithm (see `differential_evolution_tests`); the methods and their
// shared helpers are kept, with their own tests, until the dispatch is
// either rebuilt or the modules are retired outright.
#![allow(dead_code)]

mod cma_es;
mod constrained_de;
mod nsga2;
mod turbo;

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

pub(crate) type EvaluatePoint<'a> = dyn FnMut(&[f64]) -> ScoredPoint + 'a;

pub(crate) fn clamp_to_bounds(values: &mut [f64], bounds: &[(f64, f64)]) {
    for (value, &(lower, upper)) in values.iter_mut().zip(bounds) {
        *value = value.clamp(lower, upper);
    }
}

pub(crate) fn normalized_distance_squared(
    left: &[f64],
    right: &[f64],
    bounds: &[(f64, f64)],
) -> f64 {
    left.iter()
        .zip(right)
        .zip(bounds)
        .map(|((&a, &b), &(lower, upper))| {
            let width = (upper - lower).max(f64::EPSILON);
            ((a - b) / width).powi(2)
        })
        .sum()
}
