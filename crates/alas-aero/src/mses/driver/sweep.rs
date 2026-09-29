// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

#[path = "recovery.rs"]
mod recovery;
use recovery::{RecoveredPolarPoint, RequestedPolarRecovery};

impl Mses {
    pub(super) fn solve_sweep(
        &self,
        dir: &Path,
        alphas: &[f64],
        reynolds: f64,
        mach: f64,
        cancel: Option<&AtomicBool>,
    ) -> Result<SweepOutcome, MsesFailure> {
        if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Err(MsesFailure::solver(
                "MSES sweep cancelled before mesh generation",
            ));
        }
        // MSET requires a named `blade.case` file with its four far-field
        // boundaries. `Airfoil::write_dat` is deliberately a Selig parity
        // export and cannot be passed as the MSET case argument directly.
        std::fs::write(
            dir.join("blade.case"),
            deck::mset_blade(&self.airfoil.name, &self.airfoil.coordinates),
        )
        .map_err(|error| MsesFailure::solver(error.to_string()))?;
        let first = *alphas
            .first()
            .ok_or_else(|| MsesFailure::solver("no angles to sweep"))?;
        self.run_mset(dir, first, cancel)?;

        let mut outcome = SweepOutcome::default();
        let mut recovery = RequestedPolarRecovery::new(alphas);
        // Bounded per-point retry flags. Never initialize retry at an arbitrary
        // 0 deg or center angle; retry only from clean target-angle initialization
        // or short bounded continuation from the last known converged state.
        let mut point_retry_attempted = vec![false; alphas.len()];
        let mut continuation_warmup_attempted = false;
        let mut last_converged_alpha: Option<f64> = None;
        let mut next_initialization =
            format!("initial MSET mesh at requested alpha={first:.6} deg");
        let mut index = 0;

        while index < alphas.len() {
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                outcome.cancelled = true;
                break;
            }
            let alpha = alphas[index];
            recovery.visited_count = index + 1;
            let purpose = std::mem::take(&mut next_initialization);
            let solve = self.run_mses_case(dir, alpha, reynolds, mach, cancel);
            let solve = match solve {
                Ok(run) => run,
                Err(failure) if cancelled(&failure) => {
                    outcome.cancelled = true;
                    break;
                }
                Err(failure) => return Err(failure),
            };
            if !solve.status.success() {
                return Err(MsesFailure::solver(format!(
                    "mses exited with {}",
                    solve.status
                )));
            }

            let point_status = if parse::is_converged(&solve.stdout) {
                MsesPolarPointStatus::Converged
            } else {
                MsesPolarPointStatus::NotConverged
            };
            outcome.solver_attempts.push(MsesSolverAttempt {
                alpha_deg: alpha,
                purpose: purpose.clone(),
                status: point_status,
                solver_output: solve.stdout.clone(),
            });
            let diagnostic = MsesPolarPointDiagnostic {
                requested_alpha_deg: alpha,
                status: point_status,
                solver_output: solve.stdout.clone(),
            };
            // A requested point may be retried from clean target mesh or
            // from a continuation anchor. Keep exactly one diagnostic for
            // that requested alpha while retaining every process transcript
            // in `solver_attempts` above. This makes the public diagnostic
            // vector parallel to the requested schedule rather than to the
            // number of recovery attempts.
            if let Some(existing) = outcome.point_diagnostics.get_mut(index) {
                *existing = diagnostic;
            } else {
                outcome.point_diagnostics.push(diagnostic);
            }

