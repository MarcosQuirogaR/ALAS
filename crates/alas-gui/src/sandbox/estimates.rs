// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The right-side estimates strip: Quick Analysis results labelled as
//! initial estimates, with requested-versus-achievable values, units,
//! validity notes, the payload-range corners and the feasibility flags.
//!
//! Display conversions are presentation only: altitudes stay in metres
//! with the flight level (hundreds of feet) beside them, ranges show
//! kilometres and nautical miles, speeds show metres per second and
//! kilometres per hour.

use alas_pipeline::quick_analysis::{
    QuickBasis, QuickFeasibility, QuickMetric, QuickOutcome, QuickPayloadRange, QuickRouteFuel,
    QuickValue,
};
use egui::{pos2, vec2, RichText, ScrollArea, Stroke, Ui};

use crate::state::AppState;
use crate::views::{tr, tr_fields};

use super::quick::MetricState;

const FOOT_M: f64 = 0.3048;
const NAUTICAL_MILE_M: f64 = 1852.0;

fn format_quantity(metric: QuickMetric, value: f64) -> String {
    match metric {
        QuickMetric::CruiseAltitude | QuickMetric::ServiceCeiling => {
            format!("{value:.0} m (FL{:.0})", value / FOOT_M / 100.0)
        }
        QuickMetric::Range => format!(
            "{:.0} km ({:.0} NM)",
            value / 1000.0,
            value / NAUTICAL_MILE_M
        ),
        QuickMetric::CruiseSpeed => format!("{value:.1} m/s ({:.0} km/h)", value * 3.6),
        QuickMetric::CruiseLiftToDrag => format!("{value:.2}"),
        QuickMetric::StaticMargin => format!("{value:.1} % MAC"),
        _ => format!("{value:.0} {}", metric.unit()),
    }
}

fn show_value(ui: &mut Ui, metric: QuickMetric, value: &QuickValue) {
    let achieved = RichText::new(format_quantity(metric, value.achieved)).strong();
    ui.label(achieved).on_hover_text(&value.note);
    if let Some(requested) = value.requested {
        let label = match metric {
            QuickMetric::TakeoffMass => "declared MTOW",
            QuickMetric::CarriedPayload => "structural cap",
            QuickMetric::CarriedFuel => "capacity",
            QuickMetric::StaticMargin => "minimum",
            QuickMetric::Range => "route",
            _ => "requested",
        };
        ui.label(
            RichText::new(format!(
                "{}: {}",
                tr(label),
                format_quantity(metric, requested)
            ))
            .weak()
            .small(),
        );
    }
}

/// The card title: the metric label, with the route's planner tier and its
/// excess over the great circle once the route fuel is known.
fn metric_title(metric: QuickMetric, state: &MetricState) -> String {
    let MetricState::Done(QuickOutcome::Route(route)) = state else {
        return tr(metric.label());
    };
    let source = match route.source.as_str() {
        "navdata_graph" => tr("airway"),
        "simbrief_kml" => tr("KML dispatch plan"),
        "simbrief_api" => tr("SimBrief dispatch plan"),
        _ => tr("great circle"),
    };
    tr_fields(
        "Block fuel: Route ({source}, {excess} %)",
        &[
            ("source", source),
            (
                "excess",
                format!("{:+.1}", 100.0 * route.excess_over_great_circle()),
            ),
        ],
    )
}

fn show_route(ui: &mut Ui, route: &QuickRouteFuel) {
    ui.label(RichText::new(format!("{:.0} kg", route.block_fuel_kg)).strong())
        .on_hover_text(&route.note);
    ui.label(
        RichText::new(tr_fields(
            "{route} km flown against the {great_circle} km great circle; takeoff fuel {fuel} kg at {mass} kg",
            &[
                ("route", format!("{:.0}", route.route_distance_m / 1000.0)),
                ("great_circle", format!("{:.0}", route.great_circle_m / 1000.0)),
                ("fuel", format!("{:.0}", route.takeoff_fuel_kg)),
                ("mass", format!("{:.0}", route.takeoff_mass_kg)),
            ],
        ))
        .weak()
        .small(),
    );
    if route.shortfall_kg > 0.0 {
        ui.label(
            RichText::new(tr_fields(
                "{shortfall} kg short of the route's policy fuel",
                &[("shortfall", format!("{:.0}", route.shortfall_kg))],
            ))
            .color(ui.visuals().warn_fg_color)
            .small(),
        );
    }
}

