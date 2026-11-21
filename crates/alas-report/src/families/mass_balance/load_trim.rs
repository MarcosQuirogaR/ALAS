// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Manufacturer-style load-and-trim (weight-and-balance) chart.
//!
//! The chart is drawn in physical coordinates, CG in %MAC across and gross
//! weight up, as manufacturers' weight-and-balance envelopes are. The
//! balance index `I = W (x - x_ref) / C + K` of airline load sheets (`W` the
//! aircraft mass in kg, `x` the CG station in m aft of the nose tip, `x_ref`
//! the 25 %MAC station, `C` a scale constant in kg m per index unit, `K = 50`)
//! stays in the data for load-sheet use; the figure does not draw it.
//!
//! # What the sheet shows
//!
//! - Three envelopes as light filled polygons with thin edges, as a
//!   weight-and-balance manual draws them: takeoff up to MTOW, landing up to
//!   MLW and zero fuel up to MZFW, each from the dry operating weight. Their
//!   limits are the model's own mechanisms the gate applies in that phase; a
//!   band where an envelope's forward limit lies aft of its aft limit is
//!   shaded red. The en-route states above the MLW (no landing trim) are an
//!   outline of their own, keyed by the forward limit the gate found there.
//! - The loading potato: the cargo curves (forward or aft hold first) and the
//!   four seat orders (window or aisle seats first, front to back or back to
//!   front), and the fuel curve from ZFW to TOW along the tank sequence.
//! - The states DOW, ZFW, TOW and LW, labelled at their points, each ringed
//!   when the run's CG gate failed it.
//! - A sourced certified envelope, thin and dashed, where one exists.
//!
//! The figure takes an explicit [`LoadTrimSheetData`]; [`data`] builds one
//! from a pipeline result or from an analysis report.

pub mod data;
mod gate;
mod geom;
mod limits;
mod mechanisms;
mod numbers;
mod panel;
mod plot_data;
mod published;
mod render;
mod sequences;
mod weights;

pub use gate::{GatePhase, StepGate};
pub use limits::AboveMlwLimits;
pub use mechanisms::{CgLines, Mechanism, MechanismLine, PublishedLine};
pub use weights::MassRole;

pub use panel::SHEET_TEXT;
pub use render::{figure_load_trim_sheet, figure_load_trim_sheet_sized};

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
    /// En-route flight limits up to the MLW (landing trim forward,
    /// static-margin floor aft), ascending mass.
    pub flight_limits: Vec<LimitVertex>,
    /// Landing limits (landing trim forward, ground mechanisms aft),
    /// ascending mass, up to the landing mass.
    pub landing_limits: Vec<LimitVertex>,
    /// En-route limits above the MLW and their forward bound; `None` when the
    /// gate scoped no state above the MLW.
    pub en_route_above_mlw: Option<AboveMlwLimits>,
    /// Zero-fuel operational limits, ascending mass (empty if unknown).
    pub zfw_limits: Vec<LimitVertex>,
    /// The per-mechanism limit lines the chart draws and the published
    /// comparison envelope. The four sets above stay as the exported and
    /// frame-sizing data; when `lines` holds mechanisms, the operational
    /// envelope is theirs.
    pub lines: CgLines,
    /// Analyzed and design mass lines.
    pub weight_lines: Vec<WeightLine>,
    /// The boarding potato, ascending mass, from DOW to takeoff mass.
    pub potato: Vec<PotatoLevel>,
    /// Every composed loading order, retained for envelope verification.
    pub sequences: Vec<NamedPath>,
    /// The loading curves behind the potato: cargo, then seats, each from
    /// the state it starts at (the dry operating weight, or the weight with
    /// the holds loaded).
    pub loading_curves: Vec<LoadingCurve>,
    /// Fuel curve (mass kg, %MAC), ascending mass, from ZFW to TOW; burn
    /// retraces it downwards.
    pub fuel_curve: Vec<(f64, f64)>,
    /// Worked loading case, first step is the empty (DOW) state.
    pub steps: Vec<LoadStep>,
}

