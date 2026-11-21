// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Straight planform edges between declared breaks.
//!
//! A real wing, tailplane or fin is drawn with straight leading and trailing edges.
//! The sweep changes only at genuine planform breaks: the Yehudi kink, a leading-edge
//! crank, the start of a raked tip, the junction with a winglet, the fillet at the side
//! of body, and the rounded corners of a tip. Everything between two breaks is one
//! straight line. A drawing scan that picks up a nacelle, a pylon or a flap-track
//! fairing leaves small sweep changes the aircraft does not have, and this module is
//! the guarantee that no such change enters a preset.
//!
//! [`DECLARED_BREAKS`] lists, per preset, surface and edge, every genuine break with its
//! kind and the drawing it is read from. [`planform_violations`] builds the leading and
//! trailing edge polylines of a preset (main wing from the side of body outboard; tails
//! from the root line to the tip) and reports:
//!
//! * an edge vertex that is not inside a declared break and is a corner: its sweep change
//!   exceeds [`MAX_UNDECLARED_SWEEP_CHANGE_DEG`] and it lies more than
//!   [`MAX_UNDECLARED_DEVIATION_M`] off the straight chord between the neighbouring
//!   breaks;
//! * a declared point break that is not a corner (it must be removed or the data fixed);
//! * a declared break with no vertex of the planform at its span fraction.
//!
//! A zone (`from_eta < to_eta`) covers a drawn curve (fillet, rounded tip, winglet root
//! flare): the vertices inside it approximate the curve and are not checked, but the
//! vertices at its ends and every vertex outside it are.
//!
//! Frame: `eta` is the span fraction (distance from the centreline over the semispan for
//! the wing; from the configured root line over the root-to-tip run for a tail); `x` is
//! metres aft in the planform projection, in the frame of the planform stations. Tails
//! are taken at unit tail scale.

use super::AircraftPreset;
use crate::geometry::TailEdgeStation;

/// A vertex of an edge counts as a corner when its sweep change exceeds this, degrees.
pub const MAX_UNDECLARED_SWEEP_CHANGE_DEG: f64 = 0.1;

/// ... and when it is further than this from the straight chord between the two
/// neighbouring declared breaks, metres. A vertex must break both limits to be an error,
/// so a data rounding of a few tenths of a millimetre on a long straight edge never does.
pub const MAX_UNDECLARED_DEVIATION_M: f64 = 0.001;

/// A declared point break must change the sweep by at least this, degrees.
pub const MIN_DECLARED_SWEEP_CHANGE_DEG: f64 = 0.1;

/// Span-fraction tolerance for matching a declared break to a vertex.
const ETA_TOLERANCE: f64 = 1.0e-3;

/// Which surface of a preset a break belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanformSurface {
    /// The main wing, from the side of body to the tip.
    MainWing,
    /// The horizontal tail.
    HorizontalTail,
    /// The vertical tail.
    VerticalTail,
}

/// Which edge of the surface a break belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanformEdge {
    /// Leading edge only.
    Leading,
    /// Trailing edge only.
    Trailing,
    /// Both edges.
    Both,
}

/// What kind of planform break it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanformBreakKind {
    /// Yehudi kink: the trailing edge (or both) changes sweep where the wing is cranked.
    Kink,
    /// A crank or step of one edge away from the kink (an inboard leading-edge crank,
    /// a trailing-edge step at a flap end).
    Crank,
    /// Start of a swept-back tip rake, to the tip.
    Rake,
    /// Junction of the wing with a winglet or sharklet, to the tip.
    Winglet,
    /// Fillet or blend at a body junction: a wing root fillet, a dorsal fillet.
    Fillet,
    /// Rounded corners of a tail tip, a polyline over a drawn curve.
    TipRound,
    /// A drawn fairing on a tail: the bullet at the top of a fin.
    Fairing,
}

/// One genuine break (`from_eta == to_eta`) or one drawn curve (`from_eta < to_eta`).
#[derive(Debug, Clone, Copy)]
pub struct DeclaredBreak {
    /// Registry name of the preset.
    pub preset: &'static str,
    /// The surface.
    pub surface: PlanformSurface,
    /// The edge.
    pub edge: PlanformEdge,
    /// What it is.
    pub kind: PlanformBreakKind,
    /// Span fraction where it starts.
    pub from_eta: f64,
    /// Span fraction where it ends (equal to `from_eta` for a corner).
    pub to_eta: f64,
    /// The drawing or document it is read from.
    pub source: &'static str,
}

