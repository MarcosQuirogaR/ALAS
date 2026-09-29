// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The detached External Tools manager and first-start disclosure.
//!
//! Shown once per installation, gated on `EXTERNAL_TOOLS_INTRO_MARKER_FILE`
//! independently of the general onboarding walkthrough's own marker: a user
//! who skips or dismisses the walkthrough must still see, once, what
//! external tools ALAS can use, their official source and licence, and
//! whether ALAS can fetch any of them automatically. Reopen the manager at
//! any time from the application menu, including while the Sandbox is active.
//!
//! Source and licence claims follow `THIRD-PARTY-NOTICES.md`,
//! `docs/downloads.md`, and linked official publisher pages.
//!
//! Automatic acquisitions reuse the existing background/atomic/cancellable
//! mechanisms. Other tools have a publisher link and an in-window configuration
//! card. No purchase or third-party installer runs inside ALAS.

use egui::{Context, RichText, ScrollArea, Ui, ViewportBuilder, ViewportCommand};

use crate::native_viewport::{show_native_viewport, viewport_id};
use crate::state::{AppState, LogKind};
use crate::views::external_tool_catalog::{ExternalToolConfig, USER_SUPPLIED_TOOLS};
use crate::views::{tr, tr_fields};

const TOOL_MANAGER_VIEWPORT: &str = "external_tools_manager";

/// Open or raise the native manager from either desktop workspace.
pub fn open_tool_manager(state: &mut AppState, ctx: &Context) {
    if state.show_tool_intro {
        ctx.send_viewport_cmd_to(viewport_id(TOOL_MANAGER_VIEWPORT), ViewportCommand::Focus);
    } else {
        state.tool_intro_selected_config = None;
    }
    state.show_tool_intro = true;
}

/// Top-bar entry point shared by the guided workspace and Sandbox.
pub fn show_menu_action(state: &mut AppState, ui: &mut Ui) {
    if ui.button(tr("External Tools")).clicked() {
        open_tool_manager(state, ui.ctx());
    }
}

/// Render the manager as its own OS window, if it is open.
pub fn show_tool_intro(state: &mut AppState, ctx: &Context) {
    if !state.show_tool_intro {
        return;
    }
    // A file picker may finish after the user returns to the overview.
    crate::views::tools_view::apply_completed_path_selection(state);
    let mut close = false;
    let response = show_native_viewport(
        ctx,
        TOOL_MANAGER_VIEWPORT,
        tr("External Tools"),
        ViewportBuilder::default()
            .with_title(tr("External Tools"))
            .with_inner_size(egui::vec2(900.0, 680.0))
            .with_min_inner_size(egui::vec2(620.0, 420.0))
            .with_resizable(true),
        |child_ctx, ui, _class| {
            if child_ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
                close = true;
            }
            ui.horizontal(|ui| {
                if let Some(config) = state.tool_intro_selected_config {
                    if ui.button(tr("All tools")).clicked() {
                        state.tool_intro_selected_config = None;
                    }
                    if config == ExternalToolConfig::All {
                        ui.heading(tr("All settings"));
                    } else if let Some(tool) =
                        USER_SUPPLIED_TOOLS.iter().find(|tool| tool.config == config)
                    {
                        ui.heading(tr(tool.name));
                    }
                } else {
                    ui.heading(tr("External tools ALAS can use"))
                        .on_hover_text(tr("Manage the external programs ALAS can use, their official acquisition pages, and your local configuration."));
                    if ui.button(tr("All settings")).clicked() {
                        select_configuration(state, ExternalToolConfig::All);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr("Close")).clicked() {
                        close = true;
                    }
                });
            });
            ui.separator();
            if state.tool_intro_selected_config == Some(ExternalToolConfig::All) {
                crate::views::tools_view::show_tools_view(state, ui);
            } else {
                ScrollArea::vertical()
                    .id_salt((
                        "external_tools_manager_scroll",
                        state.tool_intro_selected_config,
                    ))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if let Some(config) = state.tool_intro_selected_config {
                            configuration_section(state, ui, config);
                        } else {
                            ready_now_section(ui);
                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(10.0);
                            optional_download_section(state, ui);
                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(10.0);
                            user_supplied_section(state, ui);
                        }
                    });
            }
        },
    );
    if close || response.close_requested {
        state.show_tool_intro = false;
        state.tool_intro_selected_config = None;
        state.tool_intro_download_all_consent = false;
        state.tool_intro_navdata_consent = false;
        state.tool_intro_openvsp_preview_consent = false;
    }
}

