// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The pure model of the Files explorer: what an item is, how a folder is
//! listed, sorted and filtered, the navigation history, the address-bar path
//! rules (a typed path never leaves the allowed roots) and the actions each
//! kind of item offers. Nothing here draws or touches the application state.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

#[path = "files_rules.rs"]
mod rules;
pub(crate) use rules::*;
#[path = "files_listing.rs"]
mod listing;
pub(super) use listing::{list_dir, search_tree};

/// What an item of the data folder is, which decides its icon, its type label
/// and the actions it offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum ItemKind {
    Folder,
    RunFolder,
    Aircraft,
    Mission,
    Payload,
    Results,
    Config,
    Airport,
    Engine,
    Cabin,
    Airfoil,
    Image,
    Csv,
    Log,
    Other,
}

impl ItemKind {
    /// The type column text (English; passed through the catalogue).
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Folder => "File folder",
            Self::RunFolder => "Run folder",
            Self::Aircraft => "Aircraft",
            Self::Mission => "Mission plan",
            Self::Payload => "Payload layout",
            Self::Results => "Results snapshot",
            Self::Config => "Configuration",
            Self::Airport => "Airport",
            Self::Engine => "Engine",
            Self::Cabin => "Cabin layout",
            Self::Airfoil => "Airfoil",
            Self::Image => "Image",
            Self::Csv => "CSV file",
            Self::Log => "Log file",
            Self::Other => "File",
        }
    }

    pub(super) const fn is_folder(self) -> bool {
        matches!(self, Self::Folder | Self::RunFolder)
    }

    /// Whether ALAS can load the file as the open case (or a part of it).
    pub(super) const fn loadable(self) -> bool {
        matches!(
            self,
            Self::Aircraft | Self::Mission | Self::Payload | Self::Results | Self::Config
        )
    }

    /// Whether an autosave slot of this kind can be restored.
    pub(super) const fn restorable(self) -> bool {
        matches!(self, Self::Aircraft | Self::Mission | Self::Payload)
    }
}

/// One listed item.
#[derive(Debug, Clone)]
pub(super) struct Entry {
    pub path: PathBuf,
    /// The text shown in the name column (a relative path in a subfolder search).
    pub name: String,
    /// Lower-case `name`, for sorting and filtering.
    pub lower: String,
    pub kind: ItemKind,
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub modified: Option<u64>,
}

impl Entry {
    pub(super) fn is_folder(&self) -> bool {
        self.kind.is_folder()
    }
}

/// The kind named by a library header found in the first bytes of a JSON file.
fn header_kind(text: &str) -> Option<ItemKind> {
    let after = &text[text.find("\"alas_object\"")?..];
    let after = &after[after.find("\"kind\"")? + 6..];
    let start = after.find('"')? + 1;
    let value = &after[start..];
    let id = &value[..value.find('"')?];
    Some(match id {
        "aircraft" => ItemKind::Aircraft,
        "engine" => ItemKind::Engine,
        "airfoil" => ItemKind::Airfoil,
        "mission_plan" => ItemKind::Mission,
        "payload_layout" => ItemKind::Payload,
        "airport" => ItemKind::Airport,
        "results" => ItemKind::Results,
        _ => ItemKind::Other,
    })
}

fn classify_json(path: &Path, head: &[u8]) -> ItemKind {
    let text = String::from_utf8_lossy(head);
    if text.contains("\"alas_my_aircraft\"") {
        return ItemKind::Aircraft;
    }
    if let Some(kind) = header_kind(&text) {
        return kind;
    }
    let parent = path
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match parent.as_str() {
        "my-aircraft" => ItemKind::Aircraft,
        "my-airports" => ItemKind::Airport,
        "my-engines" => ItemKind::Engine,
        "my-cabins" => ItemKind::Cabin,
        "my-airfoils" => ItemKind::Airfoil,
        _ => ItemKind::Other,
    }
}

