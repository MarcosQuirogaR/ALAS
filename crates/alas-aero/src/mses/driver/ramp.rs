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
/// Smallest Mach increment tried before the ramp declares a stall: five
/// halvings of [`RAMP_MAX_STEP`] (0.003125). Three halvings (0.0125) stalled
/// the A340-300 root at M 0.639-0.655 (Re 5.7e7) although a steady solution
/// exists up to its design M 0.714: restarted from the last converged Mach with
/// steps down to 0.003 the walk reaches the target in about 30 runs. Every
/// halving costs one diverging solve of a few seconds.
const RAMP_MIN_STEP: f64 = RAMP_MAX_STEP / 32.0;
/// Resource bound on the number of solver runs one ramp may take.
const RAMP_MAX_RUNS: usize = 48;
/// Grid-point offsets (added to the configured `mset_n`) tried, in order, when
/// a section's Mach walk stalls on the configured mesh. A stalled walk with a
/// small, constant residual (1e-5 on the DC-10 root at M 0.3, Re 7.6e7) or a
/// stall at low Mach (A400M root, M 0.39) is a transition-node limit cycle
/// tied to where the nodes fall, not a flow limit: a different node count
/// converges the same section (DC-10 root 161, DC-10 kink 171, A400M root
/// 161 points). The mesh density is a numerical setting, not a physical
/// assumption; every retry is recorded in the solver attempts.
const MESH_RETRY_OFFSETS: [i64; 5] = [20, -20, 30, 10, -10];
/// Angle offset of the lower-lift anchor used when a cold point's Mach walk
/// stalls at its own angle. Two bounded continuation steps
/// ([`MAX_CONTINUATION_STEP_DEG`] each) below the requested angle.
const CL_ANCHOR_OFFSET_DEG: f64 = 2.0 * MAX_CONTINUATION_STEP_DEG;
/// Newton iterations allowed to each intermediate ramp stage. Converging
/// stages need 10-40; a stage that has not converged by 60 is diverging, so a
/// longer budget only delays the halved retry. The first (M=0.3) stage and
/// the final stage use the configured budget: the first can converge slowly
/// on a thick aft-loaded section (residual 3e-5 at 60 iterations).
const RAMP_STAGE_ITERATIONS: i64 = 60;

/// Fewest surface points a grid retry may use.
const MIN_RETRY_MESH_POINTS: i64 = 81;

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
        match self.mach_ramp_cold_start(dir, alpha, reynolds, mach, cancel, attempts)? {
            RampOutcome::Stalled => {
                match self.cl_anchored_cold_start(dir, alpha, reynolds, mach, cancel, attempts)? {
                    RampOutcome::Stalled => self
                        .mesh_retry_cold_start(dir, alpha, reynolds, mach, cancel, attempts)
                        .map(Some),
                    outcome => Ok(Some(outcome)),
                }
            }
            outcome => Ok(Some(outcome)),
        }
    }

    /// Repeat the Mach walk on meshes with a different number of surface
    /// points ([`MESH_RETRY_OFFSETS`]) after the configured mesh stalled. The
    /// first mesh whose walk reaches the target wins; if none does, the last
    /// state is unusable and the caller sees `Stalled`.
    pub(super) fn mesh_retry_cold_start(
        &self,
        dir: &Path,
        alpha: f64,
        reynolds: f64,
        mach: f64,
        cancel: Option<&AtomicBool>,
        attempts: &mut Vec<MsesSolverAttempt>,
    ) -> Result<RampOutcome, MsesFailure> {
        for offset in MESH_RETRY_OFFSETS {
            let points = self.mset_n + offset;
            if points < MIN_RETRY_MESH_POINTS {
                continue;
            }
            match self.run_mset_with_points(dir, alpha, points, cancel) {
                Ok(()) => {}
                Err(failure) if cancelled(&failure) => return Ok(RampOutcome::Cancelled),
                Err(failure) => return Err(failure),
            }
            attempts.push(MsesSolverAttempt {
                alpha_deg: alpha,
                purpose: format!(
                    "grid retry: MSET re-run with n={points} surface points (configured {}) after the Mach walk stalled",
                    self.mset_n
                ),
                status: MsesPolarPointStatus::NotConverged,
                solver_output: String::new(),
            });
            match self.mach_ramp_cold_start(dir, alpha, reynolds, mach, cancel, attempts)? {
                RampOutcome::Stalled => {}
                outcome => return Ok(outcome),
            }
        }
        Ok(RampOutcome::Stalled)
    }

    /// Lower-lift start for a cold point whose Mach walk stalled at its own
    /// angle. The walk is repeated [`CL_ANCHOR_OFFSET_DEG`] below the requested
    /// angle, where the shock is weaker, the solution is then carried up to the
    /// requested angle by bounded alpha bridging at the target Mach, and the
    /// requested angle is solved from that converged state. This is the MSES
    /// guide's standard order for a hard case: converge the lower-CL condition
    /// first and step the angle up. A stall at any stage returns `Stalled`.
    pub(super) fn cl_anchored_cold_start(
        &self,
        dir: &Path,
        alpha: f64,
        reynolds: f64,
        mach: f64,
        cancel: Option<&AtomicBool>,
        attempts: &mut Vec<MsesSolverAttempt>,
    ) -> Result<RampOutcome, MsesFailure> {
        let anchor = alpha - CL_ANCHOR_OFFSET_DEG;
        if self.run_mset_or_cancel(dir, anchor, cancel)? {
            return Ok(RampOutcome::Cancelled);
        }
        match self.mach_ramp_cold_start(dir, anchor, reynolds, mach, cancel, attempts)? {
            RampOutcome::Converged(_) => {}
            RampOutcome::Stalled => return Ok(RampOutcome::Stalled),
            RampOutcome::Cancelled => return Ok(RampOutcome::Cancelled),
        }
        if !save_converged_state(dir) {
            return Ok(RampOutcome::Stalled);
        }
        match self.bridge_to_target(dir, anchor, alpha, reynolds, mach, cancel, attempts)? {
            BridgeOutcome::Reached(_) => {}
            BridgeOutcome::Exhausted => return Ok(RampOutcome::Stalled),
            BridgeOutcome::Cancelled => return Ok(RampOutcome::Cancelled),
        }
        let run = match self.run_mses_case(dir, alpha, reynolds, mach, cancel) {
            Ok(run) => run,
            Err(failure) if cancelled(&failure) => return Ok(RampOutcome::Cancelled),
            Err(failure) => return Err(failure),
        };
        let converged = parse::is_converged(&run.stdout);
        attempts.push(MsesSolverAttempt {
            alpha_deg: alpha,
            purpose: format!(
                "lower-lift anchored start: converged at M={mach:.4} from alpha={anchor:.6} deg,                  bridged to alpha={alpha:.6} deg"
            ),
            status: if converged {
                MsesPolarPointStatus::Converged
            } else {
                MsesPolarPointStatus::NotConverged
            },
            solver_output: run.stdout.clone(),
        });
        Ok(if converged {
            RampOutcome::Converged(run)
        } else {
            RampOutcome::Stalled
        })
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
        for _ in 0..4 {
            assert!(ramp.failed());
        }
        assert!(
            !ramp.failed(),
            "the sixth halving of 0.1 goes below the 0.003125 floor"
        );
    }
}
