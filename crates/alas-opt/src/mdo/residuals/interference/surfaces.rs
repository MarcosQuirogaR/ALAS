// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Surface-level clearance measurements of the interference residuals: the
//! lowest points of the wing, engine bodies and tail after the ground pose,
//! the nacelle-to-wing gap and the tail-root attachment. See the parent
//! module for the frame, units and the sources.

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;

use super::{is_engine_body, section_at, section_radius, GroundPose, SECTION_POINTS};

/// Lowest engine-body point heights, m, after the ground pose; `rise_at`
/// gives the flight-shape height to remove at lateral station `|y|`.
pub(crate) fn nacelle_clearance_m(
    plane: &Airplane,
    pose: &GroundPose,
    rise_at: &dyn Fn(f64) -> f64,
) -> Option<f64> {
    let mut lowest = f64::INFINITY;
    for body in plane.fuselages.iter().skip(1).filter(|b| is_engine_body(b)) {
        for section in &body.xsecs {
            let n = if section.shape.is_finite() && section.shape > 0.0 {
                section.shape
            } else {
                2.0
            };
            let (a, b) = (0.5 * section.width, 0.5 * section.height);
            for k in 0..SECTION_POINTS {
                let angle = std::f64::consts::TAU * k as f64 / SECTION_POINTS as f64;
                let (s, c) = angle.sin_cos();
                let y = section.xyz_c[1] + a * c.signum() * c.abs().powf(2.0 / n);
                let z = section.xyz_c[2] + b * s.signum() * s.abs().powf(2.0 / n)
                    - rise_at(section.xyz_c[1].abs());
                lowest = lowest.min(pose.height_m([section.xyz_c[0], y, z]));
            }
        }
    }
    lowest.is_finite().then_some(lowest)
}

/// Lowest point height of one lifting surface, m, after the ground pose: the
/// lower-surface coordinates of every section's airfoil, twisted about the
/// leading edge (positive twist is nose-up). `rise_at` gives the flight-shape
/// height to remove at `|y|`. No thickness margin is added or assumed.
pub(crate) fn surface_clearance_m(
    wing: &Wing,
    pose: &GroundPose,
    rise_at: &dyn Fn(f64) -> f64,
) -> Option<f64> {
    let mut lowest = f64::INFINITY;
    for section in &wing.xsecs {
        let twist = section.twist.to_radians();
        let (sin_t, cos_t) = twist.sin_cos();
        let rise = rise_at(section.xyz_le[1].abs());
        for &(x_c, z_c) in section.airfoil.lower_coordinates() {
            let x = section.xyz_le[0] + section.chord * (x_c * cos_t + z_c * sin_t);
            let z = section.xyz_le[2] + section.chord * (z_c * cos_t - x_c * sin_t) - rise;
            lowest = lowest.min(pose.height_m([x, section.xyz_le[1], z]));
        }
    }
    lowest.is_finite().then_some(lowest)
}

/// Lower-surface ordinate `z/c` of an airfoil at chordwise station `x_c`.
fn lower_ordinate(airfoil: &alas_geom::aircraft::airfoil::Airfoil, x_c: f64) -> f64 {
    surface_ordinate(airfoil.lower_coordinates(), x_c)
}

/// Upper-surface ordinate `z/c` of an airfoil at chordwise station `x_c`.
fn upper_ordinate(airfoil: &alas_geom::aircraft::airfoil::Airfoil, x_c: f64) -> f64 {
    surface_ordinate(airfoil.upper_coordinates(), x_c)
}

/// Ordinate of one surface's `(x/c, z/c)` points at `x_c`, linear between
/// the bracketing points, in either point order.
fn surface_ordinate(points: &[(f64, f64)], x_c: f64) -> f64 {
    points
        .windows(2)
        .find(|w| (w[0].0 - x_c) * (w[1].0 - x_c) <= 0.0)
        .map_or(0.0, |w| {
            let span = w[1].0 - w[0].0;
            if span.abs() < 1.0e-12 {
                w[0].1
            } else {
                w[0].1 + (w[1].1 - w[0].1) * (x_c - w[0].0) / span
            }
        })
}

