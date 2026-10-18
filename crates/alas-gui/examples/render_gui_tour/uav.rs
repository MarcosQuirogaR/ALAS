// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! UAV page shots of the visual tour: one small competition search on the
//! reviewed fixture propeller tables, then each section of the page.

use std::path::PathBuf;

use alas_gui::state::Language;
use alas_gui::uav::UavSection;
use alas_gui::{AppState, AppTheme};

use super::Shot;

/// Shots of the fixed-wing UAV page: each section in both languages, with the
/// light theme paged and the dark theme at the top, like the jet pages.
pub(super) fn uav_shots() -> Vec<Shot> {
    let mut list = Vec::new();
    for (theme, theme_name) in [(AppTheme::Light, "light"), (AppTheme::Dark, "dark")] {
        for (language, code) in [(Language::En, "en"), (Language::Es, "es")] {
            for (section, name) in [
                (UavSection::Mission, "uav_mission"),
                (UavSection::Airframe, "uav_airframe"),
                (UavSection::Results, "uav_results"),
            ] {
                list.push(Shot {
                    name: format!("{name}_{theme_name}_{code}"),
                    language,
                    theme,
                    page: "uav",
                    results_tab: None,
                    advanced_tab: None,
                    max_pages: if theme == AppTheme::Light { 30 } else { 1 },
                    mtow_mode: None,
                    uav_section: Some(section),
                });
            }
        }
    }
    list
}

/// Run one small competition search so the UAV result sections have content.
/// ALAS ships no propeller tables; the reviewed fixture tables stand in for
/// them here, so the UAV renders show a fixture-backed search.
pub(super) fn prepare_uav_state(state: &mut AppState) {
    alas_uav::propulsion_electric::prop_data::set_prop_data_dir(Some(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../alas-uav/tests/fixtures/prop_data"),
    ));
    state.uav.evaluations = 64;
    if state.uav.start() {
        while state.uav.is_running() {
            state.uav.poll();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        state.uav.poll();
    }
    state.uav.active_section = UavSection::Results;
}
