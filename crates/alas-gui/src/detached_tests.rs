// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Detachable windows are real OS windows.
//!
//! Every surface a user can drag out of the main window goes through
//! `native_viewport`, which opens an egui viewport (a native window) and falls
//! back to an egui window only where the platform cannot create one. A bare
//! `egui::Window` is allowed only for the surfaces listed here: modal prompts,
//! the first-run tour anchored to the interface, and debug diagnostics.

use std::path::{Path, PathBuf};

use crate::native_viewport::{capture, viewport_id};
use crate::state::AppState;

/// The in-window surfaces (`Window::new` or `Area::new` calls), per file and
/// with why. The count is exact: a new surface of either kind fails the guard
/// until it is listed here, which is the moment to ask whether it should be a
/// native window instead.
const IN_WINDOW_SURFACES: [(&str, usize); 9] = [
    ("native_viewport.rs", 1),            // the embedded fallback itself
    ("layout_debug.rs", 3),               // debug-build diagnostics
    ("shell_confirm.rs", 2),              // the modal unsaved-changes prompt: blocker and card
    ("views/overlays/walkthrough.rs", 2), // the first-run tour: scrim and card, anchored to the UI
    ("views/search_overlay.rs", 2),       // the search palette and its dimmed backdrop
    ("views/files_popup.rs", 1),          // the Files context menu, kept inside the window
    ("views/form_page/cabin_io.rs", 2),   // the cabin import/export prompts
    ("views/inputs_selectors.rs", 1),     // the Save-as-my-engine prompt
    ("app/nav_panel.rs", 1),              // the collapsed navigation rail
];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("source dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_detachable_surface_is_a_bare_in_window_window() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&root, &mut files);
    let mut counted = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(&root)
            .expect("under src")
            .to_string_lossy()
            .replace('\\', "/");
        if relative.ends_with("_tests.rs") || relative.contains("/tests") {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("read");
        let surfaces = text
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .filter(|line| line.contains("Window::new(") || line.contains("Area::new("))
            .count();
        if surfaces > 0 {
            counted.push((relative, surfaces));
        }
    }
    counted.sort();
    let mut allowed: Vec<(String, usize)> = IN_WINDOW_SURFACES
        .iter()
        .map(|(file, count)| ((*file).to_owned(), *count))
        .collect();
    allowed.sort();
    assert_eq!(
        counted, allowed,
        "an in-window Window or Area was added or removed; detachable surfaces must use native_viewport, and a deliberate in-window one is listed in IN_WINDOW_SURFACES"
    );
}

#[test]
// The state is opened field by field to mirror each window's own flag.
#[allow(clippy::field_reassign_with_default)]
fn every_converted_window_asks_for_a_native_os_window() {
    // The capture context stands in for a backend that can open OS windows:
    // only a surface that goes through the helper's immediate viewport is
    // recorded, so one that silently went back to an in-window surface fails.
    let (ctx, requested) = capture::native_context();
    let mut state = AppState::default();
    state.show_about = true;
    state.files.save_open = true;
    state.files.manager_open = true;
    let output = capture::frame(&ctx, |ctx| {
        crate::views::overlays::show_about(&mut state, ctx);
        crate::user_files_ui::show_windows(&mut state, ctx);
    });
    let requested = requested.borrow();
    for (key, title) in [
        ("about", None),
        ("save_my_aircraft", Some("Save as my aircraft")),
        ("my_aircraft", Some("Library")),
    ] {
        let window = requested
            .iter()
            .find(|window| window.id == viewport_id(key))
            .unwrap_or_else(|| panic!("{key} did not ask for a native window"));
        if let Some(title) = title {
            assert_eq!(window.title.as_deref(), Some(title), "{key}");
        }
    }
    assert_eq!(requested.len(), 3, "no other surface is requested");
    assert!(state.show_about);
    assert!(state.files.save_open && state.files.manager_open);
    // The root pass draws no in-window copy of them.
    assert!(
        output.shapes.is_empty(),
        "nothing is drawn in the main window"
    );
}
