// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_airfoil_reynolds`)

//! NeuralFoil coefficient maps over angle of attack and Reynolds number.

use alas_aero::neuralfoil::{self, Conditions, ModelSize};
use alas_geom::aircraft::airfoil::Airfoil;
use rayon::prelude::*;

use super::support::{add_heatmap_grid_nan_aware, cell_edges_linear, cell_edges_log, padded_range};
use crate::chart_kit::{draw_colorbar, draw_title};
use crate::colormap::Colormap;
use crate::scene::{Axes2D, Color, Scale, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

fn add_isograms(
    axes: &Axes2D,
    scene: &mut Scene,
    x: &[f64],
    y: &[f64],
    values: &[f64],
    levels: &[f64],
) {
    let nx = x.len();
    for &level in levels {
        let mut line = Vec::new();
        for row in 0..y.len() {
            let mut crossings = Vec::new();
            for col in 0..nx.saturating_sub(1) {
                let a = values[row * nx + col];
                let b = values[row * nx + col + 1];
                if !a.is_finite() || !b.is_finite() || (a - level) * (b - level) > 0.0 {
                    continue;
                }
                let denominator = b - a;
                if denominator.abs() < 1e-12 {
                    continue;
                }
                let t = ((level - a) / denominator).clamp(0.0, 1.0);
                crossings.push(x[col] + t * (x[col + 1] - x[col]));
            }
            if let Some(&crossing) = crossings.first() {
                line.push((crossing, y[row]));
            } else if line.len() > 1 {
                axes.add_line_series(
                    scene,
                    &line,
                    Stroke::new(Color::rgba(255, 255, 255, 150), 0.55),
                );
                line.clear();
            }
        }
        if line.len() > 1 {
            axes.add_line_series(
                scene,
                &line,
                Stroke::new(Color::rgba(255, 255, 255, 150), 0.55),
            );
        }
    }
}

fn linspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    if n < 2 {
        return vec![a];
    }
    (0..n)
        .map(|i| a + (b - a) * i as f64 / (n - 1) as f64)
        .collect()
}

fn logspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    linspace(a.log10(), b.log10(), n)
        .into_iter()
        .map(|v| 10.0_f64.powf(v))
        .collect()
}

/// Chord Reynolds number below which NeuralFoil cells are not drawn.
const LOW_RE_MASK: f64 = 1.0e5;

/// Hatch the columns of cells that are not drawn (Re below [`LOW_RE_MASK`]) so
/// the empty strip reads as masked, not as missing data.
fn hatch_masked_cells(
    scene: &mut Scene,
    axes: &Axes2D,
    x_edges: &[f64],
    y_edges: &[f64],
    masked_columns: usize,
    ink: &str,
) {
    if masked_columns == 0 {
        return;
    }
    let (Some(&x0), Some(&x1), Some(&y0), Some(&y1)) = (
        x_edges.first(),
        x_edges.get(masked_columns),
        y_edges.first(),
        y_edges.last(),
    ) else {
        return;
    };
    let a = axes.map_point(x0, y0);
    let b = axes.map_point(x1, y1);
    let (left, right) = (a[0].min(b[0]), a[0].max(b[0]));
    let (top, bottom) = (a[1].min(b[1]), a[1].max(b[1]));
    let base = Color::from_hex(ink);
    let stroke = Stroke::new(Color::rgba(base.r, base.g, base.b, 120), 0.6);
    let spacing = 6.0;
    let (width, height) = (right - left, bottom - top);
    // 45 degree lines rising to the right, clipped to the strip.
    let mut offset = spacing;
    while offset < width + height {
        let start_x = left + (offset - height).max(0.0);
        let start_y = bottom - (offset - (start_x - left));
        let end_x = left + offset.min(width);
        let end_y = bottom - (offset - (end_x - left));
        scene.add(SceneElement::Line {
            p1: [start_x, start_y],
            p2: [end_x, end_y],
            stroke: stroke.clone(),
        });
        offset += spacing;
    }
}

/// The footnote: which Re range is hatched and how many cells that is per map.
fn masked_note(reynolds: &[f64], alpha_count: usize) -> String {
    let masked: Vec<f64> = reynolds
        .iter()
        .copied()
        .filter(|&re| re < LOW_RE_MASK)
        .collect();
    let (Some(lo), Some(hi)) = (masked.first(), masked.last()) else {
        return "No cells are masked: every Re of the sweep is drawn.".to_owned();
    };
    format!(
        "Hatched: Re {lo:.1e} to {hi:.1e} ({} cells per map) is not drawn, unreliable low-Re surrogate range. The colour scale uses only the drawn cells.",
        masked.len() * alpha_count
    )
}

