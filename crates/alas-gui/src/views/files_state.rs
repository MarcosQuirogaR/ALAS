// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The state of the Files explorer: cached folder listings, the view order,
//! selection, navigation and the workers' answers. Nothing here draws.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::files_jobs::{Jobs, Msg};
use super::files_model::{
    allowed, autosave_slot, default_action, view_order, Action, Entry, History, ItemCtx, Sort,
    SortKey,
};
use super::files_preview::Preview;

/// Seconds between re-reads of the folder on show.
pub(super) const POLL_SECONDS: f64 = 2.5;

/// How the folder is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ViewMode {
    Details,
    Icons,
}

/// A cached folder listing.
#[derive(Debug, Default)]
pub(super) struct Listing {
    pub entries: Arc<Vec<Entry>>,
    /// A worker is reading the folder.
    pub requested: bool,
    pub error: Option<String>,
    /// Counts every change of `entries`, so the view order is rebuilt.
    pub revision: u64,
    pub loaded: bool,
}

/// The selected items, the range anchor and the keyboard cursor.
#[derive(Debug, Default)]
pub(super) struct Selection {
    pub items: HashSet<PathBuf>,
    pub anchor: Option<PathBuf>,
    pub cursor: Option<PathBuf>,
}

impl Selection {
    pub(super) fn clear(&mut self) {
        self.items.clear();
        self.anchor = None;
        self.cursor = None;
    }

    pub(super) fn only(&mut self, path: &Path) {
        self.items.clear();
        self.items.insert(path.to_path_buf());
        self.anchor = Some(path.to_path_buf());
        self.cursor = Some(path.to_path_buf());
    }
}

/// The name being typed for a rename.
#[derive(Debug)]
pub(super) struct RenameState {
    pub path: PathBuf,
    /// The name when the rename started.
    pub original: String,
    pub text: String,
    pub fresh: bool,
}

/// The address bar in text mode.
#[derive(Debug)]
pub(super) struct AddressEdit {
    pub text: String,
    pub fresh: bool,
    pub refused: bool,
}

/// The preview of the selected item, once built.
#[derive(Default)]
pub(super) struct PreviewSlot {
    pub path: PathBuf,
    pub preview: Option<Preview>,
    pub texture: Option<egui::TextureHandle>,
}

impl std::fmt::Debug for PreviewSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewSlot")
            .field("path", &self.path)
            .finish()
    }
}

/// A thumbnail being built or built.
#[derive(Debug, Clone)]
pub(super) enum Thumb {
    Loading,
    Scene(Arc<alas_report::scene::Scene>),
    None,
}

/// The view order and what it was built from.
#[derive(Debug)]
pub(super) struct OrderCache {
    pub key: (PathBuf, u64, Sort, String, bool),
    pub order: Arc<Vec<usize>>,
}

/// A running or finished subfolder search.
#[derive(Debug, Default)]
pub(super) struct SearchState {
    pub id: u64,
    /// The folder and text the running or finished search was started for.
    pub for_query: Option<(PathBuf, String)>,
    pub results: Option<(Arc<Vec<Entry>>, bool)>,
}

/// Everything the Files page remembers between frames.
#[derive(Debug)]
pub struct LibraryUi {
    /// Whether the "Save current as" window is open.
    pub save_open: bool,
    /// The kind chosen in the "Save current as" window.
    pub kind: crate::file_library::ObjectKind,
    /// The name typed for saving.
    pub save_name: String,
    /// The registry item chosen for kinds that store one.
    pub save_choice: String,
    /// Disk-usage summary and its worker.
    pub storage: crate::files_storage::StorageSummary,
    pub(super) history: Option<History>,
    pub(super) roots: Vec<PathBuf>,
    pub(super) listings: HashMap<PathBuf, Listing>,
    pub(super) order: Option<OrderCache>,
    pub(super) sort: Sort,
    pub(super) mode: ViewMode,
    pub(super) query: String,
    pub(super) search_sub: bool,
    pub(super) search: SearchState,
    pub(super) sel: Selection,
    /// Keep the cursor row in view on the next frame.
    pub(super) follow_cursor: bool,
    /// Set the list's scroll offset on the next frame.
    pub(super) scroll_to: Option<f32>,
    pub(super) list_offset: f32,
    pub(super) list_height: f32,
    pub(super) grid_columns: usize,
    /// Widths of the Type, Date modified and Size columns.
    pub(super) col_widths: [f32; 3],
    pub(super) address: Option<AddressEdit>,
    pub(super) rename: Option<RenameState>,
    pub(super) delete_confirm: Option<Vec<PathBuf>>,
    /// Autosave slot awaiting restore confirmation.
    pub(super) restore_confirm: Option<String>,
    /// Select (and optionally rename) this item once the listing shows it.
    pub(super) reveal: Option<(PathBuf, bool)>,
    pub(super) show_preview: bool,
    pub(super) expanded: HashSet<PathBuf>,
    pub(super) autosaves: Vec<(String, String)>,
    pub(super) autosaves_at: Option<f64>,
    pub(super) notice_dismissed: bool,
    pub(super) preview: Option<PreviewSlot>,
    pub(super) thumbs: HashMap<PathBuf, Thumb>,
    pub(super) thumb_revision: u64,
    pub(super) jobs: Jobs,
    pub(super) poll_at: f64,
    pub(super) was_focused: bool,
}

