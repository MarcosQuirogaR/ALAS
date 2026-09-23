// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/sidecar/figures.py: _preview_drag.
// Reference: alas @ rust-port-baseline.

//! Solver-free drag preview for the live design editor.
//!
//! The Python side evaluates the same parasite and Korn wave-drag methods used
//! by the optimizer at thirty Mach stations. Keeping this as a report factory
//! makes the GUI use the production aerodynamic formulas without running a VLM
//! solve on every debounced edit.

use alas_aero::analysis::AeroAnalysis;
use alas_config::{design_variables::DesignVector, AlasConfig};
use alas_geom::aircraft::airplane::Airplane;

use crate::scene::{Axes2D, Color, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

/// Build the live parasite and wave-drag versus Mach chart.
pub fn figure_drag_preview(
    airplane: &Airplane,
    config: &AlasConfig,
    design: &DesignVector,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let aero = AeroAnalysis::new(
        airplane,
        AeroAnalysis::quarter_chord_sweep_deg(airplane, design.sweep_deg),
        Some(config.geometry.clone()),
        Some(config.drag_model.clone()),
        Some(config.analysis.clone()),
    );
    let altitude = config.requirements.cruise_altitude_m;
    let cl_ref = 0.5;
    let machs: Vec<f64> = (0..30).map(|i| 0.3 + i as f64 * 0.62 / 29.0).collect();
    let parasite: Vec<(f64, f64)> = machs
        .iter()
        .map(|&mach| (mach, aero.parasite_drag(mach, altitude, cl_ref, None, None)))
        .collect();
    let wave: Vec<(f64, f64)> = machs
        .iter()
        .map(|&mach| (mach, aero.wave_drag(mach, cl_ref, None)))
        .collect();
    let max_cd = parasite
        .iter()
        .chain(wave.iter())
        .map(|(_, cd)| *cd)
        .fold(0.0, f64::max)
        .max(1e-4);
    let mut scene = Scene::new(720.0, 520.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Drag vs Mach (illustrative)".to_owned());
    // The explicit chart heading below includes the operating-point CL. Keep
    // that informative title and suppress Scene's automatic metadata heading
    // so the live preview does not paint the same title twice.
    scene.suppress_derived_title();
    let axes = Axes2D::new(
        (92.0, 55.0, 584.0, 340.0),
        (0.3, 0.92),
        (0.0, max_cd * 1.08),
    );
    axes.draw_frame_with_labels(&mut scene, pal, "Mach", "Drag coefficient C_D");
    axes.add_line_series(
        &mut scene,
        &parasite,
        Stroke::new(Color::from_hex("#00d8ff"), 1.8),
    );
    axes.add_line_series(
        &mut scene,
        &wave,
        Stroke::new(Color::from_hex("#ff9900"), 1.8),
    );
    let cruise = config.requirements.cruise_mach;
    scene.add(SceneElement::Line {
        p1: axes.map_point(cruise, 0.0),
        p2: axes.map_point(cruise, max_cd * 1.08),
        stroke: Stroke::dashed(Color::from_hex(pal.title), 1.0, 5.0, 4.0),
    });
    scene.add(SceneElement::Text {
        text: format!("Drag vs Mach (illustrative, CL={cl_ref:.2})"),
        pos: [376.0, 28.0],
        font_size: 12.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });
    scene.add(SceneElement::Text {
        text: "CD0 (parasite)   |   CD wave   |   cruise Mach".to_owned(),
        pos: [376.0, 505.0],
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
    use super::figure_drag_preview;
    use crate::scene::SceneElement;
    use alas_config::design_variables::DesignVector;
    use alas_config::AlasConfig;
    use alas_geom::builder::AircraftBuilder;

    #[test]
    fn drag_preview_contains_numeric_ticks_and_labeled_axes() {
        let config = AlasConfig::default();
        let design = DesignVector::default();
        let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap_or_else(|error| panic!("default drag preview geometry: {error}"));
        let scene = figure_drag_preview(&airplane, &config, &design, Some("dark"));
        let labels = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(labels.contains(&"Mach"));
        assert!(labels.contains(&"Drag coefficient C_D"));
        let numeric_ticks = labels
            .iter()
            .filter(|label| label.parse::<f64>().is_ok())
            .count();
        assert!(
            numeric_ticks >= 4,
            "expected numeric x/y tick labels: {labels:?}"
        );

        let text_position = |wanted: &str| {
            scene.elements.iter().find_map(|element| match element {
                SceneElement::Text { text, pos, .. } if text == wanted => Some(*pos),
                _ => None,
            })
        };
        let x_label = text_position("Mach").unwrap_or_else(|| panic!("missing x-axis label"));
        let footer = text_position("CD0 (parasite)   |   CD wave   |   cruise Mach")
            .unwrap_or_else(|| panic!("missing drag-series footer"));
        assert!(
            footer[1] - x_label[1] >= 40.0,
            "x-axis label and footer need a readable vertical gutter: {x_label:?}, {footer:?}"
        );
        assert!(!scene.render_title);
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("CL="))
        }));
    }
}
