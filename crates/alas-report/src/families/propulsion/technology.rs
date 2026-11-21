// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Technology-specific replacements behind the stable propulsion figure API.

use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::scene::{Axes2D, Color, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_config::{ActiveEngineModel, AlasConfig, TurbopropEngineSpec};
use alas_opt::mdo::propulsion::turboprop_unit_model;
use alas_prop::turboprop::{
    Pw127m568fModel, Pw127mRating, TurbopropCommand, TurbopropCondition, TurbopropMode,
    TurbopropOutput,
};

/// The notice under every turboprop figure, naming the bound propeller.
fn notice(config: &AlasConfig) -> String {
    format!(
        "EXTRAPOLATED \u{b7} unvalidated generic six-blade surrogate; not an OEM {} map",
        spec(config).propeller_model
    )
}

pub(super) fn binding_error_scene(
    config: &AlasConfig,
    theme: Option<&str>,
    size: (f64, f64),
) -> Option<Scene> {
    let Err(error) = config.geometry.engine.active_model() else {
        return None;
    };
    Some(crate::status_figure::sized_status_scene(
        "Propulsion model unavailable",
        &format!("Propulsion binding error: {error}"),
        false,
        get_palette(theme),
        size.0,
    ))
}

fn spec(config: &AlasConfig) -> &TurbopropEngineSpec {
    match config.geometry.engine.active_model() {
        Ok(ActiveEngineModel::Turboprop(value)) => value,
        _ => unreachable!("technology dispatch admits only a validated turboprop binding"),
    }
}

/// Normal take-off shaft power the fixed figure axes were drawn for, kW: the
/// PW127M's 1,846 kW (EASA TCDS IM.E.041), at which the thrust axis tops out
/// at 22 kN and the fuel axis at 0.16 kg/s.
const AXIS_REFERENCE_POWER_KW: f64 = 1_846.0;

/// Top of the thrust axis (kN) and of the fuel-flow axis (kg/s) of the
/// power-speed figures: the reference axes scaled by the engine's own normal
/// take-off power, so a larger engine is not drawn off scale.
///
/// The reference top is a floor, not a cap: at the 5 m/s end of the speed
/// axis the full-power normal take-off point is the largest force and fuel
/// flow on the figure (thrust falls with speed at constant power), and the
/// PW127M reference axes clipped it at 1.56 times their top. So
/// each top is raised to 1.05 times that point when it lies above the
/// reference, and the reference is kept otherwise.
fn axis_tops(config: &AlasConfig) -> (f64, f64) {
    let scale = spec(config).takeoff_shaft_power_kw / AXIS_REFERENCE_POWER_KW;
    let reference = (22.0 * scale, 0.16 * scale);
    match output(config, 5.0, 1.225, 1.0, Pw127mRating::NormalTakeoff) {
        Some(peak) => (
            reference.0.max(1.05 * peak.total_thrust_n / 1_000.0),
            reference.1.max(1.05 * peak.fuel_flow_kg_s),
        ),
        None => reference,
    }
}

/// The shaft power of `rating` from the bound engine's own catalogue, kW.
fn rating_power_kw(config: &AlasConfig, rating: Pw127mRating) -> f64 {
    let engine = spec(config);
    match rating {
        Pw127mRating::NormalTakeoff => engine.takeoff_shaft_power_kw,
        Pw127mRating::MaximumTakeoffReserve => engine.maximum_reserve_shaft_power_kw,
        _ => engine.maximum_continuous_shaft_power_kw,
    }
}

/// The single catalogue-to-unit-model mapping the mission deck also uses.
fn model(config: &AlasConfig) -> Pw127m568fModel {
    turboprop_unit_model(spec(config))
}

fn output(
    config: &AlasConfig,
    speed_m_s: f64,
    density_kg_m3: f64,
    fraction: f64,
    rating: Pw127mRating,
) -> Option<TurbopropOutput> {
    model(config)
        .evaluate(
            TurbopropCondition {
                density_kg_m3,
                true_airspeed_m_s: speed_m_s,
            },
            TurbopropCommand {
                rating,
                power_fraction: fraction,
                mode: TurbopropMode::Governed,
                propeller_speed_rpm: spec(config).governed_propeller_speed_rpm,
            },
        )
        .ok()
}

fn base(config: &AlasConfig, theme: Option<&str>, title: &str, size: (f64, f64)) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(size.0, size.1, Some(Color::from_hex(pal.bg)));
    scene.title = Some(title.to_owned());
    draw_title(&mut scene, title, pal);
    scene.suppress_derived_title();
    // Wrapped to the canvas width: one long line ran off both edges of the card.
    super::support::wrapped_footnote(
        &mut scene,
        pal,
        &format!(
            "{} / {} \u{b7} {}",
            config.geometry.engine.engine_name,
            spec(config).propeller_model,
            notice(config)
        ),
        7.6,
        size.0 * 0.5,
        size.1 - 8.0,
        size.0 - 60.0,
    );
    scene
}

