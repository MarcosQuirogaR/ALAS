// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// The repanel density both entry points fix at their MSES call sites:
/// `airfoil.repanel(n_points_per_side=80)`.
pub(super) const N_POINTS_PER_SIDE: usize = 80;

/// The retry offsets `run_mses_pressure_distribution` brackets a fixed point
/// with when the exact requested angle does not converge, in degrees.
pub(super) const DEFAULT_RETRY_OFFSETS_DEG: [f64; 5] = [0.0, 0.5, -0.5, 1.0, -1.0];

/// Whether an MSES run produced a usable result.
///
/// The execution and installation states exposed by an MSES run.
///
/// The reference result only had `not_run`, `ok`, and `error`. The additional
/// states are native boundary diagnostics: they preserve the reference result
/// shape while making an optional external-tool failure actionable instead of
/// presenting every problem as a solver divergence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MsesStatus {
    /// No run has been attempted.
    #[default]
    NotRun,
    /// MSES was disabled in configuration.
    Disabled,
    /// No MSES directory was found.
    Absent,
    /// An MSES directory exists but is missing one or more programs.
    Incomplete,
    /// A required MSES program could not be started.
    LaunchFailure,
    /// A required MSES program exceeded its configured timeout.
    Timeout,
    /// MSES exited, but its output was missing or malformed.
    ParseFailure,
    /// The run converged and its result is populated.
    Ok,
    /// At least one requested polar point converged, but the sweep is incomplete.
    PartialConvergence,
    /// The solver ran but did not produce a converged result.
    Error,
}

impl MsesStatus {
    /// The stable status string used by result consumers and diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotRun => "not_run",
            Self::Disabled => "disabled",
            Self::Absent => "absent",
            Self::Incomplete => "incomplete",
            Self::LaunchFailure => "launch_failure",
            Self::Timeout => "timeout",
            Self::ParseFailure => "parse_failure",
            Self::Ok => "ok",
            Self::PartialConvergence => "partial_convergence",
            Self::Error => "error",
        }
    }
}

/// The outcome reported by MSES for one requested polar angle.
///
/// This is deliberately separate from [`MsesStatus`]. The latter describes the
/// sweep as a whole, while this preserves which exact request produced the
/// solver transcript that establishes the whole-sweep verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsesPolarPointStatus {
    /// MSES printed its normal tolerance-convergence marker for this angle.
    Converged,
    /// MSES completed its run but did not print the convergence marker.
    NotConverged,
}

impl MsesPolarPointStatus {
    /// Stable diagnostic string for persisted run evidence.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Converged => "converged",
            Self::NotConverged => "not_converged",
        }
    }
}

/// Availability and format validation of the Orr-Sommerfeld database used by
/// MSES's free-transition model.
///
/// MSES can still emit a finite pressure table when this resource is missing,
/// but its transition amplification rates are then zeroed by the solver.  The
/// status is therefore kept separately from [`MsesStatus`] and is part of the
/// presentation-validity gate rather than being hidden behind a generic
/// successful process exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MsesOsmapStatus {
    /// Both surfaces use an explicitly forced transition station, so no map is
    /// needed for this run.
    #[default]
    NotRequired,
    /// A double-precision map was resolved and passed the local format check.
    Available,
    /// Free transition was requested but no usable map was resolved.
    Missing,
    /// A map was supplied but its on-disk record format is not the one used by
    /// the installed MSES executable (for example XFOIL's single-precision
    /// `osmap.dat`).
    Incompatible,
}

impl MsesOsmapStatus {
    /// Stable status string persisted in run diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotRequired => "not_required",
            Self::Available => "available",
            Self::Missing => "missing",
            Self::Incompatible => "incompatible",
        }
    }
}

/// Solver evidence retained for one requested polar angle.
///
/// `solver_output` is MSES's normalized stdout, captured before the driver
/// decides whether to reinitialize the mesh. It is intentionally not parsed
/// into an invented aerodynamic diagnosis: absence of MSES's documented
/// "Converged on tolerance" marker is the only solver conclusion this driver
/// makes. Consumers can therefore inspect the numerical transcript without
/// confusing a failed nonlinear solve with a physical coefficient.
#[derive(Debug, Clone, PartialEq)]
pub struct MsesPolarPointDiagnostic {
    /// The requested angle of attack, in degrees.
    pub requested_alpha_deg: f64,
    /// MSES's convergence outcome for this request.
    pub status: MsesPolarPointStatus,
    /// Verbatim normalized MSES stdout for this requested point.
    pub solver_output: String,
}

