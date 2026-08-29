// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Immediate altitude-profile feedback for Mission Analysis.
//!
//! A completed mission produces the detailed report figure. This lightweight
//! schematic instead responds to the edited configuration, making the phase
//! order and altitude targets understandable before a run exists.

use alas_config::{airports, AlasConfig, MissionProfileConfig};
use egui::{vec2, Align2, Color32, FontId, Pos2, Rect, Rounding, Sense, Stroke, Ui};

use crate::theme::{card_frame, success_color};
use crate::views::{tr, tr_fields};

const ACTIVE_FRACTION: f64 = 1.0e-6;
const METRES_PER_FOOT: f64 = 0.3048;

#[derive(Clone, Copy)]
enum SegmentKind {
    Climb,
    Cruise,
    Descent,
}

struct PreviewSegment {
    label: &'static str,
    altitude_m: f64,
    weight: f64,
    kind: SegmentKind,
}

/// Render the current mission profile as a live, schematic altitude figure.
pub(super) fn show_mission_profile_preview(ui: &mut Ui, config: &AlasConfig) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(egui::RichText::new(tr("Live mission profile")).strong());
        ui.label(
            egui::RichText::new(tr(
                "A live schematic of the configured altitude targets. It updates before a run; horizontal spacing is illustrative, not a flight prediction.",
            ))
            .weak()
            .small(),
        );
        ui.add_space(6.0);

        let segments = preview_segments(config);
        let (rect, response) = ui.allocate_exact_size(
            vec2(ui.available_width().max(220.0), 220.0),
            Sense::hover(),
        );
        paint_profile(ui, rect, config, &segments);
        response.on_hover_text(tr(
            "The preview shows the configured climb, cruise, and descent target levels before the full mission is solved.",
        ));
    });
}

fn preview_segments(config: &AlasConfig) -> Vec<PreviewSegment> {
    let profile = &config.mission.profile;
    let departure_m = airport_elevation_m(&config.departure_airport);
    let arrival_m = airport_elevation_m(&config.arrival_airport);
    let cruise_m = config.requirements.cruise_altitude_m.max(departure_m + 1.0);
    let first_level_m =
        (cruise_m * profile.initial_climb_altitude_fraction).max(departure_m + 3000.0);
    let second_level_m =
        (cruise_m * profile.step_climb_1_altitude_fraction).max(first_level_m + 300.0);
    let fractions = [
        profile.cruise_1_distance_fraction,
        profile.cruise_2_distance_fraction,
        profile.cruise_3_distance_fraction,
    ];
    let active_cruise = active_cruise_count(profile);
    let active_fraction_total = fractions[..active_cruise]
        .iter()
        .map(|fraction| fraction.max(0.0))
        .sum::<f64>()
        .max(ACTIVE_FRACTION);
    let mut segments = vec![
        PreviewSegment {
            label: "T/O",
            altitude_m: departure_m + profile.takeoff_altitude_gain_m,
            weight: 0.06,
            kind: SegmentKind::Climb,
        },
        PreviewSegment {
            label: "CLB",
            altitude_m: first_level_m,
            weight: 0.08,
            kind: SegmentKind::Climb,
        },
    ];

    let cruise_targets = [first_level_m, second_level_m, cruise_m];
    let cruise_labels = ["C1", "C2", "C3"];
    let step_labels = ["SC1", "SC2"];
    for index in 0..active_cruise {
        if index > 0 {
            segments.push(PreviewSegment {
                label: step_labels[index - 1],
                altitude_m: cruise_targets[index],
                weight: 0.025,
                kind: SegmentKind::Climb,
            });
        }
        segments.push(PreviewSegment {
            label: cruise_labels[index],
            altitude_m: cruise_targets[index],
            weight: 0.62 * fractions[index].max(0.0) / active_fraction_total,
            kind: SegmentKind::Cruise,
        });
    }

    let descent_targets = [
        profile.descent_1_altitude_ft,
        profile.descent_2_altitude_ft,
        profile.descent_3_altitude_ft,
        profile.descent_4_altitude_ft,
    ];
    for (index, altitude_ft) in descent_targets.into_iter().enumerate() {
        if altitude_ft > arrival_m / METRES_PER_FOOT && altitude_ft > ACTIVE_FRACTION {
            segments.push(PreviewSegment {
                label: match index {
                    0 => "D1",
                    1 => "D2",
                    2 => "D3",
                    _ => "D4",
                },
                altitude_m: altitude_ft * METRES_PER_FOOT,
                weight: 0.04,
                kind: SegmentKind::Descent,
            });
        }
    }
    segments.push(PreviewSegment {
        label: "FIN",
        altitude_m: arrival_m,
        weight: 0.05,
        kind: SegmentKind::Descent,
    });
    segments
}

fn airport_elevation_m(name: &str) -> f64 {
    airports::get(name)
        .map(|airport| airport.elevation_m)
        .unwrap_or_default()
}

