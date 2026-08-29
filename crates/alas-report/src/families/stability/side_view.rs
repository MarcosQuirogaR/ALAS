// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_stability_side_view`)
// Reference: alas @ rust-port-baseline.

//! Longitudinal stability markers over the built fuselage profile.

use alas_pipeline::full_analysis::AnalysisReport;

use super::scalars::{stability_scalars, AC_CHORD_FRACTION};
use super::status_scene;
use crate::chart_kit::{draw_legend, draw_title, equal_aspect_ranges, LegendMarker};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

/// High-contrast wing-MAC datum, kept separate from the marker and surface
/// colors so the reference chord cannot be mistaken for a stability marker.
pub(super) const WING_MAC_COLOR: &str = "#ff007f";

fn section_xz_at(
    section: &alas_geom::aircraft::wing::WingXSec,
    x_over_c: f64,
    z_over_c: f64,
    twist_deg: f64,
) -> [f64; 2] {
    // native aerodynamic model rotates a section about its positive spanwise axis. In the
    // side-view X/Z projection that is the negative of the usual 2-D
    // mathematical rotation used by this renderer.
    let angle = -twist_deg.to_radians();
    let (sin, cos) = angle.sin_cos();
    [
        section.xyz_le[0] + (x_over_c * cos - z_over_c * sin) * section.chord,
        section.xyz_le[2] + (x_over_c * sin + z_over_c * cos) * section.chord,
    ]
}

fn section_xz(
    section: &alas_geom::aircraft::wing::WingXSec,
    x_over_c: f64,
    z_over_c: f64,
) -> [f64; 2] {
    section_xz_at(section, x_over_c, z_over_c, section.twist)
}

fn draw_section_outline(
    scene: &mut Scene,
    axes: &Axes2D,
    section: &alas_geom::aircraft::wing::WingXSec,
    color: &str,
    twist_override: Option<f64>,
) {
    let points = section
        .airfoil
        .coordinates
        .iter()
        .map(|&(x_over_c, z_over_c)| {
            let [x, z] = section_xz_at(
                section,
                x_over_c,
                z_over_c,
                twist_override.unwrap_or(section.twist),
            );
            axes.map_point(x, z)
        })
        .collect::<Vec<_>>();
    if points.len() < 3 {
        return;
    }
    let color = Color::from_hex(color);
    scene.add(SceneElement::Polygon {
        points,
        fill: Some(Fill::new(Color::rgba(color.r, color.g, color.b, 72))),
        stroke: Some(Stroke::new(color, 1.4)),
    });
}

/// Draw the projected leading and trailing edges of a lifting surface.
///
/// The side view is intentionally a true geometric projection: the edge
/// lines connect every supplied section instead of replacing a tapered wing
/// with one root-section wedge. This also makes vertical-tail geometry
/// visible when it is present in the built airplane.
fn draw_surface_edges(
    scene: &mut Scene,
    axes: &Axes2D,
    wing: &alas_geom::aircraft::wing::Wing,
    color: &str,
    root_twist_override: Option<f64>,
) {
    let leading = wing
        .xsecs
        .iter()
        .map(|section| axes.map_point(section.xyz_le[0], section.xyz_le[2]))
        .collect::<Vec<_>>();
    let trailing = wing
        .xsecs
        .iter()
        .enumerate()
        .map(|(index, section)| {
            let twist = if index == 0 {
                root_twist_override.unwrap_or(section.twist)
            } else {
                section.twist
            };
            let [x, z] = section_xz_at(section, 1.0, 0.0, twist);
            axes.map_point(x, z)
        })
        .collect::<Vec<_>>();
    let stroke = Stroke::dashed(Color::from_hex(color), 1.1, 2.5, 2.0);
    if leading.len() >= 2 {
        scene.add(SceneElement::Polyline {
            points: leading,
            stroke: stroke.clone(),
        });
    }
    if trailing.len() >= 2 {
        scene.add(SceneElement::Polyline {
            points: trailing,
            stroke,
        });
    }
}

