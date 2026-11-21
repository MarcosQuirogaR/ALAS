// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Drawing the Add-ons page: the add-ons in a list, and beside it the selected
//! add-on's detail with a Run tab (inputs, progress, outputs) and a Files tab
//! (the add-on's folder as a tree with a read-only code viewer).

use alas_addons::{AddonKind, InstalledAddon};
use egui::{vec2, Color32, RichText, ScrollArea, Ui};

use super::{files, highlight, output_view, DetailTab};
use crate::state::AppState;
use crate::views::{tr, tr_fields};

pub(super) const WARNING: Color32 = Color32::from_rgb(214, 90, 49);
/// Width of the add-on list beside the detail pane.
const LIST_WIDTH: f32 = 340.0;
/// Narrower than this the list sits above the detail instead of beside it.
const SPLIT_MIN_WIDTH: f32 = 720.0;
/// Width of the file tree beside the code viewer.
const TREE_WIDTH: f32 = 210.0;
/// Height of the tree and the code viewer.
const PANE_HEIGHT: f32 = 460.0;

/// Render the Add-ons page.
pub(crate) fn show_addons_view(state: &mut AppState, ui: &mut Ui) {
    state.ensure_addons_loaded();
    if state.addons.is_running() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    ScrollArea::vertical()
        .id_salt("addons_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.heading(tr("Add-ons"));
            ui.label(RichText::new(tr(
                "Calculators and tools you add or edit. They write to the design only when you click Apply to design.",
            ))
            .weak());
            ui.add_space(6.0);
            toolbar(state, ui);
            ui.add_space(6.0);
            for (path, problem) in &state.addons.discovery.problems {
                ui.colored_label(WARNING, format!("{}: {problem}", path.display()));
            }
            let addons = state.addons.discovery.addons.clone();
            if addons.is_empty() {
                ui.label(tr("No add-ons found in the add-ons folder."));
            }
            let selected = state
                .addons
                .selected
                .as_ref()
                .and_then(|id| addons.iter().find(|addon| &addon.id == id))
                .cloned();
            if ui.available_width() >= SPLIT_MIN_WIDTH {
                let top = ui.cursor().top();
                let mut divider_x = 0.0;
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        vec2(LIST_WIDTH, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(LIST_WIDTH);
                            list(state, ui, &addons);
                        },
                    );
                    // The divider runs the full height of the taller column.
                    divider_x = ui.cursor().left() + 4.0;
                    ui.add_space(9.0);
                    let rest = ui.available_width();
                    ui.allocate_ui_with_layout(
                        vec2(rest, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(rest);
                            if let Some(addon) = &selected {
                                detail(state, ui, addon);
                            }
                        },
                    );
                });
                let bottom = ui.min_rect().bottom();
                let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
                ui.painter().vline(divider_x, top..=bottom, stroke);
            } else {
                list(state, ui, &addons);
                ui.separator();
                if let Some(addon) = &selected {
                    detail(state, ui, addon);
                }
            }
            applied_list(state, ui);
        });
}

fn list(state: &mut AppState, ui: &mut Ui, addons: &[InstalledAddon]) {
    for addon in addons {
        let selected = state.addons.selected.as_deref() == Some(addon.id.as_str());
        // The name stays on one line: cut with an ellipsis to the row width.
        let name = {
            let font = egui::TextStyle::Button.resolve(ui.style());
            let color = ui.visuals().text_color();
            let room = ui.available_width() - 2.0 * ui.spacing().button_padding.x - 4.0;
            let fits = |text: &str| {
                ui.painter()
                    .layout_no_wrap(text.to_owned(), font.clone(), color)
                    .size()
                    .x
                    <= room
            };
            let mut shown = addon.manifest.name.clone();
            while !fits(&shown) && shown.chars().count() > 1 {
                shown.pop();
                shown = shown.trim_end().to_owned();
                if fits(&format!("{shown}\u{2026}")) {
                    shown.push('\u{2026}');
                    break;
                }
            }
            shown
        };
        let label = format!(
            "{}\nv{}  \u{b7}  {}",
            name,
            addon.manifest.version,
            addon.manifest.kind.as_str()
        );
        let running = state
            .addons
            .running
            .as_ref()
            .is_some_and(|(id, _)| id == &addon.id);
        // One row widget for every entry, selected or not: as wide as the
        // list, its text left-aligned and centred in height.
        let text = if running {
            format!("{label}  ...")
        } else {
            label
        };
        let response = crate::theme::list_row(ui, selected, text, 38.0);
        let response = response.on_hover_text(addon.manifest.name.as_str());
        if response.clicked() {
            state.addons.selected = Some(addon.id.clone());
            state.addons.preview = None;
            state.addons.status = None;
        }
    }
}

