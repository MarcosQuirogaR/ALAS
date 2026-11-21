// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! In-flight part-power TSFC of the deck, from Jenkinson's SFC loops.
//!
//! L. R. Jenkinson, P. Simpkin and D. Rhodes, "Civil Jet Aircraft Design",
//! Arnold (1999), Chapter 9, Figs. 9.19(e) (bypass ratio 6.5, printed p. 208),
//! 9.20(e) (8.0, p. 211) and 9.21(e) (13.0, p. 214): the corrected specific
//! fuel consumption `SFC / theta^n` against the corrected net thrust
//! `FN / (delta FN*)` (`FN*` the sea-level static take-off thrust), one curve
//! per flight Mach number (static, 0.25, 0.45, 0.65, 0.85, 0.95). The same
//! chapter's charts (b)-(d), (f) already give the deck its maximum-climb,
//! maximum-cruise, take-off and idle ratings ([super::max_climb],
//! [super::takeoff_chart], [super::flight_idle]).
//!
//! The loops show what the sea-level-static ICAO LTO ratios cannot: in flight
//! the net thrust is the gross thrust less the inlet ram drag, which takes a
//! growing share of the gross thrust as the engine is throttled back, so the
//! TSFC rises below the bucket of each loop at every flight Mach number
//! (BPR 8.0, M 0.25: 0.356 at `FN/(delta FN*)` 0.55, 0.430 at 0.20; M 0.45:
//! 0.443 at 0.65, 0.539 at 0.20), while the static loop has the ICAO dip
//! (lowest near 0.4-0.6). Only the static curve agrees with the ICAO shape.
//!
//! The numbers below were read from the printed charts (rendered at 340 dpi,
//! the axes calibrated on their printed tick marks, each curve tracked
//! column by column from a seed between the labels and resampled at
//! `FN/(delta FN*)` steps of 0.05 as the median of the trace within +-0.01;
//! reading uncertainty about +-0.005 of `SFC / theta^n`).
//!
//! The BPR 3.0 chart (Fig. 9.18(e), p. 205) is not in the blend: its
//! abscissa is printed "Mach number" (0.3-1.0) and its curves labelled
//! "Airspeed (Mach no.)" 0.2-0.9, so the page does not state its thrust
//! axis. Read as a sensitivity check with the abscissa taken as
//! `FN/(delta FN*)` (evidently so: the layout is that of the other loop
//! charts), its curves start at 0.3 and rise there by about 15 % (M 0.2) and
//! 19 % (M 0.5) over their buckets near 0.7 (spot reads, about +-3 % on the
//! ratio), against 9 % (M 0.25) and 14 % (M 0.45) on the BPR 6.5 chart at the
//! same 0.3. Engines below BPR 6.5 (CFM56-5B4 5.5, CF6-80C2 5.1, CF6-50 4.3,
//! PW305A 4.4) are held at the BPR 6.5 chart, so their part-power rise is,
//! if anything, understated.
//!
//! Only the shape is used, as the ratio of the loop at the flown corrected
//! thrust to the loop at the lesser of its bucket and the full rating at the
//! same Mach number: the absolute level of the 1990s chart engines is not
//! carried (the deck's TSFC level stays on the engine's own ICAO and cruise
//! anchors). Above the bucket the ratio is one, as the deck has always held
//! it at cruise (the loops are flat within +-2 % there, the reading
//! uncertainty of their right-hand branches). The ratio is temperature-free:
//! the charts plot `SFC / theta^n` against `FN/(delta FN*)` at one Mach
//! number, so at a fixed `FN/(delta FN*)` and Mach the theta factor cancels.
//!
//! Interpolation: linear in fuel flow (`SFC x FN`) between the 0.05 nodes
//! of a curve (the Willans-line form of the ICAO fuel schedule), the SFC held
//! at the ends of each printed curve (as the other chart tables of the deck
//! hold their ends); linear in Mach number between the curves (held below
//! 0.25 and above 0.95; below 0.25 the caller blends with the static ICAO
//! shape, see [super::installation::tsfc_shape]); linear in bypass ratio
//! between the three charts (nearest chart outside 6.5-13.0).
//!
//! Extrapolation: a 30 min hold flies at about 0.13-0.16 of `delta FN*`.
//! The BPR 8.0 and 13.0 loops are printed from 0.20 (M 0.25, 0.45) or
//! 0.15-0.35 (higher Mach numbers), so for engines of BPR 8 and above the
//! hold point lies at or below the first printed node and takes that node's
//! SFC held flat: the rise it gets is a lower bound of the loop's own
//! continuation (which keeps rising towards low thrust on every chart).