/// Select a visible local airfoil near the area-weighted MAC station.
///
/// A multi-panel wing's global MAC may not coincide with one literal section,
/// so its outline uses the nearest built section while the chord length and
/// longitudinal placement retain the exact global MAC scalars.
fn mac_reference_section(
    wing: &alas_geom::aircraft::wing::Wing,
) -> Option<&alas_geom::aircraft::wing::WingXSec> {
    let mut area_sum = 0.0;
    let mut mac_station_y_sum = 0.0;
    for pair in wing.xsecs.windows(2) {
        let span =
            (pair[1].xyz_le[1] - pair[0].xyz_le[1]).hypot(pair[1].xyz_le[2] - pair[0].xyz_le[2]);
        let area = span * (pair[0].chord + pair[1].chord) * 0.5;
        let taper = pair[1].chord / pair[0].chord.max(1.0e-9);
        let fraction = (1.0 + 2.0 * taper) / (3.0 + 3.0 * taper);
        let station_y = pair[0].xyz_le[1] + fraction * (pair[1].xyz_le[1] - pair[0].xyz_le[1]);
        area_sum += area;
        mac_station_y_sum += area * station_y;
    }
    let mac_station_y = if area_sum.is_finite() && area_sum > 0.0 {
        mac_station_y_sum / area_sum
    } else {
        0.0
    };
    wing.xsecs.iter().min_by(|left, right| {
        (left.xyz_le[1] - mac_station_y)
            .abs()
            .total_cmp(&(right.xyz_le[1] - mac_station_y).abs())
    })
}

fn draw_mac_airfoil_outline(
    scene: &mut Scene,
    axes: &Axes2D,
    wing: &alas_geom::aircraft::wing::Wing,
    x_lemac: f64,
    mac_z: f64,
    mac_chord: f64,
) {
    let Some(section) = mac_reference_section(wing) else {
        return;
    };
    let points = section
        .airfoil
        .coordinates
        .iter()
        .map(|&(x_over_c, z_over_c)| {
            axes.map_point(x_lemac + x_over_c * mac_chord, mac_z + z_over_c * mac_chord)
        })
        .collect::<Vec<_>>();
    if points.len() < 3 {
        return;
    }
    let color = Color::from_hex(WING_MAC_COLOR);
    scene.add(SceneElement::Polygon {
        points,
        fill: Some(Fill::new(Color::rgba(color.r, color.g, color.b, 72))),
        stroke: Some(Stroke::new(color, 1.7)),
    });
}