/// Height of the main wing's lower surface at plan station `(x, |y|)`, m
/// (flight-shape rise removed), or `None` when the point is outside the
/// wing's plan at that span station. Chord, leading edge, height, twist and
/// the lower ordinate of both bounding sections' airfoils are interpolated
/// linearly in span; twist rotates the section about its leading edge
/// (positive nose-up).
pub(super) fn wing_lower_z_m(
    wing: &Wing,
    x_m: f64,
    y_m: f64,
    rise_at: &dyn Fn(f64) -> f64,
) -> Option<f64> {
    wing_surface_z_m(wing, x_m, y_m, rise_at, lower_ordinate)
}

/// Height of the main wing's upper surface at plan station `(x, |y|)`, m,
/// as [`wing_lower_z_m`] for the lower one.
fn wing_upper_z_m(wing: &Wing, x_m: f64, y_m: f64, rise_at: &dyn Fn(f64) -> f64) -> Option<f64> {
    wing_surface_z_m(wing, x_m, y_m, rise_at, upper_ordinate)
}

fn wing_surface_z_m(
    wing: &Wing,
    x_m: f64,
    y_m: f64,
    rise_at: &dyn Fn(f64) -> f64,
    ordinate_of: fn(&alas_geom::aircraft::airfoil::Airfoil, f64) -> f64,
) -> Option<f64> {
    for pair in wing.xsecs.windows(2) {
        let (inboard, outboard) = (&pair[0], &pair[1]);
        let (y0, y1) = (inboard.xyz_le[1].abs(), outboard.xyz_le[1].abs());
        if !(y0..=y1).contains(&y_m) || y1 <= y0 {
            continue;
        }
        let t = (y_m - y0) / (y1 - y0);
        let lerp = |p: f64, q: f64| p + t * (q - p);
        let leading = lerp(inboard.xyz_le[0], outboard.xyz_le[0]);
        let chord = lerp(inboard.chord, outboard.chord);
        let z_le = lerp(inboard.xyz_le[2], outboard.xyz_le[2]);
        let twist = lerp(inboard.twist, outboard.twist).to_radians();
        let x_c = (x_m - leading) / chord;
        if !(0.0..=1.0).contains(&x_c) {
            return None;
        }
        let ordinate = lerp(
            ordinate_of(&inboard.airfoil, x_c),
            ordinate_of(&outboard.airfoil, x_c),
        );
        let z = z_le + chord * (ordinate * twist.cos() - x_c * twist.sin());
        return Some(z - rise_at(y_m));
    }
    None
}

/// Stations sampled along each engine body for the wing gap.
const GAP_SAMPLES: usize = 41;

/// Smallest vertical gap between the engine bodies and the main wing over
/// the plan area they share, m (negative is penetration of the wing by the
/// nacelle); `None` when no engine body shares the wing's plan. The gap is
/// the separation of the body section's and the wing section's vertical
/// extents, so an underwing nacelle is measured from its top to the lower
/// surface and a fuselage-mounted nacelle above the wing root from the upper
/// surface to its bottom; an overlap reads its smaller penetration, whatever
/// the axis height. `nacelle_rise_at` is the flight-shape rise the
/// nacelles carry. Sampled on a fixed grid along each body, not only at its
/// stations.
pub(crate) fn nacelle_wing_gap_m(
    plane: &Airplane,
    wing_rise_at: &dyn Fn(f64) -> f64,
    nacelle_rise_at: &dyn Fn(f64) -> f64,
) -> Option<f64> {
    let wing = plane.wings.first()?;
    let mut gap = f64::INFINITY;
    for body in plane.fuselages.iter().skip(1).filter(|b| is_engine_body(b)) {
        let (Some(first), Some(last)) = (body.xsecs.first(), body.xsecs.last()) else {
            continue;
        };
        for k in 0..GAP_SAMPLES {
            let x = first.xyz_c[0]
                + (last.xyz_c[0] - first.xyz_c[0]) * k as f64 / (GAP_SAMPLES - 1) as f64;
            let Some(section) = section_at(body, x) else {
                continue;
            };
            let y = section.xyz_c[1].abs();
            let (Some(lower), Some(upper)) = (
                wing_lower_z_m(wing, x, y, wing_rise_at),
                wing_upper_z_m(wing, x, y, wing_rise_at),
            ) else {
                continue;
            };
            let axis = section.xyz_c[2] - nacelle_rise_at(y);
            let half_height = 0.5 * section.height;
            // Separation of the two vertical intervals: positive when the
            // body is wholly below (underwing) or wholly above (over the
            // root, fuselage-mounted), minus the smaller penetration when
            // they overlap.
            gap = gap.min((lower - (axis + half_height)).max(axis - half_height - upper));
        }
    }
    gap.is_finite().then_some(gap)
}

