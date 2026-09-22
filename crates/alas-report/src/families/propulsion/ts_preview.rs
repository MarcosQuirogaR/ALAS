// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Quantitative station preview with explicit ideal-gas closure guides.
use crate::chart_kit::draw_title;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::{get_palette, Palette};
use alas_config::AlasConfig;
use alas_prop::cycle::states::{compute_turbofan_cycle_states, CycleStation};

const CORE: &str = "#e67e22";
const BYPASS: &str = "#3498db";

pub(super) fn preview(config: &AlasConfig, theme: Option<&str>) -> Scene {
    let states =
        compute_turbofan_cycle_states(&super::design_point(config), &config.propulsion_cycle);
    match states
        .freestream_static
        .as_ref()
        .filter(|_| states.cycle_feasible)
    {
        Some(reference) => diagram(
            config,
            theme,
            reference,
            &states.core,
            &states.bypass,
            false,
        ),
        None => unavailable(theme, &states.infeasibility_reason),
    }
}

pub(super) fn turboprop_preview(config: &AlasConfig, theme: Option<&str>) -> Scene {
    let Ok(alas_config::ActiveEngineModel::Turboprop(spec)) = config.geometry.engine.active_model()
    else {
        return unavailable(theme, "Invalid turboprop configuration");
    };
    match alas_prop::turboprop_cycle::compute_turboprop_cycle_states(
        spec,
        config.requirements.cruise_mach,
        config.requirements.cruise_altitude_m,
        &config.propulsion_cycle,
    ) {
        Ok(states) => diagram(
            config,
            theme,
            &states.freestream_static,
            &states.core,
            &[],
            true,
        ),
        Err(reason) => unavailable(theme, &reason),
    }
}

fn unavailable(theme: Option<&str>, reason: &str) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(900.0, 550.0, Some(Color::from_hex(pal.bg)));
    label(
        &mut scene,
        "Cycle infeasible at this design point",
        [70.0, 180.0],
        pal.title,
        14.0,
    );
    label(&mut scene, reason, [70.0, 208.0], pal.tick, 11.0);
    scene
}

