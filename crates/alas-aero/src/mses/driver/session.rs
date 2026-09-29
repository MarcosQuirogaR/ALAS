// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// A configured MSES run on one already-repaneled section.
///
/// Holds the section, the solver settings read off [`MsesConfig`], and the
/// three executable paths. The section is taken already repaneled, as
/// native aerodynamic model's wrapper is constructed with `airfoil.repanel(...)`: the entry
/// points [`super::super::run_mses_polar`]/[`super::super::run_mses_pressure_distribution`]
/// repanel and build this, and the parity test builds it directly from the
/// coordinates the reference actually fed to `mset`.
pub struct Mses {
    pub(super) airfoil: Airfoil,
    pub(super) airfoil_name: String,
    pub(super) n_crit: f64,
    pub(super) xtr_upper: f64,
    pub(super) xtr_lower: f64,
    pub(super) max_iterations: i64,
    pub(super) mset_n: i64,
    pub(super) mset_e: f64,
    pub(super) mucon: f64,
    pub(super) timeout_mset_s: f64,
    pub(super) timeout_mses_s: f64,
    enabled: bool,
    mses_dir: PathBuf,
    pub(super) mset_exe: PathBuf,
    pub(super) mses_exe: PathBuf,
    pub(super) mplot_exe: PathBuf,
    osmap_required: bool,
    osmap_status: MsesOsmapStatus,
    pub(super) osmap_path: Option<PathBuf>,
    osmap_diagnostic: Option<String>,
}

impl Mses {
    /// Do not run free-transition iterations with a missing/incompatible
    /// Orr-Sommerfeld database. MSES otherwise returns zero amplification
    /// rates, which can look like convergence but is not the requested model.
    fn transition_preflight_error(&self) -> Option<String> {
        (self.osmap_required && self.osmap_status != MsesOsmapStatus::Available).then(|| {
            format!(
                "MSES free-transition analysis cannot start: {}. Supply the compatible double-precision osmapDP.dat from your MSES installation via mses.osmap_path or MSES_OSMAP. No solver retries were run; forced transition is a different physical assumption and is not applied automatically.",
                self.osmap_diagnostic.as_deref().unwrap_or("OSMAP database unavailable")
            )
        })
    }

    /// Build a driver from an already-repaneled section and its settings.
    pub fn new(airfoil: Airfoil, config: &MsesConfig, mses_dir: &Path) -> Self {
        let airfoil_name = if airfoil.name.is_empty() {
            "optimized_root_section".to_owned()
        } else {
            airfoil.name.clone()
        };
        let osmap = resolve_osmap(config, mses_dir);
        Self {
            airfoil,
            airfoil_name,
            n_crit: config.n_crit,
            xtr_upper: config.xtr_upper,
            xtr_lower: config.xtr_lower,
            max_iterations: config.max_iterations,
            mset_n: config.mset_n,
            mset_e: config.mset_e,
            mucon: config.mucon,
            timeout_mset_s: config.timeout_mset_s,
            timeout_mses_s: config.timeout_mses_s,
            enabled: config.enabled,
            mses_dir: mses_dir.to_path_buf(),
            mset_exe: mses_dir.join("mset.exe"),
            mses_exe: mses_dir.join("mses.exe"),
            mplot_exe: mses_dir.join("mplot.exe"),
            osmap_required: osmap.required,
            osmap_status: osmap.status,
            osmap_path: osmap.path,
            osmap_diagnostic: osmap.diagnostic,
        }
    }

    /// Run an alpha sweep and return the parsed polar.
    ///
    /// Never returns an error: any failure is reported through the result's
    /// `status`/`error`, as `run_mses_polar` does, so a pipeline run does not
    /// abort because one section would not converge.
    pub fn polar(&self, alphas: &[f64], reynolds: f64, mach: f64) -> MsesPolarResult {
        self.polar_with_cancel(alphas, reynolds, mach, None)
    }

