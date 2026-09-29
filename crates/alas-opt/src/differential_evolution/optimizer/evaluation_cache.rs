// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Exact, per-search reuse of deterministic full-fidelity evaluations.
//! Reduced screening scores never enter this cache. Keys retain every f64 bit:
//! no quantization, interpolation or surrogate can hide a constraint crossing.

use std::collections::{BTreeMap, BTreeSet};

use crate::search_methods::{product_de, ScoredPoint};

pub(super) struct EvaluationCache {
    scores: BTreeMap<Vec<u64>, ScoredPoint>,
    pub(super) hits: usize,
}

impl EvaluationCache {
    pub(super) fn new(verified: Option<&ScoredPoint>) -> Self {
        let mut cache = Self {
            scores: BTreeMap::new(),
            hits: 0,
        };
        if let Some(point) = verified {
            cache.insert(point.clone());
        }
        cache
    }

    pub(super) fn insert(&mut self, point: ScoredPoint) {
        self.scores.insert(key(&point.values), point);
    }

    pub(super) fn missing(&mut self, points: &[Vec<f64>]) -> Vec<Vec<f64>> {
        let mut scheduled = BTreeSet::new();
        let mut pending = Vec::new();
        for point in points {
            let key = key(point);
            if self.scores.contains_key(&key) || !scheduled.insert(key) {
                self.hits += 1;
            } else {
                pending.push(point.clone());
            }
        }
        pending
    }

    pub(super) fn resolve(&self, points: &[Vec<f64>]) -> Vec<ScoredPoint> {
        points
            .iter()
            .map(|point| {
                self.scores
                    .get(&key(point))
                    .cloned()
                    .unwrap_or_else(|| product_de::unevaluated(point))
            })
            .collect()
    }
}

fn key(point: &[f64]) -> Vec<u64> {
    point.iter().map(|value| value.to_bits()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_scores_reuse_only_exact_designs_and_keep_input_order() {
        let point = ScoredPoint {
            values: vec![0.5],
            cost: 7.0,
            valid: true,
            constraint_violation: 0.0,
            objectives: [7.0; 3],
        };
        let mut cache = EvaluationCache::new(Some(&point));
        let nearby = f64::from_bits(0.5_f64.to_bits() + 1);
        let points = vec![vec![0.5], vec![nearby], vec![nearby], vec![0.5]];
        assert_eq!(cache.missing(&points), vec![vec![nearby]]);
        let other = ScoredPoint {
            values: vec![nearby],
            valid: false,
            ..point.clone()
        };
        cache.insert(other.clone());
        assert_eq!(
            cache.resolve(&points),
            vec![point.clone(), other.clone(), other, point]
        );
        assert_eq!(cache.hits, 3);
    }
}
