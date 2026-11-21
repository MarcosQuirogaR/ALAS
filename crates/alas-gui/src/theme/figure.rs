// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one figure system of the desktop GUI: how a figure drawn in a card is
//! sized and placed.
//!
//! A card figure is a report scene laid out on a canvas in scene units; its
//! text sizes are SVG points of that canvas (one point is
//! [`CSS_PIXELS_PER_POINT`] scene units). The card draws the scene at a scale,
//! UI points per scene unit, that puts the figure's smallest text at
//! [`FIGURE_TEXT`], and builds it on the card's width divided by that scale.
//! So every figure fills the card's width at its own aspect ratio, a figure
//! that is a drawing at true scale and limited by the window height stands
//! in the middle of the card, and the text of every figure follows the UI's
//! type size at any card width and any UI zoom.

use alas_report::scene::{Scene, SceneElement, CSS_PIXELS_PER_POINT};
use egui::{vec2, Align, Id, Layout, Response, Ui, Vec2};

use super::space::FIGURE_TEXT;

/// Bounds of a figure's scale, UI points per scene unit. A scene whose text
/// is all large draws smaller than 1, one with tiny text larger; outside
/// these the figure would no longer be readable as a whole.
const SCALE_RANGE: (f32, f32) = (0.6, 2.5);

/// How long the card width holds still before a figure is built anew for it,
/// s; meanwhile the last figure is drawn.
const SETTLE_S: f64 = 0.2;

/// The size of the smallest non-empty text of `scene`, in its points.
pub fn smallest_text_pt(scene: &Scene) -> Option<f64> {
    scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text {
                text, font_size, ..
            }
            | SceneElement::TextBlock {
                text, font_size, ..
            } if !text.trim().is_empty() => Some(*font_size),
            _ => None,
        })
        .filter(|size| size.is_finite() && *size > 0.0)
        .min_by(f64::total_cmp)
}

/// The scale, UI points per scene unit, that draws the smallest text of
/// `scene` at [`FIGURE_TEXT`]; 1 for a scene without text.
pub fn text_scale(scene: &Scene) -> f32 {
    smallest_text_pt(scene)
        .map_or(1.0, |points| {
            (f64::from(FIGURE_TEXT) / (points * CSS_PIXELS_PER_POINT)) as f32
        })
        .clamp(SCALE_RANGE.0, SCALE_RANGE.1)
}

/// The scale figure `key` was last drawn at, or `fallback` before its first
/// build. A card builds its figure for this scale; [`remember_scale`] keeps
/// the one the built scene asks for.
pub fn last_scale(ui: &Ui, key: impl std::hash::Hash, fallback: f32) -> f32 {
    ui.ctx()
        .data(|data| data.get_temp::<f32>(Id::new(("card_figure_scale", key))))
        .unwrap_or(fallback)
}

/// Keep the scale figure `key` asks for, for its next build.
pub fn remember_scale(ui: &Ui, key: impl std::hash::Hash, scale: f32) {
    ui.ctx()
        .data_mut(|data| data.insert_temp(Id::new(("card_figure_scale", key)), scale));
}

/// Whether a figure built for `built` needs a new build at `wanted`: the
/// scale its text asks for differs by more than a percent.
pub fn rescale_needed(built: f32, wanted: f32) -> bool {
    (wanted - built).abs() > 0.01 * built.max(f32::EPSILON)
}

/// What a card keeps between frames about the scale of a figure that lays
/// itself out for its canvas.
///
/// The card builds the figure for a scale, reads the scale its smallest text
/// asks for and builds again when the two differ. That settles when the
/// figure's smallest text does not depend on the canvas. A figure that drops
/// or adds an element with the room it has (the cycle preview draws its inset
/// of the packed stations only where the plot is free) can ask for one scale
/// on the larger canvas and another on the smaller, each sending the card to
/// the other: the card would then build the figure anew on every frame and
/// show the two layouts in turn. The memo recognises that pair and holds the
/// figure at the larger scale, whose text is never smaller than
/// [`FIGURE_TEXT`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScaleMemo {
    /// The card width and window height the memo was taken at, points.
    frame: (f32, f32),
    /// The scale the figure was last built for.
    built: Option<f32>,
    /// The last rebuild: the scale it left and the scale it went to.
    step: Option<(f32, f32)>,
    /// A figure with no scale it settles at: the scale it is held at and the
    /// scale its build at that scale asks for.
    held: Option<(f32, f32)>,
}

impl ScaleMemo {
    /// The scale to build the figure for in a card of `frame` (card width and
    /// window height, points); `fallback` before its first build.
    pub fn build_scale(&mut self, frame: (f32, f32), fallback: f32) -> f32 {
        if self.frame != frame {
            // Another card size is another layout: what was learnt about the
            // old one does not hold, the last scale is only where to start.
            *self = Self {
                frame,
                built: self.held.map(|(held, _)| held).or(self.built),
                step: None,
                held: None,
            };
        }
        self.held
            .map(|(held, _)| held)
            .or(self.built)
            .unwrap_or(fallback)
    }

    /// The figure built for `built` asks for `wanted`: the scale to build it
    /// anew for, or `None` to keep this build.
    pub fn resolve(&mut self, built: f32, wanted: f32) -> Option<f32> {
        if let Some((_, asked)) = self.held {
            if !rescale_needed(asked, wanted) {
                return None;
            }
            // The figure itself changed: measure it afresh.
            self.held = None;
            self.step = None;
        }
        if !rescale_needed(built, wanted) {
            self.built = Some(wanted);
            self.step = None;
            return None;
        }
        let bounced = self
            .step
            .is_some_and(|(from, to)| !rescale_needed(to, built) && !rescale_needed(from, wanted));
        if bounced {
            let (hold, asked) = (built.max(wanted), built.min(wanted));
            self.held = Some((hold, asked));
            self.built = Some(hold);
            self.step = None;
            return rescale_needed(built, hold).then_some(hold);
        }
        self.step = Some((built, wanted));
        self.built = Some(wanted);
        Some(wanted)
    }

