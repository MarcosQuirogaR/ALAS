// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-payload::build`'s auto-sizer and cabin presets against
//! `alas.physics.payload`, via `golden/generators/gen_payload.py`.
//!
//! These two run *before* any interior is laid out, and everything downstream
//! inherits what they decide: `simulate_passenger_counts` is what turns a class
//! mix given as shares of cabin length into the seat counts the requirements
//! carry, and those counts are the first-pass payload mass the sizing sees.
//! A disagreement here would move every result after it while the detailed
//! layout went on agreeing with its own inputs.
//!
//! `Tier::Exact`: a seat count and a container code are whole things. The only
//! quantities compared at `Tier::Closed` are the seat geometry a preset writes
//! and the freighter payload it derives from the positions' capacity.
//!
//! The last test asserts on the *fixture* rather than on the port. Several of
//! the branches these engines are most likely to get wrong are invisible from
//! a layout's totals, and a case that stopped reaching one would leave it
//! unchecked while every comparison above went on passing.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use std::collections::BTreeMap;

use alas_payload::{apply_cabin_preset, simulate_passenger_counts, CabinGeometry};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::Value;
use support::config_and_plane;

#[derive(Debug, Deserialize)]
struct SimulationCase {
    name: String,
    input: Value,
    mix: BTreeMap<String, f64>,
    counts: BTreeMap<String, i64>,
}

#[derive(Debug, Deserialize)]
struct ClassRecord {
    share_pct: f64,
    count: i64,
    abreast: i64,
    pitch_m: f64,
    width_m: f64,
    mass_per_pax_kg: f64,
}

#[derive(Debug, Deserialize)]
struct CabinRecord {
    class_mix_mode: String,
    first: ClassRecord,
    business: ClassRecord,
    premium: ClassRecord,
    economy: ClassRecord,
    num_passengers: i64,
    cargo_payload_kg: f64,
    use_main_deck: bool,
    main_deck_uld: String,
    lower_deck_uld: String,
    loading_strategy: String,
}

#[derive(Debug, Deserialize)]
struct PresetCase {
    name: String,
    input: Value,
    after: CabinRecord,
}

#[derive(Debug, Deserialize)]
struct LayoutRecord {
    decks: Vec<String>,
    items: Vec<Value>,
    summary: Value,
}

#[derive(Debug, Deserialize)]
struct LayoutCase {
    name: String,
    layout: LayoutRecord,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    layouts: Vec<LayoutCase>,
    simulations: Vec<SimulationCase>,
    cabin_presets: Vec<PresetCase>,
}

#[test]
fn the_seat_count_auto_sizer_matches_python_on_every_shipped_mix() {
    let fixture: Fixture = alas_testkit::load("payload", "layout");
    let mut comparison = Comparison::new("alas-payload::simulate_passenger_counts", Tier::Exact);

    for case in &fixture.simulations {
        let (config, plane, _dv) = config_and_plane(&case.input);
        let g = CabinGeometry::new(
            &plane,
            &config.geometry,
            config.cabin.passenger.wall_thickness_m,
        )
        .expect("a built aircraft has a cabin frame");

        let mix: Vec<(&str, f64)> = case
            .mix
            .iter()
            .map(|(name, share)| (name.as_str(), *share))
            .collect();
        let counts = simulate_passenger_counts(&g, &config.cabin.passenger, &mix);

        for (name, expected) in &case.counts {
            comparison.exact(
                &format!("{}: {name}", case.name),
                &counts.for_class(name),
                expected,
            );
        }
    }
    comparison.finish();
}

