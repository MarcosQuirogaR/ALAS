// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::super::{
    SampledSurfaceDistribution, SampledSurfaceSample, SurfaceError, SurfaceOrder, SurfaceReference,
};
use super::error;
use std::collections::HashSet;

const EPS: f64 = 1.0e-12;

fn p2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}
fn p3(a: [f64; 3], b: [f64; 3]) -> f64 {
    p2(a, b) + (a[2] - b[2]).powi(2)
}
fn norm(a: [f64; 3]) -> f64 {
    a.iter().map(|v| v * v).sum::<f64>().sqrt()
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn raw_rows(
    text: &str,
    components: usize,
) -> Result<(Vec<([f64; 3], Vec<f64>)>, Option<usize>), SurfaceError> {
    let mut rows = Vec::new();
    let mut declared = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') || trimmed.starts_with("//") {
            let words = trimmed
                .trim_start_matches(|c| c == '#' || c == '/')
                .split_whitespace()
                .collect::<Vec<_>>();
            if let Some(i) = words.iter().position(|w| *w == "FACE_DATA") {
                declared = words.get(i + 1).and_then(|v| v.parse().ok());
            }
            continue;
        }
        let mut values = Vec::new();
        for token in trimmed
            .replace('(', " ")
            .replace(')', " ")
            .split_whitespace()
        {
            values.push(
                token
                    .parse::<f64>()
                    .map_err(|_| error("raw surface", format!("non-numeric token {token:?}")))?,
            );
        }
        if values.len() != 3 + components || values.iter().any(|v| !v.is_finite()) {
            return Err(error(
                "raw surface",
                format!(
                    "expected {} numeric columns, got {}",
                    3 + components,
                    values.len()
                ),
            ));
        }
        rows.push(([values[0], values[1], values[2]], values[3..].to_vec()));
    }
    Ok((rows, declared))
}
pub(crate) fn count_check(
    actual: usize,
    declared: Option<usize>,
    source: &str,
) -> Result<(), SurfaceError> {
    if let Some(expected) = declared {
        if actual != expected {
            return Err(SurfaceError::Mismatch(format!(
                "{source} has {actual} rows, header declares {expected}"
            )));
        }
    }
    if actual == 0 {
        return Err(SurfaceError::Mismatch(format!(
            "{source} contains no data rows"
        )));
    }
    Ok(())
}

fn tangent(point: [f64; 3], points: &[[f64; 3]], index: usize) -> [f64; 3] {
    let mut best = None;
    for (i, p) in points.iter().enumerate() {
        if i == index {
            continue;
        }
        let d = p2(point, *p);
        if best.is_none_or(|(_, old)| d < old) {
            best = Some((i, d));
        }
    }
    let Some((i, _)) = best else {
        return [1., 0., 0.];
    };
    let mut t = sub(points[i], point);
    let length = (t[0] * t[0] + t[1] * t[1]).sqrt();
    if length <= EPS {
        return [1., 0., 0.];
    }
    t = [t[0] / length, t[1] / length, 0.];
    if t[0] < -EPS {
        scale(t, -1.)
    } else {
        t
    }
}
pub(crate) fn parse_sampled(
    ptext: &str,
    stext: &str,
    r: &SurfaceReference,
) -> Result<SampledSurfaceDistribution, SurfaceError> {
    let (p, p_declared) = raw_rows(ptext, 1)?;
    let (s, s_declared) = raw_rows(stext, 3)?;
    count_check(p.len(), p_declared, "pressure raw surface")?;
    count_check(s.len(), s_declared, "wall-shear raw surface")?;
    if p.len() != s.len() || p.is_empty() {
        return Err(SurfaceError::Mismatch(
            "pressure and wall-shear raw rows differ or are empty".to_owned(),
        ));
    }
    let tol = (1.0e-10 * r.chord_m).max(1.0e-12);
    let mut used = HashSet::new();
    let mut rows = Vec::with_capacity(p.len());
    for (point, v) in p {
        let Some((index, distance)) = s
            .iter()
            .enumerate()
            .filter(|(i, _)| !used.contains(i))
            .map(|(i, (q, _))| (i, p3(point, *q)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
        else {
            return Err(SurfaceError::Mismatch("missing wall-shear row".to_owned()));
        };
        if distance > tol * tol {
            return Err(SurfaceError::Mismatch(
                "raw pressure/shear coordinates differ".to_owned(),
            ));
        }
        used.insert(index);
        let shear = [s[index].1[0], s[index].1[1], s[index].1[2]];
        rows.push((point, v[0], shear));
    }
    let positions = rows.iter().map(|(p, _, _)| *p).collect::<Vec<_>>();
    let (order, arcs) = order_points(&positions);
    let q = r.dynamic_pressure_pa();
    let mut out = Vec::with_capacity(rows.len());
    for (rank, index) in order.iter().enumerate() {
        let (point, p, shear) = rows[*index];
        let t = tangent(point, &positions, *index);
        let tau = scale(shear, r.density_kg_m3);
        out.push(SampledSurfaceSample {
            point_m: point,
            arc_length_m: arcs[rank],
            tangent_plus_chord: t,
            p_kinematic_m2_s2: p,
            pressure_pa: r.density_kg_m3 * p,
            wall_shear_kinematic_m2_s2: shear,
            wall_shear_stress_pa: tau,
            wall_shear_magnitude_pa: norm(tau),
            // wallShearStress is fluid-on-wall; skin friction on the fluid
            // therefore has the opposite sign.
            cf: -dot(tau, t) / q,
            cf_magnitude: norm(tau) / q,
            cp: (r.density_kg_m3 * p - r.pressure_reference_pa) / q,
        });
    }
    Ok(SampledSurfaceDistribution {
        order: SurfaceOrder::GeometricNearestNeighbour,
        samples: out,
    })
}

fn order_points(points: &[[f64; 3]]) -> (Vec<usize>, Vec<f64>) {
    if points.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let start = (0..points.len())
        .min_by(|a, b| {
            points[*a][0]
                .total_cmp(&points[*b][0])
                .then_with(|| points[*b][1].total_cmp(&points[*a][1]))
        })
        .unwrap_or(0);
    let mut order = vec![start];
    let mut used = HashSet::from([start]);
    while order.len() < points.len() {
        let current = *order.last().unwrap_or(&start);
        let Some(next) = (0..points.len())
            .filter(|i| !used.contains(i))
            .min_by(|a, b| {
                p2(points[current], points[*a]).total_cmp(&p2(points[current], points[*b]))
            })
        else {
            break;
        };
        used.insert(next);
        order.push(next);
    }
    let mut arc = 0.;
    let mut arcs = Vec::with_capacity(order.len());
    for rank in 0..order.len() {
        let previous = if rank == 0 {
            *order.last().unwrap_or(&start)
        } else {
            order[rank - 1]
        };
        let d = p2(points[previous], points[order[rank]]).sqrt();
        arcs.push(arc + 0.5 * d);
        arc += d;
    }
    (order, arcs)
}