            if point_status == MsesPolarPointStatus::NotConverged {
                // Point 0 warm-up recovery:
                // If the first requested point fails from its clean target-angle mesh,
                // attempt a single bounded continuation warm-up at the adjacent requested point.
                if index == 0 && !continuation_warmup_attempted {
                    continuation_warmup_attempted = true;
                    if let Some(&next_requested) = alphas.get(index + 1) {
                        let step = (next_requested - alpha).abs();
                        if step <= MAX_CONTINUATION_STEP_DEG && step > f64::EPSILON {
                            let anchor = next_requested;
                            if self.run_mset_or_cancel(dir, anchor, cancel)? {
                                outcome.cancelled = true;
                                break;
                            }
                            let warmup_purpose = format!(
                                "unrequested continuation warm-up at alpha={anchor:.6} deg after requested alpha={alpha:.6} failed"
                            );
                            let warmup =
                                match self.run_mses_case(dir, anchor, reynolds, mach, cancel) {
                                    Ok(run) => run,
                                    Err(failure) if cancelled(&failure) => {
                                        outcome.cancelled = true;
                                        break;
                                    }
                                    Err(failure) => return Err(failure),
                                };
                            let warmup_status = if parse::is_converged(&warmup.stdout) {
                                MsesPolarPointStatus::Converged
                            } else {
                                MsesPolarPointStatus::NotConverged
                            };
                            outcome.solver_attempts.push(MsesSolverAttempt {
                                alpha_deg: anchor,
                                purpose: warmup_purpose,
                                status: warmup_status,
                                solver_output: warmup.stdout,
                            });
                            if warmup_status == MsesPolarPointStatus::Converged {
                                save_converged_state(dir);
                                last_converged_alpha = Some(anchor);
                                next_initialization = format!(
                                    "continuation from converged warm-up alpha={anchor:.6} deg"
                                );
                                point_retry_attempted[0] = true;
                                continue;
                            }

                            // The warm-up failed to converge. NEVER carry a failed warm-up,
                            // NaN, or diverged flowfield into the next point.
                            // Cleanly remesh at the next requested angle with MSET.
                            if self.run_mset_or_cancel(dir, anchor, cancel)? {
                                outcome.cancelled = true;
                                break;
                            }
                            next_initialization = format!(
                                "clean target-angle MSET mesh at alpha={anchor:.6} deg after failed continuation warm-up"
                            );
                            index += 1;
                            continue;
                        }
                    }
                }

                // Clean target-angle retry or bounded continuation from known-good state.
                // Never initialize retry at arbitrary 0.0 or center; jump delta is <= MAX_CONTINUATION_STEP_DEG.
                if !point_retry_attempted[index] {
                    point_retry_attempted[index] = true;
                    if last_converged_alpha.is_some()
                        && !purpose.contains("clean target-angle")
                        && !purpose.contains("initial MSET")
                    {
                        // Continuation failed: retry from clean target-angle mesh!
                        if self.run_mset_or_cancel(dir, alpha, cancel)? {
                            outcome.cancelled = true;
                            break;
                        }
                        next_initialization = format!(
                            "clean target-angle MSET restart at alpha={alpha:.6} deg after continuation failed"
                        );
                        continue;
                    } else if let Some(last_alpha) = last_converged_alpha {
                        match self.bridge_to_target_with_recovery(
                            dir,
                            last_alpha,
                            alpha,
                            reynolds,
                            mach,
                            cancel,
                            &mut outcome.solver_attempts,
                            Some(&mut recovery),
                        )? {
                            BridgeOutcome::Reached(reached) if reached == last_alpha => {
                                next_initialization = format!(
                                    "continuation from restored known-converged alpha={last_alpha:.6} deg after clean mesh failed"
                                );
                                continue;
                            }
                            BridgeOutcome::Reached(reached) => {
                                last_converged_alpha = Some(reached);
                                next_initialization = format!(
                                    "continuation from bridged intermediate state alpha={reached:.6} deg \
                                     (bounded steps from known-converged alpha={last_alpha:.6} deg) after clean mesh failed at alpha={alpha:.6} deg"
                                );
                                continue;
                            }
                            BridgeOutcome::Cancelled => {
                                outcome.cancelled = true;
                                break;
                            }
                            BridgeOutcome::Exhausted => {}
                        }
                    }
                }

                // Point has failed and exhausted its bounded retry.
                // Prepare the working directory for the NEXT requested angle:
                // Never carry diverged flowfield into the next point.
                // Either restore the last known converged state (if nearby) or clean remesh.
                if let Some(&next_alpha) = alphas.get(index + 1) {
                    let mut bridged = false;
                    if let Some(last_alpha) = last_converged_alpha {
                        match self.bridge_to_target_with_recovery(
                            dir,
                            last_alpha,
                            next_alpha,
                            reynolds,
                            mach,
                            cancel,
                            &mut outcome.solver_attempts,
                            Some(&mut recovery),
                        )? {
                            BridgeOutcome::Reached(reached) if reached == last_alpha => {
                                next_initialization = format!(
                                    "continuation from restored known-converged alpha={last_alpha:.6} deg after alpha={alpha:.6} failed"
                                );
                                bridged = true;
                            }
                            BridgeOutcome::Reached(reached) => {
                                last_converged_alpha = Some(reached);
                                next_initialization = format!(
                                    "continuation from bridged intermediate state alpha={reached:.6} deg \
                                     (bounded steps from known-converged alpha={last_alpha:.6} deg) after alpha={alpha:.6} failed"
                                );
                                bridged = true;
                            }
                            BridgeOutcome::Cancelled => {
                                outcome.cancelled = true;
                                break;
                            }
                            BridgeOutcome::Exhausted => {}
                        }
                    }
                    if !bridged {
                        if self.run_mset_or_cancel(dir, next_alpha, cancel)? {
                            outcome.cancelled = true;
                            break;
                        }
                        next_initialization = format!(
                            "clean target-angle MSET mesh at next requested alpha={next_alpha:.6} deg after alpha={alpha:.6} failed"
                        );
                    }
                }
                index += 1;
                continue;
            }

