// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Competition-score objective for the compatibility-aware search.
//!
//! One evaluation generates the airframe and component selection, checks the
//! preliminary feasibility rules, flies the preset's mission segments with a
//! powertrain built from the selected thrust-at-speed map, and evaluates the
//! preset's score engine. Hard rules of the preset, the take-off distance
//! against the announcement, and the preliminary structural and electrical
//! checks form the constraint set. Infeasible candidates carry a violation
//! magnitude used by the Deb ordering in the search.
//!
//! Assumptions that the catalogue does not supply are named constants below;
//! they are engineering choices, not source values.

use std::collections::BTreeMap;

use crate::coupled::assess::mission_config_for;
use crate::optimizer::competition_flight::fly_and_score;
use crate::optimizer::generation::{self, CandidateSample, Choices};
use crate::optimizer::GeneratedGeometry;
use crate::optimizer::{
    OptimizationError, OptimizationProblem, PreliminaryModel, SelectedComponents, VariableBounds,
};
use crate::perf::MissionConfig;
use crate::score::{presets, ScoreEngine};
use crate::spec::mission::{Segment, SegmentKind};
use crate::{evaluate, UavDesign, UavReport};

/// Rounding tolerance on normalised hard-gate margins. A published limit can
/// equal a computed value exactly (a 3S pack at 4.2 V per cell sits on the
/// 12.6 V gate), and the floating-point sum then lands at -1e-16.
pub const GATE_TOLERANCE: f64 = 1.0e-9;
/// Bounds of the two tail-volume genes (ASSUMPTION: conventional-tail ranges
/// of the Raymer type; the catalogue does not carry them).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TailVolumeBounds {
    /// Horizontal-tail volume coefficient.
    pub horizontal: VariableBounds,
    /// Vertical-tail volume coefficient.
    pub vertical: VariableBounds,
}

impl Default for TailVolumeBounds {
    fn default() -> Self {
        Self {
            horizontal: VariableBounds {
                minimum: 0.4,
                maximum: 0.7,
            },
            vertical: VariableBounds {
                minimum: 0.02,
                maximum: 0.05,
            },
        }
    }
}

/// The competition objective and search domain.
pub struct CompetitionProblem<'a> {
    /// Catalogue, maps, objectives, geometry bounds, physics model and budget.
    /// Its `evaluations` field is the total objective-evaluation budget.
    pub base: OptimizationProblem<'a>,
    /// Preset id, for example `acc-2026`.
    pub preset_id: &'a str,
    /// Preset variant, if the preset defines one.
    pub preset_variant: Option<&'a str>,
    /// Announced take-off distance that the aircraft must meet, m.
    pub announced_takeoff_m: f64,
    /// Scalars the simulation cannot know (payload count, static margin when
    /// available, rule quantities). They override the derived inputs.
    ///
    /// Rule quantities the simulation cannot know. The static margin and the
    /// spar margins come from the M4 airframe analysis, not from here.
    pub extra_inputs: BTreeMap<String, f64>,
    /// Bounds of the tail-volume genes.
    pub tail_volume_bounds: TailVolumeBounds,
    /// Differential-evolution population size (None: the search default).
    pub population: Option<usize>,
    /// Fly the preset's landing segment. When false it is dropped before the
    /// mission. The flare ends at touchdown (`perf::landing`), so a landing
    /// that flies is evaluated; the preset scores do not read landing outputs.
    pub include_landing: bool,
}

/// Preset data compiled once per problem.
pub(super) struct CompiledObjective {
    pub(super) engine: ScoreEngine,
    pub(super) segments: Vec<Segment>,
    pub(super) config: MissionConfig,
}

/// Compile the preset, its mission template and the score engine.
pub(super) fn compile(
    problem: &CompetitionProblem<'_>,
) -> Result<CompiledObjective, OptimizationError> {
    let preset = presets::load(problem.preset_id)
        .map_err(|error| OptimizationError::InvalidProblem(format!("preset: {error}")))?;
    let config = mission_config_for(&preset);
    let engine = ScoreEngine::new(&preset, problem.preset_variant)
        .map_err(|error| OptimizationError::InvalidProblem(format!("score engine: {error}")))?;
    let mut segments = engine.preset().segments.clone();
    if !problem.include_landing {
        segments.retain(|segment| !matches!(segment.kind, SegmentKind::Landing { .. }));
    }
    Ok(CompiledObjective {
        engine,
        segments,
        config,
    })
}

/// Lexicographic fitness for the Deb ordering: feasibility first, then the
/// violation magnitude, then the negated competition score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Fitness {
    pub(super) violation: f64,
    pub(super) objective: f64,
}

impl Fitness {
    /// `self` is at least as good as `other`.
    pub(super) fn no_worse_than(self, other: Fitness) -> bool {
        self.violation < other.violation
            || (self.violation == other.violation && self.objective <= other.objective)
    }

    /// `self` is strictly better than `other`.
    pub(super) fn better_than(self, other: Fitness) -> bool {
        self.violation < other.violation
            || (self.violation == other.violation && self.objective < other.objective)
    }
}

