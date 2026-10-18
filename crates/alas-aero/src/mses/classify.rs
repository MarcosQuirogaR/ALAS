// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Why a requested MSES polar point did not converge.
//!
//! A sweep that simply counts converged points cannot tell a solver defect
//! from a request that lies beyond the section's attached-flow range (stall,
//! shock-induced separation). The two need different handling: the first is a
//! failure to fix, the second is an operating condition outside the validity
//! domain of the method and must be reported as such, never as a pass.
//!
//! The classifier uses only evidence the solver run itself produced:
//!
//! - **Separation flag.** MSES prints `H clamped at I = .., side ..` while the
//!   boundary-layer shape factor sits at its upper limit, which is the
//!   solver's own signature of a separated boundary layer. Over the 2026-10
//!   archived sweeps (30 polar files, 178 requested points) this line was
//!   present in the final iteration block of 19 of the 59 non-converged
//!   points and of none of the 119 converged ones, so it is a discriminating
//!   (not merely correlated) marker. A diverged solve (`NaN` residuals) with
//!   no clamp carries no such evidence.
//! - **Lift-curve roll-over.** A non-converged angle beyond the converged
//!   span, where the converged lift slope near that edge has fallen below
//!   [`ROLLOVER_SLOPE_FRACTION`] of the slope at the opposite end of the span
//!   (or turned negative), lies past the section's maximum lift. This needs
//!   at least three converged points.
//!
//! Both are screening heuristics, not a validated stall predictor. A point
//! with neither piece of evidence is a [`MsesPointClass::SolverFailure`].

use super::{MsesPolarPointDiagnostic, MsesPolarPointStatus, MsesPolarResult};

/// Edge lift slope below this fraction of the opposite-end slope marks the
/// lift curve as rolled over. Half the linear slope is a conventional "no
/// longer linear" screen; it is a classification threshold, not a calibrated
/// physical constant.
pub const ROLLOVER_SLOPE_FRACTION: f64 = 0.5;

/// Outcome class of one requested polar angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsesPointClass {
    /// MSES reported tolerance convergence.
    Converged,
    /// Not converged and no evidence of separated flow: a solver or
    /// initialisation failure that should be fixed or investigated.
    SolverFailure,
    /// Not converged with evidence that the angle lies beyond the section's
    /// attached-flow range. Outside the validity domain; not a pass.
    OutsideValidityDomain,
    /// The sweep stopped (cancelled or aborted) before this angle was run.
    NotReached,
}

impl MsesPointClass {
    /// Stable diagnostic string for persisted evidence.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Converged => "converged",
            Self::SolverFailure => "solver_failure",
            Self::OutsideValidityDomain => "outside_validity_domain",
            Self::NotReached => "not_reached",
        }
    }
}

/// Whether the final iteration block of an `mses` transcript carries the
/// solver's boundary-layer separation flag (`H clamped`).
pub fn solver_flags_separation(mses_stdout: &str) -> bool {
    let Some(last) = mses_stdout.rfind("rms(dR)") else {
        return mses_stdout.contains("H clamped");
    };
    let block_start = mses_stdout[..last].rfind("rms(dR)").unwrap_or(0);
    mses_stdout[block_start..].contains("H clamped")
}

/// Whether the converged lift curve has rolled over at the edge a failed
/// `alpha_deg` lies beyond. `converged` is `(alpha_deg, cl)`.
fn lift_curve_has_rolled_over(alpha_deg: f64, converged: &[(f64, f64)]) -> bool {
    if converged.len() < 3 {
        return false;
    }
    let mut sorted = converged.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let beyond_top = alpha_deg > sorted[sorted.len() - 1].0;
    let beyond_bottom = alpha_deg < sorted[0].0;
    if !beyond_top && !beyond_bottom {
        return false;
    }
    // Order the points so index 0 is the far end and the last is the edge
    // nearest the failed angle; slopes are then taken toward that edge.
    if beyond_bottom {
        sorted.reverse();
    }
    let slope = |a: (f64, f64), b: (f64, f64)| (b.1 - a.1) / (b.0 - a.0);
    let n = sorted.len();
    let reference = slope(sorted[0], sorted[1]);
    let edge = slope(sorted[n - 2], sorted[n - 1]);
    if !reference.is_finite() || !edge.is_finite() || reference.abs() < f64::EPSILON {
        return false;
    }
    // Compare in the direction of increasing |incidence|: a lift curve
    // rolling over has an edge slope that is small or of opposite sign
    // relative to the reference slope.
    edge / reference < ROLLOVER_SLOPE_FRACTION
}

