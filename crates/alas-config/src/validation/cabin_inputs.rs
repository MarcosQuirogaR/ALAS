// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Checks on the cabin inputs a person can edit: the combined passenger mass,
//! the checked-bag share of it, and each class's seat geometry against the
//! fuselage width.

use crate::{AlasConfig, SeatClassConfig};

use super::{Severity, ValidationIssue};

/// Plausible range of the combined (body plus baggage) mass per passenger, kg.
/// Standard-mass tables (FAA AC 120-27F, EASA AMC1 CAT.POL.MAB.100) sit
/// between about 84 kg and 105 kg once a bag is added.
const PASSENGER_MASS_RANGE_KG: (f64, f64) = (50.0, 200.0);

/// Widest checked-bag allowance accepted per passenger, kg.
const MAX_CHECKED_BAG_KG: f64 = 60.0;

/// Main-aisle clear width used when the cabin leaves it to the regulation:
/// the 20 in of CS 25.815 for more than 19 passengers, m.
const DEFAULT_AISLE_M: f64 = 0.51;

/// Most seats abreast a single aisle may serve (CS 25.817), then twin.
const MAX_SINGLE_AISLE_ABREAST: i64 = 6;

pub(super) fn passenger_and_bag_masses_are_physical(config: &AlasConfig) -> Vec<ValidationIssue> {
    if config.requirements.aircraft_type != "passenger" {
        return Vec::new();
    }
    let mass = config.requirements.passenger_mass_kg;
    let bag = config.cabin.passenger.checked_bag_mass_kg;
    let mut issues = Vec::new();
    let (low, high) = PASSENGER_MASS_RANGE_KG;
    if !mass.is_finite() || !(low..=high).contains(&mass) {
        issues.push(ValidationIssue {
            field_path: "requirements.passenger_mass_kg".to_owned(),
            message: format!(
                "Mass per passenger {mass:?} kg must be finite and within [{low}, {high}] kg (body plus baggage)."
            ),
            severity: Severity::Error,
        });
    }
    if !bag.is_finite() || !(0.0..=MAX_CHECKED_BAG_KG).contains(&bag) {
        issues.push(ValidationIssue {
            field_path: "cabin.passenger.checked_bag_mass_kg".to_owned(),
            message: format!(
                "Checked baggage {bag:?} kg per passenger must be finite and within [0, {MAX_CHECKED_BAG_KG}] kg."
            ),
            severity: Severity::Error,
        });
    } else if mass.is_finite() && bag >= mass {
        issues.push(ValidationIssue {
            field_path: "cabin.passenger.checked_bag_mass_kg".to_owned(),
            message: format!(
                "Checked baggage {bag} kg is not below the combined mass per passenger {mass} kg, which would leave no mass for the occupant."
            ),
            severity: Severity::Error,
        });
    }
    issues
}

/// A declared seats-abreast count has to fit the fuselage: the seats and the
/// aisles beside them cannot be wider than the cabin. The layout reduces an
/// oversized count to the seats that fit, so this is a warning that says
/// what the declared value cannot do.
pub(super) fn declared_abreast_fits_the_fuselage(config: &AlasConfig) -> Vec<ValidationIssue> {
    if config.requirements.aircraft_type != "passenger" {
        return Vec::new();
    }
    let pax = &config.cabin.passenger;
    let interior_m = config.geometry.fuselage.diameter_m - 2.0 * pax.wall_thickness_m;
    let aisle_m = if pax.aisle_width_m > 0.0 {
        pax.aisle_width_m
    } else {
        DEFAULT_AISLE_M
    };
    let mut issues = Vec::new();
    for (name, class) in [
        ("first", &pax.first),
        ("business", &pax.business),
        ("economy", &pax.economy),
    ] {
        if let Some(message) = abreast_overflow(class, interior_m, aisle_m) {
            issues.push(ValidationIssue {
                field_path: format!("cabin.passenger.{name}.abreast"),
                message,
                severity: Severity::Warning,
            });
        }
    }
    issues
}

fn abreast_overflow(class: &SeatClassConfig, interior_m: f64, aisle_m: f64) -> Option<String> {
    if class.abreast <= 0 || !interior_m.is_finite() || interior_m <= 0.0 {
        return None;
    }
    let aisles = if class.abreast <= MAX_SINGLE_AISLE_ABREAST {
        1.0
    } else {
        2.0
    };
    let needed_m = class.abreast as f64 * class.width_m + aisles * aisle_m;
    (needed_m > interior_m).then(|| {
        format!(
            "{} seats abreast of {:.3} m with {aisles:.0} aisle(s) need {needed_m:.2} m, but the cabin is {interior_m:.2} m wide inside the walls; the layout will seat fewer abreast.",
            class.abreast, class.width_m
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_defaults_are_accepted() {
        let config = AlasConfig::default();
        assert!(passenger_and_bag_masses_are_physical(&config).is_empty());
        assert!(declared_abreast_fits_the_fuselage(&config).is_empty());
    }

    #[test]
    fn a_bag_as_heavy_as_the_passenger_is_an_error() {
        let mut config = AlasConfig::default();
        config.cabin.passenger.checked_bag_mass_kg = config.requirements.passenger_mass_kg;
        let issues = passenger_and_bag_masses_are_physical(&config);
        assert!(issues.iter().any(|issue| {
            issue.field_path == "cabin.passenger.checked_bag_mass_kg"
                && issue.severity == Severity::Error
        }));
    }

    #[test]
    fn an_implausible_passenger_mass_is_an_error() {
        let mut config = AlasConfig::default();
        config.requirements.passenger_mass_kg = 10.0;
        assert!(passenger_and_bag_masses_are_physical(&config)
            .iter()
            .any(|issue| issue.field_path == "requirements.passenger_mass_kg"));
    }

    #[test]
    fn nine_abreast_seats_do_not_fit_a_narrow_body() {
        let mut config = AlasConfig::default();
        config.geometry.fuselage.diameter_m = 3.9;
        config.cabin.passenger.economy.abreast = 9;
        config.cabin.passenger.economy.width_m = 0.46;
        let issues = declared_abreast_fits_the_fuselage(&config);
        assert!(issues
            .iter()
            .any(|issue| issue.field_path == "cabin.passenger.economy.abreast"));
    }
}
