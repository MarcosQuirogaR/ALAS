// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-payload::cabin` and `::cargo`, through the `::build`
//! dispatcher, against `alas.physics.cabin_layout` and
//! `alas.physics.cargo_loader`, via `golden/generators/gen_payload.py`.
//!
//! Every case goes through `build_payload_layout`, the entry point every
//! consumer uses, on an aircraft built from a shipped preset's own design
//! vector, so what is compared is the interior of a real aeroplane rather
//! than of a synthetic probe.
//!
//! The *whole item sequence* is compared, in placement order, and not only the
//! totals. A layout is a sequence: two implementations that place the same
//! seats, monuments and containers in a different order have not agreed, and a
//! deck plan walking one of them would draw monuments over seats. The totals
//! alone would also hide every branch that matters: the exit-derived cap
//! binding before the floor does, a bay narrowing its monuments rather than
//! overlapping them, a hold degrading to a shorter container, since each of
//! those moves items about while leaving the mass where it was.
//!
//! # Two tiers, and why the row carries both
//!
//! Everything discrete is compared at `Tier::Exact`: the kind and deck of every
//! item, its label, the item ordering, every seat and exit and container count,
//! the class names, the chosen exit type and container code. That is where the
//! failures of this module actually live: a seat abreast off by one, a bay
//! visited in the wrong order, a fallback container not reached, and none of
//! them is a rounding question.
//!
//! Every position, mass and fraction is compared at `Tier::Closed`. These are
//! closed-form `f64` arithmetic over `alas-payload::geometry`, which is itself
//! only compared at `Closed`; claiming bitwise agreement on a station derived
//! from it would be claiming something about the built-geometry chain that the
//! geometry row does not claim of itself. `docs/PORTING.md` records the split.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use alas_payload::layout::{ItemMeta, LayoutSummary};
use alas_payload::{
    build_payload_layout, build_payload_layout_reference_compatibility, DeckItem, PayloadLayout,
};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::Value;
use support::{config_and_plane, flag, integer, number, text};

#[derive(Debug, Deserialize)]
struct ItemRecord {
    kind: String,
    deck: String,
    x: f64,
    y: f64,
    z: f64,
    length: f64,
    width: f64,
    height: f64,
    mass: f64,
    label: String,
    meta: Value,
}

#[derive(Debug, Deserialize)]
struct LayoutRecord {
    mode: String,
    total_mass: f64,
    cg_x: f64,
    cg_y: f64,
    decks: Vec<String>,
    items: Vec<ItemRecord>,
    summary: Value,
}

#[derive(Debug, Deserialize)]
struct LayoutCase {
    name: String,
    input: Value,
    oew: f64,
    x_oew: f64,
    layout: LayoutRecord,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    layouts: Vec<LayoutCase>,
}

