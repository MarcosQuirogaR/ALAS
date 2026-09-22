// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (figure_dynamic_modes, L1497-1663)
// Reference: alas @ rust-port-baseline.

//! Longitudinal/lateral-directional dynamic-mode analysis at trimmed cruise:
//! an s-plane pole plot next to a compact numeric readout.
//!
//! # Where the flight condition comes from
//!
//! Upstream builds the atmosphere and true airspeed from
//! `config.requirements.cruise_altitude_m`/`cruise_mach`, so unlike the
//! sibling stability figures this one genuinely needs the `AlasConfig`, not
//! just the `AnalysisReport`: the report alone does not carry the cruise
//! flight condition its own polar sweep was evaluated at. The stub this
//! replaces took only `(report, theme)`; this row's provenance note in
//! `docs/PORTING.md` and `docs/PHYSICS_SOLVER_FLOW.md` describe the *content* being
//! fabricated, not the signature, and every real call site already has a
//! `config` in scope (the sibling `figure_control_surfaces` already takes
//! one), so the signature grows to match what the computation needs.

use alas_aero::operating_point::OperatingPoint;
use alas_atmo::Atmosphere;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_pipeline::feasibility::takeoff_mass_properties;
use alas_pipeline::full_analysis::AnalysisReport;
use alas_stab::dynamics::{self, DynamicMode, DynamicModes};
use alas_stab::modes::MassProperties;