fn configuration_section(state: &mut AppState, ui: &mut Ui, config: ExternalToolConfig) {
    if let Some(tool) = USER_SUPPLIED_TOOLS
        .iter()
        .find(|tool| tool.config == config)
    {
        ui.horizontal_wrapped(|ui| {
            ui.hyperlink_to(tr(tool.action), tool.url);
            if config == ExternalToolConfig::FlowUnsteady {
                ui.hyperlink_to(tr("Julia download"), "https://julialang.org/downloads/");
            }
            ui.label(
                RichText::new(tr_fields(
                    "Source: {source}. Licence: {licence}.",
                    &[("source", tr(tool.source)), ("licence", tr(tool.licence))],
                ))
                .weak(),
            );
        });
        ui.add_space(10.0);
    }
    crate::views::tools_view::show_tool_configuration(state, ui, config);
}

fn section_heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(tr(text)).strong().size(15.0));
    ui.add_space(4.0);
}

/// Group 1: tools with nothing to configure.
fn ready_now_section(ui: &mut Ui) {
    section_heading(ui, "Ready now: nothing to do");
    if cfg!(target_os = "windows") {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("Athena AVL 3.52 (Windows)")).strong())
                .on_hover_text(tr(
                    "Source: the upstream MIT AVL distribution (web.mit.edu/drela/Public/web/avl). Licence: GPL-2.0. The unchanged executable, its source archive, and the GPL notice already ship inside the Windows package: nothing to install or configure.",
                ));
            ui.hyperlink_to(tr("Official source"), "https://web.mit.edu/drela/Public/web/avl/");
        });
        ui.add_space(6.0);
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("XFOIL Orr-Sommerfeld transition data (used by MSES)")).strong())
            .on_hover_text(tr(
                "Source: Mark Drela's official XFOIL 6.99 page (web.mit.edu/drela/Public/web/xfoil). Licence: GPL-2.0-or-later. The unmodified map and archive already ship bundled with every release. MSES itself still requires your own installation: see \"You supply these\" below.",
            ));
        ui.hyperlink_to(tr("Official source"), "https://web.mit.edu/drela/Public/web/xfoil/");
    });
}

/// Group 2: the only two acquisitions this screen may offer, per this
/// task's non-negotiable boundary: everything else stays user-supplied.
fn optional_download_section(state: &mut AppState, ui: &mut Ui) {
    section_heading(ui, "Optional automatic download: off until you opt in");
    download_all_row(state, ui);
    ui.add_space(8.0);
    navdata_row(state, ui);
    if crate::openvsp_runtime_setup::install_supported() {
        ui.add_space(8.0);
        openvsp_preview_row(state, ui);
    }
}

/// One download ALAS is allowed to start on the user's behalf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Acquisition {
    Navdata,
    OpenVspPreview,
}

/// Every acquisition this screen may offer on this host, in display order.
///
/// This is the complete set of audited in-app download paths. Other tools
/// have publisher links and user-owned installations, so "download all"
/// cannot reach them.
fn offered_acquisitions() -> Vec<Acquisition> {
    let mut offered = vec![Acquisition::Navdata];
    if crate::openvsp_runtime_setup::install_supported() {
        offered.push(Acquisition::OpenVspPreview);
    }
    offered
}

/// The offered acquisitions not already running, which "download all"
/// starts. Consent is recorded per item before this is called, so the
/// per-item checkboxes always show what was agreed to.
fn pending_acquisitions(state: &AppState, offered: &[Acquisition]) -> Vec<Acquisition> {
    offered
        .iter()
        .copied()
        .filter(|item| match item {
            Acquisition::Navdata => !state.navdata_download_in_progress,
            Acquisition::OpenVspPreview => {
                !state.openvsp_runtime_setup.running && !preview_runtime_installed(state)
            }
        })
        .collect()
}

fn preview_runtime_status(state: &AppState) -> crate::openvsp_runtime_setup::PreviewRuntimeStatus {
    let destination = crate::openvsp_runtime_setup::resolve_destination(
        state.tool_preferences.openvsp_dir.as_deref(),
    );
    crate::openvsp_runtime_setup::runtime_status(destination.as_deref())
}

fn preview_runtime_installed(state: &AppState) -> bool {
    matches!(
        preview_runtime_status(state),
        crate::openvsp_runtime_setup::PreviewRuntimeStatus::Installed { .. }
    )
}

