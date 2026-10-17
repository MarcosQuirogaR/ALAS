// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compatibility-aware differential evolution and the random baseline.
//!
//! The differential-evolution genes are the propulsion bundle (one unit
//! value selects a compatible motor/ESC/pack/propeller combination from the
//! list [`super::compat::propulsion_bundles`] produced), five unit values for
//! the discrete auxiliary components, and six unit values for the continuous
//! geometry and tail volumes. Every draw is therefore compatible by
//! construction, and no evaluation is spent on an electrically impossible
//! bundle.
//!
//! The random baseline is the previous search, kept for comparison: it draws
//! nine independent catalogue indices, so most of its evaluations are
//! incompatible bundles. Both use the same objective and the same budget.
//!
//! Determinism: the only random source is [`SplitMix64`] seeded with
//! `problem.base.seed`; the objective is a pure function of the draw. Two runs
//! with the same inputs return the same best candidate. Timing fields differ
//! between runs and are excluded from that claim.

use std::time::Instant;

use super::compat::propulsion_bundles;
use super::competition::{
    compile, evaluate_draw, materialise, CompetitionEvaluation, CompetitionProblem,
    CompiledObjective, Draw, Fitness,
};
use super::competition_progress::{CompetitionProgress, Tracker};
use super::generation::{CandidateSample, Choices};
use super::search::SplitMix64;
use super::{OptimizationError, SelectedComponents};
use crate::catalog::ComponentRecord;
use crate::{UavDesign, UavReport};

use super::GeneratedGeometry;

/// Dimension of the differential-evolution genome.
const DIMENSION: usize = 12;
const GENE_BUNDLE: usize = 0;
const GENE_SERVO: usize = 1;
const GENE_MATERIAL: usize = 2;
const GENE_RECEIVER: usize = 3;
const GENE_ELECTRONICS: usize = 4;
const GENE_LANDING_GEAR: usize = 5;
const GENE_WING_AREA: usize = 6;
const GENE_ASPECT_RATIO: usize = 7;
const GENE_FUSELAGE_LENGTH: usize = 8;
const GENE_LEADING_EDGE: usize = 9;
const GENE_HORIZONTAL_TAIL: usize = 10;
const GENE_VERTICAL_TAIL: usize = 11;

/// Differential weight F of DE/rand/1/bin (standard value, not tuned here).
const DIFFERENTIAL_WEIGHT: f64 = 0.6;
/// Binomial crossover probability CR.
const CROSSOVER_PROBABILITY: f64 = 0.9;
/// Default population size.
const DEFAULT_POPULATION: usize = 24;
/// Smallest budget that allows a population of four distinct parents.
const MINIMUM_BUDGET: usize = 4;

/// Which search produced a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMethod {
    /// Differential evolution over compatible bundles and continuous genes.
    DifferentialEvolution,
    /// Uniform draws over the raw catalogue indices (the previous search).
    RandomCatalogueIndices,
    /// Uniform draws over the compatible bundles and continuous genes: the
    /// same space as the differential evolution, without the population.
    RandomCompatibleBundles,
}

/// Continuous airframe variables of a candidate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DesignVariables {
    /// Wing reference area, m^2.
    pub wing_area_m2: f64,
    /// Wing aspect ratio.
    pub wing_aspect_ratio: f64,
    /// Fuselage length, m.
    pub fuselage_length_m: f64,
    /// Wing leading-edge location as a fraction of fuselage length.
    pub wing_leading_edge_fraction: f64,
    /// Horizontal-tail volume coefficient.
    pub horizontal_tail_volume: f64,
    /// Vertical-tail volume coefficient.
    pub vertical_tail_volume: f64,
}

/// The best candidate of a search, with its reviewable artefacts.
#[derive(Debug, Clone, PartialEq)]
pub struct CompetitionCandidate {
    /// Continuous variables.
    pub variables: DesignVariables,
    /// Objective evaluation of this candidate.
    pub evaluation: CompetitionEvaluation,
    /// Generated airframe and placements.
    pub design: UavDesign,
    /// Feasibility report (preliminary physics and evidence).
    pub report: UavReport,
    /// Generated geometry.
    pub geometry: GeneratedGeometry,
    /// Selected catalogue identifiers.
    pub components: SelectedComponents,
}

