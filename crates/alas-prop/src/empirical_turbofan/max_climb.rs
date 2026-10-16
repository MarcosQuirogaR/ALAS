// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Maximum-climb thrust of the deck over its whole envelope, from Jenkinson's
//! engine charts, and the maximum-cruise rating.
//!
//! L. R. Jenkinson, P. Simpkin and D. Rhodes, "Civil Jet Aircraft Design",
//! Arnold (1999), Chapter 9, Figs. 9.18(b), 9.19(b), 9.20(b) and 9.21(b):
//! the maximum-climb thrust ratio `FN / FN*` (`FN*` the sea-level static
//! take-off thrust) against Mach number for the heights sea level to 45,000
//! ft, for engines of bypass ratio 3.0, 6.5, 8.0 and 13.0; and Figs. 9.19(c)
//! and 9.21(c), the maximum-cruise thrust of the 6.5 and 13.0 engines.
//!
//! The numbers below were read from the printed charts (rendered from the
//! book's PDF at 4x, the axes calibrated on their printed tick marks, each
//! curve located by its pixel row at the Mach positions; reading uncertainty
//! +-0.01 of `FN / FN*` for the 30,000 to 45,000 ft curves, about +-0.02 at
//! low height and Mach). They replace the earlier two-part model (Howe's
//! class anchor at the reference point, 17-25 % above Mattingly and up to 18
//! % above these charts at FL350, carried to other conditions by the
//! Bartel-Young lapse, which falls 13-19 % below these charts at low
//! altitude: two errors that compensated in climb).
//!
//! One form serves every engine: linear interpolation in Mach number
//! (nodes 0.3 to 0.9; held at the ends, the take-off rating covers lower
//! speeds), in pressure ratio between the chart heights (thrust is nearly
//! proportional to ambient pressure), and in bypass ratio between the four
//! charts (nearest chart outside BPR 3-13). The charts are of generic 1990s
//! engines. The thrust ratio is not monotonic in bypass ratio on them
//! (0.272, 0.237, 0.228, 0.220 at 30,000 ft and M 0.8 for BPR 3, 6.5, 8,
//! 13); no data of the actual engines (CFM56, CF6, Trent, GEnx, PW1000G,
//! LEAP) are available here.

use super::correlation::standard_pressure_pa;
use super::takeoff_chart::TakeoffTable;

/// Mach numbers of the chart samples.
const CHART_MACH: [f64; 7] = [0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];

/// One chart height: pressure altitude in feet and the thrust ratios at
/// [CHART_MACH].
type ChartRow = (f64, [f64; 7]);

/// One engine chart: bypass ratio and its height rows (ascending height).
struct Chart {
    bpr: f64,
    rows: &'static [ChartRow],
}

/// Heights of the blended table, ft (the union of the chart heights).
const TABLE_HEIGHTS_FT: [f64; 9] = [
    0.0, 10_000.0, 15_000.0, 20_000.0, 25_000.0, 30_000.0, 36_000.0, 40_000.0, 45_000.0,
];