/// One printed loop: its Mach number and the corrected SFC at
/// `FN/(delta FN*)` = `x_first`, `x_first` + 0.05, ...
struct LoopCurve {
    mach: f64,
    x_first: f64,
    sfc: &'static [f64],
}

/// One engine chart: its bypass ratio and the flight loops in ascending Mach.
struct LoopChart {
    bpr: f64,
    curves: [LoopCurve; 5],
}

/// Node spacing of the printed-curve samples and of the blended grid.
const STEP: f64 = 0.05;
/// First and last `FN/(delta FN*)` node of the blended grid.
const GRID_FIRST: f64 = 0.15;
const GRID_NODES: usize = 22;
/// Mach numbers of the flight loops (all three charts print the same set).
pub(super) const LOOP_MACH: [f64; 5] = [0.25, 0.45, 0.65, 0.85, 0.95];
/// Lowest Mach number with a flight loop.
pub(super) const LOOP_MINIMUM_MACH: f64 = 0.25;

/// `SFC / theta^n` read from Jenkinson Figs. 9.19(e), 9.20(e), 9.21(e).
#[rustfmt::skip]
const SFC_LOOP_CHARTS: [LoopChart; 3] = [
    // BPR 6.5, Fig. 9.19(e), printed p. 208
    LoopChart {
        bpr: 6.5,
        curves: [
            LoopCurve { mach: 0.25, x_first: 0.15, sfc: &[0.456, 0.434, 0.413, 0.395, 0.382, 0.371, 0.365, 0.362, 0.361, 0.363, 0.370, 0.376, 0.386, 0.397, 0.409] },
            LoopCurve { mach: 0.45, x_first: 0.15, sfc: &[0.600, 0.564, 0.524, 0.494, 0.469, 0.451, 0.440, 0.434, 0.434, 0.437, 0.442, 0.449, 0.459, 0.472] },
            LoopCurve { mach: 0.65, x_first: 0.15, sfc: &[0.741, 0.695, 0.643, 0.603, 0.576, 0.560, 0.549, 0.543, 0.539, 0.540, 0.541, 0.545, 0.554, 0.568, 0.580] },
            LoopCurve { mach: 0.85, x_first: 0.20, sfc: &[0.831, 0.790, 0.745, 0.711, 0.684, 0.663, 0.649, 0.639, 0.631, 0.627, 0.627, 0.630, 0.638, 0.650] },
            LoopCurve { mach: 0.95, x_first: 0.25, sfc: &[0.839, 0.795, 0.759, 0.728, 0.703, 0.684, 0.671, 0.665, 0.663, 0.663, 0.665, 0.674, 0.682] },
        ],
    },
    // BPR 8.0, Fig. 9.20(e), printed p. 211
    LoopChart {
        bpr: 8.0,
        curves: [
            LoopCurve { mach: 0.25, x_first: 0.20, sfc: &[0.430, 0.411, 0.394, 0.377, 0.368, 0.362, 0.357, 0.356, 0.358, 0.361, 0.364, 0.368, 0.374, 0.380, 0.387, 0.393] },
            LoopCurve { mach: 0.45, x_first: 0.20, sfc: &[0.539, 0.513, 0.492, 0.475, 0.464, 0.457, 0.451, 0.447, 0.444, 0.443, 0.444, 0.447, 0.452, 0.459, 0.468] },
            LoopCurve { mach: 0.65, x_first: 0.30, sfc: &[0.589, 0.577, 0.563, 0.552, 0.545, 0.538, 0.534, 0.532, 0.532, 0.532, 0.536, 0.543, 0.551] },
            LoopCurve { mach: 0.85, x_first: 0.35, sfc: &[0.685, 0.672, 0.659, 0.649, 0.641, 0.637, 0.632, 0.630, 0.630, 0.632, 0.637] },
            LoopCurve { mach: 0.95, x_first: 0.30, sfc: &[0.747, 0.732, 0.716, 0.702, 0.690, 0.680, 0.673, 0.670, 0.668, 0.670, 0.674] },
        ],
    },
    // BPR 13.0, Fig. 9.21(e), printed p. 214
    LoopChart {
        bpr: 13.0,
        curves: [
            LoopCurve { mach: 0.25, x_first: 0.20, sfc: &[0.399, 0.379, 0.363, 0.352, 0.344, 0.338, 0.334, 0.331, 0.328, 0.330, 0.334, 0.338, 0.343, 0.348, 0.354, 0.361, 0.370] },
            LoopCurve { mach: 0.45, x_first: 0.20, sfc: &[0.536, 0.505, 0.476, 0.456, 0.440, 0.431, 0.426, 0.424, 0.422, 0.424, 0.427, 0.433, 0.439, 0.446, 0.456] },
            LoopCurve { mach: 0.65, x_first: 0.15, sfc: &[0.749, 0.693, 0.642, 0.605, 0.575, 0.553, 0.539, 0.529, 0.525, 0.523, 0.524, 0.526, 0.531, 0.538, 0.546, 0.554, 0.563, 0.573] },
            LoopCurve { mach: 0.85, x_first: 0.30, sfc: &[0.728, 0.699, 0.677, 0.660, 0.645, 0.633, 0.623, 0.619, 0.615, 0.613, 0.613, 0.616, 0.624, 0.635, 0.647] },
            LoopCurve { mach: 0.95, x_first: 0.30, sfc: &[0.788, 0.754, 0.728, 0.707, 0.689, 0.677, 0.668, 0.662, 0.657, 0.657, 0.660, 0.665, 0.674, 0.686, 0.702] },
        ],
    },
];

