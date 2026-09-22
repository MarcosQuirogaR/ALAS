// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use alas_config::MsesConfig;
use alas_geom::aircraft::airfoil::Airfoil;

use super::exec::{self, RunError, WorkDir};
use super::{
    deck, parse, MsesConvergedCheckpoint, MsesOsmapStatus, MsesPolarPointDiagnostic,
    MsesPolarPointStatus, MsesPolarResult, MsesPressureResult, MsesSolverAttempt, MsesStatus,
};

/// The `mplot` timeout for the polar summary dump, native aerodynamic model's `timeout_mplot`
/// default (`mses_analysis.py` overrides only the `mset` and `mses` timeouts).
const TIMEOUT_MPLOT_S: f64 = 10.0;

/// Maximum allowable incidence delta (degrees) for MSES continuation.
///
/// A continuation step reuses the existing mesh and flowfield (`mdat.case`) as
/// the initial guess for a different angle of attack, rather than generating a
/// fresh isentropic mesh at the target angle. MSES continuation guidance calls
/// for increments no larger than 0.5 deg in general, tightened to 0.25-0.1 deg
/// near critical Mach, where Newton under-relaxation and shock-induced
/// separation make larger jumps unreliable. This driver has no per-Mach-regime
/// branch, so 0.5 deg is used uniformly as the conservative ceiling for every
/// case; steps larger than this always fall back to a clean target-angle mesh.
const MAX_CONTINUATION_STEP_DEG: f64 = 0.5;

/// Maximum number of bounded intermediate continuation hops one bridge
/// attempt may take between a converged state and a requested angle that is
/// farther away than [`MAX_CONTINUATION_STEP_DEG`].
///
/// This is a resource bound, not a physical one: a gap that would need more
/// hops than this never narrows the requested sweep, it only means that
/// particular gap falls back to the existing clean target-angle mesh instead
/// of a multi-hop bridge.
const MAX_BRIDGE_HOPS: usize = 8;

/// Outcome of [`Mses::bridge_to_target`].
#[derive(Debug, Clone, Copy)]
enum BridgeOutcome {
    /// `mdat.case` now holds a genuinely converged state within
    /// [`MAX_CONTINUATION_STEP_DEG`] of the target angle (the value carried
    /// here), so the caller may treat it as a continuation anchor.
    Reached(f64),
    /// The bridge could not get within bound of the target inside its hop
    /// and per-hop retry budget. `mdat.case` has been restored to the
    /// starting converged state where possible, so the caller must not trust
    /// it and must fall back to a clean target-angle mesh.
    Exhausted,
    /// Cancellation landed during a bridge step; no partial bridge progress
    /// is trusted, the caller should stop the sweep and keep what it has.
    Cancelled,
}

/// Outcome of [`Mses::pressure_from_checkpoint`].
enum CheckpointAttempt {
    /// The checkpoint's flowfield was restored, bridged, and re-solved at
    /// the exact requested angle, and that solve genuinely converged.
    Solved(WorkDir, f64),
    /// Cancellation landed while trying the checkpoint anchor.
    Cancelled,
    /// The checkpoint could not be turned into a converged exact-target
    /// result (restore failure, bridge exhaustion, or a non-converged
    /// exact-target solve); the caller should fall back to the existing
    /// cold-start search rather than discard the checkpoint's evidence;
    /// there was none to discard, since nothing here was ever presented as
    /// this call's result.
    NotUsable,
}

/// Copy the working solver state aside once a point has genuinely converged.
///
/// Free functions rather than closures over `solve_sweep`'s local `dir`
/// because both `solve_sweep` and [`Mses::bridge_to_target`] need the same
/// save/restore behavior.
fn save_converged_state(dir: &Path) -> bool {
    let src = dir.join("mdat.case");
    let dst = dir.join("mdat.case.last_converged");
    if src.is_file() {
        std::fs::copy(&src, &dst).is_ok()
    } else {
        false
    }
}

/// Restore the last genuinely converged state, undoing whatever a failed
/// solve left in `mdat.case`. Returns `false` when there is nothing to
/// restore (no point has converged yet, or the backup copy itself failed).
fn restore_converged_state(dir: &Path) -> bool {
    let src = dir.join("mdat.case.last_converged");
    let dst = dir.join("mdat.case");
    if src.is_file() {
        std::fs::copy(&src, &dst).is_ok()
    } else {
        false
    }
}

#[derive(Debug)]
struct MsesFailure {
    status: MsesStatus,
    message: String,
}