fn toolbar(state: &mut AppState, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(tr("Open add-ons folder"))
            .on_hover_text(state.addons.dir.display().to_string())
            .clicked()
        {
            let _ = std::fs::create_dir_all(&state.addons.dir);
            let path = state.addons.dir.display().to_string();
            if let Err(error) = crate::views::tools_view::open_in_file_explorer(&path, true) {
                state.addons.status = Some(error);
            }
        }
        if ui.button(tr("Reload")).clicked() {
            state.reload_addons();
        }
        let installing = crate::file_dialog::DialogTarget::AddonFolder;
        if ui
            .add_enabled(
                !state.file_dialog_pending(&installing),
                egui::Button::new(tr("Install from folder...")),
            )
            .on_hover_text(tr(
                "Copy an add-on folder (with its addon.toml) into the add-ons folder. It stays disabled until you trust it. Zip archives are not supported; unpack them first.",
            ))
            .clicked()
        {
            let data = state.data_dir();
            let dialog = crate::file_dialog::spec(
                crate::file_dialog::DialogKind::PickFolder,
                "Install add-on from folder",
                "",
                &data,
                None,
                Vec::new(),
            );
            state.begin_file_dialog(installing, dialog);
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("Python interpreter for process add-ons"));
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.addons.python_text)
                .hint_text("python")
                .desired_width(280.0),
        );
        if response.lost_focus() {
            let text = state.addons.python_text.trim().to_owned();
            state.addons.settings.python = (!text.is_empty()).then_some(text);
            state.save_addon_settings();
        }
    });
}

fn detail(state: &mut AppState, ui: &mut Ui, addon: &InstalledAddon) {
    let manifest = &addon.manifest;
    ui.label(RichText::new(&manifest.name).strong().size(16.0));
    ui.label(format!(
        "v{}   {}   {}",
        manifest.version,
        manifest.author,
        manifest.kind.as_str()
    ));
    let (summary, rest) = split_summary(&manifest.description);
    ui.label(summary);
    if !rest.is_empty() {
        egui::CollapsingHeader::new(tr("Details"))
            .id_salt(("addon_details", &addon.id))
            .show(ui, |ui| {
                // Formula lines render sub- and superscripts through the notation helper.
                for line in rest.lines().filter(|line| !line.trim().is_empty()) {
                    ui.label(crate::notation::styled(ui, RichText::new(line.trim())));
                }
            });
    }
    ui.add_space(4.0);

    let process = manifest.kind == AddonKind::Process;
    if process {
        ui.colored_label(
            WARNING,
            tr("Warning: a process add-on runs an external program with your user rights and can read or change any file you can. Enable it only if you trust its author and have read its code."),
        );
    }
    let mut enabled = state
        .addons
        .settings
        .is_enabled(&addon.id, &manifest.version, manifest.kind);
    let label = if process {
        tr("I trust this add-on and enable it (this version)")
    } else {
        tr("Enabled")
    };
    if crate::checkbox::checkbox(ui, &mut enabled, label).changed() {
        state
            .addons
            .settings
            .set_enabled(&addon.id, &manifest.version, enabled);
        state.save_addon_settings();
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        crate::theme::segmented_style(ui);
        for (tab, label) in [(DetailTab::Run, "Inputs"), (DetailTab::Files, "Files")] {
            if ui
                .add(
                    crate::theme::selectable_button(tr(label), state.addons.tab == tab)
                        .min_size(egui::vec2(84.0, crate::theme::control_height(ui))),
                )
                .clicked()
            {
                state.addons.tab = tab;
            }
        }
    });
    ui.separator();
    match state.addons.tab {
        DetailTab::Run => run_tab(state, ui, addon, enabled),
        DetailTab::Files => files_tab(state, ui, addon),
    }
}

/// First sentence of a description and the remainder after it.
fn split_summary(text: &str) -> (&str, &str) {
    let text = text.trim();
    if let Some((first, rest)) = text.split_once('\n') {
        return (first.trim_end_matches(':').trim(), rest.trim());
    }
    let at = text.match_indices(". ").map(|(at, _)| at).find(|&at| {
        text[at + 2..]
            .chars()
            .next()
            .is_some_and(char::is_uppercase)
    });
    match at {
        Some(at) => (&text[..=at], text[at + 2..].trim()),
        None => (text, ""),
    }
}

