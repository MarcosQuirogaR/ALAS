// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The passenger-shortfall residual is targeted at the explicit
//! `min_passenger_capacity` floor, not at a copy of the brief's passenger count.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_opt::assess_candidate;
use alas_opt::objective::DesignObjective;

/// With no floor configured there is no passenger-count residual for the
/// candidate cabin to disagree with; once a floor is configured the
/// residual's `limit` is exactly that floor.
#[test]
fn passenger_shortfall_residual_targets_configured_floor() {
    let config = AlasConfig::default();
    let design = DesignVector::default();
    let objective = DesignObjective::new(config.clone());
    let assessment = assess_candidate(&objective, &design.to_array()).unwrap();
    assert!(
        assessment
            .residuals
            .iter()
            .all(|residual| residual.id != "passenger_shortfall"),
        "no passenger floor is configured, so no passenger_shortfall residual should be scored"
    );

    let mut shorter = design;
    shorter.fuselage_length_m *= 0.90;
    let mut target = config;
    target.requirements.min_passenger_capacity = 525;
    let objective = DesignObjective::new(target);
    let assessment = assess_candidate(&objective, &shorter.to_array()).unwrap();
    let shortfall = assessment
        .residuals
        .iter()
        .find(|residual| residual.id == "passenger_shortfall")
        .unwrap();
    assert_eq!(
        shortfall.limit, 525.0,
        "the residual's floor is the configured min_passenger_capacity, not a copied brief count"
    );
}
