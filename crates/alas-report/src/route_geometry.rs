// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/route_globe.py
// Reference: alas @ rust-port-baseline.

//! Geographic and spherical route geometry for 3D globe and flight path projections.
//!
//! Provides coordinate conversion between ellipsoidal/spherical waypoint
//! coordinates and 3D Cartesian coordinates, and synchronizes flown mission
//! profiles (mass and altitude) onto airway and great-circle waypoint tracks.

use alas_math::interp;
use alas_mission::solve::MissionResult;
use alas_route::route::Route;

/// Mean Earth radius in kilometers used for spherical geometry projections.
pub const EARTH_RADIUS_KM: f64 = 6371.0;

/// Altitude offset above the globe sphere to prevent z-fighting when drawing paths.
pub const PATH_VISIBILITY_OFFSET_KM: f64 = 30.0;

/// Synchronize mission mass and altitude profiles onto lateral route waypoints.
///
/// Flown distance is computed via trapezoidal integration of true airspeed
/// over elapsed flight time, normalized against total route length, and sampled
/// onto each waypoint's cumulative distance.
pub fn sync_mass_to_route_series(
    route: &Route,
    time_s: &[f64],
    tas_m_s: &[f64],
    mass_kg: &[f64],
    altitude_m: &[f64],
) -> (Vec<f64>, Vec<f64>) {
    let n_wp = route.waypoints.len();
    if n_wp == 0 {
        return (Vec::new(), Vec::new());
    }

    let dist_route = route.cumulative_distance_m();
    let total_route_m = dist_route.last().copied().unwrap_or(0.0);

    let n_samples = time_s.len();
    if n_samples < 2 || total_route_m <= 0.0 {
        let fallback_mass = mass_kg.last().copied().unwrap_or(0.0);
        let fallback_alt = altitude_m.last().copied().unwrap_or(0.0);
        return (vec![fallback_mass; n_wp], vec![fallback_alt; n_wp]);
    }

    // Cumulative trapezoidal integration of true airspeed over time.
    let mut dist_csv = Vec::with_capacity(n_samples);
    dist_csv.push(0.0);
    let mut cumsum = 0.0;
    for i in 1..n_samples {
        let dt = time_s[i] - time_s[i - 1];
        let v_avg = 0.5 * (tas_m_s[i - 1] + tas_m_s[i]);
        cumsum += dt * v_avg;
        dist_csv.push(cumsum);
    }

    let final_dist = dist_csv.last().copied().unwrap_or(0.0);
    if final_dist <= 0.0 {
        for (i, d) in dist_csv.iter_mut().enumerate() {
            *d = (i as f64) / ((n_samples - 1) as f64) * total_route_m;
        }
    } else {
        let scale = total_route_m / final_dist;
        for d in &mut dist_csv {
            *d *= scale;
        }
    }

    // Keep only strictly increasing distance samples to satisfy interpolation prerequisites.
    let mut dist_unique = Vec::with_capacity(n_samples);
    let mut mass_unique = Vec::with_capacity(n_samples);
    let mut alt_unique = Vec::with_capacity(n_samples);

    dist_unique.push(dist_csv[0]);
    mass_unique.push(mass_kg[0]);
    alt_unique.push(altitude_m[0]);

    for i in 1..n_samples {
        if dist_csv[i] > dist_csv[i - 1] {
            dist_unique.push(dist_csv[i]);
            mass_unique.push(mass_kg[i]);
            alt_unique.push(altitude_m[i]);
        }
    }

    let mut mass_at_route = Vec::with_capacity(n_wp);
    let mut alt_at_route = Vec::with_capacity(n_wp);

    for &d in &dist_route {
        mass_at_route.push(interp(d, &dist_unique, &mass_unique));
        alt_at_route.push(interp(d, &dist_unique, &alt_unique));
    }

    (mass_at_route, alt_at_route)
}