/// `FN/(delta FN*)` of node `index` of the blended grid.
fn grid_x(index: usize) -> f64 {
    GRID_FIRST + STEP * index as f64
}

/// Corrected SFC of `curve` at `x`: linear in fuel flow between its nodes,
/// the SFC held at its printed ends.
fn curve_sfc(curve: &LoopCurve, x: f64) -> f64 {
    let last = curve.sfc.len() - 1;
    let x_last = curve.x_first + STEP * last as f64;
    if x <= curve.x_first {
        return curve.sfc[0];
    }
    if x >= x_last {
        return curve.sfc[last];
    }
    let position = (x - curve.x_first) / STEP;
    let lower = (position.floor() as usize).min(last - 1);
    let x_lower = curve.x_first + STEP * lower as f64;
    let x_upper = x_lower + STEP;
    let fuel_lower = curve.sfc[lower] * x_lower;
    let fuel_upper = curve.sfc[lower + 1] * x_upper;
    let weight = (x - x_lower) / STEP;
    (fuel_lower + weight * (fuel_upper - fuel_lower)) / x
}

/// The SFC loops of one engine: the three charts blended at its bypass ratio
/// on a common `FN/(delta FN*)` grid, one row per flight Mach number.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SfcLoopTable {
    /// Corrected SFC at [LOOP_MACH] (rows) and the grid nodes (columns).
    sfc: [[f64; GRID_NODES]; 5],
}

impl SfcLoopTable {
    /// The table of an engine of bypass ratio `bpr`.
    pub(super) fn new(bpr: f64) -> Self {
        let mut weights = [0.0; 3];
        let first = SFC_LOOP_CHARTS[0].bpr;
        let last = SFC_LOOP_CHARTS[2].bpr;
        if bpr.is_nan() || bpr <= first {
            weights[0] = 1.0;
        } else if bpr >= last {
            weights[2] = 1.0;
        } else {
            let upper = SFC_LOOP_CHARTS.partition_point(|chart| chart.bpr <= bpr);
            let weight = (bpr - SFC_LOOP_CHARTS[upper - 1].bpr)
                / (SFC_LOOP_CHARTS[upper].bpr - SFC_LOOP_CHARTS[upper - 1].bpr);
            weights[upper - 1] = 1.0 - weight;
            weights[upper] = weight;
        }
        let mut sfc = [[0.0; GRID_NODES]; 5];
        for (row, values) in sfc.iter_mut().enumerate() {
            // Every chart prints its loops at the same Mach numbers.
            debug_assert!(SFC_LOOP_CHARTS
                .iter()
                .all(|chart| (chart.curves[row].mach - LOOP_MACH[row]).abs() < 1.0e-12));
            for (column, value) in values.iter_mut().enumerate() {
                let x = grid_x(column);
                *value = SFC_LOOP_CHARTS
                    .iter()
                    .zip(weights)
                    .filter(|(_, weight)| *weight > 0.0)
                    .map(|(chart, weight)| weight * curve_sfc(&chart.curves[row], x))
                    .sum();
            }
        }
        Self { sfc }
    }

