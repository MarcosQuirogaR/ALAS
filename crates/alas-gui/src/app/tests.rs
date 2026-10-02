// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::AlasApp;
use crate::state::{AppState, Language};
use crate::view_controls::{
    auto_zoom_factor, auto_zoom_for_physical_size, zoom_after_command, ZoomCommand,
};
use egui::vec2;

#[test]
fn shell_menu_and_log_strings_have_spanish_desktop_translations() {
    let catalog = alas_i18n::es::desktop_catalog();
    for key in [
        "Navigation",
        "Unpin",
        "Pin",
        "Collapse navigation to an 8 px hover rail",
        "Keep navigation open and reserve its column",
        "Open navigation",
        "Unknown page.",
        "File",
        "Path:",
        "Load configuration",
        "Save configuration",
        "Export figures (ZIP)",
        "Generate report (PDF)",
        "Manage storage...",
        "Load preset",
        "Exit",
        "View",
        "Dark",
        "Light",
        "Grey",
        "3D Live Preview",
        "Show Run Log",
        "Hide Run Log",
        "Close",
        "Reduced Animations",
        "Automatic zoom",
        "English",
        "Spanish",
        "Zoom in",
        "Zoom out",
        "Reset zoom (100%)",
        "Help",
        "Replay Walkthrough",
        "Advanced Walkthrough...",
        "Documentation",
        "About ALAS",
        "Figure archive with {count} SVG sources written to {path}.",
        "Figure archive failed: {error}",
        "Sectioned PDF report with {count} SVG sources written to {path}.",
        "PDF report failed: {error}",
    ] {
        assert!(catalog.contains_key(key), "missing shell text: {key}");
    }
}

#[test]
fn explicit_state_constructor_activates_the_selected_walkthrough_language() {
    let mut state = AppState::default();
    state.finish_walkthrough();
    state.language = Language::Es;
    let _app = AlasApp::from_state(state);

    assert_eq!(alas_i18n::get_language(), "es");
    alas_i18n::set_language(Some("en"));
}

#[test]
fn zoom_commands_stay_inside_the_readable_interface_range() {
    assert_eq!(zoom_after_command(2.2, ZoomCommand::In), 2.2);
    assert_eq!(zoom_after_command(0.75, ZoomCommand::Out), 0.75);
    assert_eq!(zoom_after_command(1.4, ZoomCommand::Reset), 1.0);
}

#[test]
fn automatic_zoom_scales_from_the_client_area_with_recoverable_bounds() {
    assert_eq!(auto_zoom_for_physical_size(vec2(1_240.0, 760.0)), 1.0);
    assert_eq!(auto_zoom_for_physical_size(vec2(620.0, 380.0)), 0.90);
    assert_eq!(auto_zoom_for_physical_size(vec2(3_840.0, 2_160.0)), 1.35);
    assert_eq!(auto_zoom_for_physical_size(vec2(1_400.0, 850.0)), 1.125);
}

#[test]
fn automatic_zoom_stays_stable_when_egui_applies_a_pending_zoom() {
    let context = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            vec2(1_400.0, 850.0),
        )),
        ..egui::RawInput::default()
    };

    let _ = context.run(input.clone(), |_| {});
    let automatic = auto_zoom_factor(&context);
    context.set_zoom_factor(automatic);
    let _ = context.run(input, |_| {});

    assert_eq!(auto_zoom_factor(&context), automatic);
}
