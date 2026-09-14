// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_mses_*`)
// Reference: alas @ rust-port-baseline.

//! MSES surface distributions and sampled flow-field views.

use alas_aero::mses::MsesPressureResult;
use alas_geom::aircraft::airfoil::Airfoil;

use super::support::padded_range;
use crate::chart_kit::{draw_colorbar, draw_legend, LegendMarker};
use crate::colormap::Colormap;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

mod convergence;
mod cp;
pub use convergence::figure_mses_convergence;
pub use cp::figure_mses_cp_contours;

pub(super) fn panel_title(
    scene: &mut Scene,
    axes: &Axes2D,
    text: &str,
    pal: &crate::theme::Palette,
) {
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos: [axes.left, axes.top - 10.0],
        font_size: 10.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
}

pub(super) fn unavailable(theme: Option<&str>, reason: &str) -> Scene {
    let pal = get_palette(theme);
    const MESSAGE_TOP: f64 = 82.0;
    const LINE_HEIGHT: f64 = 17.0;
    const BOTTOM_MARGIN: f64 = 16.0;
    let wrapped = crate::chart_kit::wrap_text(reason, 100);
    let line_count = wrapped.lines().count().max(1) as f64;
    let height = (420.0_f64).max(MESSAGE_TOP + line_count * LINE_HEIGHT + BOTTOM_MARGIN);
    let mut scene = Scene::new(760.0, height, Some(Color::from_hex(pal.bg)));
    scene.title = Some("MSES figure unavailable".to_owned());
    scene.suppress_derived_title();
    scene.add(SceneElement::Text {
        text: "MSES figure unavailable".to_owned(),
        pos: [24.0, 34.0],
        font_size: 16.0,
        color: Color::from_hex("#c0392b"),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });
    scene.add(SceneElement::Text {
        text: wrapped,
        pos: [24.0, MESSAGE_TOP],
        font_size: 12.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_mses_scene_has_one_red_title_and_preserves_reason() {
        let scene = unavailable(Some("grey"), "solver diagnostics");
        assert_eq!(scene.title.as_deref(), Some("MSES figure unavailable"));
        assert!(!scene.render_title);
        assert_eq!(
            scene
                .elements
                .iter()
                .filter(|element| matches!(
                    element,
                    SceneElement::Text { text, color, bold: true, .. }
                        if text == "MSES figure unavailable"
                            && *color == Color::from_hex("#c0392b")
                ))
                .count(),
            1
        );
        assert!(scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, .. } if text == "solver diagnostics"
        )));
    }
}

/// Trace the outer boundary of the structured grid that MPlot actually
/// exported.  The grid is generally a curved quadrilateral, not the enclosing
/// Cartesian axes rectangle.  Drawing that boundary prevents the unsolved
/// exterior from being mistaken for a black, missing contour region.
fn native_grid_boundary(result: &MsesPressureResult) -> Vec<usize> {
    let row_end = |row: usize| {
        result
            .field_row_offsets
            .get(row + 1)
            .copied()
            .unwrap_or(result.field_x.len())
    };
    let valid = |index: usize| {
        index < result.field_x.len()
            && result.field_x[index].is_finite()
            && result.field_y[index].is_finite()
            && result.field_mach[index].is_finite()
    };
    let rows = result
        .field_row_offsets
        .iter()
        .enumerate()
        .map(|(row, &start)| {
            (start..row_end(row))
                .filter(|&index| valid(index))
                .collect::<Vec<_>>()
        })
        .filter(|row| row.len() >= 2)
        .collect::<Vec<_>>();
    if rows.len() < 2 {
        return Vec::new();
    }

    let mut boundary = rows[0].clone();
    boundary.extend(rows.iter().skip(1).filter_map(|row| row.last().copied()));
    boundary.extend(
        rows.last()
            .into_iter()
            .flat_map(|row| row.iter().rev().skip(1).copied()),
    );
    boundary.extend(
        rows.iter()
            .enumerate()
            .rev()
            .filter(|(row, _)| *row > 0 && *row + 1 < rows.len())
            .filter_map(|(_, row)| row.first().copied()),
    );
    boundary
}

