// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The dorsal fin fillet, seated on the top of the body.
//!
//! Frame: x metres aft of the nose tip, y to starboard, z up, in the fin plane.
//!
//! The side line is one law on every aircraft: a rational quadratic from the seat point
//! on the body top to the fin leading edge at the dorsal height, its control point where
//! the body top's tangent at the seat meets the fin edge's tangent at that height. It so
//! leaves the crown and joins the fin without a kink. One shoulder number sets how far it
//! is drawn to the control point; a drawing with a nearly straight line states a small
//! one, and a line that leaves the body behind a nacelle states how high it starts.
//!
//! What stands above the body ahead of the fin leading edge is the exposed fillet: the
//! outline the side line, the fin edge and the body top close. Its sections, cut at
//! constant height, are the surface's reference planform, so wetted area, drag and paint
//! read the exposed area ([`Wing::skin_planform`]).
//!
//! The built sections carry the same leading edges but run on inside the fin, where a
//! drawn fillet has its width from: ahead of the fin's thickest station the fin's own half
//! thickness is stretched over the longer run, so the fillet thins to its leading edge,
//! and from there aft they stay just inside the fin. That hides the fin's nose inside the
//! fillet, with no step at the root and no gap between the two.
//!
//! With `vstab_dorsal_in_aero` the surface stays the exposed plate (the thinnest section
//! the NACA generator describes): a section running on inside the fin would put lattice
//! panels on the fin's. A dorsal fin is generally a flat plate, no airfoil section, in
//! that model (Sadraey, Aircraft Design: A Systems Engineering Approach, 6.8.2.3).

use alas_config::DesignVector;

use super::fin_root::FinSeat;
use super::{tail_attachment, AircraftBuilder, BuildError};
use crate::aircraft::airfoil::Airfoil;
use crate::aircraft::wing::{Wing, WingXSec, DORSAL_FIN_NAME, DORSAL_FIN_OUTSIDE_LATTICE_NAME};

/// The plate section (see the module documentation).
const DORSAL_SECTION: &str = "naca0002";

/// The side line's shoulder when the configuration states none: the fraction of the way
/// from the mid-chord of seat to fin edge towards the control point. Read off the drawn
/// dorsal lines, whose fit barely changes between 0.25 and 0.35.
const DEFAULT_DORSAL_SHOULDER: f64 = 0.3;

/// Points along the side line, past the seat.
const SIDE_LINE_POINTS: usize = 32;

/// Half base of the body top's tangent at the seat, m: over the local slope, under the
/// station spacing of a tail cone.
const BODY_TANGENT_BASE_M: f64 = 0.05;

/// Base of the fin edge's tangent above the dorsal height, m.
const FIN_TANGENT_BASE_M: f64 = 0.1;

/// Under a side line that starts above the body, the leading edge runs this far forward
/// per metre down to the body: the part behind a nacelle, which no drawing shows.
const SEAT_RUN_PER_RISE: f64 = 2.0;

/// Sections of a built dorsal, at least: fine enough that the curved edge reads as a curve.
const BUILT_STRIPS: usize = 24;

/// How far inside the fin a built section's half thickness stays, as a fraction of it.
const INSIDE_FIN_FRACTION: f64 = 0.99;

/// Where a built section ends on the fin chord: short of the fin's trailing edge.
const SECTION_END_ON_FIN: f64 = 0.97;

/// Points of a built section's surface ahead of the fin's thickest station, and aft of it.
const SECTION_POINTS_AHEAD: usize = 28;
const SECTION_POINTS_AFT: usize = 12;

/// Chordwise samples of the fin's half thickness.
const FIN_THICKNESS_SAMPLES: usize = 61;

/// Smallest section chord as a fraction of the longest: a nearly sharp end, as a
/// zero-chord section cannot be meshed.
const DORSAL_END_CHORD_FRACTION: f64 = 0.02;

/// A fillet whose longest section is shorter than this has no length, m.
const SHORTEST_DORSAL_M: f64 = 1.0e-3;

/// Steps of the searches along the leading edge and the fin edge, before bisection.
const SEARCH_STEPS: usize = 256;

/// Bisection steps of those searches: far below a millimetre.
const BISECTION_STEPS: usize = 48;

/// Points on the body top between the junction and the seat point.
const BODY_TOP_POINTS: usize = 24;

/// Heights inside a cut outline beyond the fin edge stations that can be a corner: the
/// seat point, the junction and a held leading edge's corner, and one so that a strip
/// is left between them.
const CORNERS_BEYOND_STATIONS: usize = 4;

