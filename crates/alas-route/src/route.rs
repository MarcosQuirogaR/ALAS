// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/routing/route.py
// Reference: alas @ rust-port-baseline.

//! What a route is: an ordered list of lateral positions between two airports,
//! and the distance flown along it.
//!
//! Altitude is left on the ground for a generated route. The route's job is the
//! *lateral* path -- a great circle, real jet airways, or an imported dispatch
//! plan -- and the vertical profile is the mission's, blended in later against
//! a simulated altitude-against-distance curve rather than guessed at here as a
//! climb and descent ramp.
//!
//! # Why the great circle is interpolated rather than drawn as two points
//!
//! A great circle between two airports is a straight line in three dimensions
//! and a curve on every map projection and on the globe view. Two endpoints
//! would be drawn as the straight line on whichever surface the renderer uses,
//! which for a long route is hundreds of kilometres from the path actually
//! flown, so the arc is sampled into waypoints by spherical interpolation.

use alas_config::airports::Airport;

/// The spherical Earth radius every distance here is measured on.
///
/// A sphere rather than the WGS-84 ellipsoid: the difference is a few tenths of
/// a percent, which is far inside the accuracy of a conceptual-design range
/// estimate, and the enroute fixes this routes between are themselves given to
/// a hundredth of a degree.
pub const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// One point along a route.
#[derive(Debug, Clone, PartialEq)]
pub struct Waypoint {
    /// Latitude, positive north.
    pub lat: f64,
    /// Longitude, positive east.
    pub lon: f64,
    /// Altitude above mean sea level. Zero on a generated route.
    pub alt_m: f64,
    /// The fix or airport identifier, where one is known.
    pub ident: String,
}

impl Waypoint {
    /// A waypoint on the ground with an identifier.
    pub fn named(lat: f64, lon: f64, ident: impl Into<String>) -> Self {
        Self {
            lat,
            lon,
            alt_m: 0.0,
            ident: ident.into(),
        }
    }

    /// A waypoint on the ground with nothing to call it, which is what the
    /// interpolated points of a great circle are.
    pub fn anonymous(lat: f64, lon: f64) -> Self {
        Self {
            lat,
            lon,
            alt_m: 0.0,
            ident: String::new(),
        }
    }
}

/// Where a route came from, which is also how good it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteSource {
    /// The shortest path over a sphere, which no aircraft is cleared to fly.
    GreatCircle,
    /// Real jet airways from the open enroute navigation data.
    NavdataGraph,
    /// A dispatch plan exported by hand as KML.
    SimbriefKml,
    /// A dispatch plan fetched from the operator's own SimBrief account.
    SimbriefApi,
}

impl RouteSource {
    /// The name upstream writes into `Route.source`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GreatCircle => "great_circle",
            Self::NavdataGraph => "navdata_graph",
            Self::SimbriefKml => "simbrief_kml",
            Self::SimbriefApi => "simbrief_api",
        }
    }
}

/// A lateral flight path between two airports.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// The path, in the order it is flown.
    pub waypoints: Vec<Waypoint>,
    /// How it was produced.
    pub source: RouteSource,
    /// The route's effective departure aerodrome, where it is not the one the
    /// caller asked for.
    ///
    /// Normally `None`, meaning "unchanged from what was passed in". A fetched
    /// dispatch plan is allowed to override the pair, because a real dispatched
    /// plan outranks a manually-selected one -- and the caller must then size
    /// the mission against the airports actually flown rather than the ones
    /// left selected on the page.
    pub origin_airport: Option<Airport>,
    /// The same for the arrival aerodrome.
    pub dest_airport: Option<Airport>,
}

impl Route {
    /// A route of these waypoints from this source, between the airports the
    /// caller already has.
    pub fn new(waypoints: Vec<Waypoint>, source: RouteSource) -> Self {
        Self {
            waypoints,
            source,
            origin_airport: None,
            dest_airport: None,
        }
    }

    /// Great-circle distance flown at each waypoint, starting at zero.
    ///
    /// Summed leg by leg rather than measured end to end, because the path is
    /// only a great circle between consecutive waypoints: an airway route is a
    /// chain of short arcs and is longer than the arc joining its ends.
    pub fn cumulative_distance_m(&self) -> Vec<f64> {
        let mut distances = Vec::with_capacity(self.waypoints.len());
        let mut total = 0.0;
        for (i, waypoint) in self.waypoints.iter().enumerate() {
            if i > 0 {
                let previous = &self.waypoints[i - 1];
                total += haversine_m(previous.lat, previous.lon, waypoint.lat, waypoint.lon);
            }
            distances.push(total);
        }
        distances
    }

    /// The whole path's length, and zero for a route with nothing in it.
    pub fn total_distance_m(&self) -> f64 {
        self.cumulative_distance_m().last().copied().unwrap_or(0.0)
    }

    /// Sample the great-circle arc between two airports into `n` intervals.
    ///
    /// Two coincident airports produce the two endpoints alone: the spherical
    /// interpolation divides by the sine of the arc length, and an arc of zero
    /// length has no direction to interpolate along.
    pub fn great_circle(origin: &Airport, dest: &Airport, n: usize) -> Self {
        let (lat1, lon1) = (
            origin.latitude_deg.to_radians(),
            origin.longitude_deg.to_radians(),
        );
        let (lat2, lon2) = (
            dest.latitude_deg.to_radians(),
            dest.longitude_deg.to_radians(),
        );
        let arc = angular_distance(lat1, lon1, lat2, lon2);

        let mut waypoints = vec![Waypoint::named(
            origin.latitude_deg,
            origin.longitude_deg,
            origin.icao.clone(),
        )];
        if arc > 1e-9 {
            for i in 1..n {
                let fraction = i as f64 / n as f64;
                let (lat, lon) = slerp(lat1, lon1, lat2, lon2, arc, fraction);
                waypoints.push(Waypoint::anonymous(lat.to_degrees(), lon.to_degrees()));
            }
        }
        waypoints.push(Waypoint::named(
            dest.latitude_deg,
            dest.longitude_deg,
            dest.icao.clone(),
        ));
        Self::new(waypoints, RouteSource::GreatCircle)
    }
}