impl Default for LibraryUi {
    fn default() -> Self {
        Self {
            save_open: false,
            kind: crate::file_library::ObjectKind::Aircraft,
            save_name: String::new(),
            save_choice: String::new(),
            storage: crate::files_storage::StorageSummary::default(),
            history: None,
            roots: Vec::new(),
            listings: HashMap::new(),
            order: None,
            sort: Sort {
                key: SortKey::Name,
                ascending: true,
            },
            mode: ViewMode::Details,
            query: String::new(),
            search_sub: false,
            search: SearchState::default(),
            sel: Selection::default(),
            follow_cursor: false,
            scroll_to: None,
            list_offset: 0.0,
            list_height: 400.0,
            grid_columns: 1,
            col_widths: super::files_list::DEFAULT_COLUMNS,
            address: None,
            rename: None,
            delete_confirm: None,
            restore_confirm: None,
            reveal: None,
            show_preview: true,
            expanded: HashSet::new(),
            autosaves: Vec::new(),
            autosaves_at: None,
            notice_dismissed: false,
            preview: None,
            thumbs: HashMap::new(),
            thumb_revision: 1,
            jobs: Jobs::default(),
            poll_at: 0.0,
            was_focused: true,
        }
    }
}

impl LibraryUi {
    /// Forget the cached listings so the next frame reads the folders again.
    pub fn invalidate(&mut self) {
        for listing in self.listings.values_mut() {
            listing.loaded = false;
        }
        self.preview = None;
        self.thumbs.clear();
        self.thumb_revision += 1;
        self.autosaves_at = None;
        self.search.for_query = None;
    }

    /// The folder shown.
    pub(super) fn current(&self) -> PathBuf {
        self.history
            .as_ref()
            .map(|history| history.current().to_path_buf())
            .unwrap_or_default()
    }

    /// The entries the list shows: the folder's, or the search's.
    pub(super) fn shown_entries(&self) -> Arc<Vec<Entry>> {
        if self.searching_subfolders() {
            if let Some((entries, _)) = &self.search.results {
                return entries.clone();
            }
            return Arc::default();
        }
        self.listings
            .get(&self.current())
            .map(|listing| listing.entries.clone())
            .unwrap_or_default()
    }

    pub(super) fn searching_subfolders(&self) -> bool {
        self.search_sub && !self.query.trim().is_empty()
    }

    /// The view order of the shown entries, rebuilt when its inputs change.
    pub(super) fn view(&mut self) -> (Arc<Vec<Entry>>, Arc<Vec<usize>>) {
        let entries = self.shown_entries();
        let searching = self.searching_subfolders();
        let revision = if searching {
            self.search.id
        } else {
            self.listings
                .get(&self.current())
                .map_or(0, |listing| listing.revision)
        };
        let query = if searching {
            String::new()
        } else {
            self.query.trim().to_lowercase()
        };
        let key = (
            self.current(),
            revision,
            self.sort,
            query.clone(),
            searching,
        );
        if self.order.as_ref().is_none_or(|cache| cache.key != key) {
            self.order = Some(OrderCache {
                order: Arc::new(view_order(&entries, self.sort, &query)),
                key,
            });
        }
        let order = self
            .order
            .as_ref()
            .map(|cache| cache.order.clone())
            .unwrap_or_default();
        (entries, order)
    }

