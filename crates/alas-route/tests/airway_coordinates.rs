// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Regression tests for complete airway endpoints and approximate-route quality.

// Test assertions require constructed routes.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::airports::Airport;
use alas_route::navdata::NavdataGraph;
use alas_route::planner::{plan_route_with_max_stretch, RouteSources};
use alas_route::route::{Route, RouteSource};

fn airport(name: &str, lat: f64, lon: f64) -> Airport {
    Airport {
        name: name.into(),
        icao: name.into(),
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
fn coordinate_airways_retain_navaids_missing_from_fix_catalog() {
    let fixes = "0 0 START\n0 4 FINISH\n";
    let airways =
        "I\n640 Version\nSTART 0 0 VOR 0 2 1 0 450 A1\nVOR 0 2 FINISH 0 4 1 0 450 A1\n99\n";
    assert!(NavdataGraph::parse(fixes, airways).is_empty());
    let graph = NavdataGraph::parse_with_airway_coordinates(fixes, airways);
    let route = graph
        .airway_route(&airport("A", 0.0, 0.0), &airport("B", 0.0, 4.0))
        .unwrap();
    assert!(route.waypoints.iter().any(|w| w.ident == "VOR"));
    let direct = Route::great_circle(&airport("A", 0.0, 0.0), &airport("B", 0.0, 4.0), 50);
    assert!((route.total_distance_m() / direct.total_distance_m() - 1.0).abs() < 1e-12);
}

#[test]
fn coordinate_airways_keep_same_named_endpoints_in_distinct_regions() {
    let airways = "I\n640 Version\nDUP 0 0 END 0 1 1 0 450 A1\nDUP 50 0 END 50 1 1 0 450 A2\n";
    let graph = NavdataGraph::parse_with_airway_coordinates("", airways);
    assert_eq!(graph.connected_fixes().len(), 4);
    assert!(graph.shortest_path(0, 2).is_none());
    let route = graph
        .airway_route(&airport("A", 50.0, 0.0), &airport("B", 50.0, 1.0))
        .unwrap();
    assert!(route.waypoints.iter().all(|w| w.lat == 50.0));
}

#[test]
fn sparse_graph_detour_falls_back_but_legacy_and_dispatch_are_selectable() {
    let graph = NavdataGraph::parse_with_airway_coordinates(
        "",
        "I\n640 Version\nA 0 0 X 10 2 1 0 450 A1\nX 10 2 B 0 4 1 0 450 A1\n",
    );
    let a = airport("A", 0.0, 0.0);
    let b = airport("B", 0.0, 4.0);
    let sources = || RouteSources {
        navdata: Some(&graph),
        ..RouteSources::none()
    };
    let legacy = plan_route_with_max_stretch(&a, &b, sources(), 0.0);
    let filtered = plan_route_with_max_stretch(&a, &b, sources(), 1.2);
    assert_eq!(legacy.source, RouteSource::NavdataGraph);
    assert_eq!(filtered.source, RouteSource::GreatCircle);
    assert!(legacy.total_distance_m() > 4.0 * filtered.total_distance_m());
    let mut dispatched = legacy;
    dispatched.source = RouteSource::SimbriefApi;
    let result = plan_route_with_max_stretch(
        &a,
        &b,
        RouteSources {
            dispatched: Some(dispatched.clone()),
            ..sources()
        },
        1.2,
    );
    assert_eq!(result, dispatched);
}

#[test]
fn modern_airways_keep_the_existing_parser() {
    let fixes = "0 0 A\n0 1 B\n";
    let airways = "I\n1100 Version\nA ES 11 B ES 11 N 1 0 450 A1\n";
    let old = NavdataGraph::parse(fixes, airways);
    let new = NavdataGraph::parse_with_airway_coordinates(fixes, airways);
    assert_eq!(old.fixes, new.fixes);
    assert_eq!(old.neighbors(0), new.neighbors(0));
}

#[test]
fn an_imported_kml_route_is_not_subject_to_the_airway_detour_limit() {
    let directory = std::env::temp_dir().join(format!("alas-route-kml-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("A_B.kml"),
        "<kml><LineString><coordinates>0,0,0 2,10,0 4,0,0</coordinates></LineString></kml>",
    )
    .unwrap();
    let a = airport("A", 0.0, 0.0);
    let b = airport("B", 0.0, 4.0);
    let result = plan_route_with_max_stretch(
        &a,
        &b,
        RouteSources {
            routes_dir: Some(&directory),
            ..RouteSources::none()
        },
        1.20,
    );
    std::fs::remove_file(directory.join("A_B.kml")).unwrap();
    std::fs::remove_dir(&directory).unwrap();
    assert_eq!(result.source, RouteSource::SimbriefKml);
    assert!(result.total_distance_m() > 4.0 * Route::great_circle(&a, &b, 50).total_distance_m());
}

#[test]
#[ignore = "requires a locally installed navigation-data snapshot"]
fn installed_navigation_snapshot_route_ratios() {
    let directory = std::env::var("ALAS_TEST_NAVDATA").expect("ALAS_TEST_NAVDATA");
    let fixes = std::fs::read_to_string(format!("{directory}/earth_fix.dat")).unwrap();
    let airways = std::fs::read_to_string(format!("{directory}/earth_awy.dat")).unwrap();
    let old = NavdataGraph::parse(&fixes, &airways);
    let new = NavdataGraph::parse_with_airway_coordinates(&fixes, &airways);
    for (name, lat1, lon1, lat2, lon2) in [
        ("LEMD-LEPA", 40.4719, -3.5626, 39.5517, 2.7388),
        ("EVRA-ESSA", 56.9236, 23.9711, 59.6519, 17.9186),
    ] {
        let a = airport("A", lat1, lon1);
        let b = airport("B", lat2, lon2);
        let direct = Route::great_circle(&a, &b, 50).total_distance_m();
        let legacy = old.airway_route(&a, &b).unwrap();
        let corrected = new.airway_route(&a, &b).unwrap();
        assert!(corrected.total_distance_m() / direct < 1.20, "{name}");
        // The ignored snapshot test prints only when explicitly requested.
        #[allow(clippy::print_stdout)]
        {
            println!(
                "{name}: GCD={:.3} km; legacy={:.5}; complete={:.5}",
                direct / 1000.0,
                legacy.total_distance_m() / direct,
                corrected.total_distance_m() / direct
            );
        }
    }
}
