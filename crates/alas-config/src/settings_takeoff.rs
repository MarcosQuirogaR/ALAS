// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The stated design take-off field length ([`AlasConfig::toda_override_m`]):
//! the one effective take-off distance available every consumer reads.
//!
//! The requirement is the shorter of the stated value and the departure
//! aerodrome's take-off distance available, so a stated value can only make
//! the requirement more demanding. A longer value is never used to lengthen
//! the runway; [`AlasConfig::takeoff_override_ignored`] reports it so a
//! notice can say so.

use crate::airports::{self, Airport, UnknownAirport};
use crate::AlasConfig;

/// Plausible range of a stated design take-off field length, m.
///
/// Engineering bounds of the transport classes the catalogue covers: below
/// 600 m no certified transport of these classes takes off at its maximum
/// mass (the ATR 72-600 needs about 1,300 m at ISA sea level, ATR 72-600
/// factsheet, 2024), and 5,000 m is
/// above any runway in the shipped airport database. A value outside is a
/// typo, not a requirement; a value above the route's is ignored anyway.
pub const TAKEOFF_FIELD_RANGE_M: std::ops::RangeInclusive<f64> = 600.0..=5000.0;

impl AlasConfig {
    /// The departure aerodrome with the effective take-off distance available
    /// applied: the shorter of the route's and the stated one.
    ///
    /// # Errors
    ///
    /// [`UnknownAirport`] when the name does not resolve.
    pub fn departure_airport_record(&self) -> Result<&'static Airport, UnknownAirport> {
        Ok(self.with_stated_takeoff(airports::get(&self.departure_airport)?))
    }

    /// `airport` (a departure resolved by any route, such as a dispatched
    /// one) with the shorter-only stated take-off distance applied; the very
    /// record when nothing is stated, the stated value is not shorter, or it
    /// lies outside [`TAKEOFF_FIELD_RANGE_M`] (the validator rejects it).
    pub fn with_stated_takeoff(&self, airport: &'static Airport) -> &'static Airport {
        let stated = self
            .toda_override_m
            .filter(|stated| TAKEOFF_FIELD_RANGE_M.contains(stated) && *stated < airport.toda_m);
        airports::with_declared_distances(airport, stated, None)
    }

    /// As [`Self::with_stated_takeoff`] for an owned record.
    pub fn with_stated_takeoff_owned(&self, airport: &Airport) -> Airport {
        let mut applied = airport.clone();
        if let Some(stated) = self.toda_override_m {
            if TAKEOFF_FIELD_RANGE_M.contains(&stated) && stated < applied.toda_m {
                applied.toda_m = stated;
                applied.notes = airports::stated_takeoff_note(&applied.notes);
            }
        }
        applied
    }

    /// `(stated, route)` take-off distances when the stated one is longer
    /// than the route's and is therefore ignored.
    pub fn takeoff_override_ignored(&self) -> Option<(f64, f64)> {
        let stated = self.toda_override_m?;
        let route = airports::get(&self.departure_airport).ok()?.toda_m;
        (TAKEOFF_FIELD_RANGE_M.contains(&stated) && stated > route).then_some((stated, route))
    }

    /// Why the stated take-off field length is unusable, if it is.
    pub(crate) fn takeoff_override_issue(&self) -> Option<String> {
        let stated = self.toda_override_m?;
        let range = TAKEOFF_FIELD_RANGE_M;
        (!(stated.is_finite() && range.contains(&stated))).then(|| {
            format!(
                "The design take-off field length must be finite and between {} and {} m (got {stated}); leave it unset to use the departure aerodrome's.",
                range.start(),
                range.end()
            )
        })
    }
}

