// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native refinement starts from full-model pilot anchors and coupled draws.

use super::{screening, BatchEvaluator, PlanformProjection, Run, SearchObjective};

pub(super) fn top_up<E: SearchObjective + ?Sized>(
    run: &Run<'_>,
    evaluator: &BatchEvaluator<'_, E>,
    seeds: &mut Vec<Vec<f64>>,
    population: usize,
) {
    let Some(projection) = run
        .pre_gate
        .and_then(|config| PlanformProjection::new(config, run.bounds))
    else {
        return;
    };
    let needed = population.saturating_sub(seeds.len());
    if needed == 0 {
        return;
    }
    let history = evaluator.objective.history();
    let best = seeds
        .iter()
        .filter_map(|point| {
            let row = evaluator.cache.row(point)?;
            Some((point, row))
        })
        .min_by(|(_, a), (_, b)| {
            let valid = |row: usize| history.valid.get(row).copied().unwrap_or(false);
            let violation = |row: usize| {
                history
                    .hard_violation
                    .get(row)
                    .copied()
                    .unwrap_or(f64::INFINITY)
            };
            let cost = |row: usize| history.cost.get(row).copied().unwrap_or(f64::INFINITY);
            valid(*b)
                .cmp(&valid(*a))
                .then(violation(*a).total_cmp(&violation(*b)))
                .then(cost(*a).total_cmp(&cost(*b)))
                .then(a.cmp(b))
        })
        .map(|(point, _)| point.clone());
    let anchor = best.or_else(|| run.baseline.clone());
    let points = screening::sample(
        run.bounds,
        anchor.as_deref(),
        population * 2 + 1,
        run.seed.wrapping_add(2),
        Some(&projection),
    );
    for point in points.into_iter().skip(1) {
        if !seeds.contains(&point) {
            seeds.push(point);
        }
        if seeds.len() >= population {
            break;
        }
    }
}