const fn brk(
    preset: &'static str,
    surface: PlanformSurface,
    edge: PlanformEdge,
    kind: PlanformBreakKind,
    from_eta: f64,
    to_eta: f64,
    source: &'static str,
) -> DeclaredBreak {
    DeclaredBreak {
        preset,
        surface,
        edge,
        kind,
        from_eta,
        to_eta,
        source,
    }
}

use PlanformBreakKind as K;
use PlanformEdge as E;
use PlanformSurface as S;

const A340_AC: &str =
    "Airbus A340 Aircraft Characteristics Dec-2025, fig 2-2-0-991-007-A01, plan view";
const A350_AC: &str =
    "Airbus A350 Aircraft Characteristics Rev 16 Feb-2026, fig 2-2-0-991-001-A01 sheet 2, plan view";
const A380_AC: &str =
    "Airbus A380 Aircraft Characteristics Rev 20, fig 2-2-0-991-001-A01 sheet 2, plan view";
const A320_AC: &str =
    "Airbus A320 Aircraft Characteristics Jul-2026, fig 2-2-0-991-004-A01, plan view";
const A220_AC: &str = "Airbus A220 Airport Characteristics issue 013, fig 1, plan view";
const B787_AC: &str =
    "Boeing 787 Airplane Characteristics for Airport Planning, sec 2.2.2 (787-9), plan view";
const B747_AC: &str =
    "Boeing 747-400 Airplane Characteristics for Airport Planning Rev F, sec 2.2.1, plan view";
const DC10_AC: &str =
    "McDonnell Douglas DC-10 Airplane Characteristics for Airport Planning, p 14, plan view";
const E2_APM: &str = "Embraer E2 Airport Planning Manual Rev 21, fig 2.2 (195-E2), plan view";
const C919_APM: &str = "COMAC C919 Airport Planning Manual issue 003, ch 02-02 fig 1, plan view";
const ATR_EWADE: &str = "ATR 72 EWADE 2011, three-view, plan view";
const LJ60_PTM: &str = "FlightSafety Learjet 60 Pilot Training Manual, fig 1-2, plan view";
const C295_JANES: &str =
    "Jane's All the World's Aircraft 2008-09, p 545, general arrangement plan view";
const A400M_BR: &str = "Airbus A400M brochure 2025, p 25, tail views";
const TAIL_TIP: &str = "tail tip outline read off the same drawing set, plan and side views";