/// One external-solver attempt retained for convergence audits.
///
/// Requested-point diagnostics intentionally remain the compact public polar
/// record.  This sibling record also includes unrequested continuation warm-up
/// solves and restart attempts, together with the raw MSES transcript.  The
/// transcript contains the solver's iteration residuals, so a report can
/// distinguish a failed initial state from a failed final requested condition
/// without turning a non-converged coefficient into a guessed value.
#[derive(Debug, Clone, PartialEq)]
pub struct MsesSolverAttempt {
    /// The angle supplied to MSES for this attempt, in degrees.
    pub alpha_deg: f64,
    /// Why this attempt was made (initial solve, restart, or warm-up).
    pub purpose: String,
    /// Whether MSES printed its tolerance-convergence marker.
    pub status: MsesPolarPointStatus,
    /// Normalized MSES stdout, including its residual history where emitted.
    pub solver_output: String,
}

/// The result of an MSES alpha sweep on one section.
///
/// The coefficient arrays are parallel and hold one entry per angle that
/// converged; the four `cd*` fields, the moment and the two transition
/// stations follow upstream's `MSESPolarResult` field for field. Lower-cased
/// (`cl`, not `CL`) to satisfy Rust naming; they are the standard lift, drag
/// and moment coefficients.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MsesPolarResult {
    /// Stable status string for this result.
    pub status: MsesStatus,
    /// A human-readable reason when `status` is not successful.
    pub error: Option<String>,
    /// The section's name, or `"optimized_root_section"` when it had none.
    pub airfoil_name: String,
    /// The freestream Mach number the sweep ran at.
    pub mach: f64,
    /// The chord-referenced Reynolds number the sweep ran at.
    pub reynolds: f64,
    /// Number of operating points requested from the external solver.
    pub requested_alpha_count: usize,
    /// Number of requested operating points that converged and were parsed.
    pub converged_alpha_count: usize,
    /// One solver transcript for each requested polar point reached by MSES.
    ///
    /// An incomplete sweep retains non-converged points here even though its
    /// coefficient arrays contain only converged points. This makes the
    /// partial-convergence verdict auditable without fabricating coefficients.
    pub point_diagnostics: Vec<MsesPolarPointDiagnostic>,
    /// Every MSES process attempt, including unrequested continuation warm-ups.
    ///
    /// This is kept separately from [`Self::point_diagnostics`] so the latter
    /// continues to describe only requested operating points.
    pub solver_attempts: Vec<MsesSolverAttempt>,
    /// Whether this run asked MSES to predict natural transition on either
    /// surface. Forced-transition cases do not need an Orr-Sommerfeld map.
    pub osmap_required: bool,
    /// Format/resource status for the map used by the live solver process.
    pub osmap_status: MsesOsmapStatus,
    /// Resolved map path, when one was selected for the process environment.
    pub osmap_path: Option<String>,
    /// Actionable map-resolution or format diagnostic, if any.
    pub osmap_diagnostic: Option<String>,
    /// The angle of attack of each converged point, in degrees.
    pub alpha_deg: Vec<f64>,
    /// Lift coefficient at each converged point.
    pub cl: Vec<f64>,
    /// Total drag coefficient at each converged point.
    pub cd: Vec<f64>,
    /// Pitching-moment coefficient at each converged point.
    pub cm: Vec<f64>,
    /// Viscous drag coefficient at each converged point.
    pub cdv: Vec<f64>,
    /// Wave drag coefficient at each converged point.
    pub cdw: Vec<f64>,
    /// Upper-surface transition station (x/c) at each converged point.
    pub xtr_top: Vec<f64>,
    /// Lower-surface transition station (x/c) at each converged point.
    pub xtr_bot: Vec<f64>,
    /// A reusable, self-contained snapshot of each genuinely converged point
    /// in this sweep whose backing `mdat.case` could be captured. Not
    /// guaranteed to align index-for-index with [`Self::alpha_deg`] (a rare
    /// capture I/O failure just omits that one entry); search by nearest
    /// [`MsesConvergedCheckpoint::alpha_deg`], not by position.
    ///
    /// A caller solving a pressure point at (or near) one of these angles can
    /// pass the nearest checkpoint into
    /// [`crate::mses::Mses::pressure_with_checkpoint_and_cancel`] as a warm
    /// anchor instead of starting from a cold clean mesh: see
    /// [`MsesConvergedCheckpoint`] for the identity checks that gate reuse.
    pub checkpoints: Vec<MsesConvergedCheckpoint>,
}

