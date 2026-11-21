// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tests of the Files page state: navigation inside the allowed roots and
//! selection by click.

use super::files_model::{ItemKind, Sort, SortKey};
use super::*;

fn entry(name: &str) -> Entry {
    Entry {
        path: PathBuf::from("root").join(name),
        name: name.to_owned(),
        lower: name.to_lowercase(),
        kind: ItemKind::Other,
        size: 1,
        modified: None,
    }
}

fn ui_at(root: &str) -> LibraryUi {
    let mut ui = LibraryUi {
        roots: vec![PathBuf::from(root)],
        ..LibraryUi::default()
    };
    ui.history = Some(History::new(PathBuf::from(root)));
    ui
}

#[test]
fn navigation_refuses_folders_outside_the_roots_and_keeps_history() {
    let mut ui = ui_at("root");
    ui.go(PathBuf::from("elsewhere"));
    ui.go(PathBuf::from("root").join("..").join("elsewhere"));
    assert_eq!(ui.current(), PathBuf::from("root"));
    ui.go(PathBuf::from("root").join("a"));
    ui.go(PathBuf::from("root").join("a").join("b"));
    assert_eq!(ui.current(), PathBuf::from("root").join("a").join("b"));
    ui.step(false);
    assert_eq!(ui.current(), PathBuf::from("root").join("a"));
    ui.step(true);
    assert_eq!(ui.current(), PathBuf::from("root").join("a").join("b"));
    assert!(ui.expanded.contains(&PathBuf::from("root").join("a")));
}

#[test]
fn view_order_follows_the_sort_and_the_filter() {
    let mut ui = ui_at("root");
    let listing = ui.listings.entry(PathBuf::from("root")).or_default();
    listing.entries = std::sync::Arc::new(vec![entry("b.csv"), entry("a.csv"), entry("c.log")]);
    listing.revision = 1;
    let (entries, order) = ui.view();
    let names = |order: &[usize]| -> Vec<String> {
        order.iter().map(|&i| entries[i].name.clone()).collect()
    };
    assert_eq!(names(&order), ["a.csv", "b.csv", "c.log"]);
    ui.sort = Sort {
        key: SortKey::Name,
        ascending: false,
    };
    let (_, order) = ui.view();
    assert_eq!(names(&order), ["c.log", "b.csv", "a.csv"]);
    ui.query = "CSV".to_owned();
    let (_, order) = ui.view();
    assert_eq!(names(&order), ["b.csv", "a.csv"]);
}

#[test]
fn ctrl_toggles_and_shift_extends_the_selection() {
    let entries = vec![entry("a"), entry("b"), entry("c"), entry("d")];
    let order: Vec<usize> = (0..4).collect();
    let mut state = AppState::default();
    let ctrl = egui::Modifiers::COMMAND;
    let shift = egui::Modifiers::SHIFT;
    select_click(&mut state, &entries, &order, 1, egui::Modifiers::NONE);
    select_click(&mut state, &entries, &order, 3, shift);
    assert_eq!(state.shell.library.sel.items.len(), 3);
    select_click(&mut state, &entries, &order, 2, ctrl);
    assert_eq!(state.shell.library.sel.items.len(), 2);
    select_click(&mut state, &entries, &order, 0, egui::Modifiers::NONE);
    assert_eq!(state.shell.library.sel.items.len(), 1);
}
