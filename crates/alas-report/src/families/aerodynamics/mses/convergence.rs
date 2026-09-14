// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! MSES sweep-convergence figure, split out of `mses.rs` to stay under the
//! per-file line budget.

use alas_aero::mses::{MsesPolarPointStatus, MsesPolarResult};

use super::super::support::padded_range;
use crate::chart_kit::draw_axes_without_x_tick_labels;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke};
use crate::theme::get_palette;

/// Show the converged samples and the requested-point verdicts of an MSES
/// sweep.  Missing coefficients are intentionally not interpolated: a
/// partial run is evidence of the points that converged, not a complete polar.
pub fn figure_mses_convergence(result: &MsesPolarResult, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    if !result.has_usable_data() && result.point_diagnostics.is_empty() {
        return super::unavailable(theme, "MSES sweep diagnostics unavailable.");
    }

    let converged = result
        .alpha_deg
        .iter()
        .zip(&result.cl)
        .zip(&result.cd)
        .zip(&result.cm)
        .filter_map(|(((&alpha, &cl), &cd), &cm)| {
            (alpha.is_finite() && cl.is_finite() && cd.is_finite() && cm.is_finite())
                .then_some((alpha, cl, cd, cm))
        })
        .collect::<Vec<_>>();
    let requested = if result.point_diagnostics.is_empty() {
        converged
            .iter()
            .map(|&(alpha, _, _, _)| (alpha, MsesPolarPointStatus::Converged))
            .collect::<Vec<_>>()
    } else {
        result
            .point_diagnostics
            .iter()
            .filter_map(|diagnostic| {
                diagnostic
                    .requested_alpha_deg
                    .is_finite()
                    .then_some((diagnostic.requested_alpha_deg, diagnostic.status))
            })
            .collect::<Vec<_>>()
    };
    if requested.is_empty() && converged.is_empty() {
        return super::unavailable(theme, "MSES retained no finite sweep points.");
    }

    let alpha = padded_range(
        requested
            .iter()
            .map(|&(alpha, _)| alpha)
            .chain(converged.iter().map(|&(alpha, _, _, _)| alpha)),
        0.08,
    );
    let cl = padded_range(converged.iter().map(|&(_, cl, _, _)| cl), 0.08);
    let cd = padded_range(converged.iter().map(|&(_, _, cd, _)| cd), 0.08);
    let cm = padded_range(converged.iter().map(|&(_, _, _, cm)| cm), 0.08);
    let axes = [
        Axes2D::new((60.0, 55.0, 370.0, 235.0), alpha, cl).with_y_tick_decimals(2),
        Axes2D::new((480.0, 55.0, 370.0, 235.0), alpha, cd).with_y_tick_decimals(2),
        Axes2D::new((60.0, 350.0, 370.0, 235.0), alpha, cm).with_y_tick_decimals(2),
        Axes2D::new((480.0, 350.0, 370.0, 235.0), alpha, (0.0, 1.2)).with_y_tick_decimals(2),
    ];
    let mut scene = Scene::new(900.0, 650.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("MSES Sweep Convergence".to_owned());
    for (index, (axis, (plot_title, y_label))) in axes
        .iter()
        .zip([
            ("Converged lift samples", "CL"),
            ("Converged drag samples", "CD"),
            ("Converged moment samples", "CM"),
            ("Requested-point status", "state"),
        ])
        .enumerate()
    {
        // The lower row carries the shared alpha label. Omitting duplicate
        // upper-row labels keeps the two plot rows visually compact.
        if index < 2 {
            draw_axes_without_x_tick_labels(axis, &mut scene, pal, None, Some(y_label));
        } else {
            axis.draw_frame_with_labels(&mut scene, pal, "alpha [deg]", y_label);
        }
        super::panel_title(&mut scene, axis, plot_title, pal);
    }

    let cl_points = converged
        .iter()
        .map(|&(alpha, cl, _, _)| (alpha, cl))
        .collect::<Vec<_>>();
    let cd_points = converged
        .iter()
        .map(|&(alpha, _, cd, _)| (alpha, cd))
        .collect::<Vec<_>>();
    let cm_points = converged
        .iter()
        .map(|&(alpha, _, _, cm)| (alpha, cm))
        .collect::<Vec<_>>();
    let converged_color = Color::from_hex("#009e73");
    let drag_color = Color::from_hex("#d55e00");
    let moment_color = Color::from_hex("#56b4e9");
    axes[0].add_line_series(&mut scene, &cl_points, Stroke::new(converged_color, 1.6));
    axes[1].add_line_series(&mut scene, &cd_points, Stroke::new(drag_color, 1.6));
    axes[2].add_line_series(&mut scene, &cm_points, Stroke::new(moment_color, 1.6));
    for &(alpha, cl) in &cl_points {
        scene.add(SceneElement::Circle {
            center: axes[0].map_point(alpha, cl),
            radius: 3.0,
            fill: Some(Fill::new(converged_color)),
            stroke: None,
        });
    }
    for &(alpha, cd) in &cd_points {
        scene.add(SceneElement::Circle {
            center: axes[1].map_point(alpha, cd),
            radius: 3.0,
            fill: Some(Fill::new(drag_color)),
            stroke: None,
        });
    }
    for &(alpha, cm) in &cm_points {
        scene.add(SceneElement::Circle {
            center: axes[2].map_point(alpha, cm),
            radius: 3.0,
            fill: Some(Fill::new(moment_color)),
            stroke: None,
        });
    }

    for &(alpha, status) in &requested {
        let (y, color) = match status {
            MsesPolarPointStatus::Converged => (1.0, Color::from_hex("#27ae60")),
            MsesPolarPointStatus::NotConverged => (0.35, Color::from_hex("#d62728")),
        };
        let center = axes[3].map_point(alpha, y);
        if status == MsesPolarPointStatus::Converged {
            scene.add(SceneElement::Circle {
                center,
                radius: 4.0,
                fill: Some(Fill::new(color)),
                stroke: None,
            });
        } else {
            let size = 4.0;
            scene.add(SceneElement::Line {
                p1: [center[0] - size, center[1] - size],
                p2: [center[0] + size, center[1] + size],
                stroke: Stroke::new(color, 1.8),
            });
            scene.add(SceneElement::Line {
                p1: [center[0] - size, center[1] + size],
                p2: [center[0] + size, center[1] - size],
                stroke: Stroke::new(color, 1.8),
            });
        }
    }
    scene
}
