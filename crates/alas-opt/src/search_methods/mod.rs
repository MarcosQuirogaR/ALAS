// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one native optimizer kernel: [`lshade_de`], dispatched by
//! [`product_de`]. Every kernel receives normalized bounds and the same
//! scored-candidate contract, so changing the search algorithm cannot
//! silently change the aircraft physics being evaluated.

mod lshade_de;
pub(crate) mod product_de;
pub(crate) mod restoration;
pub(crate) mod rng;

/// How far a candidate got, best first. The order is a ranking rule, not a
/// physical claim: a design that fails a design-vector check is never
/// analysed, so its violation is not comparable with an analysed design's
/// residual table, and a design whose mass/mission fixed point did not close
/// has no trustworthy residual table either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Tier {
    /// Every hard residual met.
    Feasible,
    /// The coupled analysis closed; some hard residual is violated.
    ClosedInfeasible,
    /// The coupled analysis failed or its sizing closure did not converge.
    NotClosed,
    /// Rejected by the design-vector pre-gate; never analysed.
    PreGateFailed,
}

/// Objective and feasibility data attached to one evaluated design.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScoredPoint {
    pub(crate) values: Vec<f64>,
    pub(crate) cost: f64,
    pub(crate) tier: Tier,
    /// Sum of normalized violations inside the tier (dimensionless).
    pub(crate) constraint_violation: f64,
    pub(crate) objectives: [f64; 3],
}

impl ScoredPoint {
    pub(crate) fn valid(&self) -> bool {
        self.tier == Tier::Feasible
    }

    /// Deb's feasibility rule extended by the tiers: tier first, then
    /// normalized violation, then objective.
    pub(crate) fn feasibility_key(&self) -> (Tier, OrderedF64, OrderedF64) {
        (
            self.tier,
            OrderedF64(self.constraint_violation),
            OrderedF64(self.cost),
        )
    }

    /// Force a non-finite score to the bottom of its tier ordering so it can
    /// never be read as a real result.
    pub(crate) fn sanitized(mut self) -> Self {
        if !self.cost.is_finite() || !self.constraint_violation.is_finite() {
            self.tier = self.tier.max(Tier::NotClosed);
            self.constraint_violation = f64::INFINITY;
        }
        self
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

/// Evaluate a whole generation's candidates in one call and return the
/// scores in input order. How the batch is spread across threads is the
/// implementation's business; the scores it returns may not depend on it.
pub(crate) type EvaluateBatch<'a> = dyn FnMut(&[Vec<f64>]) -> Vec<ScoredPoint> + 'a;

/// Cheap design-vector admission check, without any coupled analysis.
pub(crate) type Admission<'a> = dyn Fn(&[f64]) -> bool + 'a;

pub(crate) fn clamp_to_bounds(values: &mut [f64], bounds: &[(f64, f64)]) {
    for (value, &(lower, upper)) in values.iter_mut().zip(bounds) {
        *value = if value.is_finite() {
            value.clamp(lower, upper)
        } else {
            lower
        };
    }
}

/// One stratified sample per row in every free coordinate: `count` points
/// over `bounds`, in physical units. Fixed coordinates keep their value.
pub(crate) fn latin_hypercube(
    bounds: &[(f64, f64)],
    count: usize,
    rng: &mut rng::SearchRng,
) -> Vec<Vec<f64>> {
    let mut points = vec![vec![0.0; bounds.len()]; count];
    for (dimension, &(lower, upper)) in bounds.iter().enumerate() {
        let mut strata: Vec<usize> = (0..count).collect();
        for position in (1..count).rev() {
            strata.swap(position, rng.below(position + 1));
        }
        for (row, stratum) in strata.into_iter().enumerate() {
            let fraction = (stratum as f64 + rng.unit()) / count as f64;
            points[row][dimension] = (lower + fraction * (upper - lower)).clamp(lower, upper);
        }
    }
    points
}

/// Distance between two design vectors with every free coordinate scaled to
/// its bound width, divided by the square root of the free-dimension count
/// so it lies in `[0, 1]` whatever the dimension.
pub(crate) fn normalized_distance(left: &[f64], right: &[f64], bounds: &[(f64, f64)]) -> f64 {
    let mut sum = 0.0;
    let mut free = 0usize;
    for ((a, b), &(lower, upper)) in left.iter().zip(right).zip(bounds) {
        if upper > lower {
            free += 1;
            sum += ((a - b) / (upper - lower)).powi(2);
        }
    }
    if free == 0 {
        0.0
    } else {
        (sum / free as f64).sqrt()
    }
}
