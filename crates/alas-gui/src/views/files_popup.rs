// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A context menu that stays inside the window: it opens at the pointer, flips
//! to open upward (or to the left) when there is no room below (or to the
//! right), and keeps a margin to the window edge. egui's own context menu is
//! only pushed back until it touches the edge.

use egui::{Context, Id, Order, Pos2, Rect, Response, Ui, Vec2};

/// Free space kept between the menu and the window edge, points.
const EDGE_MARGIN: f32 = 8.0;

#[derive(Clone, Copy)]
struct Open {
    owner: Id,
    pos: Pos2,
    /// The size the menu had on the last frame it was drawn.
    size: Vec2,
    /// The frame it opened in: a click that opens it must not close it.
    opened: u64,
}

fn slot() -> Id {
    Id::new("window_constrained_context_menu")
}

/// Where a menu of `size` asked for at `pos` is placed inside `screen`.
pub(super) fn placed(pos: Pos2, size: Vec2, screen: Rect) -> Pos2 {
    let area = screen.shrink(EDGE_MARGIN);
    let mut out = pos;
    if out.y + size.y > area.bottom() {
        out.y = pos.y - size.y;
    }
    if out.x + size.x > area.right() {
        out.x = pos.x - size.x;
    }
    pos2_clamped(out, size, area)
}

fn pos2_clamped(pos: Pos2, size: Vec2, area: Rect) -> Pos2 {
    Pos2::new(
        pos.x.min(area.right() - size.x).max(area.left()),
        pos.y.min(area.bottom() - size.y).max(area.top()),
    )
}

/// Open the menu of `response` on a secondary click and draw it while open.
/// `add` draws the entries and returns true when one was chosen (the menu
/// then closes).
pub(super) fn context_menu(response: &Response, add: impl FnOnce(&mut Ui) -> bool) {
    let ctx = &response.ctx;
    let frame = ctx.cumulative_pass_nr();
    if response.secondary_clicked() {
        if let Some(pos) = ctx.input(|input| input.pointer.interact_pos()) {
            ctx.data_mut(|data| {
                data.insert_temp(
                    slot(),
                    Open {
                        owner: response.id,
                        pos,
                        size: Vec2::ZERO,
                        opened: frame,
                    },
                );
            });
        }
    }
    let Some(mut open) = ctx.data(|data| data.get_temp::<Open>(slot())) else {
        return;
    };
    if open.owner != response.id {
        return;
    }
    let screen = ctx.screen_rect();
    let at = placed(open.pos, open.size, screen);
    let mut chosen = false;
    let first = open.size == Vec2::ZERO;
    let area = egui::Area::new(slot().with("area"))
        .order(Order::Foreground)
        .fixed_pos(at)
        .constrain(false)
        .show(ctx, |ui| {
            // Its size is unknown until it has been laid out once: that first
            // pass is not shown, so the menu never appears and then jumps.
            if first {
                ui.set_invisible();
            }
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                chosen = add(ui);
            });
        });
    let rect = area.response.rect;
    open.size = rect.size();
    ctx.data_mut(|data| data.insert_temp(slot(), open));
    if first || at != placed(open.pos, open.size, screen) {
        ctx.request_repaint();
    }
    close_if_done(
        ctx,
        rect,
        open.opened != frame && chosen_or_outside(ctx, rect, chosen),
    );
}

fn chosen_or_outside(ctx: &Context, rect: Rect, chosen: bool) -> bool {
    chosen
        || ctx.input(|input| {
            input.key_pressed(egui::Key::Escape)
                || (input.pointer.any_pressed()
                    && input
                        .pointer
                        .interact_pos()
                        .is_some_and(|pos| !rect.contains(pos)))
        })
}

fn close_if_done(ctx: &Context, _rect: Rect, done: bool) {
    if done {
        ctx.data_mut(|data| data.remove::<Open>(slot()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    #[test]
    fn a_menu_near_the_bottom_opens_upward_and_keeps_a_margin() {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 800.0));
        let at = placed(pos2(100.0, 760.0), vec2(180.0, 220.0), screen);
        assert!(
            at.y + 220.0 <= 800.0 - EDGE_MARGIN + 1e-3 && at.y >= EDGE_MARGIN,
            "{at:?}"
        );
        assert!(
            (at.y - 540.0).abs() < 1e-3,
            "flipped above the click: {at:?}"
        );
        let free = placed(pos2(100.0, 100.0), vec2(180.0, 220.0), screen);
        assert_eq!(free, pos2(100.0, 100.0));
        let corner = placed(pos2(990.0, 790.0), vec2(180.0, 220.0), screen);
        assert!(corner.x + 180.0 <= 1000.0 - EDGE_MARGIN + 1e-3);
    }
}