/// Render wing AC, CG, neutral point and tail AC from one shared scalar set.
pub fn figure_stability_side_view(report: &AnalysisReport, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let Some(s) = stability_scalars(report) else {
        return status_scene("Stability side view", "No wing data available", pal);
    };
    let Some(fus) = report.airplane.fuselages.first() else {
        return status_scene("Stability side view", "No fuselage data available", pal);
    };
    let mut x0 = fus.xsecs.first().map(|x| x.xyz_c[0]).unwrap_or(0.0);
    let mut x1 = fus.xsecs.last().map(|x| x.xyz_c[0]).unwrap_or(1.0);
    let z_values = fus
        .xsecs
        .iter()
        .flat_map(|x| [x.xyz_c[2] - x.height * 0.5, x.xyz_c[2] + x.height * 0.5]);
    let mut z0 = f64::INFINITY;
    let mut z1 = f64::NEG_INFINITY;
    for z in z_values {
        z0 = z0.min(z);
        z1 = z1.max(z);
    }
    if !z0.is_finite() {
        return status_scene("Stability side view", "No fuselage stations available", pal);
    }
    let vstab = report
        .airplane
        .wings
        .iter()
        .find(|wing| wing.name == "Vertical Stabilizer");
    for surface in [Some(s.wing), s.hstab, vstab].into_iter().flatten() {
        for section in &surface.xsecs {
            x0 = x0.min(section.xyz_le[0]);
            for &(x_over_c, z_over_c) in &section.airfoil.coordinates {
                let [x, z] = section_xz(section, x_over_c, z_over_c);
                x0 = x0.min(x);
                x1 = x1.max(x);
                z0 = z0.min(z);
                z1 = z1.max(z);
            }
        }
    }
    let top = z1 + (z1 - z0).max(0.5) * 0.55;
    let bottom = z0 - (z1 - z0).max(0.5) * 0.75;
    let provisional = Axes2D::new(
        (60.0, 50.0, 620.0, 380.0),
        (x0 - 1.0, x1 + 2.0),
        (bottom, top),
    );
    let (x_min, x_max, z_min, z_max) = equal_aspect_ranges(&provisional);
    let axes = Axes2D::new((60.0, 50.0, 620.0, 380.0), (x_min, x_max), (z_min, z_max));
    let mut scene = Scene::new(900.0, 500.0, Some(Color::from_hex(pal.bg)));
    let title = format!("Longitudinal Stability - SM = {:.1}% MAC", s.sm * 100.0);
    scene.title = Some(title.clone());
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();
    axes.draw_frame(&mut scene, pal);
    let outline = fus
        .xsecs
        .iter()
        .map(|x| axes.map_point(x.xyz_c[0], x.xyz_c[2] + x.height * 0.5))
        .chain(
            fus.xsecs
                .iter()
                .rev()
                .map(|x| axes.map_point(x.xyz_c[0], x.xyz_c[2] - x.height * 0.5)),
        )
        .collect::<Vec<_>>();
    scene.add(SceneElement::Polygon {
        points: outline,
        fill: Some(Fill::new(Color::rgba(149, 165, 166, 100))),
        stroke: Some(Stroke::new(Color::from_hex("#616a6e"), 1.1)),
    });
    if let Some(root) = s.wing.xsecs.first() {
        draw_section_outline(&mut scene, &axes, root, "#2980b9", Some(0.0));
    }
    if let Some(tip) = s.wing.xsecs.last() {
        draw_section_outline(&mut scene, &axes, tip, "#2980b9", None);
    }
    if let Some(tail) = s.hstab {
        if let Some(root) = tail.xsecs.first() {
            draw_section_outline(&mut scene, &axes, root, "#1abc9c", None);
        }
        if let Some(tip) = tail.xsecs.last() {
            draw_section_outline(&mut scene, &axes, tip, "#1abc9c", None);
        }
    }
    draw_surface_edges(&mut scene, &axes, s.wing, "#1a5276", Some(0.0));
    if let Some(tail) = s.hstab {
        draw_surface_edges(&mut scene, &axes, tail, "#0e6655", None);
    }
    if let Some(tail) = vstab {
        if let Some(root) = tail.xsecs.first() {
            draw_section_outline(&mut scene, &axes, root, "#f1c40f", None);
        }
        if let Some(tip) = tail.xsecs.last() {
            draw_section_outline(&mut scene, &axes, tip, "#f1c40f", None);
        }
        draw_surface_edges(&mut scene, &axes, tail, "#f1c40f", None);
    }

    let mac_color = Color::from_hex(WING_MAC_COLOR);
    let mac_z = s.wing.aerodynamic_center(AC_CHORD_FRACTION)[2];
    draw_mac_airfoil_outline(&mut scene, &axes, s.wing, s.x_lemac, mac_z, s.c_ref);
    draw_legend(
        &mut scene,
        [705.0, 72.0],
        &[(
            format!("Wing MAC ({:.2} m)", s.c_ref),
            LegendMarker::Line(Stroke::new(mac_color, 1.7)),
        )],
        pal,
        9.0,
    );
    let mut markers = vec![
        ("Wing AC", s.x_wing_ac, "#c0392b"),
        ("Phys CG", s.x_cg_phys, "#e67e22"),
        ("Aero CG", s.x_cg_aero, "#2980b9"),
        ("Neutral Pt", s.x_np, "#8e44ad"),
    ];
    if s.hstab.is_some() {
        markers.push(("H-Stab AC", s.x_hstab_ac, "#27ae60"));
    }
    markers.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (index, (name, x, color)) in markers.iter().enumerate() {
        let line = axes.map_point(*x, bottom);
        let line_top = axes.map_point(*x, top);
        scene.add(SceneElement::Line {
            p1: line,
            p2: line_top,
            stroke: Stroke::dashed(Color::from_hex(color), 1.3, 5.0, 3.0),
        });
        scene.add(SceneElement::Text {
            text: format!("{}\n{:.1} m ({:.0}% MAC)", name, x, s.pct(*x)),
            // The values can legitimately cluster within a small fraction of
            // one MAC. A dedicated, ordered margin keeps their identifiers
            // readable while the color-matched datum lines retain association.
            pos: [705.0, 94.0 + index as f64 * 42.0],
            font_size: 9.0,
            color: Color::from_hex(color),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: true,
        });
    }
    let arm_y = axes.map_point(0.0, bottom + (z1 - z0) * 0.35)[1];
    scene.add(SceneElement::Line {
        p1: [axes.map_point(s.x_cg_aero, 0.0)[0], arm_y],
        p2: [axes.map_point(s.x_np, 0.0)[0], arm_y],
        stroke: Stroke::new(Color::from_hex("#8e44ad"), 2.0),
    });
    scene
}
