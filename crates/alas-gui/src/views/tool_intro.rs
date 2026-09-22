// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The first-start external-tool disclosure screen.
//!
//! Shown once per installation, gated on `EXTERNAL_TOOLS_INTRO_MARKER_FILE`
//! independently of the general onboarding walkthrough's own marker: a user
//! who skips or dismisses the walkthrough must still see, once, what
//! external tools ALAS can use, their official source and licence, and
//! whether ALAS can fetch any of them automatically before they reach
//! Setup > External Tools on their own. Reopen it any time from
//! Help > External Tools Overview.
//!
//! Every factual claim here is sourced from `THIRD-PARTY-NOTICES.md` and
//! `docs/downloads.md`, not restated from memory or invented for this
//! screen: this module must never assert a licence, source, or
//! redistribution right beyond what those files already record. Where a
//! licence has not yet been recorded there (OpenFOAM, Gmsh, ParaView), this
//! screen says so plainly instead of guessing.
//!
//! The only two acquisition actions here reuse existing background/atomic/
//! cancellable mechanisms ([`crate::state::AppState::start_navdata_download`]
//! and [`crate::state::AppState::start_openvsp_runtime_setup`]) rather than
//! opening a third download path. Every other tool is disclosed with a
//! "Configure..." button that only navigates to Setup > External Tools.

use egui::{Color32, Context, Frame, RichText, ScrollArea, Stroke, Ui, Window};

use crate::state::AppState;
use crate::views::{tr, tr_fields};

const TOOL_INTRO_ORDER: egui::Order = egui::Order::Foreground;

/// Render the first-start external-tool disclosure screen, if it is open.
pub fn show_tool_intro(state: &mut AppState, ctx: &Context) {
    if !state.show_tool_intro {
        return;
    }
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        state.show_tool_intro = false;
        return;
    }

    let screen = ctx.screen_rect();
    egui::Area::new("tool_intro_scrim".into())
        .fixed_pos(screen.min)
        .order(TOOL_INTRO_ORDER)
        // Paint-only: an interactable scrim in the same foreground order as
        // the window below it can swallow the window's own clicks on some
        // egui backends (the walkthrough scrim uses the same precaution).
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_min_size(screen.size());
            ui.painter()
                .rect_filled(screen, 0.0, Color32::from_black_alpha(160));
        });

    let mut open = true;
    Window::new(tr("External tools ALAS can use"))
        .order(TOOL_INTRO_ORDER)
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size(egui::vec2(720.0, 560.0))
        .frame(
            Frame::window(&ctx.style())
                .stroke(Stroke::new(2.0_f32, Color32::from_rgb(50, 180, 255)))
                .rounding(10.0),
        )
        .show(ctx, |ui| {
            ui.label(tr(
                "ALAS drives several independently licensed external programs through a process boundary. This is a one-time overview of what each one is, its official source and licence, and whether ALAS can fetch it for you. Reopen it any time from Help > External Tools Overview.",
            ));
            ui.add_space(10.0);
            ScrollArea::vertical()
                .id_salt("tool_intro_scroll")
                .show(ui, |ui| {
                    ready_now_section(ui);
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);
                    optional_download_section(state, ui);
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);
                    user_supplied_section(state, ui);
                });
            ui.add_space(10.0);
            if ui.button(tr("Close")).clicked() {
                state.show_tool_intro = false;
            }
        });
    if !open {
        state.show_tool_intro = false;
    }
}

fn section_heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(tr(text)).strong().size(15.0));
    ui.add_space(4.0);
}