    /// Run an alpha sweep while observing a cooperative cancellation flag.
    ///
    /// Completed points remain in the returned result when cancellation lands
    /// between solver calls, so the caller can still inspect the partial
    /// convergence evidence instead of losing the work already performed.
    pub fn polar_with_cancel(
        &self,
        alphas: &[f64],
        reynolds: f64,
        mach: f64,
        cancel: Option<&AtomicBool>,
    ) -> MsesPolarResult {
        let mut result = MsesPolarResult {
            airfoil_name: self.airfoil_name.clone(),
            mach,
            reynolds,
            requested_alpha_count: alphas.len(),
            osmap_required: self.osmap_required,
            osmap_status: self.osmap_status,
            osmap_path: self
                .osmap_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            osmap_diagnostic: self.osmap_diagnostic.clone(),
            ..MsesPolarResult::default()
        };
        if !self.enabled {
            return result.into_failure(
                MsesStatus::Disabled,
                "MSES analysis is disabled in configuration".to_owned(),
            );
        }
        if let Some(status) = super::super::installation_status(&self.mses_dir) {
            return result.into_failure(status, installation_message(&self.mses_dir, status));
        }

        if let Some(error) = self.transition_preflight_error() {
            return result.into_failure(MsesStatus::Incomplete, error);
        }

        let workdir = match WorkDir::new("alas_mses_") {
            Ok(dir) => dir,
            Err(error) => return result.into_error(error.to_string()),
        };
        let outcome = match self.solve_sweep(workdir.path(), alphas, reynolds, mach, cancel) {
            Ok(outcome) => outcome,
            Err(failure) => return result.into_failure(failure.status, failure.message),
        };
        result.point_diagnostics = outcome.point_diagnostics;
        result.solver_attempts = outcome.solver_attempts;
        result.checkpoints = outcome.checkpoints;

        let converged = outcome.accumulated.get("alpha").map_or(0, Vec::len);
        result.converged_alpha_count = converged;
        if converged == 0 {
            return result.into_error("MSES did not converge at any swept alpha".to_owned());
        }
        let [alpha_deg, cl, cd, cm, cdv, cdw, xtr_top, xtr_bot] =
            match validated_polar_columns(&outcome.accumulated, converged) {
                Ok(columns) => columns,
                Err(error) => return result.into_failure(MsesStatus::ParseFailure, error),
            };
        result.alpha_deg = alpha_deg;
        result.cl = cl;
        result.cd = cd;
        result.cm = cm;
        result.cdv = cdv;
        result.cdw = cdw;
        result.xtr_top = xtr_top;
        result.xtr_bot = xtr_bot;
        result.status = polar_completion_status(alphas.len(), converged);
        if result.status == MsesStatus::PartialConvergence {
            result.error = Some(if outcome.cancelled {
                format!(
                    "MSES sweep cancelled after {converged} of {} requested alpha points",
                    alphas.len()
                )
            } else {
                format!(
                    "MSES converged at {converged} of {} requested alpha points",
                    alphas.len()
                )
            });
        } else if outcome.cancelled {
            result.error =
                Some("MSES sweep cancelled after completing its requested points".to_owned());
        }
        result
    }

    /// Solve one point and read back its surface pressure and Mach distribution.
    ///
    /// Never returns an error, for the same reason [`Mses::polar`] does not.
    pub fn pressure(
        &self,
        alpha_deg: f64,
        reynolds: f64,
        mach: f64,
        retry_offsets: &[f64],
    ) -> MsesPressureResult {
        self.pressure_with_cancel(alpha_deg, reynolds, mach, retry_offsets, None)
    }

    /// Solve one pressure point while observing a cooperative cancellation
    /// flag.  A cancellation before a retry returns a diagnostic result;
    /// completed surface data are never replaced by fabricated values.
    pub fn pressure_with_cancel(
        &self,
        alpha_deg: f64,
        reynolds: f64,
        mach: f64,
        retry_offsets: &[f64],
        cancel: Option<&AtomicBool>,
    ) -> MsesPressureResult {
        self.pressure_with_checkpoint_and_cancel(
            alpha_deg,
            reynolds,
            mach,
            retry_offsets,
            None,
            cancel,
        )
    }