impl LoadTrimSheetData {
    /// The operational envelope: at each mass the most restrictive of the
    /// ground, takeoff, flight and landing limit sets, each held at its end
    /// values beyond its own mass span, as a load sheet's allowed zone is.
    /// Ascending mass.
    pub fn operational_limits(&self) -> Vec<LimitVertex> {
        if !self.lines.mechanisms.is_empty() {
            let masses = self
                .lines
                .mechanisms
                .iter()
                .flat_map(|l| l.points.iter().map(|p| p.0))
                .chain(self.lines.band_edges());
            let lo = masses.clone().fold(f64::INFINITY, f64::min);
            let hi = masses.fold(f64::NEG_INFINITY, f64::max);
            return self.lines.operational(lo, hi);
        }
        let sets: Vec<&[LimitVertex]> = [
            &self.ground_limits,
            &self.takeoff_limits,
            &self.flight_limits,
            &self.landing_limits,
        ]
        .into_iter()
        .map(Vec::as_slice)
        .filter(|set| !set.is_empty())
        .collect();
        let mut masses: Vec<f64> = Vec::new();
        for set in &sets {
            for v in *set {
                masses.push(v.mass_kg);
            }
            if let Some(first) = set.first() {
                masses.push(first.mass_kg - 1.0);
            }
        }
        masses.sort_by(f64::total_cmp);
        masses.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        masses
            .into_iter()
            .map(|m| {
                let mut fwd = f64::NEG_INFINITY;
                let mut aft = f64::INFINITY;
                for set in &sets {
                    // Every set holds its end values beyond its own span: the
                    // forward and aft limits of a transport do not vanish below
                    // the zero-fuel weight, the sheet states one envelope.
                    fwd = fwd.max(limit_at(set, m, true));
                    aft = aft.min(limit_at(set, m, false));
                }
                LimitVertex {
                    mass_kg: m,
                    fwd_pct_mac: fwd,
                    aft_pct_mac: aft,
                }
            })
            .filter(|v| v.fwd_pct_mac.is_finite() && v.aft_pct_mac.is_finite())
            .collect()
    }

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

/// Plot geometry: pixel rectangle and data ranges (%MAC, kg). The chart is
/// drawn in physical coordinates, CG in %MAC across and gross weight up, as
/// manufacturers' weight-and-balance envelopes are.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frame {
    pub(crate) left: f64,
    pub(crate) top: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    /// CG range across the frame, %MAC.
    pub(crate) x_range_pct: (f64, f64),
    pub(crate) w_range_kg: (f64, f64),
}

impl Frame {
    /// Pixel of a (%MAC, kg) point.
    pub(crate) fn map(&self, pct: f64, mass_kg: f64) -> Point2D {
        let fx = (pct - self.x_range_pct.0) / (self.x_range_pct.1 - self.x_range_pct.0);
        let fy = (mass_kg - self.w_range_kg.0) / (self.w_range_kg.1 - self.w_range_kg.0);
        [
            self.left + fx * self.width,
            self.top + (1.0 - fy) * self.height,
        ]
    }

