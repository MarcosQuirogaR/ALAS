// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/routing/simbrief_route.py
// Reference: alas @ rust-port-baseline.

//! Reading a real dispatch flight plan into a route.
//!
//! A dispatch service's own engine has already computed real departure,
//! airway and arrival routing against a current data cycle by the time this
//! runs, which is routing this program could not produce itself without
//! sourcing licence-encumbered terminal-procedure data.
//!
//! # What this can and cannot do
//!
//! The public, no-approval endpoint returns an operator's *most recently
//! generated* plan and nothing else: there is no free way to ask for a new
//! dispatch for a particular city pair. So the workflow is to generate the plan
//! for the exact pair on the planner's own site, and then run the analysis for
//! that same pair -- and if the fetched plan turns out to be for somewhere
//! else, it is either someone else's or a stale one, and this says so rather
//! than silently flying the wrong route.
//!
//! # Where the fetching lives
//!
//! This module parses a plan; it does not fetch one. Upstream's `urllib` call
//! is the only network access anywhere in the routing package, and putting an
//! HTTP client and a TLS stack into this crate would put them into everything
//! that computes a distance. [`fetch_url`] builds the request this needs and
//! the application performs it, which is the same boundary
//! `alas-config::settings` already draws for file codecs. This is a documented
//! scope boundary and not a `deviation-candidate`: the transport is absent
//! because it belongs elsewhere in the layering, not because it was reproduced
//! wrongly.

use alas_config::airports::Airport;
use serde_json::Value;

use crate::route::{Route, RouteSource, Waypoint};

/// The endpoint an operator's most recent plan is fetched from.
const FETCH_URL: &str = "https://www.simbrief.com/api/xml.fetcher.php";

/// Field elevations and runway lengths arrive in feet.
const M_PER_FT: f64 = 0.3048;

/// The request that fetches `identifier`'s most recent dispatch plan.
///
/// A numeric identifier is a pilot ID and anything else is a username, which
/// the endpoint takes under different parameter names. Returns `None` for a
/// blank identifier, which is how a caller that has not configured one skips
/// this tier.
pub fn fetch_url(identifier: &str) -> Option<String> {
    let identifier = identifier.trim();
    if identifier.is_empty() {
        return None;
    }
    let parameter = if identifier.chars().all(|c| c.is_ascii_digit()) {
        "userid"
    } else {
        "username"
    };
    Some(format!(
        "{FETCH_URL}?{parameter}={}&json=1",
        percent_encode(identifier)
    ))
}

/// Transport boundary for the SimBrief request.
///
/// The route crate deliberately does not own an HTTPS client. Applications
/// provide this small boundary when they have a transport available, while
/// tests can supply a fixture without network access.
pub trait SimbriefTransport {
    /// Fetch a JSON response from `url`, applying `timeout_s` at the transport.
    fn fetch(&self, url: &str, timeout_s: f64) -> Result<String, String>;
}

/// Observable outcome of the optional SimBrief routing tier.
///
/// The route planner may legitimately continue with KML, navdata, or a great
/// circle after this tier fails. Keeping the failure here prevents that useful
/// fallback from making a live service failure indistinguishable from an
/// account that was never configured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimbriefFetchStatus {
    /// No account identifier was configured, so no request was attempted.
    NotConfigured,
    /// The caller supplied an already-fetched dispatch route.
    SuppliedByCaller,
    /// The live request returned a route accepted for the selected city pair.
    Fetched,
    /// The operating-system transport could not return a document.
    TransportFailed(String),
    /// The response was malformed, incomplete, or for a rejected city pair.
    ResponseRejected,
}

/// Route and status returned by the observable SimBrief fetch boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct SimbriefFetchOutcome {
    /// Accepted dispatch route, when this tier succeeded.
    pub route: Option<Route>,
    /// Why this tier did or did not supply a route.
    pub status: SimbriefFetchStatus,
}

/// Fetch and parse a SimBrief route through an injected transport.
///
/// Transport failures and schema failures return `None`, preserving the route
/// planner's fall-through contract.
pub fn fetch_route<T: SimbriefTransport>(
    transport: &T,
    identifier: &str,
    origin: &Airport,
    dest: &Airport,
    timeout_s: f64,
    allow_mismatch: bool,
) -> Option<Route> {
    fetch_route_with_status(
        transport,
        identifier,
        origin,
        dest,
        timeout_s,
        allow_mismatch,
    )
    .route
}

