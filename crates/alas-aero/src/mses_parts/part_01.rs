// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

#[path = "../mses/deck.rs"]
mod deck;
#[path = "../mses/driver.rs"]
mod driver;
#[path = "../mses/exec.rs"]
mod exec;
#[path = "../mses/parse.rs"]
mod parse;

pub use driver::Mses;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use alas_config::MsesConfig;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::spacing::linspace;

/// The repanel density both entry points fix at their MSES call sites --
/// `airfoil.repanel(n_points_per_side=80)`.
const N_POINTS_PER_SIDE: usize = 80;

/// The retry offsets `run_mses_pressure_distribution` brackets a fixed point
/// with when the exact requested angle does not converge, in degrees.
const DEFAULT_RETRY_OFFSETS_DEG: [f64; 5] = [0.0, 0.5, -0.5, 1.0, -1.0];

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

/// Availability and format validation of the Orr--Sommerfeld database used by
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
    /// surface. Forced-transition cases do not need an Orr--Sommerfeld map.
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

/// A self-contained snapshot of one genuinely converged MSES solve --
/// geometry, solver configuration, Mach, Reynolds, OSMAP identity, the
/// converged angle, and the raw `mdat.case` continuation state -- that a
/// later, independent driver call can validate and reuse as a warm-start
/// anchor instead of a cold clean mesh.
///
/// There is no public constructor: the only way to obtain one is to receive
/// it back from a driver call that genuinely converged the point it
/// describes (see [`MsesPolarResult::checkpoints`]), which is what "reusable
/// ... converged-only provenance" means here -- a caller cannot hand-build or
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
    /// Verbatim MSES stdout for the solve that produced this checkpoint --
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

    /// The lift-to-drag ratio at each point, `0.0` where drag is negligible --
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

    fn into_error(self, message: String) -> Self {
        self.into_failure(MsesStatus::Error, message)
    }

    fn into_failure(mut self, status: MsesStatus, message: String) -> Self {
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

/// Surface pressure and local Mach at one angle, split by surface, plus the
/// flowfield Mach/Cp contour points and the panelled outline they are drawn over.
///
/// The upper/lower arrays are each ordered by x/c with wake points excluded.
/// Surface assignment follows the reference MSES analysis contract exactly:
/// points with `y >= 0` are upper-surface samples and points with `y < 0` are
/// lower-surface samples. `airfoil_x`/`airfoil_y` are the exact panelled
/// geometry MSES solved, so a contour plot's outline matches its flowfield.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MsesPressureResult {
    /// Stable status string for this result.
    pub status: MsesStatus,
    /// A human-readable reason when `status` is not successful.
    pub error: Option<String>,
    /// The angle that actually converged and was extracted -- the requested
    /// one, or a retry offset if bridging back to the exact requested angle
    /// (see [`Mses::bridge_to_target`]) did not converge either. Compare
    /// against [`Self::requested_alpha_deg`], or call
    /// [`Self::is_exact_alpha`], before presenting this result as the
    /// requested operating point: an off-target result is real, converged
    /// evidence, but it is not the condition that was asked for.
    pub alpha_deg: f64,
    /// The angle this pressure solve was actually asked for.
    ///
    /// Always the caller's `alpha_deg` argument, regardless of what
    /// eventually converged. Equal to [`Self::alpha_deg`] exactly when the
    /// result is the exact requested condition.
    pub requested_alpha_deg: f64,
    /// Every supervised MSES attempt made while selecting this pressure
    /// point, including failed retry offsets. A successful pressure result
    /// is accepted only after one of these attempts reports the native
    /// convergence marker; retaining the transcripts prevents a finite MPlot
    /// table from being mistaken for independent convergence evidence.
    pub solver_attempts: Vec<MsesSolverAttempt>,
    /// Whether this run asked MSES to predict natural transition on either
    /// surface. Forced-transition cases do not need an Orr--Sommerfeld map.
    pub osmap_required: bool,
    /// Format/resource status for the map used by the live solver process.
    pub osmap_status: MsesOsmapStatus,
    /// Resolved map path, when one was selected for the process environment.
    pub osmap_path: Option<String>,
    /// Actionable map-resolution or format diagnostic, if any.
    pub osmap_diagnostic: Option<String>,
    /// Upper-surface x/c stations, ascending.
    pub x_upper: Vec<f64>,
    /// Upper-surface pressure coefficient at each `x_upper`.
    pub cp_upper: Vec<f64>,
    /// Upper-surface local Mach at each `x_upper`.
    pub mach_upper: Vec<f64>,
    /// Lower-surface x/c stations, ascending.
    pub x_lower: Vec<f64>,
    /// Lower-surface pressure coefficient at each `x_lower`.
    pub cp_lower: Vec<f64>,
    /// Lower-surface local Mach at each `x_lower`.
    pub mach_lower: Vec<f64>,
    /// Flowfield sample x coordinates (for Mach/Cp contours).
    pub field_x: Vec<f64>,
    /// Flowfield sample y coordinates (for Mach/Cp contours).
    pub field_y: Vec<f64>,
    /// Flowfield local Mach at each sample.
    pub field_mach: Vec<f64>,
    /// Flowfield pressure coefficient at each sample.
    ///
    /// MPlot option 11 writes this as its ninth column. Older retained dumps
    /// may leave entries non-finite when that column was not present; those
    /// samples remain usable for Mach contours but are excluded from Cp plots.
    pub field_cp: Vec<f64>,
    /// Starting sample index of each structured `mplot` flow-field row.
    ///
    /// Blank lines in option 11 separate constant-grid-index rows. Keeping
    /// those boundaries allows the report layer to fill the native solver
    /// cells instead of reducing the field to disconnected point markers.
    pub field_row_offsets: Vec<usize>,
    /// The panelled section's x coordinates (the shape MSES actually solved).
    pub airfoil_x: Vec<f64>,
    /// The panelled section's y coordinates.
    pub airfoil_y: Vec<f64>,
    /// Verbatim `mplot` option-12 boundary-layer export used for this result.
    ///
    /// Retaining the source table makes the plotted surface distributions
    /// independently replayable instead of leaving only derived arrays after
    /// the temporary solver directory is removed.
    pub raw_bl_dump: String,
    /// Verbatim `mplot` option-11 flow-field export used for this result.
    ///
    /// This preserves the solver grid and every exported column for later
    /// contour reconstruction and parser audits.
    pub raw_flowfield_dump: String,
}