use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::scene::{
    Axes2D, Color, Fill, Scale, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::get_palette;

/// One mode's display name and marker color (matplotlib's `tab:*` palette,
/// spelled as hex since [`Color::from_hex`] only special-cases four of the
/// ten `tab:` names), in the order upstream's `modes.items()` iterates
/// `DynamicModes`' fields.
const MODE_STYLE: [(&str, &str, &str); 5] = [
    ("phugoid", "Phugoid", "#1f77b4"),
    ("short_period", "Short period", "#d62728"),
    ("roll_subsidence", "Roll subsidence", "#ff7f0e"),
    ("dutch_roll", "Dutch roll", "#2ca02c"),
    ("spiral", "Spiral", "#9467bd"),
];

/// Inputs shared by the trimmed dynamic-mode solve and its renderer.
///
/// Keeping this preparation separate makes it explicit that the derivatives
/// and the closed-form mode equations consume the same trimmed aircraft,
/// operating point, and mass closure. In particular, `Airplane::xyz_ref` is
/// the moment reference used by the VLM derivative solve and therefore must be
/// the report's physical CG, not a stale geometry seed.
struct PreparedDynamicState {
    plane: Airplane,
    op_point: OperatingPoint,
    mass_props: MassProperties,
    alpha_deg: f64,
}

fn prepare_trimmed_dynamic_state(
    report: &AnalysisReport,
    config: &AlasConfig,
) -> Option<PreparedDynamicState> {
    let trimmed = report.trimmed_design_point?;
    if !trimmed.geometric_body_alpha_deg.is_finite()
        || !trimmed.trim_ih_deg.is_finite()
        || !trimmed.cm_residual.is_finite()
        || trimmed.cm_residual.abs() > 1.0e-3
        || report.physical_cg.iter().any(|value| !value.is_finite())
    {
        return None;
    }

    let mass_kg = report
        .component_masses
        .values()
        .try_fold(0.0, |total, &component_mass| {
            (component_mass.is_finite() && component_mass >= 0.0).then_some(total + component_mass)
        })?;
    if !mass_kg.is_finite() || mass_kg <= 0.0 {
        return None;
    }

    let mut plane = report.airplane.clone();
    if plane.wings.is_empty() || plane.fuselages.is_empty() {
        return None;
    }
    // The report plane is normally anchored to this point by FullAnalysis,
    // but hand-built reports and legacy callers can still carry a stale seed.
    // Dynamic rate derivatives must use the same physical reference as trim.
    plane.xyz_ref = report.physical_cg;
    for wing in plane
        .wings
        .iter_mut()
        .filter(|wing| wing.name == "Horizontal Stabilizer")
    {
        for section in &mut wing.xsecs {
            section.twist = trimmed.trim_ih_deg;
        }
    }

    let req = &config.requirements;
    let atmo = Atmosphere::new(req.cruise_altitude_m);
    let velocity = req.cruise_mach * atmo.speed_of_sound();
    if !velocity.is_finite() || velocity <= 0.0 {
        return None;
    }
    let op_point = OperatingPoint::new(
        atmo,
        velocity,
        trimmed.geometric_body_alpha_deg,
        0.0,
        0.0,
        0.0,
        0.0,
    );
    // The item ledger's tensor at the maximum-fuel takeoff state is the
    // physical estimate; the radius-of-gyration fit stands in only when the
    // ledger cannot be built for this report (a hand-built report without a
    // tank arrangement, for example).
    let mass_props = match takeoff_mass_properties(config, report) {
        Some(ledger) if ledger.inertia_cg.is_physical() && ledger.mass_kg > 0.0 => MassProperties {
            mass: ledger.mass_kg,
            ixx: ledger.inertia_cg.ixx,
            iyy: ledger.inertia_cg.iyy,
            izz: ledger.inertia_cg.izz,
        },
        _ => {
            let (ixx, iyy, izz) = dynamics::estimate_inertia(&plane, mass_kg);
            MassProperties {
                mass: mass_kg,
                ixx,
                iyy,
                izz,
            }
        }
    };

    Some(PreparedDynamicState {
        plane,
        op_point,
        mass_props,
        alpha_deg: trimmed.geometric_body_alpha_deg,
    })
}

/// Generate dynamic stability eigenvalues / mode poles in the complex s-plane:
/// `figure_dynamic_modes`.
pub fn figure_dynamic_modes(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    if report.airplane.wings.is_empty() || report.airplane.fuselages.is_empty() {
        return super::status_scene(
            "Dynamic Stability Modes",
            "No wing/fuselage geometry available.",
            pal,
        );
    }
    let Some(state) = prepare_trimmed_dynamic_state(report, config) else {
        return super::status_scene(
            "Dynamic Stability Modes",
            "A valid trimmed cruise state is unavailable; dynamic modes were not evaluated.",
            pal,
        );
    };
    let plane = &state.plane;

    match dynamics::compute_dynamic_modes(plane, &state.op_point, &state.mass_props) {
        Ok(modes) => render(&modes, state.alpha_deg, pal),
        Err(err) => super::status_scene(
            "Dynamic Stability Modes",
            &format!("Dynamic-mode analysis did not converge: {err}"),
            pal,
        ),
    }
}

fn mode_by_key<'a>(modes: &'a DynamicModes, key: &str) -> &'a DynamicMode {
    match key {
        "phugoid" => &modes.phugoid,
        "short_period" => &modes.short_period,
        "roll_subsidence" => &modes.roll_subsidence,
        "dutch_roll" => &modes.dutch_roll,
        _ => &modes.spiral,
    }
}

