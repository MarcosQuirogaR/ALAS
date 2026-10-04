// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Sparse closed contours on the shared airfoil and body loft geometry.

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::section_outline::mirror_y;
use alas_geom::aircraft::wing::Wing;

use crate::scene::{Color, Scene, SceneElement, Stroke};

use super::visibility::ContourVisibility;
use super::BULKHEAD_SEGMENTS;

fn draw_visible_contour(
    scene: &mut Scene,
    points: &[[f64; 3]],
    visibility: &ContourVisibility,
    stroke: Stroke,
) {
    let mut path = Vec::new();
    for pair in points.windows(2) {
        for segment in visibility.visible_segments(pair[0], pair[1]) {
            if path.last().is_some_and(|last| *last != segment[0]) {
                scene.add(SceneElement::Polyline {
                    points: std::mem::take(&mut path),
                    stroke: stroke.clone(),
                });
            }
            if path.is_empty() {
                path.push(segment[0]);
            }
            path.push(segment[1]);
        }
    }
    if path.len() >= 2 {
        scene.add(SceneElement::Polyline {
            points: path,
            stroke,
        });
    }
}

fn profile_break(a: &Wing, index: usize, left: f64, right: f64) -> bool {
    let profiles = [
        &a.xsecs[index - 1].airfoil,
        &a.xsecs[index].airfoil,
        &a.xsecs[index + 1].airfoil,
    ];
    let coordinates = profiles.map(|profile| &profile.coordinates);
    let changes = |a: f64, b: f64, c: f64| ((b - a) / left - (c - b) / right).abs() > 1e-8;
    if coordinates[0].len() == coordinates[1].len() && coordinates[1].len() == coordinates[2].len()
    {
        return coordinates[0]
            .iter()
            .zip(coordinates[1])
            .zip(coordinates[2])
            .any(|((&a, &b), &c)| changes(a.0, b.0, c.0) || changes(a.1, b.1, c.1));
    }
    let stations = (0..=20)
        .map(|index| f64::from(index) / 20.0)
        .collect::<Vec<_>>();
    let samples = profiles.map(|profile| {
        let mut samples = profile.local_camber(&stations);
        samples.extend(profile.local_thickness(&stations));
        samples
    });
    samples[0]
        .iter()
        .zip(&samples[1])
        .zip(&samples[2])
        .any(|((&a, &b), &c)| changes(a, b, c))
}

pub(super) fn defining_stations(wing: &Wing) -> Vec<usize> {
    let count = wing.xsecs.len();
    if count < 3 {
        return (0..count).collect();
    }
    let mut stations = vec![0, count - 1];
    // Preserve planform and twist breaks, independent of solver subdivisions.
    for index in 1..count - 1 {
        let a = &wing.xsecs[index - 1];
        let b = &wing.xsecs[index];
        let c = &wing.xsecs[index + 1];
        let distance = |p: &alas_geom::aircraft::wing::WingXSec,
                        q: &alas_geom::aircraft::wing::WingXSec| {
            ((q.xyz_le[1] - p.xyz_le[1]).powi(2) + (q.xyz_le[2] - p.xyz_le[2]).powi(2)).sqrt()
        };
        let left = distance(a, b);
        let right = distance(b, c);
        if left <= f64::EPSILON || right <= f64::EPSILON {
            stations.push(index);
            continue;
        }
        let changed = (0..3).any(|axis| {
            ((b.xyz_le[axis] - a.xyz_le[axis]) / left - (c.xyz_le[axis] - b.xyz_le[axis]) / right)
                .abs()
                > 1e-8
        }) || ((b.chord - a.chord) / left - (c.chord - b.chord) / right).abs() > 1e-8
            || ((b.twist - a.twist) / left - (c.twist - b.twist) / right).abs() > 1e-8;
        if changed || profile_break(wing, index, left, right) {
            stations.push(index);
        }
    }
    stations.sort_unstable();
    stations
}

pub(super) fn section_stations(wing: &Wing) -> Vec<usize> {
    let mut stations = defining_stations(wing);
    for fraction in 1..4 {
        let count = wing.xsecs.len();
        if count == 0 {
            break;
        }
        stations.push(fraction * (count - 1) / 4);
    }
    stations.sort_unstable();
    stations.dedup();
    stations
}

pub(super) fn draw_surface_sections(
    scene: &mut Scene,
    plane: &Airplane,
    visibility: &ContourVisibility,
    color: Color,
) {
    for wing in &plane.wings {
        let outlines = wing.section_outlines(Some(80));
        let defining = defining_stations(wing);
        let mut sections = section_stations(wing)
            .into_iter()
            .filter_map(|index| {
                outlines
                    .get(index)
                    .filter(|outline| !outline.is_empty())
                    .map(|outline| (outline.clone(), defining.contains(&index)))
            })
            .collect::<Vec<_>>();
        if outlines.len() == 2 {
            // Intermediate contours lie on the same straight loft faces as the skin.
            for fraction in [0.25, 0.5, 0.75] {
                sections.push((
                    outlines[0]
                        .iter()
                        .zip(&outlines[1])
                        .map(|(a, b)| {
                            std::array::from_fn(|axis| a[axis] + fraction * (b[axis] - a[axis]))
                        })
                        .collect(),
                    false,
                ));
            }
        }
        for (outline, defining) in sections {
            if outline.is_empty() {
                continue;
            }
            let stroke = if defining {
                Stroke::new(color, 0.9)
            } else {
                Stroke::new(Color::rgba(color.r, color.g, color.b, 105), 0.65)
            };
            for mirror in if wing.symmetric {
                &[false, true][..]
            } else {
                &[false][..]
            } {
                let mut points = outline
                    .iter()
                    .map(|&point| if *mirror { mirror_y(point) } else { point })
                    .collect::<Vec<_>>();
                points.push(points[0]);
                draw_visible_contour(scene, &points, visibility, stroke.clone());
            }
        }
    }
    for fuselage in &plane.fuselages {
        let count = fuselage.xsecs.len();
        for index in 0..count {
            if index != 0 && index + 1 != count && index % (count / 6).max(1) != 0 {
                continue;
            }
            let section = &fuselage.xsecs[index];
            let points = (0..=BULKHEAD_SEGMENTS)
                .map(|step| {
                    let theta = std::f64::consts::TAU * step as f64 / BULKHEAD_SEGMENTS as f64;
                    [
                        section.xyz_c[0],
                        section.xyz_c[1] + section.width * 0.5 * theta.cos(),
                        section.xyz_c[2] + section.height * 0.5 * theta.sin(),
                    ]
                })
                .collect::<Vec<_>>();
            draw_visible_contour(scene, &points, visibility, Stroke::new(color, 0.65));
        }
    }
}
