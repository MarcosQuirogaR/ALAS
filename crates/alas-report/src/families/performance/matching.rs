// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/sidecar/figures_extra.py (`figure_matching_chart`).
// Reference: alas @ rust-port-baseline.

//! The configuration- and report-driven aircraft sizing matching chart.

use alas_config::AlasConfig;
use alas_perf::performance::{build_matching_chart, far25_oei_gradient, MatchingChartData};
use alas_pipeline::full_analysis::AnalysisReport;

use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::families::performance::support::{
    resolve_airport, static_thrust_to_weight, status_message_scene,
};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

const AIRPORT_COLOURS: [&str; 6] = [
    "#e74c3c", "#e67e22", "#f1c40f", "#2ecc71", "#1abc9c", "#9b59b6",
];

/// Generate the matching chart from the analyzed polar, live geometry and
/// configured requirements/field constraints.
pub fn figure_matching_chart(
    report: &AnalysisReport,
    config: &AlasConfig,
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
            "Matching Chart",
            "The analyzed report has no main wing; matching constraints cannot be computed.",
            theme,
        );
    }
    let departure = match resolve_airport(&config.departure_airport) {
        Ok(airport) => airport,
        Err(_) => {
            return status_message_scene(
                "Matching Chart",
                "Departure/arrival airport not found in the database.",
                theme,
            )
        }
    };
    let arrival = match resolve_airport(&config.arrival_airport) {
        Ok(airport) => airport,
        Err(_) => {
            return status_message_scene(
                "Matching Chart",
                "Departure/arrival airport not found in the database.",
                theme,
            )
        }
    };

    let performance = &config.performance;
    let requirements = &config.requirements;
    let wing_area = report
        .geometry_summary
        .get("wing_area_m2")
        .copied()
        .unwrap_or(report.airplane.s_ref);
    let n_engines = config.geometry.engine.spanwise_positions_m.len() as i64;
    let oei_gradient = far25_oei_gradient(n_engines).unwrap_or(performance.oei_gradient);
    let tw_design = static_thrust_to_weight(config, 0.30);
    let data = build_matching_chart(
        report.polar_fit.cd0,
        report.polar_fit.k,
        requirements.cruise_mach,
        requirements.cruise_altitude_m,
        requirements.mtow_kg,
        wing_area,
        n_engines,
        &[departure.clone(), arrival.clone()],
        Some(performance.cl_max_to),
        Some(performance.cl_max_land),
        Some(performance.thrust_lapse),
        Some(oei_gradient),
        Some(performance.k_land),
        Some(performance.oei_climb_cl),
        Some(performance.oei_climb_delta_cd),
        Some(tw_design),
        performance.matching_chart_resolution,
        Some(performance.ws_min_pa),
        Some(performance.ws_max_pa),
    );

    draw_matching_chart(&data, oei_gradient, theme)
}

