// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py, figure_mission_profile (L4365-4447)
// Reference: alas @ rust-port-baseline.

//! Four-panel mission analysis model mission profile: altitude, mass, true airspeed and SFC
//! vs. time, plus a fuel-burned/block-time footer.
//!
//! Mirrors `mission analysis model.Plots.Performance.Mission_Plots`' own
//! `plot_flight_conditions`/`plot_altitude_sfc_weight`. Each panel reads a
//! column `export_data.py` writes: `Altitude_m` is `c.freestream.altitude`,
//! `Mass_kg` is `c.weights.total_mass`, `TAS_m_s` is `c.freestream.velocity`,
//! and `SFC_kg_kgf_hr` is computed inline there as
//! `(mdot * 3600) / (thrust / g0)`, which is not a stored mission analysis model field.

use super::{collect_series, draw_time_axis_label, draw_time_panel, time_domain, G0};
use crate::scene::{Color, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_mission::solve::MissionResult;

const CANVAS_W: f64 = 720.0;
const CANVAS_H: f64 = 960.0;
const PANEL_LEFT: f64 = 70.0;
const PANEL_WIDTH: f64 = 620.0;
const PANEL_HEIGHT: f64 = 160.0;
const PANEL_GAP: f64 = 46.0;
const PANEL_TOP0: f64 = 60.0;

fn panel_rect(index: usize) -> (f64, f64, f64, f64) {
    (
        PANEL_LEFT,
        PANEL_TOP0 + index as f64 * (PANEL_HEIGHT + PANEL_GAP),
        PANEL_WIDTH,
        PANEL_HEIGHT,
    )
}

/// Four-panel altitude/mass/TAS/SFC mission profile.
pub fn figure_mission_profile(mission: &MissionResult, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(CANVAS_W, CANVAS_H, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Mission Profile".to_owned());

    let x_range = time_domain(mission);

    // Altitude, feet (0.3048 m/ft, the same conversion `_isa_atmo_functions`'s
    // callers already use elsewhere in this codebase).
    let altitude_ft = collect_series(mission, |c, i| c.altitude_m[i] / 0.3048);
    draw_time_panel(
        &mut scene,
        pal,
        panel_rect(0),
        x_range,
        &altitude_ft,
        Stroke::new(Color::from_hex("tab:blue"), 1.5),
        "Altitude (ft)",
        9.5,
    );

    // Total mass, tonnes: matches Python's choice to avoid clipping 6-digit
    // kg tick labels at a narrow embedded canvas width (see the Python
    // source's comment on this exact panel).
    let mass_t = collect_series(mission, |c, i| c.total_mass_kg[i] / 1000.0);
    draw_time_panel(
        &mut scene,
        pal,
        panel_rect(1),
        x_range,
        &mass_t,
        Stroke::new(Color::from_hex("tab:red"), 1.5),
        "Total mass (t)",
        9.5,
    );

    // True airspeed, knots (1.943844 kt per m/s).
    let tas_kt = collect_series(mission, |c, i| c.velocity_m_s[i] * 1.943844);
    draw_time_panel(
        &mut scene,
        pal,
        panel_rect(2),
        x_range,
        &tas_kt,
        Stroke::new(Color::from_hex("tab:green"), 1.5),
        "True airspeed (kt)",
        9.5,
    );

    // SFC, kg fuel / (kgf . hr): export_data.py's inline formula, not a
    // stored mission analysis model column. NaN (Python's `float("nan")`) where thrust is
    // zero, which draw_time_panel breaks the line at rather than connecting
    // across.
    let sfc_rect = panel_rect(3);
    let sfc = collect_series(mission, |c, i| {
        let thrust_n = c.thrust_force_vector_n[i][0];
        if thrust_n == 0.0 {
            f64::NAN
        } else {
            (c.vehicle_mass_rate_kg_s[i] * 3600.0) / (thrust_n / G0)
        }
    });
    draw_time_panel(
        &mut scene,
        pal,
        sfc_rect,
        x_range,
        &sfc,
        Stroke::new(Color::from_hex("tab:orange"), 1.5),
        "SFC (kg/kgf-hr)",
        9.5,
    );
    draw_time_axis_label(&mut scene, pal, sfc_rect);

    // Fuel-burned / block-time footer, centered under the panel stack.
    let fuel_kg = mission.fuel_burned_kg();
    let block_h = mission.block_time_s() / 3600.0;
    scene.add(SceneElement::Text {
        text: format!("Fuel burned: {fuel_kg:.0} kg   |   Block time: {block_h:.2} h"),
        pos: [CANVAS_W * 0.5, CANVAS_H - 22.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });

    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::mission::test_support::sample_mission;
    use crate::svg::render_svg;

    #[test]
    fn altitude_panel_converts_metres_to_feet_at_the_first_control_point() {
        let mission = sample_mission();
        let pts = collect_series(&mission, |c, i| c.altitude_m[i] / 0.3048);
        // 10_000 m / 0.3048 m/ft.
        assert!((pts[0].1 - 32_808.398_950_131_23).abs() < 1e-6);
    }

    #[test]
    fn sfc_uses_the_export_data_formula_and_the_footer_reads_real_fuel_and_time() {
        let mission = sample_mission();
        // First control point: mdot=0.35 kg/s, thrust=36_000 N.
        let expected_sfc = (0.35 * 3600.0) / (36_000.0 / G0);
        let sfc = collect_series(&mission, |c, i| {
            (c.vehicle_mass_rate_kg_s[i] * 3600.0) / (c.thrust_force_vector_n[i][0] / G0)
        });
        assert!((sfc[0].1 - expected_sfc).abs() < 1e-9);

        // Two 400 kg-per-segment burns (68_000 -> 67_600 each), joined.
        assert!((mission.fuel_burned_kg() - 800.0).abs() < 1e-9);
        assert!(mission.block_time_s() > 0.0);
    }

    #[test]
    fn renders_four_panels_and_a_footer_line() {
        let mission = sample_mission();
        let scene = figure_mission_profile(&mission, Some("dark"));
        let svg = render_svg(&scene);
        // Four panel titles + one footer text, at minimum.
        assert!(svg.matches("<text").count() >= 5);
        assert!(svg.contains("polyline"));
        let footer_y = scene.elements.iter().find_map(|element| match element {
            SceneElement::Text { text, pos, .. } if text.starts_with("Fuel burned:") => {
                Some(pos[1])
            }
            _ => None,
        });
        assert!(footer_y.is_some_and(|y| y <= scene.height - 20.0));
    }
}
