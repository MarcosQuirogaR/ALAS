// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Take-off thrust of the deck from Jenkinson's engine charts, and the
//! take-off thrust band.
//!
//! L. R. Jenkinson, P. Simpkin and D. Rhodes, "Civil Jet Aircraft Design",
//! Arnold (1999), Chapter 9, Figs. 9.18(a), 9.19(a), 9.20(a) and 9.21(a): the
//! take-off thrust ratio `FN / FN*` (`FN*` the sea-level static take-off
//! thrust) against Mach number for engines of bypass ratio 3.0, 6.5, 8.0 and
//! 13.0. Sea level, 5,000 ft and 10,000 ft for the BPR 6.5, 8.0 and 13.0
//! charts (Mach 0 to 0.4); sea level, 10,000 ft and 20,000 ft for BPR 3.0
//! (Mach 0 to 0.6). These are the same book, engines and axes as the
//! maximum-climb charts of [`super::max_climb`], so one engine model serves
//! take-off, climb and cruise (owner decision 2026-10-14).
//!
//! The numbers were read from the printed charts (PDF pages 218, 221, 224,
//! 227 of the book, rendered at 4x, the axes calibrated on the printed tick
//! marks, each curve located by its pixel row at the Mach nodes; reading
//! uncertainty +-0.01 of `FN / FN*`). The BPR 13.0 curves are printed only to
//! Mach 0.345 and again from 0.41 (the labels interrupt them): their Mach 0.4
//! value interpolates across that gap. Mach 0 is the curve start (the
//! sea-level curve starts at 1.0 by the chart's normalisation).
//!
//! The charts are not monotonic in bypass ratio (sea level, Mach 0.2: 0.827,
//! 0.822, 0.900, 0.791 for BPR 3, 6.5, 8, 13): the BPR 8 chart is the high
//! one. The interpolation in bypass ratio is the linear blend used for the
//! climb charts (nearest chart outside BPR 3-13).

use super::max_climb::{delta_at_feet, interpolate};

/// Mach numbers of the take-off chart nodes.
const TAKEOFF_CHART_MACH: [f64; 5] = [0.0, 0.1, 0.2, 0.3, 0.4];

/// One chart height: pressure altitude in feet and the thrust ratios at
/// [TAKEOFF_CHART_MACH].
type Row = (f64, [f64; 5]);

/// One take-off chart: bypass ratio and its height rows (ascending height).
struct Chart {
    bpr: f64,
    rows: &'static [Row],
}

/// Heights of the blended table, ft.
const TABLE_HEIGHTS_FT: [f64; 4] = [0.0, 5_000.0, 10_000.0, 20_000.0];

/// Take-off thrust over static take-off thrust, Jenkinson Figs. 9.18(a),
/// 9.19(a), 9.20(a), 9.21(a).
const TAKEOFF_CHARTS: [Chart; 4] = [
    Chart {
        bpr: 3.0,
        rows: &[
            (0.0, [1.000, 0.903, 0.827, 0.771, 0.719]),
            (10_000.0, [0.713, 0.657, 0.600, 0.568, 0.538]),
            (20_000.0, [0.497, 0.459, 0.423, 0.399, 0.383]),
        ],
    },
    Chart {
        bpr: 6.5,
        rows: &[
            (0.0, [1.000, 0.908, 0.822, 0.764, 0.706]),
            (5_000.0, [0.866, 0.797, 0.728, 0.678, 0.630]),
            (10_000.0, [0.742, 0.689, 0.635, 0.597, 0.560]),
        ],
    },
    Chart {
        bpr: 8.0,
        rows: &[
            (0.0, [1.000, 0.948, 0.900, 0.853, 0.807]),
            (5_000.0, [0.877, 0.798, 0.744, 0.694, 0.661]),
            (10_000.0, [0.711, 0.664, 0.623, 0.594, 0.574]),
        ],
    },
    Chart {
        bpr: 13.0,
        rows: &[
            (0.0, [1.000, 0.888, 0.791, 0.711, 0.649]),
            (5_000.0, [0.884, 0.798, 0.714, 0.647, 0.590]),
            (10_000.0, [0.785, 0.709, 0.637, 0.579, 0.531]),
        ],
    },
];