fn active_cruise_count(profile: &MissionProfileConfig) -> usize {
    [
        profile.cruise_1_distance_fraction,
        profile.cruise_2_distance_fraction,
        profile.cruise_3_distance_fraction,
    ]
    .iter()
    .rposition(|fraction| *fraction > ACTIVE_FRACTION)
    .map_or(1, |index| index + 1)
}

fn paint_profile(ui: &Ui, rect: Rect, config: &AlasConfig, segments: &[PreviewSegment]) {
    let painter = ui.painter();
    painter.rect_filled(rect, Rounding::same(8.0), ui.visuals().extreme_bg_color);
    painter.rect_stroke(
        rect,
        Rounding::same(8.0),
        Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    let plot = Rect::from_min_max(rect.min + vec2(42.0, 18.0), rect.max - vec2(16.0, 34.0));
    let departure_m = airport_elevation_m(&config.departure_airport);
    let max_altitude_m = segments
        .iter()
        .map(|segment| segment.altitude_m)
        .fold(config.requirements.cruise_altitude_m, f64::max)
        .max(departure_m + 1.0);
    let range_m = (max_altitude_m - departure_m).max(1.0);
    let weak = ui.visuals().weak_text_color();
    for fraction in [0.0, 0.5, 1.0] {
        let y = egui::lerp(plot.bottom()..=plot.top(), fraction);
        painter.line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(1.0_f32, Color32::from_white_alpha(28)),
        );
    }
    painter.line_segment(
        [plot.left_bottom(), plot.right_bottom()],
        Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    painter.line_segment(
        [plot.left_bottom(), plot.left_top()],
        Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    painter.text(
        Pos2::new(plot.left() - 6.0, plot.top()),
        Align2::RIGHT_TOP,
        format_altitude(max_altitude_m),
        FontId::proportional(10.0),
        weak,
    );
    painter.text(
        Pos2::new(plot.left() - 6.0, plot.bottom()),
        Align2::RIGHT_BOTTOM,
        format_altitude(departure_m),
        FontId::proportional(10.0),
        weak,
    );
    painter.text(
        Pos2::new(plot.center().x, rect.bottom() - 8.0),
        Align2::CENTER_BOTTOM,
        tr("Mission progress (schematic)"),
        FontId::proportional(10.0),
        weak,
    );

    let total_weight = segments
        .iter()
        .map(|segment| segment.weight)
        .sum::<f64>()
        .max(ACTIVE_FRACTION);
    let point = |x: f32, altitude_m: f64| {
        let y = plot.bottom() - (altitude_m - departure_m) as f32 / range_m as f32 * plot.height();
        Pos2::new(x, y.clamp(plot.top(), plot.bottom()))
    };
    let mut x = plot.left();
    let mut previous = point(x, departure_m);
    for segment in segments {
        x += plot.width() * (segment.weight / total_weight) as f32;
        let next = point(x, segment.altitude_m);
        let color = match segment.kind {
            SegmentKind::Climb => ui.visuals().hyperlink_color,
            SegmentKind::Cruise => success_color(ui.visuals()),
            SegmentKind::Descent => ui.visuals().warn_fg_color,
        };
        painter.line_segment([previous, next], Stroke::new(2.5_f32, color));
        painter.circle_filled(next, 3.0, color);
        painter.text(
            Pos2::new((previous.x + next.x) * 0.5, next.y - 8.0),
            Align2::CENTER_BOTTOM,
            segment.label,
            FontId::proportional(10.0),
            color,
        );
        previous = next;
    }
    painter.text(
        Pos2::new(plot.right(), plot.top()),
        Align2::RIGHT_TOP,
        tr_fields(
            "Configured segments: {count}",
            &[("count", segments.len().to_string())],
        ),
        FontId::proportional(10.0),
        weak,
    );
}

fn format_altitude(altitude_m: f64) -> String {
    format!("{:.1} km", altitude_m.max(0.0) / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::{active_cruise_count, preview_segments};
    use alas_config::AlasConfig;

    #[test]
    fn preview_matches_the_default_profile_phase_order() {
        let config = AlasConfig::default();
        let labels: Vec<&str> = preview_segments(&config)
            .iter()
            .map(|segment| segment.label)
            .collect();
        assert_eq!(
            labels,
            ["T/O", "CLB", "C1", "SC1", "C2", "SC2", "C3", "D1", "D2", "D3", "D4", "FIN"]
        );
    }

    #[test]
    fn zero_share_cruise_slots_are_not_shown_as_active_phases() {
        let mut config = AlasConfig::default();
        config.mission.profile.cruise_2_distance_fraction = 0.0;
        config.mission.profile.cruise_3_distance_fraction = 0.0;
        assert_eq!(active_cruise_count(&config.mission.profile), 1);
        let labels: Vec<&str> = preview_segments(&config)
            .iter()
            .map(|segment| segment.label)
            .collect();
        assert!(!labels.contains(&"SC1"));
        assert!(!labels.contains(&"C2"));
    }
}
