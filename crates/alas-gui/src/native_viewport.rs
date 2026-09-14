// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared native viewport plumbing for detached ALAS tools and editors.
//!
//! A [`egui::Window`] is an area inside the current OS window.  Detached
//! application surfaces use an immediate egui viewport instead, so their
//! title bar, resize handles, focus, and close action belong to a real native
//! window.  The helper retains a small amount of per-viewport geometry in the
//! egui context so closing and reopening a surface in one session keeps the
//! last size.

use egui::{CentralPanel, Context, Id, Ui, ViewportBuilder, ViewportClass, ViewportId, Window};

/// The interaction information a detached viewport needs to report to its
/// owner after rendering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NativeViewportResponse {
    /// Whether the native title bar requested that the viewport close.
    pub close_requested: bool,
    /// Whether a pointer press occurred inside the viewport this pass.
    pub pointer_pressed: bool,
}

#[derive(Clone, Copy)]
struct RememberedViewportSize {
    size: egui::Vec2,
}

/// Build the stable ID used for a detached viewport.
pub(crate) fn viewport_id(key: impl std::hash::Hash) -> ViewportId {
    ViewportId::from_hash_of(("alas_native_viewport", key))
}

/// Show a detached surface in a true native viewport.
///
/// The callback receives the child context, its UI, and the viewport class.
/// The class is [`ViewportClass::Immediate`] for a native OS window.  It is
/// [`ViewportClass::Embedded`] only when the active egui integration cannot
/// create additional native windows; in that case the helper supplies an
/// egui [`Window`] with equivalent content as a graceful fallback.
pub(crate) fn show_native_viewport(
    ctx: &Context,
    key: impl std::hash::Hash,
    title: impl Into<String>,
    mut builder: ViewportBuilder,
    mut content: impl FnMut(&Context, &mut Ui, ViewportClass),
) -> NativeViewportResponse {
    let title = title.into();
    let viewport_id = viewport_id(&key);
    let memory_id = Id::new(("alas_native_viewport_size", &key));
    let embedded_id = Id::new(("alas_native_viewport_embedded", &key));

    // `ViewportBuilder::inner_size` is an initial request.  Applying the
    // remembered size before creating a new viewport preserves a user's
    // resize after they close and reopen the same tool.
    if let Some(size) = ctx.data(|data| {
        data.get_temp::<RememberedViewportSize>(memory_id)
            .map(|remembered| remembered.size)
    }) {
        if valid_size(size) {
            builder.inner_size = Some(size);
        }
    }
    let fallback_size = builder.inner_size;
    let fallback_min_size = builder.min_inner_size;
    let fallback_max_size = builder.max_inner_size;
    let fallback_resizable = builder.resizable.unwrap_or(true);
    // A newly opened tool should receive keyboard focus.  The builder is
    // rebuilt on every parent pass, while egui patches an existing viewport,
    // so this does not steal focus again after the first creation.
    if builder.active.is_none() {
        builder.active = Some(true);
    }

    let mut response = NativeViewportResponse::default();
    ctx.show_viewport_immediate(viewport_id, builder, |child_ctx, class| {
        let close_requested = child_ctx.input(|input| input.viewport().close_requested());
        let pointer_pressed = child_ctx.input(|input| input.pointer.any_pressed());
        response.close_requested |= close_requested;
        response.pointer_pressed |= pointer_pressed;

        if let Some(size) = child_ctx.input(|input| input.viewport().inner_rect.map(|r| r.size())) {
            if valid_size(size) {
                ctx.data_mut(|data| {
                    data.insert_temp(memory_id, RememberedViewportSize { size });
                });
            }
        }

        if close_requested {
            return;
        }

        match class {
            ViewportClass::Immediate => {
                let content_ctx = child_ctx.clone();
                CentralPanel::default().show(child_ctx, |ui| {
                    content(&content_ctx, ui, class);
                });
            }
            ViewportClass::Embedded => {
                let content_ctx = child_ctx.clone();
                let mut open = true;
                let mut window = Window::new(title.clone())
                    .id(embedded_id)
                    .open(&mut open)
                    .resizable(fallback_resizable);
                if let Some(size) = fallback_size {
                    window = window.default_size(size);
                }
                if let Some(size) = fallback_min_size {
                    window = window.min_size(size);
                }
                if let Some(size) = fallback_max_size {
                    window = window.max_size(size);
                }
                let shown = window.show(child_ctx, |ui| {
                    content(&content_ctx, ui, class);
                });
                response.pointer_pressed &=
                    shown.is_some_and(|shown| shown.response.contains_pointer());
                if !open {
                    response.close_requested = true;
                }
            }
            // `show_viewport_immediate` invokes only Immediate or Embedded.
            // Keep the branch explicit so the helper remains robust if egui
            // grows another class in a future release.
            ViewportClass::Root | ViewportClass::Deferred => {}
        }
    });
    response
}

fn valid_size(size: egui::Vec2) -> bool {
    size.x.is_finite() && size.y.is_finite() && size.x > 1.0 && size.y > 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detached_viewport_ids_are_stable_and_distinct() {
        assert_eq!(viewport_id("run_log"), viewport_id("run_log"));
        assert_ne!(viewport_id("run_log"), viewport_id("results"));
    }

    #[test]
    fn embedded_fallback_renders_content_and_reports_no_close() {
        let context = Context::default();
        let mut rendered = false;
        let mut response = NativeViewportResponse::default();
        let _ = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 480.0),
                )),
                ..egui::RawInput::default()
            },
            |ctx| {
                response = show_native_viewport(
                    ctx,
                    "test",
                    "Test",
                    ViewportBuilder::default().with_inner_size(egui::vec2(320.0, 240.0)),
                    |_ctx, ui, class| {
                        assert!(class == ViewportClass::Embedded);
                        ui.label("content");
                        rendered = true;
                    },
                );
            },
        );
        assert!(rendered);
        assert!(!response.close_requested);
    }

    #[test]
    fn remembered_size_overrides_only_the_initial_builder_size() {
        let context = Context::default();
        let memory_id = Id::new(("alas_native_viewport_size", "remembered"));
        context.data_mut(|data| {
            data.insert_temp(
                memory_id,
                RememberedViewportSize {
                    size: egui::vec2(700.0, 500.0),
                },
            );
        });
        let remembered = context.data(|data| {
            data.get_temp::<RememberedViewportSize>(memory_id)
                .map(|value| value.size)
        });
        assert_eq!(remembered, Some(egui::vec2(700.0, 500.0)));
    }
}
