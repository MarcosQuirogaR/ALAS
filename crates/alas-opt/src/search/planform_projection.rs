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
//!   below the kink chord, `c_r >= c_k / r`; otherwise `c_r >= c_k`. The root
//!   chord itself is compared only when it is a chord of the wing
//!   (`WingConfig::root_chord_is_only_a_datum`), so `r` may exceed one.
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
//! A third bound is the wing-area ceiling ([`PlanformProjection::new`]'s
//! area limit, carried into drawn area): the drawn area rises monotonically
//! with the root chord (the root station, and a side-of-body chord that is
//! its pinned ratio or clipped interpolation), so the ceiling caps the root
//! chord from above. On a box whose upper corner lies far above the
//! ceiling (LJ60 clean sheet: 25.5 m^2 against a registered 24.57 m^2) the
//! two chord bounds alone left 5472 of 5520 seeded draws to the pre-gate's
//! area check, and the scan analysed 22 of its 60 points.
//!
//! All three bound the root chord alone once span, kink chord and sweep are
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
//! `w`. Measured over 2000 projected points (seed 7) on the two chord
//! bounds, before the area bound was added, the 10th, 50th and
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
const TIP_CHORD: usize = 3;
const SWEEP: usize = 4;

/// The configured stations the two constraints depend on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlanformProjection {
    root: (f64, f64),
    kink_fraction: f64,
    /// `(span fraction, chord ratio)` of a pinned side-of-body station.
    side_of_body: Option<(f64, f64)>,
    /// Span fraction of the side-of-body station, pinned chord ratio or not.
    side_fraction: f64,
    /// The root chord is a datum the builder leaves out of its order check.
    root_is_datum: bool,
    /// `wing_span_m(span) - span`, m: minus twice the reach of a wingtip
    /// device whose published span is the design span, else 0. The builder's
    /// semispan is `(span + span_offset_m) / 2`.
    span_offset_m: f64,
    /// Drawn inboard leading-edge sweep, deg, when the wing carries one.
    inboard_sweep_deg: Option<f64>,
    /// The reference-area limit expressed as a drawn area, m^2
    /// ([`Self::drawn_per_reference_area`]).
    area_limit_m2: f64,
    coupling: Option<CoupledGeometry>,
    /// `(start span fraction, chord at the extreme tip)` of a swept-back tip: the design
    /// tip chord is then the one the straight edges reach, and the rake cuts the corner.
    rake: Option<(f64, f64)>,
    /// Straight-edge tip chord the rake chord was read against, m
    /// (`WingConfig::tip_rake_reference_tip_chord_m`): the rake chord then keeps its ratio
    /// to the sampled tip chord.
    rake_reference_tip_chord_m: Option<f64>,
    /// `(span fraction, chord offset m)` of up to two outboard trailing-edge breaks, in span
    /// order: the chord there is the straight kink-to-tip chord plus the offset. A design whose
    /// break chord is not positive is rejected by the pre-gate (the builder's `NonPositive`).
    te_breaks: [Option<(f64, f64)>; 2],
    /// `(span fraction, chord offset m)` of the wing's edge stations
    /// ([`alas_config::WingEdgeStation`]), in span order. They are local features of the
    /// drawn edges: in the drawn area, out of the order check, as a break is.
    edge_stations: [Option<(f64, f64)>; alas_config::MAX_EDGE_STATIONS],
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
                side_fraction,
                root_is_datum: wing.root_chord_is_only_a_datum(),
                span_offset_m: wing.wing_span_m(0.0),
                inboard_sweep_deg: wing.inboard_le_sweep_deg.filter(|_| {
                    wing.side_of_body_span_fraction.is_some() || wing.kink_span_fraction.is_some()
                }),
                area_limit_m2: crate::mdo::wing_area_upper_m2(config)
                    .min(config.requirements.mtow_kg / config.requirements.min_wing_loading_kg_m2),
                coupling: CoupledGeometry::new(config),
                rake: wing
                    .tip_rake_start_span_fraction
                    .zip(wing.tip_rake_tip_chord_m)
                    .filter(|(start, chord)| {
                        *start > kink_fraction && *start < 1.0 && *chord > 0.0
                    }),
                rake_reference_tip_chord_m: wing.tip_rake_reference_tip_chord_m,
                te_breaks: {
                    let rake_start = wing.tip_rake_start_span_fraction.unwrap_or(1.0);
                    let mut previous = kink_fraction;
                    [
                        wing.te_break_span_fraction
                            .zip(wing.te_break_chord_offset_m),
                        wing.te_break_2_span_fraction
                            .zip(wing.te_break_2_chord_offset_m),
                    ]
                    .map(|pair| {
                        pair.filter(|(fraction, offset)| {
                            let ordered = *fraction > previous && *fraction < rake_start;
                            previous = previous.max(*fraction);
                            ordered && offset.is_finite()
                        })
                    })
                },
                edge_stations: std::array::from_fn(|index| {
                    wing.edge_stations.get(index).map(|station| {
                        (
                            station.span_fraction,
                            station.te_offset_m - station.le_offset_m,
                        )
                    })
                }),
            })
            .map(|mut projection| {
                projection.area_limit_m2 *= projection.drawn_per_reference_area(config);
                projection
            })
    }

    /// The drawn area per unit of the reference area the pre-gate and the
    /// wing loading measure, at the configured nominal design: the area
    /// limit is declared on the reference area (published, reference
    /// trapezoid or drawn, [`alas_config::ReferenceWingMode`]) while
    /// [`Self::area_m2`] integrates the drawn stations, which on the
    /// registered presets differ by up to 11 % (E195-E2 drawn 115.8 m^2
    /// against 103.0 m^2 published). The ratio carries the limit into the
    /// projection's own units; the pre-gate still decides on the reference
    /// area exactly. 1 when either area is unavailable.
    fn drawn_per_reference_area(&self, config: &AlasConfig) -> f64 {
        let nominal = config.configured_nominal_design();
        let wing = &config.geometry.wing;
        let reference = wing
            .transport_planform(&nominal)
            .map_or(f64::NAN, |planform| wing.reference_area_m2(&planform));
        let ratio = self.area_m2(&nominal.to_array()) / reference;
        if ratio.is_finite() && ratio > 0.0 {
            ratio
        } else {
            1.0
        }
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
            let run = (self.kink_fraction - inboard) * self.semispan_m(span);
            let inboard_sweep = self.inboard_sweep_deg.unwrap_or(sweep);
            self.root
                .1
                .min((run * inboard_sweep.to_radians().tan() + kink_chord) / ratio)
        };
        let interval =
            (lower.is_finite() && upper.is_finite() && lower <= upper).then_some((lower, upper))?;
        let interval = self.under_area_ceiling(point, interval)?;
        // m6: the builder also rejects non-positive chords outboard of the root
        // (`validate_positive`, e.g. a trailing-edge break chord at or below zero) and
        // non-monotone chords of the straight edges (`NonMonotoneChord`). Such a point has
        // no admissible root chord: return None so the pre-gate rejects it, rather than
        // projecting it into a build failure.
        self.outboard_chords_buildable(point, lower)
            .then_some(interval)
    }

    /// `interval` cut to the root chords whose drawn area stays at or below
    /// the area limit: the area is nondecreasing in the root chord, so the
    /// cut is an upper bound found by bisection (60 halvings, below 1e-12 of
    /// the row). `None` when even the lower end exceeds the limit; the
    /// interval unchanged when the limit is not finite.
    fn under_area_ceiling(&self, point: &[f64], (lower, upper): (f64, f64)) -> Option<(f64, f64)> {
        if !self.area_limit_m2.is_finite() {
            return Some((lower, upper));
        }
        let mut probe = point.to_vec();
        let mut area_at = |root_chord: f64| {
            probe[ROOT_CHORD] = root_chord;
            self.area_m2(&probe)
        };
        if area_at(lower) > self.area_limit_m2 {
            return None;
        }
        if area_at(upper) <= self.area_limit_m2 {
            return Some((lower, upper));
        }
        let (mut low, mut high) = (lower, upper);
        for _ in 0..60 {
            let middle = 0.5 * (low + high);
            if area_at(middle) <= self.area_limit_m2 {
                low = middle;
            } else {
                high = middle;
            }
        }
        Some((lower, low))
    }

    /// The builder's chord rules on the stations outboard of the root, at the
    /// root chord `root_chord` (`WingConfig::transport_planform`): every chord
    /// positive and finite, and no chord of the straight edges larger than its
    /// inboard neighbour (tolerance 1e-9 m, as the builder). A trailing-edge
    /// break or an edge station is a local feature of the drawn edge that the
    /// builder leaves out of the order check, so it may stand above its neighbours;
    /// only its positivity is checked. The side-of-body station is included; the
    /// interval keeps it at or above the kink chord, so it does not add a
    /// further constraint. A root chord that is only a datum is not compared. A
    /// tip rake needs its extreme-tip chord below the sampled tip chord
    /// (`TipRakeChordNotBelowTip`), whatever the root chord; a rake with a reference
    /// tip chord keeps its ratio to the tip chord and always meets it.
    fn outboard_chords_buildable(&self, point: &[f64], root_chord: f64) -> bool {
        let mut probe = point.to_vec();
        probe[ROOT_CHORD] = root_chord;
        let stations = self.line_stations(&probe);
        // `stations` order: root, side of body (when drawn), kink, the breaks,
        // then the rake start (when drawn) and the tip.
        let kink = usize::from(self.side_fraction > 0.0) + 1;
        let breaks = kink + 1..kink + 1 + self.te_breaks.iter().flatten().count();
        let straight: Vec<f64> = stations
            .iter()
            .enumerate()
            .filter(|(index, _)| !breaks.contains(index) && !(self.root_is_datum && *index == 0))
            .map(|(_, (_, chord))| *chord)
            .collect();
        let tip_chord = probe[TIP_CHORD];
        let rake_below_tip = self
            .rake
            .is_none_or(|(_, chord)| self.rake_tip_chord_m(chord, tip_chord) < tip_chord);
        rake_below_tip
            && self.stations(&probe)[1..]
                .iter()
                .all(|(_, chord)| chord.is_finite() && *chord > 0.0)
            && straight.windows(2).all(|pair| pair[1] <= pair[0] + 1e-9)
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

    /// Semispan of the built wing proper for a design-vector span, m
    /// (`WingConfig::wing_span_m`: a device counted in the span shortens it).
    fn semispan_m(&self, design_span_m: f64) -> f64 {
        (design_span_m + self.span_offset_m) / 2.0
    }

    /// The builder's planform stations `(y, chord)`, m, root to extreme tip
    /// (`WingConfig::transport_planform`): the side-of-body chord is the pinned
    /// ratio of the root chord, or else the root-to-kink interpolation clipped
    /// so that its trailing edge stays aft of the kink's (unswept inboard TE)
    /// and not below the kink chord.
    fn line_stations(&self, point: &[f64]) -> Vec<(f64, f64)> {
        let semispan = self.semispan_m(point[SPAN]);
        let (root, kink_chord, tip_chord) =
            (point[ROOT_CHORD], point[KINK_CHORD], point[TIP_CHORD]);
        let kink_y = self.kink_fraction * semispan;
        let mut stations = vec![(0.0, root)];
        if self.side_fraction > 0.0 {
            let side_y = self.side_fraction * semispan;
            let chord = match self.side_of_body {
                Some((_, ratio)) => ratio * root,
                None => {
                    let tan = self
                        .inboard_sweep_deg
                        .unwrap_or(point[SWEEP])
                        .to_radians()
                        .tan();
                    let interpolated =
                        root + (self.side_fraction / self.kink_fraction) * (kink_chord - root);
                    interpolated
                        .min((kink_y - side_y) * tan + kink_chord)
                        .max(kink_chord)
                }
            };
            stations.push((side_y, chord));
        }
        stations.push((kink_y, kink_chord));
        for (fraction, offset) in self.te_breaks.into_iter().flatten() {
            let t = (fraction - self.kink_fraction) / (1.0 - self.kink_fraction);
            stations.push((
                fraction * semispan,
                kink_chord + t * (tip_chord - kink_chord) + offset,
            ));
        }
        if let Some((start, final_chord)) = self.rake {
            let t = (start - self.kink_fraction) / (1.0 - self.kink_fraction);
            stations.push((start * semispan, kink_chord + t * (tip_chord - kink_chord)));
            stations.push((semispan, self.rake_tip_chord_m(final_chord, tip_chord)));
        } else {
            stations.push((semispan, tip_chord));
        }
        stations
    }

    /// The builder's extreme-tip chord of the rake for the sampled tip chord `tip_chord`,
    /// m (`WingConfig::tip_rake_chord_m`).
    fn rake_tip_chord_m(&self, rake_chord: f64, tip_chord: f64) -> f64 {
        alas_config::WingConfig::tip_rake_chord_m(
            rake_chord,
            self.rake_reference_tip_chord_m,
            tip_chord,
        )
    }

    /// [`Self::line_stations`] with the edge stations applied
    /// (`WingConfig::apply_edge_stations`): each interpolates in the lines and adds
    /// its offset, replacing a station it lands on.
    fn stations(&self, point: &[f64]) -> Vec<(f64, f64)> {
        let mut stations = self.line_stations(point);
        if self.edge_stations[0].is_none() {
            return stations;
        }
        let lines = stations.clone();
        let semispan = self.semispan_m(point[SPAN]);
        let same_station_m = alas_config::STATION_SPAN_TOLERANCE * semispan;
        for (fraction, offset) in self.edge_stations.into_iter().flatten() {
            let y = fraction * semispan;
            let chord = lines.windows(2).find(|pair| y <= pair[1].0).map(|pair| {
                let t = (y - pair[0].0) / (pair[1].0 - pair[0].0);
                pair[0].1 + t * (pair[1].1 - pair[0].1)
            });
            if let Some(chord) = chord {
                let at = stations.partition_point(|station| station.0 < y - same_station_m);
                if stations
                    .get(at)
                    .is_some_and(|station| station.0 <= y + same_station_m)
                {
                    stations[at].1 = chord + offset;
                } else {
                    stations.insert(at, (y, chord + offset));
                }
            }
        }
        stations
    }

    /// Full projected planform area of the drawn wing, m^2: the builder's
    /// `TransportPlanform::reference_area_m2` (the main wing's `projected_area`).
    /// The reference area `s_ref` follows `ReferenceWingMode` (Published,
    /// Trapezoid or Drawn) and is read from the planform by the pre-gate, not here.
    pub(super) fn area_m2(&self, point: &[f64]) -> f64 {
        self.stations(point)
            .windows(2)
            .map(|pair| (pair[0].1 + pair[1].1) * (pair[1].0 - pair[0].0))
            .sum()
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
        (1.0 - self.kink_fraction) * self.semispan_m(point[SPAN])
    }

    /// Semispan integrals of chord, y*chord and chord^2: m^2, m^3, m^3.
    pub(super) fn chord_integrals(&self, point: &[f64]) -> (f64, f64, f64) {
        let stations = self.stations(point);
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
            side_fraction: 0.0,
            root_is_datum: false,
            span_offset_m: 0.0,
            inboard_sweep_deg: None,
            area_limit_m2: 100.0,
            coupling: None,
            rake: None,
            rake_reference_tip_chord_m: None,
            te_breaks: [None, None],
            edge_stations: [None; alas_config::MAX_EDGE_STATIONS],
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

    /// m6: a trailing-edge break whose chord is not positive is a build
    /// failure (`validate_positive`); the projection must report no admissible
    /// root chord for it instead of projecting the point. A break chord above
    /// the kink chord is a bulge the builder accepts (it leaves breaks out of
    /// its `NonMonotoneChord` check), and so does the projection. The area
    /// ceiling is lifted: the 5 m bulge alone would exceed it.
    #[test]
    fn a_non_positive_break_chord_has_no_admissible_root_chord_and_a_bulge_has_one() {
        let (config, bounds) = preset("A320-200");
        let mut projection = PlanformProjection::new(&config, &bounds).unwrap();
        projection.area_limit_m2 = f64::INFINITY;
        let point: Vec<f64> = bounds.iter().map(|(lo, hi)| 0.5 * (lo + hi)).collect();
        projection.te_breaks = [Some((0.5, -50.0)), None];
        assert!(projection.root_chord_interval(&point).is_none());
        projection.te_breaks = [Some((0.5, 5.0)), None];
        assert!(projection.root_chord_interval(&point).is_some());
    }

    /// A pinned side-of-body chord above the root datum builds, so the projection must not
    /// declare such a point empty: the admissible root chords run below the kink chord.
    #[test]
    fn a_side_of_body_chord_above_the_datum_keeps_an_admissible_root_chord() {
        let (mut config, mut bounds) = preset("A320-200");
        config.geometry.wing.side_of_body_chord_ratio = Some(1.05);
        // The exposed trailing-edge bound falls below the preset's own root-chord row.
        bounds[ROOT_CHORD].0 = 2.0;
        let projection = PlanformProjection::new(&config, &bounds).unwrap();
        let mut point: Vec<f64> = bounds.iter().map(|(lo, hi)| 0.5 * (lo + hi)).collect();
        let (lower, _) = projection.root_chord_interval(&point).unwrap();
        point[ROOT_CHORD] = lower;
        let design = alas_config::DesignVector::from_array(&point).unwrap();
        assert!(config.geometry.wing.transport_planform(&design).is_ok());
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
            // The chord bounds alone, for the emptiness count below.
            let chords_only = PlanformProjection {
                area_limit_m2: f64::INFINITY,
                ..projection
            };
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
                if chords_only.root_chord_interval(before).is_none() {
                    empty += 1;
                }
                let Some((lower, upper)) = projection.root_chord_interval(before) else {
                    assert_eq!(after, before, "{name}: an empty interval leaves the point");
                    continue;
                };
                // The area ceiling holds on the projected point.
                assert!(
                    projection.area_m2(after) <= projection.area_limit_m2 * (1.0 + 1e-12),
                    "{name}"
                );
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
            // Points whose other coordinates admit no root chord under the
            // chord bounds: under a tenth of the box on every preset here
            // (the area ceiling empties more, all of them over the limit
            // at the smallest admissible root chord).
            assert!(empty < 200, "{name}: {empty} empty intervals");
            assert!(widths.iter().all(|&w| (0.0..=1.0).contains(&w)));
        }
    }
}
