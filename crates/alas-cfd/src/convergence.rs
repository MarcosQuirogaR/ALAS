// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// Parse the quality gate output. A zero exit code is insufficient: several
/// OpenFOAM releases report failed mesh checks while still returning success.
pub fn parse_mesh_quality(output: &str) -> MeshQuality {
    let lower = output.to_ascii_lowercase();
    let explicit_ok = lower.contains("mesh ok");
    let failed_checks = lower
        .split("failed")
        .filter_map(|tail| tail.split_whitespace().next())
        .filter_map(|token| token.parse::<u64>().ok())
        .any(|count| count > 0);
    let passed = explicit_ok && !failed_checks;
    let cells = output.lines().find_map(|line| {
        let lower_line = line.to_ascii_lowercase();
        lower_line.find("cells:").and_then(|index| {
            line[index + "cells:".len()..]
                .split_whitespace()
                .find_map(|token| token.parse::<u64>().ok())
        })
    });
    let max_non_orthogonality_deg = find_labeled_number(output, "max non-orthogonality")
        .or_else(|| find_labeled_number(output, "non-orthogonality max"));
    let max_skewness = find_labeled_number(output, "max skewness");
    let min_volume_m3 = find_labeled_number(output, "min volume");
    // `checkMesh` prints `*Number of severely non-orthogonal (> 70 degrees)
    // faces: N.` as a warning and still passes the check.  The count is what
    // makes the maximum angle readable, so it is recorded, not acted on.
    // `checkMesh -meshQuality` counts the faces violating each declared limit.
    // It is the only place this toolchain separates internal from boundary
    // skewness, so without it `max_boundary_skewness` can only ever be
    // unmeasured.  The line is `faces with skewness > 4 (internal) or 20
    // (boundary) : N`; `N == 0` demonstrates BOTH limits, and `N > 0` is a
    // failure of at least one, which is reported as a failure of both because
    // the text does not attribute it.
    let skewness_faces_in_error = find_labeled_number(output, "(boundary) :")
        .filter(|count| count.is_finite() && *count >= 0.0)
        .map(|count| count as u64);
    let non_orthogonality_faces_in_error =
        find_labeled_number(output, "degrees                        :")
            .filter(|count| count.is_finite() && *count >= 0.0)
            .map(|count| count as u64);
    let severely_non_orthogonal_faces =
        find_labeled_number(output, "severely non-orthogonal (> 70 degrees) faces:")
            .filter(|count| count.is_finite() && *count >= 0.0)
            .map(|count| count as u64);
    MeshQuality {
        passed,
        cells,
        max_non_orthogonality_deg,
        max_skewness,
        min_volume_m3,
        severely_non_orthogonal_faces,
        // Filled from the written `skewness` field by the result builder; the
        // log text does not contain a boundary maximum.
        max_boundary_skewness: None,
        max_boundary_skewness_patch: None,
        skewness_faces_in_error,
        non_orthogonality_faces_in_error,
        raw_output: output.to_owned(),
        distributions: Vec::new(),
        near_wall: None,
        near_wall_distribution: None,
    }
}

fn find_labeled_number(text: &str, label: &str) -> Option<f64> {
    text.lines().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        let offset = lower.find(label)? + label.len();
        extract_numeric_values(&line[offset..]).into_iter().next()
    })
}

