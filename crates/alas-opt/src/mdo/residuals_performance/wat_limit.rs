// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The take-off weight an aerodrome allows (the climb and obstacle part of the
//! AFM weight-altitude-temperature limit).
//!
//! The CS 25.121(b) second-segment gradient, the CS 25.115 net-path gradient
//! and the CAT.POL.A.210 obstacle clearance are certified at sea-level ISA
//! conditions (the certification basis) and flown at the departure aerodrome's
//! elevation and temperature (CS 25.111, AC 25-7D). At an elevated or hot
//! departure the thrust lapse and the lower air density cost gradient, and the
//! operator answers with a lower take-off weight, not with a different
//! aircraft. This module therefore splits the checks:
//!
//! Applies to `DesignMode::BaselineSandbox` only (a fixed certified aircraft); the other
//! modes keep every departure check hard at the real airport and the mission mass.
//!
//! * The residuals keep their ids and are evaluated on the **sea-level ISA**
//!   basis at the aerodrome's own runway and obstacle survey. A deficit there
//!   is a design deficit and stays a hard requirement.
//! * The same rows at the aerodrome's elevation and ISA deviation are reported
//!   as diagnostics (`*_at_departure`).
//! * The CS 25.113 field length against the runway (TORA and ASDA, `toda_m` is their minimum)
//!   is read at the departure and stays a hard row of its own; it takes part in the
//!   weight solve and is named when it limits.
//! * When those departure rows fail at the mission take-off mass, the largest
//!   mass that meets all of them is found by bracketed bisection on a uniform
//!   mass scale of the take-off and design gross masses (thrust, engine
//!   deck and drag table are fixed), and `takeoff_weight_limit` reports it
//!   with the limiting constraint and the reduction from the mission mass.
//!
//! The bracket is 50 % to 100 % of the mission mass; a limit below 50 % is not
//! reported as a weight limit and the departure rows stay hard. Bisection
//! stops at 0.2 % of the mass; the reported limit is the feasible end.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use alas_config::airports::Airport;
use alas_config::AlasConfig;

use super::performance_residuals_at;
use crate::mdo::sizing::SizingOutcome;
use crate::mdo::types::{ConstraintFamily, ConstraintResidual};
use crate::mdo::ResidualRole;

/// Row id of the reported take-off weight limit.
pub const WEIGHT_LIMIT_ID: &str = "takeoff_weight_limit";
/// Lowest mass fraction of the mission mass the search considers.
const LOWEST_FRACTION: f64 = 0.5;
/// Bisection stops when the bracket is narrower than this mass fraction.
const FRACTION_TOLERANCE: f64 = 0.002;
/// Row id prefixes the sea-level basis replaces.
const BASIS_PREFIXES: [&str; 4] = [
    "oei_second_segment",
    "oei_first_segment",
    "takeoff_net_path_gradient",
    "takeoff_obstacle_clearance",
];
/// The checks whose violation limits the weight, with their reporting names.
const LIMITING: [(&str, &str, &str); 4] = [
    (
        "oei_second_segment",
        "oei_second_segment_at_departure",
        "CS 25.121(b) second-segment gradient",
    ),
    (
        "takeoff_net_path_gradient",
        "takeoff_net_path_gradient_at_departure",
        "CS 25.115 net-path gradient",
    ),
    (
        "takeoff_obstacle_clearance",
        "takeoff_obstacle_clearance_at_departure",
        "CAT.POL.A.210 obstacle clearance",
    ),
    (
        "takeoff_field",
        "",
        "CS 25.113 take-off field length against TORA and ASDA",
    ),
];
/// Departure-condition copies of the first-segment row (reported, never limiting).
const FIRST_SEGMENT: (&str, &str) = ("oei_first_segment", "oei_first_segment_at_departure");

