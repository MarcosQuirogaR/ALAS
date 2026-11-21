// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Flight-idle (descent) net thrust and fuel flow of the deck, from
//! Jenkinson's engine charts.
//!
//! L. R. Jenkinson, P. Simpkin and D. Rhodes, "Civil Jet Aircraft Design",
//! Arnold (1999), Chapter 9, for engines of bypass ratio 3.0, 6.5, 8.0 and
//! 13.0 (the same four engines whose maximum-climb charts [super::max_climb]
//! reads):
//!
//! * descent thrust `FN / FN*`, Figs. 9.18(d) (printed p. 204), 9.19(d) (p.
//!   207), 9.20(d) (p. 210) and 9.21(d) (p. 213): the **net** thrust (gross
//!   thrust minus the inlet ram drag) of the engine at its flight-idle
//!   setting against Mach number, one curve per height, `FN*` the sea-level
//!   static take-off thrust. It is small and, at low height and high Mach
//!   number, **negative**: at idle the engine's gross thrust no longer pays
//!   for the momentum drag of the air it swallows (BPR 6.5 at sea level:
//!   `+0.001` at M 0.3 and `-0.039` at M 0.8);
//! * descent fuel flow `lb/hr/FN*`, Figs. 9.18(f) (p. 205), 9.19(f) (p. 208),
//!   9.20(f) (p. 211) and 9.21(f) (p. 214), against Mach number and height.
//!
//! The earlier deck took flight idle as 7 % of the take-off thrust available
//! at the condition (`+0.03` to `+0.06` of `FN*` at low height), which has the
//! wrong sign below about 25,000 ft and made every widebody descent a
//! 1,000-1,200 ft/min near-glide.
//!
//! **How the numbers were read.** The charts were rendered from the book's
//! PDF at 4x, the axes calibrated on their printed tick marks and each curve
//! traced by its pixel rows (the method of [super::max_climb]); the traces
//! were overlaid on the rendered pages and checked by eye. Reading
//! uncertainty: `+-0.002` of `FN*` on the thrust ratio, `+-2 %` on the fuel
//! ratio. Notes on the sources, none of which changes a number:
//!
//! * the BPR 8.0 chart prints its height legend as "(10 000 ft)"; the labels
//!   45, 40, ... 10, SL are 45,000 ft ... sea level as on every other chart;
//! * the BPR 6.5 and 8.0 fuel charts print their lowest axis label as "0.050"
//!   and "0.004"; the tick positions make them 0.005 (equal spacing in the
//!   charts' linear axis), and the curves were calibrated on the three
//!   unambiguous labels above them;
//! * on the thrust charts all curves of one engine cross at one point near
//!   M 0.2-0.27 (BPR 6.5: M 0.20, 0.0098; BPR 13: M 0.225, 0.0047) and, for
//!   BPR 8, the SL, 10,000 and 15,000 ft curves at M 0.23 (the 25,000-45,000 ft
//!   curves fan out below that point and are read column by column at M 0.2 and
//!   0.3); the traces are reliable to the right of it (M >= 0.5). The values at M 0.3 and 0.4 of BPR 6.5 and 13 (and of the
//!   BPR 3 curves that pass through it, M 0.265, 0.0) are the straight lines
//!   from that common point to the traced M 0.5 value, which is how the
//!   charts draw them; where curves of one engine merge into one line (BPR 8
//!   at 30,000-45,000 ft from M 0.4; BPR 6.5 at 40,000 ft) they share the traced value,
//!   and the BPR 6.5 40,000 ft curve is held non-increasing in Mach number;
//! * the lowest Mach node, 0.2 (approach and landing, where the net idle
//!   thrust is positive, a few per cent of `FN*`), is the straight line from
//!   each curve's left end (read at the axis) to the chart's crossing point
//!   `P` (README of the trace directory lists every reading), for the curves
//!   that pass through it; the Mach 0.3 node of the BPR 8.0 chart is the
//!   straight line from `P` to its traced Mach 0.4 value, as the page draws
//!   it, for the SL, 10,000 and 15,000 ft curves. Below Mach 0.2 the table holds the
//!   Mach 0.2 value. The BPR 3.0 sea-level curve is printed flat from Mach
//!   0.70 to its end and the table holds it;
//! * the thrust curves end at M 0.82-0.85, so the last node (0.8) is held to
//!   0.9, which understates the net drag growth between 0.8 and 0.9; the fuel
//!   curves of the BPR 6.5 and 13 charts reach 0.9 and carry their own value
//!   there, those of BPR 3.0 and 8.0 end near 0.83 and hold theirs;
//! * the idle fuel flow is anchored on the engine's ICAO idle point (7 % of the
//!   rated thrust, sea-level static) and carried by the chart's shape, while the
//!   idle *thrust* at static conditions is the chart's Mach 0.2 value (0.5-2.9 %
//!   of `FN*`, held below Mach 0.2): the two are not the same operating point
//!   (ground idle at 7 % thrust against flight idle) and only the flight phases
//!   (Mach >= 0.2) read the table;
//! * the BPR 3.0 fuel chart is not monotone in height (30,000 ft below
//!   40,000 ft); no BPR 3.0 engine is in the catalogue, and the chart enters
//!   only by interpolation in bypass ratio below BPR 6.5.
//!
//! One form serves every engine, as for the climb charts: linear interpolation
//! in Mach number (held at the end nodes), in pressure ratio between the
//! chart heights and in bypass ratio between the four charts (nearest chart
//! outside BPR 3-13). The charts are of generic 1990s engines; the net idle
//! thrust of one engine differs from them by the engine's own idle schedule.