/// Result of one competition search.
#[derive(Debug, Clone)]
pub struct CompetitionOptimum {
    /// Search method that produced this result.
    pub method: SearchMethod,
    /// Seed used by the search.
    pub seed: u64,
    /// Objective evaluations spent (equal to the budget).
    pub evaluations: usize,
    /// Evaluations that were feasible.
    pub feasible_evaluations: usize,
    /// Compatible propulsion bundles in the catalogue.
    pub compatible_bundles: usize,
    /// Incompatible propulsion bundles in the catalogue.
    pub rejected_bundles: usize,
    /// Best candidate by the Deb ordering. Check `evaluation.feasible`.
    pub best: Option<CompetitionCandidate>,
    /// Wall-clock time of the search, s (not reproducible).
    pub elapsed_s: f64,
    /// Objective evaluations per second, wall clock (not reproducible).
    pub evaluations_per_s: f64,
}

/// Resolved indices of one compatible bundle in the optimizer's family lists.
struct ResolvedBundle {
    motor: usize,
    esc: usize,
    battery: usize,
    propeller: usize,
}

struct Context<'a> {
    objective: CompiledObjective,
    choices: Choices<'a>,
    bundles: Vec<ResolvedBundle>,
    compatible_bundles: usize,
    rejected_bundles: usize,
}

/// Run the compatibility-aware differential evolution.
///
/// # Errors
///
/// [`OptimizationError::InvalidProblem`] when the budget is below four, a
/// component family is empty, no bundle is compatible, or the preset cannot be
/// compiled.
pub fn optimize_competition(
    problem: &CompetitionProblem<'_>,
) -> Result<CompetitionOptimum, OptimizationError> {
    run(problem, SearchMethod::DifferentialEvolution, None)
}

/// Run the differential evolution and report progress after every evaluation.
/// The observer returns `false` to cancel; the search then returns
/// [`OptimizationError::Cancelled`] at the next evaluation boundary.
///
/// # Errors
///
/// As [`optimize_competition`], plus cancellation.
pub fn optimize_competition_observed(
    problem: &CompetitionProblem<'_>,
    observer: &mut dyn FnMut(CompetitionProgress) -> bool,
) -> Result<CompetitionOptimum, OptimizationError> {
    run(problem, SearchMethod::DifferentialEvolution, Some(observer))
}

/// Run the previous random search (nine independent catalogue indices) with
/// the same objective and budget, for comparison.
///
/// # Errors
///
/// As [`optimize_competition`].
pub fn optimize_competition_random(
    problem: &CompetitionProblem<'_>,
) -> Result<CompetitionOptimum, OptimizationError> {
    run(problem, SearchMethod::RandomCatalogueIndices, None)
}

/// Run uniform random draws over the compatible space with the same objective
/// and budget. The gap to [`optimize_competition`] is the population effect;
/// the gap to [`optimize_competition_random`] is the compatibility effect.
///
/// # Errors
///
/// As [`optimize_competition`].
pub fn optimize_competition_random_compatible(
    problem: &CompetitionProblem<'_>,
) -> Result<CompetitionOptimum, OptimizationError> {
    run(problem, SearchMethod::RandomCompatibleBundles, None)
}

fn run(
    problem: &CompetitionProblem<'_>,
    method: SearchMethod,
    observer: Option<&mut dyn FnMut(CompetitionProgress) -> bool>,
) -> Result<CompetitionOptimum, OptimizationError> {
    let started = Instant::now();
    let context = setup(problem)?;
    let budget = problem.base.evaluations;
    let mut rng = SplitMix64::new(problem.base.seed);
    let mut tracker = Tracker {
        budget,
        observer,
        ..Tracker::default()
    };
    match method {
        SearchMethod::DifferentialEvolution => {
            differential_evolution(problem, &context, &mut rng, budget, &mut tracker);
        }
        SearchMethod::RandomCatalogueIndices => {
            for _ in 0..budget {
                if tracker.cancelled {
                    break;
                }
                let draw = random_draw(&context, problem, &mut rng);
                let outcome = evaluate_draw(problem, &context.objective, &context.choices, draw);
                tracker.observe(draw, outcome);
            }
        }
        SearchMethod::RandomCompatibleBundles => {
            for _ in 0..budget {
                if tracker.cancelled {
                    break;
                }
                let genes = random_genes(&mut rng);
                let draw = decode(&context, problem, &genes);
                let outcome = evaluate_draw(problem, &context.objective, &context.choices, draw);
                tracker.observe(draw, outcome);
            }
        }
    }
    if tracker.cancelled {
        return Err(OptimizationError::Cancelled {
            evaluated_candidates: tracker.evaluations,
        });
    }
    let elapsed_s = started.elapsed().as_secs_f64();
    // None when every draw was rejected before a design existed to report.
    let best = tracker
        .best
        .take()
        .and_then(|best| candidate(problem, &context, best.draw, best.evaluation));
    Ok(CompetitionOptimum {
        method,
        seed: problem.base.seed,
        evaluations: tracker.evaluations,
        feasible_evaluations: tracker.feasible,
        compatible_bundles: context.compatible_bundles,
        rejected_bundles: context.rejected_bundles,
        best,
        elapsed_s,
        evaluations_per_s: if elapsed_s > 0.0 {
            tracker.evaluations as f64 / elapsed_s
        } else {
            f64::INFINITY
        },
    })
}