// Tests assert on values built from fixtures, so a failed unwrap is the
// assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn heathrow() -> AlasConfig {
        AlasConfig {
            departure_airport: "London Heathrow (EGLL)".to_owned(),
            ..AlasConfig::default()
        }
    }

    #[test]
    fn absent_the_route_distance_is_the_requirement_and_the_record_is_the_database_one() {
        let config = heathrow();
        let record = config.departure_airport_record().unwrap();
        assert!(std::ptr::eq(
            record,
            airports::get("London Heathrow (EGLL)").unwrap()
        ));
        assert_eq!(config.takeoff_override_ignored(), None);
    }

    #[test]
    fn a_shorter_stated_value_is_the_effective_requirement() {
        let mut config = heathrow();
        let route = airports::get(&config.departure_airport).unwrap().toda_m;
        config.toda_override_m = Some(route - 500.0);
        let record = config.departure_airport_record().unwrap();
        assert_eq!(record.toda_m, route - 500.0);
        assert_eq!(record.icao, "EGLL");
        assert_eq!(config.takeoff_override_ignored(), None);
    }

    #[test]
    fn a_stated_value_equal_to_the_route_is_neither_applied_nor_reported() {
        let mut config = heathrow();
        let route = airports::get(&config.departure_airport).unwrap();
        config.toda_override_m = Some(route.toda_m);
        assert!(std::ptr::eq(
            config.departure_airport_record().unwrap(),
            route
        ));
        assert_eq!(config.takeoff_override_ignored(), None);
    }

    #[test]
    fn the_record_itself_refuses_values_the_validator_rejects_and_names_the_source() {
        let mut config = heathrow();
        let route = airports::get(&config.departure_airport).unwrap();
        for bad in [0.0, -1.0, 100.0] {
            config.toda_override_m = Some(bad);
            assert!(std::ptr::eq(
                config.departure_airport_record().unwrap(),
                route
            ));
        }
        config.toda_override_m = Some(2_000.0);
        let record = config.departure_airport_record().unwrap();
        assert!(
            record.notes.contains("stated for this case"),
            "{}",
            record.notes
        );
        let owned = config.with_stated_takeoff_owned(route);
        assert_eq!(owned.toda_m, 2_000.0);
        assert!(owned.notes.contains("stated for this case"));
    }

    #[test]
    fn a_longer_stated_value_never_lengthens_the_runway_and_is_reported() {
        let mut config = heathrow();
        let route = airports::get(&config.departure_airport).unwrap().toda_m;
        config.toda_override_m = Some(route + 800.0);
        // The public API itself refuses a longer value, not just this wrapper.
        let base = airports::get(&config.departure_airport).unwrap();
        assert!(std::ptr::eq(
            airports::with_declared_distances(base, Some(route + 800.0), None),
            base
        ));
        assert_eq!(config.departure_airport_record().unwrap().toda_m, route);
        assert_eq!(
            config.takeoff_override_ignored(),
            Some((route + 800.0, route))
        );
    }

    #[test]
    fn values_outside_the_plausible_range_are_rejected_with_a_message() {
        for bad in [0.0, -1.0, 599.0, 5_001.0, f64::NAN, f64::INFINITY] {
            let mut config = heathrow();
            config.toda_override_m = Some(bad);
            let issues = crate::validation::validate(&config);
            assert!(
                issues
                    .iter()
                    .any(|issue| issue.field_path == "toda_override_m"
                        && issue.message.contains("between 600 and 5000")),
                "{bad}"
            );
        }
        let mut ok = heathrow();
        ok.toda_override_m = Some(2_000.0);
        assert!(crate::validation::validate(&ok)
            .iter()
            .all(|issue| issue.field_path != "toda_override_m"));
    }

    #[test]
    fn the_field_round_trips_and_old_files_without_it_load() {
        let mut config = heathrow();
        config.toda_override_m = Some(2_345.0);
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(value["toda_override_m"], 2_345.0);
        let back = AlasConfig::from_value(&value).unwrap();
        assert_eq!(back.toda_override_m, Some(2_345.0));
        let old = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
        assert_eq!(old.toda_override_m, None);
        let plain = serde_json::to_value(&old).unwrap();
        assert!(plain.get("toda_override_m").is_none());
    }
}