mod charts;

use super::max_climb::{delta_at_feet, interpolate, TABLE_HEIGHTS_FT};
use charts::{DESCENT_FUEL_CHARTS, DESCENT_THRUST_CHARTS};

/// Mach numbers of the thrust-chart samples.
const THRUST_MACH: [f64; 7] = [0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
/// Mach numbers of the fuel-chart samples (the first node holds below it).
const FUEL_MACH: [f64; 9] = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];

/// One engine chart: bypass ratio and its height rows (ascending height).
pub(super) struct Chart<const N: usize> {
    pub(super) bpr: f64,
    pub(super) rows: &'static [(f64, [f64; N])],
}

/// A chart family blended at one bypass ratio on the common height grid.
#[derive(Debug, Clone, PartialEq)]
struct BlendedTable<const N: usize> {
    /// Pressure ratios of the heights, ascending (45,000 ft first).
    deltas: [f64; 9],
    /// Chart values at `deltas` (rows) and the chart's Mach nodes (columns).
    values: [[f64; N]; 9],
}

impl<const N: usize> BlendedTable<N> {
    fn new(charts: &[Chart<N>; 4], bpr: f64) -> Self {
        let bprs: Vec<f64> = charts.iter().map(|chart| chart.bpr).collect();
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
        let mut values = [[0.0; N]; 9];
        for (index, &height_ft) in TABLE_HEIGHTS_FT.iter().rev().enumerate() {
            let delta = delta_at_feet(height_ft);
            deltas[index] = delta;
            for (chart, &weight) in charts.iter().zip(&weights) {
                if weight == 0.0 {
                    continue;
                }
                let chart_deltas: Vec<f64> = chart
                    .rows
                    .iter()
                    .rev()
                    .map(|(feet, _)| delta_at_feet(*feet))
                    .collect();
                for (column, value) in values[index].iter_mut().enumerate() {
                    let column_values: Vec<f64> = chart
                        .rows
                        .iter()
                        .rev()
                        .map(|(_, row)| row[column])
                        .collect();
                    *value += weight * interpolate(&chart_deltas, &column_values, delta);
                }
            }
        }
        Self { deltas, values }
    }

    /// Bilinear value at `mach` (nodes `nodes`, held at the ends) and
    /// pressure ratio `delta` (held at the grid ends).
    fn value(&self, nodes: &[f64; N], mach: f64, delta: f64) -> f64 {
        let mut column = [0.0; 9];
        for (entry, row) in column.iter_mut().zip(&self.values) {
            *entry = interpolate(nodes, row, mach);
        }
        interpolate(&self.deltas, &column, delta)
    }
}

#[cfg(test)]
/// Pound-force hour consumption per pound-force to kg/(N s): `(0.45359237 /
/// 3600) / 4.4482216152605` (international pound, lbf = 4.4482216152605 N).
const LB_PER_HR_PER_LBF_TO_KG_PER_N_S: f64 = 0.453_592_37 / 3_600.0 / 4.448_221_615_260_5;

/// The flight-idle net thrust and fuel flow of one engine, per unit of its
/// sea-level static take-off thrust.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct FlightIdleTable {
    thrust: BlendedTable<7>,
    fuel: BlendedTable<9>,
    /// The chart fuel flow at sea level and Mach 0 (held at the lowest node),
    /// the denominator of [`Self::fuel_flow_over_static`].
    fuel_static: f64,
}

impl FlightIdleTable {
    /// The table of an engine of bypass ratio `bpr`.
    pub(super) fn new(bpr: f64) -> Self {
        let fuel = BlendedTable::new(&DESCENT_FUEL_CHARTS, bpr);
        let fuel_static = fuel.value(&FUEL_MACH, 0.0, 1.0);
        Self {
            thrust: BlendedTable::new(&DESCENT_THRUST_CHARTS, bpr),
            fuel,
            fuel_static,
        }
    }

