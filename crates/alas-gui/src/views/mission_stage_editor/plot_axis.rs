// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The horizontal axis of the mission plot: a piecewise-linear map from mission
//! progress to plot width that gives every phase a minimum width, and the axis
//! drawn under the plot.
//!
//! Only the x mapping changes: each phase keeps its own start and end altitude
//! and its order; a phase is stretched when it would be too narrow to read and
//! the long phases (cruise legs) are compressed to pay for it.

use egui::{vec2, Align2, Pos2, Rect, Rounding, Stroke, Ui};

use super::model::{ProfileSegment, Trend};
use super::plot_style::{small_font, text_width};

/// Narrowest a phase is drawn, pt: room for its numbered marker.
pub(super) const MIN_PHASE_WIDTH: f32 = 28.0;
/// Clear space kept between two tick labels, pt (the least acceptable is 8).
const TICK_GAP: f32 = 10.0;
/// Distance of the tick labels under the axis line, pt.
const TICK_LABEL_OFFSET: f32 = 12.0;
/// Share of the plot width the phases' minimum widths may take together; the rest goes to the
/// long phases.
const MAX_FILL: f32 = 0.95;
/// A phase drawn narrower than this share of its true width is marked with an
/// axis break.
pub(super) const BREAK_BELOW: f32 = 0.85;

/// Map from progress (0 to 1) to the fraction of the plot width (0 to 1).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct XMap {
    /// `(progress, fraction)` at every phase boundary, both increasing.
    bounds: Vec<(f64, f32)>,
}

impl XMap {
    /// The map for `segments` on a plot `width` pt wide, phase `i` no narrower
    /// than `mins[i]` (all reduced together when the phases could not fit at
    /// those widths; a missing entry is [`MIN_PHASE_WIDTH`]).
    pub(super) fn new(segments: &[ProfileSegment], width: f32, mins: &[f32]) -> Self {
        let n = segments.len();
        if n == 0 || width <= 1.0 {
            return Self {
                bounds: vec![(0.0, 0.0), (1.0, 1.0)],
            };
        }
        let shares: Vec<f64> = segments
            .iter()
            .map(|segment| (segment.progress[1] - segment.progress[0]).max(0.0))
            .collect();
        let wanted: Vec<f32> = (0..n)
            .map(|i| mins.get(i).copied().unwrap_or(MIN_PHASE_WIDTH).max(1.0))
            .collect();
        let shrink = (MAX_FILL * width / wanted.iter().sum::<f32>()).min(1.0);
        let min_fractions: Vec<f32> = wanted.iter().map(|m| m * shrink / width).collect();
        let mut fixed = vec![false; n];
        let mut fractions = vec![0.0_f32; n];
        loop {
            let fixed_total: f32 = min_fractions
                .iter()
                .zip(&fixed)
                .filter(|(_, &f)| f)
                .map(|(m, _)| m)
                .sum();
            let free_share: f64 = shares
                .iter()
                .zip(&fixed)
                .filter(|(_, &f)| !f)
                .map(|(share, _)| share)
                .sum();
            let remaining = (1.0 - fixed_total).max(0.0);
            let mut changed = false;
            for i in 0..n {
                let min_fraction = min_fractions[i];
                if fixed[i] {
                    fractions[i] = min_fraction;
                    continue;
                }
                let proportional = if free_share > 0.0 {
                    remaining * (shares[i] / free_share) as f32
                } else {
                    remaining / (n as f32)
                };
                if proportional < min_fraction {
                    fixed[i] = true;
                    changed = true;
                } else {
                    fractions[i] = proportional;
                }
            }
            if !changed {
                break;
            }
        }
        let start = segments[0].progress[0];
        let mut bounds = vec![(start, 0.0_f32)];
        let mut cumulative = 0.0_f32;
        for (segment, fraction) in segments.iter().zip(&fractions) {
            cumulative += fraction;
            bounds.push((segment.progress[1], cumulative));
        }
        // Absorb rounding so the axis ends exactly at the plot's right edge.
        if let Some(last) = bounds.last_mut() {
            last.1 = 1.0;
        }
        Self { bounds }
    }

    /// Plot-width fraction of `progress`.
    pub(super) fn fraction(&self, progress: f64) -> f32 {
        let first = self.bounds[0];
        if progress <= first.0 {
            return first.1;
        }
        for pair in self.bounds.windows(2) {
            if progress <= pair[1].0 {
                let span = pair[1].0 - pair[0].0;
                if span <= f64::EPSILON {
                    return pair[1].1;
                }
                let t = ((progress - pair[0].0) / span) as f32;
                return pair[0].1 + t * (pair[1].1 - pair[0].1);
            }
        }
        self.bounds[self.bounds.len() - 1].1
    }

    /// The `(start, end)` fractions of phase `index`.
    pub(super) fn span(&self, index: usize) -> (f32, f32) {
        (self.bounds[index].1, self.bounds[index + 1].1)
    }

