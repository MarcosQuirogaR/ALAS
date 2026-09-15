// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_payload_range`) and
// alas/physics/performance.py (`payload_range_diagram`, `wing_fuel_volume_m3`).
// Reference: alas @ rust-port-baseline.

//! The conceptual A-B-C-D payload-range curve from the analyzed aircraft.
//!
//! The report crate owns this orchestration because the numerical payload-range
//! calculation consumes both a full-analysis report and configuration. Keeping
//! the inputs here prevents a renderer from quietly reverting to a plausible
//! transport-sized set of points when the aircraft or its fuel model changes.

use alas_atmo::Atmosphere;
use alas_config::{presets, ActiveEngineModel, AlasConfig};
use alas_geom::aircraft::wing::Wing;
use alas_perf::performance::breguet_range_m;
use alas_pipeline::feasibility::{assess_fuel_capacity, FuelCapacityEvidence};
use alas_pipeline::full_analysis::AnalysisReport;
use alas_pipeline::quick_analysis::payload_capacity_estimate;

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

/// One labelled corner of the conceptual payload-range curve.
#[derive(Debug, Clone, Copy)]
pub struct PayloadRangePoint {
    /// Which corner of the A-B-C-D curve this is.
    pub label: &'static str,
    /// Still-air Breguet range at this corner, nautical miles.
    pub range_nm: f64,
    /// Payload carried at this corner, kilograms.
    pub payload_kg: f64,
}

/// The four corners and the mass/fuel basis they were computed on.
///
/// Public so the same numbers the figure draws can be read back and
/// correlated against a published payload-range chart. A figure nobody can
/// query is a figure nobody can check.
#[derive(Debug, Clone)]
pub struct PayloadRangeData {
    /// The A-B-C-D corners, in order.
    pub points: [PayloadRangePoint; 4],
    /// Usable fuel the curve was built on, kilograms.
    pub fuel_capacity_kg: f64,
    /// What limited that fuel figure.
    pub fuel_capacity_limit: &'static str,
    /// What limited the maximum payload.
    pub payload_basis: &'static str,
    /// Modelled operating empty weight, kilograms.
    pub oew_kg: f64,
    /// Maximum takeoff weight the curve was built on, kilograms.
    pub mtow_kg: f64,
    /// Range method and its provenance.
    pub method_note: String,
}

