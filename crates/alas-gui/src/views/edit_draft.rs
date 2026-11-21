// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One undo step per pointer gesture.
//!
//! A drag on a value or on a canvas changes it on every frame, and every
//! committed change is an undo step. [`begin`] and [`finish`] bracket an
//! editor's frame: while the pointer is held the edited copy is kept in the
//! egui context and the case is not touched, and when the pointer is released
//! the final copy is committed once.

use egui::{Context, Id};

/// A draft and the committed value it was made from.
#[derive(Clone)]
struct Draft<T> {
    base: T,
    edited: T,
    /// The frame it was stored in: a draft not taken up by the next frame (the
    /// page was left mid-gesture) is stale and dropped.
    frame: u64,
}

/// The copy to edit this frame: the held draft when one exists for the
/// current `committed` value (it carries on through the release frame, whose
/// last edit is the one to commit), else `committed`.
pub(crate) fn begin<T: Clone + PartialEq + Send + Sync + 'static>(
    ctx: &Context,
    id: Id,
    committed: T,
) -> T {
    match ctx.data(|data| data.get_temp::<Draft<T>>(id)) {
        Some(draft)
            if draft.base == committed
                && ctx.cumulative_pass_nr().saturating_sub(draft.frame) <= 1 =>
        {
            draft.edited
        }
        _ => committed,
    }
}

/// What to commit after the frame's edits: `Some(edited)` when it differs from
/// `committed` and the pointer is up, `None` otherwise (an unchanged copy
/// clears the draft; a changed one is kept while the pointer is held).
pub(crate) fn finish<T: Clone + PartialEq + Send + Sync + 'static>(
    ctx: &Context,
    id: Id,
    committed: &T,
    edited: T,
) -> Option<T> {
    if &edited == committed {
        ctx.data_mut(|data| data.remove::<Draft<T>>(id));
        return None;
    }
    if ctx.input(|input| input.pointer.any_down()) {
        let draft = Draft {
            base: committed.clone(),
            edited,
            frame: ctx.cumulative_pass_nr(),
        };
        ctx.data_mut(|data| data.insert_temp(id, draft));
        return None;
    }
    ctx.data_mut(|data| data.remove::<Draft<T>>(id));
    Some(edited)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ctx: &Context, down: bool, run: impl FnOnce(&Context)) {
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::PointerButton {
            pos: egui::pos2(5.0, 5.0),
            button: egui::PointerButton::Primary,
            pressed: down,
            modifiers: egui::Modifiers::NONE,
        });
        let mut run = Some(run);
        let _ = ctx.run(raw, |ctx| {
            if let Some(run) = run.take() {
                run(ctx);
            }
        });
    }

    #[test]
    fn a_draft_the_next_frame_did_not_take_up_is_dropped() {
        let ctx = Context::default();
        let id = Id::new("abandoned");
        frame(&ctx, true, |ctx| {
            assert_eq!(finish(ctx, id, &1, 2), None);
        });
        // The page was not drawn on the release frame, nor the one after.
        frame(&ctx, false, |_| {});
        frame(&ctx, false, |_| {});
        frame(&ctx, false, |ctx| {
            assert_eq!(begin(ctx, id, 1), 1, "an abandoned draft is not revived");
        });
    }

    #[test]
    fn a_draft_made_from_an_older_value_is_dropped() {
        let ctx = Context::default();
        let id = Id::new("stale");
        frame(&ctx, true, |ctx| {
            assert_eq!(finish(ctx, id, &1, 2), None);
        });
        frame(&ctx, false, |ctx| {
            // The committed value changed meanwhile (an undo, a file open).
            assert_eq!(begin(ctx, id, 7), 7);
        });
    }

    #[test]
    fn a_held_pointer_keeps_a_draft_and_the_release_commits_it_once() {
        let ctx = Context::default();
        let id = Id::new("draft");
        frame(&ctx, true, |ctx| {
            assert_eq!(finish(ctx, id, &1, 2), None, "held: no commit");
        });
        frame(&ctx, true, |ctx| {
            assert_eq!(begin(ctx, id, 1), 2, "the draft carries on");
            assert_eq!(finish(ctx, id, &1, 3), None);
        });
        frame(&ctx, false, |ctx| {
            assert_eq!(
                begin(ctx, id, 1),
                3,
                "the release frame still edits the draft"
            );
            assert_eq!(finish(ctx, id, &1, 3), Some(3));
        });
        frame(&ctx, false, |ctx| {
            assert_eq!(finish(ctx, id, &3, 3), None);
        });
    }
}