            let plot = match exec::run_tool_with_cancel(
                &self.mplot_exe,
                &["case"],
                dir,
                deck::MPLOT_POLAR_KEYSTROKES,
                TIMEOUT_MPLOT_S,
                cancel,
            ) {
                Ok(run) => run,
                Err(RunError::Cancelled { .. }) => {
                    outcome.cancelled = true;
                    break;
                }
                Err(error) => return Err(MsesFailure::from_run(error)),
            };
            if !plot.status.success() {
                return Err(MsesFailure::solver(format!(
                    "mplot exited with {}",
                    plot.status
                )));
            }
            let summary = parse::parse_polar_summary(&plot.stdout).map_err(MsesFailure::parse)?;

            // Save the known-good converged state for subsequent points.
            save_converged_state(dir);
            last_converged_alpha = Some(alpha);
            recovery.points[index] = Some(RecoveredPolarPoint {
                summary,
                checkpoint: self.checkpoint_from_converged(
                    dir,
                    alpha,
                    reynolds,
                    mach,
                    &solve.stdout,
                ),
                solver_output: solve.stdout,
            });

            if let Some(&next_alpha) = alphas.get(index + 1) {
                match self.bridge_to_target_with_recovery(
                    dir,
                    alpha,
                    next_alpha,
                    reynolds,
                    mach,
                    cancel,
                    &mut outcome.solver_attempts,
                    Some(&mut recovery),
                )? {
                    BridgeOutcome::Reached(reached) if reached == alpha => {
                        next_initialization = format!(
                            "continuation from prior converged requested alpha={alpha:.6} deg"
                        );
                    }
                    BridgeOutcome::Reached(reached) => {
                        last_converged_alpha = Some(reached);
                        next_initialization = format!(
                            "continuation from bridged intermediate state alpha={reached:.6} deg \
                             (bounded steps from prior converged requested alpha={alpha:.6} deg) toward requested alpha={next_alpha:.6} deg"
                        );
                    }
                    BridgeOutcome::Cancelled => {
                        outcome.cancelled = true;
                        break;
                    }
                    BridgeOutcome::Exhausted => {
                        if self.run_mset_or_cancel(dir, next_alpha, cancel)? {
                            outcome.cancelled = true;
                            break;
                        }
                        next_initialization = format!(
                            "clean target-angle MSET mesh at requested alpha={next_alpha:.6} deg"
                        );
                    }
                }
            }
            index += 1;
        }

        recovery.finish(&mut outcome);
        Ok(outcome)
    }

    // Explicit flow state, operating conditions and audit sinks belong to this solver boundary.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn bridge_to_target(
        &self,
        dir: &Path,
        from_alpha: f64,
        to_alpha: f64,
        reynolds: f64,
        mach: f64,
        cancel: Option<&AtomicBool>,
        solver_attempts: &mut Vec<MsesSolverAttempt>,
    ) -> Result<BridgeOutcome, MsesFailure> {
        self.bridge_to_target_with_recovery(
            dir,
            from_alpha,
            to_alpha,
            reynolds,
            mach,
            cancel,
            solver_attempts,
            None,
        )
    }

    // Explicit flow state, operating conditions and audit sinks belong to this solver boundary.
    #[allow(clippy::too_many_arguments)]
    fn bridge_to_target_with_recovery(
        &self,
        dir: &Path,
        from_alpha: f64,
        to_alpha: f64,
        reynolds: f64,
        mach: f64,
        cancel: Option<&AtomicBool>,
        solver_attempts: &mut Vec<MsesSolverAttempt>,
        mut recovery: Option<&mut RequestedPolarRecovery>,
    ) -> Result<BridgeOutcome, MsesFailure> {
        if !restore_converged_state(dir) {
            return Ok(BridgeOutcome::Exhausted);
        }
        let gap = to_alpha - from_alpha;
        if gap.abs() <= MAX_CONTINUATION_STEP_DEG {
            return Ok(BridgeOutcome::Reached(from_alpha));
        }
        let sign = gap.signum();
        let planned_hops = (gap.abs() / MAX_CONTINUATION_STEP_DEG).ceil() as usize - 1;
        if planned_hops == 0 || planned_hops > MAX_BRIDGE_HOPS {
            return Ok(BridgeOutcome::Exhausted);
        }

        let mut current = from_alpha;
        let mut hops_done = 0_usize;
        while (to_alpha - current).abs() > MAX_CONTINUATION_STEP_DEG {
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return Ok(BridgeOutcome::Cancelled);
            }
            if hops_done >= MAX_BRIDGE_HOPS {
                restore_converged_state(dir);
                return Ok(BridgeOutcome::Exhausted);
            }
            let mut step = MAX_CONTINUATION_STEP_DEG.min((to_alpha - current).abs());
            let mut hop_converged = false;
            // At most one full-step attempt and one halved-step retry per
            // hop position: a bounded budget, not an unbounded search.
            for _ in 0..2 {
                let target = current + sign * step;
                let hop = match self.run_mses_case(dir, target, reynolds, mach, cancel) {
                    Ok(run) => run,
                    Err(failure) if cancelled(&failure) => return Ok(BridgeOutcome::Cancelled),
                    Err(failure) => return Err(failure),
                };
                let status = if parse::is_converged(&hop.stdout) {
                    MsesPolarPointStatus::Converged
                } else {
                    MsesPolarPointStatus::NotConverged
                };
                solver_attempts.push(MsesSolverAttempt {
                    alpha_deg: target,
                    purpose: format!(
                        "bounded intermediate continuation bridge step to alpha={target:.6} deg \
                         (hop {} toward requested alpha={to_alpha:.6} deg)",
                        hops_done + 1
                    ),
                    status,
                    solver_output: hop.stdout.clone(),
                });
                if status == MsesPolarPointStatus::Converged {
                    if let Some(recovery) = recovery.as_deref_mut() {
                        if let Some(index) = recovery.missing_requested_index(target) {
                            // Extract now, before the next hop overwrites mdat.case.
                            // A solver convergence marker alone is not a polar row.
                            if let Ok(plot) = exec::run_tool_with_cancel(
                                &self.mplot_exe,
                                &["case"],
                                dir,
                                deck::MPLOT_POLAR_KEYSTROKES,
                                TIMEOUT_MPLOT_S,
                                cancel,
                            ) {
                                if plot.status.success() {
                                    if let Ok(summary) = parse::parse_polar_summary(&plot.stdout) {
                                        recovery.points[index] = Some(RecoveredPolarPoint {
                                            summary,
                                            checkpoint: self.checkpoint_from_converged(
                                                dir,
                                                target,
                                                reynolds,
                                                mach,
                                                &hop.stdout,
                                            ),
                                            solver_output: hop.stdout.clone(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    save_converged_state(dir);
                    current = target;
                    hop_converged = true;
                    break;
                }
                // Never let a diverged intermediate state pollute the next
                // attempt: restore the last known-good state before retrying
                // smaller from the same starting point.
                if !restore_converged_state(dir) {
                    return Ok(BridgeOutcome::Exhausted);
                }
                step /= 2.0;
                if step <= f64::EPSILON {
                    break;
                }
            }
            if !hop_converged {
                restore_converged_state(dir);
                return Ok(BridgeOutcome::Exhausted);
            }
            hops_done += 1;
        }
        Ok(BridgeOutcome::Reached(current))
    }
}

#[cfg(test)]
#[path = "sweep_tests.rs"]
mod tests;
