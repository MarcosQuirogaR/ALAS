// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/routing/navdata_graph.py
// Reference: alas @ rust-port-baseline.

//! Enroute routing along real jet airways, from the open navigation data.
//!
//! The X-Plane-format enroute files (`earth_fix.dat` and `earth_awy.dat`) are
//! parsed into a graph of waypoints joined by airway segments, and the route is
//! the shortest path through it between the fixes nearest the two airports. The
//! result is a path an aircraft could actually be cleared to fly, rather than a
//! straight line no controller would accept.
//!
//! # Duplicate identifiers, and why they are not collapsed
//!
//! Five-letter fix identifiers are unique only regionally. This dataset's
//! three-column legacy fix format carries no region code, so a few percent of
//! identifiers in a global set name two or more unrelated physical fixes: a
//! "MITSO" near the United Kingdom and a completely different "MITSO" near
//! Riyadh. The airway file references its endpoints by identifier alone, so
//! keeping only the first occurrence of each name can wire a short local
//! segment to the wrong fix thousands of kilometres away, producing a route
//! that looks airway-derived and contains one physically impossible hop.
//!
//! Every occurrence is therefore kept, and each segment is disambiguated by
//! choosing the candidate pair with the smallest great-circle distance: a real
//! airway leg is tens to a few hundred kilometres, so the nearest candidate is
//! always the right one.
//!
//! # Known simplification
//!
//! Terminal procedures are not modeled: that needs full ARINC 424 parsing,
//! which for non-US data is licence-encumbered. The transition between an
//! airport and its nearest enroute fix is a straight line.
//!
//! # The data is not bundled
//!
//! The navigation data is published under the GPL by the X-Plane project.
//! Shipping it would impose that licence on anyone redistributing this program,
//! so it is neither committed nor built into the executable, and until it has
//! been fetched this module reports no route and the caller falls back to a
//! great circle.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap, HashMap};
use std::path::Path;

use alas_config::airports::Airport;

mod coordinates;

use crate::route::{angular_distance, haversine_m, Route, RouteSource, Waypoint, EARTH_RADIUS_M};

/// The fix file's name within a navigation-data directory.
pub const FIX_FILE: &str = "earth_fix.dat";
/// The airway file's name within the same directory.
pub const AIRWAY_FILE: &str = "earth_awy.dat";

/// How far an airport may be from the fix its route joins the airways at.
///
/// About 215 nautical miles. Beyond that the straight-line transition stands in
/// for so much of the flight that the airway path is no longer describing it,
/// and a great circle is the more honest answer.
const MAX_TRANSITION_M: f64 = 400_000.0;

/// Columns a fix line must have before it is worth parsing: latitude,
/// longitude and identifier.
const FIX_COLUMNS: usize = 3;
/// Columns an airway line must have. Fewer means a header, a version marker or
/// the file's end.
const AIRWAY_COLUMNS: usize = 10;

/// One enroute waypoint.
#[derive(Debug, Clone, PartialEq)]
pub struct Fix {
    /// Its identifier, which is unique only regionally.
    pub ident: String,
    /// Latitude, positive north.
    pub lat: f64,
    /// Longitude, positive east.
    pub lon: f64,
}

/// Whether a directory holds both files this module needs.
pub fn navdata_available(navdata_dir: &Path) -> bool {
    [FIX_FILE, AIRWAY_FILE].iter().all(|name| {
        crate::assets::NAVDATA_FILES
            .iter()
            .find(|file| file.name == *name)
            .is_some_and(|file| crate::assets::navdata_file_is_usable(navdata_dir, file))
    })
}

/// The parsed waypoint and airway network.
///
/// Upstream keeps a one-entry cache keyed by the directory and both files'
/// modification times, because every mission run would otherwise re-parse tens
/// of thousands of lines. Here the parsed graph is a value the caller holds, so
/// re-parsing is something they have to ask for rather than something they have
/// to be saved from.
#[derive(Debug, Clone, Default)]
pub struct NavdataGraph {
    /// Every parsed fix, in file order. Graph nodes are indices into this,
    /// rather than identifiers, so that two fixes sharing a name stay distinct.
    pub fixes: Vec<Fix>,
    /// Each connected fix's neighbours and the leg distance to them.
    edges: BTreeMap<usize, Vec<(usize, f64)>>,
    /// The fixes that appear in at least one airway, ascending.
    connected: Vec<usize>,
    /// Those fixes' coordinates in radians, in the same order.
    coords_rad: Vec<(f64, f64)>,
}

