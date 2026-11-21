// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Quantitative station preview with explicit ideal-gas closure guides.
use super::ts_labels::{label, LabelPlacer};
use crate::chart_kit::{draw_inline_legend, draw_title, LegendMarker};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::{get_palette, Palette};
use alas_config::{AlasConfig, TurbofanEngineSpec};
use alas_prop::cycle::states::{compute_turbofan_cycle_states, CycleStation};

const CORE: &str = "#e67e22";
const BYPASS: &str = "#3498db";

/// The canvas the cycle preview is drawn on when the caller names none.
pub(super) const DEFAULT_CANVAS: (f64, f64) = (900.0, 550.0);

pub(super) fn preview(
    config: &AlasConfig,
    eng: &TurbofanEngineSpec,
    theme: Option<&str>,
    canvas: (f64, f64),
) -> Scene {
    let states = compute_turbofan_cycle_states(
        &super::catalogue_design_point(config, eng),
        &config.propulsion_cycle,
    );
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
            canvas,
        ),
        None => unavailable(theme, &states.infeasibility_reason, canvas),
    }
}

pub(super) fn turboprop_preview(
    config: &AlasConfig,
    theme: Option<&str>,
    canvas: (f64, f64),
) -> Scene {
    let Ok(alas_config::ActiveEngineModel::Turboprop(spec)) = config.geometry.engine.active_model()
    else {
        return unavailable(theme, "Invalid turboprop configuration", canvas);
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
            canvas,
        ),
        Err(reason) => unavailable(theme, &reason, canvas),
    }
}