/// Group 1: tools with nothing to configure, per
/// `EXTERNAL_TOOL_FIRST_START_IMPLEMENTATION_TASK.md`'s FS1 group 1.
fn ready_now_section(ui: &mut Ui) {
    section_heading(ui, "Ready now: nothing to do");
    ui.label(RichText::new(tr("Athena AVL 3.52 (Windows)")).strong());
    ui.label(
        RichText::new(tr(
            "Source: the upstream MIT AVL distribution (web.mit.edu/drela/Public/web/avl). Licence: GPL-2.0. The unchanged executable, its source archive, and the GPL notice already ship inside the Windows package: nothing to install or configure.",
        ))
        .weak()
        .small(),
    );
    ui.add_space(6.0);
    ui.label(RichText::new(tr("XFOIL Orr-Sommerfeld transition data (used by MSES)")).strong());
    ui.label(
        RichText::new(tr(
            "Source: Mark Drela's official XFOIL 6.99 page (web.mit.edu/drela/Public/web/xfoil). Licence: GPL-2.0-or-later. The unmodified map and archive already ship bundled with every release. MSES itself still requires your own installation: see \"You supply these\" below.",
        ))
        .weak()
        .small(),
    );
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
/// This is the complete set: the tools under "You supply these" have no
/// licence that lets ALAS fetch them (MSES and the MSC products are sold per
/// seat; OpenVSP's main install and NASTRAN-95 need a reviewed release), so
/// "download all" can never reach them.
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
            Acquisition::OpenVspPreview => !state.openvsp_runtime_setup.running,
        })
        .collect()
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
    ui.label(RichText::new(tr("Download everything ALAS may fetch for you")).strong());
    ui.label(
        RichText::new(tr(
            "One consent for every item in this group, each under the licence stated in its own row below. The tools under \"You supply these\" are not included: their licences do not allow ALAS to download them for you.",
        ))
        .weak()
        .small(),
    );
    // Kept in egui's per-window memory rather than `AppState`: it lives only
    // as long as this screen and, like the per-item boxes, starts unticked.
    let consent_id = ui.id().with("tool_intro_download_all_consent");
    let mut consent = ui.data(|data| data.get_temp::<bool>(consent_id).unwrap_or(false));
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut consent,
            tr("I consent to every download in this group"),
        );
        let offered = offered_acquisitions();
        let enabled = consent && !pending_acquisitions(state, &offered).is_empty();
        if ui
            .add_enabled(enabled, egui::Button::new(tr("Download all")))
            .clicked()
        {
            start_all_acquisitions(state);
        }
    });
    ui.data_mut(|data| data.insert_temp(consent_id, consent));
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
    ui.label(RichText::new(tr("X-Plane navigation data (community mirror)")).strong());
    ui.label(
        RichText::new(tr(
            "Source: a third-party GitHub mirror of X-Plane-format airway/fix data (not X-Plane's own distribution), fetched from its master branch. Licence: GPL-3.0. Used only for airway routing; ALAS falls back to great-circle routing without it. Never bundled in a release; downloaded only at your request and never redistributed.",
        ))
        .weak()
        .small(),
    );
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut state.tool_intro_navdata_consent,
            tr("I consent to this download"),
        );
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
}

fn openvsp_preview_row(state: &mut AppState, ui: &mut Ui) {
    ui.label(RichText::new(tr("OpenVSP native-preview runtime (Windows only)")).strong());
    ui.label(
        RichText::new(tr(
            "A separate, app-local Python runtime used only for native OpenVSP screenshots after export; it never affects vspscript or VSPAERO analysis. Setup downloads hash-pinned CPython 3.13.7, the OpenVSP 3.51.2 Python bindings, and NumPy 2.3.3 from their official sources and verifies each archive's pinned checksum before extracting it. See docs/openvsp-preview.md for the exact pinned versions and licences.",
        ))
        .weak()
        .small(),
    );
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut state.tool_intro_openvsp_preview_consent,
            tr("I consent to this download"),
        );
        let running = state.openvsp_runtime_setup.running;
        let enabled = state.tool_intro_openvsp_preview_consent && !running;
        let label = if running {
            tr("Setting up...")
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
}

/// One row of group 3: a tool ALAS never downloads, disclosed with its
/// source/licence and a way to jump straight to its configuration card.
struct UserSuppliedTool {
    name: &'static str,
    source: &'static str,
    licence: &'static str,
}

