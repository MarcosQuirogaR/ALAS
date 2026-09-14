// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::super::{SurfaceError, SurfaceForceSummary, SurfaceReference, SurfaceSample};
use std::collections::{HashMap, HashSet};

const EPS: f64 = 1.0e-12;

#[derive(Clone)]
pub(crate) struct Geometry {
    center: [f64; 3],
    area_vector: [f64; 3],
    area: f64,
    length: f64,
    tangent: [f64; 3],
}

fn p2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}
fn norm(a: [f64; 3]) -> f64 {
    a.iter().map(|v| v * v).sum::<f64>().sqrt()
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn add(a: &mut [f64; 3], b: [f64; 3]) {
    for i in 0..3 {
        a[i] += b[i];
    }
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn face(points: &[[f64; 3]], ids: &[usize]) -> Result<Geometry, SurfaceError> {
    if ids.len() < 3 {
        return Err(SurfaceError::Parse {
            source: "faces".to_owned(),
            message: "airfoil face has fewer than three vertices".to_owned(),
        });
    }
    let mut center = [0.; 3];
    let mut area_vector = [0.; 3];
    let mut minz = f64::INFINITY;
    let mut maxz = f64::NEG_INFINITY;
    let mut tangent = [0., 0., 0.];
    let mut length = 0.;
    for id in ids {
        let p = *points.get(*id).ok_or_else(|| {
            SurfaceError::Mismatch("face references a missing mesh point".to_owned())
        })?;
        for j in 0..3 {
            center[j] += p[j];
        }
        minz = minz.min(p[2]);
        maxz = maxz.max(p[2]);
    }
    for j in 0..3 {
        center[j] /= ids.len() as f64;
    }
    for k in 0..ids.len() {
        let a = *points.get(ids[k]).ok_or_else(|| {
            SurfaceError::Mismatch("face references a missing mesh point".to_owned())
        })?;
        let b = *points.get(ids[(k + 1) % ids.len()]).ok_or_else(|| {
            SurfaceError::Mismatch("face references a missing mesh point".to_owned())
        })?;
        add(&mut area_vector, scale(cross(a, b), 0.5));
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let l = (dx * dx + dy * dy).sqrt();
        if l > length {
            length = l;
            tangent = [dx / l, dy / l, 0.];
        }
    }
    let area = norm(area_vector);
    if !area.is_finite() || area <= EPS || length <= EPS {
        return Err(SurfaceError::Mismatch(
            "airfoil wall face has zero area or length".to_owned(),
        ));
    }
    if tangent[0] < -EPS {
        tangent = scale(tangent, -1.);
    }
    Ok(Geometry {
        center,
        area_vector,
        area,
        length: if maxz - minz > EPS {
            area / (maxz - minz)
        } else {
            length
        },
        tangent,
    })
}
pub(crate) fn sample(
    local: usize,
    global: usize,
    g: Geometry,
    p: f64,
    shear: [f64; 3],
    r: &SurfaceReference,
) -> SurfaceSample {
    let tau = scale(shear, r.density_kg_m3);
    let q = r.dynamic_pressure_pa();
    SurfaceSample {
        patch_face_index: local,
        global_face_index: global,
        center_m: g.center,
        area_vector_m2: g.area_vector,
        face_area_m2: g.area,
        surface_length_m: g.length,
        arc_length_m: 0.,
        tangent_plus_chord: g.tangent,
        p_kinematic_m2_s2: p,
        pressure_pa: r.density_kg_m3 * p,
        wall_shear_kinematic_m2_s2: shear,
        wall_shear_stress_pa: tau,
        wall_shear_magnitude_pa: norm(tau),
        // OpenFOAM reports wallShearStress as the fluid-on-wall traction.
        // The wall traction on the fluid is its negative, which is the
        // streamwise skin-friction convention used by this API.
        cf: -dot(tau, g.tangent) / q,
        cf_magnitude: norm(tau) / q,
        cp: (r.density_kg_m3 * p - r.pressure_reference_pa) / q,
    }
}

pub(crate) fn order_faces(
    samples: &[SurfaceSample],
    faces: &[Vec<usize>],
    points: &[[f64; 3]],
    start: usize,
) -> (Vec<usize>, Vec<f64>, bool) {
    let mut edges: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (local, global) in (start..start + samples.len()).enumerate() {
        let Some(face) = faces.get(global) else {
            continue;
        };
        for k in 0..face.len() {
            let a = face[k];
            let b = face[(k + 1) % face.len()];
            let key = if a < b { (a, b) } else { (b, a) };
            edges.entry(key).or_default().push(local);
        }
    }
    let mut adj = vec![HashSet::new(); samples.len()];
    for list in edges.values() {
        if list.len() == 2 {
            adj[list[0]].insert(list[1]);
            adj[list[1]].insert(list[0]);
        }
    }
    let valid = adj.iter().all(|v| v.len() == 2);
    if valid {
        let mut order = Vec::with_capacity(samples.len());
        let start_i = (0..samples.len())
            .min_by(|a, b| {
                samples[*a].center_m[0]
                    .total_cmp(&samples[*b].center_m[0])
                    .then_with(|| samples[*b].center_m[1].total_cmp(&samples[*a].center_m[1]))
            })
            .unwrap_or(0);
        let mut current = start_i;
        let mut previous = None;
        while order.len() < samples.len() {
            order.push(current);
            let next = adj[current]
                .iter()
                .copied()
                .filter(|v| Some(*v) != previous)
                .find(|v| *v != start_i || order.len() + 1 == samples.len());
            let Some(next) = next else { break };
            previous = Some(current);
            current = next;
        }
        if order.len() == samples.len() {
            let arcs = arcs(&order, samples);
            return (order, arcs, true);
        }
    }
    let positions = samples.iter().map(|s| s.center_m).collect::<Vec<_>>();
    let (order, arcs) = order_points(&positions);
    let _ = points;
    (order, arcs, false)
}
fn arcs(order: &[usize], samples: &[SurfaceSample]) -> Vec<f64> {
    let mut total = 0.;
    order
        .iter()
        .map(|i| {
            let s = total + 0.5 * samples[*i].surface_length_m;
            total += samples[*i].surface_length_m;
            s
        })
        .collect()
}
pub(crate) fn integrate(
    samples: &[SurfaceSample],
    r: &SurfaceReference,
) -> Result<SurfaceForceSummary, SurfaceError> {
    let q = r.dynamic_pressure_pa();
    let denom = q * r.reference_area_m2;
    if !denom.is_finite() || denom <= 0. {
        return Err(SurfaceError::InvalidInput(
            "surface coefficient denominator is not positive".to_owned(),
        ));
    }
    let p_ref = r.pressure_reference_pa / r.density_kg_m3;
    let (mut pf, mut vf, mut pm, mut vm) = ([0.; 3], [0.; 3], [0.; 3], [0.; 3]);
    for s in samples {
        let pressure = scale(
            s.area_vector_m2,
            r.density_kg_m3 * (s.p_kinematic_m2_s2 - p_ref),
        );
        let viscous = scale(s.wall_shear_stress_pa, -s.face_area_m2);
        add(&mut pf, pressure);
        add(&mut vf, viscous);
        add(
            &mut pm,
            cross(sub(s.center_m, r.moment_reference_m), pressure),
        );
        add(
            &mut vm,
            cross(sub(s.center_m, r.moment_reference_m), viscous),
        );
    }
    let total = [pf[0] + vf[0], pf[1] + vf[1], pf[2] + vf[2]];
    let tm = [pm[0] + vm[0], pm[1] + vm[1], pm[2] + vm[2]];
    let drag = r.drag_direction();
    let lift = r.lift_direction();
    let coeff = |f: [f64; 3], d: [f64; 3]| dot(f, d) / denom;
    let cm = |m: [f64; 3]| -m[2] / (denom * r.chord_m);
    Ok(SurfaceForceSummary {
        pressure_force_n: pf,
        viscous_force_n: vf,
        total_force_n: total,
        pressure_moment_nm: pm,
        viscous_moment_nm: vm,
        total_moment_nm: tm,
        cd_pressure: coeff(pf, drag),
        cd_viscous: coeff(vf, drag),
        cd: coeff(total, drag),
        cl_pressure: coeff(pf, lift),
        cl_viscous: coeff(vf, lift),
        cl: coeff(total, lift),
        cm_pressure: cm(pm),
        cm_viscous: cm(vm),
        cm: cm(tm),
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