#[derive(Debug, Default)]
struct SweepOutcome {
    accumulated: HashMap<String, Vec<f64>>,
    point_diagnostics: Vec<MsesPolarPointDiagnostic>,
    solver_attempts: Vec<MsesSolverAttempt>,
    /// A reusable snapshot of each genuinely converged, successfully
    /// MPlot-extracted point, for a later warm-start pressure solve.
    checkpoints: Vec<MsesConvergedCheckpoint>,
    /// Whether the caller stopped the sweep before all requested points ran.
    cancelled: bool,
}

/// The process-local OSMAP resource selected for one MSES driver.
#[derive(Debug, Clone)]
struct OsmapSelection {
    required: bool,
    status: MsesOsmapStatus,
    path: Option<PathBuf>,
    diagnostic: Option<String>,
}

/// Inspect the leading Fortran records used by the installed MSES map reader.
///
/// The first records are stable across the MIT map distributions: both files
/// begin with a 12-byte record, while the next numeric record is 224 bytes for
/// the double-precision map and 112 bytes for the single-precision map.  This
/// lightweight check deliberately rejects unknown files instead of guessing a
/// precision from file size.
fn inspect_osmap(path: &Path) -> Result<(), String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut header = [0_u8; 24];
    file.read_exact(&mut header)
        .map_err(|error| format!("could not read OSMAP header: {error}"))?;
    let record = |offset: usize| {
        let bytes = [
            header[offset],
            header[offset + 1],
            header[offset + 2],
            header[offset + 3],
        ];
        i32::from_le_bytes(bytes)
    };
    let first = record(0);
    let first_trailing = record(16);
    let table_record = record(20);
    if first != 12 || first_trailing != 12 {
        return Err(format!(
            "unsupported Fortran record header (first={first}, trailing={first_trailing})"
        ));
    }
    if table_record != 224 {
        if table_record == 112 {
            return Err(
                "single-precision osmap.dat detected; MSES requires osmapDP.dat".to_owned(),
            );
        }
        return Err(format!(
            "unsupported OSMAP precision record length {table_record} (expected 224 for double precision)"
        ));
    }
    Ok(())
}

fn osmap_candidate(path: PathBuf, source: &str) -> Result<PathBuf, String> {
    // MSES runs in a per-solve temporary directory.  A relative configured
    // path or `MSES_OSMAP` value would therefore resolve against that
    // temporary cwd inside the child, even though this preflight checked it
    // against ALAS's cwd.  Normalize it once before both validation and the
    // child environment so a resource cannot pass preflight and then appear
    // missing to the solver.
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    };
    if !path.is_file() {
        return Err(format!(
            "{source} OSMAP file does not exist: {}",
            path.display()
        ));
    }
    inspect_osmap(&path)
        .map(|()| path.clone())
        .map_err(|error| {
            format!(
                "{source} OSMAP file {} is incompatible: {error}",
                path.display()
            )
        })
}

/// Locate the OSMAP resource shipped with an ALAS release.
///
/// MSES itself remains a separately installed process, so its executable
/// directory is not a reliable place for a resource supplied by ALAS.  The
/// release keeps the map below `assets/mses`; search only bounded application
/// roots rather than the working directory so launching ALAS from another
/// folder cannot change which resource is selected.
fn bundled_osmap_candidates(mses_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut add_root = |root: PathBuf| {
        if !roots.iter().any(|candidate| candidate == &root) {
            roots.push(root);
        }
    };

    if let Some(root) = std::env::var_os("ALAS_APP_DIR").map(PathBuf::from) {
        add_root(root);
    }

    if let Some(root) = mses_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
    {
        // This is the release layout: <package>/external tools/MSES.
        add_root(root);
    }

    if let Ok(executable) = std::env::current_exe() {
        if let Some(root) = executable.parent() {
            add_root(root.to_path_buf());
        }
    }

    roots
        .into_iter()
        .map(|root| root.join("assets").join("mses").join("osmapDP.dat"))
        .collect()
}