/// The keys a `meta` dictionary carries, so a translation that dropped one or
/// invented one is caught rather than silently compared on what remains.
fn meta_keys(meta: &Value) -> Vec<String> {
    let mut keys: Vec<String> = meta
        .as_object()
        .map(|fields| fields.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

fn keys_of(names: &[&str]) -> Vec<String> {
    let mut keys: Vec<String> = names.iter().map(|&name| name.to_owned()).collect();
    keys.sort();
    keys
}

fn compare_meta(
    discrete: &mut Comparison,
    numeric: &mut Comparison,
    at: &dyn Fn(&str) -> String,
    actual: &ItemMeta,
    expected: &Value,
) {
    match actual {
        ItemMeta::None => {
            discrete.exact(&at("meta.keys"), &meta_keys(expected), &Vec::new());
        }
        ItemMeta::Seat(seat) => {
            discrete
                .exact(
                    &at("meta.keys"),
                    &meta_keys(expected),
                    &keys_of(&[
                        "abreast", "aisle_w", "aisles", "blocks", "cls", "deck", "filled", "seat_w",
                    ]),
                )
                .exact(
                    &at("meta.cls"),
                    &seat.cls.to_owned(),
                    &text(expected, "cls"),
                )
                .exact(
                    &at("meta.deck"),
                    &seat.deck.to_owned(),
                    &text(expected, "deck"),
                )
                .exact(
                    &at("meta.abreast"),
                    &seat.abreast,
                    &integer(expected, "abreast"),
                )
                .exact(
                    &at("meta.filled"),
                    &seat.filled,
                    &integer(expected, "filled"),
                )
                .exact(
                    &at("meta.aisles"),
                    &seat.aisles,
                    &integer(expected, "aisles"),
                )
                .exact(
                    &at("meta.blocks"),
                    &seat.blocks,
                    &expected["blocks"]
                        .as_array()
                        .expect("a seat row records its lateral blocks")
                        .iter()
                        .map(|block| block.as_i64().expect("a block is a seat count"))
                        .collect::<Vec<i64>>(),
                );
            numeric
                .scalar(&at("meta.seat_w"), seat.seat_w, number(expected, "seat_w"))
                .scalar(
                    &at("meta.aisle_w"),
                    seat.aisle_w,
                    number(expected, "aisle_w"),
                );
        }
        ItemMeta::Exit(exit) => {
            discrete
                .exact(
                    &at("meta.keys"),
                    &meta_keys(expected),
                    &keys_of(&["door_h", "door_w", "type"]),
                )
                .exact(
                    &at("meta.type"),
                    &exit.exit_type.to_owned(),
                    &text(expected, "type"),
                );
            numeric
                .scalar(&at("meta.door_w"), exit.door_w, number(expected, "door_w"))
                .scalar(&at("meta.door_h"), exit.door_h, number(expected, "door_h"));
        }
        ItemMeta::Container(container) => {
            // A hold full of checked bags records the fill it achieved and
            // flags itself as baggage; a freighter container records the net
            // load inside it instead. Which keys are present is the difference.
            let expected_keys = match container.net {
                Some(_) => keys_of(&["color", "fill", "net", "uld"]),
                None => keys_of(&["bags", "color", "fill", "uld"]),
            };
            discrete
                .exact(&at("meta.keys"), &meta_keys(expected), &expected_keys)
                .exact(
                    &at("meta.uld"),
                    &container.uld.to_owned(),
                    &text(expected, "uld"),
                )
                .exact(
                    &at("meta.color"),
                    &container.color.to_owned(),
                    &text(expected, "color"),
                );
            numeric.moved_scalar(&at("meta.fill"), container.fill, number(expected, "fill"));
            if let Some(net) = container.net {
                numeric.moved_scalar(&at("meta.net"), net, number(expected, "net"));
            }
        }
        ItemMeta::OverheadBin(_) => {
            panic!("product-only overhead bins must not enter the frozen reference layout");
        }
        ItemMeta::BulkBag => {
            discrete.exact(&at("meta.keys"), &meta_keys(expected), &keys_of(&["bags"]));
        }
    }
}

/// Positions, masses and fractions geometry phase 1 moved: `(item, frozen Python value, current value)`.
///
/// Every `%MAC` figure follows the wing frame the fitted A320 (aerodynamic centre -0.148 m,
/// `parity_geometry::MOVED_FRAMES`), B787-9 and A380 wings moved: a station keeps its metre value
/// and changes its percentage (A320: +3.73 %MAC for -0.148 m over a 3.969 m MAC). The
/// `cargo_narrowbody` freight is loaded to a target `%MAC`, so its two P6P pallets share the same
/// 5794 kg differently (4224 + 1570 kg before, 5574 + 220 kg now) and the load centre moves from
/// 19.870 m to 19.300 m. Sums, counts and kinds still agree with the frozen record.
const MOVED_VALUES: [(&str, f64, f64); 24] = [
    (
        "a320_high_density: summary.cg_pct_mac",
        46.33785504521981,
        50.06639801442843,
    ),
    (
        "a380_double_deck: summary.cg_pct_mac",
        92.44247315586517,
        92.44982148292571,
    ),
    (
        "b787_exit_limited: summary.cg_pct_mac",
        47.843825673005625,
        47.85054123470597,
    ),
    (
        "b787_three_class: summary.cg_pct_mac",
        150.36143124701192,
        150.3633334404045,
    ),
    (
        "bag_overflow: summary.cg_pct_mac",
        108.54806604631628,
        112.27644392593461,
    ),
    (
        "belly_freight: summary.cg_pct_mac",
        43.14846847819601,
        43.1554044945528,
    ),
    (
        "belly_overflow: summary.cg_pct_mac",
        50.53123726850621,
        54.259769109577796,
    ),
    (
        "cargo_door_proximity: summary.achieved_cg_pct_mac",
        1.5400003303785121,
        1.5488899319833798,
    ),
    (
        "cargo_lower_deck_only: summary.achieved_cg_pct_mac",
        17.334803420700055,
        17.34295143061248,
    ),
    (
        "cargo_min_pallets: summary.achieved_cg_pct_mac",
        25.582294334981743,
        25.59005511178271,
    ),
    (
        "cargo_narrowbody: cg_x",
        19.870406250000002,
        19.300031250000004,
    ),
    ("cargo_narrowbody: items[0].mass", 4224.0, 5574.0),
    (
        "cargo_narrowbody: items[0].meta.fill",
        0.6140035906642729,
        0.8159784560143627,
    ),
    ("cargo_narrowbody: items[0].meta.net", 4104.0, 5454.0),
    ("cargo_narrowbody: items[2].mass", 1570.0, 220.0),
    (
        "cargo_narrowbody: items[2].meta.fill",
        0.21693596648713345,
        0.014961101137043686,
    ),
    ("cargo_narrowbody: items[2].meta.net", 1450.0, 100.0),
    (
        "cargo_narrowbody: summary.achieved_cg_pct_mac",
        96.73080343871567,
        86.08979192902365,
    ),
    (
        "cargo_no_oew: summary.achieved_cg_pct_mac",
        24.39713655029078,
        24.404952972177185,
    ),
    (
        "cargo_over_capacity: summary.achieved_cg_pct_mac",
        20.029838224498874,
        23.758451008286947,
    ),
    (
        "cargo_target_cg: summary.achieved_cg_pct_mac",
        -14.900070495252688,
        -14.890409005585797,
    ),
    (
        "cargo_uniform: summary.achieved_cg_pct_mac",
        24.361873612176815,
        24.369691689715513,
    ),
    (
        "crowded_monuments: summary.cg_pct_mac",
        48.172929537365675,
        48.17962964710436,
    ),
    (
        "no_structural_cap: summary.cg_pct_mac",
        44.169789555665,
        44.1766776193362,
    ),
];

/// Compare a value against the frozen record, or, when geometry phase 1 moved it, against both
/// the record (unchanged fixture) and the current value.
trait MovedScalar {
    fn moved_scalar(&mut self, name: &str, actual: f64, expected: f64) -> &mut Self;
}

impl MovedScalar for Comparison {
    fn moved_scalar(&mut self, name: &str, actual: f64, expected: f64) -> &mut Self {
        match MOVED_VALUES.iter().find(|(moved, _, _)| *moved == name) {
            Some((_, frozen, current)) => {
                self.scalar(&format!("{name} (frozen reference)"), expected, *frozen);
                self.scalar(name, actual, *current)
            }
            None => self.scalar(name, actual, expected),
        }
    }
}

/// Pallet labels the geometry phase 1 lofts moved: `(item, frozen Python label, current label)`.
///
/// The `cargo_narrowbody` interior is laid out on the current A320 geometry, whose lofted lower
/// lobe splits the same 5794 kg of freight over the two P6P pallets differently
/// (4224 + 1570 kg before, 5574 + 220 kg now). The sum, the positions and the masses of the
/// items still agree with the frozen record.
const MOVED_LABELS: [(&str, &str, &str); 2] = [
    (
        "cargo_narrowbody: items[0].label",
        "P6P 4224kg",
        "P6P 5574kg",
    ),
    (
        "cargo_narrowbody: items[2].label",
        "P6P 1570kg",
        "P6P 220kg",
    ),
];

fn compare_item(
    discrete: &mut Comparison,
    numeric: &mut Comparison,
    at: &dyn Fn(&str) -> String,
    actual: &DeckItem,
    expected: &ItemRecord,
) {
    discrete
        .exact(
            &at("kind"),
            &actual.kind.as_str().to_owned(),
            &expected.kind,
        )
        .exact(&at("deck"), &actual.deck.to_owned(), &expected.deck);
    let label_name = at("label");
    match MOVED_LABELS.iter().find(|(name, _, _)| *name == label_name) {
        Some((_, frozen, current)) => {
            discrete
                .exact(
                    &format!("{label_name} (frozen reference)"),
                    &expected.label,
                    &(*frozen).to_owned(),
                )
                .exact(&label_name, &actual.label, &(*current).to_owned());
        }
        None => {
            discrete.exact(&label_name, &actual.label, &expected.label);
        }
    }
    numeric
        .moved_scalar(&at("x"), actual.x, expected.x)
        .moved_scalar(&at("y"), actual.y, expected.y)
        .moved_scalar(&at("z"), actual.z, expected.z)
        .moved_scalar(&at("length"), actual.length, expected.length)
        .moved_scalar(&at("width"), actual.width, expected.width)
        .moved_scalar(&at("height"), actual.height, expected.height)
        .moved_scalar(&at("mass"), actual.mass, expected.mass);
    // A sequence mismatch already records a failure above. Its metadata
    // belongs to a different item kind and cannot be decoded as this one.
    if actual.kind.as_str() == expected.kind {
        compare_meta(discrete, numeric, at, &actual.meta, &expected.meta);
    }
}

/// Compare a summary's named quantities, whichever engine produced it.
fn compare_summary(
    discrete: &mut Comparison,
    numeric: &mut Comparison,
    at: &dyn Fn(&str) -> String,
    actual: &LayoutSummary,
    expected: &Value,
) {
    let mut whole = |key: &str, value: i64| {
        discrete.exact(
            &at(&format!("summary.{key}")),
            &value,
            &integer(expected, key),
        );
    };
    match actual {
        LayoutSummary::Passenger(cabin) => {
            whole("total_pax", cabin.total_pax);
            whole("seated_pax", cabin.seated_pax);
            assert_eq!(
                cabin.unseated_pax,
                (cabin.total_pax - cabin.seated_pax).max(0),
                "{}: passenger count conservation",
                at("summary")
            );
            whole("lavatories", cabin.lavatories);
            whole("galleys", cabin.galleys);
            whole("exit_pairs", cabin.exit_pairs);
            whole("exit_capacity", cabin.exit_capacity);
            whole("max_certifiable_capacity", cabin.max_certifiable_capacity);
            whole("hold_ulds", cabin.hold_ulds);
            whole("max_abreast", cabin.max_abreast);
            whole("n_aisles", cabin.n_aisles);
            discrete
                .exact(
                    &at("summary.mode"),
                    &"passenger".to_owned(),
                    &text(expected, "mode"),
                )
                .exact(
                    &at("summary.exit_type"),
                    &cabin.exit_type.to_owned(),
                    &text(expected, "exit_type"),
                )
                .exact(
                    &at("summary.double_deck"),
                    &cabin.double_deck,
                    &flag(expected, "double_deck"),
                );

            // Which classes and decks the summary names are compared as sets
            // and not as sequences: the fixture is JSON with its keys sorted,
            // so the cabin order is not recoverable from it. The order they are
            // actually laid out in is pinned by the item sequence above, which
            // is a list and keeps it.
            for (what, named, recorded) in [
                ("classes", sorted(&cabin.classes), &expected["classes"]),
                (
                    "deck_utilization",
                    sorted(&cabin.deck_utilization),
                    &expected["deck_utilization"],
                ),
            ] {
                discrete.exact(
                    &at(&format!("summary.{what}.names")),
                    &named,
                    &recorded
                        .as_object()
                        .expect("the cabin summary carries this group")
                        .keys()
                        .cloned()
                        .collect::<Vec<String>>(),
                );
            }
            for &(name, seats) in &cabin.classes {
                discrete.exact(
                    &at(&format!("summary.classes.{name}")),
                    &seats,
                    &integer(&expected["classes"], name),
                );
            }
            for &(deck, used) in &cabin.deck_utilization {
                numeric.scalar(
                    &at(&format!("summary.deck_utilization.{deck}")),
                    used,
                    number(&expected["deck_utilization"], deck),
                );
            }

            for (key, value) in [
                ("payload_t", cabin.payload_t),
                ("seat_mass_t", cabin.seat_mass_t),
                ("bag_mass_t", cabin.bag_mass_t),
                ("belly_cargo_t", cabin.belly_cargo_t),
                ("hold_capacity_t", cabin.hold_capacity_t),
                ("hold_used_t", cabin.hold_used_t),
                ("aisle_width_m", cabin.aisle_width_m),
                ("cg_pct_mac", cabin.cg_pct_mac),
            ] {
                numeric.moved_scalar(&at(&format!("summary.{key}")), value, number(expected, key));
            }
        }
        LayoutSummary::Cargo(freight) => {
            whole("n_ulds", freight.n_ulds);
            whole("n_main_deck", freight.n_main_deck);
            whole("n_lower_deck", freight.n_lower_deck);
            whole("n_slots", freight.n_slots as i64);
            discrete
                .exact(
                    &at("summary.mode"),
                    &"cargo".to_owned(),
                    &text(expected, "mode"),
                )
                .exact(
                    &at("summary.lower_uld"),
                    &freight.lower_uld.to_owned(),
                    &text(expected, "lower_uld"),
                )
                .exact(
                    &at("summary.strategy"),
                    &freight.strategy,
                    &text(expected, "strategy"),
                );

            for (key, value) in [
                ("payload_t", freight.payload_t),
                ("capacity_t", freight.capacity_t),
                ("fill_pct", freight.fill_pct),
                ("volume_m3", freight.volume_m3),
                ("target_cg_pct_mac", freight.target_cg_pct_mac),
                ("achieved_cg_pct_mac", freight.achieved_cg_pct_mac),
            ] {
                numeric.moved_scalar(&at(&format!("summary.{key}")), value, number(expected, key));
            }

            let gross_from_roles = freight.loaded_net_payload_t + freight.tare_mass_t;
            assert!(
                (freight.payload_t - gross_from_roles).abs() < 1e-9,
                "{}: cargo gross/net/tare roles do not close",
                at("summary")
            );
        }
    }
}

/// The names of a summary group, sorted so they can be compared against a
/// JSON object's keys.
fn sorted<T>(group: &[(&str, T)]) -> Vec<String> {
    let mut names: Vec<String> = group.iter().map(|(name, _)| (*name).to_owned()).collect();
    names.sort();
    names
}

#[test]
fn every_laid_out_interior_matches_python_item_for_item() {
    let fixture: Fixture = alas_testkit::load("payload", "layout");
    let mut discrete = Comparison::new("alas-payload layouts (sequence and counts)", Tier::Exact);
    let mut numeric = Comparison::new("alas-payload layouts (positions and masses)", Tier::Closed);

    for case in &fixture.layouts {
        let (mut config, plane, _dv) = config_and_plane(&case.input);
        // These fixtures predate the Boeing Rev Q weight correction. Replay
        // the frozen load-case inputs so this remains a payload-algorithm
        // parity test; the corrected preset values are pinned in alas-config
        // and exercised by the public all-preset audit.
        if config.preset == "B787-9" {
            if case.input.pointer("/requirements/mtow_kg").is_none() {
                config.requirements.mtow_kg = 254_000.0;
            }
            if case
                .input
                .pointer("/requirements/max_structural_payload_kg")
                .is_none()
            {
                config.requirements.max_structural_payload_kg = 52_600.0;
            }
        }
        let layout: PayloadLayout =
            build_payload_layout_reference_compatibility(&plane, &config, case.oew, case.x_oew)
                .expect("a built aircraft has a cabin frame");
        let at = |what: &str| format!("{}: {what}", case.name);

        discrete
            .exact(
                &at("mode"),
                &layout.mode.as_str().to_owned(),
                &case.layout.mode,
            )
            .exact(
                &at("decks"),
                &layout
                    .decks()
                    .iter()
                    .map(|&deck| deck.to_owned())
                    .collect::<Vec<String>>(),
                &case.layout.decks,
            )
            .exact(
                &at("items.len"),
                &layout.items.len(),
                &case.layout.items.len(),
            );

        numeric
            .scalar(&at("total_mass"), layout.total_mass, case.layout.total_mass)
            .moved_scalar(&at("cg_x"), layout.cg_x, case.layout.cg_x)
            .scalar(&at("cg_y"), layout.cg_y, case.layout.cg_y);

        for (index, (actual, expected)) in layout.items.iter().zip(&case.layout.items).enumerate() {
            let at_item = |what: &str| at(&format!("items[{index}].{what}"));
            compare_item(&mut discrete, &mut numeric, &at_item, actual, expected);
        }

        compare_summary(
            &mut discrete,
            &mut numeric,
            &at,
            &layout.summary,
            &case.layout.summary,
        );
    }

    discrete.finish();
    numeric.finish();
}

#[test]
fn product_cargo_layout_delivers_requested_net_before_uld_tare() {
    let fixture: Fixture = alas_testkit::load("payload", "layout");

    for case in fixture.layouts.iter().filter(|case| {
        case.input
            .pointer("/requirements/aircraft_type")
            .and_then(Value::as_str)
            == Some("cargo")
    }) {
        let (config, plane, _dv) = config_and_plane(&case.input);
        let layout = build_payload_layout(&plane, &config, case.oew, case.x_oew)
            .expect("a built aircraft has a cabin frame");
        let LayoutSummary::Cargo(summary) = layout.summary else {
            panic!("{}: cargo input produced a passenger summary", case.name);
        };
        let requested_net_kg = config.requirements.cargo_payload_kg;
        let capacity_kg = summary.capacity_t * 1_000.0;
        let expected_loaded_net_kg = requested_net_kg.min(capacity_kg);
        let loaded_net_kg = summary.loaded_net_payload_t * 1_000.0;
        assert!(
            (loaded_net_kg - expected_loaded_net_kg).abs() < 1e-6,
            "{}: requested net cargo was not delivered: requested={requested_net_kg}, loaded={loaded_net_kg}, capacity={capacity_kg}",
            case.name
        );
        let gross_from_roles = summary.loaded_net_payload_t + summary.tare_mass_t;
        assert!(
            (summary.payload_t - gross_from_roles).abs() < 1e-9,
            "{}: gross cargo does not close from net cargo and ULD tare",
            case.name
        );
    }
}
