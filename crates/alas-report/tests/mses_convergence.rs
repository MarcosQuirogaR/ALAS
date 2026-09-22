// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Focused presentation contracts for the MSES sweep-convergence figure.

use alas_aero::mses::{
    MsesOsmapStatus, MsesPolarPointDiagnostic, MsesPolarPointStatus, MsesPolarResult, MsesStatus,
};
use alas_report::families::aerodynamics::figure_mses_convergence;
use alas_report::scene::{Color, Scene, SceneElement, TextAlign};
use alas_report::svg::render_svg;

/// The reference partially converged sweep: a free-transition run whose
/// Orr-Sommerfeld map resolved and passed the driver's local format check.
fn fixture_result() -> MsesPolarResult {
    fixture_with_osmap(
        MsesOsmapStatus::Available,
        Some("adjacent double-precision osmapDP.dat passed the local header check"),
    )
}

/// The same sweep with only its Orr-Sommerfeld map evidence varied, so the
/// figure's map gate is tested against identical coefficient data.
fn fixture_with_osmap(
    osmap_status: MsesOsmapStatus,
    osmap_diagnostic: Option<&str>,
) -> MsesPolarResult {
    MsesPolarResult {
        status: MsesStatus::PartialConvergence,
        error: Some("MSES converged at 2 of 3 requested alpha points".to_owned()),
        requested_alpha_count: 3,
        converged_alpha_count: 2,
        point_diagnostics: vec![
            MsesPolarPointDiagnostic {
                requested_alpha_deg: 0.0,
                status: MsesPolarPointStatus::Converged,
                solver_output: "native solver transcript: converged".to_owned(),
            },
            MsesPolarPointDiagnostic {
                requested_alpha_deg: 4.0,
                status: MsesPolarPointStatus::NotConverged,
                solver_output: "native solver transcript: residual history".to_owned(),
            },
            MsesPolarPointDiagnostic {
                requested_alpha_deg: 8.0,
                status: MsesPolarPointStatus::Converged,
                solver_output: "native solver transcript: converged".to_owned(),
            },
        ],
        osmap_required: true,
        osmap_status,
        osmap_diagnostic: osmap_diagnostic.map(str::to_owned),
        alpha_deg: vec![0.0, 8.0],
        cl: vec![0.2, 0.9],
        cd: vec![0.02, 0.08],
        cm: vec![0.01, -0.04],
        cdv: vec![0.01, 0.03],
        cdw: vec![0.01, 0.05],
        xtr_top: vec![0.4, 0.2],
        xtr_bot: vec![0.5, 0.7],
        ..MsesPolarResult::default()
    }
}

fn decimal_places(text: &str) -> usize {
    text.split_once('.')
        .map(|(_, fraction)| fraction.len())
        .unwrap_or(0)
}

#[test]
fn convergence_figure_removes_status_prose_and_verdict_legend() {
    let result = fixture_result();
    let scene = figure_mses_convergence(&result, Some("grey"));
    let svg = render_svg(&scene).to_ascii_lowercase();

    assert_eq!(scene.title.as_deref(), Some("MSES Sweep Convergence"));
    assert!(svg.contains("MSES Sweep Convergence".to_ascii_lowercase().as_str()));
    assert!(!svg.contains("converged request"));
    assert!(!svg.contains("not converged"));
    assert!(!svg.contains("passed the local header check"));
    assert!(!svg.contains("mses converged at"));

    // The requested-point markers and the three coefficient traces remain in
    // the scene even though their explanatory status prose was removed.
    assert_eq!(
        scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Polyline { .. }))
            .count(),
        3
    );

    // Figure generation is read-only: solver diagnostics and error fields
    // remain available to callers for logs and machine-readable reports.
    assert_eq!(
        result.error.as_deref(),
        Some("MSES converged at 2 of 3 requested alpha points")
    );
    assert_eq!(
        result.osmap_diagnostic.as_deref(),
        Some("adjacent double-precision osmapDP.dat passed the local header check")
    );
    assert_eq!(
        result.point_diagnostics[1].solver_output,
        "native solver transcript: residual history"
    );
}

