// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the embedded Spanish catalog against `alas.translations.es.CATALOG`,
//! dumped verbatim by `golden/generators/gen_i18n.py`.
//!
//! Keys are matched by exact English text, so this is `exact` tier rather
//! than a tolerance comparison: a translation one character off from the
//! Python source is exactly as wrong as one that is unrelated. Checked in
//! both directions -- every fixture entry present with the right value in the
//! shipped catalog, and nothing extra in the shipped catalog the fixture
//! doesn't know about -- because zipping the two together would let a length
//! mismatch (a key dropped from one side, or a stray extra key) hide behind
//! however many pairs happened to still agree.

use std::collections::HashMap;

use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    catalog: HashMap<String, String>,
}

#[test]
fn the_shipped_catalog_matches_every_entry_in_the_fixture() {
    let fixture: Fixture = alas_testkit::load("i18n", "es_catalog");
    let shipped = alas_i18n::es::catalog();

    let mut comparison = Comparison::new("alas-i18n::es", Tier::Exact);
    for (key, expected) in &fixture.catalog {
        match shipped.get(key) {
            Some(actual) => {
                comparison.exact(key, actual, expected);
            }
            None => {
                comparison.exact(
                    &format!("{key} (present in the shipped catalog)"),
                    &false,
                    &true,
                );
            }
        }
    }
    comparison.finish();
}

#[test]
fn the_shipped_catalog_has_nothing_the_fixture_does_not() {
    let fixture: Fixture = alas_testkit::load("i18n", "es_catalog");
    let shipped = alas_i18n::es::catalog();

    let mut comparison = Comparison::new("alas-i18n::es (extra entries)", Tier::Exact);
    for key in shipped.keys() {
        comparison.exact(
            &format!("{key} (present in the fixture)"),
            &fixture.catalog.contains_key(key),
            &true,
        );
    }
    comparison.finish();
}

#[test]
fn the_shipped_catalog_and_the_fixture_have_the_same_entry_count() {
    let fixture: Fixture = alas_testkit::load("i18n", "es_catalog");
    let shipped = alas_i18n::es::catalog();

    // The two comparisons above walk each side against the other's keys, but
    // neither would notice a duplicate-under-some-normalization or a count
    // drift that happened to leave both `contains_key` checks satisfied --
    // this is the length check the parity rule for `slice` reasons about.
    assert_eq!(
        shipped.len(),
        fixture.catalog.len(),
        "shipped catalog has {} entries, fixture has {}",
        shipped.len(),
        fixture.catalog.len()
    );
}
