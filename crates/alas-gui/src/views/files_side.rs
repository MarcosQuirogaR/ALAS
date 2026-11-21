// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The left navigation pane of the Files explorer: quick-access places with
//! friendly names, then a lazily listed tree of the data folder.

use std::path::{Path, PathBuf};

use egui::{Align2, Color32, Rounding, Sense, Stroke, Ui};

use super::files_model::{natural_cmp, ItemKind};
use super::files_state::LibraryUi;
use super::{tr, AppState, ObjectKind};

const ROW_HEIGHT: f32 = 26.0;
const INDENT: f32 = 14.0;
/// Where a quick-access label starts, from the row's left edge, points.
const LABEL_LEFT: f32 = 34.0;
/// Space kept right of a quick-access label (or of its badge), points.
const LABEL_RIGHT: f32 = 8.0;
/// Room the panel's own frame, divider and scroll bar take, points.
const PANEL_CHROME: f32 = 24.0;
/// The tree stops descending here.
const MAX_DEPTH: usize = 16;

/// A quick-access place.
struct Quick {
    label: &'static str,
    path: PathBuf,
    icon: ItemKind,
    badge: usize,
}

/// The folder run outputs are written to.
pub(super) fn outputs_dir(state: &AppState) -> PathBuf {
    match &state.pipeline_options.output_dir {
        Some(dir) if dir.is_absolute() => dir.clone(),
        _ => state.data_dir().join("outputs"),
    }
}

/// Every folder the explorer may show besides what lies inside them.
pub(super) fn allowed_roots(state: &AppState) -> Vec<PathBuf> {
    let data = state.data_dir();
    let mut roots = vec![
        data.clone(),
        data.join("autosave"),
        outputs_dir(state),
        data.join("addons"),
        state.tool_locator.library_root(),
        state.tool_locator.my_airports_dir(),
        state.tool_locator.my_engines_dir(),
        state.tool_locator.my_cabins_dir(),
        crate::user_files::aircraft_dir(&data),
    ];
    roots.extend(ObjectKind::ALL.map(|kind| crate::file_library::kind_dir(&data, kind)));
    let mut unique: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !unique.contains(&root) {
            unique.push(root);
        }
    }
    unique
}

fn quick_access(state: &AppState) -> Vec<Quick> {
    let data = state.data_dir();
    let kind_dir = |kind| crate::file_library::kind_dir(&data, kind);
    let recoverable = state.shell.library.autosaves.len();
    vec![
        Quick {
            label: "My aircraft",
            path: crate::user_files::aircraft_dir(&data),
            icon: ItemKind::Aircraft,
            badge: 0,
        },
        Quick {
            label: "My airports",
            path: state.tool_locator.my_airports_dir(),
            icon: ItemKind::Airport,
            badge: 0,
        },
        Quick {
            label: "My engines",
            path: state.tool_locator.my_engines_dir(),
            icon: ItemKind::Engine,
            badge: 0,
        },
        Quick {
            label: "My cabins",
            path: state.tool_locator.my_cabins_dir(),
            icon: ItemKind::Cabin,
            badge: 0,
        },
        Quick {
            label: "My airfoils",
            path: kind_dir(ObjectKind::Airfoil),
            icon: ItemKind::Airfoil,
            badge: 0,
        },
        Quick {
            label: "Mission plans",
            path: kind_dir(ObjectKind::MissionPlan),
            icon: ItemKind::Mission,
            badge: 0,
        },
        Quick {
            label: "Payload layouts",
            path: kind_dir(ObjectKind::PayloadLayout),
            icon: ItemKind::Payload,
            badge: 0,
        },
        Quick {
            label: "Runs (outputs)",
            path: outputs_dir(state),
            icon: ItemKind::RunFolder,
            badge: 0,
        },
        Quick {
            label: "Autosave / recovered work",
            path: data.join("autosave"),
            icon: ItemKind::Folder,
            badge: recoverable,
        },
        Quick {
            label: "Add-ons",
            path: data.join("addons"),
            icon: ItemKind::Folder,
            badge: 0,
        },
    ]
}

