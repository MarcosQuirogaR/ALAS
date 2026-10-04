// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Manufacturer-style load-and-trim (balance) chart.
//!
//! The chart is drawn in *balance-index* space, the convention airline load
//! sheets use: `I = W (x - x_ref) / C + K`, with `W` the aircraft mass (kg),
//! `x` the CG station (m, positive aft of the nose tip), `x_ref` the 25 %MAC
//! station, `C` a scale constant (kg m per index unit) and `K = 50`. A
//! constant-%MAC line is then a straight line through `(K, W = 0)`, so the
//! %MAC grid is a fan of oblique lines. A loading item of mass `dW` at
//! station `x_i` adds `dI = dW (x_i - x_ref) / C` whatever the aircraft
//! weight, so loading is vector addition: that is why load sheets use it.
//!
//! # What the sheet shows
//!
//! - Five numbered loading states and their loading/burn path.
//! - The composed loading orders remain computational data for independent
//!   loading-envelope checks; the sheet does not draw their boarding potato.
//! - Four limit sets, each drawn where its mechanism applies: ground
//!   (dashed, DOW to ramp mass), takeoff (solid, takeoff-mass band), flight
//!   (dotted) and landing (dash-dot, up to the landing mass). The potato is
//!   checked against the ground limits only.
//!
//! The figure takes an explicit [`LoadTrimSheetData`]; [`data`] builds one
//! from a pipeline result.

pub mod data;
mod gate;
mod layers;
mod limits;
mod mass_lines;
mod panel;
mod render;
mod sequences;
mod weights;

pub use gate::{GatePhase, StepGate};
pub use weights::MassRole;

pub use panel::SHEET_TEXT;
pub use render::figure_load_trim_sheet;

use crate::scene::Point2D;

/// Balance-index definition `I = W (x - x_ref) / C + K`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BalanceIndex {
    /// Reference station, m aft of the nose tip (the 25 %MAC point).
    pub x_ref_m: f64,
    /// Scale constant, kg m per index unit.
    pub c_kg_m: f64,
    /// Index offset.
    pub k: f64,
}

impl BalanceIndex {
    /// Nice scale constants the chooser picks from, kg m per index unit.
    const NICE_C: [f64; 13] = [
        100.0, 200.0, 250.0, 500.0, 1_000.0, 2_000.0, 2_500.0, 5_000.0, 10_000.0, 20_000.0,
        25_000.0, 50_000.0, 100_000.0,
    ];

    /// The index of a mass at a CG station.
    pub fn index(&self, mass_kg: f64, x_m: f64) -> f64 {
        mass_kg * (x_m - self.x_ref_m) / self.c_kg_m + self.k
    }

    /// Choose `x_ref` at 25 %MAC, `K = 50` and the smallest nice `C` that
    /// keeps the CG band `[fwd_pct, aft_pct]` within +-40 index units of `K`
    /// at `max_mass_kg`.
    pub fn for_aircraft(
        x_lemac_m: f64,
        mac_m: f64,
        max_mass_kg: f64,
        fwd_pct: f64,
        aft_pct: f64,
    ) -> Self {
        let x_ref_m = x_lemac_m + 0.25 * mac_m;
        let arm_m =
            ((aft_pct - 25.0).abs().max((25.0 - fwd_pct).abs()) / 100.0 * mac_m).max(0.05 * mac_m);
        let raw = max_mass_kg * arm_m / 40.0;
        let c_kg_m = Self::NICE_C
            .iter()
            .copied()
            .find(|&c| c >= raw)
            .unwrap_or(raw);
        Self {
            x_ref_m,
            c_kg_m,
            k: 50.0,
        }
    }
}

/// Operational CG limits at one mass, %MAC.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimitVertex {
    /// Aircraft mass, kg.
    pub mass_kg: f64,
    /// Forward limit, %MAC.
    pub fwd_pct_mac: f64,
    /// Aft limit, %MAC.
    pub aft_pct_mac: f64,
}

/// The CG spread of the composed loading orders at one mass level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PotatoLevel {
    /// Aircraft mass, kg.
    pub mass_kg: f64,
    /// Most forward CG any composed order reaches at this mass, %MAC.
    pub fwd_pct_mac: f64,
    /// Most aft CG any composed order reaches at this mass, %MAC.
    pub aft_pct_mac: f64,
}

/// One named loading order: (mass kg, %MAC) after each loaded item.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedPath {
    /// Order name, e.g. `holds forward-first | zones front-to-back, ...`.
    pub name: String,
    /// Cumulative (mass kg, CG %MAC), ascending mass.
    pub points: Vec<(f64, f64)>,
}