/// The physical extent of the MPlot option-11 flow-field samples.
///
/// MSET derives its outer grid from the section and its grid controls. The
/// resulting far-field extent is therefore evidence in the solver output, not
/// a UI guess or a separate contour-plot setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MsesFlowfieldDomain {
    /// Minimum chord-normalized streamwise coordinate.
    pub x_min: f64,
    /// Maximum chord-normalized streamwise coordinate.
    pub x_max: f64,
    /// Minimum chord-normalized normal coordinate.
    pub y_min: f64,
    /// Maximum chord-normalized normal coordinate.
    pub y_max: f64,
}

/// Why retained `mplot` option-11/12 tables could not be replayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MsesRawExportError {
    /// The boundary-layer table had no parseable numeric panel rows.
    #[error("BL dump file was empty or unparseable")]
    EmptyBoundaryLayer,
    /// MPlot did not provide both topological surface walks.
    #[error("BL dump did not contain two MPlot surface walks")]
    MissingSurfaceTopology,
}

impl MsesPressureResult {
    /// Whether [`Self::alpha_deg`] is the exact angle that was requested,
    /// rather than a retry offset or a bridge anchor that never converged at
    /// the target. A caller must not present Cp/Mach as the requested
    /// operating point's nominal solution unless this is `true`.
    pub fn is_exact_alpha(&self) -> bool {
        self.alpha_deg == self.requested_alpha_deg
    }

