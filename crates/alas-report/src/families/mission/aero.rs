// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py, figure_mission_aero_coefficients
// (L4523-4568) and figure_mission_aero_forces (L4569-4616).
// Reference: alas @ rust-port-baseline.

//! Two 2x2-panel mission timelines: angle of attack/CL/CD/L-over-D, and
//! throttle/lift/thrust/drag.
//!
//! Every panel reads a column `export_data.py` writes off
//! `segment.conditions...`: `AoA_deg` is `c.aerodynamics.angle_of_attack`
//! converted from radians, `CL`/`CD` are `c.aerodynamics.lift_coefficient`/
//! `drag_coefficient` directly, `L_over_D` is `CL / CD` (`nan` where `CD` is
//! zero, matching `export_data.py`'s own guard), `Throttle` is
//! `c.propulsion.throttle`, and `Lift_N`/`Drag_N`/`Thrust_N` are the wind- and
//! body-frame force components -- lift and drag negated because mission analysis model stores
//! them along `-z`/`-x` in their own frames.

use super::{collect_series, draw_time_axis_label, draw_time_panel, time_domain};
use crate::scene::{Color, Scene, Stroke};
use crate::theme::get_palette;
use alas_mission::solve::MissionResult;

const CANVAS_W: f64 = 750.0;
const CANVAS_H: f64 = 600.0;
const LEFT: f64 = 70.0;
const TOP: f64 = 60.0;
const HGAP: f64 = 60.0;
const VGAP: f64 = 70.0;
const PANEL_W: f64 = 295.0;
const PANEL_H: f64 = 205.0;

/// The four panel rects, in `(top-left, top-right, bottom-left, bottom-right)`
/// order, matching Python's `(ax_a, ax_b), (ax_c, ax_d) = fig.subplots(2, 2)`.
fn grid_2x2() -> [(f64, f64, f64, f64); 4] {
    let col0 = LEFT;
    let col1 = LEFT + PANEL_W + HGAP;
    let row0 = TOP;
    let row1 = TOP + PANEL_H + VGAP;
    [
        (col0, row0, PANEL_W, PANEL_H),
        (col1, row0, PANEL_W, PANEL_H),
        (col0, row1, PANEL_W, PANEL_H),
        (col1, row1, PANEL_W, PANEL_H),
    ]
}

