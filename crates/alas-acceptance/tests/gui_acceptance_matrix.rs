// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Specification checks for the user-facing GUI acceptance matrix.
//!
//! The GUI has no interaction harness in this crate yet. These checks keep the
//! human/executable acceptance specification complete and traceable without
//! pretending that a pipeline or SVG test proves desktop behavior.

const MATRIX: &str = include_str!("../../../docs/C0_GUI_ACCEPTANCE_MATRIX.md");

#[test]
fn gui_acceptance_matrix_names_every_required_interaction_family() {
    for required in [
        "Independent zoom",
        "Double-click",
        "Escape",
        "page scrolling",
        "Fit",
        "run-log",
        "Multi-column",
        "preset",
        "Disabled",
        "Absent",
        "Incomplete",
        "Launch failure",
        "Timeout",
        "Parse failure",
        "Success",
    ] {
        assert!(MATRIX.contains(required), "matrix omits '{required}'");
    }
}

#[test]
fn gui_acceptance_matrix_has_representative_window_and_scale_envelopes() {
    for required in [
        "1366 x 768",
        "1280 x 720",
        "1920 x 1080",
        "2560 x 1440",
        "100%",
        "125%",
        "150%",
        "200%",
    ] {
        assert!(MATRIX.contains(required), "matrix omits '{required}'");
    }
}

#[test]
fn every_gui_scenario_has_preconditions_actions_assertions_and_source_trace() {
    let mut scenario_count = 0;
    for line in MATRIX.lines().filter(|line| line.starts_with("| GUI-")) {
        scenario_count += 1;
        let cells: Vec<_> = line.split('|').map(str::trim).collect();
        assert_eq!(cells.len(), 8, "scenario row must have six table cells");
        assert!(!cells[2].is_empty(), "scenario needs preconditions: {line}");
        assert!(!cells[3].is_empty(), "scenario needs actions: {line}");
        assert!(!cells[4].is_empty(), "scenario needs assertions: {line}");
        assert!(
            cells[6].contains("MISSING-THINGS.md"),
            "scenario needs source trace: {line}"
        );
    }
    assert!(
        scenario_count >= 19,
        "GUI matrix is missing required scenarios"
    );
}

#[test]
fn gui_acceptance_matrix_avoids_subjective_acceptance_language() {
    for subjective in ["looks generic", "looks fine", "AI tell", "makes no sense"] {
        assert!(
            !MATRIX.contains(subjective),
            "matrix contains subjective text '{subjective}'"
        );
    }
}