/// A navigation-data directory that could not be read.
#[derive(Debug, thiserror::Error)]
#[error("could not read the navigation data at {path}")]
pub struct NavdataError {
    /// The file that could not be read.
    pub path: String,
    /// Why not.
    #[source]
    pub source: std::io::Error,
}

impl NavdataGraph {
    /// Read and parse a navigation-data directory.
    ///
    /// # Errors
    ///
    /// [`NavdataError`], when either file is missing or unreadable. Callers
    /// that mean to fall back to another routing tier should check
    /// [`navdata_available`] first, which is what upstream's caller does.
    pub fn load(navdata_dir: &Path) -> Result<Self, NavdataError> {
        let read = |name: &str| {
            let path = navdata_dir.join(name);
            std::fs::read_to_string(&path).map_err(|source| NavdataError {
                path: path.display().to_string(),
                source,
            })
        };
        Ok(Self::parse(&read(FIX_FILE)?, &read(AIRWAY_FILE)?))
    }

    /// Parse the two files' contents into a graph.
    pub fn parse(fix_data: &str, airway_data: &str) -> Self {
        let (fixes, by_ident) = parse_fixes(fix_data);
        let edges = parse_airways(airway_data, &fixes, &by_ident);
        let connected: Vec<usize> = edges.keys().copied().collect();
        let coords_rad = connected
            .iter()
            .map(|&i| (fixes[i].lat.to_radians(), fixes[i].lon.to_radians()))
            .collect();
        Self {
            fixes,
            edges,
            connected,
            coords_rad,
        }
    }

    /// Whether any airway segment was parsed at all.
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// One fix's neighbours and the leg distance to each, in the order the
    /// airway file joined them.
    ///
    /// Empty for a fix no airway touches, which is not the same as a fix that
    /// does not exist, both are unroutable, and neither is an error.
    pub fn neighbors(&self, fix: usize) -> &[(usize, f64)] {
        self.edges.get(&fix).map_or(&[], Vec::as_slice)
    }

    /// The fixes that appear in at least one airway, ascending.
    pub fn connected_fixes(&self) -> &[usize] {
        &self.connected
    }

    /// The nearest *connected* fix to a position, as an index into
    /// [`Self::fixes`], or `None` when the closest one is too far to be a
    /// plausible transition.
    ///
    /// Isolated fixes are excluded rather than merely deprioritised: snapping
    /// to one the graph cannot route from would produce no route at all, where
    /// a slightly more distant connected fix produces a good one.
    pub fn nearest_fix(&self, lat: f64, lon: f64) -> Option<usize> {
        let (lat_r, lon_r) = (lat.to_radians(), lon.to_radians());
        let mut best: Option<(f64, usize)> = None;
        for (row, &(fix_lat, fix_lon)) in self.coords_rad.iter().enumerate() {
            let distance = EARTH_RADIUS_M * angular_distance(lat_r, lon_r, fix_lat, fix_lon);
            if best.is_none_or(|(closest, _)| distance < closest) {
                best = Some((distance, row));
            }
        }
        let (distance, row) = best?;
        if distance > MAX_TRANSITION_M {
            return None;
        }
        self.connected.get(row).copied()
    }

    /// The shortest airway path between two fixes, as indices into
    /// [`Self::fixes`], including both ends.
    pub fn shortest_path(&self, start: usize, goal: usize) -> Option<Vec<usize>> {
        if !self.edges.contains_key(&start) || !self.edges.contains_key(&goal) {
            return None;
        }
        let mut distance: HashMap<usize, f64> = HashMap::from([(start, 0.0)]);
        let mut previous: HashMap<usize, usize> = HashMap::new();
        let mut visited: Vec<bool> = vec![false; self.fixes.len()];
        let mut frontier = BinaryHeap::from([Reachable {
            distance: 0.0,
            node: start,
        }]);

        while let Some(Reachable { distance: d, node }) = frontier.pop() {
            if visited.get(node).copied().unwrap_or(true) {
                continue;
            }
            visited[node] = true;
            if node == goal {
                break;
            }
            for &(neighbor, weight) in self.edges.get(&node).into_iter().flatten() {
                let through = d + weight;
                if through < distance.get(&neighbor).copied().unwrap_or(f64::INFINITY) {
                    distance.insert(neighbor, through);
                    previous.insert(neighbor, node);
                    frontier.push(Reachable {
                        distance: through,
                        node: neighbor,
                    });
                }
            }
        }
        if !distance.contains_key(&goal) {
            return None;
        }

        let mut path = vec![goal];
        while let Some(&last) = path.last() {
            if last == start {
                break;
            }
            path.push(*previous.get(&last)?);
        }
        path.reverse();
        Some(path)
    }

