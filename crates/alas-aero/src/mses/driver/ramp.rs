// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mach continuation for a cold-started MSES solve.
//!
//! MSET initialises the flowfield from an isentropic streamline solution and
//! MSES then has to find the viscous, shocked solution at the target Mach and
//! Reynolds numbers in one Newton march. At the transonic section Mach numbers
//! of swept transport wings (0.63-0.72 normal to the quarter chord) and
//! Reynolds numbers of 2e7-1e8 that march often diverges (NaN residuals) or
//! limit-cycles even though a steady solution exists. The MSES user guide's
//! standard remedy for a hard case is to converge an easy condition first and
//! walk to the hard one, restarting each solve from the previous state. A
//! fixed-Re Mach walk from M=0.3 (which converges in under 15 Newton
//! iterations for the retained sections) removes the initialisation failure;
//! where the walk stalls, the section has no steady solution at that Mach
//! (shock-induced separation) and the point stays honestly unconverged.

use super::*;

/// Mach number of the always-convergent first stage of a ramp.
pub(super) const RAMP_START_MACH: f64 = 0.3;
/// Largest Mach increment between ramp stages.
const RAMP_MAX_STEP: f64 = 0.1;
/// Smallest Mach increment tried before the ramp declares a stall: three
/// halvings of [`RAMP_MAX_STEP`]. A section that cannot be advanced by 0.0125
/// in Mach is at its steady-solution boundary (shock-induced separation), and
/// every further halving costs a full diverging solve.
const RAMP_MIN_STEP: f64 = RAMP_MAX_STEP / 8.0;
/// Resource bound on the number of solver runs one ramp may take.
const RAMP_MAX_RUNS: usize = 16;
/// Newton iterations allowed to each intermediate ramp stage. Converging
/// stages need 10-40; a stage that has not converged by 60 is diverging, so a
/// longer budget only delays the halved retry. The first (M=0.3) stage and
/// the final stage use the configured budget: the first can converge slowly
/// on a thick aft-loaded section (residual 3e-5 at 60 iterations).
const RAMP_STAGE_ITERATIONS: i64 = 60;

/// Pure step controller for the Mach walk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct MachRamp {
    /// Mach number of the last converged stage.
    pub(super) mach: f64,
    step: f64,
    target: f64,
}

impl MachRamp {
    /// Start a ramp at the always-convergent stage. `None` when the target is
    /// already easy enough that the direct solve needs no help.
    pub(super) fn new(target: f64) -> Option<Self> {
        (target.is_finite() && target > RAMP_START_MACH + RAMP_MAX_STEP / 2.0).then_some(Self {
            mach: RAMP_START_MACH,
            step: RAMP_MAX_STEP,
            target,
        })
    }

    /// Mach number of the next stage to attempt.
    pub(super) fn next_mach(&self) -> f64 {
        (self.mach + self.step).min(self.target)
    }

    /// Whether the next stage is the target itself.
    pub(super) fn next_is_target(&self) -> bool {
        self.next_mach() >= self.target
    }

    /// Record a converged stage and grow the step again (bounded).
    pub(super) fn converged(&mut self) {
        self.mach = self.next_mach();
        self.step = (self.step * 1.5).min(RAMP_MAX_STEP);
    }

    /// Record a failed stage. `false` when the step is exhausted (stall).
    pub(super) fn failed(&mut self) -> bool {
        self.step /= 2.0;
        self.step >= RAMP_MIN_STEP - f64::EPSILON
    }
}

/// Result of [`Mses::mach_ramp_cold_start`].
pub(super) enum RampOutcome {
    /// The target-Mach solve converged; its transcript is carried.
    Converged(exec::ToolRun),
    /// The walk stalled below the target Mach or ran out of budget;
    /// `mdat.case` holds an unusable state.
    Stalled,
    /// Cancellation landed; stop the sweep.
    Cancelled,
}