/// Maximum-climb thrust over take-off thrust, Jenkinson Figs. 9.18(b),
/// 9.19(b), 9.20(b), 9.21(b).
#[allow(
    clippy::approx_constant,
    reason = "chart readings, one of which (0.318) happens to be near 1/pi"
)]
const MAXIMUM_CLIMB_CHARTS: [Chart; 4] = [
    Chart {
        bpr: 3.0,
        rows: &[
            (0.0, [0.555, 0.520, 0.490, 0.470, 0.450, 0.429, 0.403]),
            (10_000.0, [0.460, 0.433, 0.412, 0.397, 0.388, 0.380, 0.373]),
            (20_000.0, [0.387, 0.367, 0.351, 0.340, 0.333, 0.326, 0.324]),
            (30_000.0, [0.289, 0.280, 0.274, 0.272, 0.272, 0.272, 0.274]),
            (40_000.0, [0.183, 0.179, 0.178, 0.178, 0.181, 0.181, 0.183]),
            (45_000.0, [0.143, 0.138, 0.136, 0.136, 0.138, 0.138, 0.140]),
        ],
    },
    Chart {
        bpr: 6.5,
        rows: &[
            (0.0, [0.608, 0.558, 0.515, 0.479, 0.448, 0.424, 0.402]),
            (10_000.0, [0.499, 0.463, 0.432, 0.406, 0.384, 0.367, 0.349]),
            (15_000.0, [0.445, 0.412, 0.387, 0.367, 0.351, 0.336, 0.324]),
            (25_000.0, [0.313, 0.307, 0.299, 0.289, 0.280, 0.272, 0.268]),
            (30_000.0, [0.255, 0.247, 0.242, 0.241, 0.240, 0.237, 0.237]),
            (36_000.0, [0.189, 0.182, 0.180, 0.180, 0.185, 0.193, 0.199]),
            (40_000.0, [0.146, 0.141, 0.140, 0.143, 0.148, 0.156, 0.163]),
            (45_000.0, [0.117, 0.114, 0.114, 0.115, 0.121, 0.129, 0.136]),
        ],
    },
    Chart {
        bpr: 8.0,
        rows: &[
            (0.0, [0.570, 0.516, 0.471, 0.433, 0.397, 0.366, 0.338]),
            (10_000.0, [0.476, 0.434, 0.401, 0.375, 0.351, 0.330, 0.310]),
            (15_000.0, [0.432, 0.395, 0.366, 0.344, 0.323, 0.304, 0.287]),
            (25_000.0, [0.342, 0.318, 0.298, 0.282, 0.267, 0.253, 0.244]),
            (30_000.0, [0.282, 0.268, 0.257, 0.247, 0.237, 0.228, 0.219]),
            (36_000.0, [0.218, 0.206, 0.198, 0.195, 0.194, 0.194, 0.193]),
            (40_000.0, [0.177, 0.167, 0.163, 0.162, 0.161, 0.158, 0.156]),
            (45_000.0, [0.138, 0.131, 0.127, 0.125, 0.124, 0.122, 0.120]),
        ],
    },
    Chart {
        bpr: 13.0,
        rows: &[
            (0.0, [0.590, 0.523, 0.467, 0.423, 0.380, 0.337, 0.307]),
            (10_000.0, [0.492, 0.439, 0.399, 0.366, 0.335, 0.308, 0.291]),
            (15_000.0, [0.436, 0.393, 0.362, 0.335, 0.310, 0.288, 0.272]),
            (20_000.0, [0.334, 0.307, 0.288, 0.276, 0.263, 0.250, 0.237]),
            (30_000.0, [0.282, 0.262, 0.248, 0.239, 0.230, 0.220, 0.212]),
            (36_000.0, [0.224, 0.210, 0.197, 0.188, 0.181, 0.180, 0.181]),
            (40_000.0, [0.182, 0.173, 0.165, 0.156, 0.152, 0.152, 0.153]),
            (45_000.0, [0.145, 0.138, 0.130, 0.124, 0.120, 0.119, 0.119]),
        ],
    },
];

/// Maximum-cruise thrust over maximum-climb thrust.
///
/// Two sources agree. Torenbeek, "Advanced Aircraft Design" (2013), Sec.
/// 9.2 (p. 262): "if the maximum climb and cruise ratings produce a thrust
/// ratio of 1.08, the two criteria yield the same thrust requirement"
/// (1 / 1.08 = 0.926). Jenkinson's maximum-cruise charts (Figs. 9.19(c),
/// 9.21(c)) over the maximum-climb charts above, read at 30,000 to 40,000 ft
/// and M 0.6 to 0.9: mean 0.93 for BPR 6.5 (range 0.917-0.950) and 0.929
/// for BPR 13 (0.908-0.962). Applied at every altitude and Mach number
/// (the charts give 0.88-0.94 over their whole height range).
pub(super) const MAXIMUM_CRUISE_OVER_MAXIMUM_CLIMB: f64 = 0.93;