    /// Whether a converged pressure table represents the requested transition
    /// model. Forced-transition runs and offline replays do not require an
    /// Orr--Sommerfeld map; live free-transition runs do.
    pub fn transition_model_is_valid(&self) -> bool {
        !self.osmap_required || self.osmap_status == MsesOsmapStatus::Available
    }

    /// Whether a pressure table may be rendered as a solved live result.
    ///
    /// Live runs retain at least one supervised solver attempt, so a missing
    /// native convergence marker is rejected even if MPlot exported a finite
    /// last iterate. Raw-export fixtures intentionally have no attempt
    /// transcript and remain replayable for parser/figure tests; a caller
    /// using such a replay must provide its own source-run evidence before
    /// treating it as a physical result.
    pub fn is_valid_for_presentation(&self) -> bool {
        self.status == MsesStatus::Ok
            && self.transition_model_is_valid()
            && (self.solver_attempts.is_empty()
                || self.solver_attempts.iter().any(|attempt| {
                    attempt.status == MsesPolarPointStatus::Converged
                        && attempt.alpha_deg.is_finite()
                }))
    }

    /// Whether this pressure table is backed by a native MSES convergence
    /// marker for the accepted operating point.
    ///
    /// A finite `mplot` table is not sufficient evidence on its own: MPlot can
    /// export the last, unconverged iterate.  Live pressure runs populate
    /// `solver_attempts` before replaying the raw exports, while offline
    /// replays intentionally remain false until their source run evidence is
    /// supplied by the caller.
    pub fn has_convergence_evidence(&self) -> bool {
        self.status == MsesStatus::Ok
            && self.transition_model_is_valid()
            && self.solver_attempts.iter().any(|attempt| {
                attempt.status == MsesPolarPointStatus::Converged && attempt.alpha_deg.is_finite()
            })
    }

    /// Return the actual finite domain exported by MPlot option 11.
    pub fn flowfield_domain(&self) -> Option<MsesFlowfieldDomain> {
        let mut points = self
            .field_x
            .iter()
            .zip(&self.field_y)
            .zip(&self.field_mach)
            .filter_map(|((&x, &y), &mach)| {
                (x.is_finite() && y.is_finite() && mach.is_finite()).then_some((x, y))
            });
        let (first_x, first_y) = points.next()?;
        let mut domain = MsesFlowfieldDomain {
            x_min: first_x,
            x_max: first_x,
            y_min: first_y,
            y_max: first_y,
        };
        for (x, y) in points {
            domain.x_min = domain.x_min.min(x);
            domain.x_max = domain.x_max.max(x);
            domain.y_min = domain.y_min.min(y);
            domain.y_max = domain.y_max.max(y);
        }
        Some(domain)
    }