/// Plot upper/lower surface pressure and local-Mach distributions.
pub fn figure_mses_pressure_distribution(
    result: &MsesPressureResult,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    if !result.is_valid_for_presentation() || result.x_upper.is_empty() || result.x_lower.is_empty()
    {
        return unavailable(
            theme,
            result
                .error
                .as_deref()
                .or(result.osmap_diagnostic.as_deref())
                .unwrap_or("MSES data unavailable"),
        );
    }
    let mut scene = Scene::new(900.0, 420.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some(format!(
        "MSES Root Section (alpha = {:.2} deg)",
        result.alpha_deg
    ));
    let x = padded_range(
        result
            .x_upper
            .iter()
            .copied()
            .chain(result.x_lower.iter().copied()),
        0.05,
    );
    let cp = padded_range(
        result
            .cp_upper
            .iter()
            .copied()
            .chain(result.cp_lower.iter().copied()),
        0.08,
    );
    let mach = padded_range(
        result
            .mach_upper
            .iter()
            .copied()
            .chain(result.mach_lower.iter().copied())
            .chain([1.0]),
        0.08,
    );
    let cp_axes = Axes2D::new((60.0, 55.0, 370.0, 300.0), x, (cp.1, cp.0));
    let mach_axes = Axes2D::new((480.0, 55.0, 370.0, 300.0), x, mach);
    cp_axes.draw_frame_with_labels(&mut scene, pal, "x/c", "Cp");
    mach_axes.draw_frame_with_labels(&mut scene, pal, "x/c", "Local Mach");
    panel_title(&mut scene, &cp_axes, "Surface pressure coefficient", pal);
    panel_title(&mut scene, &mach_axes, "Surface local Mach", pal);
    let upper = Stroke::new(Color::from_hex("tab:blue"), 1.6);
    let lower = Stroke::new(Color::from_hex("tab:red"), 1.6);
    cp_axes.add_line_series(
        &mut scene,
        &result
            .x_upper
            .iter()
            .copied()
            .zip(result.cp_upper.iter().copied())
            .collect::<Vec<_>>(),
        upper.clone(),
    );
    for &(x, y) in &result
        .x_upper
        .iter()
        .copied()
        .zip(result.cp_upper.iter().copied())
        .collect::<Vec<_>>()
    {
        scene.add(SceneElement::Circle {
            center: cp_axes.map_point(x, y),
            radius: 2.0,
            fill: Some(Fill::new(Color::from_hex("tab:blue"))),
            stroke: None,
        });
    }
    cp_axes.add_line_series(
        &mut scene,
        &result
            .x_lower
            .iter()
            .copied()
            .zip(result.cp_lower.iter().copied())
            .collect::<Vec<_>>(),
        lower.clone(),
    );
    for (&x, &y) in result.x_lower.iter().zip(&result.cp_lower) {
        scene.add(SceneElement::Circle {
            center: cp_axes.map_point(x, y),
            radius: 2.0,
            fill: Some(Fill::new(Color::from_hex("tab:red"))),
            stroke: None,
        });
    }
    cp_axes.add_line_series(
        &mut scene,
        &[(x.0, 0.0), (x.1, 0.0)],
        Stroke::new(Color::from_hex(pal.border), 0.7),
    );
    mach_axes.add_line_series(
        &mut scene,
        &result
            .x_upper
            .iter()
            .copied()
            .zip(result.mach_upper.iter().copied())
            .collect::<Vec<_>>(),
        upper.clone(),
    );
    for (&x, &y) in result.x_upper.iter().zip(&result.mach_upper) {
        scene.add(SceneElement::Circle {
            center: mach_axes.map_point(x, y),
            radius: 2.0,
            fill: Some(Fill::new(Color::from_hex("tab:blue"))),
            stroke: None,
        });
    }
    mach_axes.add_line_series(
        &mut scene,
        &result
            .x_lower
            .iter()
            .copied()
            .zip(result.mach_lower.iter().copied())
            .collect::<Vec<_>>(),
        lower.clone(),
    );
    for (&x, &y) in result.x_lower.iter().zip(&result.mach_lower) {
        scene.add(SceneElement::Circle {
            center: mach_axes.map_point(x, y),
            radius: 2.0,
            fill: Some(Fill::new(Color::from_hex("tab:red"))),
            stroke: None,
        });
    }
    mach_axes.add_line_series(
        &mut scene,
        &[(x.0, 1.0), (x.1, 1.0)],
        Stroke::dashed(Color::from_hex(pal.title), 1.0, 4.0, 3.0),
    );
    let legend = vec![
        ("Upper surface".to_owned(), LegendMarker::Line(upper)),
        ("Lower surface".to_owned(), LegendMarker::Line(lower)),
        (
            "M = 1 (sonic)".to_owned(),
            LegendMarker::Line(Stroke::dashed(Color::from_hex(pal.title), 1.0, 4.0, 3.0)),
        ),
    ];
    draw_legend(
        &mut scene,
        [mach_axes.left + 8.0, mach_axes.top + 8.0],
        &legend,
        pal,
        8.0,
    );
    scene
}

/// Render sampled MSES Mach data and the exact panelled section outline.
pub fn figure_mses_mach_contours(
    result: &MsesPressureResult,
    airfoil: Option<&Airfoil>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(760.0, 500.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some(format!(
        "MSES Mach Field (alpha = {:.2} deg)",
        result.alpha_deg
    ));
    if !result.is_valid_for_presentation() {
        return unavailable(
            theme,
            result
                .error
                .as_deref()
                .or(result.osmap_diagnostic.as_deref())
                .unwrap_or("MSES data unavailable"),
        );
    }
    if result.field_x.is_empty() || result.field_y.is_empty() || result.field_mach.is_empty() {
        return unavailable(theme, "MSES produced no Mach field export");
    }
    let Some(domain) = result.flowfield_domain() else {
        return unavailable(theme, "MSES produced no finite Mach field samples");
    };
    // MSET derives the far-field grid. Show the complete finite MPlot domain
    // rather than imposing an unrelated plotting crop, so the visible field
    // is exactly the domain the solver exported.
    let field = result
        .field_x
        .iter()
        .zip(&result.field_y)
        .zip(&result.field_mach)
        .filter_map(|((&x, &y), &mach)| {
            (x.is_finite() && y.is_finite() && mach.is_finite()).then_some((x, y, mach))
        })
        .collect::<Vec<_>>();
    if field.len() < 4 {
        return unavailable(theme, "MSES produced no finite Mach field samples");
    }
    // Use the solver's complete finite domain as the plotting domain. Padding
    // or equal-aspect expansion leaves uncoloured strips around native cells,
    // which look like missing Mach data in the exported figure.
    let x = (domain.x_min, domain.x_max);
    let y = (domain.y_min, domain.y_max);
    let m = padded_range(field.iter().map(|point| point.2), 0.0);
    let axes = Axes2D::new((65.0, 45.0, 590.0, 380.0), x, y);
    // The axes rectangle is intentionally a neutral panel rather than the
    // scene background. MPlot's outer grid is not rectangular, so its corners
    // have no solver values; giving them a distinct no-data treatment is more
    // truthful and legible than extending the nearest finite cell into them.
    scene.add(SceneElement::Rect {
        x: axes.left,
        y: axes.top,
        width: axes.width,
        height: axes.height,
        rx: 0.0,
        fill: Some(Fill::new(Color::from_hex(pal.panel))),
        stroke: None,
    });
    let in_view = |index: usize| {
        index < result.field_x.len()
            && result.field_x[index].is_finite()
            && result.field_y[index].is_finite()
            && result.field_mach[index].is_finite()
    };
    let mut native_cells = 0_usize;
    for (row_index, rows) in result.field_row_offsets.windows(2).enumerate() {
        let first_start = rows[0];
        let second_start = rows[1];
        let first_len = second_start.saturating_sub(first_start);
        let second_end = result
            .field_row_offsets
            .get(row_index + 2)
            .copied()
            .unwrap_or(result.field_x.len());
        let second_len = second_end.saturating_sub(second_start);
        for column in 0..first_len.min(second_len).saturating_sub(1) {
            let indices = [
                first_start + column,
                first_start + column + 1,
                second_start + column + 1,
                second_start + column,
            ];
            if !indices.iter().copied().all(in_view) {
                continue;
            }
            let value = indices
                .iter()
                .map(|&index| result.field_mach[index])
                .sum::<f64>()
                / 4.0;
            let normalized = ((value - m.0) / (m.1 - m.0).max(1e-12)).clamp(0.0, 1.0);
            let contour_level = (normalized * 39.0).round() / 39.0;
            scene.add(SceneElement::Polygon {
                points: indices
                    .iter()
                    .map(|&index| axes.map_point(result.field_x[index], result.field_y[index]))
                    .collect(),
                fill: Some(Fill::new(Colormap::Turbo.sample(contour_level))),
                stroke: None,
            });
            native_cells += 1;
        }
    }
    if native_cells == 0 {
        for &(px, py, value) in &field {
            let color = Colormap::Turbo.sample((value - m.0) / (m.1 - m.0).max(1e-12));
            scene.add(SceneElement::Circle {
                center: axes.map_point(px, py),
                radius: 2.4,
                fill: Some(Fill::new(color)),
                stroke: None,
            });
        }
    }
    let native_boundary = native_grid_boundary(result);
    if native_boundary.len() >= 3 {
        scene.add(SceneElement::Polyline {
            points: native_boundary
                .iter()
                .map(|&index| axes.map_point(result.field_x[index], result.field_y[index]))
                .chain(
                    native_boundary
                        .first()
                        .into_iter()
                        .map(|&index| axes.map_point(result.field_x[index], result.field_y[index])),
                )
                .collect(),
            stroke: Stroke::dashed(Color::from_hex(pal.spine), 1.0, 3.0, 2.0),
        });
    }
    let outline = if !result.airfoil_x.is_empty() {
        result
            .airfoil_x
            .iter()
            .copied()
            .zip(result.airfoil_y.iter().copied())
            .collect::<Vec<_>>()
    } else {
        airfoil.map(|a| a.coordinates.to_vec()).unwrap_or_default()
    };
    if outline.len() > 1 {
        scene.add(SceneElement::Polygon {
            points: outline.iter().map(|&(x, y)| axes.map_point(x, y)).collect(),
            fill: Some(Fill::new(Color::from_hex("#d1d5db"))),
            stroke: Some(Stroke::new(Color::from_hex(pal.title), 1.4)),
        });
    }
    axes.draw_frame_with_labels(&mut scene, pal, "x/c", "y/c");
    draw_colorbar(
        &mut scene,
        (675.0, 70.0, 16.0, 300.0),
        Colormap::Turbo,
        m.0,
        m.1,
        "Mach",
        pal,
    );
    scene.add(SceneElement::Text {
        text: "Neutral area: outside the native MSES grid (not solved)".to_owned(),
        pos: [360.0, 474.0],
        font_size: 8.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}