/// Scatter of the chart-derived maximum-cruise thrust of an engine whose
/// altitude thrust rests on the Jenkinson charts alone (no engine datum),
/// as a fraction of the deck value: 0.051.
///
/// Derived from the spread of the charts themselves (pixel-traced from the
/// PDF of Jenkinson, Simpkin and Rhodes, "Civil Jet Aircraft Design", Figs.
/// 9.18(b)-9.21(b) maximum climb, pages printed 203, 206, 209, 212, and
/// 9.19(c), 9.21(c) maximum cruise, printed 207, 213; the traces reproduce
/// bit for bit and were checked against the rendered pages):
///
/// 1. Across bypass ratio. At FL350 the maximum-climb thrust ratio of the
///    BPR 6.5, 8 and 13 charts (the range of the engines without a datum:
///    BPR 8.5-12.5) differs by a half-range over mean of 3.3 % at M 0.78
///    growing to 4.2 % at M 0.85 (BPR 6.5/8/13 at M 0.8: 0.1997, 0.1991,
///    0.1860); the largest value over the design Mach range 0.78-0.85 of the
///    jets is taken: 4.18 %. The two maximum-cruise charts (BPR 6.5 and 13)
///    give a direct half-range of 4.1 % at M 0.78 to 6.0 % at M 0.85,
///    consistent with the root sum of the two components below (4.4 % to
///    5.1 %).
/// 2. Maximum-cruise over maximum-climb ratio at 30, 36 and 40 kft and
///    M 0.6-0.9 (BPR 6.5 and 13 charts, 20 samples): 0.9085 to 0.9615
///    (mean 0.930), half-range over mid-range 2.84 % (the deck uses 0.93).
///
/// Root sum of squares of the two independent components,
/// `sqrt(0.0418^2 + 0.0284^2) = 0.0505`, rounded up to 0.051. It is a
/// half-range (envelope) of the chart family, not a standard deviation, and
/// excludes the +-0.005 (about 2.6 %) chart reading uncertainty. It was
/// derived before the verdicts of any aircraft were examined and is not tuned.
pub const CHART_THRUST_SCATTER: f64 = 0.051;

/// Largest catalogue cruise thrust over the chart maximum-climb rating the
/// deck accepts: chart reading (+-0.01-0.02 on 0.2) and class spread.
pub(super) const MAXIMUM_CATALOGUE_CRUISE_OVER_MAXIMUM_CLIMB: f64 = 1.15;

/// Lowest Mach number of the chart reference points.
pub(super) const ANCHOR_MINIMUM_MACH: f64 = 0.4;
/// Highest Mach number of the chart reference points.
pub(super) const ANCHOR_MAXIMUM_MACH: f64 = 0.9;
/// Tropopause altitude bounding the reference altitude, m (US Standard
/// Atmosphere 1976).
pub(super) const ANCHOR_MAXIMUM_ALTITUDE_M: f64 = 11_000.0;

/// Accepted range of the engine anchor factor: the chart engines are generic
/// (+-0.02 reading, class spread), so a factor outside 0.7-1.4 says the
/// catalogue datum and the chart describe different machines.
const ENGINE_SCALE_RANGE: std::ops::RangeInclusive<f64> = 0.7..=1.4;

/// The maximum-climb table of `deck` and its engine anchor.
///
/// One factor per engine record, from engine data only (never from aircraft
/// results): when the deck's catalogue cruise thrust is a published datum of
/// the engine, it is taken as the engine's maximum-cruise thrust at the
/// reference point (the lower bound of that rating: the catalogue figure is a
/// cruise setting) and the chart's 0.93 x maximum climb is scaled onto it.
/// Without such a datum the chart is used unscaled. Returns the table, the
/// factor (`None` when unscaled) and the maximum-climb thrust at the
/// reference point, N; the error is the rejected factor.
pub(super) fn anchored_table(
    deck: &super::EmpiricalTurbofanDeck,
    reference_delta: f64,
) -> Result<(MaximumClimbTable, Option<f64>, f64), f64> {
    let chart = MaximumClimbTable::new(deck.bypass_ratio);
    let chart_n =
        deck.takeoff_thrust_n * chart.ratio(deck.max_climb_reference_mach, reference_delta);
    if !deck.cruise_reference_is_engine_datum {
        return Ok((chart, None, chart_n));
    }
    // A maximum-climb datum scales the maximum-climb chart directly; a
    // cruise datum is a maximum-cruise value, 0.93 of the chart there.
    let rating_over_climb = if deck.cruise_reference_is_maximum_climb {
        1.0
    } else {
        MAXIMUM_CRUISE_OVER_MAXIMUM_CLIMB
    };
    let factor = deck.max_climb_reference_thrust_n / (rating_over_climb * chart_n);
    if !factor.is_finite() || !ENGINE_SCALE_RANGE.contains(&factor) {
        return Err(factor);
    }
    Ok((chart.scaled(factor), Some(factor), chart_n * factor))
}