/// The weight limit of one candidate at its departure aerodrome.
#[derive(Debug, Clone, PartialEq)]
pub struct WeightLimit {
    /// Aerodrome ICAO code.
    pub icao: String,
    /// Largest take-off mass meeting every limiting check there, kg.
    pub limit_takeoff_mass_kg: f64,
    /// The mission take-off mass, kg.
    pub mission_takeoff_mass_kg: f64,
    /// The design gross mass scaled by the same fraction, kg.
    pub limit_gross_mass_kg: f64,
    /// Names of the checks that fail just above the limit.
    pub limiting: Vec<&'static str>,
}

impl WeightLimit {
    /// Payload or fuel to leave behind to depart at the limit, kg.
    pub fn reduction_kg(&self) -> f64 {
        self.mission_takeoff_mass_kg - self.limit_takeoff_mass_kg
    }
}

fn is_standard_day_at_sea_level(airport: &Airport) -> bool {
    airport.elevation_m.abs() < 1.0 && airport.isa_deviation_c.abs() < 1.0e-6
}

/// The same aerodrome (runway, survey key, position) at sea level and ISA,
/// leaked once per distinct aerodrome so the outcome can borrow it.
fn sea_level_isa(airport: &Airport) -> &'static Airport {
    type Key = (String, u64, u64, u64, u64);
    static CACHE: OnceLock<Mutex<HashMap<Key, &'static Airport>>> = OnceLock::new();
    let key = (
        airport.icao.clone(),
        airport.toda_m.to_bits(),
        airport.lda_m.to_bits(),
        airport.latitude_deg.to_bits(),
        airport.longitude_deg.to_bits(),
    );
    let mut map = CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    map.entry(key).or_insert_with(|| {
        let mut standard = airport.clone();
        standard.elevation_m = 0.0;
        standard.isa_deviation_c = 0.0;
        Box::leak(Box::new(standard))
    })
}

fn with_mass_fraction(outcome: &SizingOutcome, fraction: f64) -> SizingOutcome {
    let mut scaled = outcome.clone();
    scaled.sized.takeoff_mass_kg *= fraction;
    scaled.sized.design_gross_mass_kg *= fraction;
    scaled
}

/// Names of the limiting checks that fail in `rows`.
fn failing(rows: &[ConstraintResidual]) -> Vec<&'static str> {
    LIMITING
        .iter()
        .filter(|(id, _, _)| rows.iter().any(|row| row.id == *id && row.violated()))
        .map(|(_, _, name)| *name)
        .collect()
}

/// The largest mass fraction meeting every limiting check, by bisection;
/// `None` when even the lowest fraction fails. `rows_at_unity` are the
/// departure rows at the mission mass.
fn solve(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    rows_at_unity: &[ConstraintResidual],
) -> Option<(f64, Vec<&'static str>)> {
    let rows_at = |fraction: f64| {
        performance_residuals_at(
            &with_mass_fraction(outcome, fraction),
            config,
            ResidualRole::Diagnostic,
            None,
        )
    };
    if !failing(&rows_at(LOWEST_FRACTION)).is_empty() {
        return None;
    }
    let (mut feasible, mut infeasible) = (LOWEST_FRACTION, 1.0);
    let mut limiting = failing(rows_at_unity);
    while infeasible - feasible > FRACTION_TOLERANCE {
        let middle = 0.5 * (feasible + infeasible);
        let failed = failing(&rows_at(middle));
        if failed.is_empty() {
            feasible = middle;
        } else {
            infeasible = middle;
            limiting = failed;
        }
    }
    Some((feasible, limiting))
}

