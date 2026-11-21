// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The engine-sizing rows of a closed candidate.

use alas_config::AlasConfig;

use crate::mdo::sizing::SizingOutcome;
use crate::mdo::types::ConstraintFamily::Performance;
use crate::mdo::types::ConstraintResidual;
use crate::mdo::ResidualRole;

use super::{EngineSizing, MATCH_TOLERANCE};

/// The engine-sizing rows of a closed candidate: the hard match and validity
/// rows of a sized engine, the requirement margin of a held one.
pub(crate) fn residuals(outcome: &SizingOutcome, config: &AlasConfig) -> Vec<ConstraintResidual> {
    outcome
        .engine_sizing
        .as_ref()
        .map_or_else(Vec::new, |sizing| rows_of(sizing, config))
}

/// The rows of one engine-sizing result (see [`residuals`]).
fn rows_of(sizing: &EngineSizing, config: &AlasConfig) -> Vec<ConstraintResidual> {
    let engine = &config.geometry.engine;
    let (rating, unit) = match engine.scaling_rating() {
        Some(rating) if engine.thrust_kn() > 0.0 => (rating, "kN"),
        Some(rating) => (rating, "kW"),
        None => return Vec::new(),
    };
    // No thrust row could be evaluated (airport or evidence missing): those
    // gaps are hard rows of their own, and there is no requirement to report.
    let Some(required_ratio) = sizing.required_ratio else {
        return Vec::new();
    };
    let required = rating * required_ratio;
    if !sizing.sizes {
        // A held engine is the user's choice: an engine whose requirement is
        // more than the rating window below its rating (rating above
        // requirement / (1 - window), 25 percent at the default 20 percent
        // window) is reported (Diagnostic row here, the pipeline warning finding
        // `engine_oversize` and the acceptance rule ENGINE-OVERSIZE), not
        // refused. A Preference would reward a candidate for raising its own
        // thrust requirement; sufficiency (too weak) stays a hard row elsewhere.
        let window = config.optimizer.design_space.engine_rating_window;
        let oversize = ((1.0 - required_ratio) - window).max(0.0) / window;
        let mut rows = vec![
            ConstraintResidual::direct(
                "engine_thrust_margin",
                Performance,
                rating,
                required,
                unit,
                required - rating,
                0.0,
                ResidualRole::Diagnostic,
            )
            .with_detail(format!(
                "the engine is held at its declared rating; the governing requirement is {}",
                sizing.governing
            )),
            {
                // Raw residual positive when violated (types.rs): the rating
                // over the largest rating that still matches the requirement.
                let largest_match = required / (1.0 - window);
                let row = ConstraintResidual::direct(
                    "engine_oversize",
                    Performance,
                    rating,
                    largest_match,
                    unit,
                    rating - largest_match,
                    oversize,
                    ResidualRole::Diagnostic,
                );
                if oversize > 0.0 {
                    row.with_detail(format!(
                        "the governing requirement ({}) is more than the {:.0} percent rating window below the held rating: the rating exceeds it by more than {:.1} percent",
                        sizing.governing,
                        100.0 * window,
                        100.0 * (1.0 / (1.0 - window) - 1.0)
                    ))
                } else {
                    row
                }
            },
        ];
        // A search model holding an engine its run sizes predicts the sized
        // engine's verdict: the scale the sizing aims at on this airframe.
        if sizing.predicts_scale {
            let held = engine.catalogue_scale().unwrap_or(f64::NAN);
            rows.push(
                scale_validity(held * required_ratio * (1.0 + 0.5 * MATCH_TOLERANCE), window)
                    .with_detail(format!(
                        "predicted by the search model's held engine from the {} requirement",
                        sizing.governing
                    )),
            );
        }
        return rows;
    }
    let deviation = (1.0 - required_ratio).abs();
    let match_violation = if required_ratio.is_finite() {
        (deviation - MATCH_TOLERANCE).max(0.0) / MATCH_TOLERANCE
    } else {
        f64::INFINITY
    };
    let mut rows = vec![ConstraintResidual::direct(
        "engine_thrust_match",
        Performance,
        rating,
        required,
        unit,
        // Positive when violated, on either side of the requirement; an
        // unconverged iteration is violated whatever its last ratio (the
        // normalized violation is at least 1).
        if sizing.converged {
            rating * (deviation - MATCH_TOLERANCE)
        } else {
            rating * (deviation - MATCH_TOLERANCE).max(MATCH_TOLERANCE)
        },
        if sizing.converged {
            match_violation
        } else {
            match_violation.max(1.0)
        },
        ResidualRole::Constraint,
    )
    .with_detail(format!(
        "sized to {}; {} closures",
        sizing.governing, sizing.closures
    ))];
    let window = config.optimizer.design_space.engine_rating_window;
    rows.push(scale_validity(
        engine.catalogue_scale().unwrap_or(f64::NAN),
        window,
    ));
    rows
}

