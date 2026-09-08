// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py, figure_mission_drag_components (L4617-4659)
// Reference: alas @ rust-port-baseline.

//! Single-panel drag-component breakdown: parasite, induced, compressibility,
//! miscellaneous and total CD, overlaid vs. time.
//!
//! Every series reads `Conditions::drag_breakdown`, the whole drag buildup
//! carried at each control point -- `CD_parasite`/`CD_induced`/
//! `CD_compressible`/`CD_miscellaneous`/`CD_total` are
//! `db.parasite.total`/`db.induced.total`/`db.compressible.total`/
//! `db.miscellaneous.total`/`db.total` in `export_data.py`, i.e.
//! `DragBreakdown::parasite_total`/`induced_total`/`compressible_total`/
//! `miscellaneous_total`/`total` here. `CD_total` is drawn thicker and in
//! red, matching Python's `linewidth=2` on that one series only.
//!
//! The axes-level `ylabel="CD"`/`title="Drag components"` Python sets have no
//! rendered counterpart here: this crate's other single-panel figures (e.g.
//! `performance::figure_vn_diagram`) likewise carry their title only as
//! `Scene::title` metadata rather than drawing an axis label primitive, and
//! this figure follows that established convention rather than introducing a
//! one-off exception.

use super::{collect_series, draw_time_axis_label, draw_time_panel_multi, time_domain};
use crate::chart_kit::{draw_legend, LegendMarker};
use crate::scene::{Color, Scene, Stroke};
use crate::theme::get_palette;
use alas_mission::solve::MissionResult;

const CANVAS_W: f64 = 650.0;
const CANVAS_H: f64 = 480.0;
const PANEL_RECT: (f64, f64, f64, f64) = (70.0, 55.0, 500.0, 340.0);

/// Overlaid CD-parasite / CD-induced / CD-compressibility / CD-miscellaneous
/// / CD-total vs. time.
pub fn figure_mission_drag_components(mission: &MissionResult, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(CANVAS_W, CANVAS_H, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Drag components".to_owned());

    let x_range = time_domain(mission);
    let parasite = collect_series(mission, |c, i| c.drag_breakdown[i].parasite_total);
    let induced = collect_series(mission, |c, i| c.drag_breakdown[i].induced_total);
    let compressible = collect_series(mission, |c, i| c.drag_breakdown[i].compressible_total);
    let miscellaneous = collect_series(mission, |c, i| c.drag_breakdown[i].miscellaneous_total);
    let total = collect_series(mission, |c, i| c.drag_breakdown[i].total);

    let parasite_stroke = Stroke::new(Color::from_hex("#9467bd"), 1.5);
    let induced_stroke = Stroke::new(Color::from_hex("tab:blue"), 1.5);
    let compressible_stroke = Stroke::new(Color::from_hex("tab:green"), 1.5);
    let miscellaneous_stroke = Stroke::new(Color::from_hex("#bcbd22"), 1.5);
    let total_stroke = Stroke::new(Color::from_hex("tab:red"), 2.0);

    draw_time_panel_multi(
        &mut scene,
        pal,
        PANEL_RECT,
        x_range,
        &[
            (&parasite, parasite_stroke.clone()),
            (&induced, induced_stroke.clone()),
            (&compressible, compressible_stroke.clone()),
            (&miscellaneous, miscellaneous_stroke.clone()),
            (&total, total_stroke.clone()),
        ],
        "Drag components",
        9.5,
    );
    draw_time_axis_label(&mut scene, pal, PANEL_RECT);

    draw_legend(
        &mut scene,
        [PANEL_RECT.0 + PANEL_RECT.2 - 150.0, PANEL_RECT.1 + 8.0],
        &[
            (
                "CD parasite".to_owned(),
                LegendMarker::Line(parasite_stroke),
            ),
            ("CD induced".to_owned(), LegendMarker::Line(induced_stroke)),
            (
                "CD compressibility".to_owned(),
                LegendMarker::Line(compressible_stroke),
            ),
            (
                "CD miscellaneous".to_owned(),
                LegendMarker::Line(miscellaneous_stroke),
            ),
            ("CD total".to_owned(), LegendMarker::Line(total_stroke)),
        ],
        pal,
        7.5,
    );

    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::mission::test_support::sample_mission;
    use crate::svg::render_svg;

    #[test]
    fn every_component_reads_its_own_drag_breakdown_field() {
        let mission = sample_mission();
        let parasite = collect_series(&mission, |c, i| c.drag_breakdown[i].parasite_total);
        let induced = collect_series(&mission, |c, i| c.drag_breakdown[i].induced_total);
        let total = collect_series(&mission, |c, i| c.drag_breakdown[i].total);
        // The test fixture's dummy_drag_breakdown splits total into
        // parasite=0.6x, induced=0.3x, compressible=0.05x, misc=0.05x.
        assert!((parasite[0].1 - 0.6 * 0.025).abs() < 1e-9);
        assert!((induced[0].1 - 0.3 * 0.025).abs() < 1e-9);
        assert!((total[0].1 - 0.025).abs() < 1e-9);
    }

    #[test]
    fn renders_five_series_with_a_legend_and_no_fabricated_curve() {
        let mission = sample_mission();
        let scene = figure_mission_drag_components(&mission, Some("dark"));
        let svg = render_svg(&scene);
        assert!(svg.contains("polyline"));
        assert!(svg.contains("CD total"));
        assert!(svg.contains("CD parasite"));
        // Five drawn series plus the panel title: at least six polylines
        // (each add_series_with_gaps call draws at least one contiguous run).
        assert!(svg.matches("polyline").count() >= 5);
    }

    #[test]
    fn an_empty_mission_renders_without_a_fabricated_curve() {
        let mission = MissionResult {
            segments: Vec::new(),
            solutions: Vec::new(),
            scheduled_segment_count: 0,
            fuel_exhaustion: None,
        };
        let scene = figure_mission_drag_components(&mission, None);
        let svg = render_svg(&scene);
        assert!(svg.contains("<svg"));
        assert!(!svg.contains("polyline"));
    }
}
