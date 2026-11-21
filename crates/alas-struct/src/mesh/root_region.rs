// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The covers of the boxed product deck between the root plane and the first
//! full-length rib.
//!
//! On a swept wing the ribs run normal to the rear spar, so the ribs next to
//! the root are cut short by the streamwise root plane: each one starts at the
//! front spar and ends on the root rib, or on the rear spar a little outboard
//! of it. The region they cross is the swept-root triangle, the highly
//! indeterminate part of the box where the rear-spar root carries the load
//! concentration (M. C. Y. Niu, *Airframe Structural Design*, Conmilit Press,
//! 1988, Sec. 8.2, Fig. 8.2.2, p. 251; NACA TN 2232, Heldenfels, Zender and
//! Libove, 1950, p. 1). Its stress field is what a finite-element box is for,
//! so the skin there is meshed like the rest of the box rather than spanned by
//! one strip from the root to the first full rib.
//!
//! Every truncated rib is a mesh line from the front spar to where it ends,
//! and every bay between two such lines is split into at least
//! [`MIN_ROWS_PER_BAY`] rows of shells by intermediate lines that run between
//! the front spar and the aft boundary (the root rib, then the rear spar). The
//! skin, the spar webs and caps and the truncated ribs therefore share their
//! grids: no truncated rib is tied to the skin through a rigid element, so no
//! load is introduced along an artificial line. The intermediate lines take
//! their plan position from the two ribs either side and their height from
//! the wing's loft (`surface`), so the cover has no step or fold the wing
//! does not have.
// The surface index `s` (0 upper, 1 lower) addresses several parallel arrays.
#![allow(clippy::needless_range_loop)]

use alas_geom::wing_structure::RibStation;

use super::build::RibRegions;
use super::cards::{CoverStrip, Deck};
use super::elements::{add_quad, add_tria, PID_MAIN_RIB, PID_SEC_RIB, PID_SKIN};
use super::nodes::{NodeMap, Surface};

/// Fewest rows of shells across each bay between two truncated ribs.
pub(super) const MIN_ROWS_PER_BAY: usize = 4;

/// Fewest shells along every rib line of the region.
pub(super) const MIN_CHORD_ELEMENTS: usize = 4;
/// Fraction of a root-rib segment within which a truncated rib's root cut
/// ends on the segment's grid rather than beside it.
const SNAP_TO_ROOT_GRID: f64 = 0.25;

/// The two cover surfaces, upper first, as array indices.
const SURFACES: [Surface; 2] = [Surface::Ext, Surface::Int];

/// A point of the region: an existing rib grid, or a new one.
#[derive(Debug, Clone, Copy)]
enum Point {
    Grid(i64, [f64; 3]),
    New([f64; 3]),
}

impl Point {
    fn xyz(self) -> [f64; 3] {
        match self {
            Self::Grid(_, xyz) | Self::New(xyz) => xyz,
        }
    }
}

/// One line of the region on both surfaces, front spar first.
#[derive(Debug, Clone)]
struct Line {
    points: [Vec<Point>; 2],
    /// The rib this line is, when it is one.
    rib: Option<usize>,
}

/// The region, resolved into lines before any grid or element is written.
#[derive(Debug)]
pub(super) struct RootPlan {
    /// The root rib between the spars, front spar first.
    root: [Vec<Point>; 2],
    /// The truncated ribs and then the first full rib, root outboard.
    lines: Vec<Line>,
    /// The first full-length skin rib, where the region ends.
    pub(super) skin_start: usize,
    /// The truncated ribs, whose webs the region writes.
    pub(super) transition_ribs: Vec<usize>,
}

