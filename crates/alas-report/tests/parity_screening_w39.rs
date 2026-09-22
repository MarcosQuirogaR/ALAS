// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! W3.9 screening figure contracts: every panel keeps the reference labels,
//! data stage, and honest unavailable behavior visible to SVG and GUI callers.

// Checked-in fixture data and deliberately complete sample results make a
// missing value an immediate test failure, not a recoverable runtime case.
#![cfg_attr(test, allow(clippy::expect_used))]

use alas_report::families::screening::{
    fig_mses_verification, fig_ranking_bars, fig_rerank_2d_3d, fig_section_shapes, fig_trade_map,
};
use alas_report::svg::render_svg;
use alas_screen::{AirfoilCandidateResult, AirfoilScreeningResult};
use serde_json::Value;

fn candidate(name: &str, refined: bool, verified: bool) -> AirfoilCandidateResult {
    AirfoilCandidateResult {
        name: name.to_owned(),
        status: "ok".to_owned(),
        l_over_d: Some(16.0),
        l_over_d_3d: Some(14.0),
        tank_capacity_kg: Some(12_000.0),
        max_thickness_frac: Some(0.12),
        refined,
        mses_verified: verified,
        l_over_d_mses: Some(13.5),
        cdw_mses: Some(0.0012),
        ..Default::default()
    }
}

#[test]
fn screening_scenes_keep_reference_labels_and_series_in_both_themes() {
    let result = AirfoilScreeningResult {
        baseline_airfoil: "naca0012".to_owned(),
        candidates: vec![
            candidate("naca0012", true, true),
            candidate("naca2412", true, false),
        ],
        ..Default::default()
    };

    for theme in ["light", "dark"] {
        let trade = render_svg(&fig_trade_map(&result, Some(theme)).expect("trade data"));
        for label in [
            "Trade map: L/D vs fuel capacity",
            "Cruise L/D (3-D wing)",
            "Wing fuel-tank capacity (kg)",
            "Section t/c (%)",
        ] {
            assert!(
                trade.contains(label),
                "trade map missing {label} in {theme}"
            );
        }

        let rerank = render_svg(&fig_rerank_2d_3d(&result, Some(theme)).expect("refinement data"));
        for label in [
            "2-D proxy L/D (isolated section)",
            "3-D wing L/D (this design, cruise)",
            "2-D = 3-D",
        ] {
            assert!(rerank.contains(label), "rerank missing {label} in {theme}");
        }

        let ranking = render_svg(&fig_ranking_bars(&result, Some(theme)).expect("ranking data"));
        for label in ["Cruise L/D (3-D wing)", "naca0012 (current)"] {
            assert!(
                ranking.contains(label),
                "ranking missing {label} in {theme}"
            );
        }

        let sections = render_svg(&fig_section_shapes(&result, Some(theme)).expect("section data"));
        for label in [
            "x/c",
            "y/c  (sections offset vertically)",
            "Naca0012 (current)",
        ] {
            assert!(
                sections.contains(label),
                "section shapes missing {label} in {theme}"
            );
        }

        let mses = render_svg(&fig_mses_verification(&result, Some(theme)).expect("MSES data"));
        for label in [
            "Cruise L/D",
            "CDw=12 cts",
            "MSES (Stage 3, real shock/viscous)",
        ] {
            assert!(
                mses.contains(label),
                "MSES figure missing {label} in {theme}"
            );
        }
    }
}

#[test]
fn mses_scene_is_unavailable_when_stage_three_did_not_run() {
    let result = AirfoilScreeningResult {
        candidates: vec![candidate("naca0012", true, false)],
        ..Default::default()
    };
    assert!(fig_mses_verification(&result, Some("dark")).is_none());
}

#[test]
fn screening_scenes_are_unavailable_without_their_required_stage_data() {
    let result = AirfoilScreeningResult {
        candidates: vec![candidate("naca0012", false, false)],
        ..Default::default()
    };
    assert!(fig_rerank_2d_3d(&result, None).is_none());
    assert!(fig_mses_verification(&result, None).is_none());
}

#[test]
fn reference_fixture_covers_every_w39_figure_in_both_parity_themes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../golden/report/reference_render_w39.json"
    ))
    .expect("W3.9 fixture is valid JSON");
    assert_eq!(fixture["schema"], "reference-render-w39/v1");
    let figures = fixture["figures"].as_object().expect("figure map");
    for id in [
        "optimization_history",
        "trade_map",
        "rerank_2d_3d",
        "ranking_bars",
        "section_shapes",
        "mses_verification",
    ] {
        for theme in ["light", "dark"] {
            let contract = &figures[&format!("{id}:{theme}")];
            assert_eq!(contract["available"], true, "{id}:{theme} unavailable");
            assert_eq!(contract["theme"], theme);
            assert!(contract["panel_count"].as_u64().unwrap_or(0) > 0);
            assert!(contract["axes"].as_array().is_some());
        }
    }

    let optimization = &figures["optimization_history:light"];
    assert_eq!(optimization["panel_count"], 2);
    assert_eq!(optimization["axes"][0]["xlabel"], "valid evaluation #");
    assert_eq!(optimization["axes"][0]["ylabel"], "L/D");
    assert_eq!(optimization["axes"][0]["legend"][0], "evaluation");
    assert_eq!(optimization["axes"][0]["legend"][1], "best so far");
    assert_eq!(optimization["axes"][1]["ylabel"], "span [m]");

    let trade = &figures["trade_map:light"];
    assert_eq!(trade["panel_count"], 2);
    assert_eq!(trade["axes"][0]["xlabel"], "Cruise L/D (3-D wing)");
    assert_eq!(trade["axes"][1]["ylabel"], "Section t/c (%)");

    let mses = &figures["mses_verification:light"];
    assert_eq!(
        mses["axes"][0]["legend"][1],
        "MSES (Stage 3, real shock/viscous)"
    );
    assert!(mses["axes"][0]["annotations"]
        .as_array()
        .expect("MSES annotations")
        .iter()
        .any(|text| text == "CDw=12 cts"));
}
