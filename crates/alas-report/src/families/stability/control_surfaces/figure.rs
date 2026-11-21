// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py:figure_control_surfaces (L1735-1991)

use super::geometry::{
    cs_surface_area, cs_surface_patch, draw_planform_fill, draw_top_patch, fmt_volume_coef,
    planform_bounds, TopAxes,
};
use crate::chart_kit::{draw_title, LegendMarker};
use crate::scene::{
    Axes2D, Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
    CSS_PIXELS_PER_POINT,
};
use crate::theme::{get_palette, Palette};
use alas_config::control_surfaces::ControlSurfacesConfig;
use alas_config::AlasConfig;
use alas_geom::aircraft::wing::Wing;
use alas_pipeline::full_analysis::AnalysisReport;
use alas_stab::trim::tail_volume_coefficients;

/// Tailplane volume-coefficient band the figure annotates `Vh` against: the
/// range Jenkinson, *Civil Jet Aircraft Design*, Ch. 6 (PDF p. 137)
/// gives for existing civil transports (0.5 to 1.2); Raymer Table 6.4 (printed p. 160) quotes
/// 1.0 as the jet-transport typical value. A reading aid only; no search or
/// feasibility check uses it. The band is for civil jet transports: a military
/// T-tail turboprop such as the A400M (Jane's 2008-09: tailplane 67.00 m2, model
/// 67.7 m2) sits above it at Vh 1.27 with its published areas.
const HSTAB_VOLUME_BAND: (f64, f64) = (0.5, 1.2);

/// Fin volume-coefficient band the figure annotates `Vv` against: the range
/// Jenkinson, *Civil Jet Aircraft Design*, Ch. 6 (PDF p. 137) gives
/// for existing civil transports (0.04 to 0.12); Raymer Table 6.4 (printed p. 160) quotes 0.09
/// as the jet-transport typical value. A reading aid only.
const VSTAB_VOLUME_BAND: (f64, f64) = (0.04, 0.12);

/// Generate the control-surface layout and tail-volume sizing figure.
pub fn figure_control_surfaces(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let width = if report.airplane.wings.get(2).is_some() {
        1020.0
    } else {
        780.0
    };
    figure_control_surfaces_sized(report, config, theme, (width, 660.0))
}

/// Room around the panels, scene units: left of the top view (its tick labels
/// and axis title), above the panels (title, note and panel titles), below
/// them (tick labels and axis titles) down to the legend, between the two
/// panels and right of the last one.
const LEFT: f64 = 60.0;
const TOP: f64 = 70.0;
const TO_LEGEND: f64 = 70.0;
const PANEL_GAP: f64 = 90.0;
const RIGHT: f64 = 24.0;
/// Smallest text of the figure (ticks, legend, the mass-method note), pt.
const SMALL_PT: f64 = 8.5;