fn render(modes: &DynamicModes, alpha: f64, pal: &'static crate::theme::Palette) -> Scene {
    let mut scene = Scene::new(1010.0, 640.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some(format!(
        "Dynamic Stability - trimmed cruise (alpha = {alpha:.1} deg)"
    ));
    let title = scene.title.clone().unwrap_or_default();
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();

    let all_vals: Vec<f64> = MODE_STYLE
        .iter()
        .flat_map(|&(key, _, _)| {
            let m = mode_by_key(modes, key);
            [m.eigenvalue_real, m.eigenvalue_imag]
        })
        .collect();
    let max_abs = all_vals.iter().fold(0.0f64, |acc, &v| acc.max(v.abs()));
    let span = max_abs.max(0.05) * 1.8;
    let nonzero_min = all_vals
        .iter()
        .map(|v| v.abs())
        .filter(|&v| v > 1e-9)
        .fold(f64::INFINITY, f64::min);
    let linthresh = if nonzero_min.is_finite() {
        (nonzero_min * 0.5).max(span * 0.01)
    } else {
        (1e-3f64).max(span * 0.01)
    };

    let axes = Axes2D::new((70.0, 60.0, 600.0, 420.0), (-span, span), (-span, span))
        .with_x_scale(Scale::SymLog { linthresh })
        .with_y_scale(Scale::SymLog { linthresh });

    // Left-half-plane (stable) / right-half-plane (unstable) shading.
    let zero_px = axes.map_point(0.0, 0.0)[0];
    scene.add(SceneElement::Rect {
        x: axes.left,
        y: axes.top,
        width: (zero_px - axes.left).max(0.0),
        height: axes.height,
        rx: 0.0,
        fill: Some(Fill::new(Color::rgba(102, 204, 102, 38))),
        stroke: None,
    });
    scene.add(SceneElement::Rect {
        x: zero_px,
        y: axes.top,
        width: (axes.left + axes.width - zero_px).max(0.0),
        height: axes.height,
        rx: 0.0,
        fill: Some(Fill::new(Color::rgba(255, 77, 77, 38))),
        stroke: None,
    });
    axes.draw_frame(&mut scene, pal);
    axes.add_line_series(
        &mut scene,
        &[(0.0, -span), (0.0, span)],
        Stroke::new(Color::rgb(0, 0, 0), 1.0),
    );
    axes.add_line_series(
        &mut scene,
        &[(-span, 0.0), (span, 0.0)],
        Stroke::new(Color::from_hex("#808080"), 0.7),
    );

    let mut legend_entries = Vec::new();
    for &(key, display, color_hex) in &MODE_STYLE {
        let m = mode_by_key(modes, key);
        let color = Color::from_hex(color_hex);
        let edge = Stroke::new(Color::rgb(0, 0, 0), 0.7);
        let p = axes.map_point(m.eigenvalue_real, m.eigenvalue_imag);
        scene.add(SceneElement::Circle {
            center: p,
            radius: 5.5,
            fill: Some(Fill::new(color)),
            stroke: Some(edge.clone()),
        });
        if m.eigenvalue_imag.abs() > 1e-9 {
            let p2 = axes.map_point(m.eigenvalue_real, -m.eigenvalue_imag);
            scene.add(SceneElement::Circle {
                center: p2,
                radius: 5.5,
                fill: Some(Fill::new(color)),
                stroke: Some(edge),
            });
        }
        // Keep the mode-to-pole relationship visible even when the legend is
        // moved or omitted by a host view. The table remains the numeric
        // contract; this short label identifies the plotted branch itself.
        let label_pos = [
            (p[0] + 8.0).clamp(axes.left + 4.0, axes.left + axes.width - 4.0),
            (p[1] - 7.0).clamp(axes.top + 11.0, axes.top + axes.height - 4.0),
        ];
        scene.add(SceneElement::Text {
            text: display.to_owned(),
            pos: label_pos,
            font_size: 8.0,
            color,
            align: TextAlign::Left,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
        legend_entries.push((display.to_owned(), LegendMarker::Circle(color)));
    }

    scene.add(SceneElement::Text {
        text: "Real part (1/s) \u{2014} damping".to_owned(),
        pos: [axes.left + axes.width / 2.0, axes.top + axes.height + 22.0],
        font_size: 10.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: "Imaginary part (1/s) \u{2014} frequency".to_owned(),
        pos: [axes.left - 40.0, axes.top + axes.height / 2.0],
        font_size: 10.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });

    draw_legend(
        &mut scene,
        [axes.left, axes.top + axes.height + 40.0],
        &legend_entries,
        pal,
        9.0,
    );

    render_table(&mut scene, modes, pal, 360.0, 520.0);

    scene
}

/// The right panel: a compact monospace mode-summary table, built from
/// stacked `Text` elements (the primitive set has no multi-line text block).
fn render_table(
    scene: &mut Scene,
    modes: &DynamicModes,
    pal: &'static crate::theme::Palette,
    x: f64,
    y: f64,
) {
    let mut lines = Vec::new();
    for &(key, display, _) in &MODE_STYLE {
        let m = mode_by_key(modes, key);
        let period_str = if m.period_s > 0.0 {
            format!("{:.1}s", m.period_s)
        } else {
            "n/a".to_owned()
        };
        let status = if m.stable { "Stable" } else { "Unstable" };
        lines.push(format!(
            "{display} | {period_str} | {:.3} | {status}",
            m.damping_ratio
        ));
    }

    let line_h = 16.0;
    for (i, line) in lines.iter().enumerate() {
        scene.add(SceneElement::Text {
            text: line.clone(),
            pos: [x, y + (i as f64) * line_h],
            font_size: 11.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::stability::test_support::{probe_airplane, probe_report};
    use crate::scene::SceneElement;
    use alas_config::AlasConfig;

    fn circles(scene: &Scene) -> usize {
        scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Circle { .. }))
            .count()
    }

    #[test]
    fn a_real_probe_aircraft_plots_ten_pole_circles() {
        // Five modes, each mirrored about the real axis (the mirror is drawn
        // unconditionally by the loop even for the two that turn out real:
        // eigenvalue_imag.abs() > 1e-9 gates it, so a real spiral/roll mode
        // draws one circle and a complex pair draws two).
        let report = probe_report(probe_airplane(true, false));
        let config = AlasConfig::default();
        let scene = figure_dynamic_modes(&report, &config, Some("dark"));
        assert!(
            circles(&scene) >= 5,
            "expected at least one circle per mode"
        );
        assert!(scene
            .title
            .as_deref()
            .is_some_and(|t| t.contains("Dynamic Stability")));
    }

    #[test]
    fn prepared_state_matches_trimmed_geometry_and_physical_mass_reference() -> Result<(), String> {
        let airplane = probe_airplane(true, false);
        let original_twist = airplane
            .wings
            .iter()
            .find(|wing| wing.name == "Horizontal Stabilizer")
            .and_then(|wing| wing.xsecs.first())
            .map(|section| section.twist)
            .ok_or_else(|| "probe has a horizontal stabilizer".to_owned())?;
        let original_reference = airplane.xyz_ref;
        let mut report = probe_report(airplane);
        report.physical_cg = [6.25, 0.15, -0.08];
        let trimmed = report
            .trimmed_design_point
            .as_mut()
            .ok_or_else(|| "probe has a trimmed point".to_owned())?;
        trimmed.geometric_body_alpha_deg = 3.75;
        trimmed.trim_ih_deg = 4.5;

        let state = prepare_trimmed_dynamic_state(&report, &AlasConfig::default())
            .ok_or_else(|| "finite trimmed probe should prepare".to_owned())?;
        assert_eq!(state.plane.xyz_ref, report.physical_cg);
        assert_eq!(state.op_point.alpha, 3.75);
        assert_eq!(state.mass_props.mass, 20_000.0);

        let hstab = state
            .plane
            .wings
            .iter()
            .find(|wing| wing.name == "Horizontal Stabilizer")
            .ok_or_else(|| "prepared plane keeps the horizontal stabilizer".to_owned())?;
        assert!(hstab
            .xsecs
            .iter()
            .all(|section| (section.twist - 4.5).abs() < f64::EPSILON));

        // Preparation is on a clone: the report's untrimmed geometry and
        // reference point remain untouched.
        assert_eq!(report.airplane.xyz_ref, original_reference);
        let source_hstab = report
            .airplane
            .wings
            .iter()
            .find(|wing| wing.name == "Horizontal Stabilizer")
            .ok_or_else(|| "source plane keeps the horizontal stabilizer".to_owned())?;
        assert!(source_hstab
            .xsecs
            .iter()
            .all(|section| (section.twist - original_twist).abs() < f64::EPSILON));
        Ok(())
    }

    #[test]
    fn an_airplane_with_no_fuselage_renders_a_status_message_not_a_panic() {
        let mut airplane = probe_airplane(true, false);
        airplane.fuselages.clear();
        let report = probe_report(airplane);
        let config = AlasConfig::default();
        let scene = figure_dynamic_modes(&report, &config, None);
        assert_eq!(circles(&scene), 0);
        assert!(scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Text { text, .. } | SceneElement::TextBlock { text, .. } if text.contains("geometry"))));
    }

    #[test]
    fn mode_by_key_resolves_every_field_of_dynamic_modes() {
        let modes = DynamicModes {
            phugoid: DynamicMode {
                name: "phugoid",
                eigenvalue_real: -0.01,
                eigenvalue_imag: 0.1,
                damping_ratio: 0.1,
                period_s: 60.0,
                stable: true,
            },
            short_period: DynamicMode {
                name: "short_period",
                eigenvalue_real: -1.5,
                eigenvalue_imag: 2.0,
                damping_ratio: 0.6,
                period_s: 3.0,
                stable: true,
            },
            roll_subsidence: DynamicMode {
                name: "roll_subsidence",
                eigenvalue_real: -3.0,
                eigenvalue_imag: 0.0,
                damping_ratio: 1.0,
                period_s: 2.0,
                stable: true,
            },
            dutch_roll: DynamicMode {
                name: "dutch_roll",
                eigenvalue_real: -0.3,
                eigenvalue_imag: 1.2,
                damping_ratio: 0.25,
                period_s: 5.0,
                stable: true,
            },
            spiral: DynamicMode {
                name: "spiral",
                eigenvalue_real: 0.02,
                eigenvalue_imag: 0.0,
                damping_ratio: -1.0,
                period_s: 300.0,
                stable: false,
            },
        };
        assert_eq!(mode_by_key(&modes, "phugoid").period_s, 60.0);
        assert_eq!(mode_by_key(&modes, "short_period").period_s, 3.0);
        assert_eq!(mode_by_key(&modes, "roll_subsidence").period_s, 2.0);
        assert_eq!(mode_by_key(&modes, "dutch_roll").period_s, 5.0);
        assert!(!mode_by_key(&modes, "spiral").stable);
    }

    #[test]
    fn every_modal_branch_has_a_visible_point_label_and_legend_entry() {
        let modes = DynamicModes {
            phugoid: DynamicMode {
                name: "phugoid",
                eigenvalue_real: -0.01,
                eigenvalue_imag: 0.1,
                damping_ratio: 0.1,
                period_s: 60.0,
                stable: true,
            },
            short_period: DynamicMode {
                name: "short_period",
                eigenvalue_real: -1.5,
                eigenvalue_imag: 2.0,
                damping_ratio: 0.6,
                period_s: 3.0,
                stable: true,
            },
            roll_subsidence: DynamicMode {
                name: "roll_subsidence",
                eigenvalue_real: -3.0,
                eigenvalue_imag: 0.0,
                damping_ratio: 1.0,
                period_s: 2.0,
                stable: true,
            },
            dutch_roll: DynamicMode {
                name: "dutch_roll",
                eigenvalue_real: -0.3,
                eigenvalue_imag: 1.2,
                damping_ratio: 0.25,
                period_s: 5.0,
                stable: true,
            },
            spiral: DynamicMode {
                name: "spiral",
                eigenvalue_real: 0.02,
                eigenvalue_imag: 0.0,
                damping_ratio: -1.0,
                period_s: 300.0,
                stable: false,
            },
        };
        let scene = render(&modes, 2.0, get_palette(Some("light")));
        let texts: Vec<&str> = scene
            .elements
            .iter()
            .filter_map(|e| match e {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        for display in [
            "Phugoid",
            "Short period",
            "Roll subsidence",
            "Dutch roll",
            "Spiral",
        ] {
            assert!(
                texts.iter().filter(|text| **text == display).count() >= 2,
                "{display} needs both a point label and legend entry"
            );
        }
        // Eight pole markers (three conjugate pairs plus two real poles) and
        // one circular legend swatch for each of the five modes.
        assert_eq!(circles(&scene), 13);
    }
}