fn start_all_acquisitions(state: &mut AppState) {
    let offered = offered_acquisitions();
    for item in &offered {
        match item {
            Acquisition::Navdata => state.tool_intro_navdata_consent = true,
            Acquisition::OpenVspPreview => state.tool_intro_openvsp_preview_consent = true,
        }
    }
    for item in pending_acquisitions(state, &offered) {
        match item {
            Acquisition::Navdata => {
                let configured = navdata_dir(state);
                state.start_navdata_download(&configured);
            }
            Acquisition::OpenVspPreview => state.start_openvsp_runtime_setup(),
        }
    }
}

fn download_all_row(state: &mut AppState, ui: &mut Ui) {
    ui.label(RichText::new(tr("Download everything ALAS may fetch for you")).strong())
        .on_hover_text(tr(
            "One consent for every automated download in this group. Other tools have official publisher links and require a separate user installation or licence.",
        ));
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut state.tool_intro_download_all_consent,
            tr("I consent to every download in this group"),
        )
        .on_hover_text(tr(
            "One consent for every automated download in this group. Other tools have official publisher links and require a separate user installation or licence.",
        ));
        let offered = offered_acquisitions();
        let enabled = state.tool_intro_download_all_consent
            && !pending_acquisitions(state, &offered).is_empty();
        if ui
            .add_enabled(enabled, egui::Button::new(tr("Download all")))
            .clicked()
        {
            start_all_acquisitions(state);
        }
    });
}

fn navdata_dir(state: &AppState) -> String {
    state
        .config_values
        .get("mission")
        .and_then(|group| group.get("navdata_dir"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_owned()
}

fn navdata_row(state: &mut AppState, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("X-Plane navigation data (community mirror)")).strong())
            .on_hover_text(tr(
                "Source: a third-party GitHub mirror of X-Plane-format airway/fix data (not X-Plane's own distribution), fetched from its master branch. Licence: GPL-3.0. Used only for airway routing; ALAS falls back to great-circle routing without it. Never bundled in a release; downloaded only at your request and never redistributed.",
            ));
        ui.hyperlink_to(tr("View source"), "https://github.com/mcantsin/x-plane-navdata");
    });
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut state.tool_intro_navdata_consent,
            tr("I consent to this download"),
        )
        .on_hover_text(tr(
            "Source: a third-party GitHub mirror of X-Plane-format airway/fix data (not X-Plane's own distribution), fetched from its master branch. Licence: GPL-3.0. Used only for airway routing; ALAS falls back to great-circle routing without it. Never bundled in a release; downloaded only at your request and never redistributed.",
        ));
        let in_progress = state.navdata_download_in_progress;
        let enabled = state.tool_intro_navdata_consent && !in_progress;
        let label = if in_progress {
            tr("Downloading...")
        } else {
            tr("Download now")
        };
        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
            let configured = navdata_dir(state);
            state.start_navdata_download(&configured);
        }
        if in_progress && ui.button(tr("Cancel")).clicked() {
            state.cancel_navdata_download();
        }
    });
    if let Some((message, kind)) = &state.navdata_download_feedback {
        let text = match kind {
            LogKind::Info => RichText::new(message),
            LogKind::Warn => RichText::new(message).color(ui.visuals().warn_fg_color),
            LogKind::Error => RichText::new(message).color(ui.visuals().error_fg_color),
        };
        ui.label(text.small());
    }
}