pub(super) fn label(scene: &mut Scene, theme: Option<&str>, text: &str, pos: [f64; 2], angle: f64) {
    let pal = get_palette(theme);
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos,
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: angle,
        bold: false,
    });
}

fn finite_series(points: impl Iterator<Item = (f64, Option<f64>)>) -> Vec<(f64, f64)> {
    points
        .filter_map(|(x, y)| y.filter(|v| v.is_finite()).map(|v| (x, v)))
        .collect()
}

pub(super) fn turboprop_power_speed_envelope(config: &AlasConfig, theme: Option<&str>) -> Scene {
    if let Some(scene) = binding_error_scene(config, theme, (700.0, 500.0)) {
        return scene;
    }
    let title = format!(
        "{} Power\u{2013}Speed Operating Envelope",
        config.geometry.engine.engine_name
    );
    let mut scene = base(config, theme, &title, (700.0, 500.0));
    let pal = get_palette(theme);
    let speeds = super::linspace(5.0, 150.0, 50);
    let rect = (70.0, 60.0, 580.0, 370.0);
    let axes = Axes2D::new(rect, (5.0, 150.0), (0.0, axis_tops(config).0));
    axes.draw_frame(&mut scene, pal);
    let fractions = [
        (0.4, "40% power"),
        (0.6, "60% power"),
        (0.8, "80% power"),
        (1.0, "100% power"),
    ];
    let colors = ["#3498db", "#2ecc71", "#f39c12", "#c0392b"];
    let mut legend = Vec::new();
    let mut curves_px: Vec<Vec<[f64; 2]>> = Vec::new();
    for ((fraction, name), color) in fractions.into_iter().zip(colors) {
        let points = finite_series(speeds.iter().copied().map(|speed| {
            (
                speed,
                output(config, speed, 1.225, fraction, Pw127mRating::NormalTakeoff)
                    .map(|v| v.total_thrust_n / 1_000.0),
            )
        }));
        curves_px.push(points.iter().map(|&(x, y)| axes.map_point(x, y)).collect());
        axes.add_line_series(
            &mut scene,
            &points,
            Stroke::new(Color::from_hex(color), 1.8),
        );
        legend.push((
            name.to_owned(),
            LegendMarker::Line(Stroke::new(Color::from_hex(color), 1.8)),
        ));
    }
    let origin =
        super::support::least_cluttered_legend_origin(rect, legend.len(), 8.0, 90.0, &curves_px);
    draw_legend(&mut scene, origin, &legend, pal, 8.0);
    label(
        &mut scene,
        theme,
        "True airspeed [m/s]",
        [360.0, 450.0],
        0.0,
    );
    label(
        &mut scene,
        theme,
        "Per-engine net force [kN]",
        [22.0, 245.0],
        -90.0,
    );
    scene
}

