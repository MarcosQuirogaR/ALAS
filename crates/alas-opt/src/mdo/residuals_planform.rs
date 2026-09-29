// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in transport shape preferences, evaluated on the candidate wing.
//! These are study preferences, not structural or certification limits.
//! Thresholds reuse ObjectiveWeights; product ranking uses the common
//! normalized soft-residual policy, not L/D penalty multipliers.

use crate::mdo::{ConstraintFamily, ConstraintResidual};
use crate::transport_planform::assess_product_transport_planform;
use alas_config::{AlasConfig, ConstraintPolicy, DesignVector, ObjectiveWeights};
use alas_geom::aircraft::airplane::Airplane;

pub(super) fn residuals(
    plane: &Airplane,
    design: &DesignVector,
    config: &AlasConfig,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    let w = &config.optimizer.weights;
    if policy == ConstraintPolicy::Off
        || !w.transport_planform_constraints_enabled
        || !w.transport_shape_priors_enabled
    {
        return Vec::new();
    }
    if !limits_valid(w)
        || config
            .structures
            .spar_chord_fractions
            .iter()
            .any(|fraction| !fraction.is_finite() || *fraction < 0.0 || *fraction > 1.0)
    {
        return invalid();
    }
    let Some(a) = assess_product_transport_planform(plane, design, config) else {
        return invalid();
    };
    let policy = match policy {
        ConstraintPolicy::Hard => ConstraintPolicy::Soft,
        other => other,
    };
    vec![
        lower(
            "transport_root_wingbox_depth",
            a.root_wingbox_depth_m,
            w.min_root_wingbox_depth_m,
            "m",
            policy,
        ),
        lower(
            "transport_kink_wingbox_depth",
            a.kink_wingbox_depth_m,
            w.min_break_wingbox_depth_m,
            "m",
            policy,
        ),
        lower(
            "transport_kink_wingbox_width",
            a.kink_wingbox_width_m,
            w.min_break_wingbox_width_m,
            "m",
            policy,
        ),
        lower(
            "transport_flap_area_fraction",
            a.flap_area_fraction,
            w.min_flap_area_fraction,
            "-",
            policy,
        ),
        ConstraintResidual::scaled(
            "transport_root_box_slenderness",
            ConstraintFamily::Geometry,
            a.root_bending_box_slenderness,
            w.max_root_bending_box_slenderness,
            "-",
            a.root_bending_box_slenderness - w.max_root_bending_box_slenderness,
            policy,
        ),
        window(
            "transport_inboard_te_sweep",
            a.inboard_trailing_edge_sweep_deg,
            w.min_inboard_te_sweep_deg,
            w.max_inboard_te_sweep_deg,
            "deg",
            policy,
        ),
        window(
            "transport_break_root_chord_ratio",
            a.break_root_chord_ratio,
            w.min_break_root_chord_ratio,
            w.max_break_root_chord_ratio,
            "-",
            policy,
        ),
        lower(
            "transport_tip_root_chord_ratio",
            a.tip_root_chord_ratio,
            w.min_tip_root_chord_ratio,
            "-",
            policy,
        ),
    ]
}

fn invalid() -> Vec<ConstraintResidual> {
    vec![ConstraintResidual::direct(
        "transport_planform_invalid",
        ConstraintFamily::Geometry,
        1.0,
        0.0,
        "bool",
        1.0,
        1.0,
        ConstraintPolicy::Hard,
    )]
}

fn limits_valid(w: &ObjectiveWeights) -> bool {
    let positive = [
        w.min_root_wingbox_depth_m,
        w.min_break_wingbox_depth_m,
        w.min_break_wingbox_width_m,
        w.min_flap_area_fraction,
        w.max_root_bending_box_slenderness,
        w.min_break_root_chord_ratio,
        w.max_break_root_chord_ratio,
        w.min_tip_root_chord_ratio,
    ];
    positive.iter().all(|x| x.is_finite() && *x > 0.0)
        && w.min_flap_area_fraction <= 1.0
        && w.min_tip_root_chord_ratio <= 1.0
        && w.min_break_root_chord_ratio < w.max_break_root_chord_ratio
        && w.max_break_root_chord_ratio <= 1.0
        && w.min_inboard_te_sweep_deg.is_finite()
        && w.max_inboard_te_sweep_deg.is_finite()
        && w.min_inboard_te_sweep_deg < w.max_inboard_te_sweep_deg
        && w.min_inboard_te_sweep_deg > -90.0
        && w.max_inboard_te_sweep_deg < 90.0
}