    /// Solve one pressure point, optionally warm-started from a genuinely
    /// converged [`MsesConvergedCheckpoint`] (typically one a prior
    /// [`Mses::polar_with_cancel`] call returned), while observing a
    /// cooperative cancellation flag.
    ///
    /// When `checkpoint` is `Some` and its geometry/config/Mach/Re/OSMAP
    /// identity matches this driver instance exactly (see
    /// [`MsesConvergedCheckpoint::matches`] via [`Mses::checkpoint_matches`]),
    /// it is tried first: the checkpoint's converged flowfield is restored
    /// and bridged toward the exact requested angle in bounded <=0.5 deg
    /// steps: cheaper than the cold clean-mesh retry-offset search below
    /// (no `mset` call at all), and it reuses the same bounded mechanism
    /// rather than inventing new solving logic. A `None` checkpoint, a
    /// mismatched one, or one that does not lead to a converged exact-target
    /// (or usable off-target) result all fall through unchanged to the
    /// existing cold-start retry-offset search, so this is purely additive:
    /// [`Mses::pressure_with_cancel`] behaves exactly as before by passing
    /// `None` here.
    pub fn pressure_with_checkpoint_and_cancel(
        &self,
        alpha_deg: f64,
        reynolds: f64,
        mach: f64,
        retry_offsets: &[f64],
        checkpoint: Option<&MsesConvergedCheckpoint>,
        cancel: Option<&AtomicBool>,
    ) -> MsesPressureResult {
        let mut result = MsesPressureResult {
            alpha_deg,
            requested_alpha_deg: alpha_deg,
            osmap_required: self.osmap_required,
            osmap_status: self.osmap_status,
            osmap_path: self
                .osmap_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            osmap_diagnostic: self.osmap_diagnostic.clone(),
            ..MsesPressureResult::default()
        };
        if !self.enabled {
            return result.into_failure(
                MsesStatus::Disabled,
                "MSES analysis is disabled in configuration".to_owned(),
            );
        }
        if let Some(status) = super::super::installation_status(&self.mses_dir) {
            return result.into_failure(status, installation_message(&self.mses_dir, status));
        }
        if let Some(error) = self.transition_preflight_error() {
            return result.into_failure(MsesStatus::Incomplete, error);
        }
        if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return result.into_error("MSES pressure analysis cancelled".to_owned());
        }

        if let Some(checkpoint) = checkpoint {
            if self.checkpoint_matches(checkpoint, mach, reynolds) {
                match self.pressure_from_checkpoint(
                    alpha_deg,
                    reynolds,
                    mach,
                    checkpoint,
                    cancel,
                    &mut result,
                ) {
                    CheckpointAttempt::Cancelled => {
                        return result.into_error("MSES pressure analysis cancelled".to_owned());
                    }
                    CheckpointAttempt::Solved(workdir, solved_alpha) => {
                        return self.finish_pressure_result(
                            result,
                            workdir.path(),
                            solved_alpha,
                            cancel,
                        );
                    }
                    // No usable anchor result: fall through to the existing
                    // cold-start retry-offset search below, unchanged. The
                    // checkpoint was a bonus, cheaper first attempt, not a
                    // replacement for the established recovery path.
                    CheckpointAttempt::NotUsable => {}
                }
            }
        }

        let mut attempt_failures: Vec<MsesFailure> = Vec::new();
        let mut winner: Option<(WorkDir, f64)> = None;
        for &offset in retry_offsets {
            let candidate = alpha_deg + offset;
            let workdir = match WorkDir::new("alas_mses_cp_") {
                Ok(dir) => dir,
                Err(error) => return result.into_error(error.to_string()),
            };
            match self.solve_sweep(workdir.path(), &[candidate], reynolds, mach, cancel) {
                Ok(outcome) if outcome.accumulated.get("alpha").map_or(0, Vec::len) > 0 => {
                    result.solver_attempts.extend(outcome.solver_attempts);
                    winner = Some((workdir, candidate));
                    break;
                }
                Ok(outcome) if outcome.cancelled => {
                    result.solver_attempts.extend(outcome.solver_attempts);
                    return result.into_error("MSES pressure analysis cancelled".to_owned());
                }
                Ok(outcome) => {
                    result.solver_attempts.extend(outcome.solver_attempts);
                    attempt_failures.push(MsesFailure::solver(format!(
                        "alpha={candidate:.2}: did not converge"
                    )));
                }
                Err(failure) if cancelled(&failure) => {
                    return result.into_error("MSES pressure analysis cancelled".to_owned());
                }
                Err(mut failure) => {
                    failure.message = format!("alpha={candidate:.2}: {}", failure.message);
                    attempt_failures.push(failure);
                }
            }
        }

