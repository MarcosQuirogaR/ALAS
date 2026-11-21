// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Layout of the Files page's large-icons list and quick-access sidebar,
//! rendered headless at the widths of the review's windows.

use std::sync::Arc;

use super::files_list::Events;
use super::files_model::{Entry, ItemKind};
use super::*;
use crate::test_raster::{render, write_in, Shot};

fn entry(name: &str, kind: ItemKind) -> Entry {
    Entry {
        path: PathBuf::from("root").join(name),
        name: name.to_owned(),
        lower: name.to_lowercase(),
        kind,
        size: 1,
        modified: None,
    }
}

/// The listing of the review's data folder: folders, then files.
fn listing() -> Vec<Entry> {
    let folders = [
        ".agent",
        "addons",
        "alas",
        "autosave",
        "downloads",
        "external tools",
        "library",
        "outputs",
        "run",
        "sidecar-cache",
        "suave-cache",
        "toolchains",
        "tools-staging",
    ];
    let files = [
        "airfoil-cfd-environment.json",
        "alas-run-log-1790499267.txt",
        "custom-airports.json.migrated",
        "external-tools-preferences.json",
        "installed-tools-manifest.json",
        "onboarding-state.json",
        "tool-preferences.json",
        "aircraft-library-index.json",
    ];
    folders
        .iter()
        .map(|name| entry(name, ItemKind::Folder))
        .chain(files.iter().map(|name| entry(name, ItemKind::Other)))
        .collect()
}

/// The large-icons list in a list pane `width` points wide, scrolled to the end.
fn large_icons(width: f32, height: f32) -> Shot {
    let mut state = AppState::default();
    let entries = Arc::new(listing());
    let order: Arc<Vec<usize>> = Arc::new((0..entries.len()).collect());
    state.shell.library.scroll_to = Some(1.0e6);
    render(width, height, 6, 0, |ui| {
        let mut events = Events::default();
        files_grid::show_grid(
            &mut state,
            ui,
            &entries,
            &order,
            std::path::Path::new("root"),
            &mut events,
        );
        state.shell.library.scroll_to = Some(1.0e6);
    })
}

/// Scrolled to the end, the last row of cells lies wholly inside the list: its
/// labels (two lines at most, the second elided with the full name as its
/// tooltip) end above the bottom edge with padding to spare.
#[test]
fn the_last_row_of_large_icons_ends_inside_the_list_with_padding() {
    for (name, list_width) in [("1100", 516.0), ("1600", 1016.0)] {
        let height = 480.0;
        let shot = large_icons(list_width, height);
        write_in(
            &crate::test_raster::render_dir(),
            &format!("files-large-{name}.png"),
            &shot,
        );
        let last = shot
            .texts
            .iter()
            .filter(|(text, _)| !text.is_empty())
            .map(|(_, rect)| rect.bottom())
            .fold(0.0_f32, f32::max);
        assert!(
            last <= height - 6.0,
            "{name}: the last label ends at {last}, the list at {height}"
        );
    }
}

/// The sidebar `width` points wide with four recoverable autosaves.
fn sidebar(width: f32) -> (Shot, f32) {
    let mut state = AppState::default();
    state.shell.library.autosaves = (0..4)
        .map(|i| (format!("autosave-{i}.json"), "2026-10-01".to_owned()))
        .collect();
    let preferred = {
        let ctx = egui::Context::default();
        crate::theme::apply_theme(crate::theme::AppTheme::Dark, &ctx);
        let mut preferred = 0.0;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                preferred = files_side::preferred_width(&state, ui, 264.0);
            });
        });
        preferred
    };
    let width = if width <= 0.0 { preferred } else { width };
    let shot = render(width, 420.0, 4, 0, |ui| {
        files_side::show_side(&mut state, ui);
    });
    (shot, preferred)
}

/// At the sidebar's default width the "Autosave / recovered work" label is one
/// whole line beside its badge; in a narrower sidebar it wraps to two lines
/// instead of being cut.
#[test]
fn the_autosave_label_has_the_room_it_needs_beside_its_badge() {
    let (shot, preferred) = sidebar(0.0);
    write_in(
        &crate::test_raster::render_dir(),
        "files-sidebar-default.png",
        &shot,
    );
    let (_, rect) = shot
        .texts
        .iter()
        .find(|(text, _)| text == "Autosave / recovered work")
        .expect("the label is painted");
    let line = shot
        .texts
        .iter()
        .find(|(text, _)| text == "My aircraft")
        .map(|(_, rect)| rect.height())
        .expect("a one-line label is painted");
    assert!(rect.height() < line * 1.3, "one line: {rect:?}");
    assert!(
        rect.right() < preferred - 8.0 - 20.0,
        "clear of the badge: {rect:?}"
    );
    let (narrow, _) = sidebar(200.0);
    write_in(
        &crate::test_raster::render_dir(),
        "files-sidebar-narrow.png",
        &narrow,
    );
    let (_, rect) = narrow
        .texts
        .iter()
        .find(|(text, _)| text.starts_with("Autosave"))
        .expect("the label is painted");
    assert!(
        rect.height() > line * 1.05,
        "two lines in a narrow sidebar: {rect:?}"
    );
    assert!(rect.height() < 28.0, "inside its row: {rect:?}");
}