    /// Inverse of [`Self::map`]: (%MAC, kg) of a pixel.
    #[cfg(test)]
    pub(crate) fn unmap(&self, p: Point2D) -> (f64, f64) {
        let fx = (p[0] - self.left) / self.width;
        let fy = 1.0 - (p[1] - self.top) / self.height;
        (
            self.x_range_pct.0 + fx * (self.x_range_pct.1 - self.x_range_pct.0),
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

/// Which of the three manufacturer-style envelopes of the sheet.
///
/// A weight-and-balance manual charts the CG range for the takeoff weight
/// band (up to MTOW), for landing (up to MLW) and for the zero-fuel weight
/// (up to MZFW). Each is built from the model's own limit mechanisms the
/// gate applies in that phase (see [`mechanisms`]): the ground mechanisms
/// everywhere, the rotation limit at takeoff, the landing-trim limit at zero
/// fuel and landing, the static-margin floor everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeKind {
    /// Takeoff, from the minimum weight up to the maximum takeoff weight.
    Takeoff,
    /// Landing, up to the maximum landing weight.
    Landing,
    /// Zero fuel, up to the maximum zero-fuel weight.
    ZeroFuel,
}

impl EnvelopeKind {
    /// Drawing order: the widest weight band first, so the nested ones read
    /// through it.
    pub const ALL: [Self; 3] = [Self::Takeoff, Self::Landing, Self::ZeroFuel];

    /// The key text of the envelope. Static strings so the desktop catalog
    /// can translate them.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Takeoff => "Takeoff envelope",
            Self::Landing => "Landing envelope",
            Self::ZeroFuel => "Zero-fuel envelope",
        }
    }

    /// The label of the weight the envelope runs up to, and (`capped`) of the
    /// heaviest loadable mass when that lies below the declared weight.
    #[must_use]
    pub const fn weight_label(self, capped: bool) -> &'static str {
        match (self, capped) {
            (Self::Takeoff, false) => "MTOW",
            (Self::Landing, false) => "MLW",
            (Self::ZeroFuel, false) => "MZFW",
            (Self::Takeoff, true) => "Max TOW",
            (Self::Landing, true) => "Max LW",
            (Self::ZeroFuel, true) => "Max ZFW",
        }
    }

    /// Whether the gate applies `mechanism` in this envelope's phase.
    #[must_use]
    pub const fn admits(self, mechanism: Mechanism) -> bool {
        match mechanism {
            Mechanism::NoseGearLoad
            | Mechanism::MinNoseLoad
            | Mechanism::TipBack
            | Mechanism::StaticMargin
            // Every envelope (take-off, landing, zero fuel) is flown clean en route.
            | Mechanism::CleanTrim => true,
            Mechanism::Rotation => matches!(self, Self::Takeoff),
            Mechanism::LandingTrim => !matches!(self, Self::Takeoff),
        }
    }
}

/// One envelope as a polygon over mass: the limits at ascending masses from
/// the minimum weight to the envelope's top weight.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    /// Which envelope.
    pub kind: EnvelopeKind,
    /// Lightest mass (the dry operating weight), kg.
    pub bottom_kg: f64,
    /// Heaviest mass the envelope runs up to, kg.
    pub top_kg: f64,
    /// Whether `top_kg` is the declared weight (MTOW, MLW, MZFW) and not the
    /// heaviest mass the model could load below it.
    pub declared: bool,
    /// The label of the top weight: MTOW, MLW or MZFW; "Max ..." for a
    /// capped top; "ZFW (analysed)" when the zero-fuel top is the analysed
    /// zero-fuel mass because no maximum zero-fuel weight is registered.
    pub label: &'static str,
    /// Forward and aft limit at ascending mass, `bottom_kg` to `top_kg`.
    pub vertices: Vec<LimitVertex>,
}

impl Envelope {
    /// Closed outline in (%MAC, kg): forward edge ascending, then the aft
    /// edge descending.
    #[must_use]
    pub fn outline(&self) -> Vec<(f64, f64)> {
        let mut outline: Vec<(f64, f64)> = self
            .vertices
            .iter()
            .map(|v| (v.fwd_pct_mac, v.mass_kg))
            .collect();
        outline.extend(
            self.vertices
                .iter()
                .rev()
                .map(|v| (v.aft_pct_mac, v.mass_kg)),
        );
        outline
    }

