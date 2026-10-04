// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

impl Mses {
    /// Extract the final surface/flowfield tables from `dir`'s genuinely
    /// converged `mdat.case` at `converged_alpha`, and fold them into
    /// `result`.
    ///
    /// Shared by every path that can produce a converged pressure state:
    /// the cold-start retry-offset search, its bridge-back recovery, and the
    /// checkpoint-anchored warm start, so MPlot extraction always runs
    /// against whichever state actually ended up converged, never against an
    /// intermediate or mismatched one.
    pub(super) fn finish_pressure_result(
        &self,
        mut result: MsesPressureResult,
        dir: &Path,
        converged_alpha: f64,
        cancel: Option<&AtomicBool>,
    ) -> MsesPressureResult {
        result.alpha_deg = converged_alpha;

        let dump_name = "bl_dump.txt";
        let _dump = match exec::run_tool_with_cancel(
            &self.mplot_exe,
            &["case"],
            dir,
            &deck::mplot_dump_keystrokes(12, dump_name),
            self.timeout_mses_s,
            cancel,
        ) {
            Ok(run) if run.status.success() => run,
            Ok(run) => {
                return result.into_error(format!("mplot exited with {}", run.status));
            }
            Err(error) => {
                let failure = MsesFailure::from_run(error);
                return result.into_failure(failure.status, failure.message);
            }
        };
        let dump_path = dir.join(dump_name);
        if !dump_path.exists() {
            return result.into_failure(
                MsesStatus::ParseFailure,
                "mplot did not produce a BL dump file".to_owned(),
            );
        }

        let flowfield_name = "flowfield.txt";
        let _flowfield = match exec::run_tool_with_cancel(
            &self.mplot_exe,
            &["case"],
            dir,
            &deck::mplot_dump_keystrokes(11, flowfield_name),
            self.timeout_mses_s,
            cancel,
        ) {
            // Python does not check option 11's return code: the flow field is
            // an optional figure export, while the option-12 surface result is
            // already a valid converged analysis.
            Ok(run) => run,
            Err(error) => {
                let failure = MsesFailure::from_run(error);
                return result.into_failure(failure.status, failure.message);
            }
        };
        let flowfield_path = dir.join(flowfield_name);

        let dump_text = read_lossy(&dump_path);
        let flowfield_text = if flowfield_path.exists() {
            read_lossy(&flowfield_path)
        } else {
            String::new()
        };
        match MsesPressureResult::replay_raw_exports(
            converged_alpha,
            dump_text.clone(),
            flowfield_text.clone(),
            &self.airfoil.coordinates,
        ) {
            Ok(mut parsed) => {
                // Keep the failure branch able to retain its raw exports too.
                // The attempt transcripts are small relative to the solver
                // tables, and cloning here avoids partially moving `result`
                // before the parser-failure branch can attach those tables.
                parsed.solver_attempts = result.solver_attempts.clone();
                parsed.requested_alpha_deg = result.requested_alpha_deg;
                parsed.osmap_required = result.osmap_required;
                parsed.osmap_status = result.osmap_status;
                parsed.osmap_path = result.osmap_path.clone();
                parsed.osmap_diagnostic = result.osmap_diagnostic.clone();
                parsed
            }
            Err(error) => {
                result.raw_bl_dump = dump_text;
                result.raw_flowfield_dump = flowfield_text;
                result.into_failure(MsesStatus::ParseFailure, error.to_string())
            }
        }
    }