/// Generate an idealized Breguet payload-range curve from the report's masses,
/// aerodynamic point, and typed fuel-capacity evidence.
///
/// This is a conceptual model check, not an AFM/WBM operational capability
/// envelope or a mission-certified range result.
pub fn figure_payload_range(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let Some(data) = payload_range_data(report, config) else {
        let message = if report.airplane.wings.is_empty() {
            "The analyzed report has no main wing; conceptual payload range cannot be computed."
        } else {
            "Typed usable-fuel capacity evidence is unavailable; conceptual payload range cannot be computed."
        };
        return status_scene("Conceptual Payload-Range Diagram", message, theme);
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
    scene.title = Some("Conceptual Payload-Range Diagram".to_owned());
    draw_title(&mut scene, "Conceptual Payload-Range Diagram", pal);
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
            "OEW: {} kg   |   MTOW: {} kg",
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

/// Compute the conceptual payload-range corners for `report` under `config`.
///
/// Returns `None` when the report has no main wing or no typed usable-fuel
/// capacity evidence, which are the two cases the figure renders as a status
/// panel rather than a curve.
pub fn payload_range_data(
    report: &AnalysisReport,
    config: &AlasConfig,
) -> Option<PayloadRangeData> {
    report.airplane.wings.first()?;
    let masses = &report.component_masses;
    let oew_kg: f64 = OEW_KEYS
        .iter()
        .map(|key| masses.get(*key).copied().unwrap_or(0.0))
        .sum();
    let analyzed_payload_kg = masses.get("Payload").copied().unwrap_or(0.0);
    let mtow_kg = config.requirements.mtow_kg;

    // A payload-range chart is an aircraft-capability curve, not a second
    // drawing of the currently selected cabin load. The old implementation
    // used the latter as point A, so a 130-seat A220 or 525-seat A380 could
    // never show the structural payload the preset was meant to represent.
    // Bound the configured/effective payload by MZFW - the *modeled* OEW;
    // this keeps the chart on the same mass convention as the feasibility
    // check and prevents an MZFW violation from being hidden in a figure.
    let configured_payload_limit = config.requirements.max_structural_payload_kg;
    let effective_payload_limit = report
        .geometry_summary
        .get("effective_structural_payload_limit_kg")
        .copied();
    let published_mzfw_payload_limit = presets::get(&config.preset)
        .ok()
        .and_then(|preset| preset.reference.mzfw_kg)
        .map(|mzfw_kg| mzfw_kg - oew_kg);
    // Achievable payload: the declared cap, the effective or MZFW-derived limit
    // and the MTOW - OEW budget, resolved by the same estimator the sandbox
    // Quick Analysis uses so both surfaces publish one capacity basis.
    let mzfw_limit_kg = [effective_payload_limit, published_mzfw_payload_limit]
        .into_iter()
        .flatten()
        .filter(|value| value.is_finite() && *value > 0.0)
        .reduce(f64::min);
    let capacity = payload_capacity_estimate(
        configured_payload_limit,
        mzfw_limit_kg,
        analyzed_payload_kg,
        mtow_kg,
        oew_kg,
    );
    let (max_payload_kg, payload_basis) = (capacity.capacity_kg, capacity.basis);

    let fuel_capacity = assess_fuel_capacity(config, &report.design, report);
    let fuel_capacity_kg = fuel_capacity
        .capacity_kg
        .filter(|value| value.is_finite())?;
    let fuel_capacity_limit = match fuel_capacity.evidence {
        FuelCapacityEvidence::PublishedPreset => "published usable capacity",
        FuelCapacityEvidence::GeometryEstimate => "geometry-estimated capacity",
        FuelCapacityEvidence::Unavailable => "unavailable",
    };
    let structural_capacity_kg = (mtow_kg - oew_kg).max(0.0);
    let (fuel_capacity_kg, fuel_capacity_limit) = if fuel_capacity_kg <= structural_capacity_kg {
        (fuel_capacity_kg, fuel_capacity_limit)
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
    let active_model = config.geometry.engine.active_model().ok()?;
    let (range_nm, method_note): (Box<dyn Fn(f64, f64) -> f64>, String) = match active_model {
        ActiveEngineModel::Turbofan(spec) => {
            let tsfc_si = spec.cruise_tsfc_kg_kgf_hr / (G * 3600.0);
            (
                Box::new(move |start_kg, end_kg| {
                    breguet_range_m(tas_m_s, l_over_d, tsfc_si, start_kg, end_kg) / M_TO_NM
                }),
                // Keep the stable figure label generic; the engine family is
                // already explicit in the typed calculation branch and the
                // footer's limitation warning.
                "CONCEPTUAL BREGUET RANGE ONLY".to_owned(),
            )
        }
        ActiveEngineModel::Turboprop(spec) => {
            // The catalogue anchor is a two-engine figure; scale it with the
            // installed engine count so an edited installation changes range.
            let installed = config.geometry.engine.spanwise_positions_m.len();
            let fuel_flow_kg_h = spec.installed_cruise_fuel_flow_kg_h(installed)?;
            if !fuel_flow_kg_h.is_finite() || fuel_flow_kg_h <= 0.0 {
                return None;
            }
            (Box::new(move |start_kg, end_kg| tas_m_s * 3600.0 * (start_kg - end_kg).max(0.0) / fuel_flow_kg_h / M_TO_NM), format!("CONCEPTUAL TURBOPROP CONSTANT-FLOW RANGE AT MAX-CRUISE ANCHOR ({fuel_flow_kg_h:.0} kg/h TOTAL); NO OFF-DESIGN DECK"))
        }
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
        payload_basis,
        oew_kg,
        mtow_kg,
        method_note,
    })
}

// Retained as an explicit compatibility/reference correlation for standalone
// comparison tests; product capacity comes from typed feasibility evidence.
#[allow(dead_code)]
fn wing_fuel_volume_m3(wing: &Wing, usable_fraction: f64) -> f64 {
    if wing.xsecs.len() < 2 {
        return 0.0;
    }
    let x: Vec<f64> = (0..=100).map(|i| i as f64 / 100.0).collect();
    let t_over_c_root = wing.xsecs[0].airfoil.max_thickness(&x);
    let area = wing.unfolded_area();
    let span = wing.unfolded_span().max(1e-6);
    let taper = wing.taper_ratio();
    let taper_term = (1.0 + taper + taper * taper) / (1.0 + taper).powi(2);
    let geometric_volume = 0.54 * (area * area / span) * t_over_c_root * taper_term;
    geometric_volume * usable_fraction.clamp(0.0, 1.0)
}

fn status_scene(title: &str, message: &str, theme: Option<&str>) -> Scene {
    crate::status_figure::figure_status_message(title, message, false, theme)
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