fn setup<'a>(problem: &CompetitionProblem<'a>) -> Result<Context<'a>, OptimizationError> {
    if problem.base.evaluations < MINIMUM_BUDGET {
        return Err(OptimizationError::InvalidProblem(format!(
            "competition search needs at least {MINIMUM_BUDGET} evaluations"
        )));
    }
    let objective = compile(problem)?;
    let choices = Choices::from_catalog(problem.base.catalog);
    for (length, family) in [
        (choices.batteries.len(), "battery"),
        (choices.motors.len(), "motor"),
        (choices.escs.len(), "ESC"),
        (choices.propellers.len(), "propeller"),
        (choices.servos.len(), "servo"),
        (choices.materials.len(), "material"),
        (choices.receivers.len(), "receiver"),
        (choices.electronics.len(), "electronics"),
        (choices.landing_gear.len(), "landing-gear"),
    ] {
        if length == 0 {
            return Err(OptimizationError::InvalidProblem(format!(
                "catalogue has no {family} record"
            )));
        }
    }
    let catalogue = propulsion_bundles(problem.base.catalog, problem.base.propulsion_maps);
    let mut bundles = Vec::with_capacity(catalogue.compatible.len());
    for ids in &catalogue.compatible {
        bundles.push(ResolvedBundle {
            motor: position(&choices.motors, &ids.motor_id)?,
            esc: position(&choices.escs, &ids.esc_id)?,
            battery: position(&choices.batteries, &ids.battery_id)?,
            propeller: position(&choices.propellers, &ids.propeller_id)?,
        });
    }
    if bundles.is_empty() {
        return Err(OptimizationError::InvalidProblem(
            "no propulsion bundle passes the compatibility rules".to_owned(),
        ));
    }
    Ok(Context {
        objective,
        choices,
        bundles,
        compatible_bundles: catalogue.compatible.len(),
        rejected_bundles: catalogue.rejected.len(),
    })
}

fn position(records: &[&ComponentRecord], id: &str) -> Result<usize, OptimizationError> {
    records
        .iter()
        .position(|record| record.id == id)
        .ok_or_else(|| {
            OptimizationError::InvalidProblem(format!("record '{id}' is not in the catalogue"))
        })
}

/// Map a unit value to an index of a list of `length` entries.
fn pick(unit: f64, length: usize) -> usize {
    let scaled = (unit.clamp(0.0, 1.0) * length as f64).floor() as usize;
    scaled.min(length.saturating_sub(1))
}

fn decode(
    context: &Context<'_>,
    problem: &CompetitionProblem<'_>,
    genes: &[f64; DIMENSION],
) -> Draw {
    let bundle = &context.bundles[pick(genes[GENE_BUNDLE], context.bundles.len())];
    let choices = &context.choices;
    let bounds = problem.base.geometry_bounds;
    let tails = problem.tail_volume_bounds;
    Draw {
        sample: CandidateSample {
            battery_index: bundle.battery,
            motor_index: bundle.motor,
            esc_index: bundle.esc,
            propeller_index: bundle.propeller,
            servo_index: pick(genes[GENE_SERVO], choices.servos.len()),
            material_index: pick(genes[GENE_MATERIAL], choices.materials.len()),
            receiver_index: pick(genes[GENE_RECEIVER], choices.receivers.len()),
            electronics_index: pick(genes[GENE_ELECTRONICS], choices.electronics.len()),
            landing_gear_index: pick(genes[GENE_LANDING_GEAR], choices.landing_gear.len()),
            wing_area_m2: bounds.wing_area_m2.sample(genes[GENE_WING_AREA]),
            wing_aspect_ratio: bounds.wing_aspect_ratio.sample(genes[GENE_ASPECT_RATIO]),
            fuselage_length_m: bounds.fuselage_length_m.sample(genes[GENE_FUSELAGE_LENGTH]),
            wing_leading_edge_fraction: bounds
                .wing_leading_edge_fraction
                .sample(genes[GENE_LEADING_EDGE]),
        },
        horizontal_tail_volume: tails.horizontal.sample(genes[GENE_HORIZONTAL_TAIL]),
        vertical_tail_volume: tails.vertical.sample(genes[GENE_VERTICAL_TAIL]),
    }
}