/// Fetch and parse a SimBrief route without discarding the tier's outcome.
pub fn fetch_route_with_status<T: SimbriefTransport>(
    transport: &T,
    identifier: &str,
    origin: &Airport,
    dest: &Airport,
    timeout_s: f64,
    allow_mismatch: bool,
) -> SimbriefFetchOutcome {
    let Some(url) = fetch_url(identifier) else {
        return SimbriefFetchOutcome {
            route: None,
            status: SimbriefFetchStatus::NotConfigured,
        };
    };
    let document = match transport.fetch(&url, timeout_s) {
        Ok(document) => document,
        Err(error) => {
            return SimbriefFetchOutcome {
                route: None,
                status: SimbriefFetchStatus::TransportFailed(error),
            };
        }
    };
    match route_from_document(&document, origin, dest, allow_mismatch) {
        Some(route) => SimbriefFetchOutcome {
            route: Some(route),
            status: SimbriefFetchStatus::Fetched,
        },
        None => SimbriefFetchOutcome {
            route: None,
            status: SimbriefFetchStatus::ResponseRejected,
        },
    }
}

/// Parse a JSON response returned by SimBrief into a route.
///
/// Keeping JSON decoding at the input boundary makes the transport replaceable
/// and lets public pipeline tests use the same document the application would
/// receive from the service.
pub fn route_from_document(
    document: &str,
    origin: &Airport,
    dest: &Airport,
    allow_mismatch: bool,
) -> Option<Route> {
    let plan: Value = serde_json::from_str(document).ok()?;
    route_from_ofp(&plan, origin, dest, allow_mismatch)
}

/// Percent-encode a query parameter, leaving the characters a URL may carry
/// unescaped.
///
/// The unreserved set of RFC 3986 plus the path characters `/` and `~`, which
/// is what Python's `urllib.parse.quote` leaves alone by default. A dispatch
/// identifier is in practice alphanumeric, so this almost never changes
/// anything -- it is here so that one containing a space produces a request
/// rather than a malformed URL.
fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"_.-~/".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// Read a fetched dispatch plan into a route.
///
/// `allow_mismatch` decides what happens when the plan is for a different city
/// pair than the one asked for. With it off, this returns `None` and the caller
/// falls through to the next routing tier. With it on, the plan is used and its
/// *own* airports come back on the route: splicing the configured pair's
/// coordinates onto someone else's navigation log would produce a track through
/// neither city.
///
/// Never fails loudly. A response whose shape does not match returns `None`
/// rather than guessing at a schema that has changed, because the alternative
/// is a plausible route built out of misread fields.
pub fn route_from_ofp(
    plan: &Value,
    origin: &Airport,
    dest: &Airport,
    allow_mismatch: bool,
) -> Option<Route> {
    let plan_origin = icao_of(plan.get("origin")?)?;
    let plan_dest = icao_of(plan.get("destination")?)?;
    let mismatch =
        plan_origin != origin.icao.to_uppercase() || plan_dest != dest.icao.to_uppercase();

    if mismatch && !allow_mismatch {
        tracing::info!(
            plan_origin = %plan_origin,
            plan_dest = %plan_dest,
            requested_origin = %origin.icao.to_uppercase(),
            requested_dest = %dest.icao.to_uppercase(),
            "the most recent dispatch plan is for a different city pair; \
             generate that exact pair first. Using the next route tier."
        );
        return None;
    }

    let effective_origin = if mismatch {
        airport_from_ofp(plan.get("origin")?, origin)
    } else {
        origin.clone()
    };
    let effective_dest = if mismatch {
        airport_from_ofp(plan.get("destination")?, dest)
    } else {
        dest.clone()
    };
    if mismatch {
        tracing::info!(
            plan_origin = %plan_origin,
            plan_dest = %plan_dest,
            requested_origin = %origin.icao.to_uppercase(),
            requested_dest = %dest.icao.to_uppercase(),
            "overriding the configured city pair with the dispatch plan's own, \
             which is the highest-fidelity route available"
        );
    }

    // The service's XML-to-JSON conversion collapses a single-entry navigation
    // log to a bare object rather than a one-element list.
    let navlog = plan.get("navlog")?.get("fix")?;
    let fixes: Vec<&Value> = match navlog {
        Value::Array(entries) => entries.iter().collect(),
        entry => vec![entry],
    };

    let mut waypoints = vec![Waypoint::named(
        effective_origin.latitude_deg,
        effective_origin.longitude_deg,
        effective_origin.icao.clone(),
    )];
    for fix in fixes {
        waypoints.push(Waypoint::named(
            as_number(fix.get("pos_lat")?)?,
            as_number(fix.get("pos_long")?)?,
            fix.get("ident").and_then(as_text).unwrap_or_default(),
        ));
    }
    waypoints.push(Waypoint::named(
        effective_dest.latitude_deg,
        effective_dest.longitude_deg,
        effective_dest.icao.clone(),
    ));

    Some(Route {
        waypoints,
        source: RouteSource::SimbriefApi,
        origin_airport: Some(effective_origin),
        dest_airport: Some(effective_dest),
    })
}