/// The performance residuals with the sea-level ISA certification basis for the
/// climb and obstacle rows and the weight limit of the departure aerodrome.
/// A departure at sea level on a standard day (or none) is evaluated as is.
pub(super) fn performance_residuals_with_limit(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let real = performance_residuals_at(outcome, config, role, None);
    // Only a fixed certified baseline reads the departure checks as operating
    // limitations; every mode that designs or sizes the aircraft for the mission
    // (clean sheet, reference adaptation) keeps them hard at the real airport.
    if config.optimizer.design_space.mode != alas_config::DesignMode::BaselineSandbox {
        return real;
    }
    let Some(departure) = outcome
        .departure
        .filter(|a| !is_standard_day_at_sea_level(a))
    else {
        return real;
    };
    let mut standard = outcome.clone();
    standard.departure = Some(sea_level_isa(departure));
    let basis = performance_residuals_at(&standard, config, role, Some(departure.elevation_m));

    let in_basis = |row: &ConstraintResidual| BASIS_PREFIXES.iter().any(|p| row.id.starts_with(p));
    let mut residuals: Vec<ConstraintResidual> =
        real.iter().filter(|row| !in_basis(row)).cloned().collect();
    residuals.extend(basis.into_iter().filter(|row| in_basis(row)));

    let departure_rows: Vec<ConstraintResidual> = real
        .iter()
        .filter(|row| LIMITING.iter().any(|(id, _, _)| row.id == *id) || row.id == FIRST_SEGMENT.0)
        .cloned()
        .collect();
    let failed = failing(&departure_rows);
    for mut row in departure_rows {
        let copy_id = LIMITING
            .iter()
            .find(|(id, _, _)| *id == row.id)
            .map_or(FIRST_SEGMENT.1, |(_, copy, _)| *copy);
        // The field row stays a hard requirement at its own id: it is read, not copied.
        if copy_id.is_empty() {
            continue;
        }
        row.id = copy_id;
        row.role = ResidualRole::Diagnostic;
        residuals.push(row);
    }
    if failed.is_empty() {
        return residuals;
    }
    let Some((fraction, limiting)) = solve(outcome, config, &real) else {
        // No weight in the bracket meets the checks: a deficit, never a weight limit.
        return real;
    };
    let limit = WeightLimit {
        icao: departure.icao.clone(),
        limit_takeoff_mass_kg: fraction * outcome.sized.takeoff_mass_kg,
        mission_takeoff_mass_kg: outcome.sized.takeoff_mass_kg,
        limit_gross_mass_kg: fraction * outcome.sized.design_gross_mass_kg,
        limiting,
    };
    residuals.push(limit_row(&limit, departure));
    residuals
}

fn limit_row(limit: &WeightLimit, departure: &Airport) -> ConstraintResidual {
    ConstraintResidual::direct(
        WEIGHT_LIMIT_ID,
        ConstraintFamily::Performance,
        limit.limit_takeoff_mass_kg,
        limit.mission_takeoff_mass_kg,
        "kg",
        limit.reduction_kg(),
        0.0,
        ResidualRole::Diagnostic,
    )
    .with_detail(format!(
        "TOW limited at {}: {} limit the take-off to {:.0} kg of the {:.0} kg mission mass; leave {:.0} kg of payload or fuel behind. An operating limitation, not a design violation: the sea-level ISA certification rows are reported separately",
        departure.icao,
        limit.limiting.join(" / "),
        limit.limit_takeoff_mass_kg,
        limit.mission_takeoff_mass_kg,
        limit.reduction_kg(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &'static str, violated: bool) -> ConstraintResidual {
        ConstraintResidual::direct(
            id,
            ConstraintFamily::Performance,
            0.0,
            0.0,
            "m",
            0.0,
            if violated { 1.0 } else { 0.0 },
            ResidualRole::Diagnostic,
        )
    }

    #[test]
    fn only_limiting_checks_that_fail_are_named() {
        let rows = [
            row("oei_second_segment", true),
            row("takeoff_obstacle_clearance", false),
            row("oei_first_segment", true),
        ];
        assert_eq!(failing(&rows), vec!["CS 25.121(b) second-segment gradient"]);
    }

    #[test]
    fn sea_level_copy_is_standard_and_cached() {
        let mut airport = alas_config::airports::get("LEMD")
            .expect("LEMD is tabulated")
            .clone();
        assert!(!is_standard_day_at_sea_level(&airport));
        let first = sea_level_isa(&airport);
        assert!(is_standard_day_at_sea_level(first));
        assert_eq!(first.icao, airport.icao);
        assert!(std::ptr::eq(first, sea_level_isa(&airport)));
        airport.toda_m += 1.0;
        assert!(!std::ptr::eq(first, sea_level_isa(&airport)));
    }
}