    /// The blended row at `mach` (linear between the loops, held outside).
    fn row_at(&self, mach: f64) -> [f64; GRID_NODES] {
        let mach = mach.clamp(LOOP_MACH[0], LOOP_MACH[4]);
        let upper = LOOP_MACH
            .partition_point(|&value| value <= mach)
            .clamp(1, 4);
        let weight = (mach - LOOP_MACH[upper - 1]) / (LOOP_MACH[upper] - LOOP_MACH[upper - 1]);
        let mut row = [0.0; GRID_NODES];
        for (column, value) in row.iter_mut().enumerate() {
            *value = self.sfc[upper - 1][column]
                + weight * (self.sfc[upper][column] - self.sfc[upper - 1][column]);
        }
        row
    }

    /// Corrected SFC of `row` at `x`: linear in fuel flow between the grid
    /// nodes, held at the grid ends.
    fn row_sfc(row: &[f64; GRID_NODES], x: f64) -> f64 {
        let x_last = grid_x(GRID_NODES - 1);
        if x <= GRID_FIRST {
            return row[0];
        }
        if x >= x_last {
            return row[GRID_NODES - 1];
        }
        let lower = (((x - GRID_FIRST) / STEP).floor() as usize).min(GRID_NODES - 2);
        let x_lower = grid_x(lower);
        let x_upper = grid_x(lower + 1);
        let weight = (x - x_lower) / STEP;
        (row[lower] * x_lower + weight * (row[lower + 1] * x_upper - row[lower] * x_lower)) / x
    }

    /// TSFC at the corrected thrust `x` = `FN/(delta FN*)` over TSFC at the
    /// reference point of the same loop, at flight Mach number `mach`.
    ///
    /// The reference is the lesser of the loop's bucket (its lowest node)
    /// and `full_rating_x`, the corrected thrust of the rating the
    /// utilization is a share of: at and above it the ratio is one, below
    /// it the loop's rise. So the full rating keeps its anchored TSFC
    /// exactly, and the shape is continuous.
    pub(super) fn part_power_ratio(&self, x: f64, full_rating_x: f64, mach: f64) -> f64 {
        let row = self.row_at(mach);
        let bucket_x = row
            .iter()
            .enumerate()
            .fold((0, f64::INFINITY), |best, (column, &value)| {
                if value < best.1 {
                    (column, value)
                } else {
                    best
                }
            })
            .0;
        let reference_x = grid_x(bucket_x).min(full_rating_x);
        if x.is_nan() || x >= reference_x {
            return 1.0;
        }
        let reference = Self::row_sfc(&row, reference_x);
        if reference.is_nan() || reference <= 0.0 {
            return 1.0;
        }
        Self::row_sfc(&row, x) / reference
    }
}

// A test asserts on values it constructed here directly.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printed_curves_are_reproduced_at_their_nodes() {
        let curve = &SFC_LOOP_CHARTS[1].curves[0];
        assert!((curve_sfc(curve, 0.20) - 0.430).abs() < 1.0e-12);
        assert!((curve_sfc(curve, 0.55) - 0.356).abs() < 1.0e-12);
        // Held at the printed ends.
        assert!((curve_sfc(curve, 0.10) - 0.430).abs() < 1.0e-12);
        let table = SfcLoopTable::new(8.0);
        assert!((table.sfc[0][1] - 0.430).abs() < 1.0e-12);
    }

    #[test]
    fn the_flight_loops_rise_below_the_bucket_and_are_flat_above_it() {
        for bpr in [5.1, 6.5, 8.0, 10.56, 13.0] {
            let table = SfcLoopTable::new(bpr);
            for mach in [0.25, 0.35, 0.6, 0.8, 0.85] {
                // Full rating and the bucket carry no change.
                assert_eq!(table.part_power_ratio(0.8, 0.8, mach), 1.0);
                assert_eq!(table.part_power_ratio(0.9, 0.8, mach), 1.0);
                // Holding-like corrected thrust: a rise, monotone downward.
                let hold = table.part_power_ratio(0.20, 0.6, mach);
                let lower = table.part_power_ratio(0.15, 0.6, mach);
                assert!(hold > 1.05, "BPR {bpr} M{mach}: {hold}");
                // Below the first printed node the curve is held flat, so the two may be equal.
                assert!(lower >= hold - 1.0e-12, "BPR {bpr} M{mach}: {lower} < {hold}");
                assert!(hold < 1.6, "BPR {bpr} M{mach}: {hold}");
            }
        }
    }
}