impl Mses {
    /// Decide whether a failed direct cold start earns one Mach-walk retry,
    /// and run it. `None` when it does not apply: the direct solve converged,
    /// the start was a continuation from a bridged state (its neighbour
    /// already carries the right flow regime), or the target Mach is easy.
    /// Otherwise MSET is re-run for `alpha` and the walk starts from a fresh
    /// mesh.
    // Explicit flow state, operating conditions and audit sinks belong to this solver boundary.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn cold_start_ramp_retry(
        &self,
        dir: &Path,
        alpha: f64,
        reynolds: f64,
        mach: f64,
        purpose: &str,
        direct_stdout: &str,
        cancel: Option<&AtomicBool>,
        attempts: &mut Vec<MsesSolverAttempt>,
    ) -> Result<Option<RampOutcome>, MsesFailure> {
        let cold = purpose.contains("initial MSET") || purpose.contains("clean target-angle");
        if parse::is_converged(direct_stdout) || !cold || MachRamp::new(mach).is_none() {
            return Ok(None);
        }
        if self.run_mset_or_cancel(dir, alpha, cancel)? {
            return Ok(Some(RampOutcome::Cancelled));
        }
        self.mach_ramp_cold_start(dir, alpha, reynolds, mach, cancel, attempts)
            .map(Some)
    }

    /// Walk the cold-started mesh in `dir` from [`RAMP_START_MACH`] to `mach`
    /// at fixed `alpha` and Reynolds number. The caller must have just run
    /// MSET for `alpha`. Every stage is recorded in `attempts`.
    pub(super) fn mach_ramp_cold_start(
        &self,
        dir: &Path,
        alpha: f64,
        reynolds: f64,
        mach: f64,
        cancel: Option<&AtomicBool>,
        attempts: &mut Vec<MsesSolverAttempt>,
    ) -> Result<RampOutcome, MsesFailure> {
        let Some(mut ramp) = MachRamp::new(mach) else {
            return Ok(RampOutcome::Stalled);
        };
        let backup = dir.join("mdat.case.ramp");
        let mut runs = 0_usize;
        let mut have_backup = false;
        loop {
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return Ok(RampOutcome::Cancelled);
            }
            let is_target = have_backup && ramp.next_is_target();
            let stage_mach = if have_backup {
                ramp.next_mach()
            } else {
                RAMP_START_MACH
            };
            let iterations = if is_target || !have_backup {
                self.max_iterations
            } else {
                RAMP_STAGE_ITERATIONS.min(self.max_iterations)
            };
            let run = match self
                .run_mses_case_with_iterations(dir, alpha, reynolds, stage_mach, iterations, cancel)
            {
                Ok(run) => run,
                Err(failure) if cancelled(&failure) => return Ok(RampOutcome::Cancelled),
                Err(failure) => return Err(failure),
            };
            runs += 1;
            let converged = parse::is_converged(&run.stdout);
            attempts.push(MsesSolverAttempt {
                alpha_deg: alpha,
                purpose: format!(
                    "Mach continuation ramp stage M={stage_mach:.4} toward M={mach:.4} at alpha={alpha:.6} deg"
                ),
                status: if converged {
                    MsesPolarPointStatus::Converged
                } else {
                    MsesPolarPointStatus::NotConverged
                },
                solver_output: run.stdout.clone(),
            });
            if converged {
                if is_target {
                    return Ok(RampOutcome::Converged(run));
                }
                if have_backup {
                    ramp.converged();
                }
                have_backup = std::fs::copy(dir.join("mdat.case"), &backup).is_ok();
                if !have_backup {
                    return Ok(RampOutcome::Stalled);
                }
            } else {
                if !have_backup {
                    // Even the M=0.3 stage failed: nothing to restart from.
                    return Ok(RampOutcome::Stalled);
                }
                if std::fs::copy(&backup, dir.join("mdat.case")).is_err() || !ramp.failed() {
                    return Ok(RampOutcome::Stalled);
                }
            }
            if runs >= RAMP_MAX_RUNS {
                return Ok(RampOutcome::Stalled);
            }
        }
    }
}

#[cfg(test)]
// Test fixtures fail at the assertion site when a controller invariant breaks.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn easy_targets_need_no_ramp() {
        assert!(MachRamp::new(0.3).is_none());
        assert!(MachRamp::new(0.34).is_none());
        assert!(MachRamp::new(f64::NAN).is_none());
        assert!(MachRamp::new(0.44).is_some());
    }

    #[test]
    fn ramp_walks_in_bounded_steps_and_ends_exactly_on_the_target() {
        let mut ramp = MachRamp::new(0.695).expect("transonic target");
        let mut stages = Vec::new();
        while !ramp.next_is_target() {
            stages.push(ramp.next_mach());
            ramp.converged();
        }
        assert_eq!(ramp.next_mach(), 0.695);
        assert!(stages
            .windows(2)
            .all(|pair| pair[1] - pair[0] <= 0.1 + 1e-12));
        assert!(stages.len() <= 6, "{stages:?}");
    }

    #[test]
    fn ramp_halves_after_failures_and_stalls_at_the_minimum_step() {
        let mut ramp = MachRamp::new(0.7).expect("transonic target");
        assert!(ramp.failed());
        assert!((ramp.next_mach() - 0.35).abs() < 1e-12);
        assert!(ramp.failed());
        assert!(ramp.failed());
        assert!(
            !ramp.failed(),
            "three halvings of 0.1 reach the 0.0125 floor"
        );
    }
}
