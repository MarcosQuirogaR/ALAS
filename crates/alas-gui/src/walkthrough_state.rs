// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Walkthrough bookkeeping: opening the tour, preparing each step's shell
//! state, measuring spotlight targets, and restoring the shell afterwards.
//!
//! Split out of [`crate::state`] to keep that module under the line limit;
//! everything here is an `impl AppState` block.

use crate::state::{AppState, WalkthroughRestore};
use crate::views::tour_data::{TourTarget, TOUR_STEPS};

impl AppState {
    /// Open the walkthrough and remember the shell state it temporarily changes.
    pub fn begin_walkthrough(&mut self) {
        if self.walkthrough_restore.is_none() {
            self.walkthrough_restore = Some(WalkthroughRestore {
                active_page: self.active_page.clone(),
                nav_pinned: self.nav_pinned,
                nav_hover_open: self.nav_hover_open,
                preview_open: self.preview_open,
            });
        }
        self.walkthrough_step = 0;
        self.show_walkthrough = true;
        self.prepare_walkthrough_step();
    }

    /// Make the current step's page or normally-collapsed shell region visible.
    pub(crate) fn prepare_walkthrough_step(&mut self) {
        if !self.show_walkthrough {
            return;
        }
        let Some(step) = TOUR_STEPS.get(self.walkthrough_step) else {
            self.finish_walkthrough();
            return;
        };
        if let Some(page) = step.page {
            self.active_page = page.to_owned();
        }
        match step.target {
            Some(TourTarget::Navigation) => {
                self.nav_pinned = true;
                self.nav_hover_open = false;
            }
            Some(TourTarget::PreviewDock) => self.preview_open = true,
            _ => {}
        }
    }

    /// Close the tour and restore the page and docks it temporarily changed.
    pub(crate) fn finish_walkthrough(&mut self) {
        self.show_walkthrough = false;
        self.walkthrough_targets.clear();
        if let Some(restore) = self.walkthrough_restore.take() {
            self.active_page = restore.active_page;
            self.nav_pinned = restore.nav_pinned;
            self.nav_hover_open = restore.nav_hover_open;
            self.preview_open = restore.preview_open;
        }
    }

    /// Start a fresh collection of response geometry for this frame.
    pub(crate) fn clear_walkthrough_targets(&mut self) {
        self.walkthrough_targets.clear();
    }

    /// Record one shell region from the response egui actually laid out.
    pub(crate) fn record_walkthrough_target(&mut self, target: TourTarget, rect: egui::Rect) {
        if self.show_walkthrough && rect.is_finite() && rect.is_positive() {
            self.walkthrough_targets.insert(target, rect);
        }
    }

    /// Return the measured rectangle for the current walkthrough target.
    pub(crate) fn current_walkthrough_target(&self) -> Option<egui::Rect> {
        let target = TOUR_STEPS.get(self.walkthrough_step)?.target?;
        self.walkthrough_targets.get(&target).copied()
    }

    /// Whether the current tour step owns a particular shell target.
    pub(crate) fn walkthrough_targets(&self, target: TourTarget) -> bool {
        self.show_walkthrough
            && TOUR_STEPS
                .get(self.walkthrough_step)
                .is_some_and(|step| step.target == Some(target))
    }
}