fn show_payload_range(ui: &mut Ui, corners: &QuickPayloadRange) {
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width().max(160.0), 120.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let inner = rect.shrink2(vec2(34.0, 16.0));
    let max_range = corners
        .points
        .iter()
        .map(|(r, _)| *r)
        .fold(0.0f64, f64::max)
        .max(1.0);
    let max_payload = corners
        .points
        .iter()
        .map(|(_, p)| *p)
        .fold(0.0f64, f64::max)
        .max(1.0);
    let map = |(r, p): (f64, f64)| {
        pos2(
            inner.left() + (r / max_range) as f32 * inner.width(),
            inner.bottom() - (p / max_payload) as f32 * inner.height(),
        )
    };
    let spine = Stroke::new(1.0_f32, ui.visuals().weak_text_color());
    painter.line_segment([inner.left_bottom(), inner.right_bottom()], spine);
    painter.line_segment([inner.left_bottom(), inner.left_top()], spine);
    let points: Vec<egui::Pos2> = corners.points.iter().map(|&p| map(p)).collect();
    painter.add(egui::Shape::line(
        points.clone(),
        Stroke::new(2.0_f32, ui.visuals().hyperlink_color),
    ));
    for p in &points {
        painter.circle_filled(*p, 3.0, ui.visuals().hyperlink_color);
    }
    let font = egui::FontId::proportional(10.0);
    painter.text(
        inner.right_bottom() + vec2(0.0, 3.0),
        egui::Align2::RIGHT_TOP,
        format!("{:.0} km", max_range / 1000.0),
        font.clone(),
        ui.visuals().weak_text_color(),
    );
    painter.text(
        inner.left_top() + vec2(-3.0, 0.0),
        egui::Align2::RIGHT_TOP,
        format!("{:.0} t", max_payload / 1000.0),
        font,
        ui.visuals().weak_text_color(),
    );
    ui.label(
        RichText::new(tr_fields(
            "OEW {oew} kg, MTOW {mtow} kg, max payload {payload} kg ({payload_basis}), fuel capacity {fuel} kg ({basis})",
            &[
                ("oew", format!("{:.0}", corners.oew_kg)),
                ("mtow", format!("{:.0}", corners.mtow_kg)),
                ("payload", format!("{:.0}", corners.max_payload_kg)),
                ("payload_basis", tr(&corners.payload_basis)),
                ("fuel", format!("{:.0}", corners.fuel_capacity_kg)),
                ("basis", corners.fuel_capacity_basis.clone()),
            ],
        ))
        .weak()
        .small(),
    )
    .on_hover_text(&corners.note);
}

/// The tag that says whether a card shows the Full Analysis' own value or
/// an estimate, with the measured bound in its hover text.
fn show_basis(ui: &mut Ui, basis: QuickBasis) {
    let (tag, explanation) = match basis {
        QuickBasis::FullAnalysis => (
            tr("Full Analysis"),
            tr("The value the Full Analysis computes for this aircraft, by the same model."),
        ),
        QuickBasis::ClosureEstimate => (
            tr("Estimate"),
            tr("Mission-sized closure estimate. The Full Analysis prices the route on this closure's drag table and plan, so over the same still-air distance its dispatch agrees within its 1 kg settling tolerance. A route planned along airways is longer and needs more fuel."),
        ),
        QuickBasis::EnvelopeEstimate => (
            tr("Estimate"),
            tr("Thrust-limited envelope estimate the Full Analysis does not report: maximum-climb thrust against its trimmed drag table at the closure takeoff mass, one mass for the whole cruise."),
        ),
    };
    let color = match basis {
        QuickBasis::FullAnalysis => ui.visuals().weak_text_color(),
        QuickBasis::ClosureEstimate | QuickBasis::EnvelopeEstimate => ui.visuals().warn_fg_color,
    };
    ui.label(RichText::new(tag).small().color(color))
        .on_hover_text(explanation);
}

