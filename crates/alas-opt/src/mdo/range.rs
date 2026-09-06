// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sizing mission's still-air distance.

use alas_config::airports::Airport;
use alas_units::NAUTICAL_MILE;

/// Mean Earth radius (IUGG), m, for the great-circle fallback below. Matches
/// `alas_route::EARTH_RADIUS_M`'s own convention; `alas-opt` cannot depend on
/// `alas-route`, so the one-line haversine is reproduced here rather than
/// shared.
const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// Great-circle distance between two points given by latitude and longitude,
/// in degrees, m.
fn haversine_m(lat1_deg: f64, lon1_deg: f64, lat2_deg: f64, lon2_deg: f64) -> f64 {
    let lat1_rad = lat1_deg.to_radians();
    let lat2_rad = lat2_deg.to_radians();
    let dlat_rad = (lat2_deg - lat1_deg).to_radians();
    let dlon_rad = (lon2_deg - lon1_deg).to_radians();
    let a = (dlat_rad / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (dlon_rad / 2.0).sin().powi(2);
    let central_angle_rad = 2.0 * a.sqrt().clamp(0.0, 1.0).asin();
    EARTH_RADIUS_M * central_angle_rad
}

/// Still-air mission range, m: the configured design range when it is
/// positive, or the great-circle distance between the configured aerodromes
/// when it is zero. Reserve segments never receive range credit, so this is
/// the design mission's trip distance alone.
pub(crate) fn mission_range_m(
    design_range_nmi: f64,
    departure: Option<&Airport>,
    arrival: Option<&Airport>,
) -> f64 {
    if design_range_nmi > 0.0 {
        return design_range_nmi * NAUTICAL_MILE;
    }
    match (departure, arrival) {
        (Some(from), Some(to)) => haversine_m(
            from.latitude_deg,
            from.longitude_deg,
            to.latitude_deg,
            to.longitude_deg,
        ),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configured_range_wins_over_the_great_circle_fallback() {
        let range_m = mission_range_m(1_000.0, None, None);
        assert!((range_m - 1_000.0 * NAUTICAL_MILE).abs() < 1e-6);
    }

    #[test]
    fn a_quarter_of_the_globe_is_a_quarter_of_its_circumference() {
        let north_pole_to_equator = haversine_m(90.0, 0.0, 0.0, 0.0);
        let expected = EARTH_RADIUS_M * std::f64::consts::FRAC_PI_2;
        assert!((north_pole_to_equator - expected).abs() < 1.0);
    }

    #[test]
    fn a_zero_range_with_no_resolved_aerodromes_falls_back_to_zero() {
        assert_eq!(mission_range_m(0.0, None, None), 0.0);
    }
}
