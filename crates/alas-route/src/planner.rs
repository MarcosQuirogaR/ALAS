// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/routing/route.py (`Route.for_airports`)
// Reference: alas @ rust-port-baseline.

//! Choosing the best route available between two airports.
//!
//! Four tiers, best first: a real dispatched flight plan, a dispatch route
//! exported by hand as KML, this program's own airway routing over the open
//! navigation data, and a great circle. Each is a strictly better description
//! of what the aircraft would actually fly than the one below it, and each is
//! optional: the last is not, which is what makes this total.
//!
//! A tier that fails is skipped rather than raised. None of them is required
//! for a design to be evaluated: a mission over a great circle is a slightly
//! optimistic mission, not a broken one, and refusing to size an aircraft
//! because a flight-planning website was unreachable would be the wrong
//! trade every time.
//!
//! # Where the top tier's fetching lives
//!
//! Upstream calls the dispatch service from inside this function. Here the
//! caller passes in whatever it fetched, for the reason `crate::simbrief`
//! gives: an HTTP client in this crate would be an HTTP client in everything
//! that computes a distance. `alas-app` performs the request and hands the
//! result down. Everything else about the tiering: the order, what counts as
//! a failure, and what each tier falls through to, is reproduced here.

use std::path::Path;

use alas_config::airports::Airport;

use crate::kml::route_from_kml;
use crate::navdata::{navdata_available, NavdataGraph};
use crate::route::Route;

/// How many points a great circle is sampled into when nothing better is
/// available.
pub const DEFAULT_GREAT_CIRCLE_POINTS: usize = 50;

/// What the caller has already obtained for the tiers this crate cannot
/// perform itself.
#[derive(Debug, Default)]
pub struct RouteSources<'a> {
    /// A route read from a fetched dispatch plan, if one was fetched and
    /// parsed. See [`crate::simbrief::route_from_ofp`].
    pub dispatched: Option<Route>,
    /// Where hand-exported KML routes are kept, if anywhere.
    pub routes_dir: Option<&'a Path>,
    /// The parsed airway network, if the navigation data has been downloaded.
    pub navdata: Option<&'a NavdataGraph>,
    /// How many points to sample a great circle into.
    pub great_circle_points: usize,
}

impl RouteSources<'_> {
    /// No optional source at all, sampling a great circle at the default
    /// resolution, which is what an analysis run with nothing configured
    /// gets.
    pub fn none() -> Self {
        Self {
            dispatched: None,
            routes_dir: None,
            navdata: None,
            great_circle_points: DEFAULT_GREAT_CIRCLE_POINTS,
        }
    }
}

/// The name a hand-exported route between two airports is filed under.
pub fn kml_file_name(origin: &Airport, dest: &Airport) -> String {
    format!("{}_{}.kml", origin.icao, dest.icao)
}

/// The best route available between two airports.
///
/// Never fails: the great circle is always available, so every earlier tier is
/// an improvement that may or may not be there.
/// This entry point retains unfiltered parity behavior; the mission pipeline
/// uses [`plan_route_with_max_stretch`] to reject excessive graph detours.
pub fn plan_route(origin: &Airport, dest: &Airport, sources: RouteSources<'_>) -> Route {
    plan_route_with_max_stretch(origin, dest, sources, 0.0)
}

/// Select a route while rejecting excessive detours from the approximate airway graph.
///
/// A positive limit compares airway distance with airport-to-airport great-circle
/// distance. Zero disables the check for parity. Imported and dispatched routes
/// are authoritative inputs and are not filtered. Rejection returns an explicitly
/// labeled great-circle approximation, not an operational flight clearance.
pub fn plan_route_with_max_stretch(
    origin: &Airport,
    dest: &Airport,
    sources: RouteSources<'_>,
    max_airway_stretch: f64,
) -> Route {
    if let Some(dispatched) = sources.dispatched {
        tracing::info!(
            origin = %origin.icao,
            dest = %dest.icao,
            waypoints = dispatched.waypoints.len(),
            "using the dispatched flight plan"
        );
        return dispatched;
    }

    if let Some(routes_dir) = sources.routes_dir {
        let path = routes_dir.join(kml_file_name(origin, dest));
        if path.exists() {
            match std::fs::read_to_string(&path)
                .map_err(|error| error.to_string())
                .and_then(|document| route_from_kml(&document).map_err(|error| error.to_string()))
            {
                Ok(route) => return route,
                // An export that will not parse is not a reason to stop: the
                // tier below produces a usable route from data this program
                // already has.
                Err(error) => tracing::warn!(
                    path = %path.display(),
                    %error,
                    "the exported route could not be read; using the next route tier"
                ),
            }
        }
    }

    if let Some(navdata) = sources.navdata {
        if let Some(route) = navdata.airway_route(origin, dest) {
            let direct = crate::route::haversine_m(
                origin.latitude_deg,
                origin.longitude_deg,
                dest.latitude_deg,
                dest.longitude_deg,
            );
            let distance = route.total_distance_m();
            let limit_is_ratio = max_airway_stretch.is_finite() && max_airway_stretch >= 1.0;
            if max_airway_stretch == 0.0
                || (limit_is_ratio && distance <= direct * max_airway_stretch)
            {
                return route;
            }
            if limit_is_ratio {
                tracing::warn!(
                    airway_distance_m = distance,
                    great_circle_distance_m = direct,
                    max_airway_stretch,
                    "airway graph exceeds the configured detour limit; using a great-circle approximation"
                );
            } else {
                // A limit below one rejects every airway route, a perfectly
                // direct one included, so the route is not what is at fault.
                tracing::warn!(
                    max_airway_stretch,
                    "the airway stretch limit is neither 0 nor a finite ratio of at least 1; \
                     using a great-circle approximation"
                );
            }
        }
    }

    Route::great_circle(origin, dest, sources.great_circle_points)
}