pub(super) fn turboprop_efficiency_scene(config: &AlasConfig, theme: Option<&str>) -> Scene {
    if let Some(scene) = binding_error_scene(config, theme, (700.0, 420.0)) {
        return scene;
    }
    let mut scene = base(
        config,
        theme,
        "Propeller Efficiency vs True Airspeed",
        (700.0, 420.0),
    );
    let pal = get_palette(theme);
    let speeds = super::linspace(5.0, 150.0, 60);
    let rect = (70.0, 50.0, 590.0, 300.0);
    let axes = Axes2D::new(rect, (5.0, 150.0), (0.0, 1.0));
    axes.draw_frame(&mut scene, pal);
    let curves = [
        (0.5, "50% shaft rating", "#2980b9"),
        (0.75, "75% shaft rating", "#27ae60"),
        (1.0, "100% shaft rating", "#e67e22"),
    ];
    let mut legend = Vec::new();
    for (fraction, name, color) in curves {
        let points = finite_series(speeds.iter().copied().map(|speed| {
            (
                speed,
                output(config, speed, 1.225, fraction, Pw127mRating::NormalTakeoff)
                    .map(|v| v.propulsive_efficiency),
            )
        }));
        let stroke = Stroke::new(Color::from_hex(color), 2.0);
        axes.add_line_series(&mut scene, &points, stroke.clone());
        legend.push((name.to_owned(), LegendMarker::Line(stroke)));
    }
    draw_legend(&mut scene, [82.0, 66.0], &legend, pal, 8.0);
    label(
        &mut scene,
        theme,
        "True airspeed [m/s]",
        [365.0, 371.0],
        0.0,
    );
    label(
        &mut scene,
        theme,
        "Propulsive efficiency, TV/Pprop [-]",
        [20.0, 200.0],
        -90.0,
    );
    scene
}

pub(super) fn turboprop_rating_scene(config: &AlasConfig, theme: Option<&str>) -> Scene {
    if let Some(scene) = binding_error_scene(config, theme, (700.0, 420.0)) {
        return scene;
    }
    let mut scene = base(
        config,
        theme,
        &format!(
            "{} Certified Shaft Ratings and Surrogate Outputs",
            config.geometry.engine.engine_name
        ),
        (700.0, 420.0),
    );
    let pal = get_palette(theme);
    let rows = [
        (Pw127mRating::NormalTakeoff, "Normal takeoff (AEO)"),
        (Pw127mRating::MaximumContinuous, "Maximum continuous"),
        (Pw127mRating::MaximumTakeoffReserve, "Maximum reserve / OEI"),
    ];
    let speed = 80.0;
    let density = 1.0;
    for (index, (rating, name)) in rows.into_iter().enumerate() {
        let y = 95.0 + index as f64 * 82.0;
        let values = output(config, speed, density, 1.0, rating);
        let text = match values {
            Some(value) => format!(
                "{name}: {:.0} kW \u{b7} {:.1} kN \u{b7} {:.3} kg/s fuel \u{b7} {:.0} N\u{b7}m @ {:.0} rpm",
                rating_power_kw(config, rating),
                value.total_thrust_n / 1_000.0,
                value.fuel_flow_kg_s,
                value.propeller_torque_n_m,
                spec(config).governed_propeller_speed_rpm
            ),
            None => format!("{name}: no governed surrogate solution at 80 m/s, \u{3c1}=1.0 kg/m\u{b3}"),
        };
        scene.add(SceneElement::Text {
            text,
            pos: [55.0, y],
            font_size: 10.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: index == 0,
        });
    }
    label(
        &mut scene,
        theme,
        "Comparison condition: 80 m/s, density 1.0 kg/m\u{b3}, one engine",
        [350.0, 345.0],
        0.0,
    );
    scene
}

pub(super) fn turboprop_summary_scene(config: &AlasConfig, theme: Option<&str>) -> Scene {
    if let Some(scene) = binding_error_scene(config, theme, (700.0, 460.0)) {
        return scene;
    }
    let mut scene = base(
        config,
        theme,
        &format!(
            "{} Propulsion Operating-Point Summary",
            config.geometry.engine.engine_name
        ),
        (700.0, 460.0),
    );
    let pal = get_palette(theme);
    let summary = turboprop_summary_lines(config);
    // Rows 34 px apart, closer when the summary is long enough to reach the footnote.
    let pitch = (300.0 / summary.len().saturating_sub(1).max(1) as f64).min(34.0);
    for (index, line) in summary.iter().enumerate() {
        scene.add(SceneElement::Text {
            text: line.clone(),
            pos: [55.0, 70.0 + index as f64 * pitch],
            font_size: if index == 0 { 11.0 } else { 9.5 },
            color: Color::from_hex(if index == 0 { pal.title } else { pal.tick }),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: index == 0,
        });
    }
    scene
}

