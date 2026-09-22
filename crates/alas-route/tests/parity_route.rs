// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-route` against `alas.routing`, via
//! `golden/generators/gen_route.py`.
//!
//! Three of the four routing tiers read data this repository cannot contain:
//! the enroute navigation data is GPLv3 and deliberately not bundled, a KML
//! export is a user's own file, and a dispatch plan arrives over the network
//! from a service that only ever returns the most recently generated one. The
//! generator therefore authors its own inputs and checks them in under
//! `golden/route/inputs/`, and **this test parses the same bytes the reference
//! parsed** rather than a description of them.
//!
//! That is the stronger check, not the weaker one. A synthetic set of ten fixes
//! can be built to reach what a real global set reaches only by accident: an
//! identifier naming two unrelated fixes half a world apart, a fix no airway
//! touches, an airport past the transition limit, and two paths between the
//! same pair whose lengths differ by a tenth of a percent.
//!
//! `Tier::Closed` for the distances and coordinates: closed-form spherical
//! trigonometry over `f64`, which is what the tier is for. `Tier::Exact` for
//! everything discrete: which fixes the graph parsed, which fix indices each
//! airway joined, the identifier on each waypoint, the route's source, and
//! whether a route was found at all. A route through the wrong fixes is not a
//! tolerance question.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use std::collections::BTreeMap;
use std::path::Path;

use alas_route::assets;
use alas_route::navdata::NavdataGraph;
use alas_route::route::{haversine_m, Route, RouteSource};
use alas_route::{route_from_kml, route_from_ofp};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::Value;
use support::{compare_route, inputs_dir, read_input, AirportRecord, RouteRecord};

#[derive(Debug, Deserialize)]
struct DistanceCase {
    name: String,
    #[serde(rename = "from")]
    start: [f64; 2],
    #[serde(rename = "to")]
    end: [f64; 2],
    distance_m: f64,
}

#[derive(Debug, Deserialize)]
struct GreatCircleCase {
    name: String,
    origin: String,
    dest: String,
    n: usize,
    route: RouteRecord,
}

#[derive(Debug, Deserialize)]
struct FixRecord {
    ident: String,
    lat: f64,
    lon: f64,
}

#[derive(Debug, Deserialize)]
struct GraphRecord {
    fixes: Vec<FixRecord>,
    connected: Vec<usize>,
    edges: BTreeMap<String, Vec<(usize, f64)>>,
}

#[derive(Debug, Deserialize)]
struct AirwayCase {
    name: String,
    origin: String,
    dest: String,
    route: Option<RouteRecord>,
}

#[derive(Debug, Deserialize)]
struct OfpCase {
    name: String,
    allow_mismatch: bool,
    origin: String,
    dest: String,
    document: Value,
    route: Option<RouteRecord>,
}

#[derive(Debug, Deserialize)]
struct AssetsRecord {
    navdata_base_url: String,
    texture_url: String,
    navdata_rel: String,
    texture_rel: String,
    navdata_files: Vec<String>,
    min_bytes: BTreeMap<String, u64>,
    min_texture_bytes: u64,
    status_partial: bool,
    status_absent: bool,
    texture_absent: bool,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    assets: AssetsRecord,
    airports: BTreeMap<String, AirportRecord>,
    distances: Vec<DistanceCase>,
    great_circles: Vec<GreatCircleCase>,
    graph: GraphRecord,
    airways: Vec<AirwayCase>,
    kml: RouteRecord,
    ofp: Vec<OfpCase>,
}