fn resolve_osmap(config: &MsesConfig, mses_dir: &Path) -> OsmapSelection {
    let required = config.xtr_upper >= 1.0 || config.xtr_lower >= 1.0;
    let explicit = config
        .osmap_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                mses_dir.join(path)
            }
        });

    // An explicit saved setting is authoritative. This prevents a typo from
    // being hidden by a different map elsewhere on the machine.
    if let Some(path) = explicit {
        return match osmap_candidate(path.clone(), "configured") {
            Ok(path) => OsmapSelection {
                required,
                status: MsesOsmapStatus::Available,
                path: Some(path),
                diagnostic: Some(
                    "configured double-precision OSMAP passed the local header check".to_owned(),
                ),
            },
            Err(error) => OsmapSelection {
                required,
                status: if path.is_file() {
                    MsesOsmapStatus::Incompatible
                } else {
                    MsesOsmapStatus::Missing
                },
                path: None,
                diagnostic: Some(error),
            },
        };
    }

    // MSES names its environment hook MSES_OSMAP. It is intentionally read
    // here and passed back only to child processes; ALAS never mutates the
    // parent process environment.
    if let Ok(value) = std::env::var("MSES_OSMAP") {
        let value = value.trim();
        if !value.is_empty() {
            let path = PathBuf::from(value);
            match osmap_candidate(path.clone(), "MSES_OSMAP") {
                Ok(path) => {
                    return OsmapSelection {
                        required,
                        status: MsesOsmapStatus::Available,
                        path: Some(path),
                        diagnostic: Some("process-local MSES_OSMAP double-precision resource passed the local header check".to_owned()),
                    };
                }
                Err(error) if path.is_file() => {
                    return OsmapSelection {
                        required,
                        status: MsesOsmapStatus::Incompatible,
                        path: None,
                        diagnostic: Some(error),
                    };
                }
                Err(_) => {}
            }
        }
    }

    let adjacent = mses_dir.join("osmapDP.dat");
    match osmap_candidate(adjacent.clone(), "adjacent") {
        Ok(path) => OsmapSelection {
            required,
            status: MsesOsmapStatus::Available,
            path: Some(path),
            diagnostic: Some(
                "adjacent double-precision osmapDP.dat passed the local header check".to_owned(),
            ),
        },
        Err(adjacent_error) => {
            for candidate in bundled_osmap_candidates(mses_dir) {
                if let Ok(path) = osmap_candidate(candidate, "bundled ALAS") {
                    return OsmapSelection {
                        required,
                        status: MsesOsmapStatus::Available,
                        path: Some(path),
                        diagnostic: Some(
                            "bundled ALAS double-precision osmapDP.dat passed the local header check".to_owned(),
                        ),
                    };
                }
            }

            OsmapSelection {
                required,
                status: if adjacent.is_file() {
                    MsesOsmapStatus::Incompatible
                } else if required {
                    MsesOsmapStatus::Missing
                } else {
                    MsesOsmapStatus::NotRequired
                },
                path: None,
                diagnostic: required.then_some(adjacent_error),
            }
        }
    }
}

impl MsesFailure {
    fn solver(message: impl Into<String>) -> Self {
        Self {
            status: MsesStatus::Error,
            message: message.into(),
        }
    }

    fn parse(message: impl Into<String>) -> Self {
        Self {
            status: MsesStatus::ParseFailure,
            message: message.into(),
        }
    }

    fn from_run(error: RunError) -> Self {
        let message = error.to_string();
        let status = match &error {
            RunError::Spawn { .. } => MsesStatus::LaunchFailure,
            RunError::Timeout { .. } => MsesStatus::Timeout,
            RunError::Io { .. } | RunError::Reader { .. } | RunError::InvalidTimeout { .. } => {
                MsesStatus::Error
            }
            RunError::Cancelled { .. } => MsesStatus::Error,
        };
        Self { status, message }
    }
}

fn cancelled(error: &MsesFailure) -> bool {
    error.message.contains("was cancelled and terminated")
}