/// Generate the profile and four NeuralFoil coefficient maps used by the UI.
pub fn figure_airfoil_reynolds(airfoil: &Airfoil, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(900.0, 760.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some(format!("Airfoil Reynolds Sweep: {}", airfoil.name));
    let title = scene.title.clone().unwrap_or_default();
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();
    let alpha = linspace(-5.0, 12.0, 30);
    let reynolds = logspace(1e4, 1e7, 30);
    let mut fields = vec![vec![f64::NAN; alpha.len() * reynolds.len()]; 4];
    // Each cell is an independent NeuralFoil evaluation with no shared state,
    // so the 900 calls run on the rayon pool. `collect` on an indexed parallel
    // iterator keeps cell order, and each cell is computed exactly as in a
    // serial sweep, so the fields are bit-identical to the serial result.
    let columns = reynolds.len();
    let cells: Vec<Option<neuralfoil::NetworkAero>> = (0..alpha.len() * columns)
        .into_par_iter()
        .map(|i| {
            neuralfoil::aero_from_coordinates(
                &airfoil.coordinates,
                &Conditions::new(alpha[i / columns], reynolds[i % columns]),
                ModelSize::Large,
            )
            .ok()
        })
        .collect();
    for (i, cell) in cells.into_iter().enumerate() {
        // Low-Re cells are masked: the surrogate's laminar-separation
        // artefacts there would otherwise set the colour range.
        if reynolds[i % columns] < LOW_RE_MASK {
            continue;
        }
        if let Some(value) = cell {
            fields[0][i] = value.cl;
            fields[1][i] = value.cd;
            fields[2][i] = if value.cd > 0.0 {
                value.cl / value.cd
            } else {
                f64::NAN
            };
            fields[3][i] = value.cm;
        }
    }
    let profile_y = padded_range(airfoil.coordinates.iter().map(|&(_, y)| y), 0.12);
    let profile =
        Axes2D::new((65.0, 35.0, 770.0, 120.0), (0.0, 1.0), profile_y).with_equal_aspect();
    profile.draw_frame_with_labels(&mut scene, pal, "x/c", "");
    scene.add(SceneElement::Text {
        text: "y/c".to_owned(),
        pos: [15.0, profile.top + profile.height * 0.5],
        font_size: 10.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });
    profile.add_line_series(
        &mut scene,
        &airfoil.coordinates,
        Stroke::new(Color::from_hex("tab:blue"), 1.8),
    );
    scene.add(SceneElement::Text {
        text: "Airfoil profile".to_owned(),
        pos: [profile.left, profile.top - 7.0],
        font_size: 10.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });

    let x_edges = cell_edges_log(&reynolds);
    let masked_columns = reynolds.iter().filter(|&&re| re < LOW_RE_MASK).count();
    let y_edges = cell_edges_linear(&alpha);
    for (index, (label, cmap)) in [
        ("CL", Colormap::Plasma),
        ("CD", Colormap::Viridis),
        ("L/D", Colormap::Inferno),
        ("Cm", Colormap::Magma),
    ]
    .iter()
    .enumerate()
    {
        let col = index % 2;
        let row = index / 2;
        let axes = Axes2D::new(
            (
                65.0 + col as f64 * 440.0,
                205.0 + row as f64 * 255.0,
                315.0,
                190.0,
            ),
            (reynolds[0], *reynolds.last().unwrap_or(&1e7)),
            (alpha[0], *alpha.last().unwrap_or(&12.0)),
        )
        .with_x_scale(Scale::Log10);
        axes.draw_frame_with_labels(&mut scene, pal, "Re", "\u{03b1} [deg]");
        let range = padded_range(fields[index].iter().copied(), 0.0);
        add_heatmap_grid_nan_aware(
            &axes,
            &mut scene,
            &x_edges,
            &y_edges,
            &fields[index],
            *cmap,
            range.0,
            range.1,
        );
        let finite: Vec<f64> = fields[index]
            .iter()
            .copied()
            .filter(|v| v.is_finite())
            .collect();
        if finite.len() > 3 {
            let (lo, hi) = padded_range(finite.iter().copied(), 0.0);
            let levels: Vec<f64> = (1..=4).map(|n| lo + (hi - lo) * n as f64 / 5.0).collect();
            add_isograms(
                &axes,
                &mut scene,
                &reynolds,
                &alpha,
                &fields[index],
                &levels,
            );
        }
        hatch_masked_cells(
            &mut scene,
            &axes,
            &x_edges,
            &y_edges,
            masked_columns,
            pal.tick,
        );
        scene.add(SceneElement::Text {
            text: label.to_string(),
            pos: [axes.left, axes.top - 7.0],
            font_size: 10.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Left,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
        draw_colorbar(
            &mut scene,
            (
                axes.left + axes.width + 12.0,
                axes.top + 15.0,
                12.0,
                axes.height - 30.0,
            ),
            *cmap,
            range.0,
            range.1,
            label,
            pal,
        );
    }
    scene.add(SceneElement::Text {
        text: masked_note(&reynolds, alpha.len()),
        pos: [450.0, 750.0],
        font_size: 8.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}
