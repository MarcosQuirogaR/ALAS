// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// A normalized database-coordinate snapshot used by a case and all result
/// provenance. Coordinates remain in unit-chord `(x/c, y/c)` space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirfoilSnapshot {
    /// Database identity selected by the user.
    pub name: String,
    /// Exact normalized coordinates written to the case CSV and Gmsh source.
    pub coordinates: Vec<(f64, f64)>,
    /// Stable hash of name and coordinate bytes.
    pub coordinate_hash: String,
}

/// Resolve a database airfoil without substitution or silent repair.
pub fn resolve_airfoil(name: &str) -> Result<AirfoilSnapshot, String> {
    let Some(airfoil) = AirfoilLibrary::get(name) else {
        return Err(format!("Airfoil '{name}' is not present in the database."));
    };
    validate_coordinates(&airfoil.coordinates)?;
    Ok(AirfoilSnapshot {
        name: name.to_owned(),
        coordinate_hash: coordinate_hash(name, &airfoil.coordinates),
        coordinates: airfoil.coordinates,
    })
}

/// Validate Selig ordering, finite values, duplicates, and self-intersection.
pub fn validate_coordinates(coordinates: &[(f64, f64)]) -> Result<(), String> {
    if coordinates.len() < 8 {
        return Err("An airfoil needs at least eight coordinates.".to_owned());
    }
    if coordinates
        .iter()
        .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err("Airfoil coordinates must all be finite.".to_owned());
    }
    let (min_x, max_x) = coordinates
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |acc, p| {
            (acc.0.min(p.0), acc.1.max(p.0))
        });
    if min_x < -0.05 || max_x > 1.05 || max_x - min_x < 0.5 {
        return Err("Airfoil coordinates must be normalized to a unit chord.".to_owned());
    }
    // Database sections conventionally repeat the first point as their last
    // point. Treat that point as the loop closure for topology checks; keeping
    // it in the snapshot and coordinate hash preserves exact provenance while
    // avoiding a zero-length closing segment that falsely intersects the
    // penultimate edge.
    let topology_points = canonical_topology_points(coordinates);
    if topology_points.len() < 7 {
        return Err("An airfoil needs at least seven distinct boundary coordinates.".to_owned());
    }
    for (i, left) in topology_points.iter().enumerate() {
        for (j, right) in topology_points.iter().enumerate().skip(i + 1) {
            if distance_sq(*left, *right) < 1.0e-20 {
                return Err(format!(
                    "Duplicate airfoil coordinate at indices {i} and {j}."
                ));
            }
        }
    }
    let n = topology_points.len();
    for i in 0..n {
        let a = topology_points[i];
        let b = topology_points[(i + 1) % n];
        for j in (i + 1)..n {
            let c = topology_points[j];
            let d = topology_points[(j + 1) % n];
            if i == j || (i + 1) % n == j || i == (j + 1) % n {
                continue;
            }
            if segments_cross(a, b, c, d) {
                return Err(format!(
                    "Airfoil self-intersects near segments {i} and {j}."
                ));
            }
        }
    }
    Ok(())
}

fn canonical_topology_points(coordinates: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut points = coordinates.to_vec();
    while points.len() > 1 && distance_sq(points[0], *points.last().unwrap_or(&points[0])) < 1.0e-20
    {
        points.pop();
    }
    points
}

fn distance_sq(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).mul_add(a.0 - b.0, (a.1 - b.1) * (a.1 - b.1))
}

fn orientation(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn segments_cross(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> bool {
    let ab_c = orientation(a, b, c);
    let ab_d = orientation(a, b, d);
    let cd_a = orientation(c, d, a);
    let cd_b = orientation(c, d, b);
    if ab_c.abs() < 1.0e-10 && ab_d.abs() < 1.0e-10 && cd_a.abs() < 1.0e-10 && cd_b.abs() < 1.0e-10
    {
        return ranges_overlap(a.0, b.0, c.0, d.0) && ranges_overlap(a.1, b.1, c.1, d.1);
    }
    ((ab_c >= 0.0 && ab_d <= 0.0) || (ab_c <= 0.0 && ab_d >= 0.0))
        && ((cd_a >= 0.0 && cd_b <= 0.0) || (cd_a <= 0.0 && cd_b >= 0.0))
}

fn ranges_overlap(a: f64, b: f64, c: f64, d: f64) -> bool {
    a.min(b) <= c.max(d) + 1.0e-10 && c.min(d) <= a.max(b) + 1.0e-10
}

fn coordinate_hash(name: &str, coordinates: &[(f64, f64)]) -> String {
    // FNV-1a is deterministic across platforms and requires no crypto crate;
    // it is an identity/provenance checksum, not a security boundary.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in name.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for &(x, y) in coordinates {
        for byte in x.to_le_bytes().into_iter().chain(y.to_le_bytes()) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("fnv1a64-{hash:016x}")
}