    /// The masses over which the forward limit lies aft of the aft limit
    /// (no feasible CG range), as (start, end) pairs; empty for a sound
    /// envelope.
    #[must_use]
    pub fn crossed_bands(&self) -> Vec<(f64, f64)> {
        let gap = |v: &LimitVertex| v.fwd_pct_mac - v.aft_pct_mac;
        let mut bands = Vec::new();
        let mut start: Option<f64> = self
            .vertices
            .first()
            .filter(|v| gap(v) > 0.0)
            .map(|v| v.mass_kg);
        for pair in self.vertices.windows(2) {
            let (ga, gb) = (gap(&pair[0]), gap(&pair[1]));
            let cross = || pair[0].mass_kg + ga / (ga - gb) * (pair[1].mass_kg - pair[0].mass_kg);
            if ga <= 0.0 && gb > 0.0 {
                start = Some(cross());
            } else if ga > 0.0 && gb <= 0.0 {
                if let Some(s) = start.take() {
                    bands.push((s, cross()));
                }
            }
        }
        if let (Some(s), Some(last)) = (start, self.vertices.last()) {
            bands.push((s, last.mass_kg));
        }
        bands.retain(|(s, e)| e > s);
        bands
    }
}

/// One drawn loading curve of the boarding potato.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveKind {
    /// The lower-hold cargo, loaded forward hold first or aft hold first.
    Cargo,
    /// The passengers, seated window seats first or aisle seats first (the
    /// middle seats in between), front to back or back to front.
    Seats {
        /// Rows boarded from the front of the cabin to the back.
        front_to_back: bool,
        /// Window seats first (aisle seats first otherwise).
        window_first: bool,
    },
}

/// One loading curve: (mass kg, %MAC) after each loaded item.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadingCurve {
    /// What the curve loads.
    pub kind: CurveKind,
    /// Cumulative (mass kg, %MAC), ascending mass.
    pub points: Vec<(f64, f64)>,
}

impl LoadTrimSheetData {
    /// The lightest state of the worked case (the dry operating weight), the
    /// chart's minimum weight.
    pub(crate) fn minimum_weight_kg(&self) -> Option<f64> {
        self.steps.first().map(|s| s.mass_kg)
    }

    /// The forward (`forward`) or aft limit, %MAC, the gate applies to `kind`
    /// at `mass_kg`: the most restrictive of the mechanisms in that phase.
    #[must_use]
    pub fn envelope_limit_at(&self, kind: EnvelopeKind, mass_kg: f64, forward: bool) -> f64 {
        let finite = |v: &f64| v.is_finite();
        if self.lines.mechanisms.is_empty() {
            // Hand-built data: the ground set and the phase's own set.
            let own = match kind {
                EnvelopeKind::Takeoff => &self.takeoff_limits,
                EnvelopeKind::Landing => &self.landing_limits,
                EnvelopeKind::ZeroFuel => &self.flight_limits,
            };
            let values = [
                limit_at(&self.ground_limits, mass_kg, forward),
                limit_at(own, mass_kg, forward),
            ]
            .into_iter()
            .filter(finite);
            return if forward {
                values.fold(f64::NEG_INFINITY, f64::max)
            } else {
                values.fold(f64::INFINITY, f64::min)
            };
        }
        let values = self
            .lines
            .mechanisms
            .iter()
            .filter(|l| l.mechanism.is_forward() == forward && kind.admits(l.mechanism))
            .map(|l| l.pct_at(mass_kg))
            .filter(finite);
        if forward {
            values.fold(f64::NEG_INFINITY, f64::max)
        } else {
            values.fold(f64::INFINITY, f64::min)
        }
    }

    /// The mechanism that sets the forward or aft limit of `kind` at
    /// `mass_kg`; `None` for hand-built data without mechanism lines.
    #[must_use]
    pub fn governing_mechanism(
        &self,
        kind: EnvelopeKind,
        mass_kg: f64,
        forward: bool,
    ) -> Option<Mechanism> {
        let limit = self.envelope_limit_at(kind, mass_kg, forward);
        self.lines
            .mechanisms
            .iter()
            .filter(|l| l.mechanism.is_forward() == forward && kind.admits(l.mechanism))
            .find(|l| (l.pct_at(mass_kg) - limit).abs() < 1e-6)
            .map(|l| l.mechanism)
    }