/// Linear interpolation of `values` (at ascending `knots`) at `x`, held at
/// the end values outside the knots.
pub(super) fn interpolate(knots: &[f64], values: &[f64], x: f64) -> f64 {
    let last = knots.len() - 1;
    if x <= knots[0] {
        return values[0];
    }
    if x >= knots[last] {
        return values[last];
    }
    let upper = knots.partition_point(|&knot| knot <= x);
    let weight = (x - knots[upper - 1]) / (knots[upper] - knots[upper - 1]);
    values[upper - 1] + weight * (values[upper] - values[upper - 1])
}

/// Pressure ratio at a height in feet (US 1976).
pub(super) fn delta_at_feet(feet: f64) -> f64 {
    standard_pressure_pa(feet * 0.3048) / 101_325.0
}

/// The maximum-climb thrust ratio of one engine over the whole envelope:
/// the four charts blended at the engine's bypass ratio on the common height
/// grid, evaluated by bilinear interpolation.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct MaximumClimbTable {
    /// Pressure ratios of the heights, ascending (45,000 ft first).
    deltas: [f64; 9],
    /// Thrust ratios at `deltas` (rows) and [CHART_MACH] (columns).
    ratios: [[f64; 7]; 9],
    /// The same engine's take-off charts (not scaled by the engine anchor:
    /// the static take-off thrust is the rated one).
    takeoff: TakeoffTable,
}

impl MaximumClimbTable {
    /// The table of an engine of bypass ratio `bpr`.
    pub(super) fn new(bpr: f64) -> Self {
        let bprs: Vec<f64> = MAXIMUM_CLIMB_CHARTS.iter().map(|chart| chart.bpr).collect();
        // Blend weights of the four charts.
        let mut weights = [0.0; 4];
        if bpr <= bprs[0] {
            weights[0] = 1.0;
        } else if bpr >= bprs[3] {
            weights[3] = 1.0;
        } else {
            let upper = bprs.partition_point(|&value| value <= bpr);
            let weight = (bpr - bprs[upper - 1]) / (bprs[upper] - bprs[upper - 1]);
            weights[upper - 1] = 1.0 - weight;
            weights[upper] = weight;
        }
        let mut deltas = [0.0; 9];
        let mut ratios = [[0.0; 7]; 9];
        for (index, &height_ft) in TABLE_HEIGHTS_FT.iter().rev().enumerate() {
            let delta = delta_at_feet(height_ft);
            deltas[index] = delta;
            for (chart, &weight) in MAXIMUM_CLIMB_CHARTS.iter().zip(&weights) {
                if weight == 0.0 {
                    continue;
                }
                let chart_deltas: Vec<f64> = chart
                    .rows
                    .iter()
                    .rev()
                    .map(|(feet, _)| delta_at_feet(*feet))
                    .collect();
                for (column, ratio) in ratios[index].iter_mut().enumerate() {
                    let values: Vec<f64> = chart
                        .rows
                        .iter()
                        .rev()
                        .map(|(_, row)| row[column])
                        .collect();
                    *ratio += weight * interpolate(&chart_deltas, &values, delta);
                }
            }
        }
        Self {
            deltas,
            ratios,
            takeoff: TakeoffTable::new(bpr),
        }
    }

    /// The table scaled by `factor` (the engine anchor, see
    /// `EmpiricalTurbofanModel::engine_scale`).
    pub(super) fn scaled(mut self, factor: f64) -> Self {
        for row in &mut self.ratios {
            for ratio in row {
                *ratio *= factor;
            }
        }
        self
    }

    /// Take-off thrust ratio `F / F_SLS` at `mach` and pressure ratio
    /// `delta` ([super::takeoff_chart]).
    pub(super) fn takeoff_ratio(&self, mach: f64, delta: f64) -> f64 {
        self.takeoff.ratio(mach, delta)
    }

    /// Thrust ratio `F / F_SLS` at `mach` and pressure ratio `delta`, held
    /// at the table ends (Mach 0.3-0.9, sea level to 45,000 ft).
    pub(super) fn ratio(&self, mach: f64, delta: f64) -> f64 {
        let mut columns = [0.0; 9];
        for (column, row) in columns.iter_mut().zip(&self.ratios) {
            *column = interpolate(&CHART_MACH, row, mach);
        }
        interpolate(&self.deltas, &columns, delta)
    }
}