/// Half the spread between the two take-off thrust sources over their mean,
/// the take-off thrust band: 0.082.
///
/// The two sources are the Bartel-Young/OpenAP take-off correlation
/// ([`super::flat_rating::bartel_young_takeoff_ratio`], the previous deck) and
/// the Jenkinson charts above. The band is the largest of
/// `|J - BY| / (J + BY)` (half-range over mean) over the whole fleet and the
/// whole V2 domain, not over one aircraft: the bypass ratios of the catalogue
/// engines (4.4 CF6-50 to 11.37 PW1500G, 11 values), pressure ratios 1.0, 0.85
/// and 0.70 (sea level to about 3,000 m aerodromes) and Mach 0.20, 0.25, 0.30
/// (the V2 range of the transport presets). It is reached at BPR 8.5 (the
/// Trent 900/970, on the high BPR 8 chart) and Mach 0.30 at sea level: 0.081.
/// `the_take_off_band_is_the_largest_half_spread_of_the_two_sources` recomputes
/// it. It is an envelope of the two sources, not a standard deviation, it
/// excludes the +-0.01 chart reading uncertainty, and it was derived before
/// the verdict of any aircraft was examined.
pub const TAKEOFF_THRUST_SCATTER: f64 = 0.082;

/// The take-off chart table of one engine: the four charts blended at the
/// engine's bypass ratio on the common height grid.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TakeoffTable {
    /// Pressure ratios of the heights, ascending (20,000 ft first).
    deltas: [f64; 4],
    /// Thrust ratios at `deltas` (rows) and [TAKEOFF_CHART_MACH] (columns).
    ratios: [[f64; 5]; 4],
}

impl TakeoffTable {
    /// The table of an engine of bypass ratio `bpr`.
    pub(super) fn new(bpr: f64) -> Self {
        let bprs: Vec<f64> = TAKEOFF_CHARTS.iter().map(|chart| chart.bpr).collect();
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
        let mut deltas = [0.0; 4];
        let mut ratios = [[0.0; 5]; 4];
        for (index, &height_ft) in TABLE_HEIGHTS_FT.iter().rev().enumerate() {
            let delta = delta_at_feet(height_ft);
            deltas[index] = delta;
            for (chart, &weight) in TAKEOFF_CHARTS.iter().zip(&weights) {
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
                    // Above the highest printed curve the thrust is taken in
                    // proportion to the ambient pressure (Torenbeek 2013,
                    // Sec. 8.5), held from that curve.
                    let value = if delta < chart_deltas[0] {
                        values[0] * delta / chart_deltas[0]
                    } else {
                        interpolate(&chart_deltas, &values, delta)
                    };
                    *ratio += weight * value;
                }
            }
        }
        Self { deltas, ratios }
    }

    /// Thrust ratio `F / F_SLS` at `mach` and pressure ratio `delta`: held at
    /// the Mach ends (0 to 0.4) and at the sea-level end above it, in
    /// proportion to pressure below the 20,000 ft node.
    pub(super) fn ratio(&self, mach: f64, delta: f64) -> f64 {
        let mut columns = [0.0; 4];
        for (column, row) in columns.iter_mut().zip(&self.ratios) {
            *column = interpolate(&TAKEOFF_CHART_MACH, row, mach);
        }
        if delta < self.deltas[0] {
            return columns[0] * delta / self.deltas[0];
        }
        interpolate(&self.deltas, &columns, delta)
    }
}