        let (workdir, mut converged_alpha) = match winner {
            Some(pair) => pair,
            None => {
                let tried = retry_offsets
                    .iter()
                    .map(|offset| format!("{:.2}", alpha_deg + offset))
                    .collect::<Vec<_>>()
                    .join(", ");
                let last = attempt_failures
                    .last()
                    .map(|failure| failure.message.clone())
                    .unwrap_or_else(|| "unknown".to_owned());
                let status = attempt_failures
                    .last()
                    .map_or(MsesStatus::Error, |failure| failure.status);
                return result.into_failure(
                    status,
                    format!(
                        "MSES did not converge at alpha={alpha_deg:.2} deg or any retry offset \
                     (tried: {tried} deg). Last error: {last}"
                    ),
                );
            }
        };

        // A genuinely converged offset candidate is real evidence, but it is
        // not the requested condition. Try to bridge back to the exact
        // requested angle in bounded <=0.5 deg steps before accepting it as
        // an explicitly off-target result: this is the same anchor-and-hop
        // mechanism the polar sweep uses, applied to the pressure entry
        // point's own single converged candidate.
        if converged_alpha != alpha_deg {
            let anchor_src = workdir.path().join("mdat.case");
            let anchor_dst = workdir.path().join("mdat.case.pressure_anchor");
            let anchor_saved =
                anchor_src.is_file() && std::fs::copy(&anchor_src, &anchor_dst).is_ok();
            if anchor_saved {
                match self.bridge_to_target(
                    workdir.path(),
                    converged_alpha,
                    alpha_deg,
                    reynolds,
                    mach,
                    cancel,
                    &mut result.solver_attempts,
                ) {
                    Ok(BridgeOutcome::Cancelled) => {
                        return result.into_error("MSES pressure analysis cancelled".to_owned());
                    }
                    Ok(BridgeOutcome::Reached(_)) => {
                        match self.run_mses_case(workdir.path(), alpha_deg, reynolds, mach, cancel)
                        {
                            Ok(run) => {
                                let status = if parse::is_converged(&run.stdout) {
                                    MsesPolarPointStatus::Converged
                                } else {
                                    MsesPolarPointStatus::NotConverged
                                };
                                result.solver_attempts.push(MsesSolverAttempt {
                                    alpha_deg,
                                    purpose: format!(
                                        "exact-target continuation solve at requested alpha={alpha_deg:.6} deg \
                                         after bridging from converged offset alpha={converged_alpha:.6} deg"
                                    ),
                                    status,
                                    solver_output: run.stdout,
                                });
                                if status == MsesPolarPointStatus::Converged {
                                    // The exact requested angle now has its own
                                    // genuinely converged state in `workdir`;
                                    // MPlot extracts from this, not the offset.
                                    converged_alpha = alpha_deg;
                                } else {
                                    // Never extract from a failed exact-target
                                    // attempt: restore the known-good off-target
                                    // anchor so mplot reads a converged state.
                                    let _ = std::fs::copy(&anchor_dst, &anchor_src);
                                }
                            }
                            Err(failure) if cancelled(&failure) => {
                                return result
                                    .into_error("MSES pressure analysis cancelled".to_owned());
                            }
                            Err(_) => {
                                let _ = std::fs::copy(&anchor_dst, &anchor_src);
                            }
                        }
                    }
                    // A bridge that could not reach the target, or a hard
                    // failure in the bridge itself, both fall back the same
                    // way: this is a bonus best-effort recovery, not a
                    // reason to discard an already-known-good converged
                    // candidate.
                    Ok(BridgeOutcome::Exhausted) | Err(_) => {
                        let _ = std::fs::copy(&anchor_dst, &anchor_src);
                    }
                }
            }
        }
        self.finish_pressure_result(result, workdir.path(), converged_alpha, cancel)
    }
}