/// Every genuine planform break and drawn curve of the presets (see the module
/// documentation). A preset or surface without a row has straight edges end to end.
pub const DECLARED_BREAKS: &[DeclaredBreak] = &[
    // --- main wings
    brk("AVE", S::MainWing, E::Trailing, K::Kink, 0.35, 0.35, "AVE reference twin: Yehudi at 35 % of the semispan (design-space definition)"),
    brk("A340-300", S::MainWing, E::Leading, K::Fillet, 0.1, 0.158_89, A340_AC),
    brk("A340-300", S::MainWing, E::Trailing, K::Kink, 0.319_52, 0.319_52, A340_AC),
    brk("A340-300", S::MainWing, E::Trailing, K::Kink, 0.679_1, 0.679_1, A340_AC),
    brk("A350-900", S::MainWing, E::Leading, K::Crank, 0.227, 0.227, A350_AC),
    brk("A350-900", S::MainWing, E::Leading, K::Rake, 0.937_3, 1.0, A350_AC),
    brk("A350-900", S::MainWing, E::Trailing, K::Kink, 0.344_7, 0.344_7, A350_AC),
    brk("A380-800", S::MainWing, E::Leading, K::Fillet, 0.089_53, 0.135_4, A380_AC),
    brk("A380-800", S::MainWing, E::Leading, K::Kink, 0.359_24, 0.359_24, A380_AC),
    brk("A380-800", S::MainWing, E::Leading, K::Rake, 0.959_6, 1.0, A380_AC),
    brk("A380-800", S::MainWing, E::Trailing, K::Crank, 0.289_1, 0.289_1, A380_AC),
    brk("A380-800", S::MainWing, E::Trailing, K::Kink, 0.660_3, 0.660_3, A380_AC),
    brk("B787-9", S::MainWing, E::Both, K::Rake, 0.898, 1.0, B787_AC),
    brk("B787-9", S::MainWing, E::Trailing, K::Fillet, 0.336, 0.406, "Boeing 787 Airplane Characteristics for Airport Planning, sec 2.2.2 (787-9), plan view: the trailing-edge kink is drawn rounded"),
    brk("B787-9", S::MainWing, E::Trailing, K::Kink, 0.369_6, 0.369_6, B787_AC),
    brk("B787-9", S::MainWing, E::Trailing, K::Kink, 0.75, 0.75, B787_AC),
    brk("A320-200", S::MainWing, E::Leading, K::Fillet, 0.115_84, 0.136, A320_AC),
    brk("A320-200", S::MainWing, E::Leading, K::Rake, 0.961_7, 1.0, A320_AC),
    brk("A320-200", S::MainWing, E::Trailing, K::Kink, 0.379_3, 0.379_3, A320_AC),
    brk("A220-300", S::MainWing, E::Leading, K::Fillet, 0.1, 0.125, A220_AC),
    brk("A220-300", S::MainWing, E::Both, K::Rake, 0.92, 1.0, A220_AC),
    brk("A220-300", S::MainWing, E::Trailing, K::Kink, 0.36, 0.36, A220_AC),
    brk("ATR72-600", S::MainWing, E::Both, K::Kink, 0.32, 0.32, ATR_EWADE),
    brk("ATR72-600", S::MainWing, E::Leading, K::Rake, 0.97, 1.0, ATR_EWADE),
    brk("DC-10", S::MainWing, E::Both, K::Rake, 0.992, 1.0, DC10_AC),
    brk("DC-10", S::MainWing, E::Trailing, K::Kink, 0.373, 0.373, DC10_AC),
    brk("E195-E2", S::MainWing, E::Both, K::Fillet, 0.086, 0.113_9, E2_APM),
    brk("E195-E2", S::MainWing, E::Both, K::Rake, 0.887, 1.0, E2_APM),
    brk("E195-E2", S::MainWing, E::Trailing, K::Kink, 0.322_8, 0.322_8, E2_APM),
    brk("C919", S::MainWing, E::Leading, K::Fillet, 0.117_96, 0.131_1, C919_APM),
    brk("C919", S::MainWing, E::Trailing, K::Fillet, 0.274, 0.476_6, "COMAC C919 Airport Planning Manual issue 003, ch 02-02 fig 1, plan view: a fillet joins the inboard flat (y 4.6 m) to the straight outboard line (y 8.0 m)"),
    brk("C919", S::MainWing, E::Trailing, K::Kink, 0.382_5, 0.382_5, C919_APM),
    brk("B747-400", S::MainWing, E::Leading, K::Crank, 0.690_3, 0.690_3, B747_AC),
    brk("B747-400", S::MainWing, E::Trailing, K::Kink, 0.421_9, 0.421_9, B747_AC),
    brk("LJ60", S::MainWing, E::Leading, K::Fillet, 0.156_2, 0.278_9, LJ60_PTM),
    brk("LJ60", S::MainWing, E::Leading, K::Rake, 0.956, 1.0, LJ60_PTM),
    brk("LJ60", S::MainWing, E::Trailing, K::Crank, 0.462_2, 0.462_2, LJ60_PTM),
    brk("LJ60", S::MainWing, E::Trailing, K::Kink, 0.478_1, 0.478_1, LJ60_PTM),
    brk("LJ60", S::MainWing, E::Trailing, K::Winglet, 0.973_2, 1.0, LJ60_PTM),
    brk("C295", S::MainWing, E::Both, K::Kink, 0.328_6, 0.328_6, C295_JANES),
    brk("C295", S::MainWing, E::Leading, K::Rake, 0.956, 1.0, C295_JANES),
    // --- horizontal tails (tip outlines)
    brk("A340-300", S::HorizontalTail, E::Both, K::TipRound, 0.91, 1.0, TAIL_TIP),
    brk("A350-900", S::HorizontalTail, E::Both, K::TipRound, 0.905, 1.0, TAIL_TIP),
    brk("A380-800", S::HorizontalTail, E::Both, K::TipRound, 0.92, 1.0, TAIL_TIP),
    brk("B787-9", S::HorizontalTail, E::Both, K::TipRound, 0.97, 1.0, TAIL_TIP),
    brk("A320-200", S::HorizontalTail, E::Both, K::TipRound, 0.965, 1.0, TAIL_TIP),
    brk("A220-300", S::HorizontalTail, E::Both, K::TipRound, 0.98, 1.0, TAIL_TIP),
    brk("ATR72-600", S::HorizontalTail, E::Leading, K::Fillet, 0.0, 0.71, "ATR 72 EWADE 2011, plan: tailplane LE ahead of its line inboard of y 2.6 m, held to the root"),
    brk("ATR72-600", S::HorizontalTail, E::Both, K::TipRound, 0.85, 1.0, ATR_EWADE),
    brk("DC-10", S::HorizontalTail, E::Both, K::TipRound, 0.965, 1.0, TAIL_TIP),
    brk("E195-E2", S::HorizontalTail, E::Both, K::TipRound, 0.94, 1.0, TAIL_TIP),
    brk("A400M", S::HorizontalTail, E::Both, K::TipRound, 0.92, 1.0, A400M_BR),
    // --- vertical tails
    brk("A340-300", S::VerticalTail, E::Both, K::TipRound, 0.96, 1.0, TAIL_TIP),
    brk("A350-900", S::VerticalTail, E::Both, K::TipRound, 0.97, 1.0, TAIL_TIP),
    brk("A380-800", S::VerticalTail, E::Leading, K::Fillet, 0.0, 0.14, "Airbus A380 Aircraft Characteristics Rev 20, side view: fin root fillet"),
    brk("A380-800", S::VerticalTail, E::Both, K::TipRound, 0.96, 1.0, TAIL_TIP),
    brk("B787-9", S::VerticalTail, E::Both, K::TipRound, 0.92, 1.0, TAIL_TIP),
    brk("A320-200", S::VerticalTail, E::Both, K::TipRound, 0.96, 1.0, TAIL_TIP),
    brk("A220-300", S::VerticalTail, E::Both, K::TipRound, 0.96, 1.0, TAIL_TIP),
    brk("ATR72-600", S::VerticalTail, E::Leading, K::Fillet, 0.0, 0.93, "ATR 72 EWADE 2011, side view: fin LE fillet into the dorsal and the small bow above it (straight within 0.15 m above z 3.3 m)"),
    brk("ATR72-600", S::VerticalTail, E::Both, K::TipRound, 0.93, 1.0, ATR_EWADE),
    brk("DC-10", S::VerticalTail, E::Both, K::TipRound, 0.97, 1.0, TAIL_TIP),
    brk("E195-E2", S::VerticalTail, E::Both, K::TipRound, 0.965_3, 1.0, TAIL_TIP),
    brk("C919", S::VerticalTail, E::Both, K::TipRound, 0.96, 1.0, TAIL_TIP),
    brk("A400M", S::VerticalTail, E::Both, K::TipRound, 0.8, 1.0, A400M_BR),
    brk("LJ60", S::VerticalTail, E::Both, K::Fairing, 0.812, 1.0, "FlightSafety Learjet 60 Pilot Training Manual, fig 1-2, side view: bullet on the fin top"),
];