/// An aerodrome synthesized from a dispatch plan's own endpoint data.
///
/// Used when the plan overrides the configured pair: its airport may not be in
/// this program's own table at all. Anything the plan does not supply falls
/// back to the configured airport's value, so the field-performance
/// calculations downstream still have a real number to work with.
fn airport_from_ofp(node: &Value, fallback: &Airport) -> Airport {
    let field = |key: &str, default: f64| node.get(key).and_then(as_number).unwrap_or(default);
    let icao = node
        .get("icao_code")
        .and_then(as_text)
        .map(|code| code.trim().to_uppercase())
        .filter(|code| !code.is_empty())
        .unwrap_or_else(|| fallback.icao.clone());
    let name = node
        .get("name")
        .and_then(as_text)
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| icao.clone());
    // The fallback elevation is converted to feet before being converted back,
    // as upstream does, so that a plan supplying no elevation reproduces the
    // configured one rather than a value scaled by the conversion.
    let elevation_m = field("elevation", fallback.elevation_m / M_PER_FT) * M_PER_FT;
    let runway_m = field("plan_rwy_length", 0.0) * M_PER_FT;

    Airport {
        name: format!("{name} ({icao})"),
        icao,
        elevation_m,
        toda_m: if runway_m == 0.0 {
            fallback.toda_m
        } else {
            runway_m
        },
        lda_m: if runway_m == 0.0 {
            fallback.lda_m
        } else {
            runway_m
        },
        isa_deviation_c: fallback.isa_deviation_c,
        notes: "From SimBrief OFP".to_owned(),
        latitude_deg: field("pos_lat", fallback.latitude_deg),
        longitude_deg: field("pos_long", fallback.longitude_deg),
    }
}

/// The uppercased ICAO code of an endpoint node.
fn icao_of(node: &Value) -> Option<String> {
    node.get("icao_code")
        .and_then(as_text)
        .map(|code| code.trim().to_uppercase())
}