    /// A route following real jet airways, or `None` when either airport is
    /// too far from the network or no path joins the two.
    pub fn airway_route(&self, origin: &Airport, dest: &Airport) -> Option<Route> {
        let entry = self.nearest_fix(origin.latitude_deg, origin.longitude_deg)?;
        let exit = self.nearest_fix(dest.latitude_deg, dest.longitude_deg)?;
        let path = self.shortest_path(entry, exit)?;

        let mut waypoints = vec![Waypoint::named(
            origin.latitude_deg,
            origin.longitude_deg,
            origin.icao.clone(),
        )];
        waypoints.extend(path.into_iter().map(|i| {
            Waypoint::named(
                self.fixes[i].lat,
                self.fixes[i].lon,
                self.fixes[i].ident.clone(),
            )
        }));
        waypoints.push(Waypoint::named(
            dest.latitude_deg,
            dest.longitude_deg,
            dest.icao.clone(),
        ));
        Some(Route::new(waypoints, RouteSource::NavdataGraph))
    }
}

/// A node on the search frontier, ordered so the nearest comes off the heap
/// first.
///
/// Ties break on the node index, which is the order the reference's own binary
/// heap imposes on equal distances, so two paths of exactly equal length
/// resolve the same way in both implementations rather than depending on which
/// one the heap happened to hold.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Reachable {
    distance: f64,
    node: usize,
}

impl Eq for Reachable {}

impl Ord for Reachable {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then_with(|| other.node.cmp(&self.node))
    }
}

impl PartialOrd for Reachable {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Parse the fix file, keeping every occurrence of every identifier.
///
/// Returns the fixes in file order and an index from identifier to the
/// positions sharing it: almost always one, and the whole point of this
/// module's care when it is not.
fn parse_fixes(data: &str) -> (Vec<Fix>, HashMap<String, Vec<usize>>) {
    let mut fixes = Vec::new();
    let mut by_ident: HashMap<String, Vec<usize>> = HashMap::new();
    for line in data.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < FIX_COLUMNS {
            continue;
        }
        // A header, a version marker or the file's terminator, none of which
        // begins with two numbers.
        let (Ok(lat), Ok(lon)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) else {
            continue;
        };
        by_ident
            .entry(parts[2].to_owned())
            .or_default()
            .push(fixes.len());
        fixes.push(Fix {
            ident: parts[2].to_owned(),
            lat,
            lon,
        });
    }
    (fixes, by_ident)
}

/// Parse the airway file into an adjacency map keyed by fix index.
///
/// The direction column is read and not used: a one-way airway leg is a
/// restriction this fidelity does not model, so every segment is joined both
/// ways.
fn parse_airways(
    data: &str,
    fixes: &[Fix],
    by_ident: &HashMap<String, Vec<usize>>,
) -> BTreeMap<usize, Vec<(usize, f64)>> {
    let mut edges: BTreeMap<usize, Vec<(usize, f64)>> = BTreeMap::new();
    for line in data.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < AIRWAY_COLUMNS {
            continue;
        }
        let Some((a, b, leg)) = resolve_pair(parts[0], parts[3], fixes, by_ident) else {
            continue;
        };
        edges.entry(a).or_default().push((b, leg));
        edges.entry(b).or_default().push((a, leg));
    }
    edges
}