/// One vertex of an edge polyline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeVertex {
    /// Span fraction.
    pub eta: f64,
    /// Spanwise coordinate, metres.
    pub y_m: f64,
    /// Chordwise coordinate, metres, positive aft.
    pub x_m: f64,
}

/// The leading and trailing edge polylines of one surface.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgePolylines {
    /// Leading edge, root (or side of body) to tip.
    pub leading: Vec<EdgeVertex>,
    /// Trailing edge, root (or side of body) to tip.
    pub trailing: Vec<EdgeVertex>,
}

/// The edge polylines of `surface` of `preset`, `None` when the preset has no such planform.
#[must_use]
pub fn edge_polylines(preset: &AircraftPreset, surface: PlanformSurface) -> Option<EdgePolylines> {
    match surface {
        PlanformSurface::MainWing => {
            let planform = preset
                .geometry
                .wing
                .transport_planform(&preset.design_vector)
                .ok()?;
            let semi_span_m = planform.tip.y_m;
            let first = planform.side_of_body.map_or(0.0, |sob| sob.y_m);
            let stations: Vec<_> = planform
                .stations_with_rake()
                .into_iter()
                .filter(|station| station.y_m >= first - 1.0e-9)
                .collect();
            let vertex = |station: &crate::MainWingStation, x_m: f64| EdgeVertex {
                eta: station.y_m / semi_span_m,
                y_m: station.y_m,
                x_m,
            };
            Some(EdgePolylines {
                leading: stations
                    .iter()
                    .map(|s| vertex(s, s.leading_edge_x_m))
                    .collect(),
                trailing: stations
                    .iter()
                    .map(|s| vertex(s, s.leading_edge_x_m + s.chord_m))
                    .collect(),
            })
        }
        PlanformSurface::HorizontalTail => {
            let e = &preset.geometry.empennage;
            tail_polylines(
                e.hstab_tip_le_m.1.abs(),
                e.hstab_tip_le_m.0,
                e.hstab_root_chord_m,
                e.hstab_tip_chord_m,
                &e.hstab_edge_stations,
            )
        }
        PlanformSurface::VerticalTail => {
            let e = &preset.geometry.empennage;
            tail_polylines(
                e.vstab_tip_le_m.2.abs(),
                e.vstab_tip_le_m.0,
                e.vstab_root_chord_m,
                e.vstab_tip_chord_m,
                &e.vstab_edge_stations,
            )
        }
    }
}