fn diagram(
    config: &AlasConfig,
    theme: Option<&str>,
    reference: &CycleStation,
    core: &[CycleStation],
    bypass: &[CycleStation],
    turboprop: bool,
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(900.0, 550.0, Some(Color::from_hex(pal.bg)));
    let title = format!("{} * T-s", config.geometry.engine.engine_name);
    scene.title = Some(title.clone());
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();
    let all = std::iter::once(reference)
        .chain(core)
        .chain(bypass)
        .collect::<Vec<_>>();
    if all.iter().any(|state| {
        !state.temperature_k.is_finite()
            || state.temperature_k <= 0.0
            || !state.pressure_pa.is_finite()
            || state.pressure_pa <= 0.0
            || !state.entropy_j_kgk.is_finite()
    }) {
        label(
            &mut scene,
            "Cycle unavailable: invalid station state",
            [70.0, 180.0],
            pal.title,
            12.0,
        );
        return scene;
    }
    let mut legend = vec![(78.0, "Core flow", CORE, false)];
    if !bypass.is_empty() {
        legend.push((205.0, "Bypass flow", BYPASS, false));
    }
    legend.push((355.0, "Reference closure", pal.tick, true));
    for (x, text, color, dashed) in legend {
        scene.add(SceneElement::Line {
            p1: [x, 53.0],
            p2: [x + 24.0, 53.0],
            stroke: if dashed {
                Stroke::dashed(Color::from_hex(color), 1.1, 4.0, 4.0)
            } else {
                Stroke::new(Color::from_hex(color), 2.2)
            },
        });
        label(&mut scene, text, [x + 30.0, 53.0], pal.tick, 10.0);
    }
    label(
        &mut scene,
        &format!(
            "M {:.2} * h {:.1} km",
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m / 1000.0
        ),
        [690.0, 53.0],
        pal.tick,
        10.0,
    );
    let s_min = all
        .iter()
        .map(|v| v.entropy_j_kgk / 1000.0)
        .fold(0.0_f64, f64::min);
    let s_max = all
        .iter()
        .map(|v| v.entropy_j_kgk / 1000.0)
        .fold(0.1_f64, f64::max);
    let t_max = all.iter().map(|v| v.temperature_k).fold(1.0_f64, f64::max);
    let axes = Axes2D::new(
        (78.0, 82.0, 760.0, 400.0),
        (
            s_min - 0.08 * (s_max - s_min),
            s_max + 0.18 * (s_max - s_min),
        ),
        (reference.temperature_k * 0.70, t_max * 1.12),
    );
    axes.draw_frame_with_labels(&mut scene, pal, "s - s0 [kJ/(kg*K)]", "Temperature T [K]");
    draw_branch(&mut scene, &axes, reference, core, CORE);
    if !bypass.is_empty() {
        draw_branch(&mut scene, &axes, &bypass[0], &bypass[1..], BYPASS);
    }
    let cfg = &config.propulsion_cycle;
    for (branch, cp, gamma) in [
        (core, cfg.cp_hot_j_kgk, cfg.gamma_hot),
        (bypass, cfg.cp_cold_j_kgk, cfg.gamma_cold),
    ] {
        if let Some(exit) = branch.last() {
            let points = closure(reference, exit, cp, gamma);
            axes.add_line_series(
                &mut scene,
                &points,
                Stroke::dashed(Color::from_hex(pal.tick), 1.1, 4.0, 4.0),
            );
            arrow(&mut scene, &axes, &points, pal.tick);
        }
    }
    for (id, text, dx, dy) in [
        ("3", "3 * HPC", 12.0, 17.0),
        ("4", "4 * Burner", -72.0, -15.0),
        (
            "45",
            if turboprop {
                "45 * Gas generator"
            } else {
                "45 * HPT"
            },
            -122.0,
            0.0,
        ),
        (
            "5",
            if turboprop {
                "5 * Power turbine"
            } else {
                "5 * LPT"
            },
            -120.0,
            -14.0,
        ),
        ("6", "6", 10.0, -7.0),
        ("9", "9", 12.0, 12.0),
    ] {
        if let Some(state) = core.iter().find(|s| s.station == id) {
            let point = axes.map_point(state.entropy_j_kgk / 1000.0, state.temperature_k);
            label(
                &mut scene,
                text,
                [point[0] + dx, point[1] + dy],
                pal.title,
                11.0,
            );
        }
    }
    let p = axes.map_point(reference.entropy_j_kgk / 1000.0, reference.temperature_k);
    label(&mut scene, "0s", [p[0] - 22.0, p[1] + 3.0], pal.title, 10.0);
    draw_detail(&mut scene, reference, core, bypass, pal);
    scene
}