/// The hard `engine_scale_validity` row of an engine at `scale` on its
/// catalogue rating: violated outside the rating window.
fn scale_validity(scale: f64, window: f64) -> ConstraintResidual {
    let excess = (scale - 1.0).abs() - window;
    let bound = if scale >= 1.0 {
        1.0 + window
    } else {
        1.0 - window
    };
    ConstraintResidual::direct(
        "engine_scale_validity",
        Performance,
        scale,
        bound,
        "ratio",
        excess,
        if scale.is_finite() {
            excess.max(0.0) / window
        } else {
            f64::INFINITY
        },
        ResidualRole::Constraint,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdo::build::build_geometry;
    use crate::mdo::types::ExternalPolar;
    use crate::{assess_candidate_with_polar_cancellable, canonicalize_design, DesignObjective};
    use alas_config::optimizer::DesignMode;
    use alas_config::presets;
    use serde_json::json;

    /// An unconverged sizing whose last ratio sits inside the 1 percent match
    /// tolerance (0.995) is still a violated match: raw residual positive and
    /// normalized violation at least one, never the converged-looking zero.
    #[test]
    fn an_unconverged_match_row_is_violated_whatever_its_last_ratio() {
        let config = AlasConfig::from_value(&json!({ "preset": "A320-200" })).unwrap();
        let sized = alas_config::SizedEngine::of(&config);
        let sizing = |converged| EngineSizing {
            base: sized.clone(),
            sized: sized.clone(),
            sizes: true,
            required_ratio: Some(0.995),
            governing: "test",
            closures: 3,
            converged,
            predicts_scale: false,
        };
        let row = |converged| {
            rows_of(&sizing(converged), &config)
                .into_iter()
                .find(|row| row.id == "engine_thrust_match")
                .expect("a sized engine carries the match row")
        };
        let open = row(false);
        assert!(open.raw_residual > 0.0, "{open:?}");
        assert!(open.normalized_violation >= 1.0, "{open:?}");
        let settled = row(true);
        assert!(settled.raw_residual <= 0.0, "{settled:?}");
        assert_eq!(settled.normalized_violation, 0.0);
    }

    /// A search model's held engine predicts the sized engine's validity:
    /// a requirement 30 percent under the held catalogue engine aims at a
    /// scale of 0.7035, outside the 20 percent window (hard and violated);
    /// one 10 percent under stays inside; a held engine the run does not
    /// size carries no validity row.
    #[test]
    fn a_held_search_engine_predicts_the_scale_validity_verdict() {
        let config = AlasConfig::from_value(&json!({ "preset": "A320-200" })).unwrap();
        let sized = alas_config::SizedEngine::of(&config);
        let validity = |ratio, predicts_scale| {
            let sizing = EngineSizing {
                base: sized.clone(),
                sized: sized.clone(),
                sizes: false,
                required_ratio: Some(ratio),
                governing: "test",
                closures: 1,
                converged: true,
                predicts_scale,
            };
            rows_of(&sizing, &config)
                .into_iter()
                .find(|row| row.id == "engine_scale_validity")
        };
        let low = validity(0.7, true).expect("a predicting model carries the row");
        assert_eq!(low.role, ResidualRole::Constraint);
        assert!((low.actual - 0.7 * 1.005).abs() < 1e-12, "{low:?}");
        assert!(low.violated(), "{low:?}");
        let inside = validity(0.9, true).expect("a predicting model carries the row");
        assert!(!inside.violated(), "{inside:?}");
        assert!(validity(0.7, false).is_none());
    }

    /// A supplied polar fixes the cruise drag, so the engine is held and the
    /// held rows come out of this same function: the oversize row obeys the
    /// sign contract of `ConstraintResidual` (raw residual positive when
    /// violated, actual above limit) and carries no detail while met.
    #[test]
    fn the_polar_path_emits_the_signed_oversize_row() {
        let mut config = AlasConfig::from_value(&json!({ "preset": "A320-200" })).unwrap();
        config.optimizer.design_space.mode = DesignMode::CleanSheet;
        let preset = presets::registry()
            .iter()
            .find(|p| p.name == "A320-200")
            .unwrap();
        let design = canonicalize_design(&config, preset.design_vector).unwrap();
        let (_, _, plane) = build_geometry(&config, &design.to_array()).unwrap();
        let polar = |config: &AlasConfig| ExternalPolar {
            cd0: 0.012,
            induced_factor_k: 0.030,
            wave_drag_cd: 0.0005,
            lift_to_drag: 22.0,
            alpha_deg: 2.0,
            incidence_deg: -1.0,
            x_np: 18.0,
            mach: config.requirements.cruise_mach,
            altitude_m: config.requirements.cruise_altitude_m,
            reference_area_m2: plane.s_ref,
            target_cl: 0.5,
            source: "test",
            bracketed: true,
        };
        for (window, violated) in [(0.02, true), (0.30, false)] {
            let mut config = config.clone();
            config.optimizer.design_space.engine_rating_window = window;
            let objective = DesignObjective::new_with_nominal(config.clone(), design);
            let a = assess_candidate_with_polar_cancellable(
                &objective,
                &design.to_array(),
                &polar(&config),
                None,
            )
            .unwrap_or_else(|reason| panic!("polar assessment: {reason}"));
            let row = a
                .residuals
                .iter()
                .find(|row| row.id == "engine_oversize")
                .expect("the held polar path emits the oversize row");
            assert_eq!(row.role, ResidualRole::Diagnostic);
            assert_eq!(row.normalized_violation > 0.0, violated, "{row:?}");
            assert_eq!(row.raw_residual > 0.0, violated, "{row:?}");
            assert_eq!(row.actual > row.limit, violated, "{row:?}");
            assert_eq!(row.detail.is_some(), violated);
        }
    }
}