/// What the region wrote, for the rest of the deck to connect to.
#[derive(Debug, Default)]
pub(super) struct RootMesh {
    /// Front-spar grids from the root to the first full rib, upper and lower.
    pub(super) front: [Vec<i64>; 2],
    /// Rear-spar grids from the root to the first full rib, upper and lower.
    pub(super) rear: [Vec<i64>; 2],
    /// Grids on the root plane, constrained with the root rib.
    pub(super) root_plane: Vec<i64>,
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn lerp(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    add(a, scale(sub(b, a), t))
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = sub(a, b);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// Normalised arc-length positions of a polyline's points.
fn arc_params(points: &[[f64; 3]]) -> Vec<f64> {
    let mut s = vec![0.0];
    for pair in points.windows(2) {
        s.push(s.last().copied().unwrap_or(0.0) + dist(pair[0], pair[1]));
    }
    let total = s.last().copied().unwrap_or(0.0);
    if total > 0.0 {
        s.iter_mut().for_each(|v| *v /= total);
    }
    s
}

/// The polyline at normalised arc position `u`.
fn at_param(points: &[[f64; 3]], params: &[f64], u: f64) -> [f64; 3] {
    for k in 0..points.len().saturating_sub(1) {
        if u <= params[k + 1] || k + 2 == points.len() {
            let span = params[k + 1] - params[k];
            let t = if span > 0.0 {
                ((u - params[k]) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return lerp(points[k], points[k + 1], t);
        }
    }
    points.first().copied().unwrap_or([0.0; 3])
}

/// Where the segment `p0 -> p1` crosses `q0 -> q1` in plan view, as the two
/// segment fractions.
fn crossing_xy(p0: [f64; 3], p1: [f64; 3], q0: [f64; 3], q1: [f64; 3]) -> Option<(f64, f64)> {
    let r = [p1[0] - p0[0], p1[1] - p0[1]];
    let s = [q1[0] - q0[0], q1[1] - q0[1]];
    let denom = r[0] * s[1] - r[1] * s[0];
    if denom.abs() < 1e-14 {
        return None;
    }
    let w = [q0[0] - p0[0], q0[1] - p0[1]];
    let t = (w[0] * s[1] - w[1] * s[0]) / denom;
    let u = (w[0] * r[1] - w[1] * r[0]) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some((t, u))
}

fn station_point(station: &RibStation, nodes: &NodeMap, rib: usize, j: usize, s: usize) -> Point {
    let xyz = match SURFACES[s] {
        Surface::Ext => station.extrados[j],
        Surface::Int => station.intrados[j],
    };
    match nodes.get((rib, j, SURFACES[s])) {
        Some(nid) => Point::Grid(nid, xyz),
        None => Point::New(xyz),
    }
}

/// Insert midpoints into the longest segments (measured on the upper surface)
/// until the line has `target` points; the same insertion on both surfaces
/// keeps the rib web between them a row of quadrilaterals.
fn refine_line(line: &mut Line, target: usize) {
    while line.points[0].len() < target {
        let upper: Vec<[f64; 3]> = line.points[0].iter().map(|p| p.xyz()).collect();
        let Some(k) = (0..upper.len().saturating_sub(1))
            .max_by(|&a, &b| dist(upper[a], upper[a + 1]).total_cmp(&dist(upper[b], upper[b + 1])))
        else {
            return;
        };
        for s in 0..2 {
            let (a, b) = (line.points[s][k].xyz(), line.points[s][k + 1].xyz());
            line.points[s].insert(k + 1, Point::New(lerp(a, b, 0.5)));
        }
    }
}

/// Resolve the region into lines, or `None` when the wing has no truncated
/// rib, carries more than two spars, or its root is not a swept-back root
/// whose truncated ribs end on the root rib or the rear spar in the order
/// they leave the front spar (the deck then keeps its single-strip root).
pub(super) fn plan(
    stations: &[RibStation],
    nodes: &NodeMap,
    regions: &RibRegions,
) -> Option<RootPlan> {
    let root = stations.first()?;
    if root.j_spars.len() != 2 || regions.transition_ribs.is_empty() {
        return None;
    }
    let skin_start = *regions.skin_ribs_main.first()?;
    if regions.skin_ribs.get(1) != Some(&skin_start)
        || regions.transition_ribs.iter().any(|&rib| rib >= skin_start)
    {
        return None;
    }
    let (front0, rear0) = super::boxed::spar_window(root)?;
    let root_line: [Vec<Point>; 2] = [0, 1].map(|s| {
        (front0..=rear0)
            .map(|j| station_point(root, nodes, 0, j, s))
            .collect()
    });
    let root_upper: Vec<[f64; 3]> = root_line[0].iter().map(|p| p.xyz()).collect();
    let root_lower: Vec<[f64; 3]> = root_line[1].iter().map(|p| p.xyz()).collect();

    let mut ribs: Vec<usize> = regions.transition_ribs.clone();
    ribs.push(skin_start);

    // Each rib from its front spar to its own end: the rear spar when it
    // reaches it, otherwise its root-plane cut.
    let mut lines: Vec<Line> = Vec::new();
    let mut on_rear: Vec<bool> = Vec::new();
    for &rib in &ribs {
        let station = &stations[rib];
        let front = *station.j_spars.first()?;
        let rear = *station.j_spars.last()?;
        if front < 0 {
            return None;
        }
        let reaches = rear > front;
        let end = if reaches {
            rear as usize
        } else {
            station.extrados.len().checked_sub(1)?
        };
        let front = front as usize;
        if end <= front || end >= station.intrados.len() {
            return None;
        }
        lines.push(Line {
            points: [0, 1].map(|s| {
                (front..=end)
                    .map(|j| station_point(station, nodes, rib, j, s))
                    .collect()
            }),
            rib: Some(rib),
        });
        on_rear.push(reaches);
    }

    // The rear spar in plan view from the root out, through the ribs that
    // reach it.
    let mut rear_chain: Vec<[f64; 3]> = vec![*root_upper.last()?];
    for (line, &reaches) in lines.iter().zip(&on_rear) {
        if reaches {
            rear_chain.push(line.points[0].last()?.xyz());
        }
    }
    if rear_chain.windows(2).any(|w| w[1][1] <= w[0][1]) {
        return None;
    }

    // A truncated rib either crosses the rear spar just outboard of the root
    // (its spar point lost to the root cut), or ends on the root rib.
    let mut root_cut_x: Vec<(f64, usize)> = Vec::new();
    for (index, line) in lines.iter_mut().enumerate() {
        if on_rear[index] {
            continue;
        }
        let upper: Vec<[f64; 3]> = line.points[0].iter().map(|p| p.xyz()).collect();
        let crossing = (0..upper.len() - 1).find_map(|k| {
            (0..rear_chain.len().saturating_sub(1)).find_map(|c| {
                crossing_xy(upper[k], upper[k + 1], rear_chain[c], rear_chain[c + 1])
                    .map(|(t, _)| (k, t))
            })
        });
        if let Some((k, t)) = crossing {
            // The rib ends on the spar: the point is on the rib, in plan and
            // in height, and the spar is straight between its grids.
            for s in 0..2 {
                let (a, b) = (line.points[s][k].xyz(), line.points[s][k + 1].xyz());
                line.points[s].truncate(k + 1);
                if t > 1e-6 {
                    line.points[s].push(Point::New(lerp(a, b, t)));
                }
            }
            if line.points[0].len() < 2 {
                return None;
            }
            on_rear[index] = true;
            continue;
        }
        // The cut ends on the root plane: its aft point is put on the root
        // rib itself, on each surface, rather than on the rib's pinched cut.
        // A cut within a quarter of a root-rib segment of one of its grids
        // ends on that grid: the region's rows run to every root-rib grid,
        // so a cut beside one would leave a row a sliver wide, which the
        // lower surface (offset in x by the twist) can fold over.
        let end = *upper.last()?;
        if (end[1] - root.y_station).abs() > 0.05 * dist(upper[0], end) {
            return None;
        }
        let k = (0..root_upper.len() - 1)
            .find(|&k| end[0] >= root_upper[k][0] && end[0] <= root_upper[k + 1][0])?;
        let t = (end[0] - root_upper[k][0]) / (root_upper[k + 1][0] - root_upper[k][0]).max(1e-12);
        for s in 0..2 {
            let points = if s == 0 { &root_upper } else { &root_lower };
            let cut = if t < SNAP_TO_ROOT_GRID {
                root_line[s][k]
            } else if t > 1.0 - SNAP_TO_ROOT_GRID {
                root_line[s][k + 1]
            } else {
                Point::New(lerp(points[k], points[k + 1], t))
            };
            let last = line.points[s].len() - 1;
            line.points[s][last] = cut;
        }
        root_cut_x.push((end[0], index));
    }

    // The lines must reach the aft boundary in the order they leave the
    // front spar: root cuts front to back, then the rear spar root outward.
    root_cut_x.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut rear_y: Vec<(f64, usize)> = (0..lines.len())
        .filter(|&i| on_rear[i])
        .map(|i| (lines[i].points[0].last().map_or(0.0, |p| p.xyz()[1]), i))
        .collect();
    rear_y.sort_by(|a, b| a.0.total_cmp(&b.0));
    let order: Vec<usize> = root_cut_x.iter().chain(&rear_y).map(|&(_, i)| i).collect();
    if order.iter().copied().ne(0..lines.len()) {
        return None;
    }

    let target = lines
        .iter()
        .map(|line| line.points[0].len())
        .max()?
        .max(MIN_CHORD_ELEMENTS + 1);
    let truncated = lines.len() - 1;
    for line in lines.iter_mut().take(truncated) {
        refine_line(line, target);
    }
    Some(RootPlan {
        root: root_line,
        lines,
        skin_start,
        transition_ribs: regions.transition_ribs.clone(),
    })
}

/// One aft-boundary point of the region on both surfaces, with its position
/// along the boundary measured on the upper surface.
#[derive(Debug, Clone, Copy)]
struct AftPoint {
    points: [Point; 2],
    s: f64,
}

fn same(a: Point, b: Point) -> bool {
    dist(a.xyz(), b.xyz()) < 1e-9
}

/// The aft boundary: the root rib from the front spar to the rear spar, with
/// the root cuts in it, then the rear-spar ends of the lines, root outward.
/// Returns it with the position of the root rear-spar corner.
fn aft_boundary(plan: &RootPlan) -> (Vec<AftPoint>, f64) {
    let mut points: Vec<[Point; 2]> = (0..plan.root[0].len())
        .map(|k| [plan.root[0][k], plan.root[1][k]])
        .collect();
    let mut rear: Vec<[Point; 2]> = Vec::new();
    for line in &plan.lines {
        let (Some(&upper), Some(&lower)) = (line.points[0].last(), line.points[1].last()) else {
            continue;
        };
        if points.iter().any(|p| same(p[0], upper)) {
            continue;
        }
        let x = upper.xyz()[0];
        let between = points.windows(2).position(|w| {
            let (a, b) = (w[0][0].xyz(), w[1][0].xyz());
            (a[1] - upper.xyz()[1]).abs() < 1e-6 && x > a[0] && x < b[0]
        });
        match between {
            Some(k) if (plan.root[0][0].xyz()[1] - upper.xyz()[1]).abs() < 1e-6 => {
                points.insert(k + 1, [upper, lower]);
            }
            _ => rear.push([upper, lower]),
        }
    }
    let corner = points.len() - 1;
    points.extend(rear);
    let mut aft = Vec::with_capacity(points.len());
    let mut s = 0.0;
    for (k, p) in points.iter().enumerate() {
        if k > 0 {
            s += dist(points[k - 1][0].xyz(), p[0].xyz());
        }
        aft.push(AftPoint { points: *p, s });
    }
    let corner_s = aft[corner].s;
    (aft, corner_s)
}

/// The aft boundary at position `s` on `surface`, and the point there when
/// the boundary already has one.
fn aft_at(aft: &[AftPoint], s: f64, surface: usize) -> Point {
    if let Some(p) = aft.iter().find(|p| (p.s - s).abs() < 1e-12) {
        return p.points[surface];
    }
    for w in aft.windows(2) {
        if s <= w[1].s {
            let t = ((s - w[0].s) / (w[1].s - w[0].s).max(1e-12)).clamp(0.0, 1.0);
            return Point::New(lerp(
                w[0].points[surface].xyz(),
                w[1].points[surface].xyz(),
                t,
            ));
        }
    }
    aft.last()
        .map_or(Point::New([0.0; 3]), |p| p.points[surface])
}

/// The shape of a line off its own chord at arc position `u`, scaled to a
/// chord of `length`.
fn shape_offset(points: &[[f64; 3]], u: f64, length: f64) -> [f64; 3] {
    if points.len() < 2 {
        return [0.0; 3];
    }
    let (a, b) = (points[0], points[points.len() - 1]);
    let chord = dist(a, b);
    if chord <= 0.0 {
        return [0.0; 3];
    }
    let params = arc_params(points);
    scale(
        sub(at_param(points, &params, u), lerp(a, b, u)),
        length / chord,
    )
}

/// Write the region: its grids, its cover shells (one cover strip per row of
/// each surface), and the webs of the truncated ribs.
pub(super) fn build(
    deck: &mut Deck,
    eid: &mut i64,
    plan: &RootPlan,
    surface: &dyn Fn(f64, f64, usize) -> f64,
) -> RootMesh {
    let (aft, corner_s) = aft_boundary(plan);
    let end_s = |line: &[Vec<Point>; 2]| {
        line[0]
            .last()
            .and_then(|&end| aft.iter().find(|p| same(p.points[0], end)))
            .map_or(0.0, |p| p.s)
    };

    // Rows from the front-spar root corner (a single grid) outboard: each
    // truncated rib, and the intermediate rows that split each bay.
    let mut rows: Vec<([Vec<Point>; 2], f64, Option<usize>)> =
        vec![([vec![plan.root[0][0]], vec![plan.root[1][0]]], 0.0, None)];
    for line in &plan.lines {
        let (previous, s0, _) = rows.last().cloned().unwrap_or_default();
        let s1 = end_s(&line.points);
        let mut stops: Vec<f64> = aft
            .iter()
            .map(|p| p.s)
            .filter(|&s| s > s0 + 1e-12 && s < s1 - 1e-12)
            .collect();
        while stops.len() + 1 < MIN_ROWS_PER_BAY {
            let mut bounds = vec![s0];
            bounds.extend(&stops);
            bounds.push(s1);
            let Some(k) = (0..bounds.len() - 1).max_by(|&a, &b| {
                (bounds[a + 1] - bounds[a]).total_cmp(&(bounds[b + 1] - bounds[b]))
            }) else {
                break;
            };
            stops.insert(k, 0.5 * (bounds[k] + bounds[k + 1]));
        }
        let u: Vec<f64> = arc_params(&line.points[0].iter().map(|p| p.xyz()).collect::<Vec<_>>());
        for &stop in &stops {
            let f = (stop - s0) / (s1 - s0).max(1e-12);
            let row: [Vec<Point>; 2] = [0, 1].map(|s| {
                let start = lerp(previous[s][0].xyz(), line.points[s][0].xyz(), f);
                let finish = aft_at(&aft, stop, s);
                let length = dist(start, finish.xyz());
                let inner: Vec<[f64; 3]> = previous[s].iter().map(|p| p.xyz()).collect();
                let outer: Vec<[f64; 3]> = line.points[s].iter().map(|p| p.xyz()).collect();
                let mut points = vec![Point::New(start)];
                for &uk in &u[1..u.len() - 1] {
                    let shape = add(
                        scale(shape_offset(&inner, uk, length), 1.0 - f),
                        scale(shape_offset(&outer, uk, length), f),
                    );
                    // Plan position from the blended lines; height on the
                    // loft itself, so the shells follow the cover surface.
                    let [x, y, _] = add(lerp(start, finish.xyz(), uk), shape);
                    points.push(Point::New([x, y, surface(x, y, s)]));
                }
                points.push(finish);
                points
            });
            rows.push((row, stop, None));
        }
        rows.push((line.points.clone(), s1, line.rib));
    }

    // Every point the region adds goes on the loft, the root cuts on the
    // root rib included (onto its section rather than the chord between two
    // of its grids).
    for (row, _, _) in &mut rows {
        for (s, points) in row.iter_mut().enumerate() {
            for point in points.iter_mut() {
                if let Point::New([x, y, _]) = *point {
                    *point = Point::New([x, y, surface(x, y, s)]);
                }
            }
        }
    }

    // Grids, numbered after every existing one. A new point shared by two
    // rows (a root cut or spar crossing that ends a rib) is one grid.
    let mut next_nid = deck.grids.iter().map(|g| g.nid).max().unwrap_or(0) + 1;
    let mut created: Vec<([f64; 3], i64)> = Vec::new();
    let mut grid_of = |deck: &mut Deck, point: Point| -> i64 {
        match point {
            Point::Grid(nid, _) => nid,
            Point::New(xyz) => {
                if let Some(&(_, nid)) = created.iter().find(|(c, _)| dist(*c, xyz) < 1e-9) {
                    return nid;
                }
                let nid = deck.add_grid(next_nid, xyz);
                next_nid += 1;
                created.push((xyz, nid));
                nid
            }
        }
    };
    let grids: Vec<[Vec<i64>; 2]> = rows
        .iter()
        .map(|(row, _, _)| [0, 1].map(|s| row[s].iter().map(|&p| grid_of(deck, p)).collect()))
        .collect();

    for pair in grids.windows(2) {
        for s in 0..2 {
            let first = *eid;
            let params = |line: &[i64]| {
                arc_params(
                    &line
                        .iter()
                        .map(|&n| deck.grid_xyz(n).unwrap_or([0.0; 3]))
                        .collect::<Vec<_>>(),
                )
            };
            let (ua, ub) = (params(&pair[0][s]), params(&pair[1][s]));
            strip(deck, eid, &pair[0][s], &ua, &pair[1][s], &ub, s == 1);
            if *eid > first {
                deck.cover_strips.push(CoverStrip {
                    upper: s == 0,
                    shells: (first..*eid).collect(),
                });
            }
        }
    }

    let mut mesh = RootMesh::default();
    for ((_, s_end, rib), line) in rows.iter().zip(&grids) {
        for s in 0..2 {
            mesh.front[s].push(line[s][0]);
            let Some(&last) = line[s].last() else {
                continue;
            };
            if *s_end >= corner_s - 1e-12 {
                mesh.rear[s].push(last);
            }
            if *s_end <= corner_s + 1e-12 {
                mesh.root_plane.push(last);
            }
        }
        // The truncated ribs' webs on their refined lines.
        if let Some(rib) = rib {
            if *rib != plan.skin_start {
                let pid = if plan.transition_ribs.contains(rib) {
                    PID_SEC_RIB
                } else {
                    PID_MAIN_RIB
                };
                let (upper, lower) = (&line[0], &line[1]);
                for k in 0..upper.len().min(lower.len()).saturating_sub(1) {
                    add_quad(
                        deck,
                        eid,
                        [upper[k], upper[k + 1], lower[k + 1], lower[k]],
                        pid,
                    );
                }
            }
        }
    }
    // The rear spar starts at the root rib's rear corner.
    let corner = aft
        .iter()
        .find(|p| (p.s - corner_s).abs() < 1e-12)
        .map(|p| p.points);
    if let Some(corner) = corner {
        for s in 0..2 {
            let nid = grid_of(deck, corner[s]);
            if mesh.rear[s].first() != Some(&nid) {
                mesh.rear[s].insert(0, nid);
            }
        }
    }
    for p in aft.iter().filter(|p| p.s <= corner_s + 1e-12) {
        for s in 0..2 {
            mesh.root_plane.push(grid_of(deck, p.points[s]));
        }
    }
    mesh.root_plane.sort_unstable();
    mesh.root_plane.dedup();
    mesh
}

/// Shells between two lines: quadrilaterals where the lines have the same
/// number of grids, otherwise triangles advanced by arc position, a fan when
/// the inboard line is a single grid. `lower` reverses the winding so the
/// lower cover keeps an outward normal, as the rib-to-rib strips do.
fn strip(
    deck: &mut Deck,
    eid: &mut i64,
    a: &[i64],
    ua: &[f64],
    b: &[i64],
    ub: &[f64],
    lower: bool,
) {
    if a.len() == b.len() {
        for k in 0..a.len().saturating_sub(1) {
            let c = [a[k], b[k], b[k + 1], a[k + 1]];
            let c = if lower { [c[0], c[3], c[2], c[1]] } else { c };
            add_quad(deck, eid, c, PID_SKIN);
        }
        return;
    }
    let (mut i, mut j) = (0, 0);
    while i + 1 < a.len() || j + 1 < b.len() {
        let advance_a = j + 1 == b.len() || (i + 1 < a.len() && ua[i + 1] <= ub[j + 1]);
        let c = if advance_a {
            i += 1;
            [a[i - 1], b[j], a[i]]
        } else {
            j += 1;
            [a[i], b[j - 1], b[j]]
        };
        let c = if lower { [c[2], c[1], c[0]] } else { c };
        add_tria(deck, eid, c, PID_SKIN);
    }
}
