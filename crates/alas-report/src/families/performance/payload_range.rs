// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_payload_range`) and
// alas/physics/performance.py (`payload_range_diagram`).

//! The conceptual A-B-C-D payload-range curve from the analyzed aircraft,
//! flown between B and C as well as at the corners
//! ([`alas_pipeline::quick_analysis::corners::mtow_line`]): the takeoff-mass
//! line is curved, and its chord reads long.
//!
//! The report crate owns this orchestration because the numerical payload-range
//! calculation consumes both a full-analysis report and configuration. Keeping
//! the inputs here prevents a renderer from quietly reverting to a plausible
//! transport-sized set of points when the aircraft or its fuel model changes.

use alas_config::AlasConfig;
use alas_pipeline::feasibility::FuelCapacityEvidence;
use alas_pipeline::full_analysis::AnalysisReport;
use alas_pipeline::quick_analysis::corners::mtow_line::{mtow_line_points, MtowLinePoint};
use alas_pipeline::quick_analysis::corners::{
    chart_corner_masses, corner_ranges, CornerMasses, RangeBasis,
};

use super::support::format_thousands;
use crate::chart_kit::draw_title;
use crate::plot_data::{CustomSeries, SeriesKind};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

const M_TO_NM: f64 = 1852.0;
const BLUE: &str = "tab:blue";
/// Name of the exported series of points flown on the takeoff-mass line, B to C.
const MTOW_LINE_SERIES: &str = "flown takeoff-mass line B-C";

/// Offset and alignment of a corner's two-line label: the first of four anchors
/// (above-right, above-left, below-right, below-left) whose text box crosses the
/// curve least and stays inside the axes. A corner on a descending leg, as C and D
/// are on a long-range type, therefore flips to the side the curve is not on.
fn label_anchor(
    curve_px: &[[f64; 2]],
    at: [f64; 2],
    text: &str,
    axes: (f64, f64, f64, f64),
    placed: &mut Vec<(f64, f64, f64, f64)>,
) -> (f64, f64, TextAlign, TextBaseline) {
    let chars = text.lines().map(|l| l.chars().count()).max().unwrap_or(1) as f64;
    let (w, h) = (chars * 5.4, 2.0 * 14.0);
    let anchors = [
        (8.0, -10.0, TextAlign::Left, TextBaseline::Bottom),
        (-8.0, -10.0, TextAlign::Right, TextBaseline::Bottom),
        (8.0, 10.0, TextAlign::Left, TextBaseline::Top),
        (-8.0, 10.0, TextAlign::Right, TextBaseline::Top),
    ];
    let mut best = (usize::MAX, anchors[0]);
    for anchor in anchors {
        let (dx, dy, align, baseline) = anchor;
        let x0 = if align == TextAlign::Left {
            at[0] + dx
        } else {
            at[0] + dx - w
        };
        let y0 = if baseline == TextBaseline::Bottom {
            at[1] + dy - h
        } else {
            at[1] + dy
        };
        let (x1, y1) = (x0 + w, y0 + h);
        let outside =
            x0 < axes.0 || x1 > axes.0 + axes.2 || y0 < axes.1 || y1 > axes.1 + axes.3 - 2.0;
        let mut hits = if outside { 1_000 } else { 0 };
        for &(px0, py0, px1, py1) in placed.iter() {
            if x0 < px1 && x1 > px0 && y0 < py1 && y1 > py0 {
                hits += 500;
            }
        }
        for pair in curve_px.windows(2) {
            let length =
                ((pair[1][0] - pair[0][0]).powi(2) + (pair[1][1] - pair[0][1]).powi(2)).sqrt();
            let steps = (length / 3.0).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let t = k as f64 / steps as f64;
                let px = pair[0][0] + t * (pair[1][0] - pair[0][0]);
                let py = pair[0][1] + t * (pair[1][1] - pair[0][1]);
                // The marker itself sits at `at`; ignore the curve right at the label's own corner.
                let own = (px - at[0]).abs() < 6.0 && (py - at[1]).abs() < 6.0;
                if !own && px >= x0 - 3.0 && px <= x1 + 3.0 && py >= y0 - 3.0 && py <= y1 + 3.0 {
                    hits += 1;
                }
            }
        }
        if hits < best.0 {
            best = (hits, anchor);
        }
    }
    let (dx, _, align, baseline) = best.1;
    let bx0 = if align == TextAlign::Left {
        at[0] + dx
    } else {
        at[0] + dx - w
    };
    let by0 = if baseline == TextBaseline::Bottom {
        at[1] + best.1 .1 - h
    } else {
        at[1] + best.1 .1
    };
    placed.push((bx0, by0, bx0 + w, by0 + h));
    best.1
}