    /// Rebuild a pressure result directly from retained raw `mplot` exports.
    ///
    /// Live execution and offline figure audits share this parser, so a
    /// report replay cannot silently reinterpret the solver tables differently
    /// from the pipeline run that originally produced them.
    pub fn replay_raw_exports(
        alpha_deg: f64,
        bl_dump: String,
        flowfield_dump: String,
        airfoil_coordinates: &[(f64, f64)],
    ) -> Result<Self, MsesRawExportError> {
        let (xs, ys, _arc_lengths, cps, mes) = parse::parse_bl_dump(&bl_dump);
        if xs.is_empty() {
            return Err(MsesRawExportError::EmptyBoundaryLayer);
        }

        let mut upper: Vec<(f64, f64, f64)> = Vec::new();
        let mut lower: Vec<(f64, f64, f64)> = Vec::new();
        for ((&x, &y), (&cp, &mach)) in xs.iter().zip(&ys).zip(cps.iter().zip(&mes)) {
            if !(-0.01..=1.02).contains(&x) {
                continue;
            }
            if y >= 0.0 {
                upper.push((x, cp, mach));
            } else {
                lower.push((x, cp, mach));
            }
        }
        if upper.is_empty() || lower.is_empty() {
            return Err(MsesRawExportError::MissingSurfaceTopology);
        }
        upper.sort_by(|a, b| a.0.total_cmp(&b.0));
        lower.sort_by(|a, b| a.0.total_cmp(&b.0));

        let (field_x, field_y, field_mach, field_cp, field_row_offsets) =
            parse::parse_flowfield(&flowfield_dump);

        Ok(Self {
            status: MsesStatus::Ok,
            alpha_deg,
            x_upper: upper.iter().map(|point| point.0).collect(),
            cp_upper: upper.iter().map(|point| point.1).collect(),
            mach_upper: upper.iter().map(|point| point.2).collect(),
            x_lower: lower.iter().map(|point| point.0).collect(),
            cp_lower: lower.iter().map(|point| point.1).collect(),
            mach_lower: lower.iter().map(|point| point.2).collect(),
            field_x,
            field_y,
            field_mach,
            field_cp,
            field_row_offsets,
            airfoil_x: airfoil_coordinates.iter().map(|point| point.0).collect(),
            airfoil_y: airfoil_coordinates.iter().map(|point| point.1).collect(),
            raw_bl_dump: bl_dump,
            raw_flowfield_dump: flowfield_dump,
            ..Self::default()
        })
    }

    fn into_error(self, message: String) -> Self {
        self.into_failure(MsesStatus::Error, message)
    }

    fn into_failure(mut self, status: MsesStatus, message: String) -> Self {
        self.status = status;
        self.error = Some(message);
        self
    }
}

/// Run an MSES alpha sweep on `airfoil`, bracketing `trim_alpha_deg`.
///
/// The section is repaneled to [`N_POINTS_PER_SIDE`] points per side, the sweep
/// spans `[trim - halfwidth, trim + halfwidth]` at the configured point count
/// (both floored, as upstream floors them at 0.5 deg and 3 points), and each
/// point is solved through the `mset`/`mses`/`mplot` sequence [`Mses`] drives.
/// `mses_dir` is the resolved folder holding the three executables.
pub fn run_mses_polar(
    airfoil: &Airfoil,
    mach: f64,
    reynolds: f64,
    trim_alpha_deg: f64,
    config: &MsesConfig,
    mses_dir: &Path,
) -> MsesPolarResult {
    run_mses_polar_with_cancel(
        airfoil,
        mach,
        reynolds,
        trim_alpha_deg,
        config,
        mses_dir,
        None,
    )
}

/// Run the MSES polar entry point with cooperative cancellation.
///
/// This preserves the original public wrapper while allowing a pipeline
/// worker to stop between (or during) external solver calls without discarding
/// already converged alpha points.
pub fn run_mses_polar_with_cancel(
    airfoil: &Airfoil,
    mach: f64,
    reynolds: f64,
    trim_alpha_deg: f64,
    config: &MsesConfig,
    mses_dir: &Path,
    cancel: Option<&AtomicBool>,
) -> MsesPolarResult {
    let base = MsesPolarResult {
        airfoil_name: display_name(airfoil),
        mach,
        reynolds,
        ..MsesPolarResult::default()
    };
    if !config.enabled {
        return base.into_failure(
            MsesStatus::Disabled,
            "MSES analysis is disabled in configuration".to_owned(),
        );
    }
    let repaneled = match airfoil.repanel(N_POINTS_PER_SIDE) {
        Ok(repaneled) => repaneled,
        Err(error) => return base.into_error(error.to_string()),
    };
    let half = config.alpha_sweep_halfwidth_deg.max(0.5);
    let count = config.alpha_sweep_n_points.max(3) as usize;
    let alphas = linspace(trim_alpha_deg - half, trim_alpha_deg + half, count);
    Mses::new(repaneled, config, mses_dir).polar_with_cancel(&alphas, reynolds, mach, cancel)
}
