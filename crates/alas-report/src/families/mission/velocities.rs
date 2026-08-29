// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py, figure_mission_velocities
// (L4448-4488) and figure_mission_flight_path (L4489-4522).
// Reference: alas @ rust-port-baseline.

//! Two-panel airspeed (TAS+EAS overlaid, Mach) and two-panel flight path
//! (cumulative range, pitch angle) vs. time.
//!
//! `EAS_m_s` is not a stored mission analysis model field: `export_data.py` computes it inline
//! as `tas * sqrt(density / 1.225)`, the equivalent-airspeed definition
//! mission analysis model's own comment attributes to sea-level reference density. `Range_m`
//! is `c.frames.inertial.aircraft_range` and `Pitch_deg` is
//! `c.frames.body.inertial_rotations[:, 1]` (index 1 = pitch, of
//! `[roll, pitch, yaw]`) converted from radians.

use super::{
    collect_series, draw_time_axis_label, draw_time_panel, draw_time_panel_multi, time_domain,
    RHO_SL,
};
use crate::chart_kit::{draw_legend, LegendMarker};
use crate::scene::{Color, Scene, Stroke};
use crate::theme::get_palette;
use alas_mission::solve::MissionResult;

const CANVAS_W: f64 = 650.0;
const CANVAS_H: f64 = 550.0;
const PANEL_LEFT: f64 = 70.0;
const PANEL_WIDTH: f64 = 540.0;
const PANEL_HEIGHT: f64 = 190.0;
const PANEL_GAP: f64 = 60.0;
const PANEL_TOP0: f64 = 50.0;

fn top_rect() -> (f64, f64, f64, f64) {
    (PANEL_LEFT, PANEL_TOP0, PANEL_WIDTH, PANEL_HEIGHT)
}

fn bottom_rect() -> (f64, f64, f64, f64) {
    (
        PANEL_LEFT,
        PANEL_TOP0 + PANEL_HEIGHT + PANEL_GAP,
        PANEL_WIDTH,
        PANEL_HEIGHT,
    )
}

/// Two-panel TAS+EAS (overlaid) and Mach vs. time.
pub fn figure_mission_velocities(mission: &MissionResult, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(CANVAS_W, CANVAS_H, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Airspeeds".to_owned());

    let x_range = time_domain(mission);

    let tas_kt = collect_series(mission, |c, i| c.velocity_m_s[i] * 1.943844);
    let eas_kt = collect_series(mission, |c, i| {
        c.velocity_m_s[i] * (c.density_kg_m3[i] / RHO_SL).sqrt() * 1.943844
    });
    let tas_stroke = Stroke::new(Color::from_hex("tab:green"), 1.5);
    let eas_stroke = Stroke::new(Color::from_hex("tab:blue"), 1.5);
    let top = top_rect();
    draw_time_panel_multi(
        &mut scene,
        pal,
        top,
        x_range,
        &[(&tas_kt, tas_stroke.clone()), (&eas_kt, eas_stroke.clone())],
        "Airspeed (kt)",
        9.5,
    );
    draw_legend(
        &mut scene,
        [top.0 + top.2 - 90.0, top.1 + 8.0],
        &[
            ("TAS".to_owned(), LegendMarker::Line(tas_stroke)),
            ("EAS".to_owned(), LegendMarker::Line(eas_stroke)),
        ],
        pal,
        8.0,
    );

    let mach = collect_series(mission, |c, i| c.mach[i]);
    let bottom = bottom_rect();
    draw_time_panel(
        &mut scene,
        pal,
        bottom,
        x_range,
        &mach,
        Stroke::new(Color::from_hex("tab:purple"), 1.5),
        "Mach",
        9.5,
    );
    draw_time_axis_label(&mut scene, pal, bottom);

    scene
}

/// Two-panel cumulative range (nm) and pitch angle (deg) vs. time.
pub fn figure_mission_flight_path(mission: &MissionResult, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(CANVAS_W, CANVAS_H, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Flight Path".to_owned());

    let x_range = time_domain(mission);

    // Nautical miles, 1852 m/nm.
    let range_nm = collect_series(mission, |c, i| c.aircraft_range_m[i] / 1852.0);
    draw_time_panel(
        &mut scene,
        pal,
        top_rect(),
        x_range,
        &range_nm,
        Stroke::new(Color::from_hex("tab:blue"), 1.5),
        "Range (nm)",
        9.5,
    );

    let pitch_deg = collect_series(mission, |c, i| {
        c.body_inertial_rotations_rad[i][1].to_degrees()
    });
    let bottom = bottom_rect();
    draw_time_panel(
        &mut scene,
        pal,
        bottom,
        x_range,
        &pitch_deg,
        Stroke::new(Color::from_hex("tab:red"), 1.5),
        "Pitch angle (deg)",
        9.5,
    );
    draw_time_axis_label(&mut scene, pal, bottom);

    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::mission::test_support::sample_mission;
    use crate::svg::render_svg;

    #[test]
    fn eas_diverges_from_tas_by_the_sqrt_density_ratio() {
        let mission = sample_mission();
        // First control point: TAS 230 m/s, density 0.4127 kg/m^3.
        let tas = collect_series(&mission, |c, i| c.velocity_m_s[i] * 1.943844);
        let eas = collect_series(&mission, |c, i| {
            c.velocity_m_s[i] * (c.density_kg_m3[i] / RHO_SL).sqrt() * 1.943844
        });
        let expected_eas = 230.0 * (0.4127_f64 / RHO_SL).sqrt() * 1.943844;
        assert!((eas[0].1 - expected_eas).abs() < 1e-9);
        assert!(eas[0].1 < tas[0].1);
    }

    #[test]
    fn pitch_reads_index_one_of_the_body_rotation_in_degrees() {
        let mission = sample_mission();
        let pitch = collect_series(&mission, |c, i| {
            c.body_inertial_rotations_rad[i][1].to_degrees()
        });
        assert!((pitch[0].1 - 0.02_f64.to_degrees()).abs() < 1e-9);
    }

    #[test]
    fn range_converts_metres_to_nautical_miles() {
        let mission = sample_mission();
        let range_nm = collect_series(&mission, |c, i| c.aircraft_range_m[i] / 1852.0);
        assert!((range_nm[0].1 - 0.0).abs() < 1e-9);
        assert!((range_nm[2].1 - 277_000.0 / 1852.0).abs() < 1e-9);
    }

    #[test]
    fn both_figures_render_two_panels_with_a_legend_on_the_first() {
        let mission = sample_mission();
        let sc_vel = figure_mission_velocities(&mission, Some("light"));
        let svg_vel = render_svg(&sc_vel);
        assert!(svg_vel.contains("polyline"));
        assert!(svg_vel.contains("TAS"));
        assert!(svg_vel.contains("EAS"));

        let sc_path = figure_mission_flight_path(&mission, Some("light"));
        assert!(render_svg(&sc_path).contains("polyline"));
    }
}
