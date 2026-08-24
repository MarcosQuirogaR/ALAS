// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_payload_range`) and
// alas/physics/performance.py (`payload_range_diagram`, `wing_fuel_volume_m3`).
// Reference: alas @ rust-port-baseline.

//! The A-B-C-D payload-range envelope from the analyzed aircraft.
//!
//! The report crate owns this orchestration because the numerical payload-range
//! calculation consumes both a full-analysis report and configuration. Keeping
//! the inputs here prevents a renderer from quietly reverting to a plausible
//! transport-sized set of points when the aircraft or its fuel model changes.

use alas_atmo::Atmosphere;
use alas_config::AlasConfig;
use alas_geom::aircraft::wing::Wing;
use alas_perf::performance::breguet_range_m;
use alas_pipeline::full_analysis::AnalysisReport;

use super::support::format_thousands;
use crate::chart_kit::draw_title;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

const G: f64 = 9.81;
const M_TO_NM: f64 = 1852.0;
const BLUE: &str = "tab:blue";
const OEW_KEYS: &[&str] = &[
    "Wing",
    "H-Stab",
    "V-Stab",
    "Fuselage",
    "Gear",
    "Propulsion",
    "Systems",
    "Furnishings",
];

#[derive(Debug, Clone, Copy)]
struct PayloadRangePoint {
    label: &'static str,
    range_nm: f64,
    payload_kg: f64,
}

#[derive(Debug, Clone)]
struct PayloadRangeData {
    points: [PayloadRangePoint; 4],
    fuel_capacity_kg: f64,
    fuel_capacity_limit: &'static str,
    oew_kg: f64,
    mtow_kg: f64,
}