enum NavRow {
    Heading(&'static str),
    Quick(usize),
    Tree {
        path: PathBuf,
        name: String,
        depth: usize,
        open: bool,
    },
}

fn folder_children(lib: &LibraryUi, dir: &Path) -> Vec<(String, PathBuf)> {
    let Some(listing) = lib.listings.get(dir) else {
        return Vec::new();
    };
    let mut folders: Vec<(String, PathBuf)> = listing
        .entries
        .iter()
        .filter(|entry| entry.is_folder())
        .map(|entry| (entry.name.clone(), entry.path.clone()))
        .collect();
    folders.sort_by(|a, b| natural_cmp(&a.0.to_lowercase(), &b.0.to_lowercase()));
    folders
}

fn flatten_tree(lib: &LibraryUi, root: &Path, rows: &mut Vec<NavRow>, wanted: &mut Vec<PathBuf>) {
    let mut stack = vec![(root.to_path_buf(), root_label(root), 0_usize)];
    while let Some((path, name, depth)) = stack.pop() {
        let open = lib.expanded.contains(&path);
        rows.push(NavRow::Tree {
            path: path.clone(),
            name,
            depth,
            open,
        });
        if open && depth < MAX_DEPTH {
            if !lib.listings.contains_key(&path) {
                wanted.push(path.clone());
            }
            for (name, child) in folder_children(lib, &path).into_iter().rev() {
                stack.push((child, name, depth + 1));
            }
        }
    }
}

fn root_label(root: &Path) -> String {
    root.file_name().map_or_else(
        || "ALAS".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// A quick-access label for `width` points: on one line when it fits, else on
/// two lines of the small font, and only then cut with an ellipsis (the flag
/// says whether it was cut).
fn quick_label(
    painter: &egui::Painter,
    text: &str,
    font: egui::FontId,
    small: egui::FontId,
    color: Color32,
    width: f32,
) -> (std::sync::Arc<egui::Galley>, bool) {
    let width = width.max(8.0);
    let one = painter.layout_no_wrap(text.to_owned(), font.clone(), color);
    if one.size().x <= width {
        return (one, false);
    }
    // Two rows must fit the row height: a small face with tight lines.
    let face = egui::FontId::new(small.size.min(11.0), small.family);
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), face, color, width);
    for section in &mut job.sections {
        section.format.line_height = Some(12.0);
    }
    job.wrap = egui::text::TextWrapping {
        max_width: width,
        max_rows: 2,
        break_anywhere: false,
        overflow_character: Some('\u{2026}'),
    };
    let galley = painter.layout_job(job);
    let cut = galley.elided;
    (galley, cut)
}

/// The width the pane needs to show every quick-access label on one line beside
/// its badge: the default width of the panel, never below `least`.
pub(super) fn preferred_width(state: &AppState, ui: &Ui, least: f32) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let small = egui::TextStyle::Small.resolve(ui.style());
    let painter = ui.painter();
    let needed = quick_access(state)
        .iter()
        .map(|place| {
            let text = painter
                .layout_no_wrap(tr(place.label), font.clone(), Color32::WHITE)
                .size()
                .x;
            let badge = if place.badge > 0 {
                super::files_icons::badge_width(painter, place.badge, small.clone()) + 10.0
            } else {
                0.0
            };
            LABEL_LEFT + text + badge + LABEL_RIGHT + PANEL_CHROME
        })
        .fold(0.0_f32, f32::max);
    needed.ceil().max(least)
}