/// Below this sine of the turn, two runs of a fin edge are one straight run.
const STRAIGHT_RUN_SINE: f64 = 1.0e-9;

/// Corners closer in height than this are one, m.
const SAME_HEIGHT_M: f64 = 1.0e-6;

/// The built fin leading edge, root to tip: `[x, y, z]` per section.
pub(super) struct FinEdge(pub(super) Vec<[f64; 3]>);

impl FinEdge {
    /// The edge at height `z_m`, held at its ends.
    pub(super) fn at(&self, z_m: f64) -> [f64; 3] {
        let points = &self.0;
        let Some(pair) = points.windows(2).find(|pair| z_m <= pair[1][2]) else {
            return points.last().copied().unwrap_or([0.0, 0.0, z_m]);
        };
        let span = pair[1][2] - pair[0][2];
        let s = if span > 0.0 {
            ((z_m - pair[0][2]) / span).max(0.0)
        } else {
            0.0
        };
        let along = |axis: usize| pair[0][axis] + s * (pair[1][axis] - pair[0][axis]);
        [along(0), along(1), along(2)]
    }

    pub(super) fn root_z(&self) -> f64 {
        self.0.first().map_or(0.0, |point| point[2])
    }

    pub(super) fn tip_z(&self) -> f64 {
        self.0.last().map_or(0.0, |point| point[2])
    }

    /// Heights of the edge's corners strictly inside `(low, high)`: the points where it
    /// turns. The sections on a straight run of the fin are no corners of this surface,
    /// and how many of them lie inside changes with the tail scale.
    pub(super) fn corners(&self, low: f64, high: f64) -> impl Iterator<Item = f64> + '_ {
        self.0
            .windows(3)
            .filter(|points| {
                let (a, b) = (
                    [points[1][0] - points[0][0], points[1][2] - points[0][2]],
                    [points[2][0] - points[1][0], points[2][2] - points[1][2]],
                );
                (a[0] * b[1] - a[1] * b[0]).abs()
                    > STRAIGHT_RUN_SINE * a[0].hypot(a[1]) * b[0].hypot(b[1])
            })
            .map(|points| points[1][2])
            .filter(move |z| *z > low && *z < high)
    }
}

/// Whether `(x_m, z_m)` stands above every body there.
fn exposed(seat: &FinSeat, x_m: f64, z_m: f64) -> bool {
    seat.top_at(x_m).is_none_or(|top| z_m > top)
}

/// The parameter in `[0, 1]` closest to `0` at which `exposed_at` stops holding, where
/// it holds at `0`; `None` if it holds everywhere sampled.
fn first_buried(exposed_at: impl Fn(f64) -> bool) -> Option<f64> {
    let index =
        (1..=SEARCH_STEPS).find(|index| !exposed_at(*index as f64 / SEARCH_STEPS as f64))?;
    let (mut above, mut below) = (
        (index - 1) as f64 / SEARCH_STEPS as f64,
        index as f64 / SEARCH_STEPS as f64,
    );
    for _ in 0..BISECTION_STEPS {
        let middle = 0.5 * (above + below);
        if exposed_at(middle) {
            above = middle;
        } else {
            below = middle;
        }
    }
    Some(above)
}

