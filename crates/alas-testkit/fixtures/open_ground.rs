// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Open-ground departure fixture shared by the sized-finalist tests of
//! `alas-pipeline`, `alas-report` and `alas-gui`.
//!
//! This file is not compiled into `alas-testkit` (which does not depend on
//! `alas-config`). Each test that needs it includes it with `#[path]`, so the
//! fixture has one source.
//!
//! No curated survey can certify a
//! take-off obstacle clearance, so no registered preset at its own departure
//! has a feasible design (`takeoff_obstacle_clearance_undetermined` on every
//! candidate). The tests that use this fixture check that numbers flow
//! consistently between stages of a finalist, so they need a feasible design.
//! The obstacle check itself is tested in `alas-perf` and in
//! `alas-opt/tests/mission_sized.rs` on the curated surveys.
//!
//! The fixture keeps the preset's route and its runways. It replaces the
//! departure with a hand-entered aerodrome at the same position, elevation,
//! ISA deviation and declared distances. Each of the field's runways is
//! surveyed as open ground: terrain flat at the field elevation, no obstacle,
//! the 0.45 m symmetric uncertainty of a lidar surface, the 14 CFR 77.9(c)
//! mobile-object allowance (23 ft) and no completeness allowance. This is the
//! same survey as `alas-opt`'s `open_ground_departure` (ZOGD, at EDDF). It is
//! a TEST FIXTURE, not physical evidence about the field.

/// ICAO code of the hand-entered aerodrome [`open_ground_at_own_departure`]
/// registers.
pub const OWN_FIELD_OPEN_GROUND_ICAO: &str = "ZOGO";

/// Departs `config` from an open-ground copy of its own departure aerodrome.
///
/// One process registers one copy: a second call for another field panics
/// rather than silently reusing the first field.
pub fn open_ground_at_own_departure(config: &mut alas_config::AlasConfig) {
    use alas_config::airport_io::{
        find_custom, register_custom_airport, AirportProvenance, AirportProvenanceKind,
        CustomAirport,
    };
    use alas_config::airport_obstacles::{departure_surveys, DepartureSurvey};
    static REGISTERED: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    let mut registered = REGISTERED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let field = config.departure_airport.clone();
    if let Some(previous) = registered.as_ref() {
        assert_eq!(
            previous, &field,
            "the open-ground fixture is registered for one field per process"
        );
        config.departure_airport = OWN_FIELD_OPEN_GROUND_ICAO.to_owned();
        return;
    }
    assert!(
        find_custom(OWN_FIELD_OPEN_GROUND_ICAO).is_none(),
        "{OWN_FIELD_OPEN_GROUND_ICAO} is reserved for this fixture"
    );
    let airport = alas_config::airports::get(&field)
        .unwrap_or_else(|error| panic!("{field} is a curated aerodrome: {error:?}"));
    let surveys = departure_surveys(&airport.icao).unwrap_or_else(|| panic!("{field} has surveys"));
    let cell_along_m = 250.0;
    let cell_lateral_m = 30.0;
    let lateral_centres_m: Vec<f64> = (0..41)
        .map(|i| -600.0 + cell_lateral_m * f64::from(i))
        .collect();
    let rows = ((40_000.0 + 2.0 * cell_along_m) / cell_along_m) as usize;
    let open: Vec<DepartureSurvey> = surveys
        .iter()
        .map(|survey| DepartureSurvey {
            runway: survey.runway.clone(),
            heading_deg: survey.heading_deg,
            reference_elevation_m: Some(airport.elevation_m),
            terrain_source: "test fixture: open ground entered by hand".to_owned(),
            vertical_uncertainty_m: Some(0.45),
            one_sided_bias_m: Some(23.0 * 0.3048),
            completeness_allowance_m: None,
            reference_minus_aerodrome_elevation_m: Some(0.0),
            terrain_s0_m: airport.toda_m,
            terrain_max_elevation_m: vec![vec![airport.elevation_m; lateral_centres_m.len()]; rows],
            cell_along_m,
            lateral_centres_m: lateral_centres_m.clone(),
            cell_lateral_m,
            obstacles: Vec::new(),
            provenance: "test fixture".to_owned(),
        })
        .collect();
    register_custom_airport(CustomAirport {
        icao: OWN_FIELD_OPEN_GROUND_ICAO.to_owned(),
        name: format!("Open-ground test fixture at {field}"),
        latitude_deg: airport.latitude_deg,
        longitude_deg: airport.longitude_deg,
        isa_delta_c: airport.isa_deviation_c,
        altitude_m: airport.elevation_m,
        runway_lengths_m: vec![airport.toda_m],
        declared_toda_m: Some(airport.toda_m),
        declared_lda_m: Some(airport.lda_m),
        departure_surveys: open,
        provenance: AirportProvenance {
            kind: AirportProvenanceKind::UserEntered,
            source: Some("ALAS sized-finalist test fixture".to_owned()),
            note: None,
        },
    })
    .expect("the fixture aerodrome registers");
    *registered = Some(field);
    config.departure_airport = OWN_FIELD_OPEN_GROUND_ICAO.to_owned();
}