/// Great-circle distance between two points given in degrees.
pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    EARTH_RADIUS_M
        * angular_distance(
            lat1.to_radians(),
            lon1.to_radians(),
            lat2.to_radians(),
            lon2.to_radians(),
        )
}

/// The central angle between two points given in radians.
///
/// The half-angle form rather than the spherical law of cosines: the latter
/// loses most of its precision on the short legs an airway route is made of,
/// where the cosine of the angle is within an ulp or two of one.
pub fn angular_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (dlat, dlon) = (lat2 - lat1, lon2 - lon1);
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    // Rounding can push `a` a hair above one on two antipodal points, and the
    // arcsine of that is not a number.
    2.0 * a.sqrt().min(1.0).asin()
}

/// Spherical interpolation at `fraction` along an arc of angular length `arc`.
fn slerp(lat1: f64, lon1: f64, lat2: f64, lon2: f64, arc: f64, fraction: f64) -> (f64, f64) {
    let a = ((1.0 - fraction) * arc).sin() / arc.sin();
    let b = (fraction * arc).sin() / arc.sin();
    let x = a * lat1.cos() * lon1.cos() + b * lat2.cos() * lon2.cos();
    let y = a * lat1.cos() * lon1.sin() + b * lat2.cos() * lon2.sin();
    let z = a * lat1.sin() + b * lat2.sin();
    (z.atan2((x * x + y * y).sqrt()), y.atan2(x))
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn airport(icao: &str, lat: f64, lon: f64) -> Airport {
        Airport {
            name: icao.to_owned(),
            icao: icao.to_owned(),
            elevation_m: 0.0,
            toda_m: 3000.0,
            lda_m: 3000.0,
            isa_deviation_c: 0.0,
            notes: String::new(),
            latitude_deg: lat,
            longitude_deg: lon,
        }
    }

    #[test]
    fn a_quarter_of_the_way_round_the_equator_is_a_quarter_of_the_circumference() {
        let quarter = haversine_m(0.0, 0.0, 0.0, 90.0);
        assert!((quarter - EARTH_RADIUS_M * std::f64::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn antipodal_points_are_half_the_circumference_apart_rather_than_undefined() {
        // The rounding guard in `angular_distance` is what stops this being a
        // NaN, and a NaN distance would fail a range check for the wrong reason.
        let across = haversine_m(0.0, 0.0, 0.0, 180.0);
        assert!(across.is_finite());
        assert!((across - EARTH_RADIUS_M * std::f64::consts::PI).abs() < 1.0);
    }

    #[test]
    fn a_great_circle_is_sampled_into_the_intervals_it_was_asked_for() {
        let route = Route::great_circle(
            &airport("LEMD", 40.5, -3.6),
            &airport("KJFK", 40.6, -73.8),
            50,
        );
        assert_eq!(route.waypoints.len(), 51);
        assert_eq!(route.waypoints[0].ident, "LEMD");
        assert_eq!(route.waypoints[50].ident, "KJFK");
        // Only the endpoints are named: an interpolated point is not a fix.
        assert!(route.waypoints[25].ident.is_empty());
    }

    #[test]
    fn a_route_to_the_same_airport_is_two_points_and_no_interpolation() {
        // The spherical interpolation divides by the sine of the arc length.
        let here = airport("LEMD", 40.5, -3.6);
        let route = Route::great_circle(&here, &here, 50);
        assert_eq!(route.waypoints.len(), 2);
        assert_eq!(route.total_distance_m(), 0.0);
    }

    #[test]
    fn the_sampled_arc_is_the_same_length_as_the_arc_it_samples() {
        // Spherical interpolation puts every sample *on* the great circle, so
        // summing the legs must not lengthen it the way chords across a curve
        // would.
        let madrid = airport("LEMD", 40.4719, -3.5626);
        let tokyo = airport("RJTT", 35.5533, 139.7811);
        let direct = haversine_m(
            madrid.latitude_deg,
            madrid.longitude_deg,
            tokyo.latitude_deg,
            tokyo.longitude_deg,
        );
        let sampled = Route::great_circle(&madrid, &tokyo, 50).total_distance_m();
        assert!(
            (sampled - direct).abs() / direct < 1e-9,
            "sampling the arc changed its length by {} m",
            sampled - direct
        );
    }

    #[test]
    fn the_distance_at_each_waypoint_grows_along_the_path() {
        let route = Route::great_circle(&airport("A", 0.0, 0.0), &airport("B", 0.0, 30.0), 10);
        let distances = route.cumulative_distance_m();
        assert_eq!(distances.len(), route.waypoints.len());
        assert_eq!(distances[0], 0.0);
        assert!(distances.windows(2).all(|pair| pair[1] > pair[0]));
    }

    #[test]
    fn an_empty_route_has_no_length_rather_than_no_answer() {
        let empty = Route::new(Vec::new(), RouteSource::GreatCircle);
        assert!(empty.cumulative_distance_m().is_empty());
        assert_eq!(empty.total_distance_m(), 0.0);
    }
}
