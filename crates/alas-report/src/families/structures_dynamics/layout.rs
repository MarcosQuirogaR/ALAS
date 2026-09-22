// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Two small drawing helpers shared by every figure in this family, native
//! to this port: matplotlib's `fig.add_subplot(1, n, i + 1)` and `ax.bar`
//! compute the same divisions for free, so upstream has nothing to name
//! here. `alas-report` had no 1xN side-by-side layout before this family's
//! stress and modes figures needed one, so this establishes the pattern.

use crate::scene::{Axes2D, Fill, Scene, SceneElement};

/// Canvas rectangles `(left, top, width, height)` for `n` equal-width panels
/// laid out left to right inside `area`, separated by `gutter` pixels.
pub fn panel_rects(n: usize, area: (f64, f64, f64, f64), gutter: f64) -> Vec<(f64, f64, f64, f64)> {
    let (left, top, total_width, height) = area;
    let n = n.max(1);
    let panel_w = ((total_width - gutter * (n as f64 - 1.0)) / n as f64).max(1.0);
    (0..n)
        .map(|i| (left + i as f64 * (panel_w + gutter), top, panel_w, height))
        .collect()
}

/// One bar of a grouped bar chart, spanning `[x0, x1]` in data coordinates
/// from `0` up to `val`, mapped through `axes`: shared by the frequency and
/// RMS grouped bar charts, both of which need the bar's canvas width and
/// height computed from the (possibly log-scaled) axes rather than a fixed
/// pixel width.
pub fn draw_bar(scene: &mut Scene, axes: &Axes2D, x0: f64, x1: f64, val: f64, fill: Fill) {
    let x_lo = axes.x_min.min(axes.x_max);
    let x_hi = axes.x_min.max(axes.x_max);
    let y_lo = axes.y_min.min(axes.y_max);
    let y_hi = axes.y_min.max(axes.y_max);
    let left = x0.min(x1).clamp(x_lo, x_hi);
    let right = x0.max(x1).clamp(x_lo, x_hi);
    if right <= left {
        return;
    }
    let baseline = 0.0_f64.clamp(y_lo, y_hi);
    let value = val.clamp(y_lo, y_hi);
    let p0 = axes.map_point(left, value);
    let p1 = axes.map_point(right, baseline);
    let x = p0[0].min(p1[0]);
    let y = p0[1].min(p1[1]);
    let width = (p1[0] - p0[0]).abs();
    let height = (p1[1] - p0[1]).abs().max(0.5);
    scene.add(SceneElement::Rect {
        x,
        y,
        width,
        height,
        rx: 0.0,
        fill: Some(fill),
        stroke: None,
    });
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Color;

    #[test]
    fn panels_tile_the_area_left_to_right_with_no_overlap() {
        let rects = panel_rects(3, (10.0, 20.0, 310.0, 100.0), 5.0);
        assert_eq!(rects.len(), 3);
        for r in &rects {
            assert_eq!(r.1, 20.0);
            assert_eq!(r.3, 100.0);
        }
        assert_eq!(rects[0].0, 10.0);
        let panel_w = rects[0].2;
        assert!((panel_w - 100.0).abs() < 1e-9);
        assert!((rects[1].0 - (rects[0].0 + panel_w + 5.0)).abs() < 1e-9);
        assert!((rects[2].0 - (rects[1].0 + panel_w + 5.0)).abs() < 1e-9);
    }

    #[test]
    fn zero_panels_is_treated_as_one_so_the_area_never_divides_by_zero() {
        let rects = panel_rects(0, (0.0, 0.0, 200.0, 50.0), 10.0);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].2, 200.0);
    }

    #[test]
    fn a_bar_from_zero_to_a_positive_value_has_a_positive_canvas_height() {
        let axes = Axes2D::new((0.0, 0.0, 100.0, 100.0), (0.0, 10.0), (0.0, 10.0));
        let mut scene = Scene::new(100.0, 100.0, None);
        draw_bar(
            &mut scene,
            &axes,
            1.0,
            3.0,
            5.0,
            Fill::new(Color::rgb(0, 0, 0)),
        );
        let SceneElement::Rect { width, height, .. } = scene.elements[0] else {
            panic!("expected a Rect element");
        };
        assert!((width - 20.0).abs() < 1e-9);
        assert!((height - 50.0).abs() < 1e-9);
    }

    #[test]
    fn a_bar_is_clipped_to_the_axes_when_the_value_exceeds_the_range() {
        let axes = Axes2D::new((10.0, 20.0, 100.0, 100.0), (0.0, 10.0), (0.0, 10.0));
        let mut scene = Scene::new(120.0, 140.0, None);
        draw_bar(
            &mut scene,
            &axes,
            1.0,
            3.0,
            20.0,
            Fill::new(Color::rgb(0, 0, 0)),
        );
        let SceneElement::Rect { y, height, .. } = scene.elements[0] else {
            panic!("expected a Rect element");
        };
        assert_eq!(y, axes.top);
        assert_eq!(height, axes.height);
    }
}
