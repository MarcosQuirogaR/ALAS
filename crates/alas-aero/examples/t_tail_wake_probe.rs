// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Conditioning and convergence probe of a preset's lattice: solves the
//! preset at a ladder of tail strip counts (`empennage.n_subdivisions`) and
//! (spanwise, chordwise) resolutions and prints lift, pitching moment, and
//! the sideslip derivatives (finite difference over +-2 deg), or the solver
//! error, for the full aircraft.
//!
//! Usage: t_tail_wake_probe [PRESET] [fin-only]

#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_geom::builder::AircraftBuilder;

/// Distance from `point` to the semi-infinite leg from `vertex` along +x.
fn distance_to_trailing_leg(point: [f64; 3], vertex: [f64; 3]) -> f64 {
    let along = (point[0] - vertex[0]).max(0.0);
    ((point[1] - vertex[1]).powi(2)
        + (point[2] - vertex[2]).powi(2)
        + (point[0] - vertex[0] - along).powi(2))
    .sqrt()
}

/// Distance from `point` to the segment `a`-`b`.
fn distance_to_segment(point: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ap = [point[0] - a[0], point[1] - a[1], point[2] - a[2]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    let t = ((ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / len2).clamp(0.0, 1.0);
    let foot = [a[0] + t * ab[0], a[1] + t * ab[1], a[2] + t * ab[2]];
    ((point[0] - foot[0]).powi(2) + (point[1] - foot[1]).powi(2) + (point[2] - foot[2]).powi(2))
        .sqrt()
}

/// The ten collocation points that lie closest, relative to their own panel's
/// strip width, to a vortex segment or trailing leg of a different panel.
fn closest_approaches(
    plane: &alas_geom::aircraft::airplane::Airplane,
    solved: &alas_aero::vlm::VlmResult,
    chordwise: usize,
) {
    let panels = &solved.panels;
    let mut rows: Vec<(f64, String)> = Vec::new();
    for (i, field) in panels.iter().enumerate() {
        let mid = |a: [f64; 3], b: [f64; 3]| {
            [
                0.5 * (a[0] + b[0]),
                0.5 * (a[1] + b[1]),
                0.5 * (a[2] + b[2]),
            ]
        };
        let front = mid(field.front_left, field.front_right);
        let back = mid(field.back_left, field.back_right);
        let c = [
            0.25 * front[0] + 0.75 * back[0],
            0.25 * front[1] + 0.75 * back[1],
            0.25 * front[2] + 0.75 * back[2],
        ];
        let w = ((field.right_vortex_vertex[1] - field.left_vortex_vertex[1]).powi(2)
            + (field.right_vortex_vertex[2] - field.left_vortex_vertex[2]).powi(2))
        .sqrt();
        let mut best = (f64::INFINITY, 0_usize, "");
        for (k, source) in panels.iter().enumerate() {
            if k == i || (source.wing_index == field.wing_index && k / chordwise == i / chordwise) {
                continue;
            }
            for (d, kind) in [
                (
                    distance_to_trailing_leg(c, source.left_vortex_vertex),
                    "left leg",
                ),
                (
                    distance_to_trailing_leg(c, source.right_vortex_vertex),
                    "right leg",
                ),
                (
                    distance_to_segment(c, source.left_vortex_vertex, source.right_vortex_vertex),
                    "bound",
                ),
            ] {
                if d < best.0 {
                    best = (d, k, kind);
                }
            }
        }
        let source = &panels[best.1];
        rows.push((
            best.0 / w,
            format!(
                "{} #{i} at ({:.2},{:.2},{:.2}) strip {:.4}: {:.4} m from {} of {} #{} ({:.1} strips)",
                plane.wings[field.wing_index].name, c[0], c[1], c[2], w, best.0, best.2,
                plane.wings[source.wing_index].name, best.1, best.0 / w
            ),
        ));
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, text) in rows.iter().take(12) {
        println!("    {text}");
    }
}

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "A400M".to_owned());
    let keep = std::env::args().nth(2);
    let entry = alas_config::presets::get(&name).expect("preset");
    let tails: Vec<i64> = std::env::var("PROBE_TAILS").map_or(vec![6, 12, 24, 48, 96], |v| {
        v.split(',').map(|t| t.parse().unwrap()).collect()
    });
    for tail_strips in tails {
        let mut geometry = entry.geometry.clone();
        geometry.empennage.n_subdivisions = tail_strips;
        let mut plane = AircraftBuilder::new(Some(geometry))
            .build(Some(&entry.design_vector), true)
            .expect("builds");
        if let Some(only) = &keep {
            plane
                .wings
                .retain(|w| only.split(',').any(|n| w.name.starts_with(n)));
        }
        if let Ok(y) = std::env::var("STAB_TRUNC") {
            let y: f64 = y.parse().unwrap();
            for w in plane
                .wings
                .iter_mut()
                .filter(|w| w.name.starts_with("Horizontal"))
            {
                w.xsecs.retain(|x| x.xyz_le[1] <= y);
            }
        }
        if let Ok(z) = std::env::var("STAB_Z") {
            let z: f64 = z.parse().unwrap();
            for w in plane
                .wings
                .iter_mut()
                .filter(|w| w.name.starts_with("Horizontal"))
            {
                for x in &mut w.xsecs {
                    x.xyz_le[2] = z;
                }
            }
        }
        let chords: Vec<usize> = std::env::var("PROBE_CHORDS").map_or(vec![8, 16, 24], |v| {
            v.split(',').map(|t| t.parse().unwrap()).collect()
        });
        for (span, chord) in chords.into_iter().map(|c| {
            (
                std::env::var("PROBE_SPAN")
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(1),
                c,
            )
        }) {
            let result = VlmSystem::assemble(&plane, span, chord).map(|system| {
                let solve = |alpha: f64, beta: f64| {
                    let op = OperatingPoint::new(
                        Atmosphere::new(0.0),
                        100.0,
                        alpha,
                        beta,
                        0.0,
                        0.0,
                        0.0,
                    );
                    system.solve(&op)
                };
                if std::env::var("PROBE_DISTANCES")
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok())
                    == Some(chord)
                {
                    if let Ok(s) = solve(2.0, 0.0) {
                        closest_approaches(&plane, &s, chord);
                    }
                }
                if std::env::var("PROBE_WIDTHS").is_ok() {
                    if let Ok(s) = solve(2.0, 0.0) {
                        for (index, wing) in plane.wings.iter().enumerate() {
                            let mut widths: Vec<f64> = s
                                .panels
                                .iter()
                                .filter(|p| p.wing_index == index)
                                .map(|p| {
                                    (p.right_vortex_vertex[1] - p.left_vortex_vertex[1])
                                        .hypot(p.right_vortex_vertex[2] - p.left_vortex_vertex[2])
                                })
                                .collect();
                            widths.sort_by(f64::total_cmp);
                            widths.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
                            println!(
                                "    {} strip widths smallest {:?} largest {:.3}",
                                wing.name,
                                &widths[..widths.len().min(4)],
                                widths.last().copied().unwrap_or(0.0)
                            );
                        }
                    }
                }
                let pivot = solve(2.0, 0.0).map_or(f64::NAN, |r| r.solve_diagnostics.pivot_ratio);
                let text = match (solve(2.0, 2.0), solve(2.0, -2.0)) {
                    (Ok(p), Ok(m)) => format!(
                        "pivot {:.1} CL {:.4} Cm {:.4} Cn_beta/rad {:.4} Cy_beta/rad {:.4}",
                        pivot,
                        0.5 * (p.cl_lift + m.cl_lift),
                        0.5 * (p.cm_pitch + m.cm_pitch),
                        (p.cn_yaw - m.cn_yaw) / (4.0_f64.to_radians()),
                        (p.cy_side - m.cy_side) / (4.0_f64.to_radians()),
                    ),
                    (Err(e), _) | (_, Err(e)) => format!("ERR {e}"),
                };
                format!("{} panels; {text}", system.panel_count())
            });
            match result {
                Ok(text) => println!("{name} tail{tail_strips} {span}x{chord}: {text}"),
                Err(e) => println!("{name} tail{tail_strips} {span}x{chord}: ASSEMBLE ERR {e}"),
            }
        }
    }
}