/// The dorsal's side line, `(x, z)`, seat to fin edge: see the module documentation. `None`
/// when no body lies under the seat. Where the two tangents do not meet ahead of the fin
/// edge and above the seat there is no shoulder to draw to, and the line is the chord.
fn side_line(
    edge: &FinEdge,
    seat: &FinSeat,
    x_seat_m: f64,
    rise_m: f64,
    z_top_m: f64,
    shoulder: f64,
) -> Option<Vec<[f64; 2]>> {
    let start = [x_seat_m, seat.top_at(x_seat_m)? + rise_m];
    let end = [edge.at(z_top_m)[0], z_top_m];
    let (ahead, behind) = (
        seat.top_at(x_seat_m - BODY_TANGENT_BASE_M)?,
        seat.top_at(x_seat_m + BODY_TANGENT_BASE_M)?,
    );
    let body_slope = (behind - ahead) / (2.0 * BODY_TANGENT_BASE_M);
    let base = if z_top_m + FIN_TANGENT_BASE_M <= edge.tip_z() {
        FIN_TANGENT_BASE_M
    } else {
        -FIN_TANGENT_BASE_M
    };
    let fin_run = (edge.at(z_top_m + base)[0] - end[0]) / base;
    // start + a (1, body slope) = end - b (fin run, 1), with a and b both positive.
    let (dx, dz) = (end[0] - start[0], end[1] - start[1]);
    let determinant = 1.0 - fin_run * body_slope;
    let (a, b) = (
        (dx - fin_run * dz) / determinant,
        (dz - body_slope * dx) / determinant,
    );
    let mut line = Vec::with_capacity(SIDE_LINE_POINTS + 2);
    if rise_m > 0.0 {
        let x_body = x_seat_m - SEAT_RUN_PER_RISE * rise_m;
        if let Some(z_body) = seat.top_at(x_body) {
            line.push([x_body, z_body]);
        }
    }
    line.push(start);
    if a > 0.0 && b > 0.0 {
        let control = [start[0] + a, start[1] + a * body_slope];
        let weight = shoulder / (1.0 - shoulder);
        line.extend((1..SIDE_LINE_POINTS).map(|index| {
            let t = index as f64 / SIDE_LINE_POINTS as f64;
            let (w0, w1, w2) = ((1.0 - t) * (1.0 - t), 2.0 * weight * t * (1.0 - t), t * t);
            let along = |axis: usize| {
                (w0 * start[axis] + w1 * control[axis] + w2 * end[axis]) / (w0 + w1 + w2)
            };
            [along(0), along(1)]
        }));
    }
    line.push(end);
    Some(line)
}

/// The fillet's outline, `(x, z)`, closed from the last point to the first, from
/// `side_line` and the fin edge. `None` when no part of it stands above the body.
fn seated_outline(
    edge: &FinEdge,
    seat: &FinSeat,
    side_line: Vec<[f64; 2]>,
) -> Option<Vec<[f64; 2]>> {
    let (first, last) = (*side_line.first()?, *side_line.last()?);
    let z_top_m = last[1];
    if last[0].is_nan() || last[0] <= first[0] || !exposed(seat, last[0], z_top_m) {
        return None;
    }
    // The fin edge, down from the dorsal height. A fin edge standing clear all the way
    // down ends on the fin root.
    let z_low = edge.root_z().min(z_top_m);
    let on_fin = |s: f64| z_top_m - s * (z_top_m - z_low);
    let s_junction = first_buried(|s| {
        let z = on_fin(s);
        exposed(seat, edge.at(z)[0], z)
    })
    .unwrap_or(1.0);
    let z_junction = on_fin(s_junction);
    let x_junction = edge.at(z_junction)[0];
    if x_junction <= first[0] {
        return None;
    }
    let mut points = side_line;
    let mut corners: Vec<f64> = edge.corners(z_junction, z_top_m).collect();
    corners.sort_by(|a, b| b.total_cmp(a));
    points.extend(corners.into_iter().map(|z| [edge.at(z)[0], z]));
    points.push([x_junction, z_junction]);
    points.extend((1..BODY_TOP_POINTS).filter_map(|index| {
        let x = x_junction + (first[0] - x_junction) * index as f64 / BODY_TOP_POINTS as f64;
        seat.top_at(x).map(|z| [x, z])
    }));
    Some(points)
}

/// The widest run of `outline` at height `z_m`, `(forward x, aft x)`.
fn cut_at(outline: &[[f64; 2]], z_m: f64) -> Option<(f64, f64)> {
    let mut crossings: Vec<f64> = outline
        .iter()
        .zip(outline.iter().cycle().skip(1))
        .filter(|(a, b)| (a[1] <= z_m) != (b[1] <= z_m))
        .map(|(a, b)| a[0] + (z_m - a[1]) / (b[1] - a[1]) * (b[0] - a[0]))
        .collect();
    crossings.sort_by(f64::total_cmp);
    crossings
        .chunks_exact(2)
        .map(|run| (run[0], run[1]))
        .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
}

/// How many strips [`sections_of`] cuts a surface on the fin into, from the
/// configuration alone: two per mesh strip, as the outline's curved body top is not cut
/// at its points (the area is then within a percent of a fine cut on every preset), or
/// as many as its corners need.
pub(super) fn cut_strips(g: &alas_config::EmpennageConfig) -> usize {
    (2 * super::n_subdivisions_usize(g.n_subdivisions))
        .max(g.vstab_edge_stations.len() + CORNERS_BEYOND_STATIONS)
}