    /// The scale a figure with no settled scale is held and drawn at.
    pub fn held(&self) -> Option<f32> {
        self.held.map(|(held, _)| held)
    }
}

/// The scale memo of figure `key`.
pub fn scale_memo(ui: &Ui, key: impl std::hash::Hash) -> ScaleMemo {
    ui.ctx()
        .data(|data| data.get_temp::<ScaleMemo>(Id::new(("card_figure_scale_memo", key))))
        .unwrap_or_default()
}

/// Keep the scale memo of figure `key` for the next frame.
pub fn store_scale_memo(ui: &Ui, key: impl std::hash::Hash, memo: ScaleMemo) {
    ui.ctx()
        .data_mut(|data| data.insert_temp(Id::new(("card_figure_scale_memo", key)), memo));
}

/// The card width a figure is built for: the width `ui` has once it has held
/// still for [`SETTLE_S`], until then the width it was last built for, so a
/// window drag does not rebuild the figure every frame.
pub fn settled_width(ui: &Ui, key: impl std::hash::Hash) -> f32 {
    let width = ui.available_width().floor();
    let id = Id::new(("card_figure_settled_width", key));
    let now = ui.input(|input| input.time);
    let ctx = ui.ctx();
    let (built, seen, since) = ctx
        .data(|data| data.get_temp::<(f32, f32, f64)>(id))
        .unwrap_or((width, width, now));
    let (seen, since) = if (width - seen).abs() > 0.5 {
        (width, now)
    } else {
        (seen, since)
    };
    let settled = now - since >= SETTLE_S;
    let built = if settled { seen } else { built };
    if (seen - built).abs() > 0.5 {
        let wait = (SETTLE_S - (now - since)).max(0.0);
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
    }
    ctx.data_mut(|data| data.insert_temp(id, (built, seen, since)));
    built
}

/// The size, in UI points, `scene` is drawn at in a card `available` points
/// wide: the scene at `scale`, or as wide as the card (aspect kept) when it
/// is wider than that.
pub fn figure_size(scene: &Scene, scale: f32, available: f32) -> Vec2 {
    let size = vec2(scene.width as f32, scene.height as f32) * scale;
    let fit = (available.max(1.0) / size.x.max(1.0)).min(1.0);
    size * fit
}

/// Draw a card figure: `view` (a scene view of the figure) at `size`, in
/// the middle of the card's width.
pub fn show(ui: &mut Ui, size: Vec2, view: alas_viz::SceneView<'_>) -> Response {
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        ui.add(view.desired_size(size))
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::ScaleMemo;

    /// Replay the card's frames for a figure that asks for `ask(scale)` when
    /// built for `scale`; returns the build scale of each frame and the number
    /// of builds each frame made.
    fn frames(ask: impl Fn(f32) -> f32, count: usize) -> Vec<(f32, usize)> {
        let mut memo = ScaleMemo::default();
        (0..count)
            .map(|_| {
                let built = memo.build_scale((1000.0, 800.0), 1.2);
                let rebuilt = memo.resolve(built, ask(built));
                (rebuilt.unwrap_or(built), 1 + usize::from(rebuilt.is_some()))
            })
            .collect()
    }

    #[test]
    fn a_figure_with_canvas_independent_text_settles_in_one_rebuild() {
        let run = frames(|_| 1.5, 4);
        assert_eq!(run[0], (1.5, 2));
        assert!(run[1..].iter().all(|frame| *frame == (1.5, 1)), "{run:?}");
    }

    /// The cycle preview on a card about 1000 points wide: the inset fits the
    /// larger canvas only, and its tick text is smaller than the labels that
    /// replace it on the smaller canvas.
    #[test]
    fn a_figure_whose_two_layouts_send_it_to_each_other_is_held_at_the_larger_scale() {
        let ask = |scale: f32| if scale < 1.25 { 1.368 } else { 1.163 };
        let run = frames(ask, 8);
        let settled = run
            .iter()
            .position(|frame| frame.1 == 1)
            .expect("a frame without a rebuild");
        assert!(settled <= 3, "{run:?}");
        assert!(
            run[settled..].iter().all(|frame| *frame == (1.368, 1)),
            "held at the larger scale with one build a frame: {run:?}"
        );
    }

    #[test]
    fn a_new_card_size_or_a_changed_figure_is_measured_afresh() {
        let mut memo = ScaleMemo::default();
        let ask = |scale: f32| if scale < 1.25 { 1.368 } else { 1.163 };
        for _ in 0..4 {
            let built = memo.build_scale((1000.0, 800.0), 1.2);
            let _ = memo.resolve(built, ask(built));
        }
        assert_eq!(memo.held(), Some(1.368));
        // The same card, a figure that now settles at 1.0.
        let built = memo.build_scale((1000.0, 800.0), 1.2);
        assert_eq!(memo.resolve(built, 1.0), Some(1.0));
        assert_eq!(memo.held(), None);
        let built = memo.build_scale((1000.0, 800.0), 1.2);
        assert_eq!((built, memo.resolve(built, 1.0)), (1.0, None));
        // Another card width forgets a held pair.
        for _ in 0..4 {
            let built = memo.build_scale((1000.0, 800.0), 1.2);
            let _ = memo.resolve(built, ask(built));
        }
        assert!(memo.held().is_some());
        let _ = memo.build_scale((1400.0, 800.0), 1.2);
        assert_eq!(memo.held(), None);
    }
}