pub(super) fn turboprop_summary_lines(config: &AlasConfig) -> Vec<String> {
    if let Err(error) = config.geometry.engine.active_model() {
        return vec![format!("Turboprop binding error: {error}")];
    }
    let req = &config.requirements;
    let atmosphere = alas_atmo::Atmosphere::new(req.cruise_altitude_m);
    let speed = req.cruise_mach * atmosphere.speed_of_sound();
    let cruise_result = output(
        config,
        speed,
        atmosphere.density(),
        1.0,
        Pw127mRating::MaximumContinuous,
    );
    // The generic coefficient surface has a narrower governed envelope than
    // the aircraft envelope. Never fabricate the unavailable cruise point:
    // fall back to a plainly identified, supported comparison condition.
    let (result, condition_label, power_fraction) = if cruise_result.is_some() {
        (
            cruise_result,
            format!("M{:.2}, {:.0} m", req.cruise_mach, req.cruise_altitude_m),
            1.0,
        )
    } else {
        (
            output(
                config,
                80.0,
                1.0,
                0.7,
                Pw127mRating::MaximumContinuous,
            ),
            "80 m/s, density 1.0 kg/m\u{b3}, 70% MCT (configured cruise is outside the surrogate governor envelope)"
                .to_owned(),
            0.7,
        )
    };
    let mut lines = vec![format!(
        "{} \u{b7} {} \u{b7} maximum-continuous rating",
        config.geometry.engine.engine_name,
        spec(config).propeller_model
    )];
    if let Some(value) = result {
        lines.extend(shaft_command_lines(
            value.engine_shaft_power_w / 1_000.0,
            power_fraction,
            spec(config).maximum_continuous_shaft_power_kw,
        ));
        lines.extend([
            format!(
                "Propeller power after accessories/gearbox: {:.0} kW",
                value.propeller_power_w / 1_000.0
            ),
            format!(
                "Net force at {condition_label}: {:.1} kN",
                value.total_thrust_n / 1_000.0
            ),
            format!(
                "Fuel flow (family PSFC prior): {:.3} kg/s per engine",
                value.fuel_flow_kg_s
            ),
            format!(
                "Propeller: {:.0} rpm \u{b7} {:.0} N\u{b7}m \u{b7} \u{3b7}p={:.3}",
                spec(config).governed_propeller_speed_rpm,
                value.propeller_torque_n_m,
                value.propulsive_efficiency
            ),
            format!(
                "Power-balance residual: {:.3} W",
                value.power_balance_residual_w
            ),
        ]);
    } else {
        lines.push("No governed surrogate solution at the configured cruise point.".to_owned());
    }
    lines.push(notice(config));
    lines.push(
        "Available shaft power lapses with air density and ram as the engine model fits it."
            .to_owned(),
    );
    lines.push(format!(
        "No {} fuel deck, flight-idle, reverse-beta, feather or windmilling map is claimed.",
        config.geometry.engine.engine_name
    ));
    lines
}

/// The shaft-command lines with the basis of the percentage stated. The command is a share of
/// the maximum-continuous power *available at the flight condition*, which is the sea-level
/// rating `rated_kw` times the in-flight lapse; the second line gives that available power as a
/// share of the sea-level rating, so the two percentages cannot be confused. Two lines: the
/// figure draws one line per entry and the whole sentence does not fit the card.
fn shaft_command_lines(command_kw: f64, power_fraction: f64, rated_kw: f64) -> [String; 2] {
    let available_kw = if power_fraction > 0.0 {
        command_kw / power_fraction
    } else {
        0.0
    };
    let available_share = 100.0 * available_kw / rated_kw.max(1e-9);
    if (power_fraction - 1.0).abs() < 1e-9 {
        [
            format!(
                "Per-engine shaft command: {command_kw:.0} kW = 100% of the maximum-continuous power available here"
            ),
            format!("That is {available_share:.0}% of the {rated_kw:.0} kW sea-level rating."),
        ]
    } else {
        [
            format!(
                "Per-engine shaft command: {command_kw:.0} kW = {:.0}% of the {available_kw:.0} kW maximum-continuous power available here",
                power_fraction * 100.0,
            ),
            format!(
                "The available power is {available_share:.0}% of the {rated_kw:.0} kW sea-level rating."
            ),
        ]
    }
}