    /// The declared top weight of envelope `kind`, kg: the registered or
    /// closure mass of its phase.
    fn declared_top_kg(&self, kind: EnvelopeKind) -> Option<f64> {
        match kind {
            EnvelopeKind::Takeoff => self
                .weight(MassRole::RegisteredMtow)
                .or_else(|| self.weight(MassRole::SizedTakeoff))
                .or_else(|| self.weight(MassRole::AnalyzedTakeoff)),
            EnvelopeKind::ZeroFuel => self
                .weight(MassRole::MaximumZeroFuel)
                .or_else(|| self.weight(MassRole::AnalyzedZeroFuel)),
            EnvelopeKind::Landing => self.weight(MassRole::DesignLanding),
        }
    }

    /// The heaviest mass the model's CG assessment evaluated a state at, kg;
    /// `None` for hand-built data without mechanism lines.
    fn heaviest_evaluated_kg(&self) -> Option<f64> {
        self.lines
            .mechanisms
            .iter()
            .flat_map(|l| l.points.iter().map(|p| p.0))
            .filter(|m| m.is_finite())
            .reduce(f64::max)
    }

    /// The top weight of envelope `kind`, kg: its declared weight, but never
    /// above the heaviest mass the model evaluated, since a limit is not
    /// extrapolated to a loading the aircraft cannot reach (a closure takeoff
    /// mass beyond what its tanks and payload can carry).
    #[must_use]
    pub fn envelope_top_kg(&self, kind: EnvelopeKind) -> Option<f64> {
        let declared = self.declared_top_kg(kind)?;
        Some(
            self.heaviest_evaluated_kg()
                .map_or(declared, |evaluated| declared.min(evaluated)),
        )
    }

    /// Whether the top of envelope `kind` is the declared weight, not the
    /// heaviest evaluated mass below it.
    #[must_use]
    pub fn envelope_top_is_declared(&self, kind: EnvelopeKind) -> bool {
        match (self.declared_top_kg(kind), self.envelope_top_kg(kind)) {
            (Some(declared), Some(top)) => top >= declared - 0.005 * declared,
            _ => false,
        }
    }

    /// The takeoff, landing and zero-fuel envelopes, each from the minimum
    /// weight to its top weight; an envelope without a top weight is absent.
    #[must_use]
    pub fn envelopes(&self) -> Vec<Envelope> {
        let Some(bottom) = self.minimum_weight_kg() else {
            return Vec::new();
        };
        EnvelopeKind::ALL
            .into_iter()
            .filter_map(|kind| {
                let top = self
                    .envelope_top_kg(kind)
                    .filter(|top| *top > bottom + 1.0)?;
                let mut masses = vec![bottom, top];
                masses.extend(
                    self.lines
                        .mechanisms
                        .iter()
                        .flat_map(|l| l.points.iter().map(|p| p.0))
                        .chain(self.ground_limits.iter().map(|v| v.mass_kg))
                        .chain(self.takeoff_limits.iter().map(|v| v.mass_kg))
                        .chain(self.flight_limits.iter().map(|v| v.mass_kg))
                        .chain(self.landing_limits.iter().map(|v| v.mass_kg))
                        .filter(|m| *m > bottom + 0.5 && *m < top - 0.5),
                );
                masses.sort_by(f64::total_cmp);
                masses.dedup_by(|a, b| (*a - *b).abs() < 0.5);
                let vertices: Vec<LimitVertex> = masses
                    .into_iter()
                    .map(|m| LimitVertex {
                        mass_kg: m,
                        fwd_pct_mac: self.envelope_limit_at(kind, m, true),
                        aft_pct_mac: self.envelope_limit_at(kind, m, false),
                    })
                    .filter(|v| v.fwd_pct_mac.is_finite() && v.aft_pct_mac.is_finite())
                    .collect();
                (vertices.len() >= 2).then(|| Envelope {
                    kind,
                    bottom_kg: bottom,
                    top_kg: top,
                    declared: self.envelope_top_is_declared(kind),
                    label: if kind == EnvelopeKind::ZeroFuel
                        && self.weight(MassRole::MaximumZeroFuel).is_none()
                    {
                        "ZFW (analysed)"
                    } else {
                        kind.weight_label(!self.envelope_top_is_declared(kind))
                    },
                    vertices: simplified(vertices),
                })
            })
            .collect()
    }
}