/// Load the airway network from a directory, if it holds one.
///
/// Returns `None` both when the data has not been downloaded and when it
/// cannot be read, which are the same thing to a caller: the airway tier is
/// unavailable and the one below it applies.
pub fn load_navdata(navdata_dir: &Path) -> Option<NavdataGraph> {
    load_navdata_with_airway_coordinates(navdata_dir, false)
}

/// Load coordinate-bearing airway records when enabled; false retains parity.
pub fn load_navdata_with_airway_coordinates(
    navdata_dir: &Path,
    use_coordinates: bool,
) -> Option<NavdataGraph> {
    if !navdata_available(navdata_dir) {
        return None;
    }
    let result = if use_coordinates {
        std::fs::read_to_string(navdata_dir.join(crate::navdata::FIX_FILE))
            .and_then(|fixes| {
                std::fs::read_to_string(navdata_dir.join(crate::navdata::AIRWAY_FILE))
                    .map(|airways| NavdataGraph::parse_with_airway_coordinates(&fixes, &airways))
            })
            .map_err(|source| crate::navdata::NavdataError {
                path: navdata_dir.display().to_string(),
                source,
            })
    } else {
        NavdataGraph::load(navdata_dir)
    };
    match result {
        Ok(graph) => Some(graph),
        Err(error) => {
            tracing::warn!(%error, "the navigation data could not be read");
            None
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::RouteSource;

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
    fn with_nothing_configured_the_route_is_a_great_circle() {
        let route = plan_route(
            &airport("LEMD", 40.47, -3.56),
            &airport("EGKK", 51.15, -0.19),
            RouteSources::none(),
        );
        assert_eq!(route.source, RouteSource::GreatCircle);
        assert_eq!(route.waypoints.len(), DEFAULT_GREAT_CIRCLE_POINTS + 1);
    }

    #[test]
    fn a_dispatched_plan_outranks_every_other_tier() {
        let graph = NavdataGraph::parse(
            " 40.000000  -4.000000 ALPHA\n 51.000000  -0.100000 BRAVO\n",
            "ALPHA ES 11 BRAVO ES 11 N 1 100 400 UN10\n",
        );
        let dispatched = Route::new(Vec::new(), RouteSource::SimbriefApi);
        let route = plan_route(
            &airport("LEMD", 40.47, -3.56),
            &airport("EGKK", 51.15, -0.19),
            RouteSources {
                dispatched: Some(dispatched),
                navdata: Some(&graph),
                ..RouteSources::none()
            },
        );
        assert_eq!(route.source, RouteSource::SimbriefApi);
    }

    #[test]
    fn the_airway_tier_is_used_where_the_network_reaches_both_airports() {
        let graph = NavdataGraph::parse(
            " 40.000000  -4.000000 ALPHA\n 51.000000  -0.100000 BRAVO\n",
            "ALPHA ES 11 BRAVO ES 11 N 1 100 400 UN10\n",
        );
        let route = plan_route(
            &airport("LEMD", 40.47, -3.56),
            &airport("EGKK", 51.15, -0.19),
            RouteSources {
                navdata: Some(&graph),
                ..RouteSources::none()
            },
        );
        assert_eq!(route.source, RouteSource::NavdataGraph);
    }

    #[test]
    fn a_network_that_reaches_neither_airport_falls_through_to_the_great_circle() {
        // The airway tier failing is not an error; it is the reason the tier
        // below it exists.
        let graph = NavdataGraph::parse(
            "-40.000000 150.000000 ALPHA\n-41.000000 151.000000 BRAVO\n",
            "ALPHA ES 11 BRAVO ES 11 N 1 100 400 UN10\n",
        );
        let route = plan_route(
            &airport("LEMD", 40.47, -3.56),
            &airport("EGKK", 51.15, -0.19),
            RouteSources {
                navdata: Some(&graph),
                ..RouteSources::none()
            },
        );
        assert_eq!(route.source, RouteSource::GreatCircle);
    }

    #[test]
    fn an_exported_route_is_filed_under_both_airport_codes() {
        assert_eq!(
            kml_file_name(&airport("LEMD", 0.0, 0.0), &airport("EGKK", 0.0, 0.0)),
            "LEMD_EGKK.kml"
        );
    }

    #[test]
    fn a_missing_navdata_directory_leaves_the_airway_tier_unavailable() {
        assert!(load_navdata(Path::new("no/such/directory")).is_none());
    }
}