pub(super) fn turboprop_speed_power_scene(config: &AlasConfig, theme: Option<&str>) -> Scene {
    if let Some(scene) = binding_error_scene(config, theme, (1000.0, 480.0)) {
        return scene;
    }
    let mut scene = base(
        config,
        theme,
        "Turboprop Force and Fuel vs Speed and Power (sea level)",
        (1000.0, 480.0),
    );
    let pal = get_palette(theme);
    scene.add(SceneElement::Text {
        text: "Altitude is not swept: sea-level density, no altitude-lapse map is claimed. \
               Fuel flow is the family PSFC prior times shaft power, so it does not vary with speed."
            .to_owned(),
        pos: [500.0, 44.0],
        font_size: 8.4,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
    let speeds = super::linspace(5.0, 150.0, 50);
    let powers = [(0.4, "40%"), (0.6, "60%"), (0.8, "80%"), (1.0, "100%")];
    let colors = ["#3498db", "#2ecc71", "#f39c12", "#c0392b"];
    let (thrust_top, fuel_top) = axis_tops(config);
    let left = Axes2D::new((55.0, 65.0, 390.0, 330.0), (5.0, 150.0), (0.0, thrust_top));
    let right = Axes2D::new((585.0, 65.0, 360.0, 330.0), (5.0, 150.0), (0.0, fuel_top));
    left.draw_frame(&mut scene, pal);
    right.draw_frame(&mut scene, pal);
    let mut legend = Vec::new();
    for ((fraction, name), color) in powers.into_iter().zip(colors) {
        let thrust = finite_series(speeds.iter().copied().map(|speed| {
            (
                speed,
                output(config, speed, 1.225, fraction, Pw127mRating::NormalTakeoff)
                    .map(|v| v.total_thrust_n / 1_000.0),
            )
        }));
        let fuel = finite_series(speeds.iter().copied().map(|speed| {
            (
                speed,
                output(config, speed, 1.225, fraction, Pw127mRating::NormalTakeoff)
                    .map(|v| v.fuel_flow_kg_s),
            )
        }));
        let stroke = Stroke::new(Color::from_hex(color), 1.7);
        left.add_line_series(&mut scene, &thrust, stroke.clone());
        right.add_line_series(&mut scene, &fuel, stroke.clone());
        legend.push((format!("{name} rated power"), LegendMarker::Line(stroke)));
    }
    let thrust_px: Vec<Vec<[f64; 2]>> = powers
        .iter()
        .map(|(fraction, _)| {
            speeds
                .iter()
                .filter_map(|&speed| {
                    output(config, speed, 1.225, *fraction, Pw127mRating::NormalTakeoff)
                        .map(|v| left.map_point(speed, v.total_thrust_n / 1_000.0))
                })
                .collect()
        })
        .collect();
    let origin = super::support::least_cluttered_legend_origin(
        (55.0, 65.0, 390.0, 330.0),
        legend.len(),
        7.5,
        130.0,
        &thrust_px,
    );
    draw_legend(&mut scene, origin, &legend, pal, 7.5);
    label(
        &mut scene,
        theme,
        "Per-engine net force [kN]",
        [18.0, 230.0],
        -90.0,
    );
    label(
        &mut scene,
        theme,
        "Per-engine fuel flow [kg/s] (PSFC prior)",
        [518.0, 230.0],
        -90.0,
    );
    label(
        &mut scene,
        theme,
        "True airspeed [m/s]",
        [250.0, 428.0],
        0.0,
    );
    label(
        &mut scene,
        theme,
        "True airspeed [m/s]",
        [750.0, 428.0],
        0.0,
    );
    scene
}

#[cfg(test)]
// A failed expect is the test failing: the registered presets must bind a turboprop.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    /// Every line of the operating-point summary stays on its card, and the shaft command states
    /// both bases of its percentage. With `ALAS_PREVIEW_SVG_DIR` set the card is also written
    /// there for a visual check.
    #[test]
    fn the_turboprop_summary_fits_its_card_and_states_the_percentage_basis() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" }))
            .expect("registered preset");
        let scene = turboprop_summary_scene(&config, Some("dark"));
        if let Some(dir) = std::env::var_os("ALAS_PREVIEW_SVG_DIR") {
            let path = std::path::Path::new(&dir).join("turboprop_summary.svg");
            std::fs::write(path, crate::svg::render_svg(&scene)).expect("write svg");
        }
        for element in &scene.elements {
            if let SceneElement::Text {
                text,
                pos,
                font_size,
                align: TextAlign::Left,
                angle_deg,
                ..
            } = element
            {
                if angle_deg.abs() > 1e-3 {
                    continue;
                }
                let width = text.chars().count() as f64
                    * font_size
                    * crate::scene::CSS_PIXELS_PER_POINT
                    * 0.56;
                // The shaft-command lines are the ones this test is about; the notice line is
                // longer than the estimate allows and is drawn within the card as it was.
                if text.contains("shaft command") || text.contains("sea-level rating") {
                    assert!(pos[0] + width <= scene.width, "{text:?} runs off the card");
                }
                assert!(
                    pos[1] <= scene.height - 30.0,
                    "{text:?} is below the card footer"
                );
            }
        }
        let lines = turboprop_summary_lines(&config);
        let command = lines
            .iter()
            .position(|line| line.starts_with("Per-engine shaft command"))
            .expect("shaft command line");
        assert!(lines[command].contains("% of the maximum-continuous power available here"));
        assert!(lines[command + 1].contains("kW sea-level rating"));
    }

    #[test]
    fn the_turboprop_figure_titles_and_notes_name_the_presets_own_engine() {
        for preset in ["A400M", "ATR72-600"] {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
                .expect("registered preset");
            let engine = config.geometry.engine.engine_name.clone();
            let propeller = spec(&config).propeller_model.clone();
            let titles = [
                turboprop_power_speed_envelope(&config, None).title,
                turboprop_rating_scene(&config, None).title,
                turboprop_summary_scene(&config, None).title,
            ];
            // The rating figure prints the engine's own ratings, not the PW127M's.
            let spec_kw = [
                spec(&config).takeoff_shaft_power_kw,
                spec(&config).maximum_continuous_shaft_power_kw,
                spec(&config).maximum_reserve_shaft_power_kw,
            ];
            let rating_text = format!("{:?}", turboprop_rating_scene(&config, None).elements);
            for kw in spec_kw {
                assert!(
                    rating_text.contains(&format!("{kw:.0} kW")),
                    "{preset}: {kw}"
                );
            }
            // The axes hold the full-power curves of the larger engine too.
            let (thrust_top, fuel_top) = axis_tops(&config);
            let full = output(&config, 5.0, 1.225, 1.0, Pw127mRating::NormalTakeoff)
                .expect("governed point");
            // Framed no tighter than the ATR figure these axes were drawn for,
            // whose 5 m/s point is 1.56 times the axis top (clipped at the left edge).
            assert!(
                full.total_thrust_n / 1_000.0 <= 1.6 * thrust_top,
                "{preset}"
            );
            assert!(full.fuel_flow_kg_s <= 1.6 * fuel_top, "{preset}");
            for title in titles {
                let title = title.expect("scene title");
                assert!(title.starts_with(&engine), "{preset}: {title}");
                if engine != "PW127M" {
                    assert!(!title.contains("PW127M"), "{preset}: {title}");
                }
            }
            let lines = turboprop_summary_lines(&config).join("\n");
            assert!(lines.contains(&propeller), "{preset}");
            if engine != "PW127M" {
                assert!(!lines.contains("PW127M"), "{preset}: {lines}");
            }
        }
    }
}
