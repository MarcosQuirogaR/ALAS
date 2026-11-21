// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_mass_distribution`)

//! Plan-view component centroids, scaled by the mass they represent.

use alas_pipeline::full_analysis::AnalysisReport;

use super::{no_data_scene, with_alpha};
use crate::chart_kit::draw_title;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

fn component_color(name: &str) -> Color {
    match name {
        "Payload" => Color::from_hex("#27ae60"),
        "Fuel" => Color::from_hex("#e67e22"),
        "Propulsion" => Color::from_hex("#c0392b"),
        "Wing" => Color::from_hex("#2980b9"),
        "Fuselage" => Color::from_hex("#7f8c8d"),
        "Systems" => Color::from_hex("#8e44ad"),
        "Furnishings" => Color::from_hex("#a569bd"),
        "Gear" => Color::from_hex("#34495e"),
        "H-Stab" => Color::from_hex("#1abc9c"),
        "V-Stab" => Color::from_hex("#16a085"),
        _ => Color::from_hex("#95a5a6"),
    }
}

/// Draw the real aircraft planform and real component mass centroids.
pub fn figure_mass_distribution(report: &AnalysisReport, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(850.0, 560.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Mass Distribution - Plan View  (bubble area scales with mass)".to_owned());
    let title = scene.title.clone().unwrap_or_default();
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();
    crate::chart_kit::draw_basis_subtitle(
        &mut scene,
        crate::chart_kit::MTOW_SIZING_BASIS,
        report
            .component_masses
            .get(alas_mass::breakdown::FUEL)
            .copied()
            .unwrap_or(0.0)
            .max(0.0),
        pal,
    );
    if report.component_masses.is_empty() || report.mass_coordinates.is_empty() {
        return no_data_scene(scene, pal, "No mass / coordinate data available");
    }
    let Some(fus) = report.airplane.fuselages.first() else {
        return no_data_scene(scene, pal, "No fuselage data available");
    };
    let x0 = fus.xsecs.first().map(|s| s.xyz_c[0]).unwrap_or(0.0);
    let x1 = fus.xsecs.last().map(|s| s.xyz_c[0]).unwrap_or(1.0);
    let fus_len = (x1 - x0).abs().max(1.0);
    let half_span = (report.airplane.b_ref * 0.5).max(1.0);
    let axes = Axes2D::new(
        (75.0, 50.0, 700.0, 430.0),
        (-half_span * 1.05, half_span * 1.05),
        (-fus_len * 0.14, x1 + fus_len * 0.20),
    )
    .with_equal_aspect();
    axes.draw_frame_with_labels(&mut scene, pal, "y (lateral) [m]", "x (longitudinal) [m]");
    let fus_outline = fus
        .xsecs
        .iter()
        .map(|s| [s.xyz_c[0], s.width * 0.5])
        .chain(fus.xsecs.iter().rev().map(|s| [s.xyz_c[0], -s.width * 0.5]))
        .collect::<Vec<_>>();
    if fus_outline.len() > 2 {
        scene.add(SceneElement::Polygon {
            points: fus_outline
                .iter()
                .map(|&p| axes.map_point(p[1], p[0]))
                .collect(),
            fill: Some(Fill::new(with_alpha(Color::from_hex("#bdc3c7"), 0.25))),
            stroke: Some(Stroke::new(Color::from_hex("#7f8c8d"), 1.0)),
        });
    }
    let mut name_rows: Vec<(f64, f64)> = Vec::new();
    for wing in &report.airplane.wings {
        let upper = wing
            .xsecs
            .iter()
            .map(|s| [s.xyz_le[0], s.xyz_le[1]])
            .collect::<Vec<_>>();
        let lower = wing
            .xsecs
            .iter()
            .rev()
            .map(|s| [s.xyz_le[0] + s.chord, s.xyz_le[1]]);
        let mut pts = upper;
        pts.extend(lower);
        // A surface that starts on the centreline is one closed outline with
        // its mirror image. One that starts off it (a wingtip device) has a
        // mirror image of its own: joining the two into one polygon would
        // draw a band across the whole span.
        let mut mirror_part: Option<Vec<[f64; 2]>> = None;
        if wing.symmetric {
            let mirror = pts.iter().rev().map(|p| [p[0], -p[1]]).collect::<Vec<_>>();
            let on_centreline = wing
                .xsecs
                .first()
                .is_some_and(|s| s.xyz_le[1].abs() < 0.05 * half_span);
            if on_centreline {
                pts.extend(mirror);
            } else {
                mirror_part = Some(mirror);
            }
        }
        if pts.len() > 2 {
            let fill = Fill::new(with_alpha(
                Color::from_hex(if wing.name.contains("Main") {
                    "#3498db"
                } else {
                    "#95a5a6"
                }),
                0.18,
            ));
            if let Some(mirror) = &mirror_part {
                scene.add(SceneElement::Polygon {
                    points: mirror.iter().map(|&p| axes.map_point(p[1], p[0])).collect(),
                    fill: Some(fill),
                    stroke: None,
                });
            }
            scene.add(SceneElement::Polygon {
                points: pts.iter().map(|&p| axes.map_point(p[1], p[0])).collect(),
                fill: Some(fill),
                stroke: None,
            });
            // Only the main wing has a bubble of its own; every other lifting
            // surface is named where it ends, so no grey area is anonymous.
            if !wing.name.contains("Main") {
                if let Some(tip) = wing.xsecs.last() {
                    let at = axes.map_point(tip.xyz_le[1] + 0.2, tip.xyz_le[0] + 0.5 * tip.chord);
                    // Surfaces that end near each other (the tail group) would
                    // print their names over one another: step the later
                    // label down until it clears the earlier ones.
                    let mut y = at[1];
                    while name_rows
                        .iter()
                        .any(|&(px, py)| (px - at[0]).abs() < 90.0 && (py - y).abs() < 10.0)
                    {
                        y += 10.0;
                    }
                    name_rows.push((at[0], y));
                    scene.add(SceneElement::Text {
                        text: wing.name.clone(),
                        pos: [at[0].min(axes.left + axes.width - 60.0), y],
                        font_size: 7.5,
                        color: Color::from_hex(pal.tick),
                        align: TextAlign::Left,
                        baseline: TextBaseline::Middle,
                        angle_deg: 0.0,
                        bold: false,
                    });
                }
            }
        }
    }
    // Nacelles are separate fuselages in the aircraft model. They carry
    // propulsion centroids, so omitting them makes the mass bubbles appear to
    // float beside an otherwise incomplete planform.
    for nac in report.airplane.fuselages.iter().skip(1) {
        if !nac.name.to_ascii_lowercase().contains("nacelle") {
            continue;
        }
        let upper = nac
            .xsecs
            .iter()
            .map(|s| [s.xyz_c[1] + s.width * 0.5, s.xyz_c[0]])
            .collect::<Vec<_>>();
        let lower = nac
            .xsecs
            .iter()
            .rev()
            .map(|s| [s.xyz_c[1] - s.width * 0.5, s.xyz_c[0]])
            .collect::<Vec<_>>();
        let points = upper
            .into_iter()
            .chain(lower)
            .map(|p| axes.map_point(p[0], p[1]))
            .collect::<Vec<_>>();
        if points.len() > 2 {
            scene.add(SceneElement::Polygon {
                points,
                fill: Some(Fill::new(with_alpha(Color::from_hex("#e67e22"), 0.25))),
                stroke: Some(Stroke::new(Color::from_hex("#d35400"), 0.8)),
            });
        }
    }
    let total = report
        .component_masses
        .values()
        .filter(|v| v.is_finite() && **v > 0.0)
        .sum::<f64>()
        .max(1.0);
    let mut labels: Vec<BubbleLabel> = Vec::new();
    let mut names = report.component_masses.keys().collect::<Vec<_>>();
    names.sort();
    for name in names {
        let mass = *report.component_masses.get(name).unwrap_or(&0.0);
        let Some(&[x, y, _]) = report.mass_coordinates.get(name) else {
            continue;
        };
        if mass <= 0.0 || !mass.is_finite() {
            continue;
        }
        let center = axes.map_point(y, x);
        let radius = (mass / total).sqrt() * 28.0 + 3.0;
        let color = component_color(name);
        scene.add(SceneElement::Circle {
            center,
            radius,
            fill: Some(Fill::new(Color::rgba(color.r, color.g, color.b, 185))),
            stroke: Some(Stroke::new(Color::from_hex(pal.bg), 1.0)),
        });
        let pct = mass / total * 100.0;
        let (dx, dy) = label_offset(name, y);
        labels.push(BubbleLabel {
            anchor: center,
            pos: [
                (center[0] + dx).clamp(12.0, scene.width - 12.0),
                (center[1] + dy).clamp(28.0, scene.height - 18.0),
            ],
            right_aligned: dx < 0.0,
            text: format!("{name}\n{:.1} t  ({pct:.0}%)", mass / 1000.0),
        });
    }
    spread_labels(&mut labels, scene.height);
    for label in &labels {
        scene.add(SceneElement::Line {
            p1: label.anchor,
            p2: label.pos,
            stroke: Stroke::new(Color::from_hex(pal.border), 0.7),
        });
        scene.add(SceneElement::Text {
            text: label.text.clone(),
            pos: label.pos,
            font_size: 7.5,
            color: Color::from_hex(pal.title),
            align: if label.right_aligned {
                TextAlign::Right
            } else {
                TextAlign::Left
            },
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: true,
        });
    }
    let cg = axes.map_point(report.physical_cg[1], report.physical_cg[0]);
    scene.add(SceneElement::Circle {
        center: cg,
        radius: 6.0,
        fill: Some(Fill::new(Color::from_hex("#f39c12"))),
        stroke: Some(Stroke::new(Color::from_hex(pal.title), 1.0)),
    });
    scene.add(SceneElement::Text {
        text: format!("Physical CG  x={:.1} m", report.physical_cg[0]),
        pos: [axes.left + axes.width - 10.0, axes.top + 16.0],
        font_size: 9.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Right,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });
    let aero_cg = axes.map_point(report.airplane.xyz_ref[1], report.airplane.xyz_ref[0]);
    scene.add(SceneElement::Circle {
        center: aero_cg,
        radius: 5.0,
        fill: Some(Fill::new(Color::from_hex(pal.accent))),
        stroke: Some(Stroke::new(Color::from_hex(pal.title), 1.0)),
    });
    scene.add(SceneElement::Text {
        text: format!("Aero CG  x={:.1} m", report.airplane.xyz_ref[0]),
        pos: [axes.left + axes.width - 10.0, axes.top + 46.0],
        font_size: 8.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Right,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });
    let delta = report.physical_cg[0] - report.airplane.xyz_ref[0];
    if delta.abs() > 0.2 {
        scene.add(SceneElement::Line {
            p1: [cg[0], cg[1]],
            p2: [aero_cg[0], aero_cg[1]],
            stroke: Stroke::new(Color::from_hex("#e74c3c"), 1.5),
        });
        scene.add(SceneElement::Text {
            text: format!("delta={delta:+.1} m"),
            pos: [axes.left + axes.width - 10.0, axes.top + 62.0],
            font_size: 8.5,
            color: Color::from_hex("#e74c3c"),
            align: TextAlign::Right,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: true,
        });
    }
    scene
}

fn label_offset(name: &str, lateral: f64) -> (f64, f64) {
    match name {
        "Payload" => (105.0, 50.0),
        "Fuel" => (-105.0, 48.0),
        "Propulsion" => (175.0, 90.0),
        "Wing" => (-150.0, 0.0),
        "Fuselage" => (105.0, -25.0),
        "Systems" => (-120.0, 82.0),
        "Furnishings" => (-145.0, -55.0),
        "Gear" => (150.0, 0.0),
        "H-Stab" => (65.0, 45.0),
        "V-Stab" => (-65.0, 45.0),
        _ => (20.0, if lateral >= 0.0 { 12.0 } else { -20.0 }),
    }
}

/// One bubble's label, placed before it is drawn.
struct BubbleLabel {
    /// Bubble centre the leader starts at.
    anchor: [f64; 2],
    /// Text anchor.
    pos: [f64; 2],
    /// Whether the text ends at `pos` rather than starting there.
    right_aligned: bool,
    /// Two lines: the component name and its mass.
    text: String,
}

/// Height of one two-line label block and the clearance kept between blocks, px.
const LABEL_BLOCK_PX: f64 = 22.0;
/// Approximate advance of one bold 7.5 pt character, px.
const LABEL_CHAR_PX: f64 = 5.2;

/// Push labels apart vertically until no two blocks overlap.
///
/// Blocks are taken top to bottom; one that overlaps an earlier block in both
/// directions moves down to clear it. Anchors do not move, so a leader may
/// lengthen but still ends on its own bubble.
fn spread_labels(labels: &mut [BubbleLabel], canvas_height: f64) {
    let span = |label: &BubbleLabel| {
        let width = label
            .text
            .lines()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as f64
            * LABEL_CHAR_PX;
        if label.right_aligned {
            (label.pos[0] - width, label.pos[0])
        } else {
            (label.pos[0], label.pos[0] + width)
        }
    };
    let mut order: Vec<usize> = (0..labels.len()).collect();
    order.sort_by(|&a, &b| labels[a].pos[1].total_cmp(&labels[b].pos[1]));
    for slot in 1..order.len() {
        let i = order[slot];
        for &j in &order[..slot] {
            let (a, b) = (span(&labels[i]), span(&labels[j]));
            let clear_x = a.1 < b.0 || b.1 < a.0;
            if !clear_x && labels[i].pos[1] - labels[j].pos[1] < LABEL_BLOCK_PX {
                labels[i].pos[1] = labels[j].pos[1] + LABEL_BLOCK_PX;
            }
        }
        labels[i].pos[1] = labels[i].pos[1].min(canvas_height - 18.0);
    }
}