/// Classify every requested angle.
///
/// `diagnostics` is parallel to the requested schedule (it may be shorter
/// when the sweep stopped early); `converged` holds `(alpha_deg, cl)` of the
/// points that produced coefficients.
pub fn classify_requested_points(
    requested_count: usize,
    diagnostics: &[MsesPolarPointDiagnostic],
    converged: &[(f64, f64)],
) -> Vec<MsesPointClass> {
    (0..requested_count)
        .map(|index| {
            let Some(point) = diagnostics.get(index) else {
                return MsesPointClass::NotReached;
            };
            if point.status == MsesPolarPointStatus::Converged {
                return MsesPointClass::Converged;
            }
            if solver_flags_separation(&point.solver_output)
                || lift_curve_has_rolled_over(point.requested_alpha_deg, converged)
            {
                MsesPointClass::OutsideValidityDomain
            } else {
                MsesPointClass::SolverFailure
            }
        })
        .collect()
}

/// One additional spanwise section polar (kink or tip) beside the root.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MsesSpanSection {
    /// `"kink"` or `"tip"`.
    pub label: String,
    /// Semi-span station of the section, in metres.
    pub y_m: f64,
    /// Local chord, in metres.
    pub chord_m: f64,
    /// The sweep of this section.
    pub polar: MsesPolarResult,
}

impl MsesPolarResult {
    /// The class of the design (cruise) angle, when one was requested and
    /// the sweep reached it.
    pub fn design_point_class(&self) -> Option<MsesPointClass> {
        let design = self.design_alpha_deg?;
        self.point_diagnostics
            .iter()
            .position(|point| (point.requested_alpha_deg - design).abs() <= 1e-9)
            .and_then(|index| self.point_classes.get(index).copied())
    }

    /// Number of requested angles in `class`.
    pub fn count_class(&self, class: MsesPointClass) -> usize {
        self.point_classes.iter().filter(|c| **c == class).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(alpha: f64, output: &str) -> MsesPolarPointDiagnostic {
        MsesPolarPointDiagnostic {
            requested_alpha_deg: alpha,
            status: MsesPolarPointStatus::NotConverged,
            solver_output: output.to_owned(),
        }
    }

    fn converged(alpha: f64) -> MsesPolarPointDiagnostic {
        MsesPolarPointDiagnostic {
            requested_alpha_deg: alpha,
            status: MsesPolarPointStatus::Converged,
            solver_output: "Converged on tolerance".to_owned(),
        }
    }

    const CLAMPED: &str = " 90 rms(dR): 1.0\n  100   rms(dR): 0.5\n H clamped at I = 95, side 1\n  101   rms(dR): 0.4\n";
    const DIVERGED: &str = " 99 rms(dR): 1.0\n  100   rms(dR):       NaN\n";

    #[test]
    fn separation_flag_is_read_from_the_final_iteration_block_only() {
        assert!(solver_flags_separation(CLAMPED));
        assert!(!solver_flags_separation(DIVERGED));
        let early = "H clamped at I = 3\n 1 rms(dR): 1\n 2 rms(dR): 1\n 3 rms(dR): 1\n";
        assert!(!solver_flags_separation(early));
    }

    #[test]
    fn a_clamped_boundary_layer_is_outside_the_validity_domain() {
        let classes =
            classify_requested_points(2, &[converged(1.0), failed(2.0, CLAMPED)], &[(1.0, 0.8)]);
        assert_eq!(
            classes,
            [
                MsesPointClass::Converged,
                MsesPointClass::OutsideValidityDomain
            ]
        );
    }

    #[test]
    fn a_diverged_solve_without_rollover_evidence_is_a_solver_failure() {
        let classes = classify_requested_points(
            3,
            &[converged(1.0), converged(2.0), failed(3.0, DIVERGED)],
            &[(1.0, 0.5), (2.0, 0.6)],
        );
        assert_eq!(classes[2], MsesPointClass::SolverFailure);
    }

    #[test]
    fn a_failure_beyond_a_rolled_over_lift_curve_is_outside_the_domain() {
        let curve = [(1.0, 0.8), (2.0, 1.2), (3.0, 1.25)];
        let classes = classify_requested_points(
            4,
            &[
                converged(1.0),
                converged(2.0),
                converged(3.0),
                failed(4.0, DIVERGED),
            ],
            &curve,
        );
        assert_eq!(classes[3], MsesPointClass::OutsideValidityDomain);
        // A failure on the other side of the span has no such evidence.
        assert!(!lift_curve_has_rolled_over(
            0.0,
            &[(1.0, 0.8), (2.0, 1.2), (3.0, 1.6)]
        ));
        // The same curve still rolls over when the stall is at negative alpha.
        assert!(lift_curve_has_rolled_over(
            -4.0,
            &[(-3.0, -1.25), (-2.0, -1.2), (-1.0, -0.8)]
        ));
    }

    #[test]
    fn too_few_converged_points_never_claim_a_roll_over() {
        assert!(!lift_curve_has_rolled_over(5.0, &[(1.0, 0.5), (2.0, 0.55)]));
    }

    #[test]
    fn unreached_angles_are_not_reached_rather_than_stalled() {
        let classes = classify_requested_points(3, &[converged(1.0)], &[(1.0, 0.5)]);
        assert_eq!(classes[1], MsesPointClass::NotReached);
        assert_eq!(classes[2], MsesPointClass::NotReached);
    }
}
