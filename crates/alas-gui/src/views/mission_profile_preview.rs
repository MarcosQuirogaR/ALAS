// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Immediate altitude-profile feedback for Mission Analysis.
//!
//! A completed mission produces the detailed report figure. This lightweight
//! schematic instead responds to the edited configuration, making the phase
//! order and altitude targets understandable before a run exists.

use alas_config::{airport_dataset, AlasConfig, MissionProfileConfig};
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
    id: &'static str,
    label: &'static str,
    altitude_m: f64,
    weight: f64,
    kind: SegmentKind,
}

/// Render the current mission profile as a non-interactive altitude figure.
///
/// The Advanced Mission page uses [`show_interactive_mission_profile_preview`]
/// so a click can select a phase and open its detached editor. This wrapper is
/// retained for the legacy mission-form renderer and for report-side previews
/// that only need the figure.
pub(crate) fn show_mission_profile_preview(ui: &mut Ui, config: &AlasConfig) {
    show_mission_profile_preview_inner(None, ui, config);
}

/// Render the live mission profile and route clicks to the detached phase
/// editor. The profile is derived from the same `mission.profile` values that
/// the solver consumes, so the click target never edits a parallel view-only
/// model.
pub(crate) fn show_interactive_mission_profile_preview(
    state: &mut crate::state::AppState,
    ui: &mut Ui,
    config: &AlasConfig,
) {
    show_mission_profile_preview_inner(Some(state), ui, config);
}

fn show_mission_profile_preview_inner(
    mut state: Option<&mut crate::state::AppState>,
    ui: &mut Ui,
    config: &AlasConfig,
) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(egui::RichText::new(tr("Live mission profile")).strong());
        ui.add_space(4.0);

        let segments = preview_segments(config);
        // The schematic must never demand more width than the card actually
        // has: forcing a fixed floor here (420 pt) used to widen the whole
        // Inputs page past `layout::CONTENT_MIN_WIDTH` (400 pt) on a narrow
        // window, since that floor did not account for the central panel's
        // and card's own margins. The plot's own inset margins keep it
        // legible well below the floor this replaced.
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width().max(1.0), 300.0), Sense::click());
        paint_profile(ui, rect, config, &segments);
        let response = response.on_hover_text(tr(
            "Click a phase in the profile to edit its actual mission parameters in a detached window.",
        ));
        if response.clicked() {
            if let (Some(pointer), Some(state)) =
                (response.interact_pointer_pos(), state.as_deref_mut())
            {
                if let Some(phase_id) = phase_at_pointer(rect, &segments, pointer) {
                    crate::views::mission_profile_inputs::open_phase_window(state, phase_id);
                }
            }
        }
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
            id: "takeoff",
            label: "T/O",
            altitude_m: departure_m + profile.takeoff_altitude_gain_m,
            weight: 0.06,
            kind: SegmentKind::Climb,
        },
        PreviewSegment {
            id: "initial_climb",
            label: "CLB",
            altitude_m: first_level_m,
            weight: 0.08,
            kind: SegmentKind::Climb,
        },
    ];

    let cruise_targets = [first_level_m, second_level_m, cruise_m];
    let cruise_ids = ["cruise_1", "cruise_2", "cruise_3"];
    let cruise_labels = ["C1", "C2", "C3"];
    let step_ids = ["step_climb_1", "step_climb_2"];
    let step_labels = ["SC1", "SC2"];
    for index in 0..active_cruise {
        if index > 0 {
            segments.push(PreviewSegment {
                id: step_ids[index - 1],
                label: step_labels[index - 1],
                altitude_m: cruise_targets[index],
                weight: 0.025,
                kind: SegmentKind::Climb,
            });
        }
        segments.push(PreviewSegment {
            id: cruise_ids[index],
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
                id: match index {
                    0 => "descent_1",
                    1 => "descent_2",
                    2 => "descent_3",
                    _ => "descent_4",
                },
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
        id: "final_approach",
        label: "FIN",
        altitude_m: arrival_m,
        weight: 0.05,
        kind: SegmentKind::Descent,
    });
    segments
}

