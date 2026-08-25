// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/sidecar/figures_extra.py (`_draw_runway`, `_draw_bars`,
// `_figure_lto`, `figure_lto_departure`, and `figure_lto_arrival`).
// Reference: alas @ rust-port-baseline.

//! Landing and take-off runway and required-versus-available distance figures.

use alas_config::airports::Airport;
use alas_config::AlasConfig;
use alas_perf::performance::{compute_field_performance_at_masses, FieldPerformance};
use alas_pipeline::full_analysis::AnalysisReport;

use crate::chart_kit::draw_title;
use crate::families::performance::support::{
    draw_arrow, format_thousands, resolve_airport, static_thrust_to_weight, status_message_scene,
};
use crate::scene::{Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

const MS_TO_KT: f64 = 1.94384;
const GRASS: &str = "#2d5a27";
const RUNWAY: &str = "#4a4a4a";
const STRIPE: &str = "#f0f0f0";

/// Generate the departure landing-and-take-off figure.
pub fn figure_lto_departure(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let airport = match resolve_airport(&config.departure_airport) {
        Ok(airport) => airport,
        Err(_) => return missing_airport_scene(&config.departure_airport, theme),
    };
    figure_lto_for_airport(report, config, airport, "Departure", theme)
}

/// Generate the arrival landing-and-take-off figure.
pub fn figure_lto_arrival(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let airport = match resolve_airport(&config.arrival_airport) {
        Ok(airport) => airport,
        Err(_) => return missing_airport_scene(&config.arrival_airport, theme),
    };
    figure_lto_for_airport(report, config, airport, "Arrival", theme)
}

/// Generate one landing-and-take-off figure for the airport actually flown.
///
/// A dispatched route may override the airports selected in the configuration.
/// The pipeline carries those endpoints on its route result, so public callers
/// use this entry point instead of silently reporting field performance for a
/// different pair of runways.
pub fn figure_lto_for_airport(
    report: &AnalysisReport,
    config: &AlasConfig,
    airport: &Airport,
    role: &str,
    theme: Option<&str>,
) -> Scene {
    if report
        .airplane
        .wings
        .first()
        .filter(|wing| wing.xsecs.len() >= 2)
        .is_none()
    {
        return status_message_scene(
            "Landing & Take-Off",
            "The analyzed report has no main wing; field performance cannot be computed.",
            theme,
        );
    }
    let wing_area = report
        .geometry_summary
        .get("wing_area_m2")
        .copied()
        .unwrap_or(report.airplane.s_ref);
    let tw_sl = static_thrust_to_weight(config, 0.30);
    let landing_mass_kg = (config.requirements.mtow_kg * config.mass_model.mlw_fraction_mtow)
        .clamp(0.0, config.requirements.mtow_kg);
    let performance = compute_field_performance_at_masses(
        config.requirements.mtow_kg,
        landing_mass_kg,
        wing_area,
        airport,
        config.performance.cl_max_to,
        config.performance.cl_max_land,
        tw_sl,
        config.performance.k_land,
        config.performance.bfl_factor,
        &config.performance,
    );
    draw_lto(&performance, role, theme)
}

fn missing_airport_scene(airport_name: &str, theme: Option<&str>) -> Scene {
    status_message_scene(
        "Landing & Take-Off",
        &format!("Airport '{airport_name}' not found."),
        theme,
    )
}

fn draw_lto(performance: &FieldPerformance, role: &str, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(900.0, 800.0, Some(Color::from_hex(pal.bg)));
    let title = format!("Landing & Take-Off - {role}");
    scene.title = Some(title.clone());
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();
    draw_runway(&mut scene, performance, pal);
    draw_bars(&mut scene, performance, pal);
    scene
}

fn draw_runway(scene: &mut Scene, performance: &FieldPerformance, pal: &crate::theme::Palette) {
    let left = 60.0;
    let top = 50.0;
    let field_width = 780.0;
    let height = 320.0;
    let toda = performance.toda_m();
    let rw_h = height * 0.13;
    let runway_y = top + height * 0.48;
    let margin = field_width * 0.05;
    let runway_width = field_width - 2.0 * margin;
    let runway_start = left + margin;
    let runway_end = runway_start + runway_width;
    // Keep annotations inside the runway panel when a required distance is
    // longer than the available runway.  The red bar below still records the
    // exceedance; the schematic must not grow past its green plot envelope.
    let x = |distance: f64| {
        (runway_start + distance.max(0.0) / toda.max(1e-6) * runway_width)
            .clamp(runway_start, runway_end)
    };

    scene.add(SceneElement::Rect {
        x: left,
        y: top,
        width: field_width,
        height,
        rx: 10.0,
        fill: Some(Fill::new(Color::from_hex(GRASS))),
        stroke: None,
    });
    scene.add(SceneElement::Rect {
        x: runway_start,
        y: runway_y,
        width: runway_width,
        height: rw_h,
        rx: 0.0,
        fill: Some(Fill::new(Color::from_hex(RUNWAY))),
        stroke: Some(Stroke::new(Color::from_hex("#888888"), 1.5)),
    });
    let dash_len = runway_width * 0.04;
    let dash_gap = runway_width * 0.04;
    let mut dash_x = runway_start + dash_gap;
    while dash_x + dash_len < runway_end {
        scene.add(SceneElement::Line {
            p1: [dash_x, runway_y + rw_h * 0.5],
            p2: [dash_x + dash_len, runway_y + rw_h * 0.5],
            stroke: Stroke::new(Color::from_hex(STRIPE), 1.5),
        });
        dash_x += dash_len + dash_gap;
    }
    for edge in [runway_start, runway_end] {
        scene.add(SceneElement::Rect {
            x: edge - 4.0,
            y: runway_y,
            width: 8.0,
            height: rw_h,
            rx: 0.0,
            fill: Some(Fill::new(Color::from_hex(STRIPE))),
            stroke: None,
        });
    }

    let mut upper_distances = [
        ("TODR", performance.todr_m, "#3498db"),
        ("BFL", performance.bfl_m, "#e67e22"),
    ];
    upper_distances.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (index, (name, distance, colour)) in upper_distances.iter().enumerate() {
        draw_distance_arrow(
            scene,
            x(0.0),
            x(*distance),
            runway_y - rw_h * (2.30 - index as f64 * 0.85),
            Color::from_hex(colour),
            &format!("{name}  {} m", format_thousands(*distance)),
        );
    }
    let mut lower_distances = [
        ("ASD", performance.asd_m, "#e74c3c"),
        ("LDR", performance.ldr_m, "#2ecc71"),
    ];
    lower_distances.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (index, (name, distance, colour)) in lower_distances.iter().enumerate() {
        draw_distance_arrow(
            scene,
            x(0.0),
            x(*distance),
            runway_y + rw_h * (2.0 + index as f64 * 0.85),
            Color::from_hex(colour),
            &format!("{name}  {} m", format_thousands(*distance)),
        );
    }

    let v2 = performance.v_speeds.v2_ms;
    let mut speeds = vec![
        ("V1", performance.v_speeds.v1_ms, "#f1c40f"),
        ("VR", performance.v_speeds.v_r_ms, "#e67e22"),
        ("V2", v2, "#3498db"),
    ];
    speeds.sort_by(|a, b| {
        let a_pos = if v2 > 0.0 {
            (a.1 / v2).powi(2).min(1.0)
        } else {
            0.0
        };
        let b_pos = if v2 > 0.0 {
            (b.1 / v2).powi(2).min(1.0)
        } else {
            0.0
        };
        a_pos.total_cmp(&b_pos)
    });
    let runway_center_y = runway_y + rw_h * 0.5;
    let marker_half_length = rw_h * 1.15;
    for (name, speed, colour) in speeds {
        let position = if v2 > 0.0 {
            x(toda * (speed / v2).powi(2).min(1.0) * 0.75)
        } else {
            x(0.0)
        };
        scene.add(SceneElement::Line {
            p1: [position, runway_center_y - marker_half_length],
            p2: [position, runway_center_y + marker_half_length],
            stroke: Stroke::dashed(Color::from_hex(colour), 1.2, 4.0, 3.0),
        });
        let _ = name;
        let _ = speed;
    }
    for (index, (name, speed, colour)) in [
        ("V1", performance.v_speeds.v1_ms, "#f1c40f"),
        ("VR", performance.v_speeds.v_r_ms, "#e67e22"),
        ("V2", performance.v_speeds.v2_ms, "#3498db"),
    ]
    .iter()
    .enumerate()
    {
        scene.add(SceneElement::Text {
            text: format!("{name} = {:.0} kt", speed * MS_TO_KT),
            pos: [
                left + field_width - 10.0,
                top + height - 44.0 + index as f64 * 11.0,
            ],
            font_size: 8.0,
            color: Color::from_hex(colour),
            align: TextAlign::Right,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: true,
        });
    }
    scene.add(SceneElement::Text {
        text: format!(
            "{}  |  elev {:.0} m  ISA+{:.0}C\nTODA = {} m   LDA = {} m",
            performance.airport.name,
            performance.airport.elevation_m,
            performance.airport.isa_deviation_c,
            format_thousands(toda),
            format_thousands(performance.lda_m()),
        ),
        pos: [left + field_width * 0.5, top + 8.0],
        font_size: 9.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });
}

fn draw_distance_arrow(scene: &mut Scene, x0: f64, x1: f64, y: f64, colour: Color, label: &str) {
    draw_arrow(scene, [x0, y], [x1, y], colour, 1.5);
    scene.add(SceneElement::Text {
        text: label.to_owned(),
        pos: [(x0 + x1) * 0.5, y - 7.0],
        font_size: 8.0,
        color: colour,
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
}

fn draw_bars(scene: &mut Scene, performance: &FieldPerformance, pal: &crate::theme::Palette) {
    let left = 100.0;
    let top = 500.0;
    let width = 700.0;
    let height = 220.0;
    let labels = ["TODR", "BFL", "ASD", "LDR"];
    let values = [
        performance.todr_m,
        performance.bfl_m,
        performance.asd_m,
        performance.ldr_m,
    ];
    let available = [
        performance.toda_m(),
        performance.toda_m(),
        performance.toda_m(),
        performance.lda_m(),
    ];
    let colours = ["#3498db", "#e67e22", "#e74c3c", "#2ecc71"];
    let max_value = available
        .iter()
        .chain(values.iter())
        .copied()
        .fold(0.0, f64::max)
        .max(1.0);
    let scale = height / (max_value * 1.2);
    let slot = width / labels.len() as f64;
    let bar_width = slot * 0.58;
    for (i, label) in labels.iter().enumerate() {
        let center = left + slot * (i as f64 + 0.5);
        let available_height = available[i] * scale;
        scene.add(SceneElement::Rect {
            x: center - bar_width * 0.65,
            y: top + height - available_height,
            width: bar_width * 1.3,
            height: available_height,
            rx: 0.0,
            fill: Some(Fill::new(Color::from_hex("#888888"))),
            stroke: None,
        });
        let required_height = values[i] * scale;
        let feasible = values[i] <= available[i];
        scene.add(SceneElement::Rect {
            x: center - bar_width * 0.5,
            y: top + height - required_height,
            width: bar_width,
            height: required_height,
            rx: 0.0,
            fill: Some(Fill::new(Color::from_hex(colours[i]))),
            stroke: if feasible {
                None
            } else {
                Some(Stroke::new(Color::from_hex("#ff0000"), 1.5))
            },
        });
        scene.add(SceneElement::Text {
            text: format_thousands(values[i]),
            pos: [center, top + height - required_height - 5.0],
            font_size: 8.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Center,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
        scene.add(SceneElement::Text {
            text: (*label).to_owned(),
            pos: [center, top + height + 16.0],
            font_size: 9.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Center,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
        if !feasible {
            scene.add(SceneElement::Text {
                text: "EXCEEDS\nRUNWAY".to_owned(),
                pos: [center, top + height - available_height * 0.5],
                font_size: 7.5,
                color: Color::from_hex("#ff4444"),
                align: TextAlign::Center,
                baseline: TextBaseline::Middle,
                angle_deg: 0.0,
                bold: true,
            });
        }
    }
    scene.add(SceneElement::Rect {
        x: left,
        y: top,
        width,
        height,
        rx: 0.0,
        fill: None,
        stroke: Some(Stroke::new(Color::from_hex(pal.spine), 1.0)),
    });
    scene.add(SceneElement::Text {
        text: "Required vs Available".to_owned(),
        pos: [left + width * 0.5, top - 18.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
    scene.add(SceneElement::Text {
        text: "Distance [m]".to_owned(),
        pos: [left - 62.0, top + height * 0.5],
        font_size: 10.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });

    // The category labels identify the bars, but the reference panel also
    // carries a numeric distance scale.  Draw it explicitly because this
    // panel is not a continuous Axes2D series.
    let max_value = max_value * 1.2;
    let step = (max_value / 5.0).max(1.0);
    let mut tick = 0.0;
    while tick <= max_value + step * 1e-9 {
        let y = top + height - tick / max_value * height;
        scene.add(SceneElement::Line {
            p1: [left - 4.0, y],
            p2: [left, y],
            stroke: Stroke::new(Color::from_hex(pal.spine), 1.0),
        });
        scene.add(SceneElement::Text {
            text: format_thousands(tick),
            pos: [left - 8.0, y],
            font_size: 7.5,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Right,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
        tick += step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::airports::Airport;

    fn sample_performance() -> FieldPerformance {
        FieldPerformance {
            airport: Airport::custom("Fixture", 100.0, 3000.0, 2800.0, 10.0, 0.0, 0.0),
            v_speeds: alas_perf::performance::VSpeeds {
                v_stall_to_ms: 60.0,
                v_stall_land_ms: 55.0,
                v_mc_ms: 68.0,
                v1_ms: 70.0,
                v_r_ms: 74.0,
                v2_ms: 82.0,
                v_app_ms: 72.0,
                v_td_ms: 64.0,
            },
            todr_m: 2200.0,
            bfl_m: 2400.0,
            asd_m: 2400.0,
            ldr_m: 2900.0,
            landing_mass_kg: 72_000.0,
        }
    }

    #[test]
    fn the_lto_renderer_contains_all_required_distances_and_speeds() {
        let performance = sample_performance();
        let scene = draw_lto(&performance, "Departure", None);
        let labels: Vec<&str> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        for label in ["TODR", "BFL", "ASD", "LDR", "V1", "VR", "V2"] {
            assert!(
                labels.iter().any(|text| text.contains(label)),
                "missing {label}"
            );
        }
        let distance_label_x = scene.elements.iter().find_map(|element| match element {
            SceneElement::Text { text, pos, .. } if text == "Distance [m]" => Some(pos[0]),
            _ => None,
        });
        assert!(distance_label_x.is_some_and(|x| x <= 40.0));
    }
}
