// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tests of the Files explorer model: history, sorting, address validation
//! and the action sets per item kind.

use super::*;

fn entry(name: &str, kind: ItemKind, size: u64, modified: u64) -> Entry {
    Entry {
        path: PathBuf::from(name),
        name: name.to_owned(),
        lower: name.to_lowercase(),
        kind,
        size,
        modified: Some(modified),
    }
}

fn names(entries: &[Entry], order: &[usize]) -> Vec<String> {
    order.iter().map(|&i| entries[i].name.clone()).collect()
}

#[test]
fn history_goes_back_and_forward_and_drops_the_forward_branch_on_a_new_visit() {
    let mut history = History::new(PathBuf::from("a"));
    assert!(!history.can_back() && !history.can_forward());
    history.visit(PathBuf::from("b"));
    history.visit(PathBuf::from("b"));
    history.visit(PathBuf::from("c"));
    assert_eq!(history.current(), Path::new("c"));
    assert_eq!(history.back(), Some(Path::new("b")));
    assert_eq!(history.back(), Some(Path::new("a")));
    assert_eq!(history.back(), None);
    assert!(history.can_forward());
    assert_eq!(history.forward(), Some(Path::new("b")));
    history.visit(PathBuf::from("d"));
    assert!(!history.can_forward());
    assert_eq!(history.back(), Some(Path::new("b")));
}

#[test]
fn folders_come_first_and_names_sort_naturally() {
    let entries = vec![
        entry("run-10", ItemKind::Folder, 0, 1),
        entry("b.txt", ItemKind::Other, 5, 2),
        entry("run-2", ItemKind::Folder, 0, 3),
        entry("A.txt", ItemKind::Other, 9, 4),
    ];
    let ascending = Sort {
        key: SortKey::Name,
        ascending: true,
    };
    let order = view_order(&entries, ascending, "");
    assert_eq!(
        names(&entries, &order),
        ["run-2", "run-10", "A.txt", "b.txt"]
    );
    let descending = Sort {
        key: SortKey::Name,
        ascending: false,
    };
    let order = view_order(&entries, descending, "");
    assert_eq!(
        names(&entries, &order),
        ["run-10", "run-2", "b.txt", "A.txt"]
    );
}

#[test]
fn sorting_by_size_date_and_type_and_filtering_by_name() {
    let entries = vec![
        entry("small.csv", ItemKind::Csv, 10, 300),
        entry("big.png", ItemKind::Image, 900, 100),
        entry("mid.log", ItemKind::Log, 500, 200),
    ];
    let sort = |key, ascending| Sort { key, ascending };
    let by = |key, ascending| names(&entries, &view_order(&entries, sort(key, ascending), ""));
    assert_eq!(by(SortKey::Size, true), ["small.csv", "mid.log", "big.png"]);
    assert_eq!(
        by(SortKey::Size, false),
        ["big.png", "mid.log", "small.csv"]
    );
    assert_eq!(
        by(SortKey::Modified, false),
        ["small.csv", "mid.log", "big.png"]
    );
    assert_eq!(by(SortKey::Type, true), ["small.csv", "big.png", "mid.log"]);
    let order = view_order(&entries, sort(SortKey::Name, true), "g");
    assert_eq!(names(&entries, &order), ["big.png", "mid.log"]);
}

#[test]
fn a_header_click_flips_the_same_column_and_starts_a_new_one_naturally() {
    let name = Sort {
        key: SortKey::Name,
        ascending: true,
    };
    assert!(!name.clicked(SortKey::Name).ascending);
    assert!(!name.clicked(SortKey::Modified).ascending);
    assert!(name.clicked(SortKey::Type).ascending);
}

