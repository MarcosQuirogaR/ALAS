// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where the centre-of-gravity label of the side view goes: above the airframe, in the
//! canvas, clear of the body and of every lifting surface (the fin), with a leader line down to
//! the mark. Nothing of the aircraft is covered by text.

use egui::{pos2, vec2, Pos2, Rect, Vec2};

use super::canvas::Frame;
use super::side_view::SideView;

/// Gap kept between the label and the silhouette, points.
const CLEARANCE: f32 = 3.0;
/// Lowest height of the label above the body at the mark, points.
const LEADER: f32 = 14.0;
/// Steps of the search for a free place: up and sideways, points.
const STEP_UP: f32 = 6.0;
const STEP_ACROSS: f32 = 16.0;
/// Farthest the label moves sideways to be clear, points.
const MAX_ACROSS: f32 = 900.0;

/// Where the label goes and the leader from the mark to it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Placed {
    pub rect: Rect,
    /// The leader: from the mark on the body's top to the label's lower edge.
    pub leader: [Pos2; 2],
}

/// Whether the convex polygon `points` and the rectangle `rect` overlap (separating axes).
fn polygon_hits_rect(points: &[Pos2], rect: Rect) -> bool {
    if points.len() < 3 {
        return false;
    }
    let (mut lo, mut hi) = (pos2(f32::MAX, f32::MAX), pos2(f32::MIN, f32::MIN));
    for p in points {
        lo = pos2(lo.x.min(p.x), lo.y.min(p.y));
        hi = pos2(hi.x.max(p.x), hi.y.max(p.y));
    }
    if hi.x < rect.left() || lo.x > rect.right() || hi.y < rect.top() || lo.y > rect.bottom() {
        return false;
    }
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    for (index, a) in points.iter().enumerate() {
        let b = points[(index + 1) % points.len()];
        let axis = vec2(-(b.y - a.y), b.x - a.x);
        if axis.length_sq() < 1e-9 {
            continue;
        }
        let project = |p: Pos2| axis.x * p.x + axis.y * p.y;
        let (p_lo, p_hi) = points
            .iter()
            .map(|p| project(*p))
            .fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(v), h.max(v)));
        let (r_lo, r_hi) = corners
            .iter()
            .map(|p| project(*p))
            .fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(v), h.max(v)));
        if p_hi < r_lo || r_hi < p_lo {
            return false;
        }
    }
    true
}

/// Whether `rect` touches the body's side outline or a lifting surface.
fn collides(frame: &Frame, view: &SideView, rect: Rect) -> bool {
    let rect = rect.expand(CLEARANCE);
    let body = view.stations.iter().zip(&view.top).any(|(&x, &z)| {
        let at = frame.to_screen(x, z);
        at.x >= rect.left() - 6.0 && at.x <= rect.right() + 6.0 && at.y <= rect.bottom()
    });
    body || view.surfaces.iter().any(|quad| {
        let points: Vec<Pos2> = quad.iter().map(|[x, z]| frame.to_screen(*x, *z)).collect();
        polygon_hits_rect(&points, rect)
    })
}

/// The place of a label `size` points large for the mark at station `x`, inside `clip`: as near
/// to straight above the mark as the silhouette allows. `None` when the canvas has no free place
/// of that size.
pub(super) fn place(
    frame: &Frame,
    view: &SideView,
    clip: Rect,
    x: f64,
    size: Vec2,
) -> Option<Placed> {
    let mark = frame.to_screen(x, view.at(&view.top, x));
    let base = mark.y - LEADER;
    let room = clip.shrink(2.0);
    let ups = ((base - size.y - room.top()) / STEP_UP).floor().max(0.0) as i32;
    let across = (MAX_ACROSS / STEP_ACROSS) as i32;
    let mut candidates: Vec<(f32, f32, f32)> = Vec::new();
    for up in 0..=ups {
        for side in -across..=across {
            let (dy, dx) = (up as f32 * STEP_UP, side as f32 * STEP_ACROSS);
            candidates.push((dx.abs() + 2.0 * dy, dx, dy));
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, dx, dy) in candidates {
        let rect = Rect::from_min_size(pos2(mark.x + dx - size.x / 2.0, base - dy - size.y), size);
        if !room.contains_rect(rect) || collides(frame, view, rect) {
            continue;
        }
        let end = pos2(
            mark.x.clamp(rect.left() + 6.0, rect.right() - 6.0),
            rect.bottom(),
        );
        return Some(Placed {
            rect,
            leader: [mark, end],
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::payload_editor::canvas::Window;
    use crate::views::payload_editor::side_view::test_body;

    fn frame(view: &SideView) -> (Frame, Rect) {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 360.0));
        (Frame::windowed(rect, view, Window::default()), rect)
    }

    #[test]
    fn the_label_floats_above_the_body_with_a_leader_to_the_mark() {
        let view = test_body();
        let (frame, clip) = frame(&view);
        let placed = place(&frame, &view, clip, 18.0, vec2(180.0, 20.0)).expect("a place");
        let top = frame.to_screen(18.0, 2.0);
        assert!(placed.rect.bottom() <= top.y - LEADER + 0.01, "{placed:?}");
        assert!(clip.contains_rect(placed.rect));
        assert_eq!(placed.leader[0], top);
        assert!((placed.leader[1].y - placed.rect.bottom()).abs() < 1e-3);
    }

    #[test]
    fn the_label_keeps_clear_of_a_fin_and_never_sits_over_the_body() {
        let mut view = test_body();
        // A fin from x 33 to 40 m, up to 8 m: right above the mark at 36 m.
        view.surfaces = vec![[[33.0, 2.0], [36.5, 2.0], [40.0, 8.0], [37.5, 8.0]]];
        view.tail_top_z = Some(8.0);
        let (frame, clip) = frame(&view);
        let size = vec2(200.0, 20.0);
        let placed = place(&frame, &view, clip, 36.0, size).expect("a place");
        let quad: Vec<Pos2> = view.surfaces[0]
            .iter()
            .map(|[x, z]| frame.to_screen(*x, *z))
            .collect();
        assert!(!polygon_hits_rect(&quad, placed.rect), "{placed:?}");
        for (&x, &z) in view.stations.iter().zip(&view.top) {
            let at = frame.to_screen(x, z);
            let inside = at.x > placed.rect.left() && at.x < placed.rect.right();
            assert!(
                !inside || at.y > placed.rect.bottom(),
                "body under the label"
            );
        }
    }

    #[test]
    fn a_polygon_test_tells_overlap_from_clearance() {
        let quad = [
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(10.0, 10.0),
            pos2(0.0, 10.0),
        ];
        assert!(polygon_hits_rect(
            &quad,
            Rect::from_min_size(pos2(5.0, 5.0), vec2(10.0, 10.0))
        ));
        assert!(!polygon_hits_rect(
            &quad,
            Rect::from_min_size(pos2(11.0, 0.0), vec2(5.0, 5.0))
        ));
        // A slanted quad whose bounding box overlaps the rectangle but which does not.
        let slant = [
            pos2(0.0, 0.0),
            pos2(2.0, 0.0),
            pos2(10.0, 10.0),
            pos2(8.0, 10.0),
        ];
        assert!(!polygon_hits_rect(
            &slant,
            Rect::from_min_size(pos2(0.0, 6.0), vec2(3.0, 3.0))
        ));
    }
}