fn openvsp_preview_row(state: &mut AppState, ui: &mut Ui) {
    ui.label(RichText::new(tr("OpenVSP native-preview runtime (Windows only)")).strong())
        .on_hover_text(tr(
            "A separate, app-local Python runtime used only for native OpenVSP screenshots after export; it never affects vspscript or VSPAERO analysis. Setup downloads hash-pinned CPython 3.13.7, the OpenVSP 3.51.2 Python bindings, and NumPy 2.3.3 from their official sources and verifies each archive's pinned checksum before extracting it. See docs/openvsp-preview.md for the exact pinned versions and licences.",
        ));
    let status = preview_runtime_status(state);
    let installed = matches!(
        status,
        crate::openvsp_runtime_setup::PreviewRuntimeStatus::Installed { .. }
    );
    ui.label(RichText::new(status.label()).weak().small());
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut state.tool_intro_openvsp_preview_consent,
            tr("I consent to this download"),
        )
        .on_hover_text(tr(
            "A separate, app-local Python runtime used only for native OpenVSP screenshots after export; it never affects vspscript or VSPAERO analysis. Setup downloads hash-pinned CPython 3.13.7, the OpenVSP 3.51.2 Python bindings, and NumPy 2.3.3 from their official sources and verifies each archive's pinned checksum before extracting it. See docs/openvsp-preview.md for the exact pinned versions and licences.",
        ));
        let running = state.openvsp_runtime_setup.running;
        let enabled = state.tool_intro_openvsp_preview_consent && !running;
        let label = if running {
            tr("Setting up...")
        } else if installed {
            tr("Reinstall preview runtime")
        } else {
            tr("Download now")
        };
        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
            state.start_openvsp_runtime_setup();
        }
        if running {
            let cancelling = state.openvsp_runtime_setup.is_cancelling();
            if ui
                .add_enabled(!cancelling, egui::Button::new(tr("Cancel")))
                .clicked()
            {
                state.cancel_openvsp_runtime_setup();
            }
        }
    });
    if let Some(error) = state.openvsp_runtime_setup.error.clone() {
        ui.label(
            RichText::new(error)
                .color(ui.visuals().error_fg_color)
                .small(),
        );
    }
    if state.openvsp_runtime_setup.running {
        ui.label(RichText::new(state.openvsp_runtime_setup.stage.clone()).weak());
    }
}

fn user_supplied_section(state: &mut AppState, ui: &mut Ui) {
    section_heading(ui, "You supply these: ALAS never downloads them");
    for tool in USER_SUPPLIED_TOOLS {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr(tool.name)).strong())
                .on_hover_text(tr_fields(
                    "Source: {source}. Licence: {licence}.",
                    &[("source", tr(tool.source)), ("licence", tr(tool.licence))],
                ));
            ui.hyperlink_to(tr(tool.action), tool.url);
            if tool.config == ExternalToolConfig::FlowUnsteady {
                ui.hyperlink_to(tr("Julia download"), "https://julialang.org/downloads/");
            }
            if ui.button(tr("Configure...")).clicked() {
                select_configuration(state, tool.config);
            }
        });
        ui.add_space(6.0);
    }
}

fn select_configuration(state: &mut AppState, config: ExternalToolConfig) {
    state.tool_intro_selected_config = Some(config);
    state.show_tool_intro = true;
}

#[cfg(test)]
mod tests {
    use super::{
        navdata_dir, offered_acquisitions, pending_acquisitions, select_configuration, Acquisition,
        USER_SUPPLIED_TOOLS,
    };
    use crate::state::AppState;
    use crate::views::external_tool_catalog::ExternalToolConfig;

    #[test]
    fn the_per_item_consent_checkboxes_default_unchecked() {
        // `AppState::default()`'s `show_tool_intro` itself depends on a real
        // per-user marker file on disk (see
        // `state::tests::first_start_marker_tests` for that gate, tested in
        // isolation against a scoped temp path), so it is not asserted here.
        // The two consent checkboxes are plain `false` defaults independent
        // of that marker, and this is the property that matters:
        // no download starts without explicit, per-item consent.
        let state = AppState::default();
        assert!(!state.tool_intro_download_all_consent);
        assert!(!state.tool_intro_navdata_consent);
        assert!(!state.tool_intro_openvsp_preview_consent);
    }

    #[test]
    fn download_all_offers_only_the_fetchable_group_and_skips_running_items() {
        let offered = offered_acquisitions();
        assert_eq!(offered.first(), Some(&Acquisition::Navdata));
        assert_eq!(
            offered.contains(&Acquisition::OpenVspPreview),
            crate::openvsp_runtime_setup::install_supported()
        );

        let mut state = AppState::default();
        state.tool_preferences.openvsp_dir = Some(
            std::env::current_exe()
                .expect("test executable path")
                .display()
                .to_string(),
        );
        assert_eq!(pending_acquisitions(&state, &offered), offered);
        state.navdata_download_in_progress = true;
        assert!(!pending_acquisitions(&state, &offered).contains(&Acquisition::Navdata));
    }

    #[test]
    fn configure_stays_in_the_manager_instead_of_navigating_the_main_app() {
        let mut state = AppState {
            show_tool_intro: true,
            active_page: "inputs".to_owned(),
            ..Default::default()
        };
        select_configuration(&mut state, ExternalToolConfig::Mses);
        assert_eq!(state.active_page, "inputs");
        assert!(state.show_tool_intro);
        assert_eq!(
            state.tool_intro_selected_config,
            Some(ExternalToolConfig::Mses)
        );
    }

