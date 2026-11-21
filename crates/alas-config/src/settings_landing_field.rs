// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The stated design landing field length ([`AlasConfig::lda_override_m`]),
//! with the same rule as the take-off one (`settings_takeoff.rs`): the
//! requirement is the shorter of the stated value and the arrival
//! aerodrome's landing distance available, so a stated value can only make
//! it more demanding, and a longer one is reported as ignored.

use super::TAKEOFF_FIELD_RANGE_M;
use crate::airports::{self, Airport, UnknownAirport};
use crate::AlasConfig;

/// Plausible range of a stated design landing field length, m.
///
/// Below 600 m no certified transport of these classes lands at its maximum
/// landing mass: the ATR 72-600F needs a landing field length (EASA Air Ops,
/// ISA sea level, MLW 22,350 kg) of 915 m (ATR 72-600F factsheet, 2024, p.2;
/// the passenger 72-600 shares the MLW); 5,000 m is
/// above any runway in the shipped airport database. A value outside is a
/// typo, not a requirement.
pub const LANDING_FIELD_RANGE_M: std::ops::RangeInclusive<f64> = 600.0..=5000.0;

impl AlasConfig {
    /// The arrival aerodrome with the effective landing distance available
    /// applied: the shorter of the route's and the stated one.
    ///
    /// # Errors
    ///
    /// [`UnknownAirport`] when the name does not resolve.
    pub fn arrival_airport_record(&self) -> Result<&'static Airport, UnknownAirport> {
        Ok(self.with_stated_landing(airports::get(&self.arrival_airport)?))
    }

    /// `airport` (an arrival resolved by any route) with the shorter-only
    /// stated landing distance applied; the very record when nothing is
    /// stated, the stated value is not shorter, or it lies outside
    /// [`LANDING_FIELD_RANGE_M`] (the validator rejects it).
    pub fn with_stated_landing(&self, airport: &'static Airport) -> &'static Airport {
        let stated = self
            .lda_override_m
            .filter(|stated| LANDING_FIELD_RANGE_M.contains(stated) && *stated < airport.lda_m);
        airports::with_declared_distances(airport, None, stated)
    }

    /// As [`Self::with_stated_landing`] for an owned record.
    pub fn with_stated_landing_owned(&self, airport: &Airport) -> Airport {
        let mut applied = airport.clone();
        if let Some(stated) = self.lda_override_m {
            if LANDING_FIELD_RANGE_M.contains(&stated) && stated < applied.lda_m {
                applied.lda_m = stated;
                applied.notes = airports::stated_landing_note(&applied.notes);
            }
        }
        applied
    }

    /// Both endpoints of a dispatched route with the stated design field lengths
    /// applied by one rule (shorter-only): a stated length belongs to the
    /// configured aerodrome, so it applies to an endpoint only when that endpoint
    /// is the configured departure (take-off) or arrival (landing). A plan that
    /// swapped the city pair keeps each endpoint's own data.
    pub fn with_stated_for_route(
        &self,
        origin: &Airport,
        destination: &Airport,
    ) -> (Airport, Airport) {
        let configured = |name: &str| {
            airports::get(name)
                .ok()
                .map(|airport| airport.icao.as_str())
        };
        let mut origin = origin.clone();
        let mut destination = destination.clone();
        if configured(&self.departure_airport) == Some(origin.icao.as_str()) {
            origin = self.with_stated_takeoff_owned(&origin);
        }
        if configured(&self.arrival_airport) == Some(destination.icao.as_str()) {
            destination = self.with_stated_landing_owned(&destination);
        }
        (origin, destination)
    }

    /// The stated design field lengths in force, (take-off, landing): each
    /// only when stated and within its plausible range.
    pub fn stated_field_lengths(&self) -> (Option<f64>, Option<f64>) {
        (
            self.toda_override_m
                .filter(|stated| TAKEOFF_FIELD_RANGE_M.contains(stated)),
            self.lda_override_m
                .filter(|stated| LANDING_FIELD_RANGE_M.contains(stated)),
        )
    }

    /// `(stated, route)` landing distances when the stated one is longer than
    /// the route's and is therefore ignored.
    pub fn landing_override_ignored(&self) -> Option<(f64, f64)> {
        let stated = self.lda_override_m?;
        let route = airports::get(&self.arrival_airport).ok()?.lda_m;
        (LANDING_FIELD_RANGE_M.contains(&stated) && stated > route).then_some((stated, route))
    }

    /// Why the stated landing field length is unusable, if it is.
    pub(crate) fn landing_override_issue(&self) -> Option<String> {
        let stated = self.lda_override_m?;
        let range = LANDING_FIELD_RANGE_M;
        (!(stated.is_finite() && range.contains(&stated))).then(|| {
            format!(
                "The design landing field length must be finite and between {} and {} m (got {stated}); leave it unset to use the arrival aerodrome's.",
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

    fn dubai() -> AlasConfig {
        AlasConfig::default()
    }

    #[test]
    fn a_shorter_stated_value_is_the_requirement_and_a_longer_one_is_ignored() {
        let mut config = dubai();
        let route = airports::get(&config.arrival_airport).unwrap().lda_m;
        config.lda_override_m = Some(route - 400.0);
        assert_eq!(
            config.arrival_airport_record().unwrap().lda_m,
            route - 400.0
        );
        assert_eq!(config.landing_override_ignored(), None);
        config.lda_override_m = Some(route + 700.0);
        assert_eq!(config.arrival_airport_record().unwrap().lda_m, route);
        assert_eq!(
            config.landing_override_ignored(),
            Some((route + 700.0, route))
        );
    }

    #[test]
    fn absent_the_route_distance_is_the_requirement_and_the_record_is_the_database_one() {
        let config = dubai();
        let record = config.arrival_airport_record().unwrap();
        assert!(std::ptr::eq(
            record,
            airports::get(&config.arrival_airport).unwrap()
        ));
        let mut longer = dubai();
        longer.lda_override_m = Some(record.lda_m + 900.0);
        assert!(std::ptr::eq(
            longer.arrival_airport_record().unwrap(),
            record
        ));
        let mut shorter = dubai();
        shorter.lda_override_m = Some(record.lda_m - 300.0);
        let owned = shorter.with_stated_landing_owned(record);
        assert_eq!(owned.lda_m, record.lda_m - 300.0);
        assert!(owned.notes.contains("landing distance available stated"));
    }

    #[test]
    fn the_take_off_and_landing_statements_stay_on_their_own_ends() {
        let mut config = dubai();
        let toda = config.departure_airport_record().unwrap().toda_m;
        config.lda_override_m = Some(1_999.0);
        assert_eq!(config.departure_airport_record().unwrap().toda_m, toda);
        assert_eq!(config.arrival_airport_record().unwrap().lda_m, 1_999.0);
    }

    #[test]
    fn values_outside_the_plausible_range_are_rejected_and_the_field_round_trips() {
        for bad in [0.0, 599.0, 5_001.0, f64::NAN] {
            let mut config = dubai();
            config.lda_override_m = Some(bad);
            assert!(crate::validation::validate(&config)
                .iter()
                .any(|issue| issue.field_path == "lda_override_m"
                    && issue.message.contains("between 600 and 5000")));
        }
        let mut config = dubai();
        config.lda_override_m = Some(1_800.0);
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(value["lda_override_m"], 1_800.0);
        assert!(value.get("toda_override_m").is_none());
        let back = AlasConfig::from_value(&value).unwrap();
        assert_eq!(back.lda_override_m, Some(1_800.0));
    }

    #[test]
    fn the_database_filter_refuses_a_longer_landing_distance_by_itself() {
        let base = airports::get(&dubai().arrival_airport).unwrap();
        assert!(std::ptr::eq(
            airports::with_declared_distances(base, None, Some(base.lda_m + 800.0)),
            base
        ));
        let shorter = airports::with_declared_distances(base, None, Some(base.lda_m - 300.0));
        assert_eq!(shorter.lda_m, base.lda_m - 300.0);
        assert_eq!(shorter.toda_m, base.toda_m);
    }

    #[test]
    fn a_stated_length_applies_to_a_dispatched_endpoint_only_when_it_is_the_configured_one() {
        let mut config = dubai();
        let departure = airports::get(&config.departure_airport).unwrap();
        let arrival = airports::get(&config.arrival_airport).unwrap();
        config.toda_override_m = Some(departure.toda_m - 400.0);
        config.lda_override_m = Some(arrival.lda_m - 400.0);
        let (origin, destination) = config.with_stated_for_route(departure, arrival);
        assert_eq!(origin.toda_m, departure.toda_m - 400.0);
        assert_eq!(destination.lda_m, arrival.lda_m - 400.0);
        // A plan that flies the other way keeps each endpoint's own data.
        let (origin, destination) = config.with_stated_for_route(arrival, departure);
        assert_eq!(origin.toda_m, arrival.toda_m);
        assert_eq!(destination.lda_m, departure.lda_m);
    }
}