/// One labelled corner of the conceptual payload-range curve.
#[derive(Debug, Clone, Copy)]
pub struct PayloadRangePoint {
    /// Which corner of the A-B-C-D curve this is.
    pub label: &'static str,
    /// Still-air range at this corner, nautical miles (see
    /// [`PayloadRangeData::range_basis`]).
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
    /// Takeoff mass the curve was built on, kilograms: the mission-sized mass
    /// when `mass_is_sized`, else the declared MTOW.
    pub mtow_kg: f64,
    /// True when `mtow_kg` is the pipeline's sized takeoff mass rather than
    /// the declared MTOW.
    pub mass_is_sized: bool,
    /// Reserve fuel the plan holds back at each corner (A to D), kilograms;
    /// zero at A.
    pub reserve_fuel_kg: [f64; 4],
    /// What the ranges include: the reserve-inclusive fuel plan.
    pub range_basis: RangeBasis,
    /// Range method and its provenance, including the fuel scheme.
    pub method_note: String,
    /// Points flown on the takeoff-mass line strictly between B and C, in
    /// falling payload, priced as the corners are; empty when B and C
    /// coincide.
    pub mtow_line: Vec<MtowLinePoint>,
}

impl PayloadRangeData {
    /// The flown curve as (range nm, payload kg): A, B, the takeoff-mass line
    /// points, C and D.
    pub fn curve(&self) -> Vec<(f64, f64)> {
        let corner = |index: usize| (self.points[index].range_nm, self.points[index].payload_kg);
        let mut curve = vec![corner(0), corner(1)];
        curve.extend(
            self.mtow_line
                .iter()
                .map(|point| (point.range_m / M_TO_NM, point.payload_kg)),
        );
        curve.extend([corner(2), corner(3)]);
        curve
    }
}

