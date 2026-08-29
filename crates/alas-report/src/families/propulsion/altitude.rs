// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (figure_propulsion_altitude_sweep)
// Reference: alas @ rust-port-baseline.

//! Per-engine thrust and TSFC over the full (altitude, Mach) flight
//! envelope, as filled contours -- the closed-form, first-principles
//! stand-in for a semi-empirical installed-thrust-lapse table.
//!
//! Upstream draws each panel as `contourf` (20 filled levels) plus six
//! labelled black iso-lines from `ax.contour`. The filled field remains a
//! regular SVG heatmap, while the six iso-lines are reconstructed from that
//! same sampled grid so the exported chart retains the contour labels that
//! make an operating envelope readable at a glance.

use super::linspace;
use crate::chart_kit::{draw_colorbar, draw_legend, LegendMarker};
use crate::colormap::Colormap;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::{get_palette, Palette};
use alas_config::{AlasConfig, DesignRequirements};
use alas_prop::cycle::anchor_mass_flow_kg_s;
use alas_prop::cycle::sweeps::compute_altitude_sweep;

/// Per-engine thrust and TSFC as contours over the full (altitude, Mach)
/// flight envelope, holding the cycle design parameters fixed.
pub fn figure_propulsion_altitude_sweep(config: &AlasConfig, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(1000.0, 480.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Per-Engine Thrust and TSFC vs Altitude & Mach".to_owned());

    let eng = &config.geometry.engine;
    let cyc_cfg = &config.propulsion_cycle;
    let req = &config.requirements;

    let mach_vec = linspace(0.0, (0.9f64).max(req.cruise_mach * 1.15), 26);
    let alt_vec = linspace(0.0, 13_000.0, 26);
    let (mdot_total, _static) = anchor_mass_flow_kg_s(
        eng.thrust_kn,
        eng.overall_pressure_ratio,
        eng.fan_pressure_ratio,
        eng.bypass_ratio,
        eng.turbine_inlet_temp_k,
        cyc_cfg,
    );
    let sweep = compute_altitude_sweep(
        &alt_vec,
        &mach_vec,
        eng.bypass_ratio,
        eng.overall_pressure_ratio,
        eng.fan_pressure_ratio,
        eng.turbine_inlet_temp_k,
        mdot_total,
        cyc_cfg,
    );

    let alt_km: Vec<f64> = sweep.altitude_m.iter().map(|v| v / 1000.0).collect();
    let alt_edges = centers_to_edges(&alt_km);
    let mach_edges = centers_to_edges(&mach_vec);

    let thrust: Vec<Vec<f64>> = sweep
        .dimensional_thrust_kn
        .iter()
        .zip(&sweep.feasible_mask)
        .map(|(row, mask_row)| {
            row.iter()
                .zip(mask_row)
                .map(|(&v, &ok)| if ok { v } else { f64::NAN })
                .collect()
        })
        .collect();
    let tsfc: Vec<Vec<f64>> = sweep
        .tsfc_mg_ns
        .iter()
        .zip(&sweep.feasible_mask)
        .map(|(row, mask_row)| {
            row.iter()
                .zip(mask_row)
                .map(|(&v, &ok)| if ok { v } else { f64::NAN })
                .collect()
        })
        .collect();

    draw_contour_panel(
        &mut scene,
        pal,
        (50.0, 70.0, 420.0, 330.0),
        &alt_edges,
        &mach_edges,
        &thrust,
        Colormap::Inferno,
        "Thrust [kN]",
        req,
        "Per-Engine Thrust vs Altitude & Mach\n(anchored to rated static thrust)",
    );
    draw_contour_panel(
        &mut scene,
        pal,
        (560.0, 70.0, 420.0, 330.0),
        &alt_edges,
        &mach_edges,
        &tsfc,
        Colormap::Viridis,
        "TSFC [mg/(N.s)]",
        req,
        "TSFC vs Altitude & Mach",
    );

    scene
}

/// Convert `n` cell-center values (as `np.linspace` produces) into `n + 1`
/// cell-boundary edges for [`draw_masked_heatmap`], each interior edge the
/// midpoint of its two neighbouring centers and the outer two extrapolated
/// by half the adjacent step.
fn centers_to_edges(centers: &[f64]) -> Vec<f64> {
    let n = centers.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![centers[0] - 0.5, centers[0] + 0.5];
    }
    let mut edges = Vec::with_capacity(n + 1);
    edges.push(centers[0] - (centers[1] - centers[0]) * 0.5);
    for w in centers.windows(2) {
        edges.push((w[0] + w[1]) * 0.5);
    }
    edges.push(centers[n - 1] + (centers[n - 1] - centers[n - 2]) * 0.5);
    edges
}