    #[test]
    fn raising_an_open_manager_preserves_the_selected_tool() {
        let mut state = AppState {
            show_tool_intro: true,
            tool_intro_selected_config: Some(ExternalToolConfig::Mses),
            ..Default::default()
        };
        super::open_tool_manager(&mut state, &egui::Context::default());
        assert_eq!(
            state.tool_intro_selected_config,
            Some(ExternalToolConfig::Mses)
        );
    }

    #[test]
    fn clicking_configure_selects_a_card_without_closing_the_manager() {
        let mut state = AppState {
            show_tool_intro: true,
            active_page: "inputs".to_owned(),
            ..Default::default()
        };
        let context = egui::Context::default();
        let mut button_center = egui::Pos2::ZERO;
        for frame in 0..4 {
            let events = match frame {
                1 => vec![egui::Event::PointerMoved(button_center)],
                2 | 3 => vec![
                    egui::Event::PointerMoved(button_center),
                    egui::Event::PointerButton {
                        pos: button_center,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 2,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                _ => Vec::new(),
            };
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(960.0, 720.0),
                    )),
                    events,
                    time: Some(frame as f64 * 0.1),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        super::user_supplied_section(&mut state, ui);
                    });
                },
            );
            if frame == 0 {
                let label = super::tr("Configure...");
                button_center = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => {
                            Some(text.pos + text.galley.size() * 0.5)
                        }
                        _ => None,
                    })
                    .expect("first Configure button painted");
            }
        }
        assert_eq!(state.active_page, "inputs");
        assert!(state.show_tool_intro);
        assert_eq!(
            state.tool_intro_selected_config,
            Some(ExternalToolConfig::Mses)
        );
    }

    #[test]
    fn every_configuration_card_renders_in_the_detached_window_fallback() {
        let mut state = AppState {
            show_tool_intro: true,
            ..Default::default()
        };
        let context = egui::Context::default();
        for config in std::iter::once(None)
            .chain(std::iter::once(Some(ExternalToolConfig::All)))
            .chain(USER_SUPPLIED_TOOLS.iter().map(|tool| Some(tool.config)))
        {
            state.tool_intro_selected_config = config;
            let _ = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ctx| super::show_tool_intro(&mut state, ctx),
            );
            assert!(
                state.show_tool_intro,
                "window closed while showing {config:?}"
            );
        }
    }

    #[test]
    fn every_user_supplied_tool_is_disclosed_as_never_downloaded() {
        // The whole point of group 3 is that none of these ever gets an
        // acquisition action; the boundary is enforced by omission (no
        // button-building code exists for them beyond "Configure..."), and
        // this test guards the one thing that is a plain data check: the
        // nine tools named in this task's non-negotiable boundary are all
        // present and none of them is empty/placeholder text.
        let names: Vec<&str> = USER_SUPPLIED_TOOLS.iter().map(|tool| tool.name).collect();
        for expected in [
            "MSES",
            "MSC Nastran",
            "MSC Patran",
            "NASTRAN-95",
            "OpenVSP",
            "OpenFOAM",
            "Gmsh",
            "ParaView",
            "FLOWUnsteady",
        ] {
            assert!(
                names.iter().any(|name| name.contains(expected)),
                "expected a disclosed row naming {expected}, got {names:?}"
            );
        }
        for tool in USER_SUPPLIED_TOOLS {
            assert!(!tool.source.is_empty());
            assert!(!tool.licence.is_empty());
            assert!(tool.url.starts_with("https://"));
            assert!(!tool.action.is_empty());
        }
    }

    #[test]
    fn navdata_dir_reads_the_configured_mission_navdata_directory() {
        let mut state = AppState::default();
        state.config_values["mission"]["navdata_dir"] =
            serde_json::Value::String("C:\\navdata-test".to_owned());
        assert_eq!(navdata_dir(&state), "C:\\navdata-test");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn the_openvsp_preview_row_is_offered_on_64_bit_windows() {
        assert!(crate::openvsp_runtime_setup::install_supported());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn the_openvsp_preview_row_is_hidden_off_windows() {
        // `optional_download_section` only calls `openvsp_preview_row` when
        // `install_supported()` is true; off Windows (or on a 32-bit host)
        // it stays false, so the row is never built at all rather than
        // shown disabled.
        assert!(!crate::openvsp_runtime_setup::install_supported());
    }
}