fn draw_matching_chart(data: &MatchingChartData, oei_gradient: f64, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    if data.ws_pa.is_empty() {
        return status_message_scene(
            "Matching Chart",
            "Matching chart resolution produced no wing-loading points.",
            theme,
        );
    }

    let ws_kg: Vec<f64> = data.ws_pa.iter().map(|ws| ws / 9.81).collect();
    let mut tw_floor = data.tw_cruise.clone();
    for value in &mut tw_floor {
        *value = value.max(data.tw_oei_climb);
    }
    for (_, curve) in &data.tw_takeoff {
        for (floor, value) in tw_floor.iter_mut().zip(curve) {
            *floor = floor.max(*value);
        }
    }
    let floor_max = tw_floor
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(data.tw_oei_climb.max(0.0), f64::max);
    // Preserve f64::min/max NaN handling used by the translated calculation.
    #[allow(clippy::manual_clamp)]
    let y_max = (floor_max * 1.4).min(0.6).max(0.1);

    let mut scene = Scene::new(900.0, 600.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Matching Chart".to_owned());
    draw_title(&mut scene, "Matching Chart", pal);
    scene.suppress_derived_title();
    let axes = Axes2D::new(
        (80.0, 45.0, 565.0, 410.0),
        (ws_kg[0], *ws_kg.last().unwrap_or(&ws_kg[0])),
        (0.0, y_max),
    );

    axes.add_line_series(
        &mut scene,
        &ws_kg
            .iter()
            .copied()
            .zip(data.tw_cruise.iter().copied())
            .collect::<Vec<_>>(),
        Stroke::new(Color::from_hex("#3498db"), 2.0),
    );
    axes.add_line_series(
        &mut scene,
        &[
            (ws_kg[0], data.tw_oei_climb),
            (*ws_kg.last().unwrap_or(&ws_kg[0]), data.tw_oei_climb),
        ],
        Stroke::dashed(Color::from_hex("#9b59b6"), 1.8, 5.0, 4.0),
    );
    for (index, (name, curve)) in data.tw_takeoff.iter().enumerate() {
        let colour = Color::from_hex(AIRPORT_COLOURS[index % AIRPORT_COLOURS.len()]);
        axes.add_line_series(
            &mut scene,
            &ws_kg
                .iter()
                .copied()
                .zip(curve.iter().copied())
                .collect::<Vec<_>>(),
            Stroke::dashed(colour, 1.6, 6.0, 3.0),
        );
        if let Some((_, limit)) = data
            .ws_land_limits
            .iter()
            .find(|(airport, _)| airport == name)
        {
            axes.add_line_series(
                &mut scene,
                &[(*limit / 9.81, 0.0), (*limit / 9.81, y_max)],
                Stroke::dashed(colour, 1.4, 2.0, 4.0),
            );
        }
    }

    let mut fill_points: Vec<[f64; 2]> = ws_kg
        .iter()
        .copied()
        .zip(tw_floor.iter().copied())
        .map(|(ws, tw)| axes.map_point(ws, tw))
        .collect();
    fill_points.extend(ws_kg.iter().rev().map(|ws| axes.map_point(*ws, 0.0)));
    scene.add(SceneElement::Polygon {
        points: fill_points,
        fill: Some(Fill::new(Color::rgba(231, 76, 60, 31))),
        stroke: None,
    });

    if let (Some(ws), Some(tw)) = (data.design_ws_pa, data.design_tw) {
        let center = axes.map_point(ws / 9.81, tw);
        scene.add(SceneElement::Circle {
            center,
            radius: 5.0,
            fill: Some(Fill::new(Color::from_hex("#f1c40f"))),
            stroke: Some(Stroke::new(Color::from_hex("#ffffff"), 0.8)),
        });
    }

    axes.draw_frame(&mut scene, pal);
    add_axis_labels(&mut scene, &axes, pal);
    scene.add(SceneElement::Text {
        text: "FEASIBLE\nDESIGN SPACE".to_owned(),
        pos: axes.map_point(
            ws_kg[0] + 0.5 * (ws_kg[ws_kg.len() - 1] - ws_kg[0]),
            y_max * 0.25,
        ),
        font_size: 13.0,
        color: Color::from_hex("#2ecc71"),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });

    let mut legend = vec![
        (
            "Cruise (T/W0 floor)".to_owned(),
            LegendMarker::Line(Stroke::new(Color::from_hex("#3498db"), 2.0)),
        ),
        (
            format!("OEI climb >= {:.1}%", oei_gradient * 100.0),
            LegendMarker::Line(Stroke::dashed(Color::from_hex("#9b59b6"), 1.8, 5.0, 4.0)),
        ),
    ];
    for (index, (name, _)) in data.tw_takeoff.iter().enumerate() {
        let colour = Color::from_hex(AIRPORT_COLOURS[index % AIRPORT_COLOURS.len()]);
        legend.push((
            format!("T/O  {name}"),
            LegendMarker::Line(Stroke::dashed(colour, 1.6, 6.0, 3.0)),
        ));
        legend.push((
            format!("Land {name}"),
            LegendMarker::Line(Stroke::dashed(colour, 1.4, 2.0, 4.0)),
        ));
    }
    draw_legend(&mut scene, [675.0, 70.0], &legend, pal, 8.0);
    scene
}

fn add_axis_labels(scene: &mut Scene, axes: &Axes2D, pal: &crate::theme::Palette) {
    scene.add(SceneElement::Text {
        text: "Wing loading  W/S  [kg/m2]".to_owned(),
        pos: [axes.left + axes.width * 0.5, axes.top + axes.height + 30.0],
        font_size: 10.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: "Thrust-to-weight  T0/W0  [-]".to_owned(),
        pos: [axes.left - 50.0, axes.top + axes.height * 0.5],
        font_size: 10.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_matching_chart_with_no_points_degrades_to_a_status_scene() {
        let scene = draw_matching_chart(
            &MatchingChartData {
                ws_pa: Vec::new(),
                tw_cruise: Vec::new(),
                tw_oei_climb: 0.0,
                tw_takeoff: Vec::new(),
                ws_land_limits: Vec::new(),
                design_ws_pa: None,
                design_tw: None,
            },
            0.024,
            None,
        );
        assert_eq!(scene.title.as_deref(), Some("Matching Chart"));
    }
}