/// Generate the classic payload-range diagram from the report's masses,
/// geometry, aerodynamic point and the configuration's fuel model.
pub fn figure_payload_range(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let Some(data) = payload_range_data(report, config) else {
        return status_scene(
            "Payload-Range Diagram",
            "The analyzed report has no main wing; payload range cannot be computed.",
            theme,
        );
    };

    let ranges: Vec<f64> = data.points.iter().map(|point| point.range_nm).collect();
    let payloads_t: Vec<f64> = data
        .points
        .iter()
        .map(|point| point.payload_kg / 1000.0)
        .collect();
    let max_range = ranges.iter().copied().fold(0.0, f64::max).max(1.0);
    let max_payload = payloads_t.iter().copied().fold(0.0, f64::max).max(1.0);

    let mut scene = Scene::new(700.0, 500.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Payload-Range Diagram".to_owned());
    draw_title(&mut scene, "Payload-Range Diagram", pal);
    scene.suppress_derived_title();
    let axes = Axes2D::new(
        (85.0, 45.0, 530.0, 350.0),
        (-0.03 * max_range, max_range * 1.15),
        (0.0, max_payload * 1.25),
    );
    axes.draw_frame(&mut scene, pal);

    let curve: Vec<(f64, f64)> = ranges
        .iter()
        .copied()
        .zip(payloads_t.iter().copied())
        .collect();
    let mut fill_points: Vec<[f64; 2]> = curve
        .iter()
        .map(|&(range, payload)| axes.map_point(range, payload))
        .collect();
    fill_points.extend(
        curve
            .iter()
            .rev()
            .map(|&(range, _)| axes.map_point(range, 0.0)),
    );
    scene.add(SceneElement::Polygon {
        points: fill_points,
        fill: Some(Fill::new(Color::rgba(31, 119, 180, 31))),
        stroke: None,
    });
    axes.add_line_series(&mut scene, &curve, Stroke::new(Color::from_hex(BLUE), 2.5));

    let mut i = 0;
    while i < data.points.len() {
        let point = data.points[i];
        let mut labels = point.label.to_owned();
        let mut j = i;
        while j + 1 < data.points.len()
            && (data.points[j + 1].range_nm - point.range_nm).abs() < 1e-6
            && (data.points[j + 1].payload_kg - point.payload_kg).abs() < 1e-6
        {
            j += 1;
            labels.push('/');
            labels.push_str(data.points[j].label);
        }
        let mapped = axes.map_point(point.range_nm, point.payload_kg / 1000.0);
        scene.add(SceneElement::Circle {
            center: mapped,
            radius: 4.0,
            fill: Some(Fill::new(Color::from_hex(BLUE))),
            stroke: Some(Stroke::new(Color::from_hex(pal.bg), 1.0)),
        });
        let near_right_edge = point.range_nm > 0.85 * max_range;
        scene.add(SceneElement::Text {
            text: format!(
                "{}\n{} nm | {:.1} t",
                labels,
                format_thousands(point.range_nm),
                point.payload_kg / 1000.0
            ),
            pos: [
                mapped[0] + if near_right_edge { -8.0 } else { 8.0 },
                mapped[1] - 10.0,
            ],
            font_size: 8.5,
            color: Color::from_hex(pal.title),
            align: if near_right_edge {
                TextAlign::Right
            } else {
                TextAlign::Left
            },
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
        i = j + 1;
    }

    scene.add(SceneElement::Text {
        text: "Range (nm)".to_owned(),
        pos: [axes.left + axes.width * 0.5, axes.top + axes.height + 30.0],
        font_size: 9.5,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: "Payload (t)".to_owned(),
        pos: [axes.left - 42.0, axes.top + axes.height * 0.5],
        font_size: 9.5,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: format!(
            "Fuel capacity: {} kg (limited by {})   |   OEW: {} kg   |   MTOW: {} kg",
            format_thousands(data.fuel_capacity_kg),
            data.fuel_capacity_limit,
            format_thousands(data.oew_kg),
            format_thousands(data.mtow_kg)
        ),
        pos: [axes.left + axes.width * 0.5, axes.top + axes.height + 56.0],
        font_size: 7.5,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}

fn payload_range_data(report: &AnalysisReport, config: &AlasConfig) -> Option<PayloadRangeData> {
    let wing = report.airplane.wings.first()?;
    let masses = &report.component_masses;
    let oew_kg: f64 = OEW_KEYS
        .iter()
        .map(|key| masses.get(*key).copied().unwrap_or(0.0))
        .sum();
    let max_payload_kg = masses.get("Payload").copied().unwrap_or(0.0);
    let mtow_kg = config.requirements.mtow_kg;

    let tank_capacity_kg = wing_fuel_volume_m3(wing, config.mass_model.fuel_tank_usable_fraction)
        * config.mass_model.fuel_density_kg_m3;
    let structural_capacity_kg = (mtow_kg - oew_kg).max(0.0);
    let (fuel_capacity_kg, fuel_capacity_limit) = if tank_capacity_kg <= structural_capacity_kg {
        (tank_capacity_kg, "wing tank volume")
    } else {
        (structural_capacity_kg, "MTOW budget")
    };

    let l_over_d = report
        .trimmed_design_point
        .as_ref()
        .map(|point| point.l_over_d)
        .unwrap_or(report.design_point.l_over_d);
    let atmo = Atmosphere::new(config.requirements.cruise_altitude_m);
    let tas_m_s = config.requirements.cruise_mach * atmo.speed_of_sound();
    let tsfc_si = config.geometry.engine.cruise_tsfc_kg_kgf_hr / (G * 3600.0);
    let range_nm = |start_kg: f64, end_kg: f64| {
        breguet_range_m(tas_m_s, l_over_d, tsfc_si, start_kg, end_kg) / M_TO_NM
    };

    let fuel_b = fuel_capacity_kg
        .min(mtow_kg - oew_kg - max_payload_kg)
        .max(0.0);
    let tow_b = oew_kg + max_payload_kg + fuel_b;
    let payload_c = (mtow_kg - oew_kg - fuel_capacity_kg)
        .min(max_payload_kg)
        .max(0.0);
    let tow_c = oew_kg + payload_c + fuel_capacity_kg;
    let tow_d = oew_kg + fuel_capacity_kg;

    Some(PayloadRangeData {
        points: [
            PayloadRangePoint {
                label: "A",
                range_nm: 0.0,
                payload_kg: max_payload_kg,
            },
            PayloadRangePoint {
                label: "B",
                range_nm: range_nm(tow_b, tow_b - fuel_b),
                payload_kg: max_payload_kg,
            },
            PayloadRangePoint {
                label: "C",
                range_nm: range_nm(tow_c, tow_c - fuel_capacity_kg),
                payload_kg: payload_c,
            },
            PayloadRangePoint {
                label: "D",
                range_nm: range_nm(tow_d, oew_kg),
                payload_kg: 0.0,
            },
        ],
        fuel_capacity_kg,
        fuel_capacity_limit,
        oew_kg,
        mtow_kg,
    })
}

fn wing_fuel_volume_m3(wing: &Wing, usable_fraction: f64) -> f64 {
    if wing.xsecs.len() < 2 {
        return 0.0;
    }
    let x: Vec<f64> = (0..=100).map(|i| i as f64 / 100.0).collect();
    let t_over_c_root = wing.xsecs[0].airfoil.max_thickness(&x);
    let area = wing.area();
    let span = wing.span().max(1e-6);
    let taper = wing.taper_ratio();
    let taper_term = (1.0 + taper + taper * taper) / (1.0 + taper).powi(2);
    let geometric_volume = 0.54 * (area * area / span) * t_over_c_root * taper_term;
    geometric_volume * usable_fraction.clamp(0.0, 1.0)
}

fn status_scene(title: &str, message: &str, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(600.0, 300.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some(title.to_owned());
    scene.add(SceneElement::Text {
        text: message.to_owned(),
        pos: [300.0, 150.0],
        font_size: 12.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    #[test]
    fn the_wing_fuel_volume_is_zero_when_the_usable_fraction_is_zero() {
        let airfoil = Airfoil::from_name("naca0012").unwrap();
        let wing = Wing::new(
            "Probe",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 3.0, 0.0, airfoil.clone()),
                WingXSec::new([0.0, 8.0, 0.0], 3.0, 0.0, airfoil),
            ],
            true,
        );
        assert_eq!(wing_fuel_volume_m3(&wing, 0.0), 0.0);
    }
}
