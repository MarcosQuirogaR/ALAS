// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The planform, wing-loading and accommodation requirement family.

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, ConstraintPolicy, ObjectiveWeights};
use alas_geom::aircraft::airplane::Airplane;
use alas_stab::trim::tail_volume_coefficients;

use super::sizing::SizingOutcome;
use super::types::ConstraintFamily::Geometry;
use super::types::ConstraintResidual;

/// The span limit, wing-area cap, wing-loading floor, tail-volume window and
/// passenger/cargo-capacity shortfall.
pub(super) fn geometry_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    weights: &ObjectiveWeights,
    policy: ConstraintPolicy,
    target_num_passengers: i64,
    target_cargo_payload_kg: f64,
) -> Vec<ConstraintResidual> {
    if policy == ConstraintPolicy::Off {
        return Vec::new();
    }
    let req = &config.requirements;
    let objective = &config.optimizer.objective;
    let dv: DesignVector = outcome.history.dv;
    let plane: &Airplane = &outcome.plane;
    let mut residuals = Vec::new();

    if objective.max_span_m > 0.0 {
        residuals.push(ConstraintResidual::scaled(
            "span",
            Geometry,
            dv.span_m,
            objective.max_span_m,
            "m",
            dv.span_m - objective.max_span_m,
            policy,
        ));
    }

    residuals.push(ConstraintResidual::scaled(
        "wing_area",
        Geometry,
        plane.s_ref,
        req.max_wing_area_m2,
        "m^2",
        plane.s_ref - req.max_wing_area_m2,
        policy,
    ));

    let wing_loading_kg_m2 = outcome.sized.takeoff_mass_kg / plane.s_ref.max(1e-9);
    residuals.push(ConstraintResidual::scaled(
        "wing_loading",
        Geometry,
        wing_loading_kg_m2,
        req.min_wing_loading_kg_m2,
        "kg/m^2",
        req.min_wing_loading_kg_m2 - wing_loading_kg_m2,
        policy,
    ));

    // A tail-volume window is a plausibility band, not a requirement: the
    // surveyed tools rank it as a preference (research note
    // `.agent/reports/research-2026-09-05-mdo-drivers.md`, tier S), and the
    // legacy objective scores it as a quadratic add-on. Under a hard family
    // it is therefore ranked soft; diagnostic and off follow the family.
    let preference = match policy {
        ConstraintPolicy::Hard => ConstraintPolicy::Soft,
        other => other,
    };
    let (vh, vv) = tail_volume_coefficients(plane);
    if let Some(vh) = vh {
        residuals.push(tail_volume_residual(
            "tail_volume_h",
            vh,
            weights.min_hstab_volume_coef,
            weights.max_hstab_volume_coef,
            preference,
        ));
    }
    if let Some(vv) = vv {
        residuals.push(tail_volume_residual(
            "tail_volume_v",
            vv,
            weights.min_vstab_volume_coef,
            weights.max_vstab_volume_coef,
            preference,
        ));
    }

    residuals.push(if req.aircraft_type == "cargo" {
        ConstraintResidual::scaled(
            "passenger_shortfall",
            Geometry,
            req.cargo_payload_kg,
            target_cargo_payload_kg,
            "kg",
            target_cargo_payload_kg - req.cargo_payload_kg,
            policy,
        )
    } else {
        ConstraintResidual::scaled(
            "passenger_shortfall",
            Geometry,
            req.num_passengers as f64,
            target_num_passengers as f64,
            "passengers",
            (target_num_passengers - req.num_passengers) as f64,
            policy,
        )
    });

    residuals
}

/// A two-sided window residual: violated below `min_coef` or above
/// `max_coef`, and otherwise reported against whichever bound is nearer with
/// a negative (compliant) raw residual.
fn tail_volume_residual(
    id: &'static str,
    value: f64,
    min_coef: f64,
    max_coef: f64,
    policy: ConstraintPolicy,
) -> ConstraintResidual {
    let (limit, raw_residual) = if value < min_coef {
        (min_coef, min_coef - value)
    } else if value > max_coef {
        (max_coef, value - max_coef)
    } else {
        let slack_to_min = value - min_coef;
        let slack_to_max = max_coef - value;
        if slack_to_min < slack_to_max {
            (min_coef, -slack_to_min)
        } else {
            (max_coef, -slack_to_max)
        }
    };
    ConstraintResidual::scaled(id, Geometry, value, limit, "-", raw_residual, policy)
}
