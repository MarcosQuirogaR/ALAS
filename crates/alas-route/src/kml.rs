// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/routing/kml_import.py
// Reference: alas @ rust-port-baseline.

//! Importing a dispatch route exported by hand as KML.
//!
//! A flight planner's "download as KML" export packs the whole route:
//! departure procedure, airway waypoints, arrival procedure: into a single
//! `<coordinates>` block of `lon,lat,alt` triples. That is a real dispatched
//! lateral path including the terminal procedures this program's own airway
//! routing does not model, and it needs no account or API key to produce: the
//! free web planner exports it.
//!
//! Only the coordinate block is read. A KML document is a large XML schema and
//! none of the rest of it says anything about where the aircraft goes, so
//! reading it would mean an XML parser in the dependency tree for data that is
//! discarded.

use crate::route::{Route, RouteSource, Waypoint};

/// The element the whole path is packed into.
const OPEN_TAG: &str = "<coordinates>";
/// Its closing tag.
const CLOSE_TAG: &str = "</coordinates>";

/// Fewer waypoints than this is not a path between two places.
const MIN_WAYPOINTS: usize = 2;

/// A KML document this importer could not read a route out of.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KmlError {
    /// No coordinate block, so this is not a route export.
    #[error("no <coordinates> block was found, so this is not a route export")]
    NoCoordinates,
    /// A coordinate block with nothing usable in it.
    #[error("the <coordinates> block produced {found} waypoints, which is not a path")]
    TooFewWaypoints {
        /// How many were read.
        found: usize,
    },
}

/// Parse a KML export into a route.
///
/// # Errors
///
/// [`KmlError`], for a document with no coordinate block or one carrying fewer
/// than two usable points. Upstream's caller treats either as a reason to fall
/// through to the next routing tier rather than as a failure of the run.
pub fn route_from_kml(document: &str) -> Result<Route, KmlError> {
    let start = document.find(OPEN_TAG).ok_or(KmlError::NoCoordinates)?;
    let body = &document[start + OPEN_TAG.len()..];
    let end = body.find(CLOSE_TAG).ok_or(KmlError::NoCoordinates)?;

    let mut waypoints = Vec::new();
    for token in body[..end].split_whitespace() {
        // A trailing comma or a lone altitude is a malformed triple, and the
        // exporter does emit them; skipping is what keeps one bad token from
        // discarding an otherwise complete route.
        let fields: Vec<&str> = token.split(',').collect();
        if fields.len() < 2 {
            continue;
        }
        let (Ok(lon), Ok(lat)) = (fields[0].parse::<f64>(), fields[1].parse::<f64>()) else {
            continue;
        };
        let alt_m = fields
            .get(2)
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0);
        waypoints.push(Waypoint {
            lat,
            lon,
            alt_m,
            ident: String::new(),
        });
    }

    if waypoints.len() < MIN_WAYPOINTS {
        return Err(KmlError::TooFewWaypoints {
            found: waypoints.len(),
        });
    }
    Ok(Route::new(waypoints, RouteSource::SimbriefKml))
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_coordinate_block_becomes_a_route_in_the_order_it_was_written() {
        let kml = "<Placemark><LineString><coordinates>\n\
            -3.56,40.47,610 -2.00,41.00,10000 2.20,43.20,300\n\
            </coordinates></LineString></Placemark>";
        let route = route_from_kml(kml).expect("three points are a path");
        assert_eq!(route.waypoints.len(), 3);
        // KML writes longitude first, which is the opposite of every other
        // coordinate pair in this program.
        assert_eq!(route.waypoints[0].lat, 40.47);
        assert_eq!(route.waypoints[0].lon, -3.56);
        assert_eq!(route.waypoints[1].alt_m, 10000.0);
        assert_eq!(route.source, RouteSource::SimbriefKml);
    }

    #[test]
    fn a_point_with_no_altitude_sits_on_the_ground() {
        let route =
            route_from_kml("<coordinates>1,2 3,4</coordinates>").expect("two points are a path");
        assert_eq!(route.waypoints[0].alt_m, 0.0);
    }

    #[test]
    fn a_document_with_no_coordinates_is_an_error_and_not_an_empty_route() {
        // An empty route would be flown as a zero-distance mission rather than
        // reported as a bad import.
        assert_eq!(
            route_from_kml("<kml><Document/></kml>"),
            Err(KmlError::NoCoordinates)
        );
        assert_eq!(
            route_from_kml("<coordinates>1,2 3,4"),
            Err(KmlError::NoCoordinates)
        );
    }

    #[test]
    fn a_single_point_is_not_a_path() {
        assert_eq!(
            route_from_kml("<coordinates>1,2</coordinates>"),
            Err(KmlError::TooFewWaypoints { found: 1 })
        );
    }

    #[test]
    fn a_malformed_token_is_skipped_rather_than_discarding_the_route() {
        let route = route_from_kml("<coordinates>1,2,3 bogus 4,5,6 7,8,9</coordinates>")
            .expect("the three good points are a path");
        assert_eq!(route.waypoints.len(), 3);
    }
}