/// A self-contained snapshot of one genuinely converged MSES solve:
/// geometry, solver configuration, Mach, Reynolds, OSMAP identity, the
/// converged angle, and the raw `mdat.case` continuation state: that a
/// later, independent driver call can validate and reuse as a warm-start
/// anchor instead of a cold clean mesh.
///
/// There is no public constructor: the only way to obtain one is to receive
/// it back from a driver call that genuinely converged the point it
/// describes (see [`MsesPolarResult::checkpoints`]), which is what "reusable
/// ... converged-only provenance" means here: a caller cannot hand-build or
/// forge one, and a checkpoint whose identity does not match the driver
/// instance it is offered to (different geometry, solver settings, Mach, Re,
/// or OSMAP) is rejected rather than trusted; see
/// [`crate::mses::Mses::pressure_with_checkpoint_and_cancel`].
#[derive(Debug, Clone, PartialEq)]
pub struct MsesConvergedCheckpoint {
    pub(crate) airfoil_name: String,
    pub(crate) airfoil_coordinates: Vec<(f64, f64)>,
    pub(crate) n_crit: f64,
    pub(crate) xtr_upper: f64,
    pub(crate) xtr_lower: f64,
    pub(crate) mset_n: i64,
    pub(crate) mset_e: f64,
    pub(crate) mucon: f64,
    pub(crate) max_iterations: i64,
    pub(crate) mach: f64,
    pub(crate) reynolds: f64,
    pub(crate) osmap_path: Option<PathBuf>,
    /// The angle this checkpoint is genuinely converged at, in degrees.
    pub alpha_deg: f64,
    /// Verbatim MSES stdout for the solve that produced this checkpoint:
    /// audit evidence that it is real convergence, not a claim.
    pub solver_output: String,
    pub(crate) mdat_case: Vec<u8>,
}

impl MsesConvergedCheckpoint {
    /// The angle this checkpoint is genuinely converged at, in degrees.
    pub fn alpha_deg(&self) -> f64 {
        self.alpha_deg
    }

    /// Verbatim MSES stdout for the solve that produced this checkpoint.
    pub fn solver_output(&self) -> &str {
        &self.solver_output
    }

    /// The name of the airfoil section this checkpoint was solved on.
    pub fn airfoil_name(&self) -> &str {
        &self.airfoil_name
    }

    /// The Mach number this checkpoint was solved at.
    pub fn mach(&self) -> f64 {
        self.mach
    }

    /// The Reynolds number this checkpoint was solved at.
    pub fn reynolds(&self) -> f64 {
        self.reynolds
    }

    /// The resolved OSMAP database path, if any.
    pub fn osmap_path(&self) -> Option<&Path> {
        self.osmap_path.as_deref()
    }

    /// Critical amplification factor `n_crit`.
    pub fn n_crit(&self) -> f64 {
        self.n_crit
    }

    /// Upper surface forced transition location `xtr_upper`.
    pub fn xtr_upper(&self) -> f64 {
        self.xtr_upper
    }

    /// Lower surface forced transition location `xtr_lower`.
    pub fn xtr_lower(&self) -> f64 {
        self.xtr_lower
    }

    /// Mesh resolution parameter `mset_n`.
    pub fn mset_n(&self) -> i64 {
        self.mset_n
    }

    /// Mesh expansion parameter `mset_e`.
    pub fn mset_e(&self) -> f64 {
        self.mset_e
    }

    /// Mucon parameter.
    pub fn mucon(&self) -> f64 {
        self.mucon
    }

    /// Solver iteration cap.
    pub fn max_iterations(&self) -> i64 {
        self.max_iterations
    }

    /// Whether this checkpoint matches the specified geometry, flight condition,
    /// and solver configuration.
    pub fn matches(
        &self,
        airfoil_name: &str,
        airfoil_coordinates: &[(f64, f64)],
        mach: f64,
        reynolds: f64,
        config: &MsesConfig,
        osmap_path: Option<&Path>,
    ) -> bool {
        self.airfoil_name == airfoil_name
            && self.airfoil_coordinates == airfoil_coordinates
            && self.n_crit == config.n_crit
            && self.xtr_upper == config.xtr_upper
            && self.xtr_lower == config.xtr_lower
            && self.mset_n == config.mset_n
            && self.mset_e == config.mset_e
            && self.mucon == config.mucon
            && self.max_iterations == config.max_iterations
            && self.mach == mach
            && self.reynolds == reynolds
            && self.osmap_path.as_deref() == osmap_path
    }

    /// Whether this checkpoint matches the given flight condition and airfoil name.
    pub fn matches_condition(&self, airfoil_name: &str, mach: f64, reynolds: f64) -> bool {
        self.airfoil_name == airfoil_name && self.mach == mach && self.reynolds == reynolds
    }
}