/// The sections of `outline`: one at each of `strips` equal parts of its height, the
/// one nearest a corner moved onto the corner. The section count then does not depend
/// on how many corners a design's fin edge brings inside, so the lattice of a dorsal fin
/// inside it moves with the design but is not re-panelled, which would step every
/// derivative read from it. `None` for an outline without length.
pub(super) fn sections_of(
    outline: &[[f64; 2]],
    edge: &FinEdge,
    strips: usize,
    airfoil: &Airfoil,
) -> Option<Vec<WingXSec>> {
    let cuts = cuts_of(outline, edge, strips, |share| share)?;
    Some(plate_sections(&cuts, edge, airfoil))
}

/// One cut of an outline: height, forward x and aft x, m.
type Cut = (f64, f64, f64);

/// The cuts of `outline` at `strips` parts of its height, `spacing` taking the share of
/// the strips to the share of the height; see [`sections_of`].
fn cuts_of(
    outline: &[[f64; 2]],
    edge: &FinEdge,
    strips: usize,
    spacing: impl Fn(f64) -> f64,
) -> Option<Vec<Cut>> {
    let bottom = outline.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let top = outline
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max);
    if top <= bottom {
        return None;
    }
    // The corners: the seat point, the fin edge's own corners and the junction, on it.
    let mut corners: Vec<f64> = outline.first().map(|p| p[1]).into_iter().collect();
    corners.extend(outline.iter().filter_map(|p| {
        let on_edge = (edge.at(p[1])[0] - p[0]).abs() < 1.0e-9;
        on_edge.then_some(p[1])
    }));
    corners.retain(|z| *z > bottom + SAME_HEIGHT_M && *z < top - SAME_HEIGHT_M);
    corners.sort_by(f64::total_cmp);
    corners.dedup_by(|a, b| (*a - *b).abs() < SAME_HEIGHT_M);
    let strips = strips.max(corners.len() + 1);
    let height_at = |index: usize| bottom + (top - bottom) * spacing(index as f64 / strips as f64);
    let mut heights: Vec<f64> = (0..strips).map(height_at).collect();
    heights.push(top);
    let even = heights.clone();
    let mut free = 1;
    for (index, corner) in corners.iter().enumerate() {
        let nearest = (1..strips)
            .min_by(|a, b| {
                (even[*a] - corner)
                    .abs()
                    .total_cmp(&(even[*b] - corner).abs())
            })
            .unwrap_or(1);
        let last = strips - (corners.len() - index);
        let at = nearest.clamp(free, last);
        heights[at] = *corner;
        free = at + 1;
    }
    heights.sort_by(f64::total_cmp);

    // The outline's ends are points: cut just inside them.
    let inside = 1.0e-9 * (top - bottom);
    let cuts: Vec<Cut> = heights
        .iter()
        .filter_map(|&z| {
            cut_at(outline, z.clamp(bottom + inside, top - inside)).map(|(x0, x1)| (z, x0, x1))
        })
        .collect();
    let longest = cuts.iter().map(|cut| cut.2 - cut.1).fold(0.0, f64::max);
    (longest >= SHORTEST_DORSAL_M && cuts.len() >= 2).then_some(cuts)
}

/// `cuts` as sections of `airfoil`, ending on their aft x.
fn plate_sections(cuts: &[Cut], edge: &FinEdge, airfoil: &Airfoil) -> Vec<WingXSec> {
    let longest = cuts.iter().map(|cut| cut.2 - cut.1).fold(0.0, f64::max);
    let shortest = DORSAL_END_CHORD_FRACTION * longest;
    cuts.iter()
        .map(|&(z, x0, x1)| {
            let chord = (x1 - x0).max(shortest);
            WingXSec::new([x1 - chord, edge.at(z)[1], z], chord, 0.0, airfoil.clone())
        })
        .collect()
}

/// The built fin as a built dorsal section reads it, at one height.
struct FinCut {
    le_x_m: f64,
    chord_m: f64,
    /// Half thickness, m, at equal chord fractions from 0 to 1.
    half_thickness_m: [f64; FIN_THICKNESS_SAMPLES],
    /// The chord fraction of the thickest point.
    thickest_at: f64,
}

impl FinCut {
    fn half_thickness_at(&self, chord_fraction: f64) -> f64 {
        let at = chord_fraction.clamp(0.0, 1.0) * (FIN_THICKNESS_SAMPLES - 1) as f64;
        let low = (at.floor() as usize).min(FIN_THICKNESS_SAMPLES - 2);
        let s = at - low as f64;
        self.half_thickness_m[low]
            + s * (self.half_thickness_m[low + 1] - self.half_thickness_m[low])
    }
}