/// The pair of fixes an airway segment joins, and the length of the leg.
///
/// Where either identifier names more than one fix, the closest pair wins,
/// see the module doc for why the alternative wires segments across continents.
fn resolve_pair(
    ident_a: &str,
    ident_b: &str,
    fixes: &[Fix],
    by_ident: &HashMap<String, Vec<usize>>,
) -> Option<(usize, usize, f64)> {
    let candidates_a = by_ident.get(ident_a).filter(|list| !list.is_empty())?;
    let candidates_b = by_ident.get(ident_b).filter(|list| !list.is_empty())?;
    let leg =
        |a: usize, b: usize| haversine_m(fixes[a].lat, fixes[a].lon, fixes[b].lat, fixes[b].lon);
    if let ([a], [b]) = (candidates_a.as_slice(), candidates_b.as_slice()) {
        return Some((*a, *b, leg(*a, *b)));
    }
    let mut best: Option<(f64, usize, usize)> = None;
    for &a in candidates_a {
        for &b in candidates_b {
            let distance = leg(a, b);
            if best.is_none_or(|(shortest, _, _)| distance < shortest) {
                best = Some((distance, a, b));
            }
        }
    }
    best.map(|(distance, a, b)| (a, b, distance))
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    /// Two short chains joined at one fix, plus an isolated fix that no airway
    /// reaches.
    const FIXES: &str = "\
I
1100 Version

 40.000000  -4.000000 ALPHA
 41.000000  -2.000000 BRAVO
 42.000000   0.000000 CHRLI
 43.000000   2.000000 DELTA
 60.000000  30.000000 LONER
99
";
    const AIRWAYS: &str = "\
I
1100 Version

ALPHA ES 11 BRAVO ES 11 N 1 100 400 UN10
BRAVO ES 11 CHRLI LF 11 N 1 100 400 UN10
CHRLI LF 11 DELTA LF 11 N 1 100 400 UN10
99
";

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
    fn header_and_terminator_lines_are_skipped_rather_than_parsed() {
        // "I", "1100 Version" and "99" are not fixes, and a parser that took
        // them would put a waypoint at the equator.
        let (fixes, _) = parse_fixes(FIXES);
        assert_eq!(fixes.len(), 5);
        assert_eq!(fixes[0].ident, "ALPHA");
    }

    #[test]
    fn a_fix_no_airway_reaches_is_not_somewhere_a_route_can_start() {
        let graph = NavdataGraph::parse(FIXES, AIRWAYS);
        // LONER is in the fix file and in no airway, so the nearest-fix search
        // must not offer it even to an airport sitting on top of it.
        assert_eq!(graph.nearest_fix(60.0, 30.0), None);
    }

    #[test]
    fn an_airport_far_from_every_fix_gets_no_airway_route() {
        let graph = NavdataGraph::parse(FIXES, AIRWAYS);
        assert_eq!(graph.nearest_fix(-40.0, 150.0), None);
    }

    #[test]
    fn the_route_runs_through_the_airway_chain_end_to_end() {
        let graph = NavdataGraph::parse(FIXES, AIRWAYS);
        let route = graph
            .airway_route(&airport("LEXX", 39.8, -4.2), &airport("LFYY", 43.2, 2.2))
            .expect("both airports sit beside the chain");
        let idents: Vec<&str> = route
            .waypoints
            .iter()
            .map(|waypoint| waypoint.ident.as_str())
            .collect();
        assert_eq!(
            idents,
            vec!["LEXX", "ALPHA", "BRAVO", "CHRLI", "DELTA", "LFYY"]
        );
        assert_eq!(route.source, RouteSource::NavdataGraph);
    }

    #[test]
    fn a_duplicated_ident_resolves_to_the_occurrence_that_makes_a_short_leg() {
        // A second BRAVO on the far side of the world must not be what the
        // ALPHA-BRAVO segment joins to: that is the one physically impossible
        // hop this module exists to avoid.
        let fixes = format!("{FIXES}-35.000000 150.000000 BRAVO\n");
        let graph = NavdataGraph::parse(&fixes, AIRWAYS);
        let route = graph
            .airway_route(&airport("LEXX", 39.8, -4.2), &airport("LFYY", 43.2, 2.2))
            .expect("the chain still routes");
        for leg in route.cumulative_distance_m().windows(2) {
            assert!(
                leg[1] - leg[0] < 1_000_000.0,
                "an airway leg came out {} m long",
                leg[1] - leg[0]
            );
        }
    }

    #[test]
    fn two_fixes_with_no_airway_between_them_have_no_path() {
        let graph = NavdataGraph::parse(FIXES, "");
        assert!(graph.is_empty());
        assert_eq!(graph.shortest_path(0, 3), None);
    }
}
