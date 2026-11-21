// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py, figure_mission_profile (L4365-4447)

//! Four-panel mission analysis model mission profile: altitude, mass, true airspeed and SFC
//! vs. time, plus a fuel-burned/block-time footer.
//!
//! Mirrors `mission analysis model.Plots.Performance.Mission_Plots`' own
//! `plot_flight_conditions`/`plot_altitude_sfc_weight`. Each panel reads a
//! column `export_data.py` writes: `Altitude_m` is `c.freestream.altitude`,
//! `Mass_kg` is `c.weights.total_mass`, `TAS_m_s` is `c.freestream.velocity`,
//! and `SFC_kg_kgf_hr` is computed inline there as
//! `(mdot * 3600) / (thrust / g0)`, which is not a stored mission analysis model field.

use super::{
    collect_series, draw_time_axis_label, draw_time_panel, draw_time_panel_named, time_domain, G0,
};
use crate::scene::{Color, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_mission::solve::MissionResult;
use alas_mission::SegmentKind;

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

/// Four-panel altitude/mass/TAS/SFC mission profile. The caption compares the peak
/// altitude with the highest altitude the mission's own segments target.
pub fn figure_mission_profile(mission: &MissionResult, theme: Option<&str>) -> Scene {
    figure_mission_profile_vs_configured(mission, None, theme)
}

/// [`figure_mission_profile`] with the design requirement's cruise altitude, m, for the
/// caption: the mission may fly a lower cruise than configured (a short route never
/// reaches it), and the segment targets alone cannot show that.
pub fn figure_mission_profile_vs_configured(
    mission: &MissionResult,
    configured_cruise_m: Option<f64>,
    theme: Option<&str>,
) -> Scene {
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
    // stored mission analysis model column. NaN (which draw_time_panel breaks
    // the line at rather than connecting across) where the ratio carries no
    // information: thrust below 5 % of the mission's maximum (idle, or negative
    // in descent) or fuel flow at idle/zero. Plot only; the data is unchanged.
    let sfc_rect = panel_rect(3);
    let (max_thrust_n, max_flow_kg_s) = mission
        .segments
        .iter()
        .flat_map(|segment| {
            let c = &segment.conditions;
            (0..c.len()).map(move |i| (c.thrust_force_vector_n[i][0], c.vehicle_mass_rate_kg_s[i]))
        })
        .fold((0.0_f64, 0.0_f64), |(t, f), (ti, fi)| {
            (t.max(ti), f.max(fi.abs()))
        });
    let sfc = collect_series(mission, |c, i| {
        sfc_kg_per_kgf_hr(
            c.thrust_force_vector_n[i][0],
            c.vehicle_mass_rate_kg_s[i],
            max_thrust_n,
            max_flow_kg_s,
        )
    });
    // Points the propulsion deck answered outside its evidence domain are
    // extrapolated thrust and fuel flow: drawn dashed in a second colour with
    // a legend entry (never dropped, and never drawn as a valid curve).
    let extrapolated = collect_series(mission, |c, i| {
        if c.propulsion_extrapolated.get(i).copied().unwrap_or(false) {
            1.0
        } else {
            0.0
        }
    });
    let any_extrapolated = extrapolated.iter().any(|p| p.1 > 0.5);
    if any_extrapolated {
        let split = |keep_extrapolated: bool| -> Vec<(f64, f64)> {
            sfc.iter()
                .zip(&extrapolated)
                .map(|(&(t, v), &(_, flag))| {
                    let keep = (flag > 0.5) == keep_extrapolated;
                    (t, if keep { v } else { f64::NAN })
                })
                .collect()
        };
        let (in_domain, out_of_domain) = (split(false), split(true));
        let solid = Stroke::new(Color::from_hex("tab:orange"), 1.5);
        let dashed = Stroke::dashed(Color::from_hex("#c8c8c8"), 1.5, 4.0, 3.0);
        draw_time_panel_named(
            &mut scene,
            pal,
            sfc_rect,
            x_range,
            &[
                (in_domain.as_slice(), solid.clone(), None),
                (
                    out_of_domain.as_slice(),
                    dashed.clone(),
                    Some(SFC_EXTRAPOLATED_NAME),
                ),
            ],
            "SFC (kg/kgf-hr)",
            9.5,
        );
        crate::chart_kit::draw_legend(
            &mut scene,
            [sfc_rect.0 + 8.0, sfc_rect.1 + 6.0],
            &[
                (
                    "SFC, deck in domain".to_owned(),
                    crate::chart_kit::LegendMarker::Line(solid),
                ),
                (
                    SFC_EXTRAPOLATED_NAME.to_owned(),
                    crate::chart_kit::LegendMarker::Line(dashed),
                ),
            ],
            pal,
            8.0,
        );
    } else {
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
    }
    draw_time_axis_label(&mut scene, pal, sfc_rect);

    // Fuel-burned / block-time footer, centered under the panel stack.
    let fuel_kg = mission.fuel_burned_kg();
    let block_h = mission.block_time_s() / 3600.0;
    let (caption, warn) = altitude_caption(mission, configured_cruise_m);
    scene.add(SceneElement::Text {
        text: caption,
        pos: [CANVAS_W * 0.5, CANVAS_H - 40.0],
        font_size: 9.0,
        color: Color::from_hex(if warn { "tab:orange" } else { pal.tick }),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: warn,
    });
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

/// Caption comparing the peak altitude actually flown with the configured cruise
/// altitude (highest climb target or cruise hold in the segment specs), and
/// whether the route is too short to reach it. The flag only labels; the data
/// is never clipped.
fn altitude_caption(mission: &MissionResult, requirement_m: Option<f64>) -> (String, bool) {
    let peak_m = mission
        .segments
        .iter()
        .flat_map(|s| s.conditions.altitude_m.iter().copied())
        .filter(|v| v.is_finite())
        .fold(f64::NEG_INFINITY, f64::max);
    let configured_m = mission
        .segments
        .iter()
        .filter_map(|s| match s.spec.kind {
            SegmentKind::Climb { altitude_end_m, .. } => Some(altitude_end_m),
            SegmentKind::Cruise { altitude_m, .. } => altitude_m,
            SegmentKind::Descent { .. } => None,
        })
        .fold(f64::NEG_INFINITY, f64::max);
    let (configured_m, source) = match requirement_m.filter(|v| v.is_finite() && *v > 0.0) {
        Some(v) => (v, "configured cruise"),
        None => (configured_m, "mission-spec cruise"),
    };
    if !peak_m.is_finite() || !configured_m.is_finite() || configured_m <= 0.0 {
        return (
            "Peak altitude vs configured cruise: not available".to_owned(),
            false,
        );
    }
    let (peak_ft, cfg_ft) = (peak_m / 0.3048, configured_m / 0.3048);
    let mut text = format!(
        "Peak altitude {} ft vs {source} {} ft ({:.0} %)",
        crate::numfmt::grouped(peak_ft, 0),
        crate::numfmt::grouped(cfg_ft, 0),
        100.0 * peak_m / configured_m
    );
    let short = peak_m < 0.95 * configured_m;
    let over = peak_m > 1.02 * configured_m;
    if short {
        text.push_str(&format!(
            " - route too short to reach the {source} altitude"
        ));
    } else if over {
        text.push_str(&format!(" - peak exceeds the {source} altitude"));
    }
    (text, short || over)
}

/// Name of the SFC series (and legend entry) drawn where the propulsion deck
/// was outside its evidence domain.
const SFC_EXTRAPOLATED_NAME: &str = "SFC, deck extrapolated";

/// Thrust or fuel flow below this fraction of the mission maximum is idle.
const SFC_IDLE_FRACTION: f64 = 0.05;

/// SFC in kg fuel / (kgf . hr), or NaN (a gap in the curve) where thrust is
/// below 5 % of `max_thrust_n` or the fuel flow is at idle (below 5 % of
/// `max_flow_kg_s`), where fuel flow / thrust is a near-0/0 ratio.
fn sfc_kg_per_kgf_hr(thrust_n: f64, flow_kg_s: f64, max_thrust_n: f64, max_flow_kg_s: f64) -> f64 {
    if !thrust_n.is_finite()
        || !flow_kg_s.is_finite()
        || thrust_n <= 0.0
        || thrust_n < SFC_IDLE_FRACTION * max_thrust_n
        || flow_kg_s.abs() <= SFC_IDLE_FRACTION * max_flow_kg_s
    {
        return f64::NAN;
    }
    (flow_kg_s * 3600.0) / (thrust_n / G0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::mission::test_support::sample_mission;

    #[test]
    fn sfc_leaves_a_gap_at_idle_thrust_and_idle_flow() {
        assert!(sfc_kg_per_kgf_hr(0.0, 0.3, 40_000.0, 1.0).is_nan());
        assert!(sfc_kg_per_kgf_hr(-500.0, 0.3, 40_000.0, 1.0).is_nan());
        assert!(sfc_kg_per_kgf_hr(700.0, 0.3, 40_000.0, 1.0).is_nan());
        assert!(sfc_kg_per_kgf_hr(20_000.0, 0.01, 40_000.0, 1.0).is_nan());
        let cruise = sfc_kg_per_kgf_hr(20_000.0, 0.3, 40_000.0, 1.0);
        assert!((cruise - 0.3 * 3600.0 / (20_000.0 / G0)).abs() < 1e-12);
    }
    use crate::svg::render_svg;

    /// Every panel of the stack exports the shared time axis (minutes, 60 s
    /// each) and its own quantity, not the next panel's title.
    #[test]
    fn plot_data_labels_each_panel_with_the_time_axis_and_its_own_unit() {
        let mission = sample_mission();
        let data = crate::plot_data::with_capture(|| {
            let scene = figure_mission_profile(&mission, None);
            crate::plot_data::PlotData::from_scene(&scene, "mission_profile", None).unwrap()
        });
        assert_eq!(data.panels.len(), 4);
        for (panel, y_unit) in data.panels.iter().zip(["ft", "t", "kt", "kg/kgf-hr"]) {
            assert_eq!(panel.x_label.as_deref(), Some("Time (min)"));
            assert_eq!(panel.x_unit.as_deref(), Some("min"));
            assert_eq!(panel.x_si.as_ref().map(|c| c.factor), Some(60.0));
            assert_eq!(panel.y_unit.as_deref(), Some(y_unit));
        }
    }

    #[test]
    fn caption_compares_the_peak_with_the_configured_cruise_altitude() {
        // Sample mission: cruise held at 10 000 m, but the points climb to 11 000 m.
        let mut mission = sample_mission();
        let (text, warn) = altitude_caption(&mission, None);
        assert!(text.contains("mission-spec cruise 32 808 ft"), "{text}");
        assert!(warn && text.contains("exceeds"), "{text}");
        // A route that never reaches the configured altitude is called short.
        for segment in &mut mission.segments {
            for h in &mut segment.conditions.altitude_m {
                *h *= 0.5;
            }
        }
        let (text, warn) = altitude_caption(&mission, Some(11_277.6));
        assert!(
            warn && text.contains("too short") && text.contains("configured cruise 37000 ft"),
            "{text}"
        );
    }

    #[test]
    fn sfn_gap_starts_at_five_percent_thrust() {
        // 4 % of maximum thrust is a gap; 6 % is a value.
        assert!(sfc_kg_per_kgf_hr(1_600.0, 0.3, 40_000.0, 1.0).is_nan());
        assert!(sfc_kg_per_kgf_hr(2_400.0, 0.3, 40_000.0, 1.0).is_finite());
    }

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