fn random_draw(
    context: &Context<'_>,
    problem: &CompetitionProblem<'_>,
    rng: &mut SplitMix64,
) -> Draw {
    let choices = &context.choices;
    let bounds = problem.base.geometry_bounds;
    let tails = problem.tail_volume_bounds;
    let sample = CandidateSample {
        battery_index: rng.index(choices.batteries.len()),
        motor_index: rng.index(choices.motors.len()),
        esc_index: rng.index(choices.escs.len()),
        propeller_index: rng.index(choices.propellers.len()),
        servo_index: rng.index(choices.servos.len()),
        material_index: rng.index(choices.materials.len()),
        receiver_index: rng.index(choices.receivers.len()),
        electronics_index: rng.index(choices.electronics.len()),
        landing_gear_index: rng.index(choices.landing_gear.len()),
        wing_area_m2: bounds.wing_area_m2.sample(rng.unit()),
        wing_aspect_ratio: bounds.wing_aspect_ratio.sample(rng.unit()),
        fuselage_length_m: bounds.fuselage_length_m.sample(rng.unit()),
        wing_leading_edge_fraction: bounds.wing_leading_edge_fraction.sample(rng.unit()),
    };
    let horizontal_tail_volume = tails.horizontal.sample(rng.unit());
    let vertical_tail_volume = tails.vertical.sample(rng.unit());
    Draw {
        sample,
        horizontal_tail_volume,
        vertical_tail_volume,
    }
}

fn random_genes(rng: &mut SplitMix64) -> [f64; DIMENSION] {
    let mut genes = [0.0; DIMENSION];
    for gene in &mut genes {
        *gene = rng.unit();
    }
    genes
}

/// Index of a parent distinct from every index in `excluded`.
fn distinct_parent(rng: &mut SplitMix64, population: usize, excluded: &[usize]) -> usize {
    loop {
        let candidate = rng.index(population);
        if !excluded.contains(&candidate) {
            return candidate;
        }
    }
}

fn differential_evolution(
    problem: &CompetitionProblem<'_>,
    context: &Context<'_>,
    rng: &mut SplitMix64,
    budget: usize,
    tracker: &mut Tracker<'_>,
) {
    let population = problem
        .population
        .unwrap_or(DEFAULT_POPULATION)
        .clamp(MINIMUM_BUDGET, budget);
    let mut genes: Vec<[f64; DIMENSION]> = (0..population).map(|_| random_genes(rng)).collect();
    let mut fitness: Vec<Fitness> = Vec::with_capacity(population);
    for member in &genes {
        if tracker.cancelled {
            return;
        }
        let draw = decode(context, problem, member);
        let outcome = evaluate_draw(problem, &context.objective, &context.choices, draw);
        fitness.push(tracker.observe(draw, outcome));
    }
    while tracker.evaluations < budget && !tracker.cancelled {
        for index in 0..population {
            if tracker.evaluations >= budget || tracker.cancelled {
                break;
            }
            let first = distinct_parent(rng, population, &[index]);
            let second = distinct_parent(rng, population, &[index, first]);
            let third = distinct_parent(rng, population, &[index, first, second]);
            let forced = rng.index(DIMENSION);
            let mut trial = genes[index];
            for (position, gene) in trial.iter_mut().enumerate() {
                if position == forced || rng.unit() < CROSSOVER_PROBABILITY {
                    let mutant = genes[first][position]
                        + DIFFERENTIAL_WEIGHT * (genes[second][position] - genes[third][position]);
                    *gene = mutant.clamp(0.0, 1.0);
                }
            }
            let draw = decode(context, problem, &trial);
            let outcome = evaluate_draw(problem, &context.objective, &context.choices, draw);
            let trial_fitness = tracker.observe(draw, outcome);
            if trial_fitness.no_worse_than(fitness[index]) {
                genes[index] = trial;
                fitness[index] = trial_fitness;
            }
        }
    }
}

fn candidate(
    problem: &CompetitionProblem<'_>,
    context: &Context<'_>,
    draw: Draw,
    evaluation: CompetitionEvaluation,
) -> Option<CompetitionCandidate> {
    let artefacts = materialise(problem, &context.choices, draw)?;
    let sample = draw.sample;
    Some(CompetitionCandidate {
        variables: DesignVariables {
            wing_area_m2: sample.wing_area_m2,
            wing_aspect_ratio: sample.wing_aspect_ratio,
            fuselage_length_m: sample.fuselage_length_m,
            wing_leading_edge_fraction: sample.wing_leading_edge_fraction,
            horizontal_tail_volume: draw.horizontal_tail_volume,
            vertical_tail_volume: draw.vertical_tail_volume,
        },
        evaluation,
        design: artefacts.design,
        report: artefacts.report,
        geometry: artefacts.geometry,
        components: artefacts.components,
    })
}