/// A labeled analyzed or design mass line.
#[derive(Debug, Clone, PartialEq)]
pub struct WeightLine {
    /// Provenance of this mass.
    pub role: MassRole,
    /// Mass, kg.
    pub mass_kg: f64,
}

/// One numbered step of the worked loading case.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadStep {
    /// Item added (or removed) to reach this state, e.g. `Cargo`.
    pub item: String,
    /// Short name of the state reached, e.g. `ZFW`.
    pub state: String,
    /// Aircraft mass after the step, kg.
    pub mass_kg: f64,
    /// CG after the step, %MAC.
    pub pct_mac: f64,
    /// The run's CG-gate verdict for this state; `None` for a loading step
    /// the gate does not evaluate.
    pub gate: Option<StepGate>,
}

/// Everything the balance chart draws. Masses in kg, CG in %MAC of
/// `mac_m` measured from `x_lemac_m`.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadTrimSheetData {
    /// Sheet title.
    pub title: String,
    /// Leading edge of MAC, m aft of the nose tip.
    pub x_lemac_m: f64,
    /// MAC length, m.
    pub mac_m: f64,
    /// Balance-index definition.
    pub index: BalanceIndex,
    /// Ground limits (maximum nose load forward; minimum nose load and
    /// tip-back aft), ascending mass, from DOW to the ramp mass.
    pub ground_limits: Vec<LimitVertex>,
    /// Takeoff limits (rotation forward, static-margin floor aft), ascending
    /// mass, over the takeoff-mass band.
    pub takeoff_limits: Vec<LimitVertex>,
    /// En-route flight limits (landing trim forward, static-margin floor
    /// aft), ascending mass.
    pub flight_limits: Vec<LimitVertex>,
    /// Landing limits (landing trim forward, ground mechanisms aft),
    /// ascending mass, up to the landing mass.
    pub landing_limits: Vec<LimitVertex>,
    /// Zero-fuel operational limits, ascending mass (empty if unknown).
    pub zfw_limits: Vec<LimitVertex>,
    /// Analyzed and design mass lines.
    pub weight_lines: Vec<WeightLine>,
    /// The boarding potato, ascending mass, from DOW to takeoff mass.
    pub potato: Vec<PotatoLevel>,
    /// Every composed loading order, retained for envelope verification.
    pub sequences: Vec<NamedPath>,
    /// Fuel curve (mass kg, %MAC), ascending mass, from ZFW to TOW; burn
    /// retraces it downwards.
    pub fuel_curve: Vec<(f64, f64)>,
    /// Worked loading case, first step is the empty (DOW) state.
    pub steps: Vec<LoadStep>,
}

impl LoadTrimSheetData {
    fn x_at(&self, pct: f64) -> f64 {
        self.x_lemac_m + pct / 100.0 * self.mac_m
    }

    /// Index of a (mass, %MAC) pair.
    pub fn index_at(&self, mass_kg: f64, pct: f64) -> f64 {
        self.index.index(mass_kg, self.x_at(pct))
    }

    /// %MAC of an (index, mass) chart point.
    pub fn pct_at(&self, index: f64, mass_kg: f64) -> f64 {
        let x_m = (index - self.index.k) * self.index.c_kg_m / mass_kg + self.index.x_ref_m;
        (x_m - self.x_lemac_m) / self.mac_m * 100.0
    }

    /// The largest amount, %MAC, by which the potato leaves the ground
    /// limits at any level; zero when it lies inside them.
    pub fn potato_ground_exceedance_pct_mac(&self) -> f64 {
        self.potato
            .iter()
            .map(|level| {
                let fwd = limit_at(&self.ground_limits, level.mass_kg, true) - level.fwd_pct_mac;
                let aft = level.aft_pct_mac - limit_at(&self.ground_limits, level.mass_kg, false);
                fwd.max(aft)
            })
            .filter(|v| v.is_finite())
            .fold(0.0, f64::max)
    }

    /// Analyzed or design mass by its semantic role.
    pub fn weight(&self, role: MassRole) -> Option<f64> {
        self.weight_lines
            .iter()
            .find(|line| line.role == role)
            .map(|line| line.mass_kg)
    }
}