/// Extract flat flight trajectory series from a completed [`MissionResult`].
pub fn mission_trajectory_series(
    mission: &MissionResult,
) -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut time_s = Vec::new();
    let mut tas_m_s = Vec::new();
    let mut mass_kg = Vec::new();
    let mut alt_m = Vec::new();

    for seg in &mission.segments {
        let cond = &seg.conditions;
        let n = cond.time_s.len();
        for i in 0..n {
            time_s.push(cond.time_s[i]);
            let tas = cond.velocity_m_s.get(i).copied().unwrap_or(0.0);
            tas_m_s.push(tas);
            let mass = cond.total_mass_kg.get(i).copied().unwrap_or(0.0);
            mass_kg.push(mass);
            let alt = cond.altitude_m.get(i).copied().unwrap_or(0.0);
            alt_m.push(alt);
        }
    }

    (time_s, tas_m_s, mass_kg, alt_m)
}

/// Synchronize a solved [`MissionResult`] onto a [`Route`].
pub fn sync_mass_to_route(route: &Route, mission: &MissionResult) -> (Vec<f64>, Vec<f64>) {
    if mission.fuel_exhaustion.is_some() {
        return (Vec::new(), Vec::new());
    }
    let (time_s, tas_m_s, mass_kg, alt_m) = mission_trajectory_series(mission);
    sync_mass_to_route_series(route, &time_s, &tas_m_s, &mass_kg, &alt_m)
}

/// Convert spherical route waypoints and altitude profile into 3D Cartesian coordinates.
///
/// Returns an array of `[x, y, z]` coordinates in kilometers with Earth radius and
/// radial altitude offset applied.
pub fn route_to_xyz(route: &Route, altitude_m: &[f64]) -> Vec<[f64; 3]> {
    let mut coords = Vec::with_capacity(route.waypoints.len());
    for (i, wp) in route.waypoints.iter().enumerate() {
        let lat_rad = wp.lat.to_radians();
        let lon_rad = wp.lon.to_radians();
        let alt = altitude_m.get(i).copied().unwrap_or(0.0);
        let r = EARTH_RADIUS_KM + (alt / 1000.0) + PATH_VISIBILITY_OFFSET_KM;

        let x = r * lat_rad.cos() * lon_rad.cos();
        let y = r * lat_rad.cos() * lon_rad.sin();
        let z = r * lat_rad.sin();
        coords.push([x, y, z]);
    }
    coords
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_mission::FuelExhaustion;
    use alas_route::route::{RouteSource, Waypoint};

    #[test]
    fn route_to_xyz_equator_prime_meridian() {
        let route = Route::new(
            vec![
                Waypoint::named(0.0, 0.0, "GWH"),
                Waypoint::named(90.0, 0.0, "NPOLE"),
            ],
            RouteSource::GreatCircle,
        );
        let alts = vec![0.0, 10_000.0];
        let xyz = route_to_xyz(&route, &alts);

        assert_eq!(xyz.len(), 2);
        let r0 = EARTH_RADIUS_KM + PATH_VISIBILITY_OFFSET_KM;
        assert!((xyz[0][0] - r0).abs() < 1e-6);
        assert!(xyz[0][1].abs() < 1e-6);
        assert!(xyz[0][2].abs() < 1e-6);

        let r1 = EARTH_RADIUS_KM + 10.0 + PATH_VISIBILITY_OFFSET_KM;
        assert!(xyz[1][0].abs() < 1e-6);
        assert!(xyz[1][1].abs() < 1e-6);
        assert!((xyz[1][2] - r1).abs() < 1e-6);
    }

    #[test]
    fn an_incomplete_fuel_exhausted_mission_is_not_stretched_to_the_destination() {
        let route = Route::new(
            vec![
                Waypoint::named(0.0, 0.0, "START"),
                Waypoint::named(0.0, 90.0, "END"),
            ],
            RouteSource::GreatCircle,
        );
        let mission = MissionResult {
            segments: Vec::new(),
            solutions: Vec::new(),
            scheduled_segment_count: 1,
            fuel_exhaustion: Some(FuelExhaustion {
                segment_index: 0,
                segment_tag: "cruise".to_owned(),
                available_fuel_kg: 100.0,
                burned_fuel_kg: 101.0,
                minimum_mass_kg: 1_000.0,
            }),
        };
        assert_eq!(
            sync_mass_to_route(&route, &mission),
            (Vec::new(), Vec::new())
        );
    }
}