fn tail_polylines(
    run_m: f64,
    tip_le_x_m: f64,
    root_chord_m: f64,
    tip_chord_m: f64,
    stations: &[TailEdgeStation],
) -> Option<EdgePolylines> {
    if run_m <= 0.0 || stations.is_empty() {
        return None;
    }
    // Offsets are from the straight lines; the root and, unless a station sits there, the
    // tip are on them.
    let mut points: Vec<(f64, f64, f64)> = vec![(0.0, 0.0, 0.0)];
    points.extend(
        stations
            .iter()
            .map(|s| (s.span_fraction, s.le_offset_m, s.te_offset_m)),
    );
    if points.last().is_some_and(|p| p.0 < 1.0) {
        points.push((1.0, 0.0, 0.0));
    }
    let leading = |eta: f64, off: f64| eta * tip_le_x_m + off;
    let chord = |eta: f64| root_chord_m + eta * (tip_chord_m - root_chord_m);
    Some(EdgePolylines {
        leading: points
            .iter()
            .map(|&(eta, le, _)| EdgeVertex {
                eta,
                y_m: eta * run_m,
                x_m: leading(eta, le),
            })
            .collect(),
        trailing: points
            .iter()
            .map(|&(eta, _, te)| EdgeVertex {
                eta,
                y_m: eta * run_m,
                x_m: leading(eta, 0.0) + chord(eta) + te,
            })
            .collect(),
    })
}

/// A planform edge that breaks the rule.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanformViolation {
    /// Registry name of the preset.
    pub preset: &'static str,
    /// The surface.
    pub surface: PlanformSurface,
    /// `"leading"` or `"trailing"`.
    pub edge: &'static str,
    /// Span fraction of the vertex or break.
    pub eta: f64,
    /// What is wrong.
    pub what: String,
}

fn sweep_deg(a: &EdgeVertex, b: &EdgeVertex) -> f64 {
    (b.x_m - a.x_m).atan2(b.y_m - a.y_m).to_degrees()
}

/// Perpendicular distance, metres, of `v` from the straight line through `a` and `b` in
/// the planform plane.
fn deviation_m(a: &EdgeVertex, b: &EdgeVertex, v: &EdgeVertex) -> f64 {
    let (dy, dx) = (b.y_m - a.y_m, b.x_m - a.x_m);
    ((v.y_m - a.y_m) * dx - (v.x_m - a.x_m) * dy).abs() / dy.hypot(dx)
}