#[test]
fn applying_a_cabin_preset_writes_what_python_writes() {
    let fixture: Fixture = alas_testkit::load("payload", "layout");
    let mut discrete = Comparison::new("alas-payload::apply_cabin_preset (counts)", Tier::Exact);
    let mut numeric = Comparison::new("alas-payload::apply_cabin_preset (geometry)", Tier::Closed);

    for case in &fixture.cabin_presets {
        let (mut config, _plane, design_vector) = config_and_plane(&case.input);
        apply_cabin_preset(&mut config, design_vector.as_ref()).expect("the preset applies");
        let at = |what: &str| format!("{}: {what}", case.name);
        let after = &case.after;
        let pax = &config.cabin.passenger;
        let cargo = &config.cabin.cargo;

        discrete
            .exact(
                &at("class_mix_mode"),
                &pax.class_mix_mode,
                &after.class_mix_mode,
            )
            .exact(
                &at("num_passengers"),
                &config.requirements.num_passengers,
                &after.num_passengers,
            )
            .exact(
                &at("use_main_deck"),
                &cargo.use_main_deck,
                &after.use_main_deck,
            )
            .exact(
                &at("main_deck_uld"),
                &cargo.main_deck_uld,
                &after.main_deck_uld,
            )
            .exact(
                &at("lower_deck_uld"),
                &cargo.lower_deck_uld,
                &after.lower_deck_uld,
            )
            .exact(
                &at("loading_strategy"),
                &cargo.loading_strategy,
                &after.loading_strategy,
            );
        numeric.scalar(
            &at("cargo_payload_kg"),
            config.requirements.cargo_payload_kg,
            after.cargo_payload_kg,
        );

        for (name, actual, expected) in [
            ("first", &pax.first, &after.first),
            ("business", &pax.business, &after.business),
            ("premium", &pax.premium, &after.premium),
            ("economy", &pax.economy, &after.economy),
        ] {
            discrete
                .exact(
                    &at(&format!("{name}.count")),
                    &actual.count,
                    &expected.count,
                )
                .exact(
                    &at(&format!("{name}.abreast")),
                    &actual.abreast,
                    &expected.abreast,
                );
            numeric
                .scalar(
                    &at(&format!("{name}.share_pct")),
                    actual.share_pct,
                    expected.share_pct,
                )
                .scalar(
                    &at(&format!("{name}.pitch_m")),
                    actual.pitch_m,
                    expected.pitch_m,
                )
                .scalar(
                    &at(&format!("{name}.width_m")),
                    actual.width_m,
                    expected.width_m,
                )
                .scalar(
                    &at(&format!("{name}.mass_per_pax_kg")),
                    actual.mass_per_pax_kg,
                    expected.mass_per_pax_kg,
                );
        }
    }

    discrete.finish();
    numeric.finish();
}

#[test]
fn the_fixture_still_reaches_the_branches_it_was_written_for() {
    let fixture: Fixture = alas_testkit::load("payload", "layout");
    let named = |name: &str| {
        fixture
            .layouts
            .iter()
            .find(|case| case.name == name)
            .unwrap_or_else(|| panic!("the fixture no longer carries the {name} case"))
    };

    // Only reachable when the checked baggage alone exceeds the holds: the
    // belly freight is clamped to what the bags leave, and the bags are not.
    let overflow = named("bag_overflow");
    assert!(
        overflow
            .layout
            .items
            .iter()
            .any(|item| item["label"] == "Bulk overflow"),
        "no loose block was placed, so the overflow guard is unchecked"
    );

    // The narrowbody hold is too shallow for an LD3, so the loader has to
    // degrade through the fallback chain to the reduced-height container.
    let narrowbody = named("cargo_narrowbody");
    assert_eq!(
        narrowbody.layout.summary["lower_uld"], "AKH",
        "the hold no longer degrades through the fallback chain"
    );

    let double_deck = named("a380_double_deck");
    assert!(
        double_deck.layout.decks.contains(&"upper".to_owned()),
        "the second passenger deck is unchecked"
    );

    // Exit-limited rather than floor-limited: the CS-25.807 cap has to be what
    // truncates the seating.
    let exit_limited = named("b787_exit_limited");
    let summary = &exit_limited.layout.summary;
    assert!(
        summary["seated_pax"].as_i64() < summary["total_pax"].as_i64(),
        "nothing truncated the seating, so the capacity cap is unchecked"
    );

    let strategies: Vec<String> = fixture
        .layouts
        .iter()
        .filter_map(|case| case.layout.summary.get("strategy"))
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect();
    for strategy in ["target_cg", "min_pallets", "door_proximity", "uniform"] {
        assert!(
            strategies.iter().any(|found| found == strategy),
            "no case loads with the {strategy} strategy"
        );
    }
}