/// The pane's content.
pub(super) fn show_side(state: &mut AppState, ui: &mut Ui) {
    let data = state.data_dir();
    let quick = quick_access(state);
    let mut rows = vec![NavRow::Heading("Quick access")];
    rows.extend((0..quick.len()).map(NavRow::Quick));
    rows.push(NavRow::Heading("Data folder"));
    let mut wanted = Vec::new();
    flatten_tree(&state.shell.library, &data, &mut rows, &mut wanted);
    let ctx = ui.ctx().clone();
    for dir in wanted {
        let lib = &mut state.shell.library;
        let listing = lib.listings.entry(dir.clone()).or_default();
        if !listing.requested {
            listing.requested = true;
            lib.jobs.list(&ctx, dir);
        }
    }
    let current = state.shell.library.current();
    let mut go: Option<PathBuf> = None;
    let mut toggle: Option<PathBuf> = None;
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::vertical()
        .id_salt("files_nav_scroll")
        .auto_shrink([false, false])
        .show_rows(ui, ROW_HEIGHT, rows.len(), |ui, range| {
            let width = ui.available_width();
            for row in &rows[range] {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), Sense::click());
                let painter = ui.painter_at(rect);
                let font = egui::TextStyle::Body.resolve(ui.style());
                let text_color = ui.visuals().text_color();
                match row {
                    NavRow::Heading(text) => {
                        painter.text(
                            rect.left_center() + egui::vec2(8.0, 4.0),
                            Align2::LEFT_CENTER,
                            tr(text),
                            egui::TextStyle::Small.resolve(ui.style()),
                            ui.visuals().weak_text_color(),
                        );
                    }
                    NavRow::Quick(index) => {
                        let place = &quick[*index];
                        let selected = place.path == current;
                        highlight(ui, &painter, rect, selected, response.hovered());
                        let icon = egui::Rect::from_center_size(
                            rect.left_center() + egui::vec2(18.0, 0.0),
                            egui::vec2(18.0, 18.0),
                        );
                        super::files_icons::paint_place(&painter, icon, place.icon);
                        let small = egui::TextStyle::Small.resolve(ui.style());
                        let badge_room = if place.badge > 0 {
                            super::files_icons::badge_width(&painter, place.badge, small.clone())
                                + 10.0
                        } else {
                            0.0
                        };
                        let label = tr(place.label);
                        let (galley, cut) = quick_label(
                            &painter,
                            &label,
                            font.clone(),
                            small.clone(),
                            text_color,
                            width - LABEL_LEFT - LABEL_RIGHT - badge_room,
                        );
                        painter.galley(
                            rect.left_center() + egui::vec2(LABEL_LEFT, -galley.size().y * 0.5),
                            galley,
                            text_color,
                        );
                        if place.badge > 0 {
                            super::files_icons::badge(
                                &painter,
                                rect.right_center() - egui::vec2(8.0, 0.0),
                                place.badge,
                                small,
                            );
                        }
                        if response.clicked() {
                            go = Some(place.path.clone());
                        }
                        let tip = if cut {
                            format!("{label}\n{}", place.path.display())
                        } else {
                            place.path.display().to_string()
                        };
                        response.on_hover_text(tip);
                    }
                    NavRow::Tree {
                        path,
                        name,
                        depth,
                        open,
                    } => {
                        let selected = *path == current;
                        highlight(ui, &painter, rect, selected, response.hovered());
                        let left = rect.left() + 6.0 + *depth as f32 * INDENT;
                        let arrow = egui::Rect::from_center_size(
                            egui::pos2(left + 7.0, rect.center().y),
                            egui::vec2(14.0, ROW_HEIGHT),
                        );
                        paint_arrow(
                            &painter,
                            arrow.center(),
                            *open,
                            ui.visuals().weak_text_color(),
                        );
                        let icon = egui::Rect::from_center_size(
                            egui::pos2(left + 26.0, rect.center().y),
                            egui::vec2(16.0, 16.0),
                        );
                        super::files_icons::paint(&painter, icon, ItemKind::Folder);
                        let (galley, _) = super::files_icons::elided(
                            &painter,
                            name,
                            font.clone(),
                            text_color,
                            rect.right() - 8.0 - (left + 40.0),
                        );
                        painter.galley(
                            egui::pos2(left + 40.0, rect.center().y - galley.size().y * 0.5),
                            galley,
                            text_color,
                        );
                        if response.clicked() {
                            let on_arrow = response
                                .interact_pointer_pos()
                                .is_some_and(|pos| pos.x < arrow.right() + 2.0);
                            if on_arrow {
                                toggle = Some(path.clone());
                            } else {
                                go = Some(path.clone());
                            }
                        }
                        response.on_hover_text(path.display().to_string());
                    }
                }
            }
        });
    let lib = &mut state.shell.library;
    if let Some(path) = toggle {
        if !lib.expanded.remove(&path) {
            lib.expanded.insert(path);
        }
    }
    if let Some(path) = go {
        let _ = std::fs::create_dir_all(&path);
        lib.go(path);
    }
}

fn highlight(ui: &Ui, painter: &egui::Painter, rect: egui::Rect, selected: bool, hovered: bool) {
    let accent = ui.visuals().selection.bg_fill;
    if selected {
        painter.rect_filled(
            rect.shrink2(egui::vec2(3.0, 1.0)),
            Rounding::same(5.0),
            accent.gamma_multiply(0.55),
        );
    } else if hovered {
        painter.rect_filled(
            rect.shrink2(egui::vec2(3.0, 1.0)),
            Rounding::same(5.0),
            ui.visuals()
                .widgets
                .hovered
                .weak_bg_fill
                .gamma_multiply(0.7),
        );
    }
}

fn paint_arrow(painter: &egui::Painter, center: egui::Pos2, open: bool, color: Color32) {
    let stroke = Stroke::new(1.3_f32, color);
    let points = if open {
        vec![
            center + egui::vec2(-3.5, -1.5),
            center + egui::vec2(0.0, 2.0),
            center + egui::vec2(3.5, -1.5),
        ]
    } else {
        vec![
            center + egui::vec2(-1.5, -3.5),
            center + egui::vec2(2.0, 0.0),
            center + egui::vec2(-1.5, 3.5),
        ]
    };
    painter.add(egui::Shape::line(points, stroke));
}