/// The kind of the item at `path`. `head` is the start of the file (only JSON
/// files need it) and `run_folder` says a folder holds a run's manifest.
pub(super) fn classify(path: &Path, is_dir: bool, run_folder: bool, head: &[u8]) -> ItemKind {
    if is_dir {
        return if run_folder {
            ItemKind::RunFolder
        } else {
            ItemKind::Folder
        };
    }
    let ext = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "webp" | "tif" | "tiff" => ItemKind::Image,
        "csv" | "tsv" => ItemKind::Csv,
        "log" => ItemKind::Log,
        "txt"
            if path.file_stem().is_some_and(|stem| {
                stem.to_string_lossy().to_ascii_lowercase().contains("log")
            }) =>
        {
            ItemKind::Log
        }
        "dat" => ItemKind::Airfoil,
        "yaml" | "yml" => ItemKind::Config,
        "json" => classify_json(path, head),
        _ => ItemKind::Other,
    }
}

/// The columns a folder can be sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SortKey {
    Name,
    Type,
    Modified,
    Size,
}

/// A sort column and direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Sort {
    pub key: SortKey,
    pub ascending: bool,
}

impl Sort {
    /// Clicking a header: the same column flips direction; another column
    /// starts in its natural direction (dates and sizes newest/largest first).
    pub(super) fn clicked(self, key: SortKey) -> Self {
        if self.key == key {
            Self {
                key,
                ascending: !self.ascending,
            }
        } else {
            Self {
                key,
                ascending: matches!(key, SortKey::Name | SortKey::Type),
            }
        }
    }
}

/// Order two lower-case names the way Explorer does: runs of digits compare as
/// numbers ("run-2" before "run-10").
pub(super) fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut digits = String::new();
                    while let Some(&c) = chars.peek() {
                        if !c.is_ascii_digit() {
                            break;
                        }
                        digits.push(c);
                        chars.next();
                    }
                    digits.trim_start_matches('0').to_owned()
                };
                let (da, db) = (take(&mut left), take(&mut right));
                let order = da.len().cmp(&db.len()).then_with(|| da.cmp(&db));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                left.next();
                right.next();
            }
        }
    }
}

/// The indices of `entries` that match `query` (lower case, may be empty),
/// folders first, then by `sort`.
pub(super) fn view_order(entries: &[Entry], sort: Sort, query: &str) -> Vec<usize> {
    let mut order: Vec<usize> = (0..entries.len())
        .filter(|&index| query.is_empty() || entries[index].lower.contains(query))
        .collect();
    order.sort_by(|&a, &b| {
        let (x, y) = (&entries[a], &entries[b]);
        let folders = y.is_folder().cmp(&x.is_folder());
        if folders != Ordering::Equal {
            return folders;
        }
        let by_name = natural_cmp(&x.lower, &y.lower);
        let primary = match sort.key {
            SortKey::Name => by_name,
            SortKey::Type => x.kind.label().cmp(y.kind.label()).then(by_name),
            SortKey::Modified => x.modified.cmp(&y.modified).then(by_name),
            SortKey::Size => x.size.cmp(&y.size).then(by_name),
        };
        if sort.ascending {
            primary
        } else {
            primary.reverse()
        }
    });
    order
}

/// Back/forward history of visited folders.
#[derive(Debug, Clone)]
pub(super) struct History {
    stack: Vec<PathBuf>,
    position: usize,
}

impl History {
    pub(super) fn new(start: PathBuf) -> Self {
        Self {
            stack: vec![start],
            position: 0,
        }
    }

    pub(super) fn current(&self) -> &Path {
        &self.stack[self.position]
    }

    /// Visit `path`: forward history is dropped; visiting the current folder
    /// again changes nothing.
    pub(super) fn visit(&mut self, path: PathBuf) {
        if self.stack[self.position] == path {
            return;
        }
        self.stack.truncate(self.position + 1);
        self.stack.push(path);
        self.position += 1;
    }

    pub(super) fn can_back(&self) -> bool {
        self.position > 0
    }

    pub(super) fn can_forward(&self) -> bool {
        self.position + 1 < self.stack.len()
    }

