// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Form layout renders: every form page on one column grid, at the
//! sizes of the review (points; a zoomed-out UI is a larger point screen).

use crate::state::AppState;
use crate::test_raster::{render, write_in};

fn page_shot(id: &str, width: f32, height: f32, name: &str) {
    let mut state = AppState::default();
    state.load_preset("A320-200");
    let page = crate::nav::page(id).unwrap_or_else(|| panic!("page {id}"));
    let shot = render(width, height, 6, 0, |ui| {
        crate::detail::publish(ui.ctx(), crate::detail::Detail::Advanced);
        ui.set_width(width - 40.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            crate::views::form_page::show_form_page(&mut state, ui, page);
        });
    });
    write_in(
        &crate::test_raster::render_dir(),
        &format!("{name}-{width:.0}.png"),
        &shot,
    );
}

#[test]
fn r39_form_pages_render() {
    for width in [1100.0_f32, 1600.0, 2400.0, 3000.0] {
        page_shot("engine_designer", width, 900.0, "propulsion");
        page_shot("mission_advanced", width, 700.0, "mission-settings");
        page_shot("mass_advanced", width, 1400.0, "mass");
        page_shot("geometry", width, 1400.0, "geometry");
    }
}

#[test]
fn r39_mtow_card_renders() {
    for width in [1100.0_f32, 1600.0, 2400.0] {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        state.run_options.optimize = true;
        state.config_values["optimizer"]["objective"]["mtow_sizing"] = "mtow_band".into();
        let shot = render(width, 500.0, 6, 0, |ui| {
            ui.set_width(width - 40.0);
            crate::theme::card_frame(ui).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                super::inputs_mtow::show_mtow_controls(&mut state, ui);
            });
        });
        write_in(
            &crate::test_raster::render_dir(),
            &format!("mtow-{width:.0}.png"),
            &shot,
        );
    }
}

#[test]
fn r39_wing_analysis_setup_renders() {
    for width in [900.0_f32, 1400.0, 2400.0] {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        let ctx = egui::Context::default();
        super::wing_analysis_view::wing_analysis::reset_window();
        super::wing_analysis_view::open_wing_analysis(&ctx);
        let shot = render(width, 1500.0, 8, 30, |ui| {
            super::wing_analysis_view::setup::show_setup_tab(&mut state, ui);
        });
        write_in(
            &crate::test_raster::render_dir(),
            &format!("wing-analysis-{width:.0}.png"),
            &shot,
        );
    }
}

fn screening_result() -> alas_screen::types::AirfoilScreeningResult {
    let candidates = (0..30)
        .map(|i| {
            let t = f64::from(i);
            alas_screen::types::AirfoilCandidateResult {
                name: format!("naca{:04}", 12 + i),
                status: "ok".into(),
                cl: Some(0.5),
                cd: Some(0.006 + 0.0002 * t),
                l_over_d: Some(80.0 - t),
                score: Some(1.0 - 0.02 * t),
                max_thickness_frac: Some(0.10 + 0.002 * t),
                tank_volume_m3: Some(10.0 + 0.2 * t),
                tank_capacity_kg: Some(8000.0 + 100.0 * t),
                robustness: Some(0.9 - 0.005 * t),
                refined: i < 10,
                l_over_d_3d: (i < 10).then_some(20.0 - 0.3 * t),
                ..Default::default()
            }
        })
        .collect();
    alas_screen::types::AirfoilScreeningResult {
        n_total: 30,
        n_ok: 30,
        n_refined: 10,
        candidates,
        ..Default::default()
    }
}

#[test]
fn r39_screening_results_render() {
    for (width, height) in [
        (900.0_f32, 1500.0_f32),
        (1400.0, 1500.0),
        (1100.0, 1500.0),
        (2400.0, 1400.0),
    ] {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        state.screening.result = Some(screening_result());
        let shot = render(width, height, 12, 60, |ui| {
            super::screening_view::show_screening_view(&mut state, ui);
        });
        write_in(
            &crate::test_raster::render_dir(),
            &format!("screening-{width:.0}.png"),
            &shot,
        );
    }
}

/// Sibling cards are laid out one under another on the page grid (one form
/// grid): they share one left and one right edge, and none overlaps
/// the next.
#[test]
fn r39_sibling_cards_share_the_page_column() {
    for width in [1250.0_f32, 2400.0] {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        let page = crate::nav::page("mission_advanced").expect("page");
        let shot = render(width, 700.0, 8, 0, |ui| {
            crate::detail::publish(ui.ctx(), crate::detail::Detail::Advanced);
            ui.set_width(width - 40.0);
            crate::views::form_page::show_form_page(&mut state, ui, page);
        });
        write_in(
            &crate::test_raster::render_dir(),
            &format!("mission-pair-{width:.0}.png"),
            &shot,
        );
        let mut cards: Vec<egui::Rect> = shot
            .rects
            .iter()
            .copied()
            // Page-level cards: not the page background the render paints first,
            // nor a card nested in one.
            .filter(|r| {
                r.left() < 1.0
                    && r.height() > 60.0
                    && r.height() < 600.0
                    && r.width() > width - 100.0
            })
            .collect();
        cards.sort_by(|a, b| a.top().total_cmp(&b.top()));
        cards.dedup_by(|a, b| (a.top() - b.top()).abs() < 1.0);
        assert!(cards.len() >= 2, "{width}: sibling cards: {cards:?}");
        for pair in cards.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert!(
                (a.left() - b.left()).abs() < 1.0,
                "{width}: lefts {a:?} {b:?}"
            );
            assert!(
                (a.right() - b.right()).abs() < 1.0,
                "{width}: rights {a:?} {b:?}"
            );
            assert!(b.top() >= a.bottom() - 0.5, "{width}: overlap {a:?} {b:?}");
        }
    }
}