fn label(scene: &mut Scene, text: &str, pos: [f64; 2], color: &str, size: f64) {
    scene.add(SceneElement::Text {
        text: text.into(),
        pos,
        font_size: size,
        color: Color::from_hex(color),
        align: TextAlign::Left,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
}

/// Logarithmic temperature interpolation and linear entropy interpolation.
/// For one constant-property gas this is exactly a log(T), log(p) path,
/// reproducing polytropic compression/expansion. Across combustion it is
/// only a smooth connection of the model's different gas entropy datums.
fn segment(a: &CycleStation, b: &CycleStation) -> Vec<(f64, f64)> {
    (0..=32)
        .map(|i| {
            let f = f64::from(i) / 32.0;
            (
                (a.entropy_j_kgk + (b.entropy_j_kgk - a.entropy_j_kgk) * f) / 1000.0,
                a.temperature_k * (b.temperature_k / a.temperature_k).powf(f),
            )
        })
        .collect()
}

fn draw_branch(
    scene: &mut Scene,
    axes: &Axes2D,
    reference: &CycleStation,
    branch: &[CycleStation],
    color: &str,
) {
    let states = std::iter::once(reference).chain(branch).collect::<Vec<_>>();
    for pair in states.windows(2) {
        let points = segment(pair[0], pair[1]);
        axes.add_line_series(scene, &points, Stroke::new(Color::from_hex(color), 2.3));
        arrow(scene, axes, &points, color);
    }
    for state in states {
        scene.add(SceneElement::Circle {
            center: axes.map_point(state.entropy_j_kgk / 1000.0, state.temperature_k),
            radius: 2.8,
            fill: Some(Fill::new(Color::from_hex(color))),
            stroke: Some(Stroke::new(Color::from_hex("#ffffff"), 0.5)),
        });
    }
}

fn arrow(scene: &mut Scene, axes: &Axes2D, points: &[(f64, f64)], color: &str) {
    let a = axes.map_point(points[0].0, points[0].1);
    let b = axes.map_point(points[points.len() - 1].0, points[points.len() - 1].1);
    if (b[0] - a[0]).hypot(b[1] - a[1]) < 24.0 {
        return;
    }
    let k = points.len() / 2;
    let p = axes.map_point(points[k].0, points[k].1);
    let q = axes.map_point(points[k - 1].0, points[k - 1].1);
    let length = (p[0] - q[0]).hypot(p[1] - q[1]);
    if length < 1e-8 {
        return;
    }
    let (ux, uy) = ((p[0] - q[0]) / length, (p[1] - q[1]) / length);
    scene.add(SceneElement::Polygon {
        points: vec![
            p,
            [p[0] - 7.0 * ux + 3.0 * uy, p[1] - 7.0 * uy - 3.0 * ux],
            [p[0] - 7.0 * ux - 3.0 * uy, p[1] - 7.0 * uy + 3.0 * ux],
        ],
        fill: Some(Fill::new(Color::from_hex(color))),
        stroke: None,
    });
}

fn closure(reference: &CycleStation, exit: &CycleStation, cp: f64, gamma: f64) -> Vec<(f64, f64)> {
    let ambient_t =
        exit.temperature_k * (reference.pressure_pa / exit.pressure_pa).powf((gamma - 1.0) / gamma);
    let mut points = vec![(exit.entropy_j_kgk / 1000.0, exit.temperature_k)];
    for i in 0..=48 {
        let t = ambient_t * (reference.temperature_k / ambient_t).powf(f64::from(i) / 48.0);
        points.push(((exit.entropy_j_kgk + cp * (t / ambient_t).ln()) / 1000.0, t));
    }
    points
}

fn draw_detail(
    scene: &mut Scene,
    reference: &CycleStation,
    core: &[CycleStation],
    bypass: &[CycleStation],
    pal: &Palette,
) {
    // Place the inset only in unused plot space; parameter changes must not
    // hide the heat-addition or closure paths behind a fixed inset rectangle.
    let position = (0..9)
        .flat_map(|row| {
            (0..8).map(move |col| [140.0 + f64::from(col) * 42.0, 98.0 + f64::from(row) * 27.0])
        })
        .find(|&[x, y]| {
            !scene.elements.iter().any(|element| match element {
                SceneElement::Polyline { points, .. } => points.windows(2).any(|pair| {
                    let (a, b) = (pair[0], pair[1]);
                    a[0].min(b[0]) < x + 242.0
                        && a[0].max(b[0]) > x - 4.0
                        && a[1].min(b[1]) < y + 149.0
                        && a[1].max(b[1]) > y - 4.0
                }),
                _ => false,
            })
        });
    let Some([left, top]) = position else {
        return;
    };
    // The inset contains only the densely packed low-temperature states.
    let low = core
        .iter()
        .take(3)
        .chain(bypass)
        .chain(std::iter::once(reference))
        .collect::<Vec<_>>();
    let smax = low
        .iter()
        .map(|s| s.entropy_j_kgk / 1000.0)
        .fold(0.01_f64, f64::max);
    let tmin = low
        .iter()
        .map(|s| s.temperature_k)
        .fold(f64::INFINITY, f64::min);
    let tmax = low.iter().map(|s| s.temperature_k).fold(0.0_f64, f64::max);
    scene.add(SceneElement::Rect {
        x: left,
        y: top,
        width: 238.0,
        height: 145.0,
        rx: 4.0,
        fill: Some(Fill::new(Color::from_hex(pal.bg))),
        stroke: Some(Stroke::new(Color::from_hex(pal.border), 0.8)),
    });
    let axes = Axes2D::new(
        (left + 36.0, top + 14.0, 181.0, 103.0),
        (-0.35 * smax, 1.4 * smax),
        (
            tmin - 0.22 * (tmax - tmin).max(1.0),
            tmax + 0.40 * (tmax - tmin).max(1.0),
        ),
    );
    axes.draw_frame(scene, pal);
    draw_branch(scene, &axes, reference, &core[..3], CORE);
    if !bypass.is_empty() {
        draw_branch(scene, &axes, &bypass[0], &bypass[1..], BYPASS);
    }
    let mut labels = vec![
        (reference, "0s", -20.0, 12.0),
        (&core[0], "0", -18.0, -10.0),
        (&core[1], "2", 7.0, 12.0),
        (&core[2], "25", -20.0, -14.0),
    ];
    if bypass.len() >= 3 {
        labels.extend([(&bypass[1], "13", 7.0, -9.0), (&bypass[2], "19", 7.0, 12.0)]);
    }
    for (state, text, dx, dy) in labels {
        let p = axes.map_point(state.entropy_j_kgk / 1000.0, state.temperature_k);
        label(scene, text, [p[0] + dx, p[1] + dy], pal.title, 10.0);
    }
}

#[cfg(test)]
// In a test module a failing expect is the assertion failing, and these scenes
// are built from fixtures the tests define.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    #[test]
    fn process_curves_preserve_stations_and_polytropic_entropy() {
        let config = AlasConfig::default();
        let states = compute_turbofan_cycle_states(
            &super::super::design_point(&config),
            &config.propulsion_cycle,
        );
        for pair in states.core.windows(2) {
            let points = segment(&pair[0], &pair[1]);
            assert!((points[0].0 - pair[0].entropy_j_kgk / 1000.0).abs() < 1e-12);
            assert!((points[32].1 - pair[1].temperature_k).abs() < 1e-9);
            assert!(
                (points[16].1 - (pair[0].temperature_k * pair[1].temperature_k).sqrt()).abs()
                    < 1e-9
            );
        }
    }
    #[test]
    fn ideal_closure_preserves_exit_and_returns_to_reference() {
        let config = AlasConfig::default();
        let states = compute_turbofan_cycle_states(
            &super::super::design_point(&config),
            &config.propulsion_cycle,
        );
        let reference = states
            .freestream_static
            .as_ref()
            .expect("feasible reference");
        for (branch, cp, gamma) in [
            (
                &states.core,
                config.propulsion_cycle.cp_hot_j_kgk,
                config.propulsion_cycle.gamma_hot,
            ),
            (
                &states.bypass,
                config.propulsion_cycle.cp_cold_j_kgk,
                config.propulsion_cycle.gamma_cold,
            ),
        ] {
            let exit = branch.last().expect("exit");
            let points = closure(reference, exit, cp, gamma);
            assert_eq!(points[0], (exit.entropy_j_kgk / 1000.0, exit.temperature_k));
            let end = points.last().expect("closure");
            assert!(end.0.abs() < 1e-9);
            assert!((end.1 - reference.temperature_k).abs() < 1e-9);
        }
    }
}
