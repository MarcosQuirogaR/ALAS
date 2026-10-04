// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The manufacturer public planning CG envelope, checked in the loop exactly
//! as the reporting feasibility verdict checks it.
//!
//! Where a registered aircraft publishes a planning envelope, the reporting
//! verdict rejects an analysed takeoff CG outside its flight curve
//! (`alas_pipeline::feasibility::planning`). A search that does not see that
//! limit ranks designs the delivery ladder must then reject, so the same
//! comparison is a hard residual here: the lumped takeoff CG of the
//! dispatched route's load, referred to the published leading edge and chord, against the
//! published forward and aft limits at that mass. Stations are metres aft of
//! the nose; limits are percent of the published MAC.

use alas_config::{AlasConfig, CgEnvelopeCondition, CgEnvelopeEvidence};

use super::super::types::ConstraintFamily::Balance;
use super::super::types::{ConstraintResidual, ResidualRole};
use super::SizingOutcome;

pub(super) fn residuals(outcome: &SizingOutcome, config: &AlasConfig) -> Vec<ConstraintResidual> {
    let Ok(preset) = alas_config::presets::get(&config.preset) else {
        return Vec::new();
    };
    let reference = &preset.reference;
    let (CgEnvelopeEvidence::PublicPlanning, Some(envelope)) =
        (reference.cg_evidence, reference.planning_cg_envelope)
    else {
        return Vec::new();
    };
    // The reporting check sums the nonnegative components and the
    // dispatched route's brake-release fuel at the sized fuel centroid.
    // Without a flown mission the report's analysed load is the sized one.
    let fuel_kg = if config.mission.enabled {
        outcome.sized.flown_dispatch().plan.takeoff_fuel_kg()
    } else {
        outcome.masses.physical_fuel_mass_kg().unwrap_or(0.0)
    };
    let mut mass_kg = fuel_kg;
    let mut moment_kg_m = fuel_kg * outcome.coords.fuel[0];
    for ((name, kg), (_, xyz)) in outcome
        .masses
        .as_pairs()
        .into_iter()
        .zip(outcome.coords.as_pairs())
    {
        if name != alas_mass::breakdown::FUEL {
            mass_kg += kg.max(0.0);
            moment_kg_m += kg.max(0.0) * xyz[0];
        }
    }
    let cg_x_m = moment_kg_m / mass_kg;
    let mac = envelope.mac_reference;
    if !mass_kg.is_finite()
        || !cg_x_m.is_finite()
        || !mac.lemac_from_aircraft_nose_m.is_finite()
        || !mac.mean_aerodynamic_chord_m.is_finite()
        || mac.mean_aerodynamic_chord_m <= 0.0
    {
        return vec![unavailable()];
    }
    let cg_pct_mac =
        100.0 * (cg_x_m - mac.lemac_from_aircraft_nose_m) / mac.mean_aerodynamic_chord_m;
    // Outside the published mass range the reporting verdict has no limit
    // to compare against either.
    let Some(limits) = envelope.limits_at(CgEnvelopeCondition::Flight, mass_kg) else {
        return Vec::new();
    };
    let mut residuals = vec![ConstraintResidual::direct(
        "public_planning_forward_cg",
        Balance,
        cg_pct_mac,
        limits.forward_pct_mac,
        "% MAC",
        limits.forward_pct_mac - cg_pct_mac,
        ((limits.forward_pct_mac - cg_pct_mac) / 100.0).max(0.0),
        ResidualRole::Constraint,
    )];
    if let Some(aft_pct_mac) = limits.aft_pct_mac {
        residuals.push(ConstraintResidual::direct(
            "public_planning_aft_cg",
            Balance,
            cg_pct_mac,
            aft_pct_mac,
            "% MAC",
            cg_pct_mac - aft_pct_mac,
            ((cg_pct_mac - aft_pct_mac) / 100.0).max(0.0),
            ResidualRole::Constraint,
        ));
    }
    residuals
}

fn unavailable() -> ConstraintResidual {
    ConstraintResidual::direct(
        "public_planning_cg_unavailable",
        Balance,
        1.0,
        0.0,
        "bool",
        1.0,
        1.0,
        ResidualRole::Constraint,
    )
}
