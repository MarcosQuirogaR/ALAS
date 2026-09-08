// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


impl std::error::Error for OptimizationError {}

/// Search catalogue combinations and continuous geometry with a fixed seed.
pub fn optimize(problem: &OptimizationProblem<'_>) -> Result<OptimizedUav, OptimizationError> {
    optimize_with_control(problem, |_| {}, || false)
}

/// Search with cooperative cancellation and monotonic candidate progress.
///
/// Cancellation is observed only between candidates, so an uninterrupted run
/// preserves exactly the same random-number consumption, ranking, and result
/// as [`optimize`].
pub fn optimize_with_control(
    problem: &OptimizationProblem<'_>,
    mut on_progress: impl FnMut(OptimizationProgress),
    mut should_cancel: impl FnMut() -> bool,
) -> Result<OptimizedUav, OptimizationError> {
    validate_problem(problem)?;
    let choices = generation::Choices::from_catalog(problem.catalog);
    let mut rng = SplitMix64::new(problem.seed);
    let mut best: Option<OptimizedUav> = None;
    let mut rejection_counts: Vec<RejectionCount> = Vec::new();
    let mut rejection_examples: Vec<RejectionExample> = Vec::new();
    let mut best_evaluated: Option<RejectedUav> = None;
    let mut verified_candidates = 0;

    for evaluation_index in 0..problem.evaluations {
        if should_cancel() {
            return Err(OptimizationError::Cancelled {
                evaluated_candidates: evaluation_index,
            });
        }
        let sample = generation::CandidateSample {
            battery_index: rng.index(choices.batteries.len()),
            motor_index: rng.index(choices.motors.len()),
            esc_index: rng.index(choices.escs.len()),
            propeller_index: rng.index(choices.propellers.len()),
            servo_index: rng.index(choices.servos.len()),
            material_index: rng.index(choices.materials.len()),
            receiver_index: rng.index(choices.receivers.len()),
            electronics_index: rng.index(choices.electronics.len()),
            landing_gear_index: rng.index(choices.landing_gear.len()),
            wing_area_m2: problem.geometry_bounds.wing_area_m2.sample(rng.unit()),
            wing_aspect_ratio: problem.geometry_bounds.wing_aspect_ratio.sample(rng.unit()),
            fuselage_length_m: problem.geometry_bounds.fuselage_length_m.sample(rng.unit()),
            wing_leading_edge_fraction: problem
                .geometry_bounds
                .wing_leading_edge_fraction
                .sample(rng.unit()),
        };
        let candidate = match generation::generate(problem, &choices, sample) {
            Ok(candidate) => candidate,
            Err(findings) => {
                count_rejections(
                    &mut rejection_counts,
                    findings.iter().map(|finding| finding.kind),
                );
                capture_rejection_examples(&mut rejection_examples, &findings);
                on_progress(OptimizationProgress {
                    evaluated_candidates: evaluation_index + 1,
                    total_candidates: problem.evaluations,
                    verified_candidates,
                    best_score: best.as_ref().map(|current| current.metrics.score),
                });
                continue;
            }
        };
        let mut report = crate::evaluate(&candidate.design);
        report.findings.extend(candidate.findings);
        let metrics = generation::metrics(problem, &candidate.design, &report);
        if let Some(metrics) = metrics {
            if metrics.propulsive_efficiency < problem.objectives.minimum_propulsive_efficiency {
                report.findings.push(crate::Finding {
                    severity: crate::Severity::Failure,
                    kind: FindingKind::EfficiencyShortfall,
                    subject: "cruise efficiency".to_owned(),
                    message: "drag power divided by electrical input power is below the objective"
                        .to_owned(),
                    required: Some(problem.objectives.minimum_propulsive_efficiency),
                    available: Some(metrics.propulsive_efficiency),
                    units: None,
                });
            }
            if report.verified_feasible() {
                verified_candidates += 1;
                let accepted = OptimizedUav {
                    design: candidate.design,
                    report,
                    geometry: candidate.geometry,
                    components: candidate.components,
                    metrics,
                    evaluation_index,
                    seed: problem.seed,
                };
                if best
                    .as_ref()
                    .is_none_or(|current| accepted.metrics.score < current.metrics.score)
                {
                    best = Some(accepted);
                }
                on_progress(OptimizationProgress {
                    evaluated_candidates: evaluation_index + 1,
                    total_candidates: problem.evaluations,
                    verified_candidates,
                    best_score: best.as_ref().map(|current| current.metrics.score),
                });
                continue;
            }
        }
        let rejected = RejectedUav {
            design: candidate.design,
            report: report.clone(),
            geometry: candidate.geometry,
            components: candidate.components,
            metrics,
            evaluation_index,
            seed: problem.seed,
        };
        if best_evaluated
            .as_ref()
            .is_none_or(|current| rejected_rank(&rejected) < rejected_rank(current))
        {
            best_evaluated = Some(rejected);
        }
        count_rejections(
            &mut rejection_counts,
            report.findings.iter().map(|finding| finding.kind),
        );
        capture_rejection_examples(&mut rejection_examples, &report.findings);
        on_progress(OptimizationProgress {
            evaluated_candidates: evaluation_index + 1,
            total_candidates: problem.evaluations,
            verified_candidates,
            best_score: best.as_ref().map(|current| current.metrics.score),
        });
    }

    best.ok_or(OptimizationError::NoFeasibleDesign(Box::new(
        NoFeasibleDesign {
            evaluated_candidates: problem.evaluations,
            rejections: rejection_counts,
            examples: rejection_examples,
            best_evaluated,
        },
    )))
}

fn capture_rejection_examples(examples: &mut Vec<RejectionExample>, findings: &[Finding]) {
    for finding in findings {
        if examples.iter().any(|example| example.kind == finding.kind) {
            continue;
        }
        examples.push(RejectionExample {
            kind: finding.kind,
            subject: finding.subject.clone(),
            message: finding.message.clone(),
        });
    }
}

fn rejected_rank(candidate: &RejectedUav) -> (usize, usize, f64) {
    let failures = candidate
        .report
        .findings
        .iter()
        .filter(|finding| finding.severity == crate::Severity::Failure)
        .count();
    let total = candidate.report.findings.len();
    let score = candidate.metrics.map_or(f64::INFINITY, |metrics| {
        if metrics.score.is_finite() {
            metrics.score
        } else {
            f64::INFINITY
        }
    });
    (total, failures, score)
}

fn count_rejections(counts: &mut Vec<RejectionCount>, kinds: impl Iterator<Item = FindingKind>) {
    let mut unique = Vec::new();
    for kind in kinds {
        if !unique.contains(&kind) {
            unique.push(kind);
        }
    }
    for kind in unique {
        if let Some(count) = counts.iter_mut().find(|count| count.kind == kind) {
            count.candidates += 1;
        } else {
            counts.push(RejectionCount {
                kind,
                candidates: 1,
            });
        }
    }
}

fn interpolate(lower: f64, upper: f64, fraction: f64) -> f64 {
    lower + fraction * (upper - lower)
}

struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn unit(&mut self) -> f64 {
        let mantissa = self.next() >> 11;
        mantissa as f64 * (1.0 / ((1_u64 << 53) as f64))
    }

    fn index(&mut self, length: usize) -> usize {
        if length == 0 {
            0
        } else {
            (self.next() % length as u64) as usize
        }
    }
}

