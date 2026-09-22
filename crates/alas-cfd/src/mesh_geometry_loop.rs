// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Loop-level checks of the geometry audit: ordering, duplicates,
//! intersections, edge classification and section metrics.

use super::{
    AirfoilGeometryAudit, GeometryIssueCode, IssueSeverity, SectionMetrics, TrailingEdgeMeshing,
    TrailingEdgeTreatment, Winding, MAX_THICKNESS_RATIO, MAX_TRAILING_EDGE_GAP, METRIC_STATIONS,
    MIN_THICKNESS_RATIO, NEGLIGIBLE_SEGMENT,
};
use crate::geometry::segments_cross;
use crate::mesh::generation::{bounds, distance_sq, signed_area, vertical_gap};
use crate::mesh::{AirfoilTopology, EdgeKind, EDGE_X_TOLERANCE, MAX_EDGE_POINTS};

pub(super) fn audit_loop(
    audit: &mut AirfoilGeometryAudit,
    points: &[(f64, f64)],
    chord_m: Option<f64>,
) {
    use GeometryIssueCode as Code;
    use IssueSeverity::{Error, Warning};
    let n = points.len();
    let (min_x, max_x, _, _) = bounds(points);
    let span_x = max_x - min_x;
    if min_x < -0.05 || max_x > 1.05 || span_x < 0.5 {
        audit.push(
            Error,
            Code::NotNormalized,
            format!("x spans {min_x:.4}..{max_x:.4}; a unit-chord section spans 0..1"),
            "normalise the section to a unit chord with the leading edge near the origin",
            Vec::new(),
        );
    }
    let mut duplicates = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            if distance_sq(points[i], points[j]) < 1.0e-20 {
                duplicates.push(i);
                duplicates.push(j);
            }
        }
    }
    if !duplicates.is_empty() {
        audit.push(
            Error,
            Code::DuplicatePoint,
            format!("{} coincident coordinate pair(s)", duplicates.len() / 2),
            "remove the repeated coordinates; only a final closing copy of the first point is accepted",
            duplicates.clone(),
        );
    }
    let mut negligible = Vec::new();
    let (mut min_segment, mut max_segment, mut perimeter) = (f64::INFINITY, 0.0_f64, 0.0_f64);
    for i in 0..n {
        let length = distance_sq(points[i], points[(i + 1) % n]).sqrt();
        perimeter += length;
        min_segment = min_segment.min(length);
        max_segment = max_segment.max(length);
        if length > 0.0 && length < NEGLIGIBLE_SEGMENT {
            negligible.push(i);
        }
    }
    if !negligible.is_empty() {
        audit.push(
            Warning,
            Code::NegligibleSegment,
            format!(
                "{} segment(s) shorter than {NEGLIGIBLE_SEGMENT:e} c",
                negligible.len()
            ),
            "the mesher may collapse them; merge near-coincident coordinates if meshing fails",
            negligible,
        );
    }
    if duplicates.is_empty() {
        'outer: for i in 0..n {
            let (a, b) = (points[i], points[(i + 1) % n]);
            for j in (i + 1)..n {
                if (i + 1) % n == j || i == (j + 1) % n {
                    continue;
                }
                if segments_cross(a, b, points[j], points[(j + 1) % n]) {
                    audit.push(
                        Error,
                        Code::SelfIntersection,
                        format!("segments {i} and {j} cross"),
                        "the loop must be simple; check the ordering and remove crossing points",
                        vec![i, j],
                    );
                    break 'outer;
                }
            }
        }
    }
    let area = signed_area(points);
    if !area.is_finite() || area.abs() <= 1.0e-12 * span_x * span_x {
        audit.push(
            Error,
            Code::DegenerateArea,
            "the loop encloses no measurable area".to_owned(),
            "the section is collapsed to a line; use a resolved coordinate set",
            Vec::new(),
        );
    } else {
        audit.winding = Some(if area > 0.0 {
            Winding::CounterClockwise
        } else {
            Winding::Clockwise
        });
    }
    let tolerance = EDGE_X_TOLERANCE.max(1.0e-12 * span_x);
    let le_index = points
        .iter()
        .enumerate()
        .min_by(|left, right| left.1 .0.total_cmp(&right.1 .0))
        .map_or(0, |(index, _)| index);
    let mut fault = None;
    // The loop starts at the trailing edge; a sharp section returns to that
    // same point through the implicit closing edge, so the last point is
    // only required to keep x non-decreasing on the second surface.
    if (points[0].0 - max_x).abs() > tolerance {
        fault = Some((0, "the loop must start at the trailing edge"));
    }
    for i in 1..=le_index {
        if fault.is_none() && points[i].0 > points[i - 1].0 + tolerance {
            fault = Some((
                i,
                "x must decrease monotonically from the trailing edge to the leading edge",
            ));
        }
    }
    for i in (le_index + 1)..n {
        if fault.is_none() && points[i].0 < points[i - 1].0 - tolerance {
            fault = Some((
                i,
                "x must increase monotonically from the leading edge back to the trailing edge",
            ));
        }
    }
    if let Some((index, message)) = fault {
        audit.push(
            Error,
            Code::NotSingleLoop,
            format!("{message} (index {index})"),
            "order the coordinates as trailing edge, upper surface, leading edge, lower surface, trailing edge (Selig order); multi-element sections are unsupported",
            vec![index],
        );
    }
    let first_sorted = sorted_by_x(&points[..=le_index]);
    let second_sorted = sorted_by_x(&points[le_index..]);
    let upper_first = interpolate_y(&first_sorted, 0.5) >= interpolate_y(&second_sorted, 0.5);
    audit.upper_surface_first = Some(upper_first);
    if !upper_first {
        audit.push(
            Warning,
            Code::LowerSurfaceFirst,
            "the lower surface is listed before the upper surface".to_owned(),
            "accepted; the database convention lists the upper surface first",
            Vec::new(),
        );
    }
    let leading = extreme_ys(points, min_x, tolerance);
    let trailing = extreme_ys(points, max_x, tolerance);
    if leading.len() > MAX_EDGE_POINTS {
        audit.push(
            Error,
            Code::LeadingEdgeMultiplePoints,
            format!("{} points share the leading-edge x", leading.len()),
            "at most two points may lie at the leading-edge extreme",
            Vec::new(),
        );
    }
    if trailing.len() > MAX_EDGE_POINTS {
        audit.push(
            Error,
            Code::TrailingEdgeMultiplePoints,
            format!("{} points share the trailing-edge x", trailing.len()),
            "at most two points may lie at the trailing-edge extreme",
            Vec::new(),
        );
    }
    let leading_gap = vertical_gap(&leading);
    let trailing_gap = vertical_gap(&trailing);
    let kind = |gap: f64| {
        if gap <= 1.0e-8 {
            EdgeKind::Sharp
        } else {
            EdgeKind::Blunt
        }
    };
    audit.topology = Some(AirfoilTopology {
        leading_edge: kind(leading_gap),
        trailing_edge: kind(trailing_gap),
        leading_edge_points: leading.len(),
        trailing_edge_points: trailing.len(),
    });
    if trailing_gap > MAX_TRAILING_EDGE_GAP {
        audit.push(
            Error,
            Code::TrailingEdgeGapTooLarge,
            format!("blunt base of {trailing_gap:.4} c exceeds {MAX_TRAILING_EDGE_GAP} c"),
            "the single-section template resolves thin bases only; a bluff base needs its own template",
            Vec::new(),
        );
    }
    audit.trailing_edge = Some(TrailingEdgeTreatment {
        kind: kind(trailing_gap),
        gap_chord: trailing_gap,
        gap_m: chord_m.map(|chord| trailing_gap * chord),
        distinct_points: trailing.len(),
        closure: audit.closure,
        meshing: if kind(trailing_gap) == EdgeKind::Sharp {
            TrailingEdgeMeshing::FanAtSharpEdge
        } else {
            TrailingEdgeMeshing::ResolvedBluntFace
        },
    });
    let leading_edge = points[le_index];
    if leading_edge.0.abs() > 0.02 || leading_edge.1.abs() > 0.05 {
        audit.push(
            Warning,
            Code::LeadingEdgeOffOrigin,
            format!("leading edge at ({:.4}, {:.4})", leading_edge.0, leading_edge.1),
            "the chord frame and the x/c = 0.25 moment reference assume the leading edge at the origin",
            vec![le_index],
        );
    }
    let (upper, lower) = if upper_first {
        (&first_sorted, &second_sorted)
    } else {
        (&second_sorted, &first_sorted)
    };
    let (mut t_max, mut t_x, mut c_max, mut c_x) = (0.0_f64, min_x, 0.0_f64, min_x);
    for station in 1..METRIC_STATIONS {
        let x = min_x + span_x * station as f64 / METRIC_STATIONS as f64;
        let (yu, yl) = (interpolate_y(upper, x), interpolate_y(lower, x));
        let (thickness, camber) = (yu - yl, 0.5 * (yu + yl));
        if thickness > t_max {
            (t_max, t_x) = (thickness, x);
        }
        if camber.abs() > c_max.abs() {
            (c_max, c_x) = (camber, x);
        }
    }
    let thickness_ratio = t_max / span_x;
    if !(MIN_THICKNESS_RATIO..=MAX_THICKNESS_RATIO).contains(&thickness_ratio) {
        audit.push(
            Error,
            Code::ThicknessOutOfRange,
            format!(
                "maximum thickness {thickness_ratio:.4} c is outside {MIN_THICKNESS_RATIO}..{MAX_THICKNESS_RATIO} c"
            ),
            "the template covers conventional sections; plates and bluff bodies need their own",
            Vec::new(),
        );
    }
    audit.metrics = Some(SectionMetrics {
        max_thickness_ratio: thickness_ratio,
        max_thickness_x: t_x,
        max_camber_ratio: c_max / span_x,
        max_camber_x: c_x,
        leading_edge,
        trailing_edge_x: max_x,
        min_segment_chord: min_segment,
        max_segment_chord: max_segment,
        perimeter_chord: perimeter,
        signed_area_chord2: area,
        first_surface_points: le_index + 1,
        second_surface_points: n - le_index,
    });
}

fn extreme_ys(points: &[(f64, f64)], x_extreme: f64, tolerance: f64) -> Vec<f64> {
    points
        .iter()
        .filter(|(x, _)| (*x - x_extreme).abs() <= tolerance)
        .map(|(_, y)| *y)
        .collect()
}

fn sorted_by_x(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|left, right| left.0.total_cmp(&right.0));
    sorted
}

/// Piecewise-linear `y(x)` on a surface sorted by ascending `x`, clamped to
/// the end values outside the sampled range.
fn interpolate_y(surface: &[(f64, f64)], x: f64) -> f64 {
    let (Some(first), Some(last)) = (surface.first(), surface.last()) else {
        return 0.0;
    };
    if x <= first.0 {
        return first.1;
    }
    if x >= last.0 {
        return last.1;
    }
    for pair in surface.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if x >= a.0 && x <= b.0 {
            let dx = b.0 - a.0;
            return if dx <= 0.0 {
                a.1
            } else {
                a.1 + (b.1 - a.1) * (x - a.0) / dx
            };
        }
    }
    last.1
}