    /// Mesh once at the first angle, solve each in turn, accumulate the parsed
    /// summary of every angle that converged.
    /// Write the requested operating-point deck and run one `mses` solve.
    ///
    /// Keeping this process boundary in one helper makes continuation attempts
    /// use precisely the same deck and timeout as ordinary requested points.
    pub(super) fn run_mses_case(
        &self,
        dir: &Path,
        alpha: f64,
        reynolds: f64,
        mach: f64,
        cancel: Option<&AtomicBool>,
    ) -> Result<exec::ToolRun, MsesFailure> {
        std::fs::write(
            dir.join("mses.case"),
            deck::mses_case_with_mucon(
                mach,
                alpha,
                reynolds,
                self.n_crit,
                self.xtr_lower,
                self.xtr_upper,
                self.mucon,
            ),
        )
        .map_err(|error| MsesFailure::solver(error.to_string()))?;
        let osmap_value = self
            .osmap_path
            .as_ref()
            .map(|path| path.as_os_str())
            .unwrap_or_else(|| OsStr::new(""));
        exec::run_tool_with_env_and_cancel(
            &self.mses_exe,
            &["case"],
            dir,
            &deck::mses_keystrokes(self.max_iterations),
            self.timeout_mses_s,
            cancel,
            &[("MSES_OSMAP", osmap_value)],
        )
        .map_err(MsesFailure::from_run)
        .and_then(|run| {
            if run.status.success() {
                Ok(run)
            } else {
                Err(MsesFailure::solver(format!(
                    "mses exited with {}",
                    run.status
                )))
            }
        })
    }

    /// Generate the mesh at `mset_alpha`, distinguishing cancellation from a
    /// hard failure.
    ///
    /// `solve_sweep`'s retry and continuation paths call `run_mset` after
    /// points may have already converged; propagating a plain `Err` on
    /// cancellation there would discard that already-accumulated evidence
    /// instead of returning it as a cancelled partial result. Returns
    /// `Ok(true)` when the run was cancelled (the caller should stop the sweep
    /// and keep what it has), `Ok(false)` on a successful mesh.
    pub(super) fn run_mset_or_cancel(
        &self,
        dir: &Path,
        mset_alpha: f64,
        cancel: Option<&AtomicBool>,
    ) -> Result<bool, MsesFailure> {
        match self.run_mset(dir, mset_alpha, cancel) {
            Ok(()) => Ok(false),
            Err(failure) if cancelled(&failure) => Ok(true),
            Err(failure) => Err(failure),
        }
    }

    /// Capture a reusable checkpoint from `dir`'s just-converged `mdat.case`.
    ///
    /// Returns `None` only on an I/O failure reading the state file back;
    /// the caller already confirmed native convergence and a successful
    /// MPlot extraction before calling this, so a `None` here never hides a
    /// non-converged point as a checkpoint.
    pub(super) fn checkpoint_from_converged(
        &self,
        dir: &Path,
        alpha: f64,
        reynolds: f64,
        mach: f64,
        solver_output: &str,
    ) -> Option<MsesConvergedCheckpoint> {
        let mdat_case = std::fs::read(dir.join("mdat.case")).ok()?;
        Some(MsesConvergedCheckpoint {
            airfoil_name: self.airfoil_name.clone(),
            airfoil_coordinates: self.airfoil.coordinates.clone(),
            n_crit: self.n_crit,
            xtr_upper: self.xtr_upper,
            xtr_lower: self.xtr_lower,
            mset_n: self.mset_n,
            mset_e: self.mset_e,
            mucon: self.mucon,
            max_iterations: self.max_iterations,
            mach,
            reynolds,
            osmap_path: self.osmap_path.clone(),
            alpha_deg: alpha,
            solver_output: solver_output.to_owned(),
            mdat_case,
        })
    }

    /// Whether this driver instance may safely reuse `checkpoint` as a
    /// warm-start anchor for a solve at `mach`/`reynolds`.
    ///
    /// Every field that affects the physical solve: geometry, the solver
    /// knobs baked into the deck (`n_crit`, transition, `mset_n`/`mset_e`,
    /// `mucon`, the iteration cap), Mach, Reynolds, and the resolved OSMAP
    /// path, must match exactly. There is no tolerance: a mismatch on any
    /// of these means the checkpoint's `mdat.case` was built for a different
    /// problem, and reusing it would be exactly the stale-foreign-mdat reuse
    /// this check exists to prevent.
    pub(super) fn checkpoint_matches(
        &self,
        checkpoint: &MsesConvergedCheckpoint,
        mach: f64,
        reynolds: f64,
    ) -> bool {
        checkpoint.airfoil_name == self.airfoil_name
            && checkpoint.airfoil_coordinates == self.airfoil.coordinates
            && checkpoint.n_crit == self.n_crit
            && checkpoint.xtr_upper == self.xtr_upper
            && checkpoint.xtr_lower == self.xtr_lower
            && checkpoint.mset_n == self.mset_n
            && checkpoint.mset_e == self.mset_e
            && checkpoint.mucon == self.mucon
            && checkpoint.max_iterations == self.max_iterations
            && checkpoint.mach == mach
            && checkpoint.reynolds == reynolds
            && checkpoint.osmap_path == self.osmap_path
    }