impl MsesPolarResult {
    /// Whether the result contains converged points that consumers may inspect.
    pub fn has_usable_data(&self) -> bool {
        matches!(self.status, MsesStatus::Ok | MsesStatus::PartialConvergence)
            && self.converged_alpha_count > 0
            && self.transition_model_is_valid()
            && self.has_valid_coefficient_schema()
    }

    /// Whether a converged table represents the requested transition model.
    ///
    /// This remains true for forced-transition runs and for offline fixture
    /// results (whose default `osmap_required` is false), preserving replay
    /// compatibility while preventing a live free-transition run with a
    /// missing/incompatible database from being presented as valid.
    pub fn transition_model_is_valid(&self) -> bool {
        !self.osmap_required || self.osmap_status == MsesOsmapStatus::Available
    }

    /// Check the public polar schema before a consumer uses coefficient data.
    ///
    /// The live MPlot driver validates this schema while accumulating rows.
    /// Keeping the same guard on the result object protects deserialized or
    /// manually assembled results as well: a status/count pair must not make
    /// fabricated zeros, ragged columns, or NaNs look usable.
    fn has_valid_coefficient_schema(&self) -> bool {
        let expected_len = self.converged_alpha_count;
        let columns = [
            &self.alpha_deg,
            &self.cl,
            &self.cd,
            &self.cm,
            &self.cdv,
            &self.cdw,
            &self.xtr_top,
            &self.xtr_bot,
        ];
        columns.iter().all(|column| {
            column.len() == expected_len && column.iter().all(|value| value.is_finite())
        })
    }

    /// Whether every requested operating point converged.
    pub fn is_complete(&self) -> bool {
        self.status == MsesStatus::Ok
            && self.requested_alpha_count > 0
            && self.converged_alpha_count == self.requested_alpha_count
            && self.transition_model_is_valid()
            && self.has_valid_coefficient_schema()
    }

    /// Requested angles at which MSES did not report tolerance convergence.
    pub fn nonconverged_alpha_deg(&self) -> Vec<f64> {
        self.point_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.status == MsesPolarPointStatus::NotConverged)
            .map(|diagnostic| diagnostic.requested_alpha_deg)
            .collect()
    }

    /// The lift-to-drag ratio at each point, `0.0` where drag is negligible:
    /// upstream's `l_over_d` property.
    pub fn l_over_d(&self) -> Vec<f64> {
        self.cl
            .iter()
            .zip(&self.cd)
            .map(|(&cl, &cd)| if cd > 1e-9 { cl / cd } else { 0.0 })
            .collect()
    }

    /// Select the closest genuinely converged checkpoint to `target_alpha_deg`,
    /// selecting the finite closest using [`f64::total_cmp`].
    ///
    /// Validates that `target_alpha_deg` and candidate angles are finite and
    /// match the polar's Mach, Reynolds number, and airfoil name.
    /// Returns `None` if `target_alpha_deg` is non-finite or no matching
    /// checkpoint with a finite angle exists.
    pub fn closest_checkpoint(&self, target_alpha_deg: f64) -> Option<&MsesConvergedCheckpoint> {
        if !target_alpha_deg.is_finite() {
            return None;
        }
        self.checkpoints
            .iter()
            .filter(|cp| {
                cp.alpha_deg.is_finite()
                    && cp.mach == self.mach
                    && cp.reynolds == self.reynolds
                    && cp.airfoil_name == self.airfoil_name
            })
            .min_by(|a, b| {
                let dist_a = (a.alpha_deg - target_alpha_deg).abs();
                let dist_b = (b.alpha_deg - target_alpha_deg).abs();
                dist_a.total_cmp(&dist_b)
            })
    }

    pub(super) fn into_error(self, message: String) -> Self {
        self.into_failure(MsesStatus::Error, message)
    }

    pub(super) fn into_failure(mut self, status: MsesStatus, message: String) -> Self {
        self.status = status;
        self.error = Some(message);
        self
    }
}

/// Select the closest genuinely converged checkpoint to `target_alpha_deg` from a
/// slice of checkpoints, selecting the finite closest using [`f64::total_cmp`].
///
/// Returns `None` if `target_alpha_deg` is non-finite or no candidate has a finite angle.
pub fn select_closest_checkpoint(
    checkpoints: &[MsesConvergedCheckpoint],
    target_alpha_deg: f64,
) -> Option<&MsesConvergedCheckpoint> {
    if !target_alpha_deg.is_finite() {
        return None;
    }
    checkpoints
        .iter()
        .filter(|cp| cp.alpha_deg.is_finite())
        .min_by(|a, b| {
            let dist_a = (a.alpha_deg - target_alpha_deg).abs();
            let dist_b = (b.alpha_deg - target_alpha_deg).abs();
            dist_a.total_cmp(&dist_b)
        })
}
