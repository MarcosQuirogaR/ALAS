// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Ranking consistency for a declared subset of paired candidates.

use alas_opt::{CandidateScore, FidelityPair};
use serde_json::{json, Value};

/// The search's ranking key: tier, normalized violation, then objective.
fn key(score: &CandidateScore) -> (u8, f64, f64) {
    let tier = if score.feasible {
        0
    } else if score.analysed {
        1
    } else {
        2
    };
    let violation = if score.feasible {
        0.0
    } else {
        score.hard_violation
    };
    (tier, violation, score.cost)
}

fn order(pairs: &[&FidelityPair], pick: impl Fn(&FidelityPair) -> &CandidateScore) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..pairs.len()).collect();
    indices.sort_by(|&a, &b| {
        let (ka, kb) = (key(pick(pairs[a])), key(pick(pairs[b])));
        ka.0.cmp(&kb.0)
            .then(ka.1.total_cmp(&kb.1))
            .then(ka.2.total_cmp(&kb.2))
            .then(a.cmp(&b))
    });
    indices
}

/// Report tied ranks and top-k sets with the subset size and cutoff ties.
pub(super) fn statistics(pairs: &[&FidelityPair], reference_top_k: usize) -> Value {
    let screening_order = order(pairs, |pair| &pair.screening);
    let full_order = order(pairs, |pair| &pair.full);
    let position = |order: &[usize], pick: &dyn Fn(&FidelityPair) -> &CandidateScore| {
        let mut rank = vec![0.0; order.len()];
        let mut first = 0;
        while first < order.len() {
            let ranking_key = key(pick(pairs[order[first]]));
            let mut end = first + 1;
            while end < order.len() && key(pick(pairs[order[end]])) == ranking_key {
                end += 1;
            }
            let average = (first + end - 1) as f64 / 2.0;
            for &index in &order[first..end] {
                rank[index] = average;
            }
            first = end;
        }
        rank
    };
    let overlap = |k: usize| {
        let k = k.min(pairs.len());
        let top = &screening_order[..k];
        full_order[..k]
            .iter()
            .filter(|index| top.contains(index))
            .count()
    };
    let fraction = |k: usize| match k.min(pairs.len()) {
        0 => None,
        count => Some(overlap(k) as f64 / count as f64),
    };
    let cutoff_ties =
        |order: &[usize], pick: &dyn Fn(&FidelityPair) -> &CandidateScore, k: usize| {
            let Some(&cutoff) = order.get(k.min(order.len()).saturating_sub(1)) else {
                return 0;
            };
            let ranking_key = key(pick(pairs[cutoff]));
            order
                .iter()
                .filter(|&&index| key(pick(pairs[index])) == ranking_key)
                .count()
        };
    json!({
        "n": pairs.len(),
        "ranking_key_spearman_rho": super::spearman(
            &position(&screening_order, &|pair| &pair.screening),
            &position(&full_order, &|pair| &pair.full)),
        "top_k_overlap": {
            "k_reference": reference_top_k.min(pairs.len()),
            "overlap_reference": overlap(reference_top_k),
            "fraction_reference": fraction(reference_top_k),
            "k_10": 10.min(pairs.len()),
            "overlap_10": overlap(10),
            "fraction_10": fraction(10),
            "cutoff_ties_10_screening": cutoff_ties(&screening_order, &|pair| &pair.screening, 10),
            "cutoff_ties_10_full": cutoff_ties(&full_order, &|pair| &pair.full, 10),
        },
    })
}