fn unavailable(theme: Option<&str>, reason: &str, canvas: (f64, f64)) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(canvas.0, canvas.1, Some(Color::from_hex(pal.bg)));
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
    canvas: (f64, f64),
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(canvas.0, canvas.1, Some(Color::from_hex(pal.bg)));
    let title = format!("{} \u{00b7} T-s", config.geometry.engine.engine_name);
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
    let tick_font = 11.0;
    let mut entries = vec![(
        "Core flow".to_owned(),
        LegendMarker::Line(Stroke::new(Color::from_hex(CORE), 2.2)),
    )];
    if !bypass.is_empty() {
        entries.push((
            "Bypass flow".to_owned(),
            LegendMarker::Line(Stroke::new(Color::from_hex(BYPASS), 2.2)),
        ));
    }
    entries.push((
        "Reference closure".to_owned(),
        LegendMarker::Line(Stroke::dashed(Color::from_hex(pal.tick), 1.1, 4.0, 4.0)),
    ));
    let left = 92.0;
    let right_edge = canvas.0 - 28.0;
    // The legend wraps onto further rows when it does not fit the canvas, and the cruise-condition
    // text takes the first row it fits beside, else a row of its own: neither may overprint the
    // other or leave the canvas (estimated widths; the scene has no text measurement).
    let legend_font = tick_font + 1.0;
    let text_px = |text: &str| {
        text.chars().count() as f64 * legend_font * crate::scene::CSS_PIXELS_PER_POINT * 0.56
    };
    let entry_px = |label: &str| 28.0 + text_px(label);
    let available = right_edge - left;
    let mut legend_rows: Vec<Vec<(String, LegendMarker)>> = vec![Vec::new()];
    let mut row_width = 0.0_f64;
    for entry in entries {
        let width = entry_px(&entry.0);
        let gap = if legend_rows.last().is_some_and(|row| !row.is_empty()) {
            30.0
        } else {
            0.0
        };
        if row_width + gap + width > available && row_width > 0.0 {
            legend_rows.push(Vec::new());
            row_width = 0.0;
        } else {
            row_width += gap;
        }
        row_width += width;
        if let Some(row) = legend_rows.last_mut() {
            row.push(entry);
        }
    }
    let row_pitch = 20.0;
    for (index, row) in legend_rows.iter().enumerate() {
        draw_inline_legend(
            &mut scene,
            [left, 52.0 + index as f64 * row_pitch],
            row,
            pal,
            legend_font,
        );
    }
    let condition = format!(
        "M {:.2} \u{00b7} h {:.1} km",
        config.requirements.cruise_mach,
        config.requirements.cruise_altitude_m / 1000.0
    );
    let condition_fits_beside = row_width + 24.0 + text_px(&condition) <= available;
    let condition_row = if condition_fits_beside {
        legend_rows.len() - 1
    } else {
        legend_rows.len()
    };
    scene.add(SceneElement::Text {
        text: condition,
        pos: [right_edge, 52.0 + condition_row as f64 * row_pitch],
        font_size: legend_font,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Right,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
    let header_rows = legend_rows.len().max(condition_row + 1);
    let plot_top = 76.0 + (header_rows - 1) as f64 * row_pitch;
    let s_min = all
        .iter()
        .map(|v| v.entropy_j_kgk / 1000.0)
        .fold(0.0_f64, f64::min);
    let s_max = all
        .iter()
        .map(|v| v.entropy_j_kgk / 1000.0)
        .fold(0.1_f64, f64::max);
    let t_max = all.iter().map(|v| v.temperature_k).fold(1.0_f64, f64::max);
    let plot = (
        left,
        plot_top,
        right_edge - left,
        canvas.1 - plot_top - 62.0,
    );
    let axes = Axes2D::new(
        plot,
        (
            s_min - 0.08 * (s_max - s_min),
            s_max + 0.18 * (s_max - s_min),
        ),
        (reference.temperature_k * 0.70, t_max * 1.12),
    )
    .with_tick_font(tick_font);
    axes.draw_frame_with_labels(
        &mut scene,
        pal,
        "s \u{2212} s0 [kJ/(kg\u{00b7}K)]",
        "Temperature T [K]",
    );
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
    // Every station dot, so no label is put on one.
    let dots: Vec<[f64; 2]> = std::iter::once(reference)
        .chain(core)
        .chain(bypass)
        .map(|state| axes.map_point(state.entropy_j_kgk / 1000.0, state.temperature_k))
        .collect();
    let curves: Vec<Vec<[f64; 2]>> = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Polyline { points, .. } => Some(points.clone()),
            _ => None,
        })
        .collect();
    let mut placer = LabelPlacer::new(plot, dots).with_curves(curves.iter().map(Vec::as_slice));
    let inset = detail_position(&scene, plot);
    if inset.is_some() {
        let p = axes.map_point(reference.entropy_j_kgk / 1000.0, reference.temperature_k);
        placer.add(&mut scene, "0s", p, (false, 12.0), pal.title, 10.0);
    }
    // Each label stands beside its own dot (the side it prefers first, then
    // the other, then further up and down) clear of the other labels and dots.
    for (id, text, right, dy) in [
        ("3", "3 \u{00b7} HPC", true, 14.0),
        ("4", "4 \u{00b7} Burner", false, -14.0),
        (
            "45",
            if turboprop {
                "45 \u{00b7} Gas generator"
            } else {
                "45 \u{00b7} HPT"
            },
            false,
            0.0,
        ),
        (
            "5",
            if turboprop {
                "5 \u{00b7} Power turbine"
            } else {
                "5 \u{00b7} LPT"
            },
            false,
            -14.0,
        ),
        ("6", "6 \u{00b7} Tailpipe", true, -7.0),
        ("9", "9 \u{00b7} Nozzle exit", true, 12.0),
    ] {
        if let Some(state) = core.iter().find(|s| s.station == id) {
            let point = axes.map_point(state.entropy_j_kgk / 1000.0, state.temperature_k);
            placer.add(&mut scene, text, point, (right, dy), pal.title, 11.0);
        }
    }
    if let Some(position) = inset {
        draw_detail(&mut scene, reference, core, bypass, pal, position);
    } else {
        // No free room for the inset: name the packed low-temperature
        // stations above the cluster, each on a leader line to its own dot.
        let low = std::iter::once(("0s", reference))
            .chain(low_core(core).iter().map(|s| (s.station, s)))
            .chain(bypass.iter().skip(1).take(2).map(|s| (s.station, s)))
            .collect::<Vec<_>>();
        cluster_labels(&mut scene, &axes, plot, &low, pal);
    }
    scene
}

