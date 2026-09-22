// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Technology-specific replacements behind the stable propulsion figure API.

use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::scene::{Axes2D, Color, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_config::{ActiveEngineModel, AlasConfig, TurbopropEngineSpec};
use alas_prop::turboprop::{
    Pw127m568fModel, Pw127mRating, TurbopropCommand, TurbopropCondition, TurbopropMode,
    TurbopropOutput,
};

const NOTICE: &str =
    "EXTRAPOLATED \u{b7} unvalidated generic six-blade surrogate; not an OEM 568F map";

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

fn model(config: &AlasConfig) -> Pw127m568fModel {
    let spec = spec(config);
    Pw127m568fModel {
        normal_takeoff_power_w: spec.takeoff_shaft_power_kw * 1_000.0,
        maximum_takeoff_reserve_power_w: spec.maximum_reserve_shaft_power_kw * 1_000.0,
        maximum_continuous_power_w: spec.maximum_continuous_shaft_power_kw * 1_000.0,
        maximum_climb_power_w: spec.maximum_climb_shaft_power_kw * 1_000.0,
        maximum_cruise_power_w: spec.maximum_cruise_shaft_power_kw * 1_000.0,
        governed_propeller_speed_rpm: spec.governed_propeller_speed_rpm,
        propeller_diameter_m: spec.propeller_diameter_m,
        reference_psfc_kg_kwh: spec.maximum_cruise_fuel_flow_kg_h
            / (2.0 * spec.maximum_cruise_shaft_power_kw),
        ..Pw127m568fModel::default()
    }
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
    scene.add(SceneElement::Text {
        text: format!(
            "{} / {} \u{b7} {NOTICE}",
            config.geometry.engine.engine_name,
            spec(config).propeller_model
        ),
        pos: [size.0 * 0.5, size.1 - 13.0],
        font_size: 7.6,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}

fn label(scene: &mut Scene, theme: Option<&str>, text: &str, pos: [f64; 2], angle: f64) {
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
    let title = "PW127M / 568F Power\u{2013}Speed Operating Envelope";
    let mut scene = base(config, theme, title, (700.0, 500.0));
    let pal = get_palette(theme);
    let speeds = super::linspace(5.0, 150.0, 50);
    let rect = (70.0, 60.0, 580.0, 370.0);
    let axes = Axes2D::new(rect, (5.0, 150.0), (0.0, 22.0));
    axes.draw_frame(&mut scene, pal);
    let fractions = [
        (0.4, "40% power"),
        (0.6, "60% power"),
        (0.8, "80% power"),
        (1.0, "100% power"),
    ];
    let colors = ["#3498db", "#2ecc71", "#f39c12", "#c0392b"];
    let mut legend = Vec::new();
    for ((fraction, name), color) in fractions.into_iter().zip(colors) {
        let points = finite_series(speeds.iter().copied().map(|speed| {
            (
                speed,
                output(config, speed, 1.225, fraction, Pw127mRating::NormalTakeoff)
                    .map(|v| v.total_thrust_n / 1_000.0),
            )
        }));
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
    draw_legend(&mut scene, [82.0, 74.0], &legend, pal, 8.0);
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
        "PW127M Certified Shaft Ratings and Surrogate Outputs",
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
                rating.shaft_power_w() / 1_000.0,
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
        "PW127M / 568F Propulsion Operating-Point Summary",
        (700.0, 460.0),
    );
    let pal = get_palette(theme);
    for (index, line) in turboprop_summary_lines(config).iter().enumerate() {
        scene.add(SceneElement::Text {
            text: line.clone(),
            pos: [55.0, 70.0 + index as f64 * 34.0],
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
        lines.extend([
            format!(
                "Per-engine shaft command: {:.0} kW ({:.0}% of {:.0} kW MCT)",
                value.engine_shaft_power_w / 1_000.0,
                power_fraction * 100.0,
                Pw127mRating::MaximumContinuous.shaft_power_w() / 1_000.0,
            ),
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
    lines.push(NOTICE.to_owned());
    lines.push("No PW127M altitude-lapse, fuel deck, flight-idle, reverse-beta, feather or windmilling map is claimed.".to_owned());
    lines
}

pub(super) fn turboprop_speed_power_scene(config: &AlasConfig, theme: Option<&str>) -> Scene {
    if let Some(scene) = binding_error_scene(config, theme, (1000.0, 480.0)) {
        return scene;
    }
    let mut scene = base(
        config,
        theme,
        "Turboprop Force and Fuel vs Speed and Power",
        (1000.0, 480.0),
    );
    let pal = get_palette(theme);
    let speeds = super::linspace(5.0, 150.0, 50);
    let powers = [(0.4, "40%"), (0.6, "60%"), (0.8, "80%"), (1.0, "100%")];
    let colors = ["#3498db", "#2ecc71", "#f39c12", "#c0392b"];
    let left = Axes2D::new((55.0, 65.0, 390.0, 330.0), (5.0, 150.0), (0.0, 22.0));
    let right = Axes2D::new((555.0, 65.0, 390.0, 330.0), (5.0, 150.0), (0.0, 0.16));
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
    draw_legend(&mut scene, [70.0, 78.0], &legend, pal, 7.5);
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
        "Per-engine fuel flow [kg/s]",
        [518.0, 230.0],
        -90.0,
    );
    label(
        &mut scene,
        theme,
        "True airspeed [m/s]",
        [250.0, 416.0],
        0.0,
    );
    label(
        &mut scene,
        theme,
        "True airspeed [m/s]",
        [750.0, 416.0],
        0.0,
    );
    scene
}