fn status_body_text(scene: &Scene) -> String {
    scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::TextBlock { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn coefficient_traces(scene: &Scene) -> usize {
    scene
        .elements
        .iter()
        .filter(|element| matches!(element, SceneElement::Polyline { .. }))
        .count()
}

#[test]
fn missing_required_osmap_cannot_render_as_a_converged_sweep() {
    let result = fixture_with_osmap(
        MsesOsmapStatus::Missing,
        Some("adjacent OSMAP file does not exist: C:\\solver\\osmapDP.dat"),
    );
    let scene = figure_mses_convergence(&result, Some("grey"));

    // The same coefficient arrays render three traces when the map is
    // available; with the map missing none of them may be drawn, and no
    // requested point may be marked green.
    assert_eq!(scene.title.as_deref(), Some("MSES figure unavailable"));
    assert_eq!(coefficient_traces(&scene), 0);
    assert!(!scene.elements.iter().any(|element| matches!(
        element,
        SceneElement::Circle { fill: Some(fill), .. }
            if fill.color == Color::from_hex("#27ae60")
    )));
    assert_eq!(
        status_body_text(&scene),
        "adjacent OSMAP file does not exist: C:\\solver\\osmapDP.dat"
    );

    // The structured evidence the run manifest serializes is untouched.
    assert_eq!(result.osmap_status.as_str(), "missing");
    assert_eq!(
        result.osmap_diagnostic.as_deref(),
        Some("adjacent OSMAP file does not exist: C:\\solver\\osmapDP.dat")
    );
    assert_eq!(result.converged_alpha_count, 2);
}

#[test]
fn incompatible_required_osmap_cannot_render_as_a_converged_sweep() {
    let diagnostic = "configured OSMAP file C:\\solver\\osmap.dat is incompatible: \
         single-precision osmap.dat detected; MSES requires osmapDP.dat";
    let result = fixture_with_osmap(MsesOsmapStatus::Incompatible, Some(diagnostic));
    let scene = figure_mses_convergence(&result, Some("dark"));

    assert_eq!(scene.title.as_deref(), Some("MSES figure unavailable"));
    assert_eq!(coefficient_traces(&scene), 0);
    assert_eq!(status_body_text(&scene), diagnostic);
    assert_eq!(result.osmap_status.as_str(), "incompatible");
}

#[test]
fn unusable_required_osmap_without_a_diagnostic_names_the_retained_status() {
    let scene = figure_mses_convergence(
        &fixture_with_osmap(MsesOsmapStatus::Incompatible, None),
        Some("light"),
    );

    assert_eq!(scene.title.as_deref(), Some("MSES figure unavailable"));
    assert_eq!(coefficient_traces(&scene), 0);
    assert_eq!(
        status_body_text(&scene),
        "MSES free transition was requested but no usable Orr-Sommerfeld map was recorded \
         (osmap status: incompatible)"
    );
    // The solver's own status prose stays out of the figure.
    assert!(!status_body_text(&scene).contains("MSES converged at"));
}

#[test]
fn a_forced_transition_sweep_renders_without_any_osmap() {
    let mut result = fixture_with_osmap(MsesOsmapStatus::NotRequired, None);
    result.osmap_required = false;
    let scene = figure_mses_convergence(&result, Some("grey"));

    assert_eq!(scene.title.as_deref(), Some("MSES Sweep Convergence"));
    assert_eq!(coefficient_traces(&scene), 3);
}

#[test]
fn unavailable_convergence_figure_does_not_render_solver_error_prose() {
    let result = MsesPolarResult {
        status: MsesStatus::Error,
        error: Some("MSES converged at 0 of 3 requested alpha points".to_owned()),
        ..MsesPolarResult::default()
    };
    let svg = render_svg(&figure_mses_convergence(&result, Some("dark"))).to_ascii_lowercase();

    assert!(svg.contains("mses figure unavailable"));
    assert!(!svg.contains("mses converged at"));
    assert_eq!(
        result.error.as_deref(),
        Some("MSES converged at 0 of 3 requested alpha points")
    );
}

#[test]
fn convergence_figure_formats_all_y_ticks_to_two_decimal_places_in_each_theme() {
    for theme in ["light", "grey", "dark"] {
        let scene = figure_mses_convergence(&fixture_result(), Some(theme));
        let y_tick_labels = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text {
                    text,
                    align: TextAlign::Right,
                    ..
                } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(!y_tick_labels.is_empty(), "{theme} emitted no y ticks");
        assert!(
            y_tick_labels.iter().all(|label| decimal_places(label) <= 2),
            "{theme} y ticks exceed two decimal places: {y_tick_labels:?}"
        );
    }
}
