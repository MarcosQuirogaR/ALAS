// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The elite the screening stage hands to the refinement: the best
//! candidates by the feasibility rule, thinned so no two lie closer than a
//! niche radius, then topped up by greedy max-min distance picks from the
//! best fraction of the sample.
//!
//! Ranking is [`ScoredPoint::feasibility_key`], so when nothing screened is
//! feasible the least-infeasible candidates (tier first, then normalized
//! violation) lead the elite. The niche test is clearing in the sense of
//! A. Petrowski, "A Clearing Procedure as a Niching Method for Genetic
//! Algorithms," IEEE ICEC 1996, DOI 10.1109/ICEC.1996.542703; the max-min
//! top-up is the greedy farthest-point rule. [`NICHE_RADIUS`] and
//! [`TOP_FRACTION`] are engineering choices, not tuned values.

use crate::search_methods::{normalized_distance, ScoredPoint};

/// Smallest normalized distance (bound-width-scaled RMS over free
/// coordinates) between two elite members: 5 % of the box.
pub(crate) const NICHE_RADIUS: f64 = 0.05;
/// Fraction of the ranked sample the max-min top-up may draw from.
pub(crate) const TOP_FRACTION: f64 = 0.25;

/// Up to `size` members of `scored`, best first by the feasibility rule for
/// the first half, farthest-first for the rest, all at least
/// [`NICHE_RADIUS`] apart. `excluded` points (the baseline, which the
/// refinement always seeds itself) are never picked and count as already
/// selected for the distance test.
pub(crate) fn select(
    scored: &[ScoredPoint],
    bounds: &[(f64, f64)],
    size: usize,
    excluded: &[Vec<f64>],
) -> Vec<ScoredPoint> {
    let mut ranked: Vec<usize> = (0..scored.len()).collect();
    ranked.sort_by_key(|&index| (scored[index].feasibility_key(), index));
    let mut chosen: Vec<usize> = Vec::new();
    let distance_to_chosen = |candidate: usize, chosen: &[usize]| {
        chosen
            .iter()
            .map(|&other| &scored[other].values)
            .chain(excluded)
            .map(|other| normalized_distance(&scored[candidate].values, other, bounds))
            .fold(f64::INFINITY, f64::min)
    };

    let best_count = size.div_ceil(2);
    for &candidate in &ranked {
        if chosen.len() >= best_count {
            break;
        }
        if distance_to_chosen(candidate, &chosen) >= NICHE_RADIUS {
            chosen.push(candidate);
        }
    }

    let pool_len = ((scored.len() as f64 * TOP_FRACTION).ceil() as usize)
        .max(2 * size)
        .min(ranked.len());
    while chosen.len() < size {
        let farthest = ranked[..pool_len]
            .iter()
            .copied()
            .filter(|candidate| !chosen.contains(candidate))
            .map(|candidate| (distance_to_chosen(candidate, &chosen), candidate))
            .filter(|&(distance, _)| distance >= NICHE_RADIUS)
            .max_by(|left, right| left.0.total_cmp(&right.0).then(right.1.cmp(&left.1)));
        let Some((_, candidate)) = farthest else {
            break;
        };
        chosen.push(candidate);
    }
    chosen
        .into_iter()
        .map(|index| scored[index].clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search_methods::Tier;

    fn point(values: [f64; 2], cost: f64, tier: Tier, violation: f64) -> ScoredPoint {
        ScoredPoint {
            values: values.to_vec(),
            cost,
            tier,
            constraint_violation: violation,
            objectives: [cost; 3],
        }
    }

    fn min_pairwise(elite: &[ScoredPoint], bounds: &[(f64, f64)]) -> f64 {
        let mut minimum = f64::INFINITY;
        for (i, a) in elite.iter().enumerate() {
            for b in &elite[i + 1..] {
                minimum = minimum.min(normalized_distance(&a.values, &b.values, bounds));
            }
        }
        minimum
    }

    #[test]
    fn the_elite_keeps_the_best_and_spreads_the_rest_beyond_the_niche_radius() {
        let bounds = [(0.0, 1.0), (0.0, 1.0)];
        // A tight cluster of the ten cheapest feasible designs near the
        // origin, and a spread of worse feasible designs elsewhere.
        let mut scored: Vec<ScoredPoint> = (0..10)
            .map(|i| point([0.01 * i as f64, 0.0], i as f64, Tier::Feasible, 0.0))
            .collect();
        for i in 0..40 {
            let x = (i % 7) as f64 / 6.0;
            let y = (i / 7) as f64 / 6.0;
            scored.push(point([x, y], 100.0 + i as f64, Tier::Feasible, 0.0));
        }
        let elite = select(&scored, &bounds, 6, &[]);
        // Every member comes from the best quarter of the sample (13 of 50
        // here: the cluster and the three cheapest grid points), and the
        // niche radius leaves room for four of them.
        assert_eq!(elite.len(), 4);
        assert!(elite.iter().all(|member| member.cost < 103.0));
        assert_eq!(elite[0].cost, 0.0, "the best candidate always leads");
        assert!(min_pairwise(&elite, &bounds) >= NICHE_RADIUS);
        // The cluster yields at most two members at 5 % spacing; the rest
        // come from elsewhere in the box.
        let from_cluster = elite.iter().filter(|p| p.cost < 10.0).count();
        assert!(from_cluster <= 2, "{from_cluster}");
    }

    #[test]
    fn with_nothing_feasible_the_least_infeasible_lead_the_elite() {
        let bounds = [(0.0, 1.0), (0.0, 1.0)];
        let scored = vec![
            point([0.9, 0.9], 1.0, Tier::PreGateFailed, 0.01),
            point([0.1, 0.1], 5.0, Tier::ClosedInfeasible, 0.30),
            point([0.5, 0.5], 9.0, Tier::ClosedInfeasible, 0.02),
            point([0.3, 0.8], 0.5, Tier::NotClosed, 0.00),
        ];
        let elite = select(&scored, &bounds, 3, &[]);
        assert_eq!(elite[0].values, vec![0.5, 0.5]);
        assert_eq!(elite[1].values, vec![0.1, 0.1]);
    }

    #[test]
    fn an_excluded_baseline_is_never_picked_and_repels_its_neighbours() {
        let bounds = [(0.0, 1.0)];
        let scored = vec![
            ScoredPoint {
                values: vec![0.5],
                ..point([0.0, 0.0], 0.0, Tier::Feasible, 0.0)
            },
            ScoredPoint {
                values: vec![0.51],
                ..point([0.0, 0.0], 1.0, Tier::Feasible, 0.0)
            },
            ScoredPoint {
                values: vec![0.9],
                ..point([0.0, 0.0], 2.0, Tier::Feasible, 0.0)
            },
        ];
        let elite = select(&scored, &bounds, 2, &[vec![0.5]]);
        assert_eq!(elite.len(), 1);
        assert_eq!(elite[0].values, vec![0.9]);
    }
}