/// Lowest point of the main wing and its tip devices, m (see
/// [`surface_clearance_m`]).
pub(crate) fn wing_clearance_m(
    plane: &Airplane,
    pose: &GroundPose,
    rise_at: &dyn Fn(f64) -> f64,
) -> Option<f64> {
    plane
        .wings
        .iter()
        .enumerate()
        .filter(|(index, wing)| *index == 0 || wing.name.contains("Wingtip"))
        .filter_map(|(_, wing)| surface_clearance_m(wing, pose, rise_at))
        .reduce(f64::min)
}

/// Lowest point of the horizontal tail, m (see [`surface_clearance_m`]); the
/// tail carries no flight-shape rise.
pub(crate) fn tail_clearance_m(plane: &Airplane, pose: &GroundPose) -> Option<f64> {
    plane
        .wings
        .iter()
        .skip(1)
        .find(|wing| wing.name.contains("Horizontal"))
        .and_then(|wing| surface_clearance_m(wing, pose, &|_| 0.0))
}

/// How far the horizontal-tail root lies outside the structure that carries
/// it, m: `0` when the mid-chord point of the root section is inside the
/// fuselage or inside the fin (a T-tail root sits on the fin), else the
/// distance to the nearer of the two. A tail whose root hangs in free air is
/// not an aeroplane.
pub(crate) fn tail_root_detachment_m(plane: &Airplane) -> Option<f64> {
    let root = plane
        .wings
        .iter()
        .skip(1)
        .find(|wing| wing.name.contains("Horizontal"))?
        .xsecs
        .first()?;
    let point = [
        root.xyz_le[0] + 0.5 * root.chord,
        root.xyz_le[1],
        root.xyz_le[2],
    ];
    let mut best = f64::INFINITY;
    if let Some(section) = section_at(plane.fuselages.first()?, point[0]) {
        let dy = point[1] - section.xyz_c[1];
        let dz = point[2] - section.xyz_c[2];
        let distance = dy.hypot(dz);
        let radius = if distance > 1.0e-9 {
            section_radius(&section, dy / distance, dz / distance)
        } else {
            0.0
        };
        best = best.min((distance - radius).max(0.0));
    }
    if let Some(fin) = plane
        .wings
        .iter()
        .skip(1)
        .find(|wing| wing.name.contains("Vertical"))
    {
        for pair in fin.xsecs.windows(2) {
            let (low, high) = (&pair[0], &pair[1]);
            let (z0, z1) = (low.xyz_le[2], high.xyz_le[2]);
            if z0.is_nan() || z1.is_nan() || z1 <= z0 {
                continue;
            }
            let t = ((point[2] - z0) / (z1 - z0)).clamp(0.0, 1.0);
            let z_gap = (z0 - point[2]).max(point[2] - z1).max(0.0);
            let lead = low.xyz_le[0] + t * (high.xyz_le[0] - low.xyz_le[0]);
            let chord = low.chord + t * (high.chord - low.chord);
            let x_gap = (lead - point[0]).max(point[0] - (lead + chord)).max(0.0);
            best = best.min(x_gap.hypot(z_gap));
        }
    }
    best.is_finite().then_some(best)
}

/// Lowest height of every propeller disc above the ground, m: the disc is
/// centred on the first section of each engine body (the propeller plane sits
/// at the nacelle front) and its lowest blade tip hangs one radius below that
/// centre. `radius_m` is half the installed propeller diameter. The pose is
/// whatever the caller passes; CS 25.925(a) reads it level and unrolled.
pub(crate) fn propeller_clearance_m(
    plane: &Airplane,
    pose: &GroundPose,
    radius_m: f64,
) -> Option<f64> {
    plane
        .fuselages
        .iter()
        .skip(1)
        .filter(|body| is_engine_body(body))
        .filter_map(|body| body.xsecs.first())
        .map(|front| pose.height_m([front.xyz_c[0], front.xyz_c[1], front.xyz_c[2] - radius_m]))
        .reduce(f64::min)
}