/// One contour panel: filled heatmap (NaN cells left blank, matching
/// `np.where(feasible, ..., np.nan)` feeding a masked `contourf`), a
/// colorbar, and a white cruise-point marker with its legend entry.
#[allow(clippy::too_many_arguments)]
fn draw_contour_panel(
    scene: &mut Scene,
    pal: &Palette,
    rect: (f64, f64, f64, f64),
    x_edges: &[f64],
    y_edges: &[f64],
    values: &[Vec<f64>],
    cmap: Colormap,
    colorbar_label: &str,
    req: &DesignRequirements,
    title: &str,
) {
    let (x, y, w, h) = rect;
    let axes_w = w - 55.0;

    scene.add(SceneElement::Text {
        text: title.replace('\n', "  "),
        pos: [x, y - 18.0],
        font_size: 10.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });

    let mut vmin = f64::INFINITY;
    let mut vmax = f64::NEG_INFINITY;
    for row in values {
        for &v in row {
            if v.is_finite() {
                vmin = vmin.min(v);
                vmax = vmax.max(v);
            }
        }
    }

    let axes = Axes2D::new(
        (x, y, axes_w, h),
        (x_edges[0], *x_edges.last().unwrap_or(&1.0)),
        (y_edges[0], *y_edges.last().unwrap_or(&1.0)),
    );

    if !vmin.is_finite() {
        axes.draw_frame(scene, pal);
        scene.add(SceneElement::Text {
            text: "No feasible points over this envelope".to_owned(),
            pos: [x + axes_w * 0.5, y + h * 0.5],
            font_size: 9.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
        return;
    }

    draw_masked_heatmap(scene, &axes, x_edges, y_edges, values, cmap, vmin, vmax);
    let levels = contour_levels(vmin, vmax, 6);
    draw_isolines(scene, &axes, x_edges, y_edges, values, &levels, pal);
    axes.draw_frame(scene, pal);

    scene.add(SceneElement::Text {
        text: "Altitude [km]".to_owned(),
        pos: [x + axes_w * 0.5, y + h + 24.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: "Mach number  M0  [-]".to_owned(),
        pos: [x - 38.0, y + h * 0.5],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });

    draw_colorbar(
        scene,
        (x + axes_w + 14.0, y, 12.0, h),
        cmap,
        vmin,
        vmax,
        colorbar_label,
        pal,
    );

    let cruise_alt_km = req.cruise_altitude_m / 1000.0;
    let marker = axes.map_point(cruise_alt_km, req.cruise_mach);
    scene.add(SceneElement::Circle {
        center: marker,
        radius: 5.0,
        fill: Some(Fill::new(Color::rgb(255, 255, 255))),
        stroke: Some(Stroke::new(Color::rgb(0, 0, 0), 1.0)),
    });

    let panel = Color::from_hex(pal.panel);
    scene.add(SceneElement::Rect {
        x: x + 4.0,
        y: y + h - 30.0,
        width: 190.0,
        height: 25.0,
        rx: 2.0,
        fill: Some(Fill::new(Color::rgba(panel.r, panel.g, panel.b, 232))),
        stroke: Some(Stroke::new(Color::from_hex(pal.border), 0.8)),
    });

    draw_legend(
        scene,
        [x + 8.0, y + h - 22.0],
        &[(
            format!("Cruise (M{:.2} @ {cruise_alt_km:.1} km)", req.cruise_mach),
            LegendMarker::Circle(Color::rgb(255, 255, 255)),
        )],
        pal,
        8.0,
    );
}

/// Choose six evenly spaced contour levels, excluding the two filled-field
/// extrema so labels do not sit on the plot frame.
fn contour_levels(vmin: f64, vmax: f64, count: usize) -> Vec<f64> {
    if count == 0 || !vmin.is_finite() || !vmax.is_finite() || vmax <= vmin {
        return Vec::new();
    }
    (1..=count)
        .map(|i| vmin + (vmax - vmin) * i as f64 / (count + 1) as f64)
        .collect()
}

/// Draw labelled iso-lines by linearly interpolating each level across the
/// sampled altitude cells. The fields are smooth in Mach, so connecting the
/// ordered crossings in neighbouring rows gives the same readable branches
/// as the reference `contour` call without adding a contour dependency to the
/// backend-neutral scene layer.
fn draw_isolines(
    scene: &mut Scene,
    axes: &Axes2D,
    x_edges: &[f64],
    y_edges: &[f64],
    values: &[Vec<f64>],
    levels: &[f64],
    pal: &Palette,
) {
    if x_edges.len() < 2 || y_edges.len() < 2 {
        return;
    }
    let x_centers: Vec<f64> = x_edges.windows(2).map(|w| (w[0] + w[1]) * 0.5).collect();
    let y_centers: Vec<f64> = y_edges.windows(2).map(|w| (w[0] + w[1]) * 0.5).collect();
    let line_color = if pal.name == "light" {
        Color::rgba(20, 20, 20, 190)
    } else {
        Color::rgba(235, 235, 235, 190)
    };

    for &level in levels {
        let mut rows: Vec<Vec<(f64, f64)>> = Vec::new();
        for (iy, row) in values.iter().enumerate() {
            let mut crossings = Vec::new();
            for ix in 0..row.len().saturating_sub(1) {
                let a = row[ix];
                let b = row[ix + 1];
                if !a.is_finite() || !b.is_finite() || (a - level) * (b - level) > 0.0 {
                    continue;
                }
                let denominator = b - a;
                let fraction = if denominator.abs() < 1e-12 {
                    0.5
                } else {
                    ((level - a) / denominator).clamp(0.0, 1.0)
                };
                let x = x_centers[ix] + fraction * (x_centers[ix + 1] - x_centers[ix]);
                crossings.push((x, y_centers[iy]));
            }
            rows.push(crossings);
        }

        let max_crossings = rows.iter().map(Vec::len).max().unwrap_or(0);
        for branch in 0..max_crossings {
            let points: Vec<(f64, f64)> = rows
                .iter()
                .filter_map(|row| row.get(branch).copied())
                .collect();
            if points.len() < 2 {
                continue;
            }
            axes.add_line_series(scene, &points, Stroke::new(line_color, 0.8));
            if let Some((x, y)) = points.get(points.len() / 2).copied() {
                let p = axes.map_point(x, y);
                scene.add(SceneElement::Text {
                    text: format!("{level:.3}"),
                    pos: [p[0] + 3.0, p[1] - 3.0],
                    font_size: 7.0,
                    color: line_color,
                    align: TextAlign::Left,
                    baseline: TextBaseline::Bottom,
                    angle_deg: 0.0,
                    bold: false,
                });
            }
        }
    }
}

/// Draw one colored [`SceneElement::Rect`] per finite grid cell (skipping
/// `NaN`, which is how an infeasible cell reaches this function), the
/// same coarse-grid approximation `Axes2D::add_heatmap_grid` uses -- written
/// locally rather than reusing that helper because it has no masking, and
/// `NaN.clamp` would otherwise fall through to the colormap's last stop
/// rather than leaving the cell blank.
#[allow(clippy::too_many_arguments)]
fn draw_masked_heatmap(
    scene: &mut Scene,
    axes: &Axes2D,
    x_edges: &[f64],
    y_edges: &[f64],
    values: &[Vec<f64>],
    cmap: Colormap,
    vmin: f64,
    vmax: f64,
) {
    let span = (vmax - vmin).max(1e-12);
    for (iy, row) in values.iter().enumerate() {
        for (ix, &v) in row.iter().enumerate() {
            if !v.is_finite() {
                continue;
            }
            let t = ((v - vmin) / span).clamp(0.0, 1.0);
            let color = cmap.sample(t);
            let p0 = axes.map_point(x_edges[ix], y_edges[iy]);
            let p1 = axes.map_point(x_edges[ix + 1], y_edges[iy + 1]);
            let (x0, x1) = (p0[0].min(p1[0]), p0[0].max(p1[0]));
            let (y0, y1) = (p0[1].min(p1[1]), p0[1].max(p1[1]));
            scene.add(SceneElement::Rect {
                x: x0,
                y: y0,
                width: (x1 - x0).max(0.5),
                height: (y1 - y0).max(0.5),
                rx: 0.0,
                fill: Some(Fill::new(color)),
                stroke: None,
            });
        }
    }
}

// A test asserts on values it constructed or read off a fixed config here
// directly, so a failed unwrap or expect is the assertion failing, not a
// library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_engine_envelope_is_feasible_and_draws_two_filled_panels() {
        let config = AlasConfig::default();
        let scene = figure_propulsion_altitude_sweep(&config, None);
        let rects = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Rect { .. }))
            .count();
        // 26x26 cells per panel, two panels, all feasible over this envelope
        // (each also carries a colorbar's own 65 rects, so this is a floor
        // rather than an exact count).
        assert!(rects >= 2 * 26 * 26);
        // Each panel has the cruise marker and its legend swatch.
        assert_eq!(
            scene
                .elements
                .iter()
                .filter(|e| matches!(e, SceneElement::Circle { .. }))
                .count(),
            4,
            "one marker and one legend swatch per panel"
        );
    }

    #[test]
    fn centers_to_edges_produces_one_more_edge_than_centers_and_brackets_them() {
        let centers = linspace(0.0, 10.0, 6);
        let edges = centers_to_edges(&centers);
        assert_eq!(edges.len(), centers.len() + 1);
        assert!(edges[0] < centers[0]);
        assert!(*edges.last().unwrap() > *centers.last().unwrap());
        for w in edges.windows(2) {
            assert!(w[1] > w[0], "edges must be strictly increasing");
        }
    }

    #[test]
    fn a_single_center_gets_a_unit_wide_edge_pair() {
        let edges = centers_to_edges(&[5.0]);
        assert_eq!(edges, vec![4.5, 5.5]);
    }

    #[test]
    fn the_thrust_panel_values_trace_to_the_sweep_function_directly() {
        let config = AlasConfig::default();
        let eng = &config.geometry.engine;
        let mach_vec = linspace(
            0.0,
            (0.9f64).max(config.requirements.cruise_mach * 1.15),
            26,
        );
        let alt_vec = linspace(0.0, 13_000.0, 26);
        let (mdot, _) = anchor_mass_flow_kg_s(
            eng.thrust_kn,
            eng.overall_pressure_ratio,
            eng.fan_pressure_ratio,
            eng.bypass_ratio,
            eng.turbine_inlet_temp_k,
            &config.propulsion_cycle,
        );
        assert!(mdot.is_finite() && mdot > 0.0);
        let sweep = compute_altitude_sweep(
            &alt_vec,
            &mach_vec,
            eng.bypass_ratio,
            eng.overall_pressure_ratio,
            eng.fan_pressure_ratio,
            eng.turbine_inlet_temp_k,
            mdot,
            &config.propulsion_cycle,
        );
        assert!(sweep.feasible_mask.iter().flatten().all(|&ok| ok));
        // Thrust must fall roughly monotonically as altitude increases at a
        // fixed low Mach number (thinner air, less mass flow at the anchor).
        let low_mach_by_altitude: Vec<f64> = sweep
            .dimensional_thrust_kn
            .iter()
            .map(|row| row[0])
            .collect();
        assert!(low_mach_by_altitude.first().unwrap() > low_mach_by_altitude.last().unwrap());
    }
}
