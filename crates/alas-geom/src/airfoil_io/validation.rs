// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometric validation for normalized Selig outlines.

use super::AirfoilImportError;

const COORDINATE_TOLERANCE: f64 = 1.0e-6;
const DUPLICATE_TOLERANCE: f64 = 1.0e-10;
const ORIENTATION_TOLERANCE: f64 = 1.0e-12;

/// Validate a normalized Selig outline without modifying it.
pub fn validate_coordinates(
    name: &str,
    coordinates: &[(f64, f64)],
) -> Result<(), AirfoilImportError> {
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        return Err(AirfoilImportError::InvalidName);
    }
    if coordinates.len() < 5 {
        return Err(AirfoilImportError::TooFewCoordinates(coordinates.len()));
    }
    for (index, &(x, y)) in coordinates.iter().enumerate() {
        if !x.is_finite() || !y.is_finite() {
            return Err(AirfoilImportError::NonFinite { line: index + 1 });
        }
        if !(-COORDINATE_TOLERANCE..=1.0 + COORDINATE_TOLERANCE).contains(&x) {
            return Err(AirfoilImportError::InvalidOrder { line: index + 1 });
        }
    }

    let first = coordinates[0];
    let last = *coordinates.last().expect("length checked above");
    if distance(first, last) > COORDINATE_TOLERANCE {
        return Err(AirfoilImportError::OpenTrailingEdge);
    }
    if (first.0 - 1.0).abs() > COORDINATE_TOLERANCE || (last.0 - 1.0).abs() > COORDINATE_TOLERANCE {
        return Err(AirfoilImportError::InvalidTrailingEdge);
    }

    let leading_edge_x = coordinates
        .iter()
        .map(|&(x, _)| x)
        .fold(f64::INFINITY, f64::min);
    if leading_edge_x.abs() > COORDINATE_TOLERANCE {
        return Err(AirfoilImportError::MissingLeadingEdge);
    }
    let leading_edge = coordinates
        .iter()
        .position(|&(x, _)| (x - leading_edge_x).abs() <= DUPLICATE_TOLERANCE)
        .expect("finite non-empty coordinates have a minimum");
    if leading_edge == 0 || leading_edge >= coordinates.len() - 1 {
        return Err(AirfoilImportError::InvalidOrder {
            line: leading_edge + 1,
        });
    }
    for index in 1..coordinates.len() {
        if index == coordinates.len() - 1 {
            continue;
        }
        let delta = coordinates[index].0 - coordinates[index - 1].0;
        let valid = if index <= leading_edge {
            delta < -DUPLICATE_TOLERANCE
        } else {
            delta > DUPLICATE_TOLERANCE
        };
        if !valid {
            return Err(AirfoilImportError::InvalidOrder { line: index + 1 });
        }
    }

    for first_index in 0..coordinates.len() {
        for second_index in (first_index + 1)..coordinates.len() {
            if first_index == 0 && second_index == coordinates.len() - 1 {
                continue;
            }
            if distance(coordinates[first_index], coordinates[second_index]) <= DUPLICATE_TOLERANCE
            {
                return Err(AirfoilImportError::DuplicatePoint {
                    first: first_index,
                    second: second_index,
                });
            }
        }
    }

    let segment_count = coordinates.len() - 1;
    for first_segment in 0..segment_count {
        for second_segment in (first_segment + 1)..segment_count {
            if second_segment == first_segment + 1
                || (first_segment == 0 && second_segment == segment_count - 1)
            {
                continue;
            }
            if segments_intersect(
                coordinates[first_segment],
                coordinates[first_segment + 1],
                coordinates[second_segment],
                coordinates[second_segment + 1],
            ) {
                return Err(AirfoilImportError::SelfIntersection {
                    first: first_segment,
                    second: second_segment,
                });
            }
        }
    }

    let area = coordinates
        .windows(2)
        .map(|pair| pair[0].0 * pair[1].1 - pair[1].0 * pair[0].1)
        .sum::<f64>()
        .abs()
        * 0.5;
    if area <= DUPLICATE_TOLERANCE {
        return Err(AirfoilImportError::DegenerateOutline);
    }
    Ok(())
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn orientation(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn on_segment(a: (f64, f64), b: (f64, f64), point: (f64, f64)) -> bool {
    point.0 >= a.0.min(b.0) - ORIENTATION_TOLERANCE
        && point.0 <= a.0.max(b.0) + ORIENTATION_TOLERANCE
        && point.1 >= a.1.min(b.1) - ORIENTATION_TOLERANCE
        && point.1 <= a.1.max(b.1) + ORIENTATION_TOLERANCE
}

fn segments_intersect(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> bool {
    let ab_c = orientation(a, b, c);
    let ab_d = orientation(a, b, d);
    let cd_a = orientation(c, d, a);
    let cd_b = orientation(c, d, b);
    if ab_c.abs() <= ORIENTATION_TOLERANCE && on_segment(a, b, c) {
        return true;
    }
    if ab_d.abs() <= ORIENTATION_TOLERANCE && on_segment(a, b, d) {
        return true;
    }
    if cd_a.abs() <= ORIENTATION_TOLERANCE && on_segment(c, d, a) {
        return true;
    }
    if cd_b.abs() <= ORIENTATION_TOLERANCE && on_segment(c, d, b) {
        return true;
    }
    (ab_c > ORIENTATION_TOLERANCE) != (ab_d > ORIENTATION_TOLERANCE)
        && (cd_a > ORIENTATION_TOLERANCE) != (cd_b > ORIENTATION_TOLERANCE)
}