fn airport_elevation_m(name: &str) -> f64 {
    airport_dataset::resolve(name)
        .ok()
        .and_then(|airport| airport.elevation_m.value)
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
    let plot = Rect::from_min_max(rect.min + vec2(78.0, 18.0), rect.max - vec2(34.0, 40.0));
    let departure_m = airport_elevation_m(&config.departure_airport);
    let max_altitude_m = segments
        .iter()
        .map(|segment| segment.altitude_m)
        .fold(config.requirements.cruise_altitude_m, f64::max)
        .max(departure_m + 1.0);
    let tick_m = 5_000.0 * METRES_PER_FOOT;
    let min_tick_m = (departure_m / tick_m).floor() * tick_m;
    let max_tick_m = ((max_altitude_m / tick_m).ceil() * tick_m).max(min_tick_m + tick_m);
    let range_m = (max_tick_m - min_tick_m).max(tick_m);
    let weak = ui.visuals().weak_text_color();
    let tick_count = ((max_tick_m - min_tick_m) / tick_m).round() as usize;
    for tick in 0..=tick_count {
        let altitude_m = min_tick_m + tick as f64 * tick_m;
        let fraction = ((altitude_m - min_tick_m) / range_m).clamp(0.0, 1.0);
        let y = egui::lerp(plot.bottom()..=plot.top(), fraction as f32);
        painter.line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(
                1.0_f32,
                Color32::from_white_alpha(if tick % 2 == 0 { 36 } else { 20 }),
            ),
        );
        painter.text(
            Pos2::new(plot.left() - 8.0, y),
            Align2::RIGHT_CENTER,
            format_altitude_ft(altitude_m),
            FontId::proportional(10.0),
            weak,
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
        let y = plot.bottom() - (altitude_m - min_tick_m) as f32 / range_m as f32 * plot.height();
        Pos2::new(x, y.clamp(plot.top(), plot.bottom()))
    };
    let mut x = plot.left();
    let mut previous = point(x, departure_m);
    let mut label_rects = Vec::with_capacity(segments.len());
    for (index, segment) in segments.iter().enumerate() {
        x += plot.width() * (segment.weight / total_weight) as f32;
        let next = point(x, segment.altitude_m);
        let color = match segment.kind {
            SegmentKind::Climb => ui.visuals().hyperlink_color,
            SegmentKind::Cruise => success_color(ui.visuals()),
            SegmentKind::Descent => ui.visuals().warn_fg_color,
        };
        painter.line_segment([previous, next], Stroke::new(2.5_f32, color));
        painter.circle_filled(next, 3.0, color);
        let label_rect = place_label_rect(plot, next, segment.kind, index, &label_rects);
        let label_x = label_rect.center().x;
        let label_y = label_rect.center().y;
        painter.rect_filled(
            label_rect,
            Rounding::same(3.0),
            ui.visuals().extreme_bg_color,
        );
        painter.text(
            Pos2::new(label_x, label_y),
            Align2::CENTER_CENTER,
            segment.label,
            FontId::proportional(10.0),
            color,
        );
        label_rects.push(label_rect);
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

/// Place a phase label inside the plot without covering another label or the
/// phase point. The alternating preferred side follows the trajectory, while
/// the fallback offsets keep dense descent rungs readable at narrow widths.
fn place_label_rect(
    plot: Rect,
    point: Pos2,
    kind: SegmentKind,
    index: usize,
    placed: &[Rect],
) -> Rect {
    const LABEL_SIZE: egui::Vec2 = egui::vec2(30.0, 16.0);
    let preferred = match kind {
        SegmentKind::Descent if index % 2 == 0 => [20.0, -20.0, 38.0, -38.0, 56.0, -56.0, 0.0],
        SegmentKind::Descent => [-20.0, 20.0, -38.0, 38.0, -56.0, 56.0, 0.0],
        SegmentKind::Climb if index % 2 == 0 => [-20.0, 20.0, -38.0, 38.0, -56.0, 56.0, 0.0],
        SegmentKind::Climb => [20.0, -20.0, 38.0, -38.0, 56.0, -56.0, 0.0],
        SegmentKind::Cruise => [-20.0, 20.0, -38.0, 38.0, 0.0, -56.0, 56.0],
    };
    let half_width = LABEL_SIZE.x / 2.0;
    let half_height = LABEL_SIZE.y / 2.0;
    for offset in preferred {
        let center = Pos2::new(
            point
                .x
                .clamp(plot.left() + half_width, plot.right() - half_width),
            (point.y + offset).clamp(plot.top() + half_height, plot.bottom() - half_height),
        );
        // A label whose offset was clamped onto the point would obscure the
        // phase marker and its line. Try the next side before accepting it.
        if (center.y - point.y).abs() < half_height + 2.0 {
            continue;
        }
        let rect = Rect::from_center_size(center, LABEL_SIZE);
        if placed
            .iter()
            .all(|other| !other.expand(2.0).intersects(rect))
        {
            return rect;
        }
    }
    // The plot has ample vertical room for the current phase count. This
    // final bounded fallback keeps the renderer total if a future profile
    // exceeds the preferred offset palette.
    Rect::from_center_size(
        Pos2::new(
            point
                .x
                .clamp(plot.left() + half_width, plot.right() - half_width),
            point
                .y
                .clamp(plot.top() + half_height, plot.bottom() - half_height),
        ),
        LABEL_SIZE,
    )
}

fn format_altitude_ft(altitude_m: f64) -> String {
    format!("{:.0} ft", (altitude_m / METRES_PER_FOOT).max(0.0))
}

fn phase_at_pointer(
    rect: Rect,
    segments: &[PreviewSegment],
    pointer: Pos2,
) -> Option<&'static str> {
    if !rect.contains(pointer) || segments.is_empty() {
        return None;
    }
    let plot = Rect::from_min_max(rect.min + vec2(78.0, 18.0), rect.max - vec2(34.0, 40.0));
    if !plot.contains(pointer) {
        return None;
    }
    let total_weight = segments
        .iter()
        .map(|segment| segment.weight)
        .sum::<f64>()
        .max(ACTIVE_FRACTION);
    let mut x = plot.left();
    let mut nearest = None;
    let mut distance = f32::INFINITY;
    for segment in segments {
        x += plot.width() * (segment.weight / total_weight) as f32;
        let candidate = (pointer.x - x).abs();
        if candidate < distance {
            distance = candidate;
            nearest = Some(segment.id);
        }
    }
    nearest
}

#[cfg(test)]
mod tests {
    use super::{active_cruise_count, place_label_rect, preview_segments};
    use alas_config::AlasConfig;
    use egui::{pos2, vec2, Rect};

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

    #[test]
    fn default_profile_labels_stay_inside_the_plot_without_overlap() {
        let config = AlasConfig::default();
        let segments = preview_segments(&config);
        let plot = Rect::from_min_size(pos2(0.0, 0.0), vec2(308.0, 242.0));
        let mut placed = Vec::new();
        for (index, segment) in segments.iter().enumerate() {
            let point = pos2(
                plot.left() + plot.width() * (index as f32 + 1.0) / segments.len() as f32,
                plot.center().y,
            );
            let label = place_label_rect(plot, point, segment.kind, index, &placed);
            assert!(plot.contains(label.min) && plot.contains(label.max));
            assert!(placed
                .iter()
                .all(|other| !other.expand(2.0).intersects(label)));
            placed.push(label);
        }
        assert_eq!(segments.len(), placed.len());
    }
}