fn lower(
    id: &'static str,
    actual: f64,
    minimum: f64,
    unit: &'static str,
    policy: ConstraintPolicy,
) -> ConstraintResidual {
    ConstraintResidual::scaled(
        id,
        ConstraintFamily::Geometry,
        actual,
        minimum,
        unit,
        minimum - actual,
        policy,
    )
}

fn window(
    id: &'static str,
    actual: f64,
    minimum: f64,
    maximum: f64,
    unit: &'static str,
    policy: ConstraintPolicy,
) -> ConstraintResidual {
    let (limit, raw) = if actual - minimum <= maximum - actual {
        (minimum, minimum - actual)
    } else {
        (maximum, actual - maximum)
    };
    // A sweep window can include zero. Normalize by its nonzero width;
    // a tiny miss below zero must not be inflated by the generic 1e-9 floor.
    ConstraintResidual::direct(
        id,
        ConstraintFamily::Geometry,
        actual,
        limit,
        unit,
        raw,
        (raw / (maximum - minimum)).max(0.0),
        policy,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::builder::AircraftBuilder;

    fn fixture() -> (AlasConfig, DesignVector, Airplane) {
        let mut config = AlasConfig::default();
        config
            .optimizer
            .weights
            .transport_planform_constraints_enabled = true;
        config.optimizer.weights.transport_shape_priors_enabled = true;
        let design = DesignVector::default();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap();
        (config, design, plane)
    }

    #[test]
    fn preferences_are_opt_in_and_off_stays_off() {
        let (mut config, design, plane) = fixture();
        assert!(residuals(&plane, &design, &config, ConstraintPolicy::Off).is_empty());
        config.optimizer.weights.transport_shape_priors_enabled = false;
        assert!(residuals(&plane, &design, &config, ConstraintPolicy::Hard).is_empty());
        config.optimizer.weights.transport_shape_priors_enabled = true;
        config
            .optimizer
            .weights
            .transport_planform_constraints_enabled = false;
        assert!(residuals(&plane, &design, &config, ConstraintPolicy::Hard).is_empty());
    }

    #[test]
    fn violated_shape_preferences_never_become_hard_requirements() {
        let (mut config, design, plane) = fixture();
        config.optimizer.weights.min_root_wingbox_depth_m = 100.0;
        let rows = residuals(&plane, &design, &config, ConstraintPolicy::Hard);
        assert_eq!(rows.len(), 8);
        assert!(rows.iter().all(|r| r.policy == ConstraintPolicy::Soft));
        assert!(rows[0].normalized_violation > 0.0);
        assert!(rows.iter().all(|r| r.normalized_violation.is_finite()));
        let diagnostics = residuals(&plane, &design, &config, ConstraintPolicy::Diagnostic);
        assert!(diagnostics
            .iter()
            .all(|r| r.policy == ConstraintPolicy::Diagnostic));
    }

    #[test]
    fn invalid_assessment_and_nonfinite_limits_are_hard_errors() {
        let (mut config, design, mut plane) = fixture();
        plane.wings.clear();
        let rows = residuals(&plane, &design, &config, ConstraintPolicy::Diagnostic);
        assert_eq!(rows[0].id, "transport_planform_invalid");
        assert_eq!(rows[0].policy, ConstraintPolicy::Hard);
        config.optimizer.weights.min_root_wingbox_depth_m = f64::NAN;
        let rows = residuals(&plane, &design, &config, ConstraintPolicy::Hard);
        assert_eq!(rows[0].id, "transport_planform_invalid");
        assert!(rows[0].normalized_violation > 0.0);
    }

    #[test]
    fn zero_degree_boundary_is_normalized_by_window_width() {
        let row = window("angle", -0.22, 0.0, 22.0, "deg", ConstraintPolicy::Soft);
        assert!((row.normalized_violation - 0.01).abs() < 1e-12);
        let on_boundary = window("angle", 0.0, 0.0, 22.0, "deg", ConstraintPolicy::Soft);
        assert_eq!(on_boundary.normalized_violation, 0.0);
    }

    #[test]
    fn geometry_drives_wingbox_depth() {
        let (config, design, mut plane) = fixture();
        let baseline = residuals(&plane, &design, &config, ConstraintPolicy::Hard);
        plane.wings[0].xsecs[0].chord *= 0.5;
        let changed = residuals(&plane, &design, &config, ConstraintPolicy::Hard);
        assert!((changed[0].actual / baseline[0].actual - 0.5).abs() < 1e-12);
    }
}
