// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The grouped cards of the External Tools page: tool blocks of one discipline
//! under one heading.

use super::*;

/// Flight planning and route data: SimBrief and the local navigation data.
pub(in crate::views::tools_view) fn flight_card(state: &mut AppState, ui: &mut Ui) {
    group(ui, "Flight planning", |ui| {
        tool_block(
            ui,
            Some("simbrief"),
            "SimBrief flight plan",
            "https://www.simbrief.com/",
            &["Optionally use your most recent filed plan before ALAS falls back to local navigation data or a great-circle route."],
            |ui| simbrief_rows(state, ui),
        );
        block_gap(ui);
        tool_block(
            ui,
            Some("navdata"),
            "Local route data",
            "",
            &[
                "Navigation data and imported route files are kept in locations you choose. They are saved per user and never copied beside the application executable.",
                "The NASA Blue Marble route texture is bundled inside ALAS as a compressed PNG; it has no user path to configure.",
            ],
            |ui| local_route_rows(state, ui),
        );
    });
}

/// Structural solvers: MSC Nastran, the NASTRAN-95 comparison and Patran.
pub(in crate::views::tools_view) fn structural_card(state: &mut AppState, ui: &mut Ui) {
    group(ui, "Structural solvers", |ui| {
        nastran_card(state, ui);
        block_gap(ui);
        nastran95_card(state, ui);
        block_gap(ui);
        patran_card(state, ui);
    });
}

/// Airfoil and aerodynamic tools.
pub(in crate::views::tools_view) fn aero_card(state: &mut AppState, ui: &mut Ui) {
    group(ui, "Aerodynamic solvers", |ui| {
        mses_card(state, ui);
        block_gap(ui);
        avl_card(state, ui);
        block_gap(ui);
        openvsp_card(state, ui);
        block_gap(ui);
        flowunsteady_card(state, ui);
    });
}

/// The CFD environment.
pub(in crate::views::tools_view) fn cfd_card(state: &mut AppState, ui: &mut Ui) {
    group(ui, "CFD", |ui| openfoam_card(state, ui));
}