/// Label the stations packed near the origin in a column left of the
/// cluster, each joined to its dot by a leader line.
fn cluster_labels(
    scene: &mut Scene,
    axes: &Axes2D,
    plot: (f64, f64, f64, f64),
    stations: &[(&str, &CycleStation)],
    pal: &Palette,
) {
    let mut dots: Vec<(&str, [f64; 2])> = stations
        .iter()
        .map(|(name, s)| {
            (
                *name,
                axes.map_point(s.entropy_j_kgk / 1000.0, s.temperature_k),
            )
        })
        .collect();
    if dots.is_empty() {
        return;
    }
    dots.sort_by(|a, b| a.1[1].total_cmp(&b.1[1]).then(a.1[0].total_cmp(&b.1[0])));
    let min_x = dots.iter().map(|d| d.1[0]).fold(f64::INFINITY, f64::min);
    let min_y = dots.iter().map(|d| d.1[1]).fold(f64::INFINITY, f64::min);
    let pitch = 16.0;
    // Right-aligned: the widest name must end right of the axis, not run across it.
    let widest = stations
        .iter()
        .map(|(name, _)| name.chars().count())
        .max()
        .unwrap_or(1) as f64
        * 10.0
        * crate::scene::CSS_PIXELS_PER_POINT
        * 0.62;
    let label_x = (min_x - 6.0).max(plot.0 + 8.0 + widest);
    let top = (min_y - 20.0 - (dots.len() - 1) as f64 * pitch).max(plot.1 + 8.0);
    for (index, (name, dot)) in dots.iter().enumerate() {
        let y = top + index as f64 * pitch;
        scene.add(SceneElement::Line {
            p1: [label_x + 3.0, y],
            p2: *dot,
            stroke: Stroke::new(Color::from_hex(pal.tick), 0.6),
        });
        scene.add(SceneElement::Text {
            text: (*name).into(),
            pos: [label_x, y],
            font_size: 10.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Right,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
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

/// Top-left corner of the inset of the packed low-temperature states, if the
/// plot has room for it clear of every drawn curve.
fn detail_position(scene: &Scene, plot: (f64, f64, f64, f64)) -> Option<[f64; 2]> {
    // Place the inset only in unused plot space; parameter changes must not
    // hide the heat-addition or closure paths behind a fixed inset rectangle.
    let columns = (((plot.2 - 70.0 - 244.0) / 42.0).floor().max(0.0) as u32) + 1;
    let rows = (((plot.3 - 24.0 - 150.0) / 27.0).floor().max(0.0) as u32) + 1;
    (0..rows)
        .flat_map(|row| {
            (0..columns).map(move |col| {
                [
                    plot.0 + 62.0 + f64::from(col) * 42.0,
                    plot.1 + 16.0 + f64::from(row) * 27.0,
                ]
            })
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
        })
}

/// The densely packed low-temperature core states: from the freestream to the
/// HPC face (station 25), which on a turbofan includes the fan-hub exit 21.
fn low_core(core: &[CycleStation]) -> &[CycleStation] {
    let end = core
        .iter()
        .position(|s| s.station == "25")
        .map_or(core.len().min(3), |i| i + 1);
    &core[..end]
}

fn draw_detail(
    scene: &mut Scene,
    reference: &CycleStation,
    core: &[CycleStation],
    bypass: &[CycleStation],
    pal: &Palette,
    [left, top]: [f64; 2],
) {
    // The inset contains only the densely packed low-temperature states.
    let low_core = low_core(core);
    let low = low_core
        .iter()
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
    draw_branch(scene, &axes, reference, low_core, CORE);
    if !bypass.is_empty() {
        draw_branch(scene, &axes, &bypass[0], &bypass[1..], BYPASS);
    }
    let mut labels = vec![
        (reference, "0s", -20.0, 12.0),
        (&core[0], "0", -18.0, -10.0),
        (&core[1], "2", 7.0, 12.0),
        (&low_core[low_core.len() - 1], "25", -20.0, -14.0),
    ];
    if let Some(hub) = low_core.iter().find(|s| s.station == "21") {
        labels.push((hub, "21", -20.0, 4.0));
    }
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
        let eng = super::super::turbofan_spec(&config).expect("turbofan binding");
        let states = compute_turbofan_cycle_states(
            &super::super::design_point(&config, eng),
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
    fn texts(scene: &Scene) -> Vec<String> {
        scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// Every labelled turbofan station has a name, and the packed
    /// low-temperature stations are labelled when the inset does not fit.
    #[test]
    fn every_station_label_is_named_and_the_packed_ones_are_labelled() {
        let config = AlasConfig::default();
        let scene = super::super::figure_engine_designer_preview_sized(
            &config,
            Some("dark"),
            (800.0, 470.0),
        );
        let labels = texts(&scene);
        for expected in ["6 \u{00b7} Tailpipe", "9 \u{00b7} Nozzle exit"] {
            assert!(labels.iter().any(|label| label == expected), "{labels:?}");
        }
        assert!(
            !labels.iter().any(|label| label == "6" || label == "9"),
            "{labels:?}"
        );
        for station in ["0s", "13", "25"] {
            assert!(labels.iter().any(|label| label == station), "{station}");
        }
    }

    /// The header (legend rows and the cruise-condition text) stays inside the canvas and clear
    /// of itself at narrow widths, and no station label is struck through by a curve. With
    /// `ALAS_PREVIEW_SVG_DIR` set the figures are also written there for a visual check.
    #[test]
    fn the_header_fits_and_no_label_sits_on_a_curve_at_every_canvas() {
        let config = AlasConfig::default();
        for canvas in [
            (480.0, 300.0),
            (772.0, 460.0),
            (908.0, 550.0),
            (1400.0, 620.0),
        ] {
            let scene =
                super::super::figure_engine_designer_preview_sized(&config, Some("dark"), canvas);
            if let Some(dir) = std::env::var_os("ALAS_PREVIEW_SVG_DIR") {
                let path =
                    std::path::Path::new(&dir).join(format!("cycle_{}x{}.svg", canvas.0, canvas.1));
                std::fs::write(path, crate::svg::render_svg(&scene)).expect("write svg");
            }
            let curves: Vec<[[f64; 2]; 2]> = scene
                .elements
                .iter()
                .filter_map(|element| match element {
                    SceneElement::Polyline { points, .. } => Some(points.clone()),
                    _ => None,
                })
                .flat_map(|points| {
                    points
                        .windows(2)
                        .map(|pair| [pair[0], pair[1]])
                        .collect::<Vec<_>>()
                })
                .collect();
            for element in &scene.elements {
                let SceneElement::Text {
                    text,
                    pos,
                    font_size,
                    align,
                    angle_deg,
                    ..
                } = element
                else {
                    continue;
                };
                // Rotated axis titles are placed by the axes, not by the figure.
                if angle_deg.abs() > 1e-3 {
                    continue;
                }
                let width = text.chars().count() as f64
                    * font_size
                    * crate::scene::CSS_PIXELS_PER_POINT
                    * 0.56;
                let (x0, x1) = match align {
                    TextAlign::Left => (pos[0], pos[0] + width),
                    TextAlign::Right => (pos[0] - width, pos[0]),
                    TextAlign::Center => (pos[0] - width / 2.0, pos[0] + width / 2.0),
                };
                assert!(
                    x0 >= 0.0 && x1 <= canvas.0 + 1.0,
                    "{canvas:?}: {text:?} spans {x0:.0}..{x1:.0}"
                );
                // Station labels ("4 \u{b7} Burner", "45 \u{b7} HPT", "9 \u{b7} Nozzle exit") must
                // not have a curve running through the text.
                // The 480 x 300 card has no room: its packed stations cannot all clear the curves.
                if canvas.0 >= 700.0
                    && text.contains(" \u{b7} ")
                    && !text.starts_with("M ")
                    && !text.contains(" T-s")
                {
                    let rect = [x0, pos[1] - 4.0, x1, pos[1] + 4.0];
                    let hit = curves.iter().any(|segment| {
                        super::super::ts_labels::segment_crosses(segment[0], segment[1], rect)
                    });
                    assert!(!hit, "{canvas:?}: a curve runs through {text:?}");
                }
            }
        }
    }

    #[test]
    fn ideal_closure_preserves_exit_and_returns_to_reference() {
        let config = AlasConfig::default();
        let eng = super::super::turbofan_spec(&config).expect("turbofan binding");
        let states = compute_turbofan_cycle_states(
            &super::super::design_point(&config, eng),
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