#[test]
fn the_optional_assets_are_where_python_says_and_as_large() {
    // The size floors are the load-bearing part: a truncated fix file parses
    // happily and produces a quietly wrong route, so a floor that had drifted
    // would let one through.
    let fixture: Fixture = alas_testkit::load("route", "route");
    let expected = &fixture.assets;
    let mut comparison = Comparison::new("alas-route::assets", Tier::Exact);

    comparison
        .exact(
            "navdata_base_url",
            &assets::NAVDATA_BASE_URL.to_owned(),
            &expected.navdata_base_url,
        )
        .exact(
            "texture_url",
            &assets::TEXTURE_URL.to_owned(),
            &expected.texture_url,
        )
        .exact(
            "navdata_rel",
            &assets::NAVDATA_REL.to_owned(),
            &expected.navdata_rel,
        )
        .exact(
            "texture_rel",
            &assets::TEXTURE_REL.to_owned(),
            &expected.texture_rel,
        )
        .exact(
            "navdata_files",
            &assets::NAVDATA_FILES
                .iter()
                .map(|file| file.name.to_owned())
                .collect::<Vec<String>>(),
            &expected.navdata_files,
        )
        .exact(
            "min_bytes",
            &assets::NAVDATA_FILES
                .iter()
                .map(|file| (file.name.to_owned(), file.min_bytes))
                .collect::<BTreeMap<String, u64>>(),
            &expected.min_bytes,
        )
        .exact(
            "min_texture_bytes",
            &assets::MIN_TEXTURE_BYTES,
            &expected.min_texture_bytes,
        );

    // An incomplete set must not report itself usable: the authored inputs
    // carry two of the three files.
    comparison
        .exact(
            "status_partial",
            &assets::navdata_status(&inputs_dir()).available,
            &expected.status_partial,
        )
        .exact(
            "status_absent",
            &assets::navdata_status(Path::new("no/such/directory")).available,
            &expected.status_absent,
        )
        .exact(
            "texture_absent",
            &assets::texture_status(Path::new("no/such/texture.jpg")).available,
            &expected.texture_absent,
        );
    comparison.finish();
}

#[test]
fn great_circle_distances_match_python() {
    let fixture: Fixture = alas_testkit::load("route", "route");
    let mut comparison = Comparison::new("alas-route::route::haversine_m", Tier::Closed);
    for case in &fixture.distances {
        comparison.scalar(
            &case.name,
            haversine_m(case.start[0], case.start[1], case.end[0], case.end[1]),
            case.distance_m,
        );
    }
    comparison.finish();
}

#[test]
fn a_sampled_great_circle_matches_python_point_for_point() {
    let fixture: Fixture = alas_testkit::load("route", "route");
    let mut discrete = Comparison::new("alas-route great circles (structure)", Tier::Exact);
    let mut numeric = Comparison::new("alas-route great circles (coordinates)", Tier::Closed);

    for case in &fixture.great_circles {
        let origin = fixture.airports[&case.origin].airport();
        let dest = fixture.airports[&case.dest].airport();
        let route = Route::great_circle(&origin, &dest, case.n);
        let at = |what: &str| format!("{}: {what}", case.name);
        compare_route(&mut discrete, &mut numeric, &at, &route, &case.route);
    }

    discrete.finish();
    numeric.finish();
}

#[test]
fn the_airway_graph_parses_to_the_same_network_python_parsed() {
    let fixture: Fixture = alas_testkit::load("route", "route");
    let graph = NavdataGraph::parse(&read_input("earth_fix.dat"), &read_input("earth_awy.dat"));

    let mut discrete = Comparison::new("alas-route::navdata (graph structure)", Tier::Exact);
    let mut numeric = Comparison::new("alas-route::navdata (leg lengths)", Tier::Closed);

    // Every occurrence of every identifier, in file order: collapsing two fixes
    // that share a name is the mistake this module's parse exists to avoid, and
    // it would show here as a short list rather than as a wrong route.
    discrete
        .exact("fixes.len", &graph.fixes.len(), &fixture.graph.fixes.len())
        .exact(
            "fixes.idents",
            &graph
                .fixes
                .iter()
                .map(|fix| fix.ident.clone())
                .collect::<Vec<String>>(),
            &fixture
                .graph
                .fixes
                .iter()
                .map(|fix| fix.ident.clone())
                .collect::<Vec<String>>(),
        );
    for (index, (fix, record)) in graph.fixes.iter().zip(&fixture.graph.fixes).enumerate() {
        numeric
            .scalar(&format!("fixes[{index}].lat"), fix.lat, record.lat)
            .scalar(&format!("fixes[{index}].lon"), fix.lon, record.lon);
    }

    // The adjacency is compared by fix *index*, not by identifier, which is
    // what pins the duplicate-identifier disambiguation: an edge wired to the
    // wrong occurrence of a name has the right idents at both ends.
    for (node, neighbors) in &fixture.graph.edges {
        let index: usize = node.parse().expect("a graph node is a fix index");
        let actual = graph.neighbors(index);
        discrete.exact(
            &format!("edges[{node}].to"),
            &actual.iter().map(|&(to, _)| to).collect::<Vec<usize>>(),
            &neighbors.iter().map(|&(to, _)| to).collect::<Vec<usize>>(),
        );
        numeric.slice(
            &format!("edges[{node}].length_m"),
            &actual.iter().map(|&(_, leg)| leg).collect::<Vec<f64>>(),
            &neighbors.iter().map(|&(_, leg)| leg).collect::<Vec<f64>>(),
        );
    }
    discrete.exact(
        "connected",
        &graph.connected_fixes().to_vec(),
        &fixture.graph.connected,
    );

    discrete.finish();
    numeric.finish();
}