fn show_feasibility(ui: &mut Ui, flags: &QuickFeasibility) {
    let (text, color) = if flags.feasible {
        (
            tr("No blocking flag"),
            crate::theme::success_color(ui.visuals()),
        )
    } else {
        (tr("Blocking flags raised"), ui.visuals().error_fg_color)
    };
    ui.label(RichText::new(text).color(color).strong());
    for flag in &flags.flags {
        let color = if flag.blocking {
            ui.visuals().error_fg_color
        } else {
            ui.visuals().warn_fg_color
        };
        ui.label(
            RichText::new(format!("{}: {}", flag.code, flag.message))
                .color(color)
                .small(),
        );
    }
}

/// Render the estimates strip.
pub fn show_estimates_strip(state: &mut AppState, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr("Initial estimates"))
                .strong()
                .color(ui.visuals().hyperlink_color),
        )
        .on_hover_text(tr(
            "In-process analysis of the drawn aircraft at fixed geometry: a mission-sized mass closure flown on the segment mission model with reserve-inclusive fuel plans, then the full baseline analysis the Full Analysis runs. Each card states whether its value is the Full Analysis' own or an estimate. Not a validated performance figure.",
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::theme::close_icon_button(ui, tr("Collapse")).clicked() {
                state.sandbox.layout.estimates_open = false;
            }
        });
    });
    let estimates = &state.sandbox.estimates;
    if estimates.running() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new(tr("Running")).weak().small());
        });
    }
    if estimates.stale {
        ui.label(
            RichText::new(tr(
                "The model changed; these estimates are stale. Run Quick Analysis again.",
            ))
            .color(ui.visuals().warn_fg_color)
            .small(),
        );
    }
    if let (Some(first), Some(summary)) = (estimates.first_result_ms, &estimates.summary) {
        ui.label(
            RichText::new(tr_fields(
                "First result {first} ms, complete {final} ms",
                &[
                    ("first", first.to_string()),
                    ("final", summary.final_ms.to_string()),
                ],
            ))
            .weak()
            .small(),
        );
    }
    if !estimates.has_results() && !estimates.running() {
        ui.label(RichText::new(tr("Run Quick Analysis to publish initial estimates.")).weak());
        return;
    }
    ui.add_space(4.0);
    let rows: Vec<(QuickMetric, MetricState)> = estimates.states().to_vec();
    ScrollArea::vertical()
        .id_salt("sandbox_estimates")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (metric, state) in &rows {
                crate::theme::card_frame(ui).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(metric_title(*metric, state)).small().weak());
                        show_basis(ui, metric.basis());
                    });
                    match state {
                        MetricState::Idle => {}
                        MetricState::Running => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(tr("Running"));
                            });
                        }
                        MetricState::Done(QuickOutcome::Value(value)) => {
                            show_value(ui, *metric, value)
                        }
                        MetricState::Done(QuickOutcome::PayloadRange(corners)) => {
                            show_payload_range(ui, corners)
                        }
                        MetricState::Done(QuickOutcome::Route(route)) => show_route(ui, route),
                        MetricState::Done(QuickOutcome::Feasibility(flags)) => {
                            show_feasibility(ui, flags)
                        }
                        MetricState::Done(QuickOutcome::Failed(message)) => {
                            ui.label(
                                RichText::new(tr("Failed")).color(ui.visuals().error_fg_color),
                            )
                            .on_hover_text(message);
                        }
                        MetricState::Done(QuickOutcome::Unsupported(message)) => {
                            ui.label(RichText::new(tr("Unsupported")).weak())
                                .on_hover_text(message);
                        }
                    }
                });
                ui.add_space(3.0);
            }
        });
}
