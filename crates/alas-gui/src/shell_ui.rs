// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! UI state of the guided shell that belongs to no single page: the global
//! Basic / Advanced level, the search overlay, the Aircraft page's starting
//! point and the Files page.

use crate::config_edit::CleanSheetBrief;
use crate::detail::Detail;
use crate::search::SearchState;
use crate::views::aircraft_start::StartMode;
use crate::views::files_page::LibraryUi;

/// The shell-wide UI state.
#[derive(Debug, Default)]
pub struct ShellUi {
    /// The global Basic / Advanced detail level.
    pub detail: Detail,
    /// Global search (Ctrl+K) and its pending jump highlight.
    pub search: SearchState,
    /// The Files page's selection and typing state.
    pub library: LibraryUi,
    /// The starting point the Aircraft page offers.
    pub start_mode: StartMode,
    /// The clean-sheet requirements being entered.
    pub clean_sheet_draft: CleanSheetBrief,
    /// Undo and redo of edits made in the shell.
    pub undo: crate::shell_undo::ShellUndo,
    /// An action waiting for the user to confirm losing unsaved edits.
    pub pending: Option<crate::shell_confirm::Pending>,
    /// The session answer to the live-dispatch consent question.
    pub live_dispatch: crate::live_dispatch::LiveDispatchConsent,
    /// How many error lines were ever logged, so an action can tell whether it
    /// failed even when the capped log has dropped older lines.
    pub errors_logged: u64,
}
