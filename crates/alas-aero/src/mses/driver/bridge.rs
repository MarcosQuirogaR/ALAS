// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bounded alpha-continuation bridge between a converged state and a target.

use super::recovery::{RecoveredPolarPoint, RequestedPolarRecovery};
use super::*;

impl Mses {
    // Explicit flow state, operating conditions and audit sinks belong to this solver boundary.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::mses::driver) fn bridge_to_target(
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
    pub(super) fn bridge_to_target_with_recovery(
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
        if !gap.is_finite() {
            // A NaN gap fails the guard above and `NaN as usize` is 0, which
            // would underflow the hop count below.
            return Ok(BridgeOutcome::Exhausted);
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
