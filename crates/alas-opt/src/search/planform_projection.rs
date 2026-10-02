// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The screening sampler's projection of the root chord onto the two exact
//! planform constraints that reject most of the registered design boxes.
//!
//! Drawn uniformly from the box, about 54 % of A320-200 points, 14 % of
//! B787-9 points and 77 % of ATR72-600 points fail the design-vector
//! pre-gate, every one of them on one of these two checks (seeded screening
//! samples of 200 analysed points, seeds 1 to 3):
//!
//! - **Chord order** (the builder's `NonMonotoneChord`): with a pinned
//!   side-of-body chord ratio `r` the side-of-body chord `r c_r` may not be
//!   below the kink chord, `c_r >= c_k / r`; otherwise `c_r >= c_k`.
//! - **Exposed trailing edge** (while the Geometry family is hard): the
//!   kink's trailing edge may not lie forward of the inboard station's, the
//!   90 degree limit of [`crate::transport_planform::exposed_te_angle_deg`].
//!   With the leading edge at `x = y tan(Lambda)` (`Lambda` the leading-edge
//!   sweep, `y = eta b / 2`), that is
//!   `c_r <= ((y_k - y_s) tan(Lambda) + c_k) / r` with a pinned side-of-body
//!   station at `y_s`, else `c_r <= y_k tan(Lambda) + c_k` (the centreline
//!   root, leading edge at `x = 0`). The bound is the 90 degree edge itself,
//!   inside the gate's `90 + 1e-6` degree limit.
//!
//! Both bound the root chord alone once span, kink chord and sweep are
//! drawn, so the projection maps the sampled root chord's fraction of its
//! box row, `f = (x - lo) / (hi - lo)`, to the same fraction of the
//! admissible interval `[max(lo, L), min(hi, U)]`. It is deterministic,
//! keeps every point inside the box, never changes another coordinate, and
//! leaves a point whose interval is empty unchanged for the pre-gate to
//! reject. The constraints themselves are untouched: the pre-gate still
//! checks every point.
//!
//! # Coverage
//!
//! Every other coordinate keeps its Latin hypercube stratification exactly,
//! and the root chord is stratified in its admissible fraction. The joint
//! density is therefore uniform in the other coordinates and, given them,
//! uniform over the admissible root chords: a point whose admissible
//! interval is a share `w` of the box row carries weight `1 / w` against
//! sampling uniformly over the admissible set (the rejection sampling this
//! replaces), which under-samples narrow-interval regions in proportion to
//! `w`. Measured over 2000 projected points (seed 7), the 10th, 50th and
//! 90th percentiles of `w` are 0.16, 0.46 and 0.78 on the A320-200 box,
//! 0.60, 0.97 and 1.00 on the B787-9, 0.31, 0.62 and 0.93 on the A380-800
//! and 0.05, 0.25 and 0.45 on the ATR72-600; the points whose interval is
//! empty, and which the pre-gate still rejects, are 2.5 %, 0 %, 0 % and
//! 5.5 % of the box.

use alas_config::{AlasConfig, ConstraintPolicy};

/// Design-vector indices of the planform coordinates
/// (`alas_config::design_variables`).
const SPAN: usize = 0;
const ROOT_CHORD: usize = 1;
const KINK_CHORD: usize = 2;
const SWEEP: usize = 4;

/// The configured stations the two constraints depend on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlanformProjection {
    root: (f64, f64),
    kink_fraction: f64,
    /// `(span fraction, chord ratio)` of a pinned side-of-body station.
    side_of_body: Option<(f64, f64)>,
    trailing_edge: bool,
}

impl PlanformProjection {
    /// The projection for `config` over `bounds`; `None` when the root chord
    /// is not free or the stations do not describe a transport planform.
    pub(crate) fn new(config: &AlasConfig, bounds: &[(f64, f64)]) -> Option<Self> {
        let wing = &config.geometry.wing;
        let root = *bounds.get(ROOT_CHORD)?;
        let kink_fraction = wing.kink_span_fraction.unwrap_or(wing.break_span_fraction);
        let side_fraction = wing.side_of_body_span_fraction.unwrap_or(0.0);
        let side_of_body = wing
            .side_of_body_chord_ratio
            .filter(|ratio| side_fraction > 0.0 && ratio.is_finite() && *ratio > 0.0)
            .map(|ratio| (side_fraction, ratio));
        (bounds.len() > SWEEP
            && root.1 > root.0
            && kink_fraction.is_finite()
            && (0.0..1.0).contains(&side_fraction)
            && side_fraction < kink_fraction
            && kink_fraction < 1.0)
            .then_some(Self {
                root,
                kink_fraction,
                side_of_body,
                trailing_edge: config.optimizer.objective.geometry_constraints
                    == ConstraintPolicy::Hard,
            })
    }