/// Unit text for display: `-` means dimensionless and shows nothing.
fn unit_text(unit: &str) -> &str {
    match unit.trim() {
        "-" => "",
        other => other,
    }
}

/// `7200` seconds as "2 h", `90` as "1 min 30 s".
pub(super) fn format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (hours, minutes, secs) = (total / 3600, total % 3600 / 60, total % 60);
    match (hours, minutes, secs) {
        (0, 0, s) => format!("{s} s"),
        (0, m, 0) => format!("{m} min"),
        (0, m, s) => format!("{m} min {s} s"),
        (h, 0, _) => format!("{h} h"),
        (h, m, _) => format!("{h} h {m} min"),
    }
}

fn run_tab(state: &mut AppState, ui: &mut Ui, addon: &InstalledAddon, enabled: bool) {
    let manifest = &addon.manifest;
    ui.label(RichText::new(tr("Inputs")).strong());
    let mut grid = crate::views::form::CellGrid::new(ui, "addon_inputs");
    for spec in &manifest.inputs {
        let key = format!("{}/{}", addon.id, spec.name);
        let text = state
            .addons
            .input_text
            .entry(key)
            .or_insert_with(|| format!("{}", spec.default));
        let caption = if spec.description.trim().is_empty() {
            spec.name.as_str()
        } else {
            spec.description.as_str()
        };
        let unit = unit_text(&spec.unit);
        let label = if unit.is_empty() {
            caption.to_owned()
        } else {
            format!("{caption} ({unit})")
        };
        grid.cell(ui, |ui, width| {
            crate::views::form::label_row(
                ui,
                crate::notation::styled(ui, RichText::new(label)),
                |_| {},
            )
            .on_hover_text(&spec.name);
            ui.add_sized(
                [width, crate::theme::control_height(ui)],
                egui::TextEdit::singleline(text).horizontal_align(egui::Align::Center),
            );
        });
    }
    grid.finish(ui);
    ui.add_space(4.0);
    let running = state.addons.is_running();
    let run_row_top = ui.cursor().top();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(enabled && !running, egui::Button::new(tr("Run")))
            .on_disabled_hover_text(if running {
                tr("An add-on is already running.")
            } else {
                tr("Enable the add-on to run it.")
            })
            .clicked()
        {
            state.run_selected_addon();
            ui.ctx().request_repaint();
        }
        if ui
            .add_enabled(running, egui::Button::new(tr("Cancel")))
            .on_hover_text(tr("Stop the add-on and everything it started."))
            .clicked()
        {
            state.cancel_running_addon();
        }
        ui.label(
            RichText::new(tr_fields(
                "Time limit {limit}",
                &[("limit", format_duration(manifest.timeout_seconds()))],
            ))
            .weak(),
        );
    });
    progress(state, ui, running);
    if let Some(status) = state.addons.status.clone() {
        ui.label(status);
    }
    output_view::show(state, ui, addon);
    // The outputs open below the Run row, often past the bottom of the page:
    // bring the run row and its outputs into view once, when a run finishes.
    if std::mem::take(&mut state.addons.scroll_to_result) {
        let rect = egui::Rect::from_x_y_ranges(
            ui.max_rect().x_range(),
            run_row_top..=ui.min_rect().bottom(),
        );
        ui.scroll_to_rect(rect, None);
    }
}

/// The streamed progress and stderr of the running add-on.
fn progress(state: &AppState, ui: &mut Ui, running: bool) {
    let view = &state.addons.run_view;
    let Some(started) = view.started else {
        return;
    };
    if !running && view.log.is_empty() && view.text.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if running {
            ui.spinner();
            ui.label(
                RichText::new(tr_fields(
                    "Elapsed {time}",
                    &[("time", format_duration(started.elapsed().as_secs_f64()))],
                ))
                .weak(),
            );
        }
        match view.fraction {
            Some(fraction) => {
                ui.add(
                    egui::ProgressBar::new(fraction.clamp(0.0, 1.0) as f32)
                        .desired_width(220.0)
                        .text(view.text.clone()),
                );
            }
            None => {
                ui.label(&view.text);
            }
        }
    });
    if !view.log.is_empty() {
        ui.label(RichText::new(tr("Live output")).strong().small());
        ScrollArea::vertical()
            .id_salt("addon_live_log")
            .max_height(150.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &view.log {
                    ui.label(RichText::new(line).monospace().small());
                }
            });
    }
}