/// The loading envelope of *every* loading order of a set of items, as a
/// closed (mass, %MAC) polygon. Items are (mass kg, station m) increments
/// added to a start state. In index space each item is a vector of slope
/// `(x_i - x_ref)/C` per kg; adding them in ascending station order traces
/// the forward (lowest-index) chain and in descending order the aft chain,
/// and every other order lies between them, so the two chains bound the
/// reachable set exactly and each is convex.
pub fn loading_envelope_polygon(
    start_mass_kg: f64,
    start_x_m: f64,
    items: &[(f64, f64)],
    x_lemac_m: f64,
    mac_m: f64,
) -> Vec<(f64, f64)> {
    let mut sorted: Vec<(f64, f64)> = items
        .iter()
        .copied()
        .filter(|(m, x)| *m > 0.0 && x.is_finite())
        .collect();
    sorted.sort_by(|a, b| a.1.total_cmp(&b.1));
    let chain = |order: &mut dyn Iterator<Item = &(f64, f64)>| {
        let (mut mass, mut moment) = (start_mass_kg, start_mass_kg * start_x_m);
        let mut out = vec![(mass, (start_x_m - x_lemac_m) / mac_m * 100.0)];
        for &(m, x) in order {
            mass += m;
            moment += m * x;
            out.push((mass, (moment / mass - x_lemac_m) / mac_m * 100.0));
        }
        out
    };
    let mut polygon = chain(&mut sorted.iter());
    let mut aft = chain(&mut sorted.iter().rev());
    aft.reverse();
    polygon.extend(aft.into_iter().skip(1));
    polygon
}

/// Piecewise-linear interpolation of a limit side over mass, held constant
/// beyond the end vertices.
pub(crate) fn limit_at(limits: &[LimitVertex], mass_kg: f64, fwd: bool) -> f64 {
    let pick = |v: &LimitVertex| if fwd { v.fwd_pct_mac } else { v.aft_pct_mac };
    let (Some(first), Some(last)) = (limits.first(), limits.last()) else {
        return f64::NAN;
    };
    if mass_kg <= first.mass_kg {
        return pick(first);
    }
    if mass_kg >= last.mass_kg {
        return pick(last);
    }
    for pair in limits.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if mass_kg <= b.mass_kg {
            let t = if b.mass_kg > a.mass_kg {
                (mass_kg - a.mass_kg) / (b.mass_kg - a.mass_kg)
            } else {
                0.0
            };
            return pick(a) + t * (pick(b) - pick(a));
        }
    }
    pick(last)
}

/// Plot geometry: pixel rectangle and data ranges (index, kg).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frame {
    pub(crate) left: f64,
    pub(crate) top: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) i_range: (f64, f64),
    pub(crate) w_range_kg: (f64, f64),
}

impl Frame {
    pub(crate) fn map(&self, index: f64, mass_kg: f64) -> Point2D {
        let fx = (index - self.i_range.0) / (self.i_range.1 - self.i_range.0);
        let fy = (mass_kg - self.w_range_kg.0) / (self.w_range_kg.1 - self.w_range_kg.0);
        [
            self.left + fx * self.width,
            self.top + (1.0 - fy) * self.height,
        ]
    }

    /// Inverse of [`Self::map`]: (index, kg) of a pixel.
    pub(crate) fn unmap(&self, p: Point2D) -> (f64, f64) {
        let fx = (p[0] - self.left) / self.width;
        let fy = 1.0 - (p[1] - self.top) / self.height;
        (
            self.i_range.0 + fx * (self.i_range.1 - self.i_range.0),
            self.w_range_kg.0 + fy * (self.w_range_kg.1 - self.w_range_kg.0),
        )
    }

    pub(crate) fn right(&self) -> f64 {
        self.left + self.width
    }

    pub(crate) fn bottom(&self) -> f64 {
        self.top + self.height
    }
}

/// Canvas size and chart rectangle; the side panel lives right of the chart.
pub(crate) const SHEET_W: f64 = 1420.0;
pub(crate) const SHEET_H: f64 = 750.0;
pub(crate) const CHART: (f64, f64, f64, f64) = (100.0, 70.0, 860.0, 620.0);

/// Round `value` down/up to a multiple of `step`.
fn snap(value: f64, step: f64, up: bool) -> f64 {
    if up {
        (value / step).ceil() * step
    } else {
        (value / step).floor() * step
    }
}

/// A weight-axis step that gives 6-12 gridlines.
pub(crate) fn weight_step_kg(span_kg: f64) -> f64 {
    [
        1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0, 50_000.0, 100_000.0,
    ]
    .into_iter()
    .find(|s| span_kg / s <= 12.0)
    .unwrap_or(200_000.0)
}

