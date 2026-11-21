// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The slim summary bar under the payload editor: the payload mass and a
//! live centre-of-gravity bar against the forward and aft limits.
//!
//! The marker is the zero-fuel CG (operating empty mass plus this payload;
//! the payload CG alone when no mass estimate exists) in percent MAC. The
//! limits are the aerodynamic ones the analysis uses: the aft limit is the
//! neutral point less the target static margin, the forward limit one
//! CG-envelope width ahead of it. The marker is green inside the limits,
//! amber within a tenth of the envelope width of either, red outside.

use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, Ui};

use super::glyphs::faded;
use super::side_view::SideView;
use crate::views::{tr, tr_fields, tr_label};

/// How near a limit counts as a warning, as a fraction of the envelope width.
const NEAR_FRACTION: f64 = 0.10;
/// The bar shows this fraction of the envelope width beyond each limit.
const OVERSHOOT: f64 = 0.35;
const BAR_HEIGHT: f32 = 18.0;

/// Where the CG stands against the limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Status {
    Inside,
    NearForward,
    NearAft,
    BeyondForward,
    BeyondAft,
}

impl Status {
    const fn outside(self) -> bool {
        matches!(self, Self::BeyondForward | Self::BeyondAft)
    }
}

/// Classify station `cg` against `[forward, aft]` (m).
pub(super) fn status(cg: f64, [forward, aft]: [f64; 2]) -> Status {
    let near = (aft - forward) * NEAR_FRACTION;
    if cg < forward {
        Status::BeyondForward
    } else if cg > aft {
        Status::BeyondAft
    } else if cg - forward < near {
        Status::NearForward
    } else if aft - cg < near {
        Status::NearAft
    } else {
        Status::Inside
    }
}

fn color(ui: &Ui, status: Status) -> Color32 {
    match status {
        Status::Inside => crate::theme::success_color(ui.visuals()),
        Status::NearForward | Status::NearAft => ui.visuals().warn_fg_color,
        Status::BeyondForward | Status::BeyondAft => ui.visuals().error_fg_color,
    }
}

/// The warning text of a status, empty inside the limits.
fn warning(view: &SideView, cg: f64, limits: [f64; 2], status: Status) -> Option<String> {
    let (limit, side, beyond) = match status {
        Status::Inside => return None,
        Status::BeyondForward => (limits[0], "forward", true),
        Status::BeyondAft => (limits[1], "aft", true),
        Status::NearForward => (limits[0], "forward", false),
        Status::NearAft => (limits[1], "aft", false),
    };
    let pct = (view.pct_mac(cg) - view.pct_mac(limit)).abs();
    let template = match (beyond, side) {
        (true, "forward") => "CG excursion: {pct} % MAC ahead of the forward limit",
        (true, _) => "CG excursion: {pct} % MAC behind the aft limit",
        (false, "forward") => "CG is {pct} % MAC from the forward limit",
        (false, _) => "CG is {pct} % MAC from the aft limit",
    };
    Some(tr_fields(template, &[("pct", format!("{pct:.1}"))]))
}

/// Draw the summary strip: the payload mass and the zero-fuel CG as large
/// figures, the verdict against the limits, and the limits bar. The strip's
/// edge turns amber near a limit and red past it.
pub(super) fn show(ui: &mut Ui, view: &SideView) {
    let cg = view.zero_fuel_cg.unwrap_or(view.cg_x);
    let name = if view.zero_fuel_cg.is_some() {
        tr("Zero-Fuel CG")
    } else {
        tr("Payload CG")
    };
    let state = view.cg_limits.map(|limits| status(cg, limits));
    let edge = state.map_or(ui.visuals().widgets.noninteractive.bg_stroke.color, |s| {
        if s == Status::Inside {
            ui.visuals().widgets.noninteractive.bg_stroke.color
        } else {
            color(ui, s)
        }
    });
    crate::theme::card_frame(ui)
        .stroke(Stroke::new(
            if state.is_some_and(|s| s != Status::Inside) {
                2.0_f32
            } else {
                1.0_f32
            },
            edge,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                figure(
                    ui,
                    &tr_label("Total Payload"),
                    &format!("{:.0} kg", view.payload_mass),
                );
                if let Some(oew) = view.oew_kg {
                    figure(ui, &tr_label("OEW estimate"), &format!("{oew:.0} kg"));
                }
                figure(
                    ui,
                    &name,
                    &format!(
                        "{:.1} % MAC{}{}",
                        view.pct_mac(cg),
                        if view.estimate { "*" } else { "" },
                        view.balance
                            .map(|balance| view.pct_mac(balance.target_x))
                            .or(view.target_pct)
                            .map_or_else(String::new, |pct| format!(
                                " \u{b7} {} {pct:.0} %",
                                tr_label("target")
                            )),
                    ),
                );
                match (view.cg_limits, state) {
                    (Some(limits), Some(state)) => {
                        let text = warning(view, cg, limits, state)
                            .unwrap_or_else(|| tr_label("CG Within Limits"));
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(text)
                                    .color(color(ui, state))
                                    .strong()
                                    .size(14.0),
                            )
                            .wrap(),
                        );
                    }
                    _ => {
                        ui.label(egui::RichText::new(tr_label("Limits: n/a")).weak())
                            .on_hover_text(tr_label("CG Limits Unavailable"));
                    }
                }
                if view.estimate {
                    ui.label(
                        egui::RichText::new(format!("* {}", tr_label("Estimate")))
                            .weak()
                            .small(),
                    );
                }
            });
            // The empty-payload notice has its own wrapped row: beside the figures it widened the
            // card past the page and pushed the zero-fuel CG out of sight.
            if view.custom_only && view.payload_mass <= 0.0 {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(tr_label(
                            "Custom payload is empty: place zones or point masses; sizing will not run",
                        ))
                        .color(ui.visuals().error_fg_color)
                        .strong(),
                    )
                    .wrap(),
                );
            }
            if let Some(limits) = view.cg_limits {
                bar(ui, view, cg, limits);
            }
        });
}