fn files_tab(state: &mut AppState, ui: &mut Ui, addon: &InstalledAddon) {
    if state.addons.browser.addon_id != addon.id {
        state.addons.browser.rescan(&addon.id, &addon.dir);
        let entry = addon.dir.join(&addon.manifest.entry);
        if entry.is_file() {
            state.addons.browser.viewed = Some(files::load_viewed(&entry));
        }
    }
    let selected = state
        .addons
        .browser
        .viewed
        .as_ref()
        .map(|viewed| viewed.path.clone());
    let mut clicked = None;
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            vec2(TREE_WIDTH, PANE_HEIGHT),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(TREE_WIDTH);
                ScrollArea::both()
                    .id_salt("addon_file_tree")
                    .max_height(PANE_HEIGHT)
                    .show(ui, |ui| {
                        tree(
                            ui,
                            &state.addons.browser.tree,
                            selected.as_deref(),
                            &mut clicked,
                        );
                    });
            },
        );
        ui.separator();
        viewer(state, ui, addon);
    });
    if let Some(path) = clicked {
        state.addons.browser.viewed = Some(files::load_viewed(&path));
    }
}

fn tree(
    ui: &mut Ui,
    nodes: &[files::TreeNode],
    selected: Option<&std::path::Path>,
    clicked: &mut Option<std::path::PathBuf>,
) {
    for node in nodes {
        match &node.children {
            Some(children) => {
                egui::CollapsingHeader::new(&node.name)
                    .id_salt(node.path.display().to_string())
                    .default_open(false)
                    .show(ui, |ui| tree(ui, children, selected, clicked));
            }
            None => {
                let is_selected = selected == Some(node.path.as_path());
                if ui.selectable_label(is_selected, &node.name).clicked() {
                    *clicked = Some(node.path.clone());
                }
            }
        }
    }
}

fn viewer(state: &mut AppState, ui: &mut Ui, addon: &InstalledAddon) {
    ui.vertical(|ui| {
        let Some(viewed) = state.addons.browser.viewed.clone() else {
            ui.label(
                RichText::new(tr(
                    "Select a file to read it. It is read-only here; edit it in your editor.",
                ))
                .weak(),
            );
            return;
        };
        ui.horizontal_wrapped(|ui| {
            let relative = viewed.path.strip_prefix(&addon.dir).unwrap_or(&viewed.path);
            ui.label(
                RichText::new(relative.display().to_string())
                    .monospace()
                    .strong(),
            );
            if ui
                .button(tr("Open in editor"))
                .on_hover_text(tr("Open this file in your system's editor."))
                .clicked()
            {
                if let Err(error) = files::open_in_editor(&viewed.path) {
                    state.addons.status = Some(error);
                }
            }
            if ui.button(tr("Open folder")).clicked() {
                let folder = viewed
                    .path
                    .parent()
                    .unwrap_or(&addon.dir)
                    .display()
                    .to_string();
                if let Err(error) = crate::views::tools_view::open_in_file_explorer(&folder, true) {
                    state.addons.status = Some(error);
                }
            }
        });
        if let Some(note) = &viewed.note {
            ui.label(RichText::new(tr(note)).weak().small());
        }
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let job = highlight::layout(
            &viewed.text,
            highlight::Language::from_path(&viewed.path),
            font,
            ui.visuals().dark_mode,
        );
        ScrollArea::both()
            .id_salt("addon_code_viewer")
            .max_height(PANE_HEIGHT)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(egui::Label::new(job).extend());
            });
    });
}

fn applied_list(state: &AppState, ui: &mut Ui) {
    if state.addons.applied().is_empty() {
        return;
    }
    ui.separator();
    ui.label(RichText::new(tr("Values set by add-ons (recorded with the next run)")).strong());
    for record in state.addons.applied() {
        ui.label(record.provenance_line());
    }
}

#[cfg(test)]
mod tests {
    use super::format_duration;

    #[test]
    fn durations_read_in_the_largest_sensible_units() {
        assert_eq!(format_duration(10.0), "10 s");
        assert_eq!(format_duration(90.0), "1 min 30 s");
        assert_eq!(format_duration(120.0), "2 min");
        assert_eq!(format_duration(7200.0), "2 h");
        assert_eq!(format_duration(9000.0), "2 h 30 min");
    }
}