    /// Net idle thrust over the sea-level static take-off thrust, `FN / FN*`
    /// (negative where the ram drag exceeds the idle gross thrust).
    pub(super) fn net_thrust_ratio(&self, mach: f64, delta: f64) -> f64 {
        self.thrust.value(&THRUST_MACH, mach, delta)
    }

    /// Idle fuel flow at `mach` and pressure ratio `delta` over the idle
    /// fuel flow at sea-level static, dimensionless: the chart value there
    /// over the chart value at sea level and the lowest Mach node (held to
    /// Mach 0).
    pub(super) fn fuel_flow_over_static(&self, mach: f64, delta: f64) -> f64 {
        self.fuel.value(&FUEL_MACH, mach, delta) / self.fuel_static
    }

    /// The chart idle fuel flow, kg/(N s) of `FN*`, as printed (generic
    /// engine, lb/hr/lbf of `FN*`).
    #[cfg(test)]
    pub(super) fn chart_fuel_kg_n_s(&self, mach: f64, delta: f64) -> f64 {
        self.fuel.value(&FUEL_MACH, mach, delta) * LB_PER_HR_PER_LBF_TO_KG_PER_N_S
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEA_LEVEL_DELTA: f64 = 1.0;

    fn delta(feet: f64) -> f64 {
        delta_at_feet(feet)
    }

    #[test]
    fn the_charts_are_read_where_they_were_traced() {
        // BPR 6.5, sea level, M 0.8: Fig. 9.19(d) -0.039 (p. 207).
        let table = FlightIdleTable::new(6.5);
        let ratio = table.net_thrust_ratio(0.8, SEA_LEVEL_DELTA);
        assert!((ratio + 0.0394).abs() < 1.0e-9, "{ratio}");
        // BPR 13, sea level, M 0.8: Fig. 9.21(d), -0.059 (p. 213).
        let table = FlightIdleTable::new(13.0);
        assert!((table.net_thrust_ratio(0.8, SEA_LEVEL_DELTA) + 0.0591).abs() < 1.0e-9);
    }

    #[test]
    fn ram_drag_makes_the_idle_thrust_of_a_high_bypass_engine_negative_at_low_height_and_high_mach()
    {
        let table = FlightIdleTable::new(5.9);
        let low = table.net_thrust_ratio(0.6, delta(5_000.0));
        let cruise = table.net_thrust_ratio(0.78, delta(36_000.0));
        assert!(low < -0.005, "net idle thrust at 5,000 ft M 0.6: {low}");
        assert!(
            cruise.abs() < 0.01,
            "net idle thrust at FL360 M 0.78 is near zero: {cruise}"
        );
        // The more the ram drag, the lower: the drag grows with Mach.
        assert!(
            table.net_thrust_ratio(0.8, delta(5_000.0))
                < table.net_thrust_ratio(0.4, delta(5_000.0))
        );
    }

    #[test]
    fn the_table_is_continuous_in_mach_height_and_bypass_ratio() {
        for bpr in [3.0, 5.9, 6.5, 7.0, 8.0, 10.0, 13.0, 15.0] {
            let table = FlightIdleTable::new(bpr);
            for feet in (0..=45).map(|k| f64::from(k) * 1_000.0) {
                let mut last = table.net_thrust_ratio(0.2, delta(feet));
                let mut last_fuel = table.fuel_flow_over_static(0.0, delta(feet));
                for step in 1..=100 {
                    let mach = 0.2 + 0.007 * f64::from(step);
                    let thrust = table.net_thrust_ratio(mach, delta(feet));
                    let fuel = table.fuel_flow_over_static(mach, delta(feet));
                    assert!((thrust - last).abs() < 0.02, "BPR {bpr} {feet} ft M {mach}");
                    assert!(
                        (fuel - last_fuel).abs() < 0.2,
                        "BPR {bpr} {feet} ft M {mach}"
                    );
                    last = thrust;
                    last_fuel = fuel;
                }
            }
        }
        for step in 0..60 {
            let bpr = 3.0 + 0.2 * f64::from(step);
            let a = FlightIdleTable::new(bpr).net_thrust_ratio(0.7, delta(20_000.0));
            let b = FlightIdleTable::new(bpr + 0.2).net_thrust_ratio(0.7, delta(20_000.0));
            assert!((a - b).abs() < 0.02, "BPR {bpr}");
        }
    }

    /// Plausibility of the chart idle thrust against the glide it implies: an
    /// aeroplane of lift-to-drag ratio 15-18 descends at the idle thrust of
    /// two high-bypass engines on a path of 3-4.5 degrees (the 3:1 rule of
    /// thumb and the usual 2,000-3,000 ft/min idle descent of a transport at
    /// 250-300 kt), and the former +7 % of take-off thrust could not.
    #[test]
    fn the_idle_net_thrust_gives_the_descent_gradient_of_a_transport() {
        // A320-class: 60 t, two engines of 117.8 kN static thrust, BPR 5.7.
        let table = FlightIdleTable::new(5.7);
        let total_static_thrust_n = 2.0 * 117_770.0;
        let weight_n = 60_000.0 * 9.806_65;
        for (feet, mach, lift_to_drag) in [(25_000.0, 0.70, 17.0), (15_000.0, 0.62, 16.0)] {
            let idle_n = total_static_thrust_n * table.net_thrust_ratio(mach, delta(feet));
            let drag_n = weight_n / lift_to_drag;
            let gradient = ((drag_n - idle_n) / weight_n).asin().to_degrees();
            assert!(
                (3.0..4.5).contains(&gradient),
                "{feet} ft M {mach}: idle {idle_n:.0} N gives {gradient:.2} degrees"
            );
        }
        // The former deck's +7 % of the take-off thrust available at the
        // condition (about 0.07 x 0.45 FN* at 10,000 ft) cannot.
        let former_n = 0.07 * 0.45 * total_static_thrust_n;
        let drag_n = weight_n / 16.0;
        let gradient = ((drag_n - former_n) / weight_n).asin().to_degrees();
        assert!(gradient < 3.0, "{gradient}");
    }

    /// Physical invariants of the traced charts, independent of the trace:
    /// where the net idle thrust is negative at two adjacent heights (Mach >=
    /// 0.4, beyond the charts' crossing points) it does not fall as the air
    /// gets thinner, because the ram drag falls with density. Where it is
    /// positive no height trend follows (the gross thrust falls with density
    /// too), so none is asserted. Below 25,000 ft it does not rise with Mach number
    /// beyond Mach 0.3; at approach speed (Mach 0.2) it is positive; and the
    /// fuel flow of the BPR >= 6.5 charts falls with height and rises with Mach.
    #[test]
    fn the_traced_charts_obey_the_physical_invariants() {
        for chart in &DESCENT_THRUST_CHARTS {
            for (j, mach) in THRUST_MACH.iter().enumerate() {
                if *mach >= 0.4 {
                    for pair in chart.rows.windows(2) {
                        if pair[0].1[j] >= 0.0 || pair[1].1[j] >= 0.0 {
                            continue;
                        }
                        assert!(
                            pair[1].1[j] >= pair[0].1[j] - 1.0e-9,
                            "BPR {} M {mach}: {} ft {} below {} ft {}",
                            chart.bpr,
                            pair[1].0,
                            pair[1].1[j],
                            pair[0].0,
                            pair[0].1[j]
                        );
                    }
                }
            }
            for (feet, row) in chart.rows {
                assert!(
                    row[0] > 0.0,
                    "BPR {} {feet} ft: net idle thrust at M 0.2",
                    chart.bpr
                );
                if *feet <= 25_000.0 {
                    for pair in row[1..].windows(2) {
                        assert!(
                            pair[1] <= pair[0] + 1.0e-9,
                            "BPR {} {feet} ft not falling with Mach",
                            chart.bpr
                        );
                    }
                }
            }
        }
        for chart in DESCENT_FUEL_CHARTS.iter().filter(|chart| chart.bpr >= 6.5) {
            for pair in chart.rows.windows(2) {
                for (low, high) in pair[0].1.iter().zip(&pair[1].1) {
                    assert!(
                        high < low,
                        "BPR {} fuel does not fall with height",
                        chart.bpr
                    );
                }
            }
            for (feet, row) in chart.rows {
                for pair in row.windows(2) {
                    assert!(
                        pair[1] >= pair[0] - 1.0e-9,
                        "BPR {} {feet} ft fuel falls with Mach",
                        chart.bpr
                    );
                }
            }
        }
    }

    #[test]
    fn idle_fuel_flow_falls_with_height_and_is_one_at_static_sea_level() {
        let table = FlightIdleTable::new(5.9);
        assert!((table.fuel_flow_over_static(0.0, SEA_LEVEL_DELTA) - 1.0).abs() < 1.0e-12);
        let low = table.fuel_flow_over_static(0.5, delta(5_000.0));
        let high = table.fuel_flow_over_static(0.78, delta(36_000.0));
        assert!(high < low && high > 0.2, "{low} {high}");
        // Fig. 9.19(f): 0.0158 lb/hr/FN* at sea level, M 0 (p. 208).
        let chart = FlightIdleTable::new(6.5).chart_fuel_kg_n_s(0.0, SEA_LEVEL_DELTA);
        assert!((chart - 0.0158 * LB_PER_HR_PER_LBF_TO_KG_PER_N_S).abs() < 1.0e-12);
    }
}