/// Four-panel angle of attack / CL / CD / L-over-D vs. time.
pub fn figure_mission_aero_coefficients(mission: &MissionResult, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(CANVAS_W, CANVAS_H, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Aerodynamic Coefficients".to_owned());

    let x_range = time_domain(mission);
    let [r_aoa, r_cl, r_cd, r_ld] = grid_2x2();

    let aoa_deg = collect_series(mission, |c, i| c.angle_of_attack_rad[i].to_degrees());
    draw_time_panel(
        &mut scene,
        pal,
        r_aoa,
        x_range,
        &aoa_deg,
        Stroke::new(Color::from_hex("tab:blue"), 1.3),
        "Angle of attack (deg)",
        9.0,
    );

    let cl = collect_series(mission, |c, i| c.lift_coefficient[i]);
    draw_time_panel(
        &mut scene,
        pal,
        r_cl,
        x_range,
        &cl,
        Stroke::new(Color::from_hex("tab:orange"), 1.3),
        "CL",
        9.0,
    );

    let cd = collect_series(mission, |c, i| c.drag_coefficient[i]);
    draw_time_panel(
        &mut scene,
        pal,
        r_cd,
        x_range,
        &cd,
        Stroke::new(Color::from_hex("tab:green"), 1.3),
        "CD",
        9.0,
    );
    draw_time_axis_label(&mut scene, pal, r_cd);

    let l_over_d = collect_series(mission, |c, i| {
        let cd = c.drag_coefficient[i];
        if cd == 0.0 {
            f64::NAN
        } else {
            c.lift_coefficient[i] / cd
        }
    });
    draw_time_panel(
        &mut scene,
        pal,
        r_ld,
        x_range,
        &l_over_d,
        Stroke::new(Color::from_hex("#9467bd"), 1.3),
        "L/D",
        9.0,
    );
    draw_time_axis_label(&mut scene, pal, r_ld);

    scene
}

/// Four-panel throttle / lift / thrust / drag vs. time.
pub fn figure_mission_aero_forces(mission: &MissionResult, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(CANVAS_W, CANVAS_H, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Aerodynamic & Propulsive Forces".to_owned());

    let x_range = time_domain(mission);
    let [r_thr, r_lift, r_thrust, r_drag] = grid_2x2();

    let throttle = collect_series(mission, |c, i| c.throttle[i]);
    draw_time_panel(
        &mut scene,
        pal,
        r_thr,
        x_range,
        &throttle,
        Stroke::new(Color::from_hex("tab:blue"), 1.3),
        "Throttle",
        9.0,
    );

    // Lift is stored along -z of the wind frame, drag along -x: both negated
    // on the way out, matching export_data.py's `-_col(..., 2)`/`-_col(..., 0)`.
    let lift_kn = collect_series(mission, |c, i| -c.wind_lift_force_vector_n[i][2] / 1000.0);
    draw_time_panel(
        &mut scene,
        pal,
        r_lift,
        x_range,
        &lift_kn,
        Stroke::new(Color::from_hex("tab:orange"), 1.3),
        "Lift (kN)",
        9.0,
    );

    let thrust_kn = collect_series(mission, |c, i| c.thrust_force_vector_n[i][0] / 1000.0);
    draw_time_panel(
        &mut scene,
        pal,
        r_thrust,
        x_range,
        &thrust_kn,
        Stroke::new(Color::from_hex("tab:green"), 1.3),
        "Thrust (kN)",
        9.0,
    );
    draw_time_axis_label(&mut scene, pal, r_thrust);

    let drag_kn = collect_series(mission, |c, i| -c.wind_drag_force_vector_n[i][0] / 1000.0);
    draw_time_panel(
        &mut scene,
        pal,
        r_drag,
        x_range,
        &drag_kn,
        Stroke::new(Color::from_hex("tab:red"), 1.3),
        "Drag (kN)",
        9.0,
    );
    draw_time_axis_label(&mut scene, pal, r_drag);

    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::mission::test_support::sample_mission;
    use crate::svg::render_svg;

    #[test]
    fn aoa_converts_radians_to_degrees() {
        let mission = sample_mission();
        let pts = collect_series(&mission, |c, i| c.angle_of_attack_rad[i].to_degrees());
        assert!((pts[0].1 - 0.03_f64.to_degrees()).abs() < 1e-9);
    }

    #[test]
    fn lift_and_drag_forces_are_negated_out_of_their_stored_frame_components() {
        let mission = sample_mission();
        let lift_kn = collect_series(&mission, |c, i| -c.wind_lift_force_vector_n[i][2] / 1000.0);
        let drag_kn = collect_series(&mission, |c, i| -c.wind_drag_force_vector_n[i][0] / 1000.0);
        // Stored as -700_000 N (lift) and -35_000 N (drag); reported positive.
        assert!((lift_kn[0].1 - 700.0).abs() < 1e-9);
        assert!((drag_kn[0].1 - 35.0).abs() < 1e-9);
    }

    #[test]
    fn l_over_d_matches_cl_over_cd() {
        let mission = sample_mission();
        let l_over_d = collect_series(&mission, |c, i| {
            c.lift_coefficient[i] / c.drag_coefficient[i]
        });
        assert!((l_over_d[0].1 - 0.5 / 0.025).abs() < 1e-9);
    }

    #[test]
    fn both_2x2_figures_render_four_panels() {
        let mission = sample_mission();
        let sc_coef = figure_mission_aero_coefficients(&mission, Some("dark"));
        let svg_coef = render_svg(&sc_coef);
        assert!(svg_coef.contains("CL"));
        assert!(svg_coef.contains("polyline"));

        let sc_forces = figure_mission_aero_forces(&mission, Some("dark"));
        let svg_forces = render_svg(&sc_forces);
        assert!(svg_forces.contains("Throttle"));
        assert!(svg_forces.contains("polyline"));
    }
}