/// Choose the frame so displayed limits, weight lines, fuel curve and
/// steps fit.
pub(crate) fn frame_for(data: &LoadTrimSheetData) -> Frame {
    let limits = [
        &data.ground_limits,
        &data.takeoff_limits,
        &data.flight_limits,
        &data.landing_limits,
        &data.zfw_limits,
    ];
    let mut masses: Vec<f64> = limits
        .iter()
        .flat_map(|set| set.iter().map(|v| v.mass_kg))
        .collect();
    masses.extend(data.weight_lines.iter().map(|l| l.mass_kg));
    masses.extend(data.steps.iter().map(|s| s.mass_kg));
    masses.extend(data.fuel_curve.iter().map(|point| point.0));
    let lo = masses.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = masses.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let step = weight_step_kg(hi - lo * 0.9);
    let w_range_kg = (snap(lo * 0.94, step, false), snap(hi * 1.02, step, true));

    let mut indices = Vec::new();
    for w in [w_range_kg.0, w_range_kg.1] {
        for fwd in [true, false] {
            indices.push(data.index_at(w, limit_at(&data.ground_limits, w, fwd)));
        }
    }
    indices.extend(
        data.steps
            .iter()
            .map(|s| data.index_at(s.mass_kg, s.pct_mac)),
    );
    indices.extend(
        data.fuel_curve
            .iter()
            .map(|&(mass, pct)| data.index_at(mass, pct)),
    );
    for set in limits {
        for pair in set.windows(2) {
            for (start_pct, end_pct) in [
                (pair[0].fwd_pct_mac, pair[1].fwd_pct_mac),
                (pair[0].aft_pct_mac, pair[1].aft_pct_mac),
            ] {
                let slope = (end_pct - start_pct) / (pair[1].mass_kg - pair[0].mass_kg);
                if !slope.is_finite() || slope == 0.0 {
                    continue;
                }
                let arm_pct = 100.0 * (data.x_at(start_pct) - data.index.x_ref_m) / data.mac_m;
                // Linear percent-MAC limits become quadratic in balance index.
                let mass = 0.5 * (pair[0].mass_kg - arm_pct / slope);
                if mass > pair[0].mass_kg && mass < pair[1].mass_kg {
                    indices.push(data.index_at(mass, start_pct + slope * (mass - pair[0].mass_kg)));
                }
            }
        }
        for vertex in set {
            indices.push(data.index_at(vertex.mass_kg, vertex.fwd_pct_mac));
            indices.push(data.index_at(vertex.mass_kg, vertex.aft_pct_mac));
        }
    }
    let finite = indices.iter().copied().filter(|v| v.is_finite());
    let i_lo = finite.clone().fold(f64::INFINITY, f64::min);
    let i_hi = finite.fold(f64::NEG_INFINITY, f64::max);
    let (left, top, width, height) = CHART;
    Frame {
        left,
        top,
        width,
        height,
        i_range: (snap(i_lo - 6.0, 10.0, false), snap(i_hi + 6.0, 10.0, true)),
        w_range_kg,
    }
}

/// Clip the segment of constant `pct` between the bottom and top weights to
/// the index range; `None` when it misses the frame.
pub(crate) fn fan_segment(
    data: &LoadTrimSheetData,
    frame: &Frame,
    pct: f64,
) -> Option<((f64, f64), (f64, f64))> {
    let (w0, w1) = frame.w_range_kg;
    let (i0, i1) = (data.index_at(w0, pct), data.index_at(w1, pct));
    let (lo, hi) = frame.i_range;
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    let di = i1 - i0;
    if di.abs() < 1e-12 {
        if i0 < lo || i0 > hi {
            return None;
        }
    } else {
        let ta = (lo - i0) / di;
        let tb = (hi - i0) / di;
        t0 = t0.max(ta.min(tb));
        t1 = t1.min(ta.max(tb));
        if t0 >= t1 {
            return None;
        }
    }
    let at = |t: f64| (i0 + t * di, w0 + t * (w1 - w0));
    Some((at(t0), at(t1)))
}

/// Envelope outline in (index, kg): forward side ascending, then aft side
/// descending, between `bottom_kg` and `top_kg`.
pub(crate) fn envelope_outline(
    data: &LoadTrimSheetData,
    limits: &[LimitVertex],
    bottom_kg: f64,
    top_kg: f64,
) -> Vec<(f64, f64)> {
    let n = 80;
    let masses: Vec<f64> = (0..=n)
        .map(|k| bottom_kg + (top_kg - bottom_kg) * f64::from(k) / f64::from(n))
        .collect();
    let mut outline: Vec<(f64, f64)> = masses
        .iter()
        .map(|&m| (data.index_at(m, limit_at(limits, m, true)), m))
        .collect();
    outline.extend(
        masses
            .iter()
            .rev()
            .map(|&m| (data.index_at(m, limit_at(limits, m, false)), m)),
    );
    outline
}

#[cfg(test)]
mod tests;
