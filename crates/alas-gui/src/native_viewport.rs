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

use egui::{
    CentralPanel, Context, Id, Pos2, Rect, Ui, Vec2, ViewportBuilder, ViewportClass,
    ViewportCommand, ViewportId, Window,
};

const INITIAL_SIZE_FRACTION: f32 = 0.6;
const POSITION_EPSILON: f32 = 1.0;

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

#[derive(Clone, Copy, Debug, PartialEq)]
struct InitialViewportGeometry {
    size: Vec2,
    native_position: Option<Pos2>,
    embedded_position: Option<Pos2>,
    root_outer_rect: Option<Rect>,
    root_native_pixels_per_point: Option<f32>,
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
    builder: ViewportBuilder,
    content: impl FnMut(&Context, &mut Ui, ViewportClass),
) -> NativeViewportResponse {
    show_native_viewport_with_size_policy(ctx, key, title, builder, false, content)
}

/// Show a compact detached surface at the size requested by its builder.
///
/// Most ALAS workspaces intentionally occupy a fraction of the main window.
/// Small editors instead need a content-sized initial window; scaling those
/// to the parent leaves a large unused panel below their controls.
pub(crate) fn show_compact_native_viewport(
    ctx: &Context,
    key: impl std::hash::Hash,
    title: impl Into<String>,
    builder: ViewportBuilder,
    content: impl FnMut(&Context, &mut Ui, ViewportClass),
) -> NativeViewportResponse {
    show_native_viewport_with_size_policy(ctx, key, title, builder, true, content)
}