/// The figure on a `canvas` of the caller's choosing (scene units): the two
/// panels share the room the text leaves, the text keeps its size, and the
/// legend wraps onto more rows (the scene growing taller) when it is longer
/// than the canvas is wide.
pub fn figure_control_surfaces_sized(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
    canvas: (f64, f64),
) -> Scene {
    let pal = get_palette(theme);
    let plane = &report.airplane;

    if plane.wings.is_empty() {
        return super::super::status_scene(
            "Control Surfaces & Tail Sizing",
            "No wing geometry available.",
            pal,
        );
    }

    let cs = &config.control_surfaces;
    let wing = &plane.wings[0];
    let hstab = plane.wings.get(1);
    let vstab = plane.wings.get(2);

    let (width, height) = (canvas.0.max(360.0), canvas.1.max(360.0));
    let mut scene = Scene::new(width, height, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Control Surfaces & Tail Sizing".to_owned());
    draw_title(&mut scene, "Control Surfaces & Tail Sizing", pal);
    scene.suppress_derived_title();
    // The note wraps onto a second line on a narrow canvas; the panels move
    // down by that line.
    let note = crate::families::mass_balance::mass_method_note(config);
    let note_chars = ((width - 24.0) / (SMALL_PT * CSS_PIXELS_PER_POINT * 0.56)).max(8.0);
    let note = crate::chart_kit::wrap_text(note, note_chars as usize);
    let note_lines = note.lines().count().max(1) as f64;
    let top = TOP + (note_lines - 1.0) * SMALL_PT * CSS_PIXELS_PER_POINT * 1.2;
    scene.add(SceneElement::Text {
        text: note,
        pos: [width * 0.5, 31.0],
        font_size: SMALL_PT,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });

    let panel_h = (height - top - TO_LEGEND - 60.0).max(160.0);
    let (top_w, side_w) = if vstab.is_some() {
        let room = (width - LEFT - PANEL_GAP - RIGHT).max(200.0);
        (room * 560.0 / 810.0, room * 250.0 / 810.0)
    } else {
        ((width - LEFT - RIGHT).max(160.0), 0.0)
    };
    let (y_min, y_max, x_min, x_max) = planform_bounds(plane);
    let top_axes = TopAxes::new((LEFT, top, top_w, panel_h), (y_min, y_max), (x_min, x_max));

    draw_planform_fill(
        &mut scene,
        &top_axes,
        plane,
        Color::rgba(176, 190, 197, 128),
    );
    top_axes
        .0
        .draw_frame_with_labels(&mut scene, pal, "Span Y [m]", "Longitudinal X [m]");

    let mut legend: Vec<(String, Color)> = Vec::new();
    let wing_semi = wing.projected_span() / 2.0;
    // A zero chord fraction states the aircraft has no such surface.
    if cs.slat_chord_fraction > 0.0 {
        draw_top_patch(
            &mut scene,
            &top_axes,
            &mut legend,
            &wing.xsecs,
            cs.slat_span_start_frac * wing_semi,
            cs.slat_span_end_frac * wing_semi,
            0.0,
            cs.slat_chord_fraction,
            "Slat",
            wing.symmetric,
        );
    }
    // One patch per flap band: an inboard aileron between two flaps is not flap.
    for (start, end) in cs.flap_bands() {
        draw_top_patch(
            &mut scene,
            &top_axes,
            &mut legend,
            &wing.xsecs,
            start * wing_semi,
            end * wing_semi,
            1.0 - cs.flap_chord_fraction,
            1.0,
            "Flap",
            wing.symmetric,
        );
    }
    draw_top_patch(
        &mut scene,
        &top_axes,
        &mut legend,
        &wing.xsecs,
        cs.aileron_span_start_frac * wing_semi,
        cs.aileron_span_end_frac * wing_semi,
        1.0 - cs.aileron_chord_fraction,
        1.0,
        "Aileron",
        wing.symmetric,
    );
    if cs.inboard_aileron_chord_fraction > 0.0 {
        draw_top_patch(
            &mut scene,
            &top_axes,
            &mut legend,
            &wing.xsecs,
            cs.inboard_aileron_span_start_frac * wing_semi,
            cs.inboard_aileron_span_end_frac * wing_semi,
            1.0 - cs.inboard_aileron_chord_fraction,
            1.0,
            "Aileron",
            wing.symmetric,
        );
    }
    let spoiler_hi = 1.0 - cs.flap_chord_fraction;
    if cs.spoiler_chord_fraction > 0.0 {
        draw_top_patch(
            &mut scene,
            &top_axes,
            &mut legend,
            &wing.xsecs,
            cs.spoiler_span_start_frac * wing_semi,
            cs.spoiler_span_end_frac * wing_semi,
            spoiler_hi - cs.spoiler_chord_fraction,
            spoiler_hi,
            "Spoiler",
            wing.symmetric,
        );
    }
    if let Some(hstab) = hstab {
        let hstab_semi = hstab.projected_span() / 2.0;
        draw_top_patch(
            &mut scene,
            &top_axes,
            &mut legend,
            &hstab.xsecs,
            cs.elevator_span_start_frac * hstab_semi,
            cs.elevator_span_end_frac * hstab_semi,
            1.0 - cs.elevator_chord_fraction,
            1.0,
            "Elevator",
            hstab.symmetric,
        );
    }

    scene.add(SceneElement::Text {
        text: "Top view".to_owned(),
        pos: [top_axes.0.left, top_axes.0.top - 10.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
    if let Some(vstab) = vstab {
        let rect = (LEFT + top_w + PANEL_GAP, top, side_w, panel_h);
        draw_vstab_side_view(&mut scene, vstab, cs, &mut legend, rect, pal);
    }

    let legend_entries = legend
        .iter()
        .map(|(n, c)| (n.clone(), LegendMarker::Patch(*c)))
        .collect::<Vec<_>>();
    let legend_y = top + panel_h + TO_LEGEND;
    let below_legend = draw_horizontal_legend(&mut scene, [LEFT, legend_y], &legend_entries, pal);

    let (vh, vv) = tail_volume_coefficients(plane);
    let info = [
        fmt_volume_coef("V_H", vh, HSTAB_VOLUME_BAND.0, HSTAB_VOLUME_BAND.1),
        fmt_volume_coef("V_V", vv, VSTAB_VOLUME_BAND.0, VSTAB_VOLUME_BAND.1),
    ];
    let bottom = draw_info_text(&mut scene, &info, [LEFT, below_legend + 14.0], pal);
    scene.height = scene.height.max(bottom + 16.0);

    scene
}

/// Width of `text` at `font_pt`, scene units: the scene has no text
/// measurement, so a mean advance of 0.56 em is assumed.
fn text_width(text: &str, font_pt: f64) -> f64 {
    text.chars().count() as f64 * font_pt * CSS_PIXELS_PER_POINT * 0.56
}

/// The v-stab side view (X horizontal, Z vertical, not inverted): its own
/// filled outline plus the rudder patch.
fn draw_vstab_side_view(
    scene: &mut Scene,
    vstab: &Wing,
    cs: &ControlSurfacesConfig,
    legend: &mut Vec<(String, Color)>,
    rect: (f64, f64, f64, f64),
    pal: &'static Palette,
) {
    let z_vals: Vec<f64> = vstab.xsecs.iter().map(|xs| xs.xyz_le[2]).collect();
    let le: Vec<f64> = vstab.xsecs.iter().map(|xs| xs.xyz_le[0]).collect();
    let te: Vec<f64> = vstab
        .xsecs
        .iter()
        .map(|xs| xs.xyz_le[0] + xs.chord)
        .collect();

    let x_min = le
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min)
        .min(te.iter().cloned().fold(f64::INFINITY, f64::min));
    let x_max = le
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max)
        .max(te.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    let z_lo = z_vals.iter().cloned().fold(f64::INFINITY, f64::min);
    let z_hi = z_vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let axes =
        Axes2D::new(rect, (x_min - 1.0, x_max + 1.0), (z_lo - 1.0, z_hi + 1.0)).with_equal_aspect();
    axes.draw_frame_with_labels(scene, pal, "Longitudinal X [m]", "Height Z [m]");

    let mut fill_pts: Vec<(f64, f64)> = le.iter().zip(&z_vals).map(|(&x, &z)| (x, z)).collect();
    fill_pts.extend(
        te.iter()
            .rev()
            .zip(z_vals.iter().rev())
            .map(|(&x, &z)| (x, z)),
    );
    scene
        .data_trace
        .name_next_polygon(format!("{} planform", vstab.name));
    scene.add(SceneElement::Polygon {
        points: fill_pts
            .iter()
            .map(|&(x, z)| axes.map_point(x, z))
            .collect(),
        fill: Some(Fill::new(Color::rgba(176, 190, 197, 128))),
        stroke: Some(Stroke::new(Color::from_hex("#616a6e"), 1.0)),
    });

    let rz0 = z_lo + cs.rudder_span_start_frac * (z_hi - z_lo);
    let rz1 = z_lo + cs.rudder_span_end_frac * (z_hi - z_lo);
    let poly = cs_surface_patch(
        &vstab.xsecs,
        2,
        rz0,
        rz1,
        1.0 - cs.rudder_chord_fraction,
        1.0,
    );
    let side_pts: Vec<Point2D> = poly.iter().map(|&(z, x)| axes.map_point(x, z)).collect();
    let rudder = super::geometry::surface_entry(legend, "Rudder");
    scene.data_trace.name_next_polygon("Rudder".to_owned());
    scene.add(SceneElement::Polygon {
        points: side_pts,
        fill: Some(Fill::new(rudder)),
        stroke: Some(Stroke::new(Color::rgb(0, 0, 0), 0.6)),
    });
    // Computed and discarded, matching upstream's own dead `rows.append` on
    // the rudder, see the module doc.
    let _ = cs_surface_area(
        &vstab.xsecs,
        2,
        rz0,
        rz1,
        1.0 - cs.rudder_chord_fraction,
        1.0,
        vstab.symmetric,
    );

    scene.add(SceneElement::Text {
        text: "V-stab side view".to_owned(),
        pos: [axes.left, axes.top - 10.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
}

/// Place the computed Vh/Vv values in one row below the surface legend, or
/// one under the other when the row would be wider than the canvas, without a
/// high-contrast box that competes with the geometry. Returns the y below the
/// last line.
fn draw_info_text(
    scene: &mut Scene,
    lines: &[String],
    pos: [f64; 2],
    pal: &'static Palette,
) -> f64 {
    const FONT: f64 = 9.5;
    const GAP: f64 = 28.0;
    let line_h = FONT * CSS_PIXELS_PER_POINT * 1.3;
    let row: f64 = lines.iter().map(|line| text_width(line, FONT) + GAP).sum();
    let stacked = pos[0] + row > scene.width - RIGHT;
    let (mut x, mut y) = (pos[0], pos[1]);
    for line in lines {
        scene.add(SceneElement::Text {
            text: line.clone(),
            pos: [x, y],
            font_size: FONT,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
        if stacked {
            y += line_h;
        } else {
            x += text_width(line, FONT) + GAP;
        }
    }
    if stacked {
        y - line_h * 0.5
    } else {
        y + line_h * 0.5
    }
}

/// The surface legend: swatch and label per surface, left to right with a
/// clear gap between entries, onto a new row when the next entry would pass
/// the canvas's right margin. Returns the y below the last row.
fn draw_horizontal_legend(
    scene: &mut Scene,
    pos: [f64; 2],
    entries: &[(String, LegendMarker)],
    pal: &'static Palette,
) -> f64 {
    let font_size = SMALL_PT;
    let row_h = font_size * CSS_PIXELS_PER_POINT * 1.5;
    let (mut x, mut y) = (pos[0], pos[1]);
    for (label, marker) in entries {
        let entry_w = 20.0 + text_width(label, font_size);
        if x > pos[0] && x + entry_w > scene.width - RIGHT {
            x = pos[0];
            y += row_h;
        }
        let mid_y = y + font_size * 0.5;
        match marker {
            LegendMarker::Patch(color) => scene.add(SceneElement::Rect {
                x,
                y,
                width: 14.0,
                height: font_size,
                rx: 1.0,
                fill: Some(Fill::new(*color)),
                stroke: None,
            }),
            LegendMarker::Circle(color) => scene.add(SceneElement::Circle {
                center: [x + 7.0, mid_y],
                radius: 5.0,
                fill: Some(Fill::new(*color)),
                stroke: None,
            }),
            LegendMarker::Line(stroke) => scene.add(SceneElement::Line {
                p1: [x, mid_y],
                p2: [x + 18.0, mid_y],
                stroke: stroke.clone(),
            }),
        }
        scene.add(SceneElement::Text {
            text: label.clone(),
            pos: [x + 20.0, mid_y],
            font_size,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
        x += entry_w + 24.0;
    }
    y + row_h
}
