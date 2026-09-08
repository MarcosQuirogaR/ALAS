// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


impl TrialState<'_> {
    fn apply(&mut self, index: usize, trial: Vec<f64>, trial_cost: f64, trial_valid: bool) {
        if candidate_is_at_least_as_good(
            trial_cost,
            trial_valid,
            self.costs[index],
            self.validities[index],
            self.feasibility_first,
        ) {
            self.population[index] = trial;
            self.costs[index] = trial_cost;
            self.validities[index] = trial_valid;
            if candidate_is_better(
                trial_cost,
                trial_valid,
                self.costs[0],
                self.validities[0],
                self.feasibility_first,
            ) {
                self.population.swap(0, index);
                self.costs.swap(0, index);
                self.validities.swap(0, index);
            }
        }
    }
}

fn promote_best(
    population: &mut [Vec<f64>],
    costs: &mut [f64],
    validities: &mut [bool],
    feasibility_first: bool,
) {
    let Some((best_index, _)) =
        costs
            .iter()
            .enumerate()
            .min_by(|(left, left_cost), (right, right_cost)| {
                if candidate_is_better(
                    **left_cost,
                    validities[*left],
                    **right_cost,
                    validities[*right],
                    feasibility_first,
                ) {
                    std::cmp::Ordering::Less
                } else if candidate_is_better(
                    **right_cost,
                    validities[*right],
                    **left_cost,
                    validities[*left],
                    feasibility_first,
                ) {
                    std::cmp::Ordering::Greater
                } else {
                    left.cmp(right)
                }
            })
    else {
        return;
    };
    population.swap(0, best_index);
    costs.swap(0, best_index);
    validities.swap(0, best_index);
}

fn latin_hypercube_population(
    bounds: &[(f64, f64)],
    population_size: usize,
    rng: &mut RandomState,
) -> Vec<Vec<f64>> {
    let mut samples = vec![vec![0.0; bounds.len()]; population_size];
    let segment_size = 1.0 / population_size as f64;

    for (row, sample) in samples.iter_mut().enumerate() {
        for value in sample {
            *value = (row as f64 + rng.uniform(0.0, 1.0)) * segment_size;
        }
    }

    for (dimension, _) in bounds.iter().enumerate() {
        let mut order: Vec<usize> = (0..population_size).collect();
        shuffle(&mut order, rng);
        let column: Vec<f64> = order.iter().map(|&row| samples[row][dimension]).collect();
        for (row, value) in column.into_iter().enumerate() {
            samples[row][dimension] = value;
        }
    }

    samples
        .into_iter()
        .map(|sample| {
            sample
                .into_iter()
                .zip(bounds)
                .map(|(normalized, &(lo, hi))| {
                    // Keep the generated point closed over the configured
                    // interval even when a floating-point product rounds one
                    // ulp past an exact endpoint.
                    (lo + normalized * (hi - lo)).clamp(lo, hi)
                })
                .collect()
        })
        .collect()
}

fn shuffle<T>(values: &mut [T], rng: &mut RandomState) {
    for position in (1..values.len()).rev() {
        let swap_with = rng.randint(position + 1);
        values.swap(position, swap_with);
    }
}

fn select_samples(
    candidate: usize,
    number: usize,
    indices: &mut [usize],
    rng: &mut RandomState,
) -> Vec<usize> {
    shuffle(indices, rng);

    // Match SciPy's shuffled target-removal order.
    indices
        .iter()
        .copied()
        .take(number + 1)
        .filter(|&index| index != candidate)
        .take(number)
        .collect()
}

struct TrialInputs<'a> {
    population: &'a [Vec<f64>],
    bounds: &'a [(f64, f64)],
    strategy: &'a str,
    f_weight: f64,
    crossover_probability: f64,
    sample_indices: &'a mut [usize],
    rng: &'a mut RandomState,
}

fn trial_vector(candidate: usize, inputs: &mut TrialInputs<'_>) -> Vec<f64> {
    let n_dof = inputs.bounds.len();
    let fill_point = inputs.rng.randint(n_dof);
    let samples = select_samples(candidate, 5, inputs.sample_indices, inputs.rng);
    let best = &inputs.population[0];
    let current = &inputs.population[candidate];
    let vector = |index: usize| -> &Vec<f64> { &inputs.population[samples[index]] };

    let mut mutant = vec![0.0; n_dof];
    for d in 0..n_dof {
        mutant[d] = match inputs.strategy {
            "rand1bin" | "rand1exp" => {
                vector(0)[d] + inputs.f_weight * (vector(1)[d] - vector(2)[d])
            }
            "best2bin" | "best2exp" => {
                best[d]
                    + inputs.f_weight * (vector(0)[d] + vector(1)[d] - vector(2)[d] - vector(3)[d])
            }
            "rand2bin" | "rand2exp" => {
                vector(0)[d]
                    + inputs.f_weight * (vector(1)[d] + vector(2)[d] - vector(3)[d] - vector(4)[d])
            }
            "randtobest1bin" | "randtobest1exp" => {
                vector(0)[d]
                    + inputs.f_weight * (best[d] - vector(0)[d])
                    + inputs.f_weight * (vector(1)[d] - vector(2)[d])
            }
            "currenttobest1bin" | "currenttobest1exp" => {
                current[d] + inputs.f_weight * (best[d] - current[d] + vector(0)[d] - vector(1)[d])
            }
            _ => best[d] + inputs.f_weight * (vector(0)[d] - vector(1)[d]),
        };
    }

    let is_exponential = inputs.strategy.ends_with("exp");
    let mut crossovers = Vec::with_capacity(n_dof);
    for _ in 0..n_dof {
        crossovers.push(inputs.rng.uniform(0.0, 1.0) < inputs.crossover_probability);
    }

    let mut trial = current.clone();
    if is_exponential {
        crossovers[0] = true;
        let mut index = 0;
        let mut destination = fill_point;
        while index < n_dof && crossovers[index] {
            trial[destination] = mutant[destination];
            destination = (destination + 1) % n_dof;
            index += 1;
        }
    } else {
        crossovers[fill_point] = true;
        for d in 0..n_dof {
            if crossovers[d] {
                trial[d] = mutant[d];
            }
        }
    }

    // SciPy resamples out-of-range coordinates instead of clamping them.
    for (value, &(lo, hi)) in trial.iter_mut().zip(inputs.bounds) {
        if !value.is_finite() || *value < lo || *value > hi {
            *value = inputs.rng.uniform(lo, hi).clamp(lo, hi);
        }
    }
    trial
}

fn converged(costs: &[f64], tolerance: f64) -> bool {
    if costs.iter().any(|cost| !cost.is_finite()) || costs.is_empty() {
        return false;
    }
    let mean = costs.iter().sum::<f64>() / costs.len() as f64;
    let variance = costs
        .iter()
        .map(|cost| {
            let delta = cost - mean;
            delta * delta
        })
        .sum::<f64>()
        / costs.len() as f64;
    variance.sqrt() <= tolerance * mean.abs()
}

#[cfg(test)]
#[path = "../differential_evolution_tests.rs"]
mod tests;