    /// Drawn width over true width of phase `index` (1 is to scale).
    pub(super) fn scale(&self, index: usize) -> f32 {
        let (from, to) = self.span(index);
        let truth = self.bounds[index + 1].0 - self.bounds[index].0;
        let total = self.bounds[self.bounds.len() - 1].0 - self.bounds[0].0;
        if truth <= f64::EPSILON || total <= f64::EPSILON {
            return 1.0;
        }
        (to - from) / (truth / total) as f32
    }
}

/// The name of a phase from its stage-list row: the row without its number and
/// without the level, duration or reserve note after two spaces, so
/// "3. Climb - FL370  (Reserve)" gives "Climb \u{2013} FL370".
pub(super) fn phase_name(row: &str, index: usize) -> String {
    let row = row
        .split("  (")
        .next()
        .unwrap_or(row)
        .trim()
        .replace(" - ", " \u{2013} ");
    match row.split_once(". ") {
        Some((number, rest)) if number.chars().all(|c| c.is_ascii_digit()) => {
            rest.trim().to_owned()
        }
        _ if row.is_empty() => (index + 1).to_string(),
        _ => row,
    }
}

/// The tick labels drawn from `entries` (`(progress, text, main)` in order). The main ones (the
/// start, the tops of climb and descent, the destination) go first; each other label is drawn
/// only when its text is distinct and it clears every label already drawn, on both sides, by at
/// least `gap` pt. A main label is dropped too when it would touch an earlier main one. `at`
/// gives a progress' x, `measure` a text's width; the result is in left-to-right order.
pub(super) fn tick_labels(
    entries: &[(f64, String, bool)],
    at: impl Fn(f64) -> f32,
    measure: impl Fn(&str) -> f32,
    limits: (f32, f32),
    gap: f32,
) -> Vec<(f32, String)> {
    let mut drawn: Vec<(f32, f32, String)> = Vec::new();
    // Only the main boundaries are labelled: a label at every short phase (the end of the
    // take-off, a few km in) crowds the axis and the numbered markers already mark them.
    {
        for (progress, text, is_main) in entries {
            if !*is_main || drawn.iter().any(|(_, _, other)| other == text) {
                continue;
            }
            let width = measure(text);
            let left =
                (at(*progress) - width / 2.0).clamp(limits.0, (limits.1 - width).max(limits.0));
            let right = left + width;
            if drawn
                .iter()
                .all(|(l, r, _)| left > r + gap || right + gap < *l)
            {
                drawn.push((left, right, text.clone()));
            }
        }
    }
    drawn.sort_by(|a, b| a.0.total_cmp(&b.0));
    drawn
        .into_iter()
        .map(|(left, _, text)| (left, text))
        .collect()
}

/// Which of the `segments.len() + 1` boundaries are main ones: the start, the end, the top of
/// each climb, the top of each descent and the end of the trip (before the reserves).
fn main_boundaries(segments: &[ProfileSegment]) -> Vec<bool> {
    let n = segments.len();
    let mut main = vec![false; n + 1];
    main[0] = true;
    main[n] = true;
    for index in 1..n {
        let (before, after) = (segments[index - 1].trend, segments[index].trend);
        let top_of_climb = before == Trend::Climb && after != Trend::Climb;
        let top_of_descent = before != Trend::Descent && after == Trend::Descent;
        let destination = !segments[index - 1].reserve && segments[index].reserve;
        main[index] = top_of_climb || top_of_descent || destination;
    }
    main
}

/// The labelled boundaries of the axis: the distance flown in km where `km` gives
/// one (the reserves have none), else the progress in %.
fn axis_entries(
    boundaries: &[f64],
    km: Option<&[Option<f64>]>,
    main: &[bool],
) -> Vec<(f64, String, bool)> {
    let is_main = |index: usize| main.get(index).copied().unwrap_or(false);
    boundaries
        .iter()
        .enumerate()
        .filter_map(|(index, &progress)| match km {
            Some(km) => km.get(index).copied().flatten().map(|km| {
                (
                    progress,
                    alas_units::notation::group_thousands(&format!("{km:.0}")),
                    is_main(index),
                )
            }),
            None => Some((
                progress,
                format!("{:.0} %", progress * 100.0),
                is_main(index),
            )),
        })
        .collect()
}

