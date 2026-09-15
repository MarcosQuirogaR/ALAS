// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/airfoil_sweep_figures.py
// Reference: alas @ rust-port-baseline.

//! Airfoil screening figures: the ranking bars, the trade-space map, the
//! 2-D/3-D re-rank comparison, overlaid section shapes, and the MSES
//! verification chart.
//!
//! Each factory mirrors Python's `fn(result, theme) -> Figure | None`
//! (`SWEEP_FIGURES` in the reference): `None` means "no data for this
//! figure": an empty candidate list, no Stage-2 refinement, or no Stage-3
//! MSES verification, which a caller turns into an empty gallery slot
//! rather than a panic or a fabricated chart.

mod mses_verification;
mod ranking_bars;
mod rerank_2d_3d;
mod section_shapes;
mod trade_map;

pub use mses_verification::fig_mses_verification;
pub use ranking_bars::fig_ranking_bars;
pub use rerank_2d_3d::fig_rerank_2d_3d;
pub use section_shapes::fig_section_shapes;
pub use trade_map::fig_trade_map;

use alas_screen::{AirfoilCandidateResult, AirfoilScreeningResult};

use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::Palette;

/// Candidates that survived screening, best-rank first (result order): `_ok`.
fn ok_candidates(result: &AirfoilScreeningResult) -> Vec<&AirfoilCandidateResult> {
    result
        .candidates
        .iter()
        .filter(|c| c.status == "ok")
        .collect()
}

/// Refined (Stage-2, 3-D) survivors among the ok candidates: `_refined`.
fn refined_candidates(result: &AirfoilScreeningResult) -> Vec<&AirfoilCandidateResult> {
    ok_candidates(result)
        .into_iter()
        .filter(|c| c.refined)
        .collect()
}

/// MSES-verified (Stage-3) survivors among the ok candidates: `_mses_verified`.
fn mses_verified_candidates(result: &AirfoilScreeningResult) -> Vec<&AirfoilCandidateResult> {
    ok_candidates(result)
        .into_iter()
        .filter(|c| c.mses_verified)
        .collect()
}

/// Marker color for a real, wind-tunnel-validated reference section:
/// `_REFERENCE_MARKER_COLOR`. The reference's five-pointed star has no
/// equivalent primitive in [`SceneElement`]; [`mark_references`]
/// approximates it with a filled circle, the same substitution every other
/// scatter marker in this crate makes.
pub(super) const REFERENCE_MARKER_COLOR: &str = "#f5c518";

/// Overlay a marker + name label on every `is_reference` candidate among
/// `cands`, already plotted by the caller at `(xs[i], ys[i])`:
/// `_mark_references`. Returns whether any reference candidate was found, so
/// callers can decide whether to add the matching legend entry.
pub(super) fn mark_references(
    scene: &mut Scene,
    axes: &Axes2D,
    cands: &[&AirfoilCandidateResult],
    xs: &[f64],
    ys: &[f64],
    pal: &Palette,
) -> bool {
    let star_color = Color::from_hex(REFERENCE_MARKER_COLOR);
    let mut drew = false;
    for ((cand, &x), &y) in cands.iter().zip(xs).zip(ys) {
        if !cand.is_reference {
            continue;
        }
        drew = true;
        let p = axes.map_point(x, y);
        scene.add(SceneElement::Circle {
            center: p,
            radius: 7.0,
            fill: Some(Fill::new(star_color)),
            stroke: Some(Stroke::new(Color::from_hex(pal.spine), 1.0)),
        });
        scene.add(SceneElement::Text {
            text: cand.name.clone(),
            pos: [p[0] + 7.0, p[1] - 6.0],
            font_size: 7.0,
            color: star_color,
            align: TextAlign::Left,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
    }
    drew
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(
        name: &str,
        status: &str,
        refined: bool,
        mses_verified: bool,
    ) -> AirfoilCandidateResult {
        AirfoilCandidateResult {
            name: name.to_owned(),
            status: status.to_owned(),
            refined,
            mses_verified,
            ..Default::default()
        }
    }

    #[test]
    fn ok_candidates_drops_errored_ones_and_keeps_result_order() {
        let result = AirfoilScreeningResult {
            candidates: vec![
                candidate("a", "ok", false, false),
                candidate("b", "error", false, false),
                candidate("c", "ok", false, false),
            ],
            ..Default::default()
        };
        let names: Vec<&str> = ok_candidates(&result)
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, vec!["a", "c"]);
    }

    #[test]
    fn refined_and_mses_verified_further_narrow_the_ok_set() {
        let result = AirfoilScreeningResult {
            candidates: vec![
                candidate("a", "ok", true, false),
                candidate("b", "ok", false, true),
                candidate("c", "error", true, true),
            ],
            ..Default::default()
        };
        assert_eq!(refined_candidates(&result).len(), 1);
        assert_eq!(refined_candidates(&result)[0].name, "a");
        assert_eq!(mses_verified_candidates(&result).len(), 1);
        assert_eq!(mses_verified_candidates(&result)[0].name, "b");
    }
}
