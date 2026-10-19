// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in capture of the data behind a figure while its scene is built.
//!
//! A [`Scene`](crate::scene::Scene) holds pixel coordinates only. While a
//! [`CaptureGuard`] is alive on the building thread,
//! [`Axes2D::map_point`](crate::scene::Axes2D::map_point) logs the exact data
//! pair behind every pixel it returns. When the figure then adds a polyline,
//! polygon, marker or segment whose pixels all came from one axes,
//! [`observe`] resolves them back to those data pairs bit for bit: no inverse
//! arithmetic is involved. Series added through `add_line_series` and heatmap
//! grids are recorded from their inputs directly. Only bars (rectangles) are
//! recovered by inverting the axes mapping and are flagged as not exact.

use std::cell::RefCell;
use std::collections::HashMap;
mod model;
pub use model::*;

use crate::scene::{Axes2D, SceneElement};

/// `(axes index, x, y)` behind one pixel.
type MappedPoint = (u32, f64, f64);

#[derive(Default)]
struct CaptureState {
    depth: u32,
    chrome: u32,
    axes: Vec<AxesRecord>,
    last_axes: usize,
    points: HashMap<(u64, u64), Vec<MappedPoint>>,
}

thread_local! {
    static STATE: RefCell<CaptureState> = RefCell::new(CaptureState::default());
}

/// Whether the current thread records plot data.
pub fn is_active() -> bool {
    STATE.with(|state| state.borrow().depth > 0)
}

/// Record plot data for every scene built on this thread while alive.
pub struct CaptureGuard(());

impl CaptureGuard {
    /// Begin (or nest) capture on the current thread.
    pub fn begin() -> Self {
        STATE.with(|state| state.borrow_mut().depth += 1);
        Self(())
    }
}

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.depth = state.depth.saturating_sub(1);
            if state.depth == 0 {
                *state = CaptureState::default();
            }
        });
    }
}

/// Run `build` with capture active and return its result.
pub fn with_capture<T>(build: impl FnOnce() -> T) -> T {
    let _guard = CaptureGuard::begin();
    build()
}

/// Suppress capture of axes frames, ticks, legends and colour bars.
pub(crate) struct ChromeGuard(());

impl ChromeGuard {
    pub(crate) fn begin() -> Self {
        STATE.with(|state| state.borrow_mut().chrome += 1);
        Self(())
    }
}

impl Drop for ChromeGuard {
    fn drop(&mut self) {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.chrome = state.chrome.saturating_sub(1);
        });
    }
}

/// Log one `map_point` result. Called from `Axes2D::map_point`.
pub(crate) fn note_map_point(axes: &Axes2D, x: f64, y: f64, pixel: [f64; 2]) {
    if capturing() {
        note_mapped(AxesRecord::from(axes), x, y, pixel);
    }
}

fn capturing() -> bool {
    STATE.with(|state| {
        let state = state.borrow();
        state.depth > 0 && state.chrome == 0
    })
}

fn note_mapped(record: AxesRecord, x: f64, y: f64, pixel: [f64; 2]) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let index = if state
            .axes
            .get(state.last_axes)
            .is_some_and(|a| a.same_axes(&record))
        {
            state.last_axes
        } else if let Some(found) = state.axes.iter().position(|a| a.same_axes(&record)) {
            state.last_axes = found;
            found
        } else {
            state.axes.push(record);
            state.last_axes = state.axes.len() - 1;
            state.last_axes
        };
        if state.points.len() > 4_000_000 {
            state.points.clear();
        }
        let entry = state
            .points
            .entry((pixel[0].to_bits(), pixel[1].to_bits()))
            .or_default();
        if entry.len() < 6
            && !entry
                .iter()
                .any(|e| e.0 == index as u32 && e.1 == x && e.2 == y)
        {
            entry.push((index as u32, x, y));
        }
    });
}

/// Resolve canvas points back to one axes and its exact data pairs.
fn resolve(points: &[[f64; 2]]) -> Option<(AxesRecord, Vec<(f64, f64)>)> {
    if points.is_empty() {
        return None;
    }
    STATE.with(|state| {
        let state = state.borrow();
        let key = |p: &[f64; 2]| (p[0].to_bits(), p[1].to_bits());
        let candidates = state.points.get(&key(&points[0]))?;
        'axes: for candidate in candidates {
            let mut data = Vec::with_capacity(points.len());
            for point in points {
                let hit = state
                    .points
                    .get(&key(point))?
                    .iter()
                    .rev()
                    .find(|entry| entry.0 == candidate.0);
                match hit {
                    Some(entry) => data.push((entry.1, entry.2)),
                    None => continue 'axes,
                }
            }
            return Some((state.axes[candidate.0 as usize], data));
        }
        None
    })
}

