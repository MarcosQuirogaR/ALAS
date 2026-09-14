// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native MSES flow-field pressure-coefficient contour rendering.

use alas_aero::mses::MsesPressureResult;
use alas_geom::aircraft::airfoil::Airfoil;

use super::{native_grid_boundary, unavailable};
use crate::chart_kit::draw_colorbar;
use crate::colormap::Colormap;
use crate::families::aerodynamics::support::padded_range;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

/// Render the pressure coefficient exported by MPlot option 11.
///
/// The field is drawn using the native row topology retained by the MSES
/// parser. Samples without a finite Cp remain visible to the Mach figure but
/// are excluded here; this keeps missing columns from becoming fabricated
/// pressure data.
pub fn figure_mses_cp_contours(
    result: &MsesPressureResult,
    airfoil: Option<&Airfoil>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(760.0, 500.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some(format!(
        "MSES Cp Field (alpha = {:.2} deg)",
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
    if result.field_x.is_empty() || result.field_y.is_empty() || result.field_cp.is_empty() {
        return unavailable(theme, "MSES produced no Cp field export");
    }

    let field = result
        .field_x
        .iter()
        .zip(&result.field_y)
        .zip(&result.field_cp)
        .filter_map(|((&x, &y), &cp)| {
            (x.is_finite() && y.is_finite() && cp.is_finite()).then_some((x, y, cp))
        })
        .collect::<Vec<_>>();
    if field.len() < 4 {
        return unavailable(theme, "MSES produced no finite Cp field samples");
    }

    let domain = {
        let mut points = field.iter();
        let &(first_x, first_y, _) = points.next().unwrap_or(&(0.0, 0.0, 0.0));
        let mut bounds = (first_x, first_x, first_y, first_y);
        for &(x, y, _) in points {
            bounds.0 = bounds.0.min(x);
            bounds.1 = bounds.1.max(x);
            bounds.2 = bounds.2.min(y);
            bounds.3 = bounds.3.max(y);
        }
        (bounds.0, bounds.1, bounds.2, bounds.3)
    };
    let cp_range = padded_range(field.iter().map(|point| point.2), 0.0);
    let cp_limit = cp_range.0.abs().max(cp_range.1.abs()).max(0.05);
    let cp_range = (-cp_limit, cp_limit);
    let axes = Axes2D::new(
        (65.0, 45.0, 590.0, 380.0),
        (domain.0, domain.1),
        (domain.2, domain.3),
    );
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
            && index < result.field_y.len()
            && index < result.field_cp.len()
            && result.field_x[index].is_finite()
            && result.field_y[index].is_finite()
            && result.field_cp[index].is_finite()
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
                .map(|&index| result.field_cp[index])
                .sum::<f64>()
                / 4.0;
            let normalized =
                ((value - cp_range.0) / (cp_range.1 - cp_range.0).max(1e-12)).clamp(0.0, 1.0);
            scene.add(SceneElement::Polygon {
                points: indices
                    .iter()
                    .map(|&index| axes.map_point(result.field_x[index], result.field_y[index]))
                    .collect(),
                fill: Some(Fill::new(Colormap::Jet.sample(normalized))),
                stroke: None,
            });
            native_cells += 1;
        }
    }
    if native_cells == 0 {
        for &(x, y, value) in &field {
            let normalized =
                ((value - cp_range.0) / (cp_range.1 - cp_range.0).max(1e-12)).clamp(0.0, 1.0);
            scene.add(SceneElement::Circle {
                center: axes.map_point(x, y),
                radius: 2.4,
                fill: Some(Fill::new(Colormap::Jet.sample(normalized))),
                stroke: None,
            });
        }
    }

    // The boundary helper uses Mach validity, which is appropriate for the
    // normal MSES export where both scalar columns are present. It also keeps
    // this figure consistent with the Mach figure's exact native-grid outline.
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
        Colormap::Jet,
        cp_range.0,
        cp_range.1,
        "Cp",
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
