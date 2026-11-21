// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The initial cruise altitude (ICA) rows of a turbofan.
//!
//! The criterion is the cruise-level planner's: maximum-climb thrust gives a
//! 300 ft/min residual climb ([`TOC_RESIDUAL_CLIMB_M_S`], one constant shared
//! with `mission_model::cruise_levels`) at the top-of-climb mass, which is
//! the brake-release mass less the take-off roll and climb fuel the mission
//! integrates to that level (`mission_model::initial_cruise`), not a fixed
//! fraction.
//!
//! - A declared requirement (`requirements.initial_cruise_altitude_m`, at
//!   `requirements.initial_cruise_mass_kg`, default the maximum take-off
//!   weight) is the [`ICA_ROW`] row: a constraint with the caller's role,
//!   and a floor on a sized engine (`engine::sizing::requirement`).
//! - Without one nothing is required: the [`ICA_CAPABILITY_ROW`] Diagnostic
//!   reports the achievable ICA at the maximum take-off weight, up to the
//!   design cruise altitude, and sizes nothing.
//!
//! A turboprop has no rows here (its level search is not validated, see
//! `cruise_deck::toc_rows`).

use alas_config::AlasConfig;
use alas_perf::performance::TOC_RESIDUAL_CLIMB_M_S;

use crate::mdo::mission_model::InitialCruisePoint;
use crate::mdo::sizing::candidate_model::candidate_mission_model;
use crate::mdo::types::{CandidateFuelArtifacts, ConstraintFamily, ConstraintResidual};
use crate::mdo::ResidualRole;

/// Identifier of the declared initial-cruise-altitude requirement row.
pub(crate) const ICA_ROW: &str = "initial_cruise_altitude";

/// Identifier of the achievable initial-cruise-altitude Diagnostic.
pub(crate) const ICA_CAPABILITY_ROW: &str = "initial_cruise_altitude_capability";

fn is_turboprop(config: &AlasConfig) -> bool {
    matches!(
        config.geometry.engine.active_model(),
        Ok(alas_config::ActiveEngineModel::Turboprop(_))
    )
}

/// The declared requirement evaluated on the candidate's mission, with the
/// brake-release mass it is stated at; `None` for a turboprop or when no ICA
/// is declared.
///
/// # Errors
///
/// No frozen plan, or the mission model rejects the climb, as text.
pub(crate) fn declared_initial_cruise(
    config: &AlasConfig,
    artifacts: &CandidateFuelArtifacts,
    mtow_kg: f64,
) -> Option<Result<(InitialCruisePoint, f64), String>> {
    if is_turboprop(config) {
        return None;
    }
    let (altitude_m, mass_kg) = config.requirements.initial_cruise_requirement(mtow_kg)?;
    Some((|| {
        let plan = artifacts
            .frozen_plan
            .as_ref()
            .ok_or_else(|| "the closure carries no frozen mission plan".to_owned())?;
        let model = candidate_mission_model(config, artifacts)?;
        let point = model.initial_cruise_point(
            mass_kg,
            altitude_m,
            plan.range_m,
            plan.climb_rate_scales,
        )?;
        Ok((point, mass_kg))
    })())
}

/// The ICA rows of one candidate whose maximum take-off weight is
/// `mtow_kg`: the declared requirement with `role`, else the achievable-ICA
/// Diagnostic. Empty for a turboprop or without a frozen plan (the
/// `toc_evidence_gap` row then carries the gap).
pub(super) fn initial_cruise_rows(
    config: &AlasConfig,
    artifacts: &CandidateFuelArtifacts,
    mtow_kg: f64,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    if is_turboprop(config) || !config.performance.cruise_thrust_from_deck {
        return Vec::new();
    }
    match declared_initial_cruise(config, artifacts, mtow_kg) {
        Some(Ok((point, mass_kg))) => vec![ConstraintResidual::direct(
            ICA_ROW,
            ConstraintFamily::Performance,
            point.point.rate_m_s,
            TOC_RESIDUAL_CLIMB_M_S,
            "m/s",
            -point.margin_m_s(),
            (-point.margin_m_s() / TOC_RESIDUAL_CLIMB_M_S).max(0.0),
            role,
        )
        .with_detail(format!(
            "residual climb at maximum-climb thrust at the required initial cruise altitude \
             {:.0} m, from {mass_kg:.0} kg at brake release: top-of-climb mass {:.0} kg \
             after {:.0} kg of take-off and climb fuel",
            point.altitude_m, point.toc_mass_kg, point.climb_fuel_kg
        ))],
        Some(Err(reason)) => vec![ConstraintResidual::direct(
            ICA_ROW,
            ConstraintFamily::Performance,
            0.0,
            TOC_RESIDUAL_CLIMB_M_S,
            "m/s",
            TOC_RESIDUAL_CLIMB_M_S,
            1.0,
            role,
        )
        .with_detail(format!(
            "the required initial cruise altitude could not be evaluated: {reason}"
        ))],
        None => capability_row(config, artifacts, mtow_kg)
            .into_iter()
            .collect(),
    }
}

/// The achievable-ICA Diagnostic: the highest level, up to the route's
/// planned cruise level or the design cruise altitude, holding the criterion from `mtow_kg`.
fn capability_row(
    config: &AlasConfig,
    artifacts: &CandidateFuelArtifacts,
    mtow_kg: f64,
) -> Option<ConstraintResidual> {
    let plan = artifacts.frozen_plan.as_ref()?;
    let model = candidate_mission_model(config, artifacts).ok()?;
    // Searched up to, and reported against, the design cruise altitude
    // (information only).
    let cap_m = config.requirements.cruise_altitude_m;
    let achievable = model
        .achievable_initial_cruise_m(mtow_kg, plan.range_m, plan.climb_rate_scales, cap_m)
        .ok()?;
    let (actual_m, detail) = match achievable {
        Some(level_m) => (
            level_m,
            format!(
                "achievable initial cruise altitude from {mtow_kg:.0} kg at brake release \
                 (300 ft/min residual climb at maximum-climb thrust at the flown top-of-climb \
                 mass), searched up to the design cruise altitude; information only"
            ),
        ),
        None => (
            0.0,
            format!(
                "no level of the climb geometry holds a 300 ft/min residual climb from \
                 {mtow_kg:.0} kg at brake release; information only"
            ),
        ),
    };
    Some(
        ConstraintResidual::direct(
            ICA_CAPABILITY_ROW,
            ConstraintFamily::Performance,
            actual_m,
            cap_m,
            "m",
            cap_m - actual_m,
            0.0,
            ResidualRole::Diagnostic,
        )
        .with_detail(detail),
    )
}