fn covers(edge: PlanformEdge, leading: bool) -> bool {
    matches!(
        (edge, leading),
        (PlanformEdge::Both, _) | (PlanformEdge::Leading, true) | (PlanformEdge::Trailing, false)
    )
}

/// Every violation of the straight-edge rule in `preset`, main wing and both tails.
#[must_use]
pub fn planform_violations(preset: &AircraftPreset) -> Vec<PlanformViolation> {
    planform_audit(preset).0
}

/// The violations of [`planform_violations`] and the largest distance, metres, of an
/// undeclared vertex from the straight chord between its neighbouring breaks (the
/// margin the fixed presets keep to [`MAX_UNDECLARED_DEVIATION_M`]).
#[must_use]
pub fn planform_audit(preset: &AircraftPreset) -> (Vec<PlanformViolation>, f64) {
    let mut out = Vec::new();
    let mut largest_m = 0.0_f64;
    for surface in [
        PlanformSurface::MainWing,
        PlanformSurface::HorizontalTail,
        PlanformSurface::VerticalTail,
    ] {
        let Some(polylines) = edge_polylines(preset, surface) else {
            continue;
        };
        for (leading, vertices, name) in [
            (true, &polylines.leading, "leading"),
            (false, &polylines.trailing, "trailing"),
        ] {
            let breaks: Vec<&DeclaredBreak> = DECLARED_BREAKS
                .iter()
                .filter(|b| {
                    b.preset == preset.name && b.surface == surface && covers(b.edge, leading)
                })
                .collect();
            check_edge(
                preset.name,
                surface,
                name,
                vertices,
                &breaks,
                &mut out,
                &mut largest_m,
            );
        }
    }
    (out, largest_m)
}

fn check_edge(
    preset: &'static str,
    surface: PlanformSurface,
    edge: &'static str,
    vertices: &[EdgeVertex],
    breaks: &[&DeclaredBreak],
    out: &mut Vec<PlanformViolation>,
    largest_m: &mut f64,
) {
    let mut report = |eta: f64, what: String| {
        out.push(PlanformViolation {
            preset,
            surface,
            edge,
            eta,
            what,
        });
    };
    let in_zone = |eta: f64| {
        breaks
            .iter()
            .any(|b| eta >= b.from_eta - ETA_TOLERANCE && eta <= b.to_eta + ETA_TOLERANCE)
    };
    // A declared break needs vertices at its ends.
    for b in breaks {
        for eta in [b.from_eta, b.to_eta] {
            if !vertices
                .iter()
                .any(|v| (v.eta - eta).abs() <= ETA_TOLERANCE)
            {
                report(
                    eta,
                    format!("declared {:?} has no vertex at eta {eta}", b.kind),
                );
            }
        }
        // A point break must be a corner.
        if (b.to_eta - b.from_eta).abs() <= ETA_TOLERANCE {
            if let Some(i) = vertices
                .iter()
                .position(|v| (v.eta - b.from_eta).abs() <= ETA_TOLERANCE)
            {
                if i > 0 && i + 1 < vertices.len() {
                    let change = (sweep_deg(&vertices[i], &vertices[i + 1])
                        - sweep_deg(&vertices[i - 1], &vertices[i]))
                    .abs();
                    if change < MIN_DECLARED_SWEEP_CHANGE_DEG {
                        report(
                            b.from_eta,
                            format!(
                                "declared {:?} is straight (sweep change {change:.3} deg): remove the declaration",
                                b.kind
                            ),
                        );
                    }
                }
            }
        }
    }
    // Pinned vertices bound the straight runs.
    let pinned: Vec<bool> = vertices
        .iter()
        .enumerate()
        .map(|(i, v)| i == 0 || i + 1 == vertices.len() || in_zone(v.eta))
        .collect();
    for i in 1..vertices.len().saturating_sub(1) {
        if pinned[i] {
            continue;
        }
        let left = (0..i).rev().find(|&j| pinned[j]).unwrap_or(0);
        let right = (i + 1..vertices.len())
            .find(|&j| pinned[j])
            .unwrap_or(vertices.len() - 1);
        let deviation = deviation_m(&vertices[left], &vertices[right], &vertices[i]);
        *largest_m = largest_m.max(deviation);
        let change = (sweep_deg(&vertices[i], &vertices[i + 1])
            - sweep_deg(&vertices[i - 1], &vertices[i]))
        .abs();
        if deviation > MAX_UNDECLARED_DEVIATION_M && change > MAX_UNDECLARED_SWEEP_CHANGE_DEG {
            report(
                vertices[i].eta,
                format!(
                    "undeclared corner: sweep change {change:.3} deg, {:.1} mm off the straight line between eta {:.4} and {:.4}",
                    deviation * 1000.0,
                    vertices[left].eta,
                    vertices[right].eta
                ),
            );
        }
    }
}

