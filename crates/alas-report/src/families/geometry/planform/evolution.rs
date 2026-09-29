// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// Generate the design-evolution montage: filled planform overlays of
/// sampled valid designs from an optimization run, rebuilt from their design
/// vectors and coloured first (cool) to last (warm) evaluation:
/// `figure_design_evolution`. Returns `None` when the history has no valid
/// evaluation, matching upstream's early return.
pub fn figure_design_evolution(
    history: &OptimizationHistory,
    builder: &AircraftBuilder,
    max_samples: usize,
    theme: Option<&str>,
) -> Option<Scene> {
    let valid_dvs: Vec<&alas_config::design_variables::DesignVector> = history
        .design_vectors
        .iter()
        .zip(&history.valid)
        .filter_map(|(dv, &ok)| ok.then_some(dv))
        .collect();
    if valid_dvs.is_empty() {
        return None;
    }

    let idxs = linspace_int_indices(valid_dvs.len(), max_samples.min(valid_dvs.len()).max(1));
    let n = idxs.len().max(1);

    let planes: Vec<(f64, Airplane)> = idxs
        .iter()
        .enumerate()
        .filter_map(|(i, &idx)| {
            let progress = i as f64 / (n - 1).max(1) as f64;
            builder
                .build(Some(valid_dvs[idx]), false)
                .ok()
                .map(|plane| (progress, plane))
        })
        .collect();
    if planes.is_empty() {
        return None;
    }

    let pal = get_palette(theme);
    // The equal-aspect planform needs a little more footer than a standard
    // chart: its horizontal axis label is below the projected aircraft.
    let mut scene = Scene::new(760.0, 580.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Design evolution (planform)".to_owned());
    draw_title(&mut scene, "Design evolution (planform)", pal);
    scene.suppress_derived_title();

    let (mut x_min, mut x_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut y_min, mut y_max) = (f64::INFINITY, f64::NEG_INFINITY);
    for (_, plane) in &planes {
        let (x0, x1, y0, y1, _, _) = airplane_bbox(plane);
        x_min = x_min.min(x0);
        x_max = x_max.max(x1);
        y_min = y_min.min(y0);
        y_max = y_max.max(y1);
    }

    let rect = (60.0, 40.0, 640.0, 420.0);
    let (u_range, v_range) =
        equal_aspect_ranges(y_min, y_max, rect.2, -x_max, -x_min, rect.3, 0.15);
    let axes = Axes2D::new(rect, u_range, v_range);
    axes.draw_frame_with_labels(&mut scene, pal, "span Y [m]", "longitudinal X [m]");

    for (progress, plane) in &planes {
        let color = Colormap::Turbo.sample(*progress);
        draw_planform(
            &mut scene,
            &axes,
            plane,
            color,
            0.0,
            false,
            Some(0.05 + 0.18 * progress),
            true,
        );
    }

    draw_horizontal_progress_bar(
        &mut scene,
        (270.0, 520.0, 220.0, 14.0),
        Colormap::Turbo,
        0.0,
        1.0,
        "Evaluation progress [-]",
        pal,
    );

    Some(scene)
}
