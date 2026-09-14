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

/// The span limit, wing-area cap, wing-loading floor, transport body-attitude
/// window, tail-volume window and passenger/cargo-capacity shortfall.
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

    // The requirement is a design wing loading, MTOW over area: the design
    // gross mass the components were sized against. That is the closed
    // takeoff mass of a coupled clean-sheet design and the declared MTOW of
    // a fixed aircraft, which does not stop being the same wing when it is
    // dispatched light on a short route.
    let wing_loading_kg_m2 = outcome.sized.design_gross_mass_kg / plane.s_ref.max(1e-9);
    residuals.push(ConstraintResidual::scaled(
        "wing_loading",
        Geometry,
        wing_loading_kg_m2,
        req.min_wing_loading_kg_m2,
        "kg/m^2",
        req.min_wing_loading_kg_m2 - wing_loading_kg_m2,
        policy,
    ));

    // The clean-sheet transport search must keep the aircraft body attitude
    // in the configured cruise window.  This is a requirement on the
    // three-dimensional trimmed aircraft, not a guessed local MSES alpha
    // limit: the pipeline still maps this solved body angle through the
    // section twist and downwash explicitly.  Registered aircraft retain
    // their measured body attitude for parity/audit reporting; applying a
    // generic 2--4 degree design target to them would rewrite the reference
    // aircraft rather than test it.
    if config.optimizer.design_space.mode == alas_config::DesignMode::CleanSheet
        && req.aircraft_type == "passenger"
        && weights.transport_planform_constraints_enabled
    {
        residuals.push(body_alpha_window_residual(
            outcome.geometric_body_alpha_deg,
            weights.geometric_body_alpha_min_deg,
            weights.geometric_body_alpha_max_deg,
            policy,
        ));
    }

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

    if req.aircraft_type == "cargo" {
        residuals.push(ConstraintResidual::scaled(
            "cargo_shortfall",
            Geometry,
            outcome.sized.carried_cargo_payload_kg,
            target_cargo_payload_kg,
            "kg",
            target_cargo_payload_kg - outcome.sized.carried_cargo_payload_kg,
            policy,
        ));
    } else if target_num_passengers > 0 {
        // Every study -- registered aircraft or clean-sheet alike -- sizes
        // its cabin from the class mix and fills the candidate floor; there
        // is no copied integer passenger target to violate. This residual
        // only appears when `DesignRequirements::min_passenger_capacity` is
        // set: a one-sided floor check on the resolved capacity, not an
        // equality constraint.
        residuals.push(ConstraintResidual::scaled(
            "passenger_shortfall",
            Geometry,
            outcome.sized.carried_passengers as f64,
            target_num_passengers as f64,
            "passengers",
            (target_num_passengers - outcome.sized.carried_passengers) as f64,
            policy,
        ));
    }

    residuals
}

/// A two-sided residual for the configured geometric body-attitude window.
///
/// The raw value is positive only outside the interval; inside, the margin
/// to the nearer bound is reported as a negative value.  The residual uses a
/// degree unit and the nearest violated bound so the optimizer receives a
/// useful direction without treating the interval as a local-section solver
/// validity claim.
fn body_alpha_window_residual(
    actual_deg: f64,
    min_deg: f64,
    max_deg: f64,
    policy: ConstraintPolicy,
) -> ConstraintResidual {
    let (limit_deg, raw_residual) = if actual_deg < min_deg {
        (min_deg, min_deg - actual_deg)
    } else if actual_deg > max_deg {
        (max_deg, actual_deg - max_deg)
    } else {
        let slack_to_min = actual_deg - min_deg;
        let slack_to_max = max_deg - actual_deg;
        if slack_to_min < slack_to_max {
            (min_deg, -slack_to_min)
        } else {
            (max_deg, -slack_to_max)
        }
    };
    // A degree interval crosses zero, so scaling by the bound's magnitude is
    // well-defined for the configured positive transport window.  The
    // generic constructor still protects malformed zero/negative bounds.
    ConstraintResidual::scaled(
        "geometric_body_alpha",
        Geometry,
        actual_deg,
        limit_deg,
        "deg",
        raw_residual,
        policy,
    )
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