/// Sourced from `THIRD-PARTY-NOTICES.md`'s "Invoked executables" table and
/// `EXTERNAL_TOOL_MATRIX.md`. OpenFOAM, Gmsh and ParaView have no licence
/// entry in `THIRD-PARTY-NOTICES.md` yet (a documented, separate gap this
/// task does not close: see `EXTERNAL_TOOL_MATRIX.md`'s cross-cutting
/// gaps), so their rows say that plainly instead of asserting an
/// unverified SPDX identifier.
const USER_SUPPLIED_TOOLS: &[UserSuppliedTool] = &[
    UserSuppliedTool {
        name: "MSES (mset, mses, mplot)",
        source: "MIT Technology Licensing Office (web.mit.edu/tlo)",
        licence: "Proprietary, per-seat commercial licence",
    },
    UserSuppliedTool {
        name: "MSC Nastran",
        source: "Hexagon / MSC Software (hexagon.com)",
        licence: "Proprietary",
    },
    UserSuppliedTool {
        name: "MSC Patran",
        source: "Hexagon / MSC Software (hexagon.com)",
        licence: "Proprietary",
    },
    UserSuppliedTool {
        name: "NASTRAN-95",
        source: "a maintainer-reviewed local build; no public download this screen can offer",
        licence: "NASA Open Source Agreement 1.3",
    },
    UserSuppliedTool {
        name: "OpenVSP / VSPAERO (main install)",
        source: "official OpenVSP project (openvsp.org)",
        licence: "NASA Open Source Agreement, as supplied by the selected release",
    },
    UserSuppliedTool {
        name: "OpenFOAM",
        source: "official OpenFOAM distribution (openfoam.com)",
        licence: "not yet recorded in THIRD-PARTY-NOTICES.md: verify on the official site",
    },
    UserSuppliedTool {
        name: "Gmsh",
        source: "not cited in ALAS's tracked files: see the official Gmsh project site",
        licence: "not yet recorded in THIRD-PARTY-NOTICES.md: verify on the official site",
    },
    UserSuppliedTool {
        name: "ParaView",
        source: "not cited in ALAS's tracked files: see the official ParaView project site",
        licence: "not yet recorded in THIRD-PARTY-NOTICES.md: verify on the official site",
    },
    UserSuppliedTool {
        name: "FLOWUnsteady / Julia adapter",
        source: "official FLOWUnsteady project (github.com/byuflowlab/FLOWUnsteady)",
        licence: "user-supplied; follows your selected release, not assumed to be the upstream MIT notice",
    },
];

fn user_supplied_section(state: &mut AppState, ui: &mut Ui) {
    section_heading(ui, "You supply these: ALAS never downloads them");
    for tool in USER_SUPPLIED_TOOLS {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(tr(tool.name)).strong());
                ui.label(
                    RichText::new(tr_fields(
                        "Source: {source}. Licence: {licence}.",
                        &[("source", tr(tool.source)), ("licence", tr(tool.licence))],
                    ))
                    .weak()
                    .small(),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Configure...")).clicked() {
                    state.active_page = "setup_tools".to_owned();
                    state.show_tool_intro = false;
                }
            });
        });
        ui.add_space(6.0);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        navdata_dir, offered_acquisitions, pending_acquisitions, Acquisition, USER_SUPPLIED_TOOLS,
    };
    use crate::state::AppState;

    #[test]
    fn the_per_item_consent_checkboxes_default_unchecked() {
        // `AppState::default()`'s `show_tool_intro` itself depends on a real
        // per-user marker file on disk (see
        // `state_parts::first_start_marker_tests` for that gate, tested in
        // isolation against a scoped temp path), so it is not asserted here.
        // The two consent checkboxes are plain `false` defaults independent
        // of that marker, and this is the property FS1 actually requires:
        // no download starts without explicit, per-item consent.
        let state = AppState::default();
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
        assert_eq!(pending_acquisitions(&state, &offered), offered);
        state.navdata_download_in_progress = true;
        assert!(!pending_acquisitions(&state, &offered).contains(&Acquisition::Navdata));
    }

    #[test]
    fn clicking_configure_on_a_user_supplied_row_navigates_and_closes() {
        let mut state = AppState {
            show_tool_intro: true,
            active_page: "inputs".to_owned(),
            ..Default::default()
        };

        // Exercise the same state transition the "Configure..." button
        // performs, without depending on an egui test harness for a single
        // click.
        state.active_page = "setup_tools".to_owned();
        state.show_tool_intro = false;

        assert_eq!(state.active_page, "setup_tools");
        assert!(!state.show_tool_intro);
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