/// Both take-off thrust sources at one condition, `F / F_SLS`: the Jenkinson
/// chart ratio the deck uses and the Bartel-Young/OpenAP correlation it
/// replaced. `delta` is the ambient pressure ratio, `mach` the flight Mach
/// (both read at the take-off rating, flat-rating break excluded).
pub fn takeoff_thrust_sources(bpr: f64, delta: f64, mach: f64) -> (f64, f64) {
    let chart = TakeoffTable::new(bpr).ratio(mach.min(0.4), delta);
    let correlation = super::flat_rating::bartel_young_takeoff_ratio(bpr, delta, mach.min(0.4));
    (chart, correlation)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bypass ratios of the catalogue engines (`engines.json`), 4.4 to 11.37.
    const FLEET_BPR: [f64; 11] = [4.4, 5.1, 5.5, 6.6, 8.5, 9.6, 10.0, 10.56, 11.0, 11.2, 11.37];

    #[test]
    fn the_table_reproduces_the_take_off_charts_at_their_nodes() {
        for chart in &TAKEOFF_CHARTS {
            let table = TakeoffTable::new(chart.bpr);
            for (feet, row) in chart.rows {
                for (&mach, &expected) in TAKEOFF_CHART_MACH.iter().zip(row) {
                    let got = table.ratio(mach, delta_at_feet(*feet));
                    assert!(
                        (got - expected).abs() < 1e-12,
                        "BPR {} {feet} ft M{mach}: {got} against {expected}",
                        chart.bpr
                    );
                }
            }
        }
    }

    #[test]
    fn every_engine_is_exactly_its_static_rating_at_sea_level_static() {
        for bpr in [0.0, 3.0, 4.4, 8.0, 10.0, 13.0, 20.0] {
            assert_eq!(TakeoffTable::new(bpr).ratio(0.0, 1.0), 1.0, "BPR {bpr}");
        }
    }

    #[test]
    fn thrust_falls_with_mach_and_with_height_on_every_blend() {
        for bpr in FLEET_BPR {
            let table = TakeoffTable::new(bpr);
            let mut previous_row = [f64::INFINITY; 5];
            for feet in [0.0, 2_500.0, 5_000.0, 10_000.0, 20_000.0, 30_000.0] {
                let delta = delta_at_feet(feet);
                let mut previous = f64::INFINITY;
                for (column, mach) in [0.0, 0.1, 0.2, 0.3, 0.4].into_iter().enumerate() {
                    let value = table.ratio(mach, delta);
                    assert!(value < previous, "BPR {bpr} {feet} ft M{mach}");
                    assert!(value < previous_row[column], "BPR {bpr} {feet} ft M{mach}");
                    previous = value;
                    previous_row[column] = value;
                }
            }
        }
    }

    #[test]
    fn above_the_printed_curves_thrust_is_proportional_to_pressure() {
        let table = TakeoffTable::new(10.0);
        let top = delta_at_feet(20_000.0);
        let half = table.ratio(0.2, 0.5 * top);
        assert!((half / table.ratio(0.2, top) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn bypass_ratio_blends_linearly_and_is_held_outside_the_charts() {
        let (low, high) = (TakeoffTable::new(8.0), TakeoffTable::new(13.0));
        let blend = TakeoffTable::new(10.0);
        let expected = 0.6 * low.ratio(0.25, 1.0) + 0.4 * high.ratio(0.25, 1.0);
        assert!((blend.ratio(0.25, 1.0) - expected).abs() < 1e-12);
        assert_eq!(TakeoffTable::new(1.0), TakeoffTable::new(3.0));
        assert_eq!(TakeoffTable::new(20.0), TakeoffTable::new(13.0));
    }

    #[test]
    fn the_take_off_band_is_the_largest_half_spread_of_the_two_sources() {
        let mut largest: f64 = 0.0;
        for bpr in FLEET_BPR {
            for delta in [1.0, 0.85, 0.70] {
                for mach in [0.20, 0.25, 0.30] {
                    let (chart, correlation) = takeoff_thrust_sources(bpr, delta, mach);
                    largest = largest.max((chart - correlation).abs() / (chart + correlation));
                }
            }
        }
        assert!(
            TAKEOFF_THRUST_SCATTER >= largest && TAKEOFF_THRUST_SCATTER < largest + 0.002,
            "largest half-spread {largest}"
        );
    }
}
