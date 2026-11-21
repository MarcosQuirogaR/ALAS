// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The aerodromes the Results field-performance figures are drawn for.
//!
//! A dispatched route carries the endpoints it flew; without one the
//! configured aerodromes are used. Either way the stated design field length
//! goes through `AlasConfig::with_stated_for_route`, the rule the optimizer's
//! acceptance route uses: shorter-only (landing by analogy), and only for
//! an endpoint that is the configured aerodrome. The landing-and-take-off
//! figures and the matching chart share this one pair, so a page never shows
//! two different aerodromes for one run.

use alas_config::airports::Airport;
use alas_config::AlasConfig;

/// The (departure, arrival) aerodromes of the run, stated lengths applied.
pub(super) fn route_airports(
    config: &AlasConfig,
    routed_origin: Option<&Airport>,
    routed_destination: Option<&Airport>,
) -> Option<(Airport, Airport)> {
    let origin = match routed_origin {
        Some(airport) => airport.clone(),
        None => alas_config::airports::get(&config.departure_airport)
            .ok()?
            .clone(),
    };
    let destination = match routed_destination {
        Some(airport) => airport.clone(),
        None => alas_config::airports::get(&config.arrival_airport)
            .ok()?
            .clone(),
    };
    Some(config.with_stated_for_route(&origin, &destination))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heathrow_dubai() -> (AlasConfig, &'static Airport, &'static Airport) {
        let config = AlasConfig::default();
        let departure = alas_config::airports::get(&config.departure_airport).unwrap();
        let arrival = alas_config::airports::get(&config.arrival_airport).unwrap();
        (config, departure, arrival)
    }

    #[test]
    fn a_stated_length_shortens_its_own_end_and_a_longer_one_is_ignored() {
        let (mut config, departure, arrival) = heathrow_dubai();
        config.lda_override_m = Some(arrival.lda_m - 500.0);
        config.toda_override_m = Some(departure.toda_m - 500.0);
        for routed in [(None, None), (Some(departure), Some(arrival))] {
            let (origin, destination) = route_airports(&config, routed.0, routed.1).unwrap();
            assert_eq!(destination.lda_m, arrival.lda_m - 500.0);
            assert_eq!(
                destination.toda_m, arrival.toda_m,
                "only its own end changes"
            );
            assert_eq!(origin.toda_m, departure.toda_m - 500.0);
            assert_eq!(origin.lda_m, departure.lda_m);
        }
        config.lda_override_m = Some(arrival.lda_m + 800.0);
        config.toda_override_m = Some(departure.toda_m + 800.0);
        let (origin, destination) = route_airports(&config, None, None).unwrap();
        assert_eq!(destination.lda_m, arrival.lda_m);
        assert_eq!(origin.toda_m, departure.toda_m);
    }

    #[test]
    fn a_swapped_city_pair_keeps_each_endpoints_own_data() {
        let (mut config, departure, arrival) = heathrow_dubai();
        config.lda_override_m = Some(arrival.lda_m - 500.0);
        config.toda_override_m = Some(departure.toda_m - 500.0);
        // The plan flies the other way: the stated lengths belong to the
        // configured aerodromes, which are now the opposite endpoints.
        let (origin, destination) =
            route_airports(&config, Some(arrival), Some(departure)).unwrap();
        assert_eq!(origin.toda_m, arrival.toda_m);
        assert_eq!(destination.lda_m, departure.lda_m);
        // The optimizer's acceptance route reads the same helper.
        let (acceptance_origin, acceptance_destination) =
            config.with_stated_for_route(arrival, departure);
        assert_eq!(acceptance_origin.toda_m, origin.toda_m);
        assert_eq!(acceptance_destination.lda_m, destination.lda_m);
    }
}
