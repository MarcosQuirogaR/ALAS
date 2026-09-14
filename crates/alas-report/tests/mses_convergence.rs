// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Focused presentation contracts for the MSES sweep-convergence figure.

use alas_aero::mses::{
    MsesPolarPointDiagnostic, MsesPolarPointStatus, MsesPolarResult, MsesStatus,
};
use alas_report::families::aerodynamics::figure_mses_convergence;
use alas_report::scene::{SceneElement, TextAlign};
use alas_report::svg::render_svg;

fn fixture_result() -> MsesPolarResult {
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
        osmap_diagnostic: Some(
            "adjacent OSMAP file does not exist: C:\\solver\\osmapDP.dat".to_owned(),
        ),
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
    assert!(!svg.contains("adjacent osmap file does not exist"));
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
        Some("adjacent OSMAP file does not exist: C:\\solver\\osmapDP.dat")
    );
    assert_eq!(
        result.point_diagnostics[1].solver_output,
        "native solver transcript: residual history"
    );
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
