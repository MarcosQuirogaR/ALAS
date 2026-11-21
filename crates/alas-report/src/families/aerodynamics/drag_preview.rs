// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/sidecar/figures.py: _preview_drag.

//! Solver-free drag preview for the live design editor.
//!
//! The Python side evaluates the same parasite and Korn wave-drag methods used
//! by the optimizer at thirty Mach stations. Keeping this as a report factory
//! makes the GUI use the production aerodynamic formulas without running a VLM
//! solve on every debounced edit.

use alas_aero::analysis::AeroAnalysis;
use alas_config::{design_variables::DesignVector, AlasConfig};
use alas_geom::aircraft::airplane::Airplane;

use crate::chart_kit::{draw_inline_legend, LegendMarker};
use crate::scene::{Axes2D, Color, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

/// The canvas the live drag chart is drawn on by default, scene units.
pub const DRAG_PREVIEW_CANVAS: (f64, f64) = (760.0, 420.0);

/// Build the live parasite and wave-drag versus Mach chart.
pub fn figure_drag_preview(
    airplane: &Airplane,
    config: &AlasConfig,
    design: &DesignVector,
    theme: Option<&str>,
) -> Scene {
    figure_drag_preview_sized(airplane, config, design, theme, DRAG_PREVIEW_CANVAS)
}

/// The drag chart on a `canvas` of the caller's choosing (scene units; the GUI
/// passes about the size it will draw it at, so the text keeps its point size).
pub fn figure_drag_preview_sized(
    airplane: &Airplane,
    config: &AlasConfig,
    design: &DesignVector,
    theme: Option<&str>,
    canvas: (f64, f64),
) -> Scene {
    let pal = get_palette(theme);
    let aero = AeroAnalysis::new(
        airplane,
        AeroAnalysis::quarter_chord_sweep_deg(airplane, design.sweep_deg),
        Some(config.geometry.clone()),
        Some(config.resolved_drag_model()),
        Some(config.analysis.clone()),
    );
    let altitude = config.requirements.cruise_altitude_m;
    let cl_ref = 0.5;
    let cruise = config.requirements.cruise_mach;
    // Drag-divergence Mach by its definition, dCDwave/dM = 0.1, on a fine grid.
    let fine: Vec<f64> = (0..=124).map(|i| 0.3 + f64::from(i) * 0.005).collect();
    let mut m_dd = 0.92;
    let mut previous: Option<(f64, f64)> = None;
    for &mach in &fine {
        let wave_cd = aero.wave_drag(mach, cl_ref, None);
        if let Some((m0, c0)) = previous {
            if (wave_cd - c0) / (mach - m0) >= 0.1 {
                m_dd = m0;
                break;
            }
        }
        previous = Some((mach, wave_cd));
    }
    let x_max = (cruise + 0.25).max(m_dd + 0.05).clamp(0.4, 0.92);
    let machs: Vec<f64> = (0..30)
        .map(|i| 0.3 + i as f64 * (x_max - 0.3) / 29.0)
        .collect();
    let parasite: Vec<(f64, f64)> = machs
        .iter()
        .map(|&mach| (mach, aero.parasite_drag(mach, altitude, cl_ref, None, None)))
        .collect();
    let wave: Vec<(f64, f64)> = machs
        .iter()
        .map(|&mach| (mach, aero.wave_drag(mach, cl_ref, None)))
        .collect();
    let parasite_max = parasite.iter().map(|p| p.1).fold(0.0, f64::max).max(1e-4);
    let data_max = parasite
        .iter()
        .chain(wave.iter())
        .map(|(_, cd)| *cd)
        .fold(0.0, f64::max)
        .max(1e-4);
    // Fit y to the plotted range; a curve far above the rest (more than three
    // times the parasite level) leaves the top and is marked, not truncated
    // silently.
    let max_cd = data_max.min(3.0 * parasite_max);
    let (width, height) = canvas;
    let mut scene = Scene::new(width, height, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Drag vs Mach (illustrative)".to_owned());
    // The page heading above the figure carries the title; the scene keeps its
    // title for the plot-data export and paints no second one.
    scene.suppress_derived_title();
    let tick_font = 11.0;
    // Room left of the plot for the widest tick label plus the rotated title.
    let (left, top, bottom, right) = (96.0, 40.0, 58.0, 24.0);
    let axes = Axes2D::new(
        (left, top, width - left - right, height - top - bottom),
        (0.3, x_max),
        (0.0, max_cd * 1.08),
    )
    .with_tick_font(tick_font);
    axes.draw_frame_with_labels(&mut scene, pal, "Mach", "Drag coefficient CD");
    let cyan = Color::from_hex("#00d8ff");
    let amber = Color::from_hex("#ff9900");
    axes.add_line_series(&mut scene, &parasite, Stroke::new(cyan, 1.8));
    axes.add_line_series(&mut scene, &wave, Stroke::new(amber, 1.8));
    let cruise_stroke = Stroke::dashed(Color::from_hex(pal.title), 1.0, 5.0, 4.0);
    scene.add(SceneElement::Line {
        p1: axes.map_point(cruise, 0.0),
        p2: axes.map_point(cruise, max_cd * 1.08),
        stroke: cruise_stroke.clone(),
    });
    // Clip arrows: where a curve crosses the top of the axes.
    let y_top = max_cd * 1.08;
    for (series, colour, name) in [(&parasite, cyan, "CD0"), (&wave, amber, "CD wave")] {
        let peak = series.iter().map(|p| p.1).fold(0.0, f64::max);
        if peak > y_top {
            let crossing = series
                .windows(2)
                .find(|w| w[0].1 <= y_top && w[1].1 > y_top)
                .map_or(series.last().map_or(x_max, |p| p.0), |w| {
                    w[0].0 + (w[1].0 - w[0].0) * (y_top - w[0].1) / (w[1].1 - w[0].1)
                });
            let tip = axes.map_point(crossing, y_top);
            scene.add(SceneElement::Polygon {
                points: vec![
                    [tip[0], tip[1] - 2.0],
                    [tip[0] - 5.0, tip[1] + 8.0],
                    [tip[0] + 5.0, tip[1] + 8.0],
                ],
                fill: Some(crate::scene::Fill::new(colour)),
                stroke: None,
            });
            scene.add(SceneElement::Text {
                text: format!("{name} off scale, up to {peak:.4}"),
                pos: [tip[0] - 8.0, tip[1] + 8.0],
                font_size: tick_font,
                color: colour,
                align: TextAlign::Right,
                baseline: TextBaseline::Top,
                angle_deg: 0.0,
                bold: true,
            });
        }
    }
    // The key sits above the plot, swatch and label per series; the operating
    // point's lift coefficient is a plain note at its right end.
    let key_y = top * 0.5;
    draw_inline_legend(
        &mut scene,
        [left, key_y],
        &[
            (
                "CD0 (parasite)".to_owned(),
                LegendMarker::Line(Stroke::new(cyan, 2.4)),
            ),
            (
                "CD wave".to_owned(),
                LegendMarker::Line(Stroke::new(amber, 2.4)),
            ),
            ("Cruise Mach".to_owned(), LegendMarker::Line(cruise_stroke)),
        ],
        pal,
        tick_font + 1.0,
    );
    scene.add(SceneElement::Text {
        text: format!("CL = {cl_ref:.2}"),
        pos: [width - right, key_y],
        font_size: tick_font + 1.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Right,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}

#[cfg(test)]
mod tests {
    use super::figure_drag_preview;
    use crate::scene::SceneElement;
    use alas_config::design_variables::DesignVector;
    use alas_config::AlasConfig;
    use alas_geom::builder::AircraftBuilder;

    #[test]
    fn drag_preview_contains_numeric_ticks_and_labeled_axes() {
        let config = AlasConfig::default();
        let design = DesignVector::default();
        let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap_or_else(|error| panic!("default drag preview geometry: {error}"));
        let scene = figure_drag_preview(&airplane, &config, &design, Some("dark"));
        let labels = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(labels.contains(&"Mach"));
        assert!(labels.contains(&"Drag coefficient CD"));
        let numeric_ticks = labels
            .iter()
            .filter(|label| label.parse::<f64>().is_ok())
            .count();
        assert!(
            numeric_ticks >= 4,
            "expected numeric x/y tick labels: {labels:?}"
        );

        let text_position = |wanted: &str| {
            scene.elements.iter().find_map(|element| match element {
                SceneElement::Text { text, pos, .. } if text == wanted => Some(*pos),
                _ => None,
            })
        };
        let x_label = text_position("Mach").unwrap_or_else(|| panic!("missing x-axis label"));
        let key = text_position("CD wave").unwrap_or_else(|| panic!("missing legend entry"));
        let y_label =
            text_position("Drag coefficient CD").unwrap_or_else(|| panic!("missing y title"));
        assert!(
            key[1] < scene.height * 0.2,
            "the key is above the plot: {key:?}"
        );
        assert!(
            x_label[1] > scene.height * 0.8,
            "the x title is below the ticks"
        );
        // The y title clears the widest tick label (the title is rotated, so
        // its x is the centre of the text line).
        let widest_tick_left = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text {
                    text,
                    pos,
                    font_size,
                    align: crate::scene::TextAlign::Right,
                    ..
                } if text.parse::<f64>().is_ok() => {
                    Some(pos[0] - text.chars().count() as f64 * font_size * 0.55)
                }
                _ => None,
            })
            .fold(f64::INFINITY, f64::min);
        assert!(
            y_label[0] + 8.0 < widest_tick_left,
            "the y title overlaps the tick labels: {y_label:?} vs {widest_tick_left}"
        );
        assert!(!scene.render_title);
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.starts_with("CL = "))
        }));
    }
}