fn contained_axes(rect: [f64; 4]) -> Option<AxesRecord> {
    STATE.with(|state| {
        let state = state.borrow();
        state
            .axes
            .iter()
            .find(|axes| {
                axes.contains_pixel([rect[0], rect[1]])
                    && axes.contains_pixel([rect[0] + rect[2], rect[1] + rect[3]])
                    && !(rect[2] >= axes.width - 1.0 && rect[3] >= axes.height - 1.0)
            })
            .copied()
    })
}

fn stroke_hex(stroke: &crate::scene::Stroke) -> String {
    stroke.color.to_hex_rgb()
}

/// Record the data behind a scene element about to be added. Cheap when inactive.
pub(crate) fn observe(trace: &mut DataTrace, element: &SceneElement) {
    let (active, chrome) = STATE.with(|state| {
        let state = state.borrow();
        (state.depth > 0, state.chrome > 0)
    });
    if !active || chrome {
        return;
    }
    match element {
        SceneElement::Polyline { points, stroke } => {
            if let Some((axes, data)) = resolve(points) {
                let series =
                    TraceSeries::from_pairs(SeriesKind::Line, Some(stroke_hex(stroke)), &data);
                trace.panel_mut(&axes).series.push(series);
            }
        }
        SceneElement::Polygon {
            points,
            fill,
            stroke,
        } => {
            if let Some((axes, data)) = resolve(points) {
                let color = fill
                    .map(|f| f.color.to_hex_rgb())
                    .or_else(|| stroke.as_ref().map(stroke_hex));
                let series = TraceSeries::from_pairs(SeriesKind::Polygon, color, &data);
                trace.panel_mut(&axes).series.push(series);
            }
        }
        SceneElement::Circle {
            center,
            fill,
            stroke,
            ..
        } => {
            if let Some((axes, data)) = resolve(std::slice::from_ref(center)) {
                let color = fill
                    .map(|f| f.color.to_hex_rgb())
                    .or_else(|| stroke.as_ref().map(stroke_hex));
                let panel = trace.panel_mut(&axes);
                match panel
                    .series
                    .last_mut()
                    .filter(|s| s.kind == SeriesKind::Points && s.color == color)
                {
                    Some(series) => {
                        series.x.push(data[0].0);
                        series.y.push(data[0].1);
                    }
                    None => {
                        panel
                            .series
                            .push(TraceSeries::from_pairs(SeriesKind::Points, color, &data))
                    }
                }
            }
        }
        SceneElement::Line { p1, p2, stroke } => {
            if let Some((axes, data)) = resolve(&[*p1, *p2]) {
                let color = Some(stroke_hex(stroke));
                let panel = trace.panel_mut(&axes);
                match panel
                    .series
                    .last_mut()
                    .filter(|s| s.kind == SeriesKind::Segments && s.color == color)
                {
                    Some(series) => {
                        series.x.extend([data[0].0, data[1].0]);
                        series.y.extend([data[0].1, data[1].1]);
                    }
                    None => panel.series.push(TraceSeries::from_pairs(
                        SeriesKind::Segments,
                        color,
                        &data,
                    )),
                }
            }
        }
        SceneElement::Rect {
            x,
            y,
            width,
            height,
            fill,
            stroke,
            ..
        } => {
            if let Some(axes) = contained_axes([*x, *y, *width, *height]) {
                let a = axes.pixel_to_data([*x, *y + *height]);
                let b = axes.pixel_to_data([*x + *width, *y]);
                let color = fill
                    .map(|f| f.color.to_hex_rgb())
                    .or_else(|| stroke.as_ref().map(stroke_hex));
                let panel = trace.panel_mut(&axes);
                match panel
                    .series
                    .last_mut()
                    .filter(|s| s.kind == SeriesKind::Bars && s.color == color)
                {
                    Some(series) => push_bar(series, a, b),
                    None => {
                        let mut series = TraceSeries::new(SeriesKind::Bars, color, false);
                        push_bar(&mut series, a, b);
                        panel.series.push(series);
                    }
                }
            }
        }
        SceneElement::Text {
            text,
            pos,
            angle_deg,
            font_size,
            ..
        } if trace.texts.len() < 4000 => {
            trace.texts.push(TraceText {
                text: text.clone(),
                pos: *pos,
                angle_deg: *angle_deg,
                font_size: *font_size,
            });
        }
        _ => {}
    }
}

fn push_bar(series: &mut TraceSeries, low: [f64; 2], high: [f64; 2]) {
    series.x.push(low[0]);
    series.y.push(low[1]);
    series.x2.push(high[0]);
    series.y2.push(high[1]);
}