    /// Write `checkpoint`'s geometry and converged `mdat.case` into `dir` so
    /// it can be used as a bridge anchor, without ever running `mset`.
    fn restore_checkpoint_into(&self, dir: &Path, checkpoint: &MsesConvergedCheckpoint) -> bool {
        if std::fs::write(
            dir.join("blade.case"),
            deck::mset_blade(&self.airfoil.name, &self.airfoil.coordinates),
        )
        .is_err()
        {
            return false;
        }
        if std::fs::write(dir.join("mdat.case"), &checkpoint.mdat_case).is_err() {
            return false;
        }
        save_converged_state(dir)
    }

    /// Try `checkpoint` as a warm-start anchor for a pressure solve at the
    /// exact requested `alpha_deg`: restore its converged flowfield (no
    /// `mset` call), bridge toward the target in bounded <=0.5 deg steps,
    /// then attempt one official solve at the exact angle.
    ///
    /// Every attempt (successful or not) is appended to `result.solver_attempts`
    /// for audit before this returns, regardless of outcome.
    pub(super) fn pressure_from_checkpoint(
        &self,
        alpha_deg: f64,
        reynolds: f64,
        mach: f64,
        checkpoint: &MsesConvergedCheckpoint,
        cancel: Option<&AtomicBool>,
        result: &mut MsesPressureResult,
    ) -> CheckpointAttempt {
        let workdir = match WorkDir::new("alas_mses_cp_") {
            Ok(dir) => dir,
            Err(_) => return CheckpointAttempt::NotUsable,
        };
        if !self.restore_checkpoint_into(workdir.path(), checkpoint) {
            return CheckpointAttempt::NotUsable;
        }
        match self.bridge_to_target(
            workdir.path(),
            checkpoint.alpha_deg,
            alpha_deg,
            reynolds,
            mach,
            cancel,
            &mut result.solver_attempts,
        ) {
            Ok(BridgeOutcome::Cancelled) => return CheckpointAttempt::Cancelled,
            Ok(BridgeOutcome::Exhausted) | Err(_) => return CheckpointAttempt::NotUsable,
            Ok(BridgeOutcome::Reached(_)) => {}
        }
        match self.run_mses_case(workdir.path(), alpha_deg, reynolds, mach, cancel) {
            Ok(run) => {
                let status = if parse::is_converged(&run.stdout) {
                    MsesPolarPointStatus::Converged
                } else {
                    MsesPolarPointStatus::NotConverged
                };
                result.solver_attempts.push(MsesSolverAttempt {
                    alpha_deg,
                    purpose: format!(
                        "exact-target solve at requested alpha={alpha_deg:.6} deg from reused \
                         converged polar checkpoint (originally converged at alpha={:.6} deg)",
                        checkpoint.alpha_deg
                    ),
                    status,
                    solver_output: run.stdout,
                });
                if status == MsesPolarPointStatus::Converged {
                    CheckpointAttempt::Solved(workdir, alpha_deg)
                } else {
                    CheckpointAttempt::NotUsable
                }
            }
            Err(failure) if cancelled(&failure) => CheckpointAttempt::Cancelled,
            Err(_) => CheckpointAttempt::NotUsable,
        }
    }

    /// Generate the mesh at `mset_alpha`.
    pub(super) fn run_mset(
        &self,
        dir: &Path,
        mset_alpha: f64,
        cancel: Option<&AtomicBool>,
    ) -> Result<(), MsesFailure> {
        let run = exec::run_tool_with_cancel(
            &self.mset_exe,
            &["case"],
            dir,
            &deck::mset_keystrokes(self.mset_n, self.mset_e, mset_alpha),
            self.timeout_mset_s,
            cancel,
        )
        .map_err(MsesFailure::from_run)?;
        if !run.status.success() {
            return Err(MsesFailure::solver(format!(
                "mset exited with {}",
                run.status
            )));
        }
        Ok(())
    }
}