/// Absolute tolerance for generated inner linear solves. `minIter 1` ensures
/// an equation is not skipped merely because its initial residual falls below
/// this tolerance. Small residuals and stationary fields alone do not establish
/// either equation failure or physical validity.
pub const LINEAR_SOLVER_RESIDUAL_FLOOR: f64 = 1.0e-8;
/// Decide whether the available evidence supports a numerically converged
/// coefficient result.
pub fn classify_convergence(
    config: &CfdStudyConfig,
    process_status: OpenFoamProcessStatus,
    mesh_quality: &MeshQuality,
    residuals: &[ResidualSample],
    forces: &[ForceSample],
    mass_balance: &[MassBalanceSample],
    field_updates: Option<&FieldUpdateEvidence>,
) -> (CfdOutcome, String) {
    // checkMesh can return exit code zero while reporting failed quality
    // checks. Preserve that concrete gate reason even though the runner marks
    // the overall study as failed.
    let mesh_reported_failure = !mesh_quality.passed
        && (!mesh_quality.raw_output.trim().is_empty())
        && (mesh_quality
            .raw_output
            .to_ascii_lowercase()
            .contains("failed"));
    if mesh_reported_failure {
        return (
            CfdOutcome::Failed,
            "checkMesh reported failed mesh checks; the solver was not launched.".to_owned(),
        );
    }
    match process_status {
        OpenFoamProcessStatus::Cancelled => {
            return (
                CfdOutcome::Cancelled,
                "The solver process was cancelled by the user.".to_owned(),
            )
        }
        OpenFoamProcessStatus::TimedOut => {
            return (
                CfdOutcome::Failed,
                "The solver exceeded its configured timeout.".to_owned(),
            )
        }
        OpenFoamProcessStatus::LaunchFailed => {
            return (
                CfdOutcome::Failed,
                "The solver could not be launched.".to_owned(),
            )
        }
        OpenFoamProcessStatus::Failed => {
            return (
                CfdOutcome::Failed,
                "The solver exited with a non-zero status.".to_owned(),
            )
        }
        OpenFoamProcessStatus::Completed => {}
    }
    if !mesh_quality.passed {
        return (
            CfdOutcome::Failed,
            "checkMesh did not report Mesh OK with zero failed checks.".to_owned(),
        );
    }
    // A run that completes its process and its mesh gate can still return
    // finite numbers no section could produce.  A relaxation-sweep case
    // `P3` did exactly that (Cl -302.74, Cd -230.15, wall y+ to 101) and was
    // classified only `unconverged`, which understates it: `unconverged` says
    // the answer is incomplete, while this says the answer is broken.  Screen
    // before the residual gate so the more specific reason is the one reported.
    let plausibility = assess_physical_plausibility(forces, mesh_quality);
    if plausibility.verdict == PlausibilityVerdict::Implausible {
        return (CfdOutcome::Failed, plausibility.detail());
    }
    if residuals.is_empty() {
        return (
            CfdOutcome::Unconverged,
            "No equation residuals were parsed from the solver log.".to_owned(),
        );
    }
    if residuals
        .iter()
        .any(|sample| !sample.initial.is_finite() || !sample.final_residual.is_finite())
    {
        return (
            CfdOutcome::Unconverged,
            "Equation residual history contains a non-finite value; convergence cannot be certified."
                .to_owned(),
        );
    }
    let required_equations: &[&str] = if config.effective_simulation().compressible {
        &["p", "ux", "uy", "e", "k", "omega"]
    } else {
        &["p", "ux", "uy", "k", "omega"]
    };
    let required_equation_count = required_equations.len();
    // OpenFOAM writes two pressure solves for each SIMPLE iteration.  Use a
    // common outer `Time =` marker and the largest initial residual for each
    // equation in that iteration; enumerating log lines would incorrectly
    // treat the second pressure correction as a later iteration.
    // The largest parsed outer time is authoritative.  Falling back to an
    // earlier complete iteration would allow a truncated or malformed final
    // solver iteration to inherit a green result from stale history.
    let latest_iteration = residuals
        .iter()
        .map(|sample| sample.iteration)
        .max()
        .unwrap_or_default();
    let missing_equations = required_equations
        .iter()
        .filter(|required| {
            !residuals.iter().any(|sample| {
                sample.iteration == latest_iteration
                    && sample.field.eq_ignore_ascii_case(required)
                    && sample.initial.is_finite()
            })
        })
        .copied()
        .collect::<Vec<_>>();
    if !missing_equations.is_empty() {
        return (
            CfdOutcome::Unconverged,
            format!(
                "The final log iteration is missing required equation(s): {}.",
                missing_equations.join(", ")
            ),
        );
    }
    let mut last_initial_residuals = BTreeMap::new();
    for residual in residuals
        .iter()
        .filter(|sample| sample.iteration == latest_iteration)
    {
        let field = residual.field.to_ascii_lowercase();
        last_initial_residuals
            .entry(field)
            .and_modify(|value: &mut f64| *value = value.max(residual.initial))
            .or_insert(residual.initial);
    }
    if required_equations.iter().any(|required| {
        !residuals.iter().any(|sample| {
            sample.iteration == latest_iteration
                && sample.field.eq_ignore_ascii_case(required)
                && sample.final_residual.is_finite()
        })
    }) {
        return (
            CfdOutcome::Unconverged,
            "The final outer SIMPLE iteration lacks a finite linear-solver residual for every primary equation."
                .to_owned(),
        );
    }
    // Compare persisted internal fields as a conservative evidence gate.
    // Equality at the configured ASCII precision is an observation, not proof
    // of an inactive equation: an update may be smaller than stored precision,
    // or confined to boundary values. Preserve the fail-safe and describe its
    // uncertainty explicitly. Generated cases request at least one inner
    // iteration and 17-digit writes to avoid preventable skipped solves and
    // round-trip information loss; legacy cases remain supported.
    let mut dead = Vec::new();
    for required in required_equations.iter().copied() {
        let Some(sample) = field_updates.and_then(|evidence| evidence.sample(required)) else {
            continue;
        };
        if sample.updated() {
            continue;
        }
        // If NOTHING moved, the solution is stationary and that is the answer,
        // not a defect.  A dead equation is recognised only by the contrast.
        if !field_updates.is_some_and(|evidence| evidence.another_field_updated(required)) {
            continue;
        }
        let residual = last_initial_residuals.get(required).map_or_else(
            || "residual unavailable".to_owned(),
            |value| format!("{value:.3e}"),
        );
        dead.push(format!(
            "{required} {residual}, and {}",
            sample.observation()
        ));
    }
    dead.dedup();
    // Only where evidence is MISSING does the residual pattern matter, and then
    // Missing evidence blocks certification UNCONDITIONALLY, not only when the
    // residual history happens to look suspicious.
    //
    // The previous form consulted the residual pattern first and reported
    // INCONCLUSIVE only for an equation that already looked frozen.  An
    // ordinary-looking residual history with absent, malformed or non-finite
    // field data therefore certified silently, which contradicts the contract
    // this gate claims to enforce.  Every required equation must have a usable
    // observation; anything else is unproven.
    let unobserved = required_equations
        .iter()
        .filter(|required| {
            field_updates
                .and_then(|evidence| evidence.sample(required))
                .is_none()
        })
        .map(|required| (*required).to_owned())
        .collect::<Vec<_>>();
    if !dead.is_empty() {
        let residual_blockers = required_equations
            .iter()
            .filter_map(|field| {
                let value = last_initial_residuals.get(*field)?;
                (*value > config.solver.residual_tolerance).then(|| format!("{field} {value:.3e}"))
            })
            .collect::<Vec<_>>();
        let residual_detail = if residual_blockers.is_empty() {
            String::new()
        } else {
            format!(
                " Independently, the following outer residuals exceed the {:.3e} acceptance tolerance: {}.",
                config.solver.residual_tolerance,
                residual_blockers.join(", ")
            )
        };
        return (
            CfdOutcome::Failed,
            format!(
                "Outer SIMPLE iteration {latest_iteration}: {}, while at least one other solved field did change. OBSERVATION, not a proven equation failure: equal written values mean no update was persisted at the stored precision, which a sub-precision update or a boundary-only change would also produce. Numerical certification is refused as a fail-safe.{residual_detail}",
                dead.join("; "),
            ),
        );
    }
    if !unobserved.is_empty() {
        // Explicitly inconclusive, and therefore not certified.  Guessing in
        // either direction here is what produced this gate's earlier false
        // verdicts.
        let reason = field_updates.map_or_else(
            || "no field-update evidence was supplied at all".to_owned(),
            |evidence| {
                evidence
                    .unavailable_reason
                    .clone()
                    .unwrap_or_else(|| "the evidence record contains no sample for them".to_owned())
            },
        );
        return (
            CfdOutcome::Unconverged,
            format!(
                "Outer SIMPLE iteration {latest_iteration}: INCONCLUSIVE. The residual gate is satisfied, but {} of {required_equation_count} primary equations have no usable written-field observation ({}): {}. Whether those equations are still being solved cannot be decided from the residual history, so the result is NOT CERTIFIED. Re-run with field writing enabled, or supply a complete write pair, to resolve it.",
                unobserved.len(),
                reason,
                unobserved.join(", "),
            ),
        );
    }
    let mut over_tolerance = required_equations
        .iter()
        .filter_map(|required| {
            let value = *last_initial_residuals.get(*required)?;
            (!value.is_finite() || value > config.solver.residual_tolerance)
                .then_some((*required, value))
        })
        .collect::<Vec<_>>();
    if !over_tolerance.is_empty() {
        // Name the equation and how far it actually is.  "At least one
        // residual is too high" cannot be acted on; a stalled omega at
        // 3e-4 and a still-falling p at 2e-3 call for different work, and
        // the difference is already in the parsed history.
        over_tolerance.sort_by(|left, right| right.1.total_cmp(&left.1));
        let detail = over_tolerance
            .iter()
            .map(|(field, value)| {
                format!(
                    "{field} {value:.3e} ({:.0}x tolerance)",
                    value / config.solver.residual_tolerance
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        return (
            CfdOutcome::Unconverged,
            format!(
                "Outer SIMPLE iteration {latest_iteration} of {} leaves {} of {required_equation_count} primary equations above the {:.3e} residual tolerance: {detail}.",
                config.solver.max_iterations,
                over_tolerance.len(),
                config.solver.residual_tolerance,
            ),
        );
    }
    if forces.iter().any(|sample| {
        !sample.time.is_finite()
            || !sample.cd.is_finite()
            || !sample.cl.is_finite()
            || !sample.cm.is_finite()
            || [
                sample.cd_pressure,
                sample.cd_viscous,
                sample.cl_pressure,
                sample.cl_viscous,
            ]
            .into_iter()
            .flatten()
            .any(|value| !value.is_finite())
    }) {
        return (
            CfdOutcome::Unconverged,
            "Force history contains a non-finite coefficient; convergence cannot be certified."
                .to_owned(),
        );
    }
    let window = config.solver.force_window;
    if forces.len() < window {
        return (
            CfdOutcome::Unconverged,
            format!(
                "Only {} of the configured {} force samples are available for stabilization.",
                forces.len(),
                window
            ),
        );
    }
    let tail = &forces[forces.len() - window..];
    if relative_spread(tail.iter().map(|row| row.cd)) > config.solver.force_tolerance
        || relative_spread(tail.iter().map(|row| row.cl)) > config.solver.force_tolerance
        || relative_spread(tail.iter().map(|row| row.cm)) > config.solver.force_tolerance
    {
        return (
            CfdOutcome::Unconverged,
            format!(
                "Lift, drag or pitching moment has not stabilized within {:.3} relative spread.",
                config.solver.force_tolerance
            ),
        );
    }
    if mass_balance.is_empty() {
        return (
            CfdOutcome::Unconverged,
            "No continuity/mass-balance diagnostic was parsed from the solver log.".to_owned(),
        );
    }
    // `cumulative` is an integral history and is expected to remain larger
    // than a per-iteration tolerance in a long run.  Gate only the latest
    // local/global continuity errors; retain cumulative values as audit data.
    let continuity_bad = mass_balance
        .iter()
        .rev()
        .find(|row| row.sum_local.is_some() || row.global.is_some())
        .map(|row| {
            [row.sum_local, row.global]
                .into_iter()
                .flatten()
                .any(|value| {
                    !value.is_finite() || value.abs() > config.solver.mass_balance_tolerance
                })
        })
        .unwrap_or(true);
    if continuity_bad {
        return (
            CfdOutcome::Unconverged,
            format!(
                "Continuity error exceeds {:.3e}.",
                config.solver.mass_balance_tolerance
            ),
        );
    }
    (
        CfdOutcome::NumericallyConverged,
        "Residuals, stabilized forces and continuity diagnostics satisfy the configured criteria."
            .to_owned(),
    )
}

/// Envelope a two-dimensional section coefficient can physically occupy.
///
/// These are deliberately loose outer bounds, not an accuracy check.  They are
/// the limits outside which a reported number cannot describe a conventional
/// single-section external-flow result at all, whatever the model or the mesh:
/// a solver that produces `Cl = -302.7` has broken down, it has not merely stopped short of a
/// tolerance.  Every bound is stated here rather than buried in the check so a
/// reader can disagree with a specific number.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlausibilityBounds {
    /// Largest magnitude of section lift coefficient accepted.  Single-element
    /// sections stall below 2; high-lift multi-element sections reach about
    /// 4.5.  5 leaves margin above every physical case.
    pub max_abs_cl: f64,
    /// Largest section drag coefficient accepted.  A flat plate normal to the
    /// stream is about 2.0, which no streamlined section can exceed.
    pub max_cd: f64,
    /// Drag must be positive: a steady section in a uniform stream cannot
    /// produce thrust.
    pub min_cd: f64,
    /// Largest magnitude of quarter-chord pitching-moment coefficient.
    pub max_abs_cm: f64,
    /// Largest wall `y+` for which any wall treatment in this template remains
    /// inside its documented validity range.
    pub max_wall_y_plus: f64,
}

impl Default for PlausibilityBounds {
    fn default() -> Self {
        Self {
            max_abs_cl: 5.0,
            max_cd: 2.0,
            min_cd: 0.0,
            max_abs_cm: 1.0,
            max_wall_y_plus: 300.0,
        }
    }
}

/// One bound a result violated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlausibilityViolation {
    /// Quantity that left the envelope, using the reported field name.
    pub quantity: String,
    /// Value the solver produced.
    pub value: f64,
    /// Bound it violated.
    pub bound: f64,
    /// Why this bound exists, in one sentence.
    pub detail: String,
}

/// Verdict of the physical-plausibility screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlausibilityVerdict {
    /// Every reported quantity lies inside the envelope.
    Plausible,
    /// At least one quantity cannot describe a conventional section result.
    Implausible,
    /// No force sample was available to screen.
    NotEvaluated,
}