#[test]
fn every_airway_route_matches_python() {
    let fixture: Fixture = alas_testkit::load("route", "route");
    let graph = NavdataGraph::parse(&read_input("earth_fix.dat"), &read_input("earth_awy.dat"));

    let mut discrete = Comparison::new("alas-route airway routes (structure)", Tier::Exact);
    let mut numeric = Comparison::new("alas-route airway routes (coordinates)", Tier::Closed);

    for case in &fixture.airways {
        let origin = fixture.airports[&case.origin].airport();
        let dest = fixture.airports[&case.dest].airport();
        let route = graph.airway_route(&origin, &dest);
        let at = |what: &str| format!("{}: {what}", case.name);

        // Whether a route exists at all is the first thing to agree on: a
        // translation that always found one would fail the transition-limit
        // case here rather than silently routing through half the planet.
        discrete.exact(&at("found"), &route.is_some(), &case.route.is_some());
        let (Some(route), Some(expected)) = (route, case.route.as_ref()) else {
            continue;
        };
        compare_route(&mut discrete, &mut numeric, &at, &route, expected);
    }

    discrete.finish();
    numeric.finish();
}

#[test]
fn an_exported_kml_route_matches_python() {
    let fixture: Fixture = alas_testkit::load("route", "route");
    let route = route_from_kml(&read_input("route.kml")).expect("the authored export parses");

    let mut discrete = Comparison::new("alas-route::kml (structure)", Tier::Exact);
    let mut numeric = Comparison::new("alas-route::kml (coordinates)", Tier::Closed);
    let at = |what: &str| format!("route.kml: {what}");
    compare_route(&mut discrete, &mut numeric, &at, &route, &fixture.kml);
    discrete.finish();
    numeric.finish();
}

#[test]
fn a_dispatch_plan_reads_the_same_route_python_read() {
    let fixture: Fixture = alas_testkit::load("route", "route");
    let mut discrete = Comparison::new("alas-route::simbrief (structure)", Tier::Exact);
    let mut numeric = Comparison::new("alas-route::simbrief (coordinates)", Tier::Closed);

    for case in &fixture.ofp {
        let origin = fixture.airports[&case.origin].airport();
        let dest = fixture.airports[&case.dest].airport();
        let route = route_from_ofp(&case.document, &origin, &dest, case.allow_mismatch);
        let at = |what: &str| format!("{}: {what}", case.name);

        // The refusals are the point of half these cases: a mismatched pair
        // with the override off, and a response of the wrong shape, both have
        // to produce nothing rather than a plausible wrong route.
        discrete.exact(&at("accepted"), &route.is_some(), &case.route.is_some());
        let (Some(route), Some(expected)) = (route, case.route.as_ref()) else {
            continue;
        };
        compare_route(&mut discrete, &mut numeric, &at, &route, expected);
    }

    discrete.finish();
    numeric.finish();
}

#[test]
fn the_authored_inputs_still_reach_the_branches_they_were_written_for() {
    // These inputs exist because the real data cannot be committed, so nothing
    // outside this repository will notice if one stops covering its branch.
    let fixture: Fixture = alas_testkit::load("route", "route");

    let mut idents: Vec<&str> = fixture
        .graph
        .fixes
        .iter()
        .map(|fix| fix.ident.as_str())
        .collect();
    idents.sort_unstable();
    let duplicated = idents.windows(2).any(|pair| pair[0] == pair[1]);
    assert!(
        duplicated,
        "no identifier names two fixes, so the disambiguation is unchecked"
    );

    let isolated = fixture.graph.fixes.len() - fixture.graph.connected.len();
    assert!(
        isolated > 0,
        "every fix is on an airway, so the connected-only nearest-fix search is unchecked"
    );

    assert!(
        fixture.airways.iter().any(|case| case.route.is_none()),
        "every airport reached the network, so the transition limit is unchecked"
    );
    assert!(
        fixture.ofp.iter().any(|case| case.route.is_none()),
        "every dispatch plan was accepted, so neither refusal branch is checked"
    );
    assert!(
        fixture
            .ofp
            .iter()
            .any(|case| case.allow_mismatch && case.route.is_some()),
        "no plan overrode the configured pair, so the synthesized airport is unchecked"
    );
    assert_eq!(
        fixture.kml.source,
        RouteSource::SimbriefKml.as_str(),
        "the authored export is no longer read as one"
    );
}