/// A configured MSES run on one already-repaneled section.
///
/// Holds the section, the solver settings read off [`MsesConfig`], and the
/// three executable paths. The section is taken already repaneled, as
/// native aerodynamic model's wrapper is constructed with `airfoil.repanel(...)`: the entry
/// points [`super::run_mses_polar`]/[`super::run_mses_pressure_distribution`]
/// repanel and build this, and the parity test builds it directly from the
/// coordinates the reference actually fed to `mset`.
pub struct Mses {
    airfoil: Airfoil,
    airfoil_name: String,
    n_crit: f64,
    xtr_upper: f64,
    xtr_lower: f64,
    max_iterations: i64,
    mset_n: i64,
    mset_e: f64,
    mucon: f64,
    timeout_mset_s: f64,
    timeout_mses_s: f64,
    enabled: bool,
    mses_dir: PathBuf,
    mset_exe: PathBuf,
    mses_exe: PathBuf,
    mplot_exe: PathBuf,
    osmap_required: bool,
    osmap_status: MsesOsmapStatus,
    osmap_path: Option<PathBuf>,
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
        if let Some(status) = super::installation_status(&self.mses_dir) {
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
        if let Some(status) = super::installation_status(&self.mses_dir) {
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
                match self.pressure_from_checkpoint(alpha_deg, reynolds, mach, checkpoint, cancel, &mut result) {
                    CheckpointAttempt::Cancelled => {
                        return result.into_error("MSES pressure analysis cancelled".to_owned());
                    }
                    CheckpointAttempt::Solved(workdir, solved_alpha) => {
                        return self.finish_pressure_result(result, workdir.path(), solved_alpha, cancel);
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
            let anchor_saved = anchor_src.is_file() && std::fs::copy(&anchor_src, &anchor_dst).is_ok();
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

    /// Extract the final surface/flowfield tables from `dir`'s genuinely
    /// converged `mdat.case` at `converged_alpha`, and fold them into
    /// `result`.
    ///
    /// Shared by every path that can produce a converged pressure state:
    /// the cold-start retry-offset search, its bridge-back recovery, and the
    /// checkpoint-anchored warm start, so MPlot extraction always runs
    /// against whichever state actually ended up converged, never against an
    /// intermediate or mismatched one.
    fn finish_pressure_result(
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
    fn solve_sweep(
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
                        match self.bridge_to_target(
                            dir,
                            last_alpha,
                            alpha,
                            reynolds,
                            mach,
                            cancel,
                            &mut outcome.solver_attempts,
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
                        match self.bridge_to_target(
                            dir,
                            last_alpha,
                            next_alpha,
                            reynolds,
                            mach,
                            cancel,
                            &mut outcome.solver_attempts,
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
            for (key, value) in summary {
                outcome.accumulated.entry(key).or_default().push(value);
            }

            // Save the known-good converged state for subsequent points.
            save_converged_state(dir);
            last_converged_alpha = Some(alpha);
            if let Some(checkpoint) = self.checkpoint_from_converged(dir, alpha, reynolds, mach, &solve.stdout) {
                outcome.checkpoints.push(checkpoint);
            }

            if let Some(&next_alpha) = alphas.get(index + 1) {
                match self.bridge_to_target(
                    dir,
                    alpha,
                    next_alpha,
                    reynolds,
                    mach,
                    cancel,
                    &mut outcome.solver_attempts,
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

        Ok(outcome)
    }

    /// Write the requested operating-point deck and run one `mses` solve.
    ///
    /// Keeping this process boundary in one helper makes continuation attempts
    /// use precisely the same deck and timeout as ordinary requested points.
    fn run_mses_case(
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
    fn run_mset_or_cancel(
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

    /// Walk `mdat.case` from a genuinely converged `from_alpha` toward
    /// `to_alpha` using bounded intermediate continuation steps, each no
    /// larger than [`MAX_CONTINUATION_STEP_DEG`], so a requested gap wider
    /// than that bound is not simply abandoned to a clean remesh.
    ///
    /// Always restores `from_alpha`'s backed-up converged state into
    /// `mdat.case` first, so callers never need to do that themselves and a
    /// caller's own failed attempt currently sitting in `mdat.case` can never
    /// leak into a bridge. Every intermediate step is an unrequested internal
    /// warm-up: it is appended to `solver_attempts` for audit, but it is
    /// never a candidate for a requested point's own diagnostic or
    /// coefficients. A step that fails to converge is retried once at half
    /// its size from the same last-known-good state (a bounded retry, not
    /// an ever-shrinking search) before the whole bridge gives up and
    /// restores `mdat.case` to `from_alpha`'s state for the caller.
    ///
    /// Eight parameters (matching the existing
    /// `run_mses_pressure_distribution_with_cancel`): each one is a distinct
    /// piece of the solve boundary (endpoints, operating point, cancellation,
    /// audit sink), not a group that collapses into a smaller struct without
    /// inventing a type used nowhere else.
    #[allow(clippy::too_many_arguments)]
    fn bridge_to_target(
        &self,
        dir: &Path,
        from_alpha: f64,
        to_alpha: f64,
        reynolds: f64,
        mach: f64,
        cancel: Option<&AtomicBool>,
        solver_attempts: &mut Vec<MsesSolverAttempt>,
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
                    solver_output: hop.stdout,
                });
                if status == MsesPolarPointStatus::Converged {
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

    /// Capture a reusable checkpoint from `dir`'s just-converged `mdat.case`.
    ///
    /// Returns `None` only on an I/O failure reading the state file back;
    /// the caller already confirmed native convergence and a successful
    /// MPlot extraction before calling this, so a `None` here never hides a
    /// non-converged point as a checkpoint.
    fn checkpoint_from_converged(
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
    fn checkpoint_matches(&self, checkpoint: &MsesConvergedCheckpoint, mach: f64, reynolds: f64) -> bool {
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
    fn pressure_from_checkpoint(
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
    fn run_mset(
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
