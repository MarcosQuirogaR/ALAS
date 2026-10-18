// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Competition and mission-requirement controls for the fixed-wing UAV page.

mod mission;

pub(super) use mission::mission_inputs;

use egui::{Grid, Ui};

fn form_grid(ui: &mut Ui, id: impl std::hash::Hash, contents: impl FnOnce(&mut Ui)) {
    Grid::new(id)
        .num_columns(2)
        .spacing([20.0, 8.0])
        .show(ui, contents);
}