/// The fin at height `z_m`, held at its ends, between its two nearest sections.
fn fin_cut(fin: &Wing, z_m: f64) -> Option<FinCut> {
    let upper = fin
        .xsecs
        .iter()
        .position(|xsec| z_m <= xsec.xyz_le[2])?
        .max(1);
    let (a, b) = (fin.xsecs.get(upper - 1)?, fin.xsecs.get(upper)?);
    let span = b.xyz_le[2] - a.xyz_le[2];
    let s = if span > 0.0 {
        ((z_m - a.xyz_le[2]) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let stations: Vec<f64> = (0..FIN_THICKNESS_SAMPLES)
        .map(|index| index as f64 / (FIN_THICKNESS_SAMPLES - 1) as f64)
        .collect();
    let thickness_a = a.airfoil.local_thickness(&stations);
    let thickness_b = if a.airfoil == b.airfoil {
        thickness_a.clone()
    } else {
        b.airfoil.local_thickness(&stations)
    };
    let mut half_thickness_m = [0.0; FIN_THICKNESS_SAMPLES];
    let (mut thickest_at, mut thickest) = (0.3, 0.0);
    for index in 0..FIN_THICKNESS_SAMPLES {
        let half =
            0.5 * (1.0 - s) * a.chord * thickness_a[index] + 0.5 * s * b.chord * thickness_b[index];
        half_thickness_m[index] = half;
        if half > thickest {
            (thickest_at, thickest) = (stations[index], half);
        }
    }
    Some(FinCut {
        le_x_m: a.xyz_le[0] + s * (b.xyz_le[0] - a.xyz_le[0]),
        chord_m: a.chord + s * (b.chord - a.chord),
        half_thickness_m,
        thickest_at,
    })
}

/// The built section of one cut: its leading edge the cut's, ahead of the fin's thickest
/// station the fin's half thickness stretched over the longer run, aft of it the fin's own
/// closed a little early. `None` where the fin has no section there.
fn built_section(cut: &Cut, fin: &Wing, edge: &FinEdge) -> Option<WingXSec> {
    let &(z, x0, _) = cut;
    let fin = fin_cut(fin, z)?;
    let x_le = x0.min(fin.le_x_m);
    let run_ahead = fin.le_x_m - x_le;
    let run_to_thickest = run_ahead + fin.thickest_at * fin.chord_m;
    let x_end = fin.le_x_m + SECTION_END_ON_FIN * fin.chord_m;
    let chord = x_end - x_le;
    if run_to_thickest <= 0.0 || chord <= run_to_thickest {
        return None;
    }
    let exponent = 1.0 + run_ahead / run_to_thickest;
    let aft_stretch = (1.0 - fin.thickest_at) / (SECTION_END_ON_FIN - fin.thickest_at);
    let mut surface = Vec::with_capacity(SECTION_POINTS_AHEAD + SECTION_POINTS_AFT + 1);
    for index in 0..=SECTION_POINTS_AHEAD {
        let angle = 0.5 * std::f64::consts::PI * index as f64 / SECTION_POINTS_AHEAD as f64;
        let u = 1.0 - angle.cos();
        let half = fin.half_thickness_at(fin.thickest_at * u.powf(exponent));
        surface.push((
            u * run_to_thickest / chord,
            INSIDE_FIN_FRACTION * half / chord,
        ));
    }
    for index in 1..=SECTION_POINTS_AFT {
        let share = index as f64 / SECTION_POINTS_AFT as f64;
        let x = run_to_thickest + share * (chord - run_to_thickest);
        let on_fin = fin.thickest_at + aft_stretch * share * (SECTION_END_ON_FIN - fin.thickest_at);
        surface.push((
            x / chord,
            INSIDE_FIN_FRACTION * fin.half_thickness_at(on_fin) / chord,
        ));
    }
    let coordinates = surface
        .iter()
        .rev()
        .copied()
        .chain(surface.iter().skip(1).map(|&(x, y)| (x, -y)))
        .collect();
    Some(WingXSec::new(
        [x_le, edge.at(z)[1], z],
        chord,
        0.0,
        Airfoil::from_coordinates("dorsal fillet", coordinates),
    ))
}

impl AircraftBuilder {
    /// The dorsal fin fillet of `dv` on its own, or `None` when there is none; see
    /// [`Self::dorsal_on`].
    ///
    /// # Errors
    ///
    /// The tail and fuselage build errors, and those of [`Self::dorsal_on`].
    pub fn build_dorsal_fin(&self, dv: &DesignVector) -> Result<Option<Wing>, BuildError> {
        let g = &self.geometry.empennage;
        let fuselage = self.build_fuselage_reported(dv, &mut super::BuildReport::default())?;
        let seat = self.fin_seat(dv, &fuselage)?;
        let tail_airfoil = Self::resolve(&g.tail_airfoil)?;
        let own_fin = self.own_fin_airfoil()?;
        let fin_airfoil = own_fin.as_ref().unwrap_or(&tail_airfoil);
        let fin = self.build_vstab(dv, fin_airfoil, true, &seat)?;
        self.dorsal_on(dv, &fin, &seat)
    }

    /// The dorsal fin fillet on the built `fin` and the body top `seat`, or `None`
    /// when none is configured or the configured one has no length or stands nowhere
    /// above the body: a design that leaves no room for it simply has none.
    ///
    /// The surface is part of the built aircraft, its exports and its wetted area.
    /// The vortex lattice carries it only with `vstab_dorsal_in_aero`: its name says
    /// which ([`Wing::is_geometry_only`]). The fillet's flow is a separated
    /// leading-edge vortex that leaves the fin lift curve unchanged up to 15 deg of
    /// sideslip and raises the stall angle above that (Obert, Aerodynamic Design of
    /// Transport Aircraft, ch. 32), which a linear attached lattice cannot represent;
    /// it would count the area as extra fin instead.
    ///
    /// # Errors
    ///
    /// [`BuildError::TailPlacement`] when only one of the dorsal length and height is
    /// set or the height is not positive.
    pub(super) fn dorsal_on(
        &self,
        dv: &DesignVector,
        fin: &Wing,
        seat: &FinSeat,
    ) -> Result<Option<Wing>, BuildError> {
        let g = &self.geometry.empennage;
        let x_fin = tail_attachment::vstab_x(g, dv);
        let length = g.vstab_dorsal_length_m.or(g
            .vstab_dorsal_x_start_m
            .map(|x_start| x_fin - (x_start + dv.tail_x_shift_m)));
        let (length, height) = match (length, g.vstab_dorsal_height_m) {
            (None, None) => return Ok(None),
            (Some(length), Some(height)) => (length, height),
            _ => {
                return Err(BuildError::TailPlacement(
                    "the dorsal fin needs both its length and its height".to_owned(),
                ))
            }
        };
        if !(length.is_finite() && height.is_finite() && height > 0.0) {
            return Err(BuildError::TailPlacement(
                "the dorsal length must be finite and its height positive".to_owned(),
            ));
        }
        if length <= 0.0 {
            return Ok(None);
        }
        let edge = FinEdge(fin.xsecs.iter().map(|xsec| xsec.xyz_le).collect());
        let z_top = (g.vstab_z_m + height).min(edge.tip_z());
        let shoulder = g.vstab_dorsal_shoulder.unwrap_or(DEFAULT_DORSAL_SHOULDER);
        if !(shoulder > 0.0 && shoulder < 1.0) {
            return Err(BuildError::TailPlacement(
                "the dorsal shoulder must lie between 0 and 1".to_owned(),
            ));
        }
        let rise = g.vstab_dorsal_seat_rise_m.unwrap_or(0.0).max(0.0);
        let Some(line) = side_line(&edge, seat, x_fin - length, rise, z_top, shoulder) else {
            return Ok(None);
        };
        let Some(outline) = seated_outline(&edge, seat, line) else {
            return Ok(None);
        };
        let plate = Self::resolve(DORSAL_SECTION)?;
        if g.vstab_dorsal_in_aero {
            let Some(sections) = sections_of(&outline, &edge, cut_strips(g), &plate) else {
                return Ok(None);
            };
            return Ok(Some(Wing::new(DORSAL_FIN_NAME, sections, false)));
        }
        // Crowded towards the body, where the side line runs nearly level.
        let strips = cut_strips(g).max(BUILT_STRIPS);
        let Some(cuts) = cuts_of(&outline, &edge, strips, |share| share * share) else {
            return Ok(None);
        };
        let Some(built) = cuts
            .iter()
            .map(|cut| built_section(cut, fin, &edge))
            .collect()
        else {
            return Ok(None);
        };
        let mut wing = Wing::new(DORSAL_FIN_OUTSIDE_LATTICE_NAME, built, false);
        wing.reference_xsecs = Some(plate_sections(&cuts, &edge, &plate));
        Ok(Some(wing))
    }
}
