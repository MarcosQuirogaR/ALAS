// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// Surface pressure and local Mach at one angle, split by surface, plus the
/// flowfield Mach/Cp contour points and the panelled outline they are drawn over.
///
/// The upper/lower arrays are each ordered by x/c with wake points excluded.
/// Surface assignment follows the reference MSES analysis contract (sign of
/// `y`) when the section stays about the chord line, else the nearer outline
/// surface (see `is_upper_surface`):
/// the sign rule files `y >= 0` samples as upper-surface and `y < 0` samples as
/// lower-surface. `airfoil_x`/`airfoil_y` are the exact panelled
/// geometry MSES solved, so a contour plot's outline matches its flowfield.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MsesPressureResult {
    /// Stable status string for this result.
    pub status: MsesStatus,
    /// A human-readable reason when `status` is not successful.
    pub error: Option<String>,
    /// The angle that actually converged and was extracted: the requested
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
    /// surface. Forced-transition cases do not need an Orr-Sommerfeld map.
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
    /// Orr-Sommerfeld map; live free-transition runs do.
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
            if is_upper_surface(x, y, airfoil_coordinates) {
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

    pub(super) fn into_error(self, message: String) -> Self {
        self.into_failure(MsesStatus::Error, message)
    }

    pub(super) fn into_failure(mut self, status: MsesStatus, message: String) -> Self {
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
    let alphas = design_centred_schedule(trim_alpha_deg, half, count);
    let mut result =
        Mses::new(repaneled, config, mses_dir).polar_with_cancel(&alphas, reynolds, mach, cancel);
    result.design_alpha_deg = Some(trim_alpha_deg);
    result
}

/// The requested angle schedule: `count` angles over
/// `[design - half, design + half]`, ascending, with `design` itself always
/// one of them. An even `count` is raised to the next odd number so the
/// design (cruise) angle is requested exactly rather than only bracketed; the
/// centre node is set to `design_alpha_deg` bit-for-bit so the result can
/// find it again.
pub fn design_centred_schedule(
    design_alpha_deg: f64,
    half_width_deg: f64,
    count: usize,
) -> Vec<f64> {
    let count = if count % 2 == 0 { count + 1 } else { count };
    let mut alphas = linspace(
        design_alpha_deg - half_width_deg,
        design_alpha_deg + half_width_deg,
        count,
    );
    alphas[count / 2] = design_alpha_deg;
    alphas
}

#[cfg(test)]
mod schedule_tests {
    use super::design_centred_schedule;

    #[test]
    fn the_design_angle_is_always_a_requested_node() {
        for count in [3, 4, 5, 7] {
            let alphas = design_centred_schedule(2.25, 1.5, count);
            assert!(alphas.len() % 2 == 1 && alphas.len() >= count);
            assert!(alphas.contains(&2.25), "count {count}: {alphas:?}");
            assert!(alphas.windows(2).all(|w| w[0] < w[1]));
            assert_eq!(alphas[0], 0.75);
            assert_eq!(alphas[alphas.len() - 1], 3.75);
        }
    }
}

/// Whether the boundary-layer sample at `(x, y)` lies on the upper surface of
/// the solved section.
///
/// The reference contract is the sign of `y`, which holds only while the whole
/// lower surface stays below the chord line. A section whose camber exceeds its
/// local half-thickness aft (a rear-loaded section, or a 1/cos-scaled swept
/// normal section) has lower-surface points with `y > 0`, and the sign rule
/// then files them as upper-surface samples. When the panelled outline is
/// available the sample is assigned to the nearer surface at its `x`; the sign
/// rule remains the fallback for a degenerate outline.
fn is_upper_surface(x: f64, y: f64, outline: &[(f64, f64)]) -> bool {
    let sign_rule = y >= 0.0;
    let Some(le) = outline
        .iter()
        .enumerate()
        .min_by(|a, b| a.1 .0.total_cmp(&b.1 .0))
        .map(|(i, _)| i)
    else {
        return sign_rule;
    };
    if le == 0 || le + 1 >= outline.len() {
        return sign_rule;
    }
    let at = |piece: &[(f64, f64)]| -> Option<f64> {
        piece.windows(2).find_map(|w| {
            let (lo, hi) = (w[0].0.min(w[1].0), w[0].0.max(w[1].0));
            if x < lo || x > hi {
                return None;
            }
            let d = w[1].0 - w[0].0;
            Some(if d.abs() < 1e-12 {
                w[0].1
            } else {
                w[0].1 + (x - w[0].0) / d * (w[1].1 - w[0].1)
            })
        })
    };
    let (Some(a), Some(b)) = (at(&outline[..=le]), at(&outline[le..])) else {
        return sign_rule;
    };
    let (upper_y, lower_y) = if a >= b { (a, b) } else { (b, a) };
    if upper_y - lower_y < 1e-6 {
        return sign_rule;
    }
    (y - upper_y).abs() <= (y - lower_y).abs()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod surface_assignment_tests {
    use super::*;

    /// Camber `0.06 sin(pi x)` with half-thickness `0.12 x (1 - x)`: aft of
    /// about x = 0.8 the whole lower surface lies above the chord line.
    fn cambered_outline() -> Vec<(f64, f64)> {
        let camber = |x: f64| 0.06 * (std::f64::consts::PI * x).sin();
        let half = |x: f64| 0.12 * x * (1.0 - x);
        let xs: Vec<f64> = (0..=20).map(|i| f64::from(i) / 20.0).collect();
        let mut outline: Vec<(f64, f64)> =
            xs.iter().rev().map(|&x| (x, camber(x) + half(x))).collect();
        outline.extend(xs.iter().skip(1).map(|&x| (x, camber(x) - half(x))));
        outline
    }

    #[test]
    fn aft_lower_surface_above_the_chord_is_not_filed_as_upper() {
        let outline = cambered_outline();
        let x: f64 = 0.9;
        let camber = 0.06 * (std::f64::consts::PI * x).sin();
        let half = 0.12 * x * (1.0 - x);
        let (y_upper, y_lower) = (camber + half, camber - half);
        assert!(
            y_lower > 0.0,
            "fixture must put the lower surface above y = 0"
        );
        let bl = format!("{x} {y_upper} {x} 0 -0.6 0 0 0.9\n{x} {y_lower} {x} 0 0.3 0 0 0.5\n");
        let result =
            MsesPressureResult::replay_raw_exports(0.0, bl, String::new(), &outline).unwrap();
        assert_eq!(result.cp_upper, vec![-0.6]);
        assert_eq!(result.cp_lower, vec![0.3]);
        assert!(is_upper_surface(x, y_upper, &outline));
        assert!(!is_upper_surface(x, y_lower, &outline));
    }

    #[test]
    fn a_degenerate_outline_keeps_the_sign_rule() {
        assert!(is_upper_surface(0.5, 0.01, &[]));
        assert!(!is_upper_surface(0.5, -0.01, &[(0.0, 0.0), (1.0, 0.0)]));
    }
}