/// Decoded design variables shared by the search and the baseline.
#[derive(Clone, Copy)]
pub(super) struct Draw {
    pub(super) sample: CandidateSample,
    pub(super) horizontal_tail_volume: f64,
    pub(super) vertical_tail_volume: f64,
}

/// Outcome of one competition evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct CompetitionEvaluation {
    /// Every hard constraint holds and the round was evaluated.
    pub feasible: bool,
    /// Sum of normalised violations (0 when feasible).
    pub violation: f64,
    /// Round competition score (points), when the round was evaluated.
    pub score: Option<f64>,
    /// Take-off distance from the flown mission, m.
    pub takeoff_distance_m: Option<f64>,
    /// Loaded take-off mass from the preliminary feasibility pass, kg.
    pub takeoff_mass_kg: Option<f64>,
    /// Hard preset gates that could not be evaluated (missing inputs).
    pub unevaluated_gates: Vec<String>,
    /// Hard preset gates that failed.
    pub failed_gates: Vec<String>,
    /// Why the candidate stopped before a score, if it did.
    pub note: Option<String>,
}

/// Result of evaluating one draw, including the Deb fitness.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Outcome {
    pub(super) evaluation: CompetitionEvaluation,
    pub(super) fitness: Fitness,
}

/// Evaluate the competition objective for one draw.
pub(super) fn evaluate_draw(
    problem: &CompetitionProblem<'_>,
    objective: &CompiledObjective,
    choices: &Choices<'_>,
    draw: Draw,
) -> Outcome {
    let model = PreliminaryModel {
        horizontal_tail_volume_coefficient: draw.horizontal_tail_volume,
        vertical_tail_volume_coefficient: draw.vertical_tail_volume,
        ..problem.base.model
    };
    let base = OptimizationProblem {
        model,
        ..problem.base
    };
    let candidate = match generation::generate(&base, choices, draw.sample) {
        Ok(candidate) => candidate,
        Err(findings) => {
            return infeasible(
                findings.len(),
                None,
                format!("{} generation findings", findings.len()),
            );
        }
    };
    let mut report = evaluate(&candidate.design);
    report.findings.extend(candidate.findings);
    if !report.verified_feasible() {
        let count = report.findings.len();
        return infeasible(
            count,
            report.takeoff_mass_kg,
            "preliminary feasibility failed".to_owned(),
        );
    }
    match fly_and_score(
        &base,
        objective,
        problem,
        &candidate.design,
        &candidate.geometry,
        &candidate.components,
        &report,
    ) {
        Ok(evaluation) => {
            let fitness = if evaluation.feasible {
                Fitness {
                    violation: 0.0,
                    objective: -evaluation.score.unwrap_or(f64::NEG_INFINITY),
                }
            } else {
                Fitness {
                    violation: evaluation.violation,
                    objective: f64::INFINITY,
                }
            };
            Outcome {
                evaluation,
                fitness,
            }
        }
        Err(rejection) => Outcome {
            evaluation: CompetitionEvaluation {
                feasible: false,
                violation: rejection.violation,
                score: None,
                takeoff_distance_m: None,
                takeoff_mass_kg: report.takeoff_mass_kg,
                unevaluated_gates: Vec::new(),
                failed_gates: Vec::new(),
                note: Some(rejection.note.clone()),
            },
            fitness: Fitness {
                violation: rejection.violation,
                objective: f64::INFINITY,
            },
        },
    }
}

fn infeasible(count: usize, takeoff_mass_kg: Option<f64>, note: String) -> Outcome {
    let violation = 10.0 + count as f64;
    Outcome {
        evaluation: CompetitionEvaluation {
            feasible: false,
            violation,
            score: None,
            takeoff_distance_m: None,
            takeoff_mass_kg,
            unevaluated_gates: Vec::new(),
            failed_gates: Vec::new(),
            note: Some(note),
        },
        fitness: Fitness {
            violation,
            objective: f64::INFINITY,
        },
    }
}

/// A design rebuilt from its draw, for reporting the best candidate.
pub(super) struct Materialised {
    pub(super) design: UavDesign,
    pub(super) report: UavReport,
    pub(super) geometry: GeneratedGeometry,
    pub(super) components: SelectedComponents,
}

/// Regenerate the report artefacts of one draw. Generation is deterministic,
/// so this reproduces the candidate that was scored.
pub(super) fn materialise(
    problem: &CompetitionProblem<'_>,
    choices: &Choices<'_>,
    draw: Draw,
) -> Option<Materialised> {
    let model = PreliminaryModel {
        horizontal_tail_volume_coefficient: draw.horizontal_tail_volume,
        vertical_tail_volume_coefficient: draw.vertical_tail_volume,
        ..problem.base.model
    };
    let base = OptimizationProblem {
        model,
        ..problem.base
    };
    let candidate = generation::generate(&base, choices, draw.sample).ok()?;
    let mut report = evaluate(&candidate.design);
    report.findings.extend(candidate.findings);
    Some(Materialised {
        design: candidate.design,
        report,
        geometry: candidate.geometry,
        components: candidate.components,
    })
}
