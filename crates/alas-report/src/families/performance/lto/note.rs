// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The line under the field-length bars that says which case governs.

use alas_perf::performance::{FieldPerformance, TakeoffField};

use crate::scene::{Color, Scene, SceneElement, TextAlign, TextBaseline};

/// Which case sets the integrated jet take-off field length. By CS 25.113 the
/// take-off distance is the larger of the engine-out balanced field length and
/// 1.15 times the all-engine distance, and at the balanced V1 the accelerate-
/// stop and continued distances are equal, so TODR, BFL and ASD coincide
/// whenever the engine-out case governs. The note says so instead of leaving
/// three identical bars unexplained.
pub(super) fn governing_note(performance: &FieldPerformance) -> Option<String> {
    performance.takeoff_field.as_ref().map(governing_text)
}

/// `tofl = max(1.15 AEO, balanced)` in the solver, so the engine-out case
/// governs exactly when `tofl` is not above the balanced length; no
/// tolerance, since a few metres of difference is the all-engine case.
fn governing_text(field: &TakeoffField) -> String {
    if field.tofl_m <= field.balanced_m {
        "Engine-out balanced field governs: TODR = BFL = ASD (CS 25.113)".to_owned()
    } else {
        "All-engine case governs TODR (1.15 x all-engine distance); BFL = ASD at the balanced V1"
            .to_owned()
    }
}

/// Draw the governing-case line under the distance bars.
pub(super) fn draw_governing_note(
    scene: &mut Scene,
    performance: &FieldPerformance,
    pal: &crate::theme::Palette,
) {
    if let Some(text) = governing_note(performance) {
        scene.add(SceneElement::Text {
            text,
            pos: [450.0, 780.0],
            font_size: 9.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(tofl_m: f64, balanced_m: f64) -> TakeoffField {
        TakeoffField {
            tofl_m,
            all_engine_m: tofl_m / 1.15,
            balanced_m,
            continued_m: balanced_m,
            accelerate_stop_m: balanced_m,
            v1_m_s: 70.0,
            vef_m_s: 65.0,
            vr_m_s: 75.0,
        }
    }

    #[test]
    fn the_note_names_the_branch_the_solver_took() {
        assert!(governing_text(&field(2109.0, 2109.0)).starts_with("Engine-out"));
        // DC-10 numbers: 3059 m against a balanced 3045 m is the all-engine
        // case although the two differ by less than half a percent.
        assert!(governing_text(&field(3059.0, 3045.0)).starts_with("All-engine"));
        assert!(governing_text(&field(2879.0, 2773.0)).starts_with("All-engine"));
    }
}