/// `vertices` without the interior ones both limits pass straight through.
fn simplified(vertices: Vec<LimitVertex>) -> Vec<LimitVertex> {
    const TOLERANCE_PCT_MAC: f64 = 0.02;
    let mut kept: Vec<LimitVertex> = Vec::with_capacity(vertices.len());
    for k in 0..vertices.len() {
        let interior = k > 0 && k + 1 < vertices.len();
        if interior {
            let (a, b, c) = (kept[kept.len() - 1], vertices[k], vertices[k + 1]);
            let t = (b.mass_kg - a.mass_kg) / (c.mass_kg - a.mass_kg);
            let on_line =
                |lo: f64, mid: f64, hi: f64| (lo + t * (hi - lo) - mid).abs() < TOLERANCE_PCT_MAC;
            if on_line(a.fwd_pct_mac, b.fwd_pct_mac, c.fwd_pct_mac)
                && on_line(a.aft_pct_mac, b.aft_pct_mac, c.aft_pct_mac)
            {
                continue;
            }
        }
        kept.push(vertices[k]);
    }
    kept
}

/// Choose the frame so the envelopes, loading curves, fuel curve, states and
/// published lines fit.
#[cfg(test)]
pub(crate) fn frame_for(data: &LoadTrimSheetData) -> Frame {
    frame_in(data, (0.0, 0.0, 800.0, 600.0))
}

/// [`frame_for`] with the chart rectangle `chart` = `(left, top, width, height)`.
pub(crate) fn frame_in(data: &LoadTrimSheetData, chart: (f64, f64, f64, f64)) -> Frame {
    let envelopes = data.envelopes();
    let mut masses: Vec<f64> = envelopes
        .iter()
        .flat_map(|e| [e.bottom_kg, e.top_kg])
        .collect();
    masses.extend(data.steps.iter().map(|s| s.mass_kg));
    masses.extend(data.fuel_curve.iter().map(|point| point.0));
    masses.extend(data.weight(MassRole::RegisteredMtow));
    let lo = masses.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = masses.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let step = weight_step_kg(hi - lo * 0.9);
    let w_range_kg = (snap(lo * 0.94, step, false), snap(hi * 1.035, step, true));

    let mut pcts: Vec<f64> = Vec::new();
    for e in &envelopes {
        for v in &e.vertices {
            pcts.extend([v.fwd_pct_mac, v.aft_pct_mac]);
        }
    }
    pcts.extend(data.steps.iter().map(|s| s.pct_mac));
    pcts.extend(data.fuel_curve.iter().map(|p| p.1));
    let above_mlw = data.en_route_above_mlw.iter().flat_map(|s| &s.vertices);
    pcts.extend(above_mlw.flat_map(|v| [v.fwd_pct_mac, v.aft_pct_mac]));
    pcts.extend(
        data.lines
            .published
            .iter()
            .flat_map(|l| l.points.iter().map(|p| p.1)),
    );
    let finite = pcts.iter().copied().filter(|v| v.is_finite());
    let p_lo = finite.clone().fold(f64::INFINITY, f64::min);
    let p_hi = finite.fold(f64::NEG_INFINITY, f64::max);
    let (left, top, width, height) = chart;
    Frame {
        left,
        top,
        width,
        height,
        x_range_pct: (snap(p_lo - 2.0, 5.0, false), snap(p_hi + 2.0, 5.0, true)),
        w_range_kg,
    }
}

#[cfg(test)]
mod tests;