/// Generate the payload-range curve from the report's masses, fuel model
/// and typed fuel-capacity evidence, every corner priced by the report's
/// segment mission model with reserves (`alas_pipeline::quick_analysis::corners`).
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
            "Typed usable-fuel capacity evidence or the fuel model is unavailable; conceptual payload range cannot be computed."
        };
        return status_scene("Conceptual Payload-Range Diagram", message, theme);
    };

    let flown = data.curve();
    let ranges: Vec<f64> = flown.iter().map(|point| point.0).collect();
    let payloads_t: Vec<f64> = flown.iter().map(|point| point.1 / 1000.0).collect();
    let max_range = ranges.iter().copied().fold(0.0, f64::max).max(1.0);
    let max_payload = payloads_t.iter().copied().fold(0.0, f64::max).max(1.0);

    let mut scene = Scene::new(700.0, 500.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Conceptual Payload-Range Diagram".to_owned());
    draw_title(&mut scene, "Conceptual Payload-Range Diagram", pal);
    scene.suppress_derived_title();
    let axes = Axes2D::new(
        (85.0, 45.0, 530.0, 350.0),
        (-0.03 * max_range, max_range * 1.35),
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
    let curve_px: Vec<[f64; 2]> = curve.iter().map(|&(r, p)| axes.map_point(r, p)).collect();

    let mut placed: Vec<(f64, f64, f64, f64)> = Vec::new();
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
        let text = format!(
            "{}\n{} nm | {:.1} t",
            labels,
            format_thousands(point.range_nm),
            point.payload_kg / 1000.0
        );
        let (dx, dy, align, baseline) = label_anchor(
            &curve_px,
            mapped,
            &text,
            (axes.left, axes.top, axes.width, axes.height),
            &mut placed,
        );
        scene.add(SceneElement::Text {
            text,
            pos: [mapped[0] + dx, mapped[1] + dy],
            font_size: 8.5,
            color: Color::from_hex(pal.title),
            align,
            baseline,
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
            "OEW: {} kg   |   {}: {} kg",
            format_thousands(data.oew_kg),
            if data.mass_is_sized {
                "Sized TOW"
            } else {
                "MTOW"
            },
            format_thousands(data.mtow_kg)
        ),
        pos: [axes.left + axes.width * 0.5, axes.top + axes.height + 66.0],
        font_size: 7.5,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: format!("Range basis: {}", data.range_basis.label()),
        pos: [axes.left + axes.width * 0.5, axes.top + axes.height + 82.0],
        font_size: 7.5,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
    record_mtow_line(&mut scene, &data);
    scene
}

/// Record the flown takeoff-mass line, B to C, as its own exported series:
/// a payload between B and C is read between flown points, not on the chord.
fn record_mtow_line(scene: &mut Scene, data: &PayloadRangeData) {
    let (b, c) = (data.points[1], data.points[2]);
    let mut rows = vec![(b.range_nm, b.payload_kg, "B".to_owned())];
    rows.extend(data.mtow_line.iter().map(|point| {
        let kind = if point.stated { "stated ZFW" } else { "ZFW" };
        (
            point.range_m / M_TO_NM,
            point.payload_kg,
            format!("{kind} {:.0} kg", point.zero_fuel_mass_kg),
        )
    }));
    rows.push((c.range_nm, c.payload_kg, "C".to_owned()));
    scene.data_trace.note_custom(CustomSeries {
        key: "mtow_line",
        x_label: "Range (nm)",
        y_label: "Payload (t)",
        name: MTOW_LINE_SERIES,
        kind: SeriesKind::Points,
        x: rows.iter().map(|row| row.0).collect(),
        y: rows.iter().map(|row| row.1 / 1000.0).collect(),
        labels: rows.into_iter().map(|row| row.2).collect(),
    });
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
    // The chart masses come from the routine the design database's
    // `characteristics.design_point_range` uses too, so both price one basis.
    let chart = chart_corner_masses(report, config)?;
    let CornerMasses {
        mtow_kg,
        oew_kg,
        max_payload_kg: _,
        tank_capacity_kg,
    } = chart.masses;
    let payload_basis = chart.payload_basis;
    let fuel_capacity_limit = match chart.tank_evidence {
        FuelCapacityEvidence::PublishedPreset => "published usable capacity",
        FuelCapacityEvidence::GeometryEstimate => "geometry-estimated capacity",
        FuelCapacityEvidence::Unavailable => "unavailable",
    };
    let structural_capacity_kg = (mtow_kg - oew_kg).max(0.0);
    let (fuel_capacity_kg, fuel_capacity_limit) = if tank_capacity_kg <= structural_capacity_kg {
        (tank_capacity_kg, fuel_capacity_limit)
    } else {
        (structural_capacity_kg, "MTOW budget")
    };

    // The ranges come from the same routine the sandbox Quick Analysis uses,
    // so both surfaces publish one set of corners for one report.
    let ranges = corner_ranges(config, report, &chart.masses).ok()?;
    // The B-C line flown by the same rule; a point the model cannot price
    // leaves the chart unavailable, as a corner does.
    let mtow_line = mtow_line_points(config, report, &chart.masses, None, None).ok()?;
    let labels = ["A", "B", "C", "D"];
    let points = std::array::from_fn(|index| PayloadRangePoint {
        label: labels[index],
        range_nm: ranges.range_m[index] / M_TO_NM,
        payload_kg: ranges.payload_kg[index],
    });

    Some(PayloadRangeData {
        points,
        fuel_capacity_kg,
        fuel_capacity_limit,
        payload_basis,
        oew_kg,
        mass_is_sized: report.sized_takeoff_mass_kg().is_some(),
        mtow_kg,
        reserve_fuel_kg: ranges.reserve_fuel_kg,
        range_basis: ranges.basis,
        method_note: ranges.note,
        mtow_line,
    })
}

fn status_scene(title: &str, message: &str, theme: Option<&str>) -> Scene {
    crate::status_figure::figure_status_message(title, message, false, theme)
}