/// A caption over a large value.
fn figure(ui: &mut Ui, caption: &str, value: &str) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.label(egui::RichText::new(caption).weak().small());
        ui.label(egui::RichText::new(value).strong().size(20.0));
    });
}

fn bar(ui: &mut Ui, view: &SideView, cg: f64, limits: [f64; 2]) {
    let state = status(cg, limits);
    let width = ui.available_width().max(120.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, BAR_HEIGHT + 18.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let span = (limits[1] - limits[0]).max(1.0e-6);
    let (low, high) = (limits[0] - OVERSHOOT * span, limits[1] + OVERSHOOT * span);
    let x_at = |x: f64| -> f32 {
        let t = ((x.clamp(low, high) - low) / (high - low)) as f32;
        rect.left() + 6.0 + t * (rect.width() - 12.0)
    };
    let track = Rect::from_min_max(
        pos2(rect.left() + 6.0, rect.top() + 2.0),
        pos2(rect.right() - 6.0, rect.top() + 2.0 + BAR_HEIGHT),
    );
    painter.rect_filled(track, 3.0, faded(ui.visuals().weak_text_color(), 40));
    let inside = Rect::from_min_max(
        pos2(x_at(limits[0]), track.top()),
        pos2(x_at(limits[1]), track.bottom()),
    );
    painter.rect_filled(
        inside,
        3.0,
        faded(crate::theme::success_color(ui.visuals()), 60),
    );
    let weak = ui.visuals().weak_text_color();
    for (limit, label, align) in [
        (limits[0], "FWD", Align2::CENTER_TOP),
        (limits[1], "AFT", Align2::CENTER_TOP),
    ] {
        let x = x_at(limit);
        painter.line_segment(
            [pos2(x, track.top() - 1.0), pos2(x, track.bottom() + 1.0)],
            Stroke::new(1.5_f32, weak),
        );
        painter.text(
            pos2(x, track.bottom() + 2.0),
            align,
            format!("{label} {:.1} % MAC", view.pct_mac(limit)),
            FontId::proportional(11.0),
            weak,
        );
    }
    let x = x_at(cg);
    let marker = color(ui, state);
    painter.line_segment(
        [pos2(x, track.top() - 2.0), pos2(x, track.bottom() + 2.0)],
        Stroke::new(3.0_f32, marker),
    );
    if state.outside() {
        painter.rect_stroke(track, 3.0, Stroke::new(1.0_f32, marker));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cg_is_classified_against_the_limits() {
        let limits = [10.0, 20.0];
        assert_eq!(status(15.0, limits), Status::Inside);
        assert_eq!(status(10.5, limits), Status::NearForward);
        assert_eq!(status(19.5, limits), Status::NearAft);
        assert_eq!(status(9.9, limits), Status::BeyondForward);
        assert_eq!(status(20.1, limits), Status::BeyondAft);
    }

    #[test]
    fn a_cg_beyond_a_limit_names_the_excursion_in_percent_mac() {
        let mut view = crate::views::payload_editor::side_view::test_body();
        view.mac = 4.0;
        view.x_lemac = 0.0;
        let text = warning(&view, 21.0, [10.0, 20.0], Status::BeyondAft).unwrap();
        assert!(text.contains("25.0"), "{text}");
        assert!(warning(&view, 15.0, [10.0, 20.0], Status::Inside).is_none());
    }
}