fn roots() -> Vec<PathBuf> {
    let base = std::env::temp_dir().join(format!("alas-files-roots-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("data").join("library")).expect("data");
    std::fs::create_dir_all(base.join("data-evil")).expect("sibling");
    std::fs::create_dir_all(base.join("elsewhere")).expect("elsewhere");
    vec![base.join("data")]
}

#[test]
fn a_typed_address_stays_inside_the_allowed_roots() {
    let roots = roots();
    let data = roots[0].clone();
    let base = data.parent().expect("parent").to_path_buf();
    assert_eq!(
        resolve_typed(&data.join("library").display().to_string(), &data, &roots),
        Ok(data.join("library"))
    );
    assert_eq!(
        resolve_typed("library", &data, &roots),
        Ok(data.join("library"))
    );
    assert_eq!(
        resolve_typed(
            &format!("\"{}\"", data.join("library").display()),
            &data,
            &roots
        ),
        Ok(data.join("library"))
    );
    // Parent escapes, sibling folders sharing a prefix, other drives' folders.
    assert_eq!(resolve_typed("..", &data, &roots), Err(PathError::Outside));
    assert_eq!(
        resolve_typed("library/../../elsewhere", &data, &roots),
        Err(PathError::Outside)
    );
    assert_eq!(
        resolve_typed(&base.join("data-evil").display().to_string(), &data, &roots),
        Err(PathError::Outside)
    );
    assert_eq!(
        resolve_typed(&base.join("elsewhere").display().to_string(), &data, &roots),
        Err(PathError::Outside)
    );
    assert_eq!(resolve_typed("   ", &data, &roots), Err(PathError::Empty));
    assert_eq!(
        resolve_typed("nothing-here", &data, &roots),
        Err(PathError::Missing)
    );
    assert!(within(&data, &roots) && !within(&base, &roots));
}

#[test]
fn breadcrumbs_start_at_the_outermost_root_and_up_stops_there() {
    let data = PathBuf::from("users").join("x").join("ALAS");
    let library = data.join("library");
    let roots = vec![library.join("my-aircraft"), data.clone(), library.clone()];
    let path = library.join("my-aircraft");
    let crumbs = breadcrumbs(&path, &roots);
    let labels: Vec<&str> = crumbs.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(labels, ["ALAS", "library", "my-aircraft"]);
    assert_eq!(crumbs[0].1, data);
    assert_eq!(up_target(&path, &roots), Some(library));
    assert_eq!(up_target(&data, &roots), None);
}

fn item(kind: ItemKind) -> ItemCtx {
    ItemCtx {
        kind,
        in_autosave: false,
        protected: false,
    }
}

#[test]
fn context_actions_follow_the_item_kind() {
    use Action::*;
    assert_eq!(
        actions_for(&[item(ItemKind::Aircraft)]),
        [
            LoadIntoAlas,
            Open,
            ShowInExplorer,
            CopyPath,
            Rename,
            Duplicate,
            ExportTo,
            Delete
        ]
    );
    assert_eq!(
        actions_for(&[item(ItemKind::Image)]),
        [
            Open,
            ShowInExplorer,
            CopyPath,
            Rename,
            Duplicate,
            ExportTo,
            Delete
        ]
    );
    assert_eq!(
        actions_for(&[item(ItemKind::RunFolder)]),
        [Open, ShowInExplorer, CopyPath, Rename, Delete]
    );
    let library_root = ItemCtx {
        protected: true,
        ..item(ItemKind::Folder)
    };
    assert_eq!(
        actions_for(&[library_root]),
        [Open, ShowInExplorer, CopyPath]
    );
    let slot = ItemCtx {
        in_autosave: true,
        ..item(ItemKind::Aircraft)
    };
    assert_eq!(
        actions_for(&[slot]),
        [Restore, ShowInExplorer, CopyPath, ExportTo, Delete]
    );
    assert_eq!(
        actions_for(&[item(ItemKind::Csv), item(ItemKind::Log)]),
        [CopyPath, Delete]
    );
    assert_eq!(
        actions_for(&[item(ItemKind::Csv), library_root]),
        [CopyPath]
    );
    assert!(actions_for(&[]).is_empty());
}

#[test]
fn double_click_loads_alas_files_and_opens_the_rest() {
    assert_eq!(default_action(item(ItemKind::Folder)), Action::Open);
    assert_eq!(
        default_action(item(ItemKind::Mission)),
        Action::LoadIntoAlas
    );
    assert_eq!(
        default_action(item(ItemKind::Payload)),
        Action::LoadIntoAlas
    );
    assert_eq!(default_action(item(ItemKind::Csv)), Action::Open);
    assert_eq!(default_action(item(ItemKind::Engine)), Action::Open);
    let slot = ItemCtx {
        in_autosave: true,
        ..item(ItemKind::Aircraft)
    };
    assert_eq!(default_action(slot), Action::Restore);
}

#[test]
fn kinds_come_from_the_header_the_folder_or_the_extension() {
    let json = |path: &str, head: &str| classify(Path::new(path), false, false, head.as_bytes());
    assert_eq!(
        json("x/a.json", r#"{"alas_my_aircraft": {"name": "x"}"#),
        ItemKind::Aircraft
    );
    assert_eq!(
        json(
            "x/m.json",
            r#"{"alas_object": {"kind": "mission_plan", "name": "m"}"#
        ),
        ItemKind::Mission
    );
    assert_eq!(
        json("x/p.json", r#"{"alas_object": {"kind": "payload_layout"}"#),
        ItemKind::Payload
    );
    assert_eq!(json("lib/my-cabins/c.json", "{}"), ItemKind::Cabin);
    assert_eq!(json("lib/my-airports/LECO.json", "{}"), ItemKind::Airport);
    assert_eq!(json("elsewhere/x.json", "{}"), ItemKind::Other);
    let plain = |path: &str| classify(Path::new(path), false, false, b"");
    assert_eq!(plain("n.dat"), ItemKind::Airfoil);
    assert_eq!(plain("n.PNG"), ItemKind::Image);
    assert_eq!(plain("n.csv"), ItemKind::Csv);
    assert_eq!(plain("alas-run-log-1.txt"), ItemKind::Log);
    assert_eq!(plain("n.yaml"), ItemKind::Config);
    assert_eq!(plain("n.bin"), ItemKind::Other);
    assert_eq!(
        classify(Path::new("d"), true, true, b""),
        ItemKind::RunFolder
    );
    assert_eq!(classify(Path::new("d"), true, false, b""), ItemKind::Folder);
}

#[test]
fn a_folder_is_listed_with_kinds_sizes_and_the_run_marker() {
    let base = std::env::temp_dir().join(format!("alas-files-list-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("run-7")).expect("run");
    std::fs::create_dir_all(base.join("plain")).expect("plain");
    std::fs::write(
        base.join("a.json"),
        r#"{"alas_object": {"kind": "mission_plan"}}"#,
    )
    .expect("json");
    std::fs::write(base.join("t.csv"), "1,2").expect("csv");
    std::fs::create_dir_all(base.join("plain").join("deep")).expect("deep");
    std::fs::write(base.join("plain").join("deep").join("needle.csv"), "x").expect("needle");
    let entries = list_dir(&base, true).expect("listing");
    let kind = |name: &str| entries.iter().find(|e| e.name == name).map(|e| e.kind);
    assert_eq!(kind("run-7"), Some(ItemKind::RunFolder));
    assert_eq!(kind("plain"), Some(ItemKind::Folder));
    assert_eq!(kind("a.json"), Some(ItemKind::Mission));
    assert_eq!(kind("t.csv"), Some(ItemKind::Csv));
    assert_eq!(
        entries.iter().find(|e| e.name == "t.csv").map(|e| e.size),
        Some(3)
    );
    let (found, truncated) = search_tree(&base, "needle", 10);
    assert!(!truncated);
    assert_eq!(found.len(), 1);
    assert!(found[0].name.ends_with("needle.csv") && found[0].name.starts_with("plain"));
    assert!(list_dir(&base.join("missing"), true).is_err());
}

#[test]
fn autosave_files_map_to_their_slots() {
    let data = PathBuf::from("data");
    let aircraft =
        crate::file_library::autosave_path(&data, crate::file_library::ObjectKind::Aircraft);
    assert_eq!(autosave_slot(&data, &aircraft), Some("aircraft".to_owned()));
    let sandbox = crate::file_library::sandbox_autosave_path(&data);
    assert_eq!(autosave_slot(&data, &sandbox), Some("sandbox".to_owned()));
    assert_eq!(
        autosave_slot(&data, &data.join("autosave").join("x.json")),
        None
    );
}

#[test]
fn sizes_and_times_are_short() {
    assert_eq!(format_size(812), "812 B");
    assert_eq!(format_size(1025), "2 KB");
    assert_eq!(format_size(3 * 1024 * 1024 + 400 * 1024), "3.4 MB");
    assert_eq!(format_modified(Some(0)), "1970-01-01 00:00");
    assert_eq!(format_modified(None), "");
}