    /// The entries of the selection, in view order.
    pub(super) fn selected<'a>(&self, entries: &'a [Entry], order: &[usize]) -> Vec<&'a Entry> {
        order
            .iter()
            .map(|&index| &entries[index])
            .filter(|entry| self.sel.items.contains(&entry.path))
            .collect()
    }

    /// Whether `path` is a root or library folder that must not be changed.
    pub(super) fn protected(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| {
            super::files_model::normalize_lexical(root)
                == super::files_model::normalize_lexical(path)
        })
    }

    /// The context of `entry` for the action rules.
    pub(super) fn item_ctx(&self, entry: &Entry, data: &Path) -> ItemCtx {
        ItemCtx {
            kind: entry.kind,
            in_autosave: autosave_slot(data, &entry.path).is_some(),
            protected: self.protected(&entry.path),
        }
    }

    /// What a double-click does to `entry`.
    pub(super) fn default_for(&self, entry: &Entry, data: &Path) -> Action {
        default_action(self.item_ctx(entry, data))
    }

    /// Visit a folder: history, selection, search and tree are updated.
    pub(super) fn go(&mut self, path: PathBuf) {
        if !allowed(&path, &self.roots) {
            return;
        }
        if let Some(history) = &mut self.history {
            history.visit(path.clone());
        }
        self.arrive(&path);
    }

    /// Back or forward one step in the history.
    pub(super) fn step(&mut self, forward: bool) {
        let target = self.history.as_mut().and_then(|history| {
            if forward {
                history.forward().map(Path::to_path_buf)
            } else {
                history.back().map(Path::to_path_buf)
            }
        });
        if let Some(path) = target {
            self.arrive(&path);
        }
    }

    fn arrive(&mut self, path: &Path) {
        self.sel.clear();
        self.rename = None;
        self.address = None;
        self.query.clear();
        self.preview = None;
        self.order = None;
        self.scroll_to = Some(0.0);
        let mut ancestor = path.parent();
        while let Some(parent) = ancestor {
            self.expanded.insert(parent.to_path_buf());
            ancestor = parent.parent();
        }
    }

    /// Fold one worker answer into the state. Returns the finished operation.
    pub(super) fn apply(
        &mut self,
        message: Msg,
        ctx: &egui::Context,
    ) -> Option<super::files_jobs::OpDone> {
        match message {
            Msg::Listing(dir, result) => self.apply_listing(&dir, result),
            Msg::Search(id, found, truncated) => {
                if id == self.search.id {
                    self.search.results = Some((Arc::new(found), truncated));
                }
            }
            Msg::Preview(path, mut preview) => {
                if let Some(slot) = self.preview.as_mut().filter(|slot| slot.path == path) {
                    slot.texture = preview.image.take().map(|image| {
                        ctx.load_texture(
                            format!("files_preview:{}", path.display()),
                            image,
                            egui::TextureOptions::LINEAR,
                        )
                    });
                    if let Some(scene) = &preview.scene {
                        self.thumbs.insert(path, Thumb::Scene(scene.clone()));
                    }
                    slot.preview = Some(*preview);
                }
            }
            Msg::Thumb(path, scene) => {
                self.jobs.thumbs_in_flight = self.jobs.thumbs_in_flight.saturating_sub(1);
                self.thumbs
                    .insert(path, scene.map_or(Thumb::None, Thumb::Scene));
            }
            Msg::Done(done) => {
                self.jobs.busy_ops = self.jobs.busy_ops.saturating_sub(1);
                return Some(done);
            }
        }
        None
    }

    fn apply_listing(&mut self, dir: &Path, result: Result<Vec<Entry>, String>) {
        let current = self.current();
        let listing = self.listings.entry(dir.to_path_buf()).or_default();
        listing.requested = false;
        listing.loaded = true;
        match result {
            Ok(entries) => {
                listing.error = None;
                if !same_listing(&listing.entries, &entries) {
                    listing.entries = Arc::new(entries);
                    listing.revision += 1;
                    if dir == current.as_path() {
                        let present: HashSet<&PathBuf> =
                            listing.entries.iter().map(|entry| &entry.path).collect();
                        self.sel.items.retain(|path| present.contains(path));
                    }
                }
            }
            Err(error) => {
                listing.error = Some(error);
                if !listing.entries.is_empty() {
                    listing.entries = Arc::default();
                    listing.revision += 1;
                }
            }
        }
    }
}

fn same_listing(old: &[Entry], new: &[Entry]) -> bool {
    old.len() == new.len() && {
        let mut a: Vec<&Entry> = old.iter().collect();
        let mut b: Vec<&Entry> = new.iter().collect();
        a.sort_by(|x, y| x.path.cmp(&y.path));
        b.sort_by(|x, y| x.path.cmp(&y.path));
        a.iter().zip(&b).all(|(x, y)| {
            x.path == y.path && x.size == y.size && x.modified == y.modified && x.kind == y.kind
        })
    }
}
