// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared View-menu and detachable appearance controls.
//!
//! Keeping them outside the shell keeps the application coordinator focused on
//! panel composition while guaranteeing the menu and movable panel edit the
//! same state.

use egui::{Context, Key, Modifiers, Ui};

use crate::state::{AppState, Language};
use crate::theme::{apply_theme, AppTheme};

fn tr(text: &str) -> String {
    alas_i18n::t(Some(text), None).into_owned()
}

/// Switch the application theme and redraw every live scene that bakes
/// the palette in: the guided preview, the result figures and the sandbox
/// scene (its background and outline colours are scene content, so the
/// live preview would otherwise keep the previous theme).
pub(crate) fn switch_theme(state: &mut AppState, ctx: &Context, theme: AppTheme) {
    state.theme = theme;
    apply_theme(theme, ctx);
    state.update_preview_scene();
    state.update_result_scene();
    state.reproject_sandbox_scene();
}

/// Render controls shared by the compact View menu and movable panel.
pub(crate) fn render_view_options(
    state: &mut AppState,
    ctx: &Context,
    ui: &mut Ui,
    close_menu: bool,
) {
    for theme in [AppTheme::Dark, AppTheme::Light, AppTheme::Grey] {
        let is_current = state.theme == theme;
        if ui.selectable_label(is_current, tr(theme.name())).clicked() {
            switch_theme(state, ctx, theme);
            if close_menu {
                ui.close_menu();
            }
        }
    }
    ui.separator();
    if ui
        .selectable_label(state.preview_open, tr("3D Live Preview"))
        .clicked()
    {
        state.preview_open = !state.preview_open;
    }
    ui.separator();
    for (lang, label) in [(Language::En, "English"), (Language::Es, "Spanish")] {
        if ui
            .selectable_label(state.language == lang, tr(label))
            .clicked()
        {
            state.language = lang;
            alas_i18n::set_language(Some(lang.code()));
            if close_menu {
                ui.close_menu();
            }
        }
    }
    ui.separator();
    if ui
        .selectable_label(state.zoom_auto, tr("Automatic zoom"))
        .clicked()
    {
        state.zoom_auto = true;
        state.zoom = auto_zoom_factor(ctx);
        if close_menu {
            ui.close_menu();
        }
    }
    if ui.button(tr("Zoom in")).clicked() {
        state.zoom = zoom_after_command(state.zoom, ZoomCommand::In);
        state.zoom_auto = false;
    }
    if ui.button(tr("Zoom out")).clicked() {
        state.zoom = zoom_after_command(state.zoom, ZoomCommand::Out);
        state.zoom_auto = false;
    }
    if ui.button(tr("Reset zoom (100%)")).clicked() {
        state.zoom = zoom_after_command(state.zoom, ZoomCommand::Reset);
        state.zoom_auto = false;
        if close_menu {
            ui.close_menu();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ZoomCommand {
    In,
    Out,
    Reset,
}

pub(crate) fn zoom_after_command(current: f32, command: ZoomCommand) -> f32 {
    match command {
        ZoomCommand::In => (current + 0.1).min(2.2),
        ZoomCommand::Out => (current - 0.1).max(0.75),
        ZoomCommand::Reset => 1.0,
    }
}

/// Calculate automatic zoom from the active egui pass's physical client area.
///
/// `ViewportInfo::inner_rect` is supplied by egui-winit before egui applies a
/// pending zoom change, so pairing it with the current pixels-per-point value
/// can mix two scale factors for one frame. The active screen rectangle is
/// rescaled by egui when its zoom changes and therefore keeps both values in
/// the same coordinate system.
pub(crate) fn auto_zoom_factor(ctx: &Context) -> f32 {
    let physical_size = ctx.screen_rect().size() * ctx.pixels_per_point();
    auto_zoom_for_physical_size(physical_size)
}

pub(crate) fn auto_zoom_for_physical_size(physical_size: egui::Vec2) -> f32 {
    let requested = (physical_size.x / 1_240.0).min(physical_size.y / 760.0);
    if requested <= 0.90 {
        return 0.90;
    }
    if requested >= 1.35 {
        return 1.35;
    }
    const AUTO_ZOOM_STEP: f32 = 0.025;
    ((requested.clamp(0.90, 1.35) / AUTO_ZOOM_STEP).round() * AUTO_ZOOM_STEP).clamp(0.90, 1.35)
}

/// Apply desktop-standard zoom shortcuts before page widgets consume input.
pub(crate) fn handle_zoom_shortcuts(ctx: &Context, zoom: &mut f32, zoom_auto: &mut bool) {
    let (zoom_in, zoom_out, reset, wheel_factor) = ctx.input_mut(|input| {
        let ctrl = Modifiers::CTRL;
        let zoom_in = input.consume_key(ctrl, Key::Plus) || input.consume_key(ctrl, Key::Equals);
        let zoom_out = input.consume_key(ctrl, Key::Minus);
        let reset = input.consume_key(ctrl, Key::Num0);
        let wheel_factor = if input.modifiers.ctrl {
            input.zoom_delta()
        } else {
            1.0
        };
        (zoom_in, zoom_out, reset, wheel_factor)
    });

    if reset {
        *zoom = zoom_after_command(*zoom, ZoomCommand::Reset);
        *zoom_auto = false;
    } else {
        if zoom_in {
            *zoom = zoom_after_command(*zoom, ZoomCommand::In);
            *zoom_auto = false;
        }
        if zoom_out {
            *zoom = zoom_after_command(*zoom, ZoomCommand::Out);
            *zoom_auto = false;
        }
        if wheel_factor.is_finite() && wheel_factor > 0.0 && wheel_factor != 1.0 {
            *zoom = (*zoom * wheel_factor).clamp(0.75, 2.2);
            *zoom_auto = false;
        }
    }
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_report::scene::Color;

    #[test]
    fn switching_the_theme_redraws_the_sandbox_scene_background() {
        let mut state = AppState::default();
        assert!(state.enter_sandbox(true));
        let ctx = Context::default();
        for theme in [AppTheme::Light, AppTheme::Grey, AppTheme::Dark] {
            let revision = state.sandbox.scene_revision;
            switch_theme(&mut state, &ctx, theme);
            let (scene, _) = state.sandbox.scene.as_ref().expect("sandbox scene");
            let expected = Color::from_hex(theme.palette().bg);
            assert_eq!(scene.background, Some(expected), "{theme:?} background");
            assert!(
                state.sandbox.scene_revision > revision,
                "{theme:?} invalidates the cache"
            );
        }
    }
}
