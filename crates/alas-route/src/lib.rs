// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The lateral path between two airports, at the best fidelity available.
//!
//! Range is what a transport aircraft is sized for, and the distance it has to
//! fly is not the distance between the two cities. A real clearance follows
//! published airways, which on a short European sector adds a few percent to
//! the great circle and on a congested one rather more, and that difference
//! lands directly on the fuel the mission burns and therefore on the takeoff
//! weight the whole design is sized around.
//!
//! # The tiers
//!
//! [`planner::plan_route`] takes the best of four, and each is a better
//! description of what would actually be flown than the one below:
//!
//! 1. A real dispatched flight plan, read by [`simbrief::route_from_ofp`],
//!    which carries the departure and arrival procedures nothing here models.
//! 2. The same plan exported by hand as KML, read by [`kml::route_from_kml`],
//!    which needs no account.
//! 3. This program's own shortest path over real jet airways
//!    ([`navdata::NavdataGraph`]), which needs the open navigation data to have
//!    been downloaded.
//! 4. A great circle ([`route::Route::great_circle`]), which is always
//!    available and which no aircraft is cleared to fly.
//!
//! [`assets`] says where the optional data for tiers two and three lives and
//! whether it is installed.
//!
//! # What this crate does not do
//!
//! It performs no network access. Upstream's routing package fetches a
//! dispatch plan over HTTPS and downloads the navigation data, and both of
//! those would put an HTTP client and a TLS stack into a crate whose job is
//! spherical geometry, and therefore into everything that depends on it.
//! The addresses, the request this needs and the size floors a completed
//! transfer must clear are all stated here; `alas-app` performs the transfers.
//! This is the same boundary `alas-config::settings` draws for file codecs, and
//! it is a documented scope decision rather than a `deviation-candidate`.

pub mod assets;
pub mod kml;
pub mod navdata;
pub mod planner;
pub mod route;
pub mod simbrief;

pub use kml::route_from_kml;
pub use navdata::{navdata_available, Fix, NavdataGraph};
pub use planner::{load_navdata, plan_route, RouteSources};
pub use route::{haversine_m, Route, RouteSource, Waypoint, EARTH_RADIUS_M};
pub use simbrief::{
    fetch_route, fetch_route_with_status, route_from_document, route_from_ofp,
    SimbriefFetchOutcome, SimbriefFetchStatus, SimbriefTransport,
};
