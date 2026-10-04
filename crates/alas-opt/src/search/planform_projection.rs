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
//! - **Exposed trailing edge**: the
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

use alas_config::AlasConfig;

use super::coupled_geometry::CoupledGeometry;

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
    area_limit_m2: f64,
    coupling: Option<CoupledGeometry>,
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
                area_limit_m2: config
                    .requirements
                    .max_wing_area_m2
                    .min(config.requirements.mtow_kg / config.requirements.min_wing_loading_kg_m2),
                coupling: CoupledGeometry::new(config),
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
        let upper = {
            let inboard = self.side_of_body.map_or(0.0, |(fraction, _)| fraction);
            let run = (self.kink_fraction - inboard) * span / 2.0;
            self.root
                .1
                .min((run * sweep.to_radians().tan() + kink_chord) / ratio)
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

    /// Full projected planform area, m^2, integrating the linear chords on both sides.
    pub(super) fn area_m2(&self, point: &[f64]) -> f64 {
        let (side, ratio) = self.side_of_body.unwrap_or((0.0, 1.0));
        let kink = self.kink_fraction;
        let root_weight = side * (1.0 + ratio) + (kink - side) * ratio;
        point[SPAN] / 2.0
            * (root_weight * point[ROOT_CHORD]
                + (1.0 - side) * point[KINK_CHORD]
                + (1.0 - kink) * point[3])
    }

    pub(super) fn area_limit_m2(&self) -> f64 {
        self.area_limit_m2
    }

    pub(super) fn repair_coupled(
        &self,
        point: &mut [f64],
        bounds: &[(f64, f64)],
        anchor: &[f64],
        sweep_fraction: f64,
    ) {
        if let Some(coupling) = self.coupling {
            coupling.repair(self, point, bounds, anchor, sweep_fraction);
        }
    }

    pub(super) fn outboard_span_m(&self, point: &[f64]) -> f64 {
        (1.0 - self.kink_fraction) * point[0] / 2.0
    }

    /// Semispan integrals of chord, y*chord and chord^2: m^2, m^3, m^3.
    pub(super) fn chord_integrals(&self, point: &[f64]) -> (f64, f64, f64) {
        let (side, ratio) = self.side_of_body.unwrap_or((0.0, 1.0));
        let semispan = point[0] / 2.0;
        let stations = [
            (0.0, point[1]),
            (side * semispan, ratio * point[1]),
            (self.kink_fraction * semispan, point[2]),
            (semispan, point[3]),
        ];
        stations.windows(2).fold((0.0, 0.0, 0.0), |sum, pair| {
            let ((y0, c0), (y1, c1)) = (pair[0], pair[1]);
            let span = y1 - y0;
            (
                sum.0 + span * (c0 + c1) / 2.0,
                sum.1 + span * (y0 * (c0 + c1) / 2.0 + span * (c0 + 2.0 * c1) / 6.0),
                sum.2 + span * (c0 * c0 + c0 * c1 + c1 * c1) / 3.0,
            )
        })
    }

    /// Planar quarter-chord station relative to the untranslated root, m.
    pub(super) fn quarter_chord_station(&self, point: &[f64]) -> f64 {
        let (area, y_chord, chord_squared) = self.chord_integrals(point);
        (point[4].to_radians().tan() * y_chord + 0.25 * chord_squared) / area
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

    #[test]
    fn chord_integrals_match_independent_simpson_integration() {
        let projection = PlanformProjection {
            root: (2.0, 6.0),
            kink_fraction: 0.4,
            side_of_body: None,
            area_limit_m2: 100.0,
            coupling: None,
        };
        let mut point = [0.0; 16];
        point[..5].copy_from_slice(&[20.0, 4.0, 2.0, 1.0, 30.0]);
        let analytic = projection.chord_integrals(&point);
        let mut numerical = [0.0; 3];
        for (y0, y1, c0, c1) in [(0.0, 4.0, 4.0, 2.0), (4.0, 10.0, 2.0, 1.0)] {
            for (fraction, weight) in [(0.0, 1.0), (0.5, 4.0), (1.0, 1.0)] {
                let y = y0 + fraction * (y1 - y0);
                let c = c0 + fraction * (c1 - c0);
                let factor = weight * (y1 - y0) / 6.0;
                for (sum, value) in numerical.iter_mut().zip([c, y * c, c * c]) {
                    *sum += factor * value;
                }
            }
        }
        for (actual, expected) in [analytic.0, analytic.1, analytic.2]
            .into_iter()
            .zip(numerical)
        {
            assert!((actual - expected).abs() < 64.0 * f64::EPSILON * expected);
        }
        assert!((2.0 * analytic.0 - projection.area_m2(&point)).abs() < f64::EPSILON * 100.0);
    }

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
}