/// A field that may arrive as a number or as the string a JSON conversion of
/// XML produces for one.
fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// A field that may arrive as a string or as a bare number.
fn as_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct FixtureTransport {
        response: String,
    }

    impl SimbriefTransport for FixtureTransport {
        fn fetch(&self, url: &str, timeout_s: f64) -> Result<String, String> {
            assert!(url.contains("username=pilot"));
            assert_eq!(timeout_s, 3.0);
            Ok(self.response.clone())
        }
    }

    struct FailingTransport;

    impl SimbriefTransport for FailingTransport {
        fn fetch(&self, _url: &str, _timeout_s: f64) -> Result<String, String> {
            Err("service unavailable".to_owned())
        }
    }

    fn airport(icao: &str, lat: f64, lon: f64) -> Airport {
        Airport {
            name: icao.to_owned(),
            icao: icao.to_owned(),
            elevation_m: 600.0,
            toda_m: 4100.0,
            lda_m: 3500.0,
            isa_deviation_c: 12.0,
            notes: String::new(),
            latitude_deg: lat,
            longitude_deg: lon,
        }
    }

    fn plan(origin: &str, dest: &str, fixes: Value) -> Value {
        json!({
            "origin": {"icao_code": origin, "pos_lat": "40.47", "pos_long": "-3.56"},
            "destination": {"icao_code": dest, "pos_lat": "51.15", "pos_long": "-0.19"},
            "navlog": {"fix": fixes},
        })
    }

    #[test]
    fn a_numeric_identifier_is_a_pilot_id_and_anything_else_is_a_username() {
        assert!(fetch_url("123456")
            .expect("a digit string is an identifier")
            .contains("userid=123456"));
        assert!(fetch_url("someone")
            .expect("a word is an identifier")
            .contains("username=someone"));
        assert_eq!(fetch_url("   "), None);
    }

    #[test]
    fn the_observable_fetch_distinguishes_unconfigured_transport_and_response_failures() {
        let origin = airport("LEMD", 40.47, -3.56);
        let destination = airport("EGKK", 51.15, -0.19);

        let unconfigured =
            fetch_route_with_status(&FailingTransport, "", &origin, &destination, 3.0, false);
        assert_eq!(unconfigured.status, SimbriefFetchStatus::NotConfigured);

        let failed = fetch_route_with_status(
            &FailingTransport,
            "pilot",
            &origin,
            &destination,
            3.0,
            false,
        );
        assert_eq!(
            failed.status,
            SimbriefFetchStatus::TransportFailed("service unavailable".to_owned())
        );

        let rejected = fetch_route_with_status(
            &FixtureTransport {
                response: "not json".to_owned(),
            },
            "pilot",
            &origin,
            &destination,
            3.0,
            false,
        );
        assert_eq!(rejected.status, SimbriefFetchStatus::ResponseRejected);
    }

    #[test]
    fn an_identifier_with_a_space_produces_a_request_rather_than_a_broken_url() {
        let url = fetch_url("two words").expect("it is not blank");
        assert!(url.contains("username=two%20words"), "{url}");
    }

    #[test]
    fn a_matching_plan_keeps_the_airports_the_caller_asked_for() {
        let document = plan(
            "LEMD",
            "EGKK",
            json!([{"pos_lat": "42.0", "pos_long": "-2.0", "ident": "BRAVO"}]),
        );
        let route = route_from_ofp(
            &document,
            &airport("LEMD", 40.47, -3.56),
            &airport("EGKK", 51.15, -0.19),
            false,
        )
        .expect("the pair matches");
        assert_eq!(route.source, RouteSource::SimbriefApi);
        assert_eq!(route.waypoints.len(), 3);
        assert_eq!(route.waypoints[1].ident, "BRAVO");
        // No override happened, so the caller's own airports still stand.
        assert_eq!(
            route.origin_airport.map(|a| a.icao),
            Some("LEMD".to_owned())
        );
    }

    #[test]
    fn a_plan_for_somewhere_else_is_refused_unless_it_is_allowed_to_win() {
        let document = plan(
            "KJFK",
            "KLAX",
            json!([{"pos_lat": "40.0", "pos_long": "-90.0", "ident": "MIDWY"}]),
        );
        let requested_origin = airport("LEMD", 40.47, -3.56);
        let requested_dest = airport("EGKK", 51.15, -0.19);
        assert_eq!(
            route_from_ofp(&document, &requested_origin, &requested_dest, false),
            None
        );

        let route = route_from_ofp(&document, &requested_origin, &requested_dest, true)
            .expect("the override is allowed");
        // The plan's own endpoints define the track: keeping the configured
        // ones would splice a Madrid endpoint onto a New York flight.
        let origin = route.origin_airport.expect("an override names its airport");
        assert_eq!(origin.icao, "KJFK");
        assert_eq!(route.waypoints[0].lat, 40.47);
        assert_eq!(origin.notes, "From SimBrief OFP");
    }

    #[test]
    fn an_overridden_airport_keeps_what_the_plan_does_not_supply() {
        // A synthesized airport still has to answer a field-performance
        // question, so the runway and the temperature deviation fall back.
        let document = plan("KJFK", "KLAX", json!([]));
        let fallback = airport("LEMD", 40.47, -3.56);
        let route = route_from_ofp(&document, &fallback, &airport("EGKK", 51.15, -0.19), true)
            .expect("the override is allowed");
        let origin = route.origin_airport.expect("an override names its airport");
        assert_eq!(origin.toda_m, fallback.toda_m);
        assert_eq!(origin.isa_deviation_c, fallback.isa_deviation_c);
        assert!((origin.elevation_m - fallback.elevation_m).abs() < 1e-9);
    }

    #[test]
    fn a_single_fix_navigation_log_arrives_as_one_object_and_not_a_list() {
        let document = plan(
            "LEMD",
            "EGKK",
            json!({"pos_lat": "45.0", "pos_long": "-1.0", "ident": "SOLO"}),
        );
        let route = route_from_ofp(
            &document,
            &airport("LEMD", 40.47, -3.56),
            &airport("EGKK", 51.15, -0.19),
            false,
        )
        .expect("one fix is still a plan");
        assert_eq!(route.waypoints.len(), 3);
        assert_eq!(route.waypoints[1].ident, "SOLO");
    }

    #[test]
    fn a_response_of_the_wrong_shape_is_declined_rather_than_guessed_at() {
        let document = json!({"error": "no flightplan found"});
        assert_eq!(
            route_from_ofp(
                &document,
                &airport("LEMD", 40.47, -3.56),
                &airport("EGKK", 51.15, -0.19),
                true
            ),
            None
        );
    }

    #[test]
    fn an_injected_transport_delivers_a_simbrief_fixture_to_the_route_parser() {
        let document = plan(
            "LEMD",
            "EGKK",
            json!([{"pos_lat": "42.0", "pos_long": "-2.0", "ident": "BRAVO"}]),
        )
        .to_string();
        let transport = FixtureTransport { response: document };
        let route = fetch_route(
            &transport,
            "pilot",
            &airport("LEMD", 40.47, -3.56),
            &airport("EGKK", 51.15, -0.19),
            3.0,
            false,
        )
        .expect("fixture transport produces a route");
        assert_eq!(route.source, RouteSource::SimbriefApi);
        assert_eq!(route.waypoints[1].ident, "BRAVO");
    }
}