    pub(super) fn back(&mut self) -> Option<&Path> {
        if !self.can_back() {
            return None;
        }
        self.position -= 1;
        Some(&self.stack[self.position])
    }

    pub(super) fn forward(&mut self) -> Option<&Path> {
        if !self.can_forward() {
            return None;
        }
        self.position += 1;
        Some(&self.stack[self.position])
    }
}

/// Something the user can do with items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Open,
    LoadIntoAlas,
    Restore,
    ShowInExplorer,
    CopyPath,
    Rename,
    Duplicate,
    ExportTo,
    Delete,
}

/// What the context menu needs to know about one selected item.
#[derive(Debug, Clone, Copy)]
pub(super) struct ItemCtx {
    pub kind: ItemKind,
    /// The item is an autosave slot file.
    pub in_autosave: bool,
    /// A root or library folder: it cannot be renamed or deleted.
    pub protected: bool,
}

/// The actions valid for the selected items, in menu order.
pub(super) fn actions_for(items: &[ItemCtx]) -> Vec<Action> {
    let mut out = Vec::new();
    match items {
        [] => {}
        [item] if item.kind.is_folder() => {
            out.extend([Action::Open, Action::ShowInExplorer, Action::CopyPath]);
            if !item.protected {
                out.extend([Action::Rename, Action::Delete]);
            }
        }
        [item] if item.in_autosave => {
            if item.kind.restorable() {
                out.push(Action::Restore);
            }
            out.extend([
                Action::ShowInExplorer,
                Action::CopyPath,
                Action::ExportTo,
                Action::Delete,
            ]);
        }
        [item] => {
            if item.kind.loadable() {
                out.push(Action::LoadIntoAlas);
            }
            out.extend([
                Action::Open,
                Action::ShowInExplorer,
                Action::CopyPath,
                Action::Rename,
                Action::Duplicate,
                Action::ExportTo,
                Action::Delete,
            ]);
        }
        many => {
            out.push(Action::CopyPath);
            if many.iter().all(|item| !item.protected) {
                out.push(Action::Delete);
            }
        }
    }
    out
}

/// What a double-click or Enter does to one item.
pub(super) fn default_action(item: ItemCtx) -> Action {
    if item.kind.is_folder() {
        Action::Open
    } else if item.in_autosave && item.kind.restorable() {
        Action::Restore
    } else if item.kind.loadable() && !item.in_autosave {
        Action::LoadIntoAlas
    } else {
        Action::Open
    }
}

/// The autosave slot (`aircraft`, `mission_plan`, `payload_layout` or
/// `sandbox`) a file of the data folder is, if any.
pub(super) fn autosave_slot(data_dir: &Path, path: &Path) -> Option<String> {
    use crate::file_library::{autosave_path, sandbox_autosave_path, ObjectKind};
    let same = |a: &Path, b: &Path| lower_components(a) == lower_components(b);
    for kind in [
        ObjectKind::Aircraft,
        ObjectKind::MissionPlan,
        ObjectKind::PayloadLayout,
    ] {
        if same(path, &autosave_path(data_dir, kind)) {
            return Some(kind.id().to_owned());
        }
    }
    same(path, &sandbox_autosave_path(data_dir)).then(|| "sandbox".to_owned())
}

/// `12 KB`, `3.4 MB`, `812 B`.
pub(super) fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let value = bytes as f64;
    if value < KB {
        format!("{bytes} B")
    } else if value < KB * KB {
        format!("{} KB", (value / KB).ceil() as u64)
    } else if value < KB * KB * KB {
        format!("{:.1} MB", value / (KB * KB))
    } else {
        format!("{:.2} GB", value / (KB * KB * KB))
    }
}

/// `2026-10-31 14:03`: the modified time in UTC.
pub(super) fn format_modified(unix: Option<u64>) -> String {
    unix.map(|seconds| {
        crate::user_files::format_utc(seconds)
            .trim_end_matches(" UTC")
            .to_owned()
    })
    .unwrap_or_default()
}

#[cfg(test)]
#[path = "files_model_tests.rs"]
mod tests;