/// Result of the physical-plausibility screen, with the bounds it used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicalPlausibility {
    /// Overall verdict.
    pub verdict: PlausibilityVerdict,
    /// Bounds applied, reported so the screen is auditable.
    pub bounds: PlausibilityBounds,
    /// Every violated bound, in the order checked.
    pub violations: Vec<PlausibilityViolation>,
}

impl PhysicalPlausibility {
    /// One-line summary naming each violated bound and its value.
    pub fn detail(&self) -> String {
        match self.verdict {
            PlausibilityVerdict::Plausible => {
                "Reported coefficients lie inside the two-dimensional section envelope.".to_owned()
            }
            PlausibilityVerdict::NotEvaluated => {
                "No force sample was available for the physical-plausibility screen.".to_owned()
            }
            PlausibilityVerdict::Implausible => format!(
                "The solution left the physical envelope for a two-dimensional section: {}. A finite number outside this envelope is a broken solution, not an unconverged one, and is refused as a result.",
                self.violations
                    .iter()
                    .map(|violation| format!(
                        "{} {:.4e} against bound {:.4e} ({})",
                        violation.quantity, violation.value, violation.bound, violation.detail
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        }
    }
}

/// Screen the reported coefficients and wall state against the physical
/// envelope of a two-dimensional section.
///
/// This is **not** a physics check and never a substitute for the residual,
/// force or mass-balance gates: it only refuses numbers that no section can
/// produce.  It is applied to the last parsed force sample and to the measured
/// wall `y+`, both of which are solver output rather than configuration.
///
/// Non-finite values are deliberately **not** screened here.  A `NaN` or
/// infinite coefficient is already refused by the separate non-finite force
/// check, and leaving the two orthogonal keeps each failure reported by the
/// gate that actually describes it.  This screen answers one question only:
/// the value is a real number, but can a section produce it?
pub fn assess_physical_plausibility(
    forces: &[ForceSample],
    mesh_quality: &MeshQuality,
) -> PhysicalPlausibility {
    let bounds = PlausibilityBounds::default();
    let Some(last) = forces
        .last()
        .filter(|sample| sample.cl.is_finite() && sample.cd.is_finite() && sample.cm.is_finite())
    else {
        return PhysicalPlausibility {
            verdict: PlausibilityVerdict::NotEvaluated,
            bounds,
            violations: Vec::new(),
        };
    };
    let mut violations = Vec::new();
    if last.cl.abs() > bounds.max_abs_cl {
        violations.push(PlausibilityViolation {
            quantity: "Cl".to_owned(),
            value: last.cl,
            bound: bounds.max_abs_cl,
            detail: "a single- or multi-element section cannot exceed this lift coefficient"
                .to_owned(),
        });
    }
    if last.cd > bounds.max_cd {
        violations.push(PlausibilityViolation {
            quantity: "Cd".to_owned(),
            value: last.cd,
            bound: bounds.max_cd,
            detail: "a streamlined section cannot exceed the drag of a normal flat plate"
                .to_owned(),
        });
    }
    if last.cd <= bounds.min_cd {
        violations.push(PlausibilityViolation {
            quantity: "Cd".to_owned(),
            value: last.cd,
            bound: bounds.min_cd,
            detail: "a steady section in a uniform stream cannot produce thrust".to_owned(),
        });
    }
    if last.cm.abs() > bounds.max_abs_cm {
        violations.push(PlausibilityViolation {
            quantity: "Cm".to_owned(),
            value: last.cm,
            bound: bounds.max_abs_cm,
            detail: "a quarter-chord moment this large is outside any section envelope".to_owned(),
        });
    }
    if let Some(max_y_plus) = mesh_quality
        .near_wall
        .as_ref()
        .map(|near_wall| near_wall.max_y_plus)
    {
        if !max_y_plus.is_finite() || max_y_plus > bounds.max_wall_y_plus {
            violations.push(PlausibilityViolation {
                quantity: "wall y+ max".to_owned(),
                value: max_y_plus,
                bound: bounds.max_wall_y_plus,
                detail: "no wall treatment in this template is valid at this wall distance"
                    .to_owned(),
            });
        }
    }
    PhysicalPlausibility {
        verdict: if violations.is_empty() {
            PlausibilityVerdict::Plausible
        } else {
            PlausibilityVerdict::Implausible
        },
        bounds,
        violations,
    }
}

fn relative_spread(values: impl Iterator<Item = f64>) -> f64 {
    let values = values.collect::<Vec<_>>();
    if values.is_empty() {
        return f64::INFINITY;
    }
    if values.iter().any(|value| !value.is_finite()) {
        return f64::INFINITY;
    }
    let min = values
        .iter()
        .fold(f64::INFINITY, |value, next| value.min(*next));
    let max = values
        .iter()
        .fold(f64::NEG_INFINITY, |value, next| value.max(*next));
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    (max - min) / mean.abs().max(1.0e-3)
}
