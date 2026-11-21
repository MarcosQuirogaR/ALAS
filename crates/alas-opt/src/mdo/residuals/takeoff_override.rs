// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The notices of a stated design take-off or landing field length that was
//! ignored.
//!
//! The effective requirement is the shorter of the route's take-off distance
//! available and the stated one (`AlasConfig::departure_airport_record`), so a
//! stated value longer than the route's never lengthens the runway. It is not
//! applied silently: this Diagnostic row says so in every mode.

use alas_config::AlasConfig;

use crate::mdo::types::ConstraintFamily::Performance;
use crate::mdo::types::ConstraintResidual;
use crate::mdo::ResidualRole;

/// The notice row, when the stated field length exceeds the route's.
pub(super) fn notice(config: &AlasConfig) -> Vec<ConstraintResidual> {
    let mut rows = Vec::new();
    if let Some((stated, route)) = config.takeoff_override_ignored() {
        rows.push(row(
            "takeoff_field_override_ignored",
            "take-off",
            "departure",
            stated,
            route,
        ));
    }
    if let Some((stated, route)) = config.landing_override_ignored() {
        rows.push(row(
            "landing_field_override_ignored",
            "landing",
            "arrival",
            stated,
            route,
        ));
    }
    rows
}

fn row(name: &'static str, phase: &str, end: &str, stated: f64, route: f64) -> ConstraintResidual {
    ConstraintResidual::direct(
        name,
        Performance,
        stated,
        route,
        "m",
        stated - route,
        0.0,
        ResidualRole::Diagnostic,
    )
    .with_detail(format!(
        "the stated design {phase} field length of {stated:.0} m is longer than the {end} aerodrome's {route:.0} m and is ignored: a stated value can only shorten the requirement"
    ))
}