fn show_native_viewport_with_size_policy(
    ctx: &Context,
    key: impl std::hash::Hash,
    title: impl Into<String>,
    mut builder: ViewportBuilder,
    prefer_requested_size: bool,
    mut content: impl FnMut(&Context, &mut Ui, ViewportClass),
) -> NativeViewportResponse {
    let title = title.into();
    let viewport_id = viewport_id(&key);
    let memory_id = Id::new(("alas_native_viewport_size", &key));
    let placement_memory_id = Id::new(("alas_native_viewport_initial_position", &key));
    let embedded_id = Id::new(("alas_native_viewport_embedded", &key));

    // `ViewportBuilder::inner_size` is an initial request.  Applying the
    // remembered size before creating a new viewport preserves a user's
    // resize after they close and reopen the same tool.
    let remembered_size = ctx.data(|data| {
        data.get_temp::<RememberedViewportSize>(memory_id)
            .map(|remembered| remembered.size)
            .filter(|&size| valid_size(size))
    });
    if let Some(size) = remembered_size {
        builder.inner_size = Some(size);
    }

    // Viewport coordinates are egui points throughout this calculation.  The
    // root viewport is used deliberately: when this helper is reached from an
    // immediate child, the active input viewport is no longer the ALAS window.
    // Missing first-frame metrics leave the caller's request intact; a later
    // frame can still supply the initial geometry.
    let geometry = initial_viewport_geometry(
        ctx,
        remembered_size,
        builder.inner_size,
        prefer_requested_size,
    );
    if remembered_size.is_none() {
        if let Some(size) = geometry.map(|geometry| geometry.size) {
            builder.inner_size = Some(size);
        }
    }
    // A viewport builder is reconstructed on every parent pass. Only send
    // creation-time geometry while the child is absent. Waiting for
    // `outer_rect` is insufficient: native backends can briefly report no
    // rectangle while a window is being dragged or moved between monitors,
    // and re-sending the position during that interval snaps the window back
    // and produces the characteristic analysis-time flicker.
    let child_exists = ctx.input(|input| input.raw.viewports.contains_key(&viewport_id));
    if !child_exists {
        // A stable viewport ID can be reused after the native child closes.
        // Treat that as a new placement cycle so a reopened tool is centred
        // on the current root window again.
        ctx.data_mut(|data| data.remove::<bool>(placement_memory_id));
        if let Some(position) = geometry.and_then(|geometry| geometry.native_position) {
            builder.position = Some(position);
        }
    }
    let initial_placement_pending =
        !ctx.data(|data| data.get_temp::<bool>(placement_memory_id).unwrap_or(false));
    let fallback_size = builder.inner_size;
    let fallback_min_size = builder.min_inner_size;
    let fallback_max_size = builder.max_inner_size;
    let fallback_resizable = builder.resizable.unwrap_or(true);
    // A newly opened tool should receive keyboard focus. Applying `active` to
    // an existing viewport asks the native backend to activate it on every
    // parent repaint, which steals focus while the user is dragging another
    // detached window. Restrict it to creation as well.
    if !child_exists && builder.active.is_none() {
        builder.active = Some(true);
    }

    let mut response = NativeViewportResponse::default();
    ctx.show_viewport_immediate(viewport_id, builder, |child_ctx, class| {
        let close_requested = child_ctx.input(|input| input.viewport().close_requested());
        let pointer_pressed = child_ctx.input(|input| input.pointer.any_pressed());
        response.close_requested |= close_requested;
        response.pointer_pressed |= pointer_pressed;

        if matches!(class, ViewportClass::Immediate) {
            if initial_placement_pending {
                if let Some(position) =
                    geometry.and_then(|geometry| corrected_native_position(geometry, child_ctx))
                {
                    let current_position =
                        child_ctx.input(|input| input.viewport().outer_rect.map(|rect| rect.min));
                    if current_position
                        .is_none_or(|current| !positions_are_close(current, position))
                    {
                        // `ViewportBuilder::position` is converted with the
                        // primary monitor's scale while the window is being
                        // created. Once the child reports its actual monitor,
                        // issue one logical-point command using that child's
                        // scale so a differently scaled secondary monitor
                        // still receives the intended desktop position.
                        child_ctx.send_viewport_cmd(ViewportCommand::OuterPosition(position));
                    }
                    ctx.data_mut(|data| data.insert_temp(placement_memory_id, true));
                }
            }
            if let Some(size) =
                child_ctx.input(|input| input.viewport().inner_rect.map(|r| r.size()))
            {
                if valid_size(size) {
                    ctx.data_mut(|data| {
                        data.insert_temp(memory_id, RememberedViewportSize { size });
                    });
                }
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
                if let Some(position) = geometry.and_then(|geometry| geometry.embedded_position) {
                    window = window.default_pos(position);
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

fn initial_viewport_geometry(
    ctx: &Context,
    remembered_size: Option<Vec2>,
    fallback_size: Option<Vec2>,
    prefer_requested_size: bool,
) -> Option<InitialViewportGeometry> {
    let (root_viewport, root_screen_rect) = ctx.input(|input| {
        (
            input.raw.viewports.get(&ViewportId::ROOT).cloned(),
            input.raw.screen_rect,
        )
    });

    let root_outer_rect = root_viewport
        .as_ref()
        .and_then(|viewport| viewport.outer_rect.filter(|&rect| valid_rect(rect)));
    let root_monitor_size = root_viewport
        .as_ref()
        .and_then(|viewport| viewport.monitor_size.filter(|&size| valid_size(size)));
    let root_native_pixels_per_point = root_viewport
        .as_ref()
        .and_then(|viewport| viewport.native_pixels_per_point)
        .filter(|scale| scale.is_finite() && *scale > 0.0);
    let requested_size = fallback_size.filter(|&size| valid_size(size));
    let parent_fraction = root_outer_rect
        .map(|rect| rect.size() * INITIAL_SIZE_FRACTION)
        .filter(|&size| valid_size(size));
    let size = remembered_size
        .filter(|&size| valid_size(size))
        .or_else(|| prefer_requested_size.then_some(requested_size).flatten())
        .or(parent_fraction)
        .or(requested_size)?;

    // `outer_rect` is already in the desktop's logical-point coordinate
    // system, so centering from it places a detached child over the main ALAS
    // window. A monitor-size center is only a first-frame fallback when the
    // integration has not reported the root window's absolute origin yet.
    let native_position = root_outer_rect
        .map(|rect| centered_position_in_rect(rect, size))
        .or_else(|| {
            root_monitor_size.map(|monitor_size| centered_position_in_size(monitor_size, size))
        });

    // Embedded viewports are egui Windows inside the root content area.  Their
    // position therefore uses the root screen rectangle, rather than the
    // monitor-relative native position above.
    let embedded_position = root_screen_rect
        .filter(|&rect| valid_rect(rect))
        .or_else(|| {
            root_viewport
                .as_ref()
                .and_then(|viewport| viewport.inner_rect)
        })
        .filter(|&rect| valid_rect(rect))
        .map(|screen_rect| centered_position_in_rect(screen_rect, size));

    Some(InitialViewportGeometry {
        size,
        native_position,
        embedded_position,
        root_outer_rect,
        root_native_pixels_per_point,
    })
}

fn corrected_native_position(
    geometry: InitialViewportGeometry,
    child_ctx: &Context,
) -> Option<Pos2> {
    let root_outer_rect = geometry.root_outer_rect?;
    let root_scale = geometry.root_native_pixels_per_point?;
    let child_scale = child_ctx
        .input(|input| input.viewport().native_pixels_per_point)
        .filter(|scale| scale.is_finite() && *scale > 0.0)?;

    Some(centered_position_for_scales(
        root_outer_rect,
        geometry.size,
        root_scale,
        child_scale,
    ))
}

fn centered_position_for_scales(
    root_outer_rect: Rect,
    child_size: Vec2,
    root_native_pixels_per_point: f32,
    child_native_pixels_per_point: f32,
) -> Pos2 {
    let root_center_in_physical_pixels =
        root_outer_rect.center().to_vec2() * root_native_pixels_per_point;
    Pos2::new(
        root_center_in_physical_pixels.x / child_native_pixels_per_point - child_size.x * 0.5,
        root_center_in_physical_pixels.y / child_native_pixels_per_point - child_size.y * 0.5,
    )
}

fn positions_are_close(first: Pos2, second: Pos2) -> bool {
    (first - second).length() <= POSITION_EPSILON
}

fn centered_position_in_size(container: Vec2, size: Vec2) -> Pos2 {
    Pos2::new(
        ((container.x - size.x) * 0.5).max(0.0),
        ((container.y - size.y) * 0.5).max(0.0),
    )
}

fn centered_position_in_rect(rect: Rect, size: Vec2) -> Pos2 {
    rect.min
        + Vec2::new(
            ((rect.width() - size.x) * 0.5).max(0.0),
            ((rect.height() - size.y) * 0.5).max(0.0),
        )
}

fn valid_rect(rect: Rect) -> bool {
    rect.min.x.is_finite()
        && rect.min.y.is_finite()
        && rect.max.x.is_finite()
        && rect.max.y.is_finite()
        && valid_size(rect.size())
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

    #[test]
    fn initial_geometry_scales_root_window_and_centers_on_parent_window() {
        let context = Context::default();
        let mut geometry = None;
        let _ = context.run(
            root_input(
                Some(Rect::from_min_size(
                    Pos2::new(120.0, 80.0),
                    egui::vec2(1280.0, 800.0),
                )),
                Some(egui::vec2(2560.0, 1440.0)),
                Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 800.0))),
            ),
            |ctx| {
                geometry =
                    initial_viewport_geometry(ctx, None, Some(egui::vec2(320.0, 240.0)), false);
            },
        );

        let geometry = geometry.expect("valid root metrics should produce initial geometry");
        assert_vec2_close(geometry.size, egui::vec2(768.0, 480.0));
        assert_pos2_close(geometry.native_position, Pos2::new(376.0, 240.0));
        assert_pos2_close(geometry.embedded_position, Pos2::new(256.0, 160.0));
    }

    #[test]
    fn remembered_size_wins_but_is_centered_using_current_parent_points() {
        let context = Context::default();
        let mut geometry = None;
        let _ = context.run(
            root_input(
                Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 800.0))),
                Some(egui::vec2(2560.0, 1440.0)),
                None,
            ),
            |ctx| {
                geometry = initial_viewport_geometry(
                    ctx,
                    Some(egui::vec2(500.0, 300.0)),
                    Some(egui::vec2(320.0, 240.0)),
                    false,
                );
            },
        );

        let geometry = geometry.expect("remembered size should produce initial geometry");
        assert_eq!(geometry.size, egui::vec2(500.0, 300.0));
        assert_pos2_close(geometry.native_position, Pos2::new(390.0, 250.0));
    }

    #[test]
    fn missing_root_metrics_keep_fallback_size_and_do_not_guess_position() {
        let context = Context::default();
        let mut geometry = None;
        let _ = context.run(egui::RawInput::default(), |ctx| {
            geometry = initial_viewport_geometry(ctx, None, Some(egui::vec2(320.0, 240.0)), false);
        });

        let geometry = geometry.expect("the caller's valid fallback size remains usable");
        assert_eq!(geometry.size, egui::vec2(320.0, 240.0));
        assert_eq!(geometry.native_position, None);
        assert_eq!(geometry.embedded_position, None);
    }

    #[test]
    fn compact_viewport_keeps_its_requested_size_inside_a_large_parent() {
        let context = Context::default();
        let mut geometry = None;
        let _ = context.run(
            root_input(
                Some(Rect::from_min_size(
                    Pos2::new(120.0, 80.0),
                    egui::vec2(1280.0, 800.0),
                )),
                Some(egui::vec2(2560.0, 1440.0)),
                Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 800.0))),
            ),
            |ctx| {
                geometry =
                    initial_viewport_geometry(ctx, None, Some(egui::vec2(780.0, 520.0)), true);
            },
        );

        let geometry = geometry.expect("compact viewport geometry");
        assert_eq!(geometry.size, egui::vec2(780.0, 520.0));
        assert_pos2_close(geometry.native_position, Pos2::new(370.0, 220.0));
        assert_pos2_close(geometry.embedded_position, Pos2::new(250.0, 140.0));
    }

    #[test]
    fn cross_monitor_position_uses_the_child_monitor_scale() {
        let root = Rect::from_min_size(Pos2::new(1000.0, 100.0), egui::vec2(1000.0, 800.0));
        let child_size = egui::vec2(500.0, 400.0);

        // Equal-DPI monitors preserve the existing centre calculation.
        assert_pos2_close(
            Some(centered_position_for_scales(root, child_size, 1.5, 1.5)),
            Pos2::new(1250.0, 300.0),
        );

        // If the root is on a 150% monitor and the child is created on a
        // 100% monitor, the logical position must be reprojected after the
        // child reports its actual monitor. The desktop centre is at
        // (2250, 750) physical pixels, so the child begins at (2000, 550)
        // logical points on its 100% monitor.
        assert_pos2_close(
            Some(centered_position_for_scales(root, child_size, 1.5, 1.0)),
            Pos2::new(2000.0, 550.0),
        );
    }

    #[test]
    fn embedded_fallback_does_not_remember_the_root_window_size() {
        let context = Context::default();
        context.set_embed_viewports(true);
        let memory_id = Id::new(("alas_native_viewport_size", "embedded"));
        let mut rendered = false;
        let _ = context.run(
            root_input(
                None,
                None,
                Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(640.0, 480.0))),
            ),
            |ctx| {
                let _ = show_native_viewport(
                    ctx,
                    "embedded",
                    "Embedded",
                    ViewportBuilder::default().with_inner_size(egui::vec2(320.0, 240.0)),
                    |_ctx, ui, class| {
                        assert!(matches!(class, ViewportClass::Embedded));
                        ui.label("content");
                        rendered = true;
                    },
                );
            },
        );

        assert!(rendered);
        assert!(
            context.data(|data| { data.get_temp::<RememberedViewportSize>(memory_id).is_none() })
        );
    }

    fn root_input(
        outer_rect: Option<Rect>,
        monitor_size: Option<Vec2>,
        screen_rect: Option<Rect>,
    ) -> egui::RawInput {
        let mut input = egui::RawInput {
            screen_rect,
            ..egui::RawInput::default()
        };
        let root = input
            .viewports
            .get_mut(&ViewportId::ROOT)
            .expect("RawInput::default includes the root viewport");
        root.outer_rect = outer_rect;
        root.inner_rect = screen_rect;
        root.monitor_size = monitor_size;
        input
    }

    fn assert_vec2_close(actual: Vec2, expected: Vec2) {
        assert!(
            (actual - expected).length() < 0.01,
            "{actual:?} != {expected:?}"
        );
    }

    fn assert_pos2_close(actual: Option<Pos2>, expected: Pos2) {
        let actual = actual.expect("expected a centered position");
        assert_vec2_close(actual.to_vec2(), expected.to_vec2());
    }
}
