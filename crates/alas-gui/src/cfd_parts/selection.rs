// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airfoil selection, preview, input invalidation, and settings persistence.

use super::super::*;
use alas_geom::airfoil_library::AirfoilLibrary;
use std::sync::atomic::Ordering;
impl AirfoilCfdState {
    /// Replace the filter and refresh the list from the embedded database.
    pub fn set_airfoil_filter(&mut self, filter: impl Into<String>) {
        let filter = filter.into();
        if self.airfoil_filter == filter {
            return;
        }
        self.airfoil_filter = filter;
        self.refresh_airfoil_filter();
    }

    /// Recompute the filtered database names without changing the selection.
    pub fn refresh_airfoil_filter(&mut self) {
        let names = AirfoilLibrary::get_available_airfoils();
        self.filtered_airfoils = alas_screen::runner::filter_names(&names, &self.airfoil_filter);
    }

    /// Return the currently selected library identity.
    pub fn selected_airfoil(&self) -> &str {
        &self.config.airfoil_name
    }

    /// Select a database section for the CFD study and invalidate its result.
    ///
    /// The aircraft configuration is intentionally untouched.  This state is
    /// an independent study contract, even when it was opened from a selected
    /// Airfoil Screening row.
    pub fn select_airfoil(&mut self, name: &str) -> bool {
        if AirfoilLibrary::get(name).is_none() {
            self.preview_coordinates = None;
            return false;
        }
        if self.config.airfoil_name == name {
            self.refresh_preview();
            return true;
        }
        self.config.airfoil_name = name.to_owned();
        self.refresh_preview();
        self.mark_inputs_changed();
        true
    }

    /// Re-resolve the selected outline without silently repairing coordinates.
    pub fn refresh_preview(&mut self) {
        self.preview_coordinates = AirfoilLibrary::get(&self.config.airfoil_name)
            .map(|airfoil| airfoil.coordinates.clone())
            .filter(|points| {
                points.len() >= 3 && points.iter().all(|(x, y)| x.is_finite() && y.is_finite())
            });
    }

    /// Invalidate dependent CFD outputs after an input edit.
    pub fn mark_inputs_changed(&mut self) {
        self.input_revision = self.input_revision.wrapping_add(1);
        self.result = None;
        self.sweep_results.clear();
        self.last_case_dir = None;
        self.selected_field = None;
        self.error = None;
        if self.running {
            self.cancel_flag.store(true, Ordering::Relaxed);
            self.status =
                "Inputs changed; cancelling the previous CFD run before accepting a new result."
                    .to_owned();
        } else {
            self.status = "Inputs changed; previous CFD results were invalidated.".to_owned();
        }
    }

    /// Invalidate a connection result after changing the execution
    /// environment. A probe describes one exact set of paths and must not be
    /// presented as evidence for a later selection.
    pub fn mark_environment_changed(&mut self) {
        self.capabilities = None;
        // There is no useful cancellation work to perform for the lightweight
        // probe thread; dropping its receiver prevents a result for the old
        // paths from being accepted after the user edits them.
        if self.probing {
            self.probe_rx = None;
            self.probing = false;
        }
        if !self.running {
            self.status = "OpenFOAM connection has not been checked.".to_owned();
        }
    }
}
