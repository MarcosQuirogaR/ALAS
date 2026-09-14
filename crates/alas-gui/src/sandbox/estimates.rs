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
    QuickFeasibility, QuickMetric, QuickOutcome, QuickPayloadRange, QuickValue,
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
            "OEW {oew} kg, MTOW {mtow} kg, fuel capacity {fuel} kg ({basis})",
            &[
                ("oew", format!("{:.0}", corners.oew_kg)),
                ("mtow", format!("{:.0}", corners.mtow_kg)),
                ("fuel", format!("{:.0}", corners.fuel_capacity_kg)),
                ("basis", corners.fuel_capacity_basis.clone()),
            ],
        ))
        .weak()
        .small(),
    )
    .on_hover_text(&corners.note);
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
            "Reduced in-process model at fixed geometry: mission-sized mass closure, in-loop vortex lattice with Raymer/Korn drag, catalogue propulsion deck, Breguet range. Not a validated performance figure.",
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
                    ui.label(RichText::new(tr(metric.label())).small().weak());
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