// Tests assert on values built here directly, so an unwrap is the assertion failing.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::registry;
    use crate::{TailEdgeStation, WingEdgeStation};

    fn describe(violations: &[PlanformViolation]) -> String {
        violations
            .iter()
            .map(|v| {
                format!(
                    "{} {:?} {} eta {:.4}: {}",
                    v.preset, v.surface, v.edge, v.eta, v.what
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn every_preset_has_straight_edges_between_declared_breaks() {
        let violations: Vec<_> = registry().iter().flat_map(planform_violations).collect();
        assert!(violations.is_empty(), "\n{}", describe(&violations));
    }

    #[test]
    fn every_declared_break_names_a_preset_and_a_source() {
        for b in DECLARED_BREAKS {
            assert!(
                registry().iter().any(|p| p.name == b.preset),
                "{} is not a preset",
                b.preset
            );
            assert!(
                b.source.len() > 20,
                "{} {:?} {}: no source",
                b.preset,
                b.kind,
                b.from_eta
            );
            assert!(
                b.to_eta >= b.from_eta,
                "{} {:?}: zone runs backwards",
                b.preset,
                b.kind
            );
            assert!(
                (0.0..=1.0).contains(&b.from_eta) && (0.0..=1.0).contains(&b.to_eta),
                "{} {:?}: eta outside the span",
                b.preset,
                b.kind
            );
        }
    }

    /// The spurious stations the audit removed would each have been caught.
    #[test]
    fn the_removed_scan_artefacts_are_rejected() {
        let mut old = crate::presets::get("A350-900").unwrap().clone();
        old.geometry.wing.edge_stations = vec![
            WingEdgeStation {
                span_fraction: 0.27,
                le_offset_m: 0.48,
                te_offset_m: 0.0,
            },
            WingEdgeStation {
                span_fraction: 0.90,
                le_offset_m: 0.19,
                te_offset_m: 0.0,
            },
        ];
        let found = planform_violations(&old);
        assert!(
            found.iter().any(|v| (v.eta - 0.9).abs() < 1e-3),
            "{}",
            describe(&found)
        );
        assert!(!found.is_empty());

        // C919: the 0.130 m trailing-edge offset at 56.6 % span, on the outboard run.
        let mut c919 = crate::presets::get("C919").unwrap().clone();
        let wobble = c919
            .geometry
            .wing
            .edge_stations
            .iter_mut()
            .find(|s: &&mut WingEdgeStation| (s.span_fraction - 0.566).abs() < 1e-9)
            .unwrap();
        wobble.te_offset_m = -0.130;
        assert!(!planform_violations(&c919).is_empty());

        // A340-300: the 0.137 m offset at 26.25 % span that bent the inboard trailing edge by 7 deg.
        let mut a340 = crate::presets::get("A340-300").unwrap().clone();
        a340.geometry.wing.edge_stations.push(WingEdgeStation {
            span_fraction: 0.2625,
            le_offset_m: 0.0,
            te_offset_m: -0.137,
        });
        assert!(!planform_violations(&a340).is_empty());

        let mut b787 = crate::presets::get("B787-9").unwrap().clone();
        // The 0.05 m trailing-edge bow the B787 tailplane carried at 30 % span.
        let bow = b787
            .geometry
            .empennage
            .hstab_edge_stations
            .iter_mut()
            .find(|s: &&mut TailEdgeStation| (s.span_fraction - 0.30).abs() < 1e-9)
            .unwrap();
        bow.te_offset_m = 0.05;
        assert!(!planform_violations(&b787).is_empty());
    }
}