/// The x axis under the plot: the baseline, a tick at every phase boundary and
/// the distance (or progress) at the ones whose label is distinct from the last
/// drawn and clear of it, and a break mark on each phase drawn well under its
/// true width.
pub(super) fn draw_axis(
    ui: &Ui,
    painter: &egui::Painter,
    (frame, plot): (Rect, Rect),
    x: &XMap,
    segments: &[ProfileSegment],
    km: Option<&[Option<f64>]>,
) {
    let line = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let font = small_font(ui);
    let text = ui.visuals().weak_text_color();
    let background = ui.visuals().panel_fill;
    painter.line_segment(
        [frame.left_bottom(), frame.right_bottom()],
        Stroke::new(1.0_f32, line),
    );
    let mut boundaries = vec![0.0_f64];
    boundaries.extend(segments.iter().map(|segment| segment.progress[1]));
    let at = |progress: f64| plot.left() + plot.width() * x.fraction(progress);
    for &progress in &boundaries {
        painter.line_segment(
            [
                Pos2::new(at(progress), plot.bottom()),
                Pos2::new(at(progress), plot.bottom() + 4.0),
            ],
            Stroke::new(1.0_f32, line),
        );
    }
    for (left, label) in tick_labels(
        &axis_entries(&boundaries, km, &main_boundaries(segments)),
        at,
        |label| text_width(ui, label, &font),
        (frame.left() - 8.0, frame.right() + 8.0),
        TICK_GAP,
    ) {
        painter.text(
            Pos2::new(left, plot.bottom() + TICK_LABEL_OFFSET),
            Align2::LEFT_TOP,
            label,
            font.clone(),
            text,
        );
    }
    for index in 0..segments.len() {
        if x.scale(index) >= BREAK_BELOW {
            continue;
        }
        let (from, to) = x.span(index);
        // A break on a phase too short to hold one clear of its two end markers is left out.
        if plot.width() * (to - from) < 4.0 * super::plot_markers::RADIUS + 8.0 {
            continue;
        }
        let middle = plot.left() + plot.width() * f32::midpoint(from, to);
        let y = plot.bottom();
        painter.rect_filled(
            Rect::from_center_size(Pos2::new(middle, y), vec2(6.0, 6.0)),
            Rounding::ZERO,
            background,
        );
        for offset in [-1.8_f32, 1.8] {
            painter.line_segment(
                [
                    Pos2::new(middle + offset - 1.5, y + 4.0),
                    Pos2::new(middle + offset + 1.5, y - 4.0),
                ],
                Stroke::new(1.2_f32, text),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::mission_stage_editor::model::profile;

    fn segments() -> Vec<ProfileSegment> {
        let mut plan = alas_config::MissionPlan::from_profile(
            &alas_config::MissionProfileConfig::default(),
            35_000.0,
        );
        plan.stages.push(
            crate::views::mission_stage_editor::model::new_stage(&plan, "Cruise", 35_000.0)
                .expect("cruise"),
        );
        profile(&plan, 0.0, 0.0, 3_000.0)
    }

    #[test]
    fn every_phase_has_its_minimum_width_and_the_map_is_monotonic() {
        let segments = segments();
        let mins: Vec<f32> = (0..segments.len()).map(|i| 50.0 + 6.0 * i as f32).collect();
        for width in [620.0_f32, 1_180.0, 2_400.0] {
            let map = XMap::new(&segments, width, &mins);
            let shrink = (MAX_FILL * width / mins.iter().sum::<f32>()).min(1.0);
            let mut last = 0.0_f32;
            for (i, min) in mins.iter().enumerate() {
                let (from, to) = map.span(i);
                assert!(from >= last - 1e-6 && to > from, "monotonic at {width}");
                assert!(
                    (to - from) * width >= min * shrink - 0.5,
                    "phase {i} at {width}"
                );
                last = to;
            }
            assert!((last - 1.0).abs() < 1e-5);
            let mut previous = -1.0_f32;
            for k in 0..=200 {
                let x = map.fraction(f64::from(k) / 200.0);
                assert!(x >= previous);
                previous = x;
            }
        }
    }

    #[test]
    fn a_phase_name_is_the_row_without_number_or_reserve_note() {
        assert_eq!(
            phase_name("3. Climb - FL370  (Reserve)", 2),
            "Climb \u{2013} FL370"
        );
        assert_eq!(phase_name("11. Missed approach", 10), "Missed approach");
        assert_eq!(phase_name("", 4), "5");
    }

    #[test]
    fn tick_labels_are_distinct_and_never_touch() {
        let boundaries = [0.0, 0.01, 0.02, 0.88, 0.88, 0.884, 0.9, 1.0];
        let entries: Vec<(f64, String, bool)> = boundaries
            .iter()
            .map(|p| (*p, format!("{:.0} %", p * 100.0), *p == 0.0 || *p == 1.0))
            .collect();
        let measure = |text: &str| text.chars().count() as f32 * 6.0;
        let at = |p: f64| 40.0 + 600.0 * p as f32;
        let labels = tick_labels(&entries, at, measure, (30.0, 660.0), 8.0);
        let texts: Vec<&str> = labels.iter().map(|(_, t)| t.as_str()).collect();
        assert!(texts.windows(2).all(|w| w[0] != w[1]), "{texts:?}");
        for pair in labels.windows(2) {
            assert!(
                pair[1].0 > pair[0].0 + measure(&pair[0].1) + 7.9,
                "{labels:?}"
            );
        }
        assert_eq!(texts.last(), Some(&"100 %"));
    }

    #[test]
    fn the_axis_labels_kilometres_where_known_and_percent_otherwise() {
        let boundaries = [0.0, 0.5, 1.0];
        let km = [Some(0.0), None, Some(812.4)];
        let texts: Vec<String> = axis_entries(&boundaries, Some(&km), &[])
            .into_iter()
            .map(|(_, t, _)| t)
            .collect();
        assert_eq!(texts, ["0", "812"]);
        let texts: Vec<String> = axis_entries(&boundaries, None, &[])
            .into_iter()
            .map(|(_, t, _)| t)
            .collect();
        assert_eq!(texts, ["0 %", "50 %", "100 %"]);
    }
}