    /// The admissible root chords for the other coordinates of `point`,
    /// clipped to the box row; `None` when empty or not finite.
    pub(crate) fn root_chord_interval(&self, point: &[f64]) -> Option<(f64, f64)> {
        let (span, kink_chord, sweep) = (
            *point.get(SPAN)?,
            *point.get(KINK_CHORD)?,
            *point.get(SWEEP)?,
        );
        let ratio = self.side_of_body.map_or(1.0, |(_, ratio)| ratio);
        let lower = self.root.0.max(kink_chord / ratio);
        let upper = if self.trailing_edge {
            let inboard = self.side_of_body.map_or(0.0, |(fraction, _)| fraction);
            let run = (self.kink_fraction - inboard) * span / 2.0;
            self.root
                .1
                .min((run * sweep.to_radians().tan() + kink_chord) / ratio)
        } else {
            self.root.1
        };
        (lower.is_finite() && upper.is_finite() && lower <= upper).then_some((lower, upper))
    }

    /// Map the root chord of `point` onto its admissible interval at the
    /// same fraction it holds in its box row.
    pub(crate) fn apply(&self, point: &mut [f64]) {
        if let Some((lower, upper)) = self.root_chord_interval(point) {
            let (lo, hi) = self.root;
            let fraction = ((point[ROOT_CHORD] - lo) / (hi - lo)).clamp(0.0, 1.0);
            point[ROOT_CHORD] = (lower + fraction * (upper - lower)).clamp(lower, upper);
        }
    }
}

// A test asserts on values it built here, so a failed unwrap is the
// assertion failing rather than a library invariant being broken.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::screening;
    use crate::DesignOptimizer;

    fn preset(name: &str) -> (AlasConfig, Vec<(f64, f64)>) {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let bounds = DesignOptimizer::new(config.clone())
            .resolved_bounds(None, None)
            .unwrap();
        (config, bounds)
    }

    #[test]
    fn projected_points_stay_in_the_box_and_never_fail_the_projected_constraints() {
        for name in ["A320-200", "B787-9", "ATR72-600", "A380-800"] {
            let (config, bounds) = preset(name);
            let projection = PlanformProjection::new(&config, &bounds).unwrap();
            let raw = screening::sample(&bounds, None, 2_000, 7, None);
            let projected = screening::sample(&bounds, None, 2_000, 7, Some(&projection));
            let (mut empty, mut widths) = (0, Vec::new());
            for (before, after) in raw.iter().zip(&projected) {
                for (index, (&value, &(lo, hi))) in after.iter().zip(&bounds).enumerate() {
                    assert!((lo..=hi).contains(&value), "{name} {index} {value}");
                    if index != ROOT_CHORD {
                        assert_eq!(value, before[index], "{name}: only the root chord moves");
                    }
                }
                let Some((lower, upper)) = projection.root_chord_interval(before) else {
                    empty += 1;
                    continue;
                };
                widths.push((upper - lower) / (bounds[ROOT_CHORD].1 - bounds[ROOT_CHORD].0));
                // The pre-gate's own planform and trailing-edge checks pass.
                let design = alas_config::DesignVector::from_array(after).unwrap();
                assert!(config.geometry.wing.transport_planform(&design).is_ok());
                let angle =
                    crate::transport_planform::exposed_te_angle_deg(&config.geometry.wing, &design)
                        .unwrap();
                assert!(angle <= crate::mdo::TE_ANGLE_LIMIT_DEG, "{name} {angle}");
            }
            // Determinism: the same seed projects to the same points.
            assert_eq!(
                projected,
                screening::sample(&bounds, None, 2_000, 7, Some(&projection))
            );
            // Only points whose other coordinates admit no root chord are
            // still rejected: under a tenth of the box on every preset here.
            assert!(empty < 200, "{name}: {empty} empty intervals");
            assert!(widths.iter().all(|&w| (0.0..=1.0).contains(&w)));
        }
    }

    #[test]
    fn a_soft_geometry_family_projects_only_the_chord_order() {
        let (mut config, bounds) = preset("A320-200");
        config.optimizer.objective.geometry_constraints = ConstraintPolicy::Soft;
        let projection = PlanformProjection::new(&config, &bounds).unwrap();
        let point = screening::sample(&bounds, None, 1, 3, None).remove(0);
        let (lower, upper) = projection.root_chord_interval(&point).unwrap();
        assert_eq!(upper, bounds[ROOT_CHORD].1);
        assert!(lower >= bounds[ROOT_CHORD].0);
    }
}
