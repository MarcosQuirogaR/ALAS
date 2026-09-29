// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission, hardware, and propulsion controls for the fixed-wing UAV workflow.

mod hardware;
mod mission;
mod propulsion;

pub(super) use hardware::component_inputs;
pub(super) use mission::mission_inputs;
pub(super) use propulsion::propulsion_inputs;

use egui::{Grid, Ui};

fn form_grid(ui: &mut Ui, id: impl std::hash::Hash, contents: impl FnOnce(&mut Ui)) {
    Grid::new(id)
        .num_columns(2)
        .spacing([20.0, 8.0])
        .show(ui, contents);
}
