// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the embedded Selig corpus against `coord_seligFmt.zip`, via the
//! digest manifest and coordinate samples `golden/generators/gen_geom_selig.py`
//! produced from the live archive.
//!
//! The digest walk is the load-bearing check: it covers all 1,665 entries and
//! is what stops `data/selig.txt` drifting from the archive it was extracted
//! from, one byte anywhere in 2.52 MB of raw text being enough to fail it. The
//! coordinate comparison covers a smaller, deliberately chosen sample (see the
//! generator's docstring) at full precision, which the digest alone would not
//! catch a parsing bug in, two different byte streams can hash the same only
//! by chance, but a parser that mishandled `float()` at all could hash
//! correctly (it reads the same bytes) while still returning the wrong numbers.
//!
//! Both are `exact` tier. Parsing a decimal literal into the nearest `f64` is
//! deterministic and correctly rounded in both Rust's and Python's parsers, so
//! there is no accumulated arithmetic here for a tolerance to forgive.

use std::collections::HashMap;

use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    digests: HashMap<String, String>,
    samples: HashMap<String, Vec<(f64, f64)>>,
}

/// FNV-1a, 64-bit, as 16 lowercase hex digits, matching
/// `gen_geom_selig.py`'s `_fnv1a64` exactly. Not a cryptographic hash: this
/// is a corruption-detection digest for one static data file, and pulling in
/// a hashing crate for that would be a dependency change this test has no
/// standing to make (`CLAUDE.md`). The algorithm
/// (http://www.isthe.com/chongo/tech/comp/fnv/) is simple enough to implement
/// identically on both sides of the port from its specification alone; the
/// known-vector test below is what confirms this side got it right.
fn fnv1a64(data: &[u8]) -> String {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut digest = OFFSET_BASIS;
    for &byte in data {
        digest ^= u64::from(byte);
        digest = digest.wrapping_mul(PRIME);
    }
    format!("{digest:016x}")
}

#[test]
fn fnv1a64_matches_the_published_test_vectors() {
    assert_eq!(fnv1a64(b""), "cbf29ce484222325");
    assert_eq!(fnv1a64(b"a"), "af63dc4c8601ec8c");
    assert_eq!(fnv1a64(b"foobar"), "85944171f73967e8");
}

#[test]
fn every_stored_entry_digests_to_what_the_archive_produced() {
    let fixture: Fixture = alas_testkit::load("geom", "selig");

    let mut comparison = Comparison::new("alas-geom::selig (digests)", Tier::Exact);
    for (stem, expected_digest) in &fixture.digests {
        match alas_geom::selig::raw(stem) {
            Some((_, raw)) => {
                comparison.exact(stem, &fnv1a64(raw.as_bytes()), expected_digest);
            }
            None => {
                comparison.exact(&format!("{stem} (present in the corpus)"), &false, &true);
            }
        }
    }
    comparison.finish();
}

#[test]
fn the_corpus_has_exactly_the_archive_entries_the_fixture_names() {
    let fixture: Fixture = alas_testkit::load("geom", "selig");
    let stems = alas_geom::selig::stems();

    assert_eq!(
        stems.len(),
        fixture.digests.len(),
        "corpus has {} entries, the archive's digest manifest has {}",
        stems.len(),
        fixture.digests.len()
    );
    for stem in &stems {
        assert!(
            fixture.digests.contains_key(*stem),
            "{stem} is in the shipped corpus but not in the archive's digest manifest"
        );
    }
}

#[test]
fn resolved_coordinates_match_the_archive_for_the_sample_entries() {
    let fixture: Fixture = alas_testkit::load("geom", "selig");

    let mut comparison = Comparison::new("alas-geom::selig (sample coordinates)", Tier::Exact);
    for (stem, expected) in &fixture.samples {
        match alas_geom::selig::get(stem) {
            Some((_, actual)) => {
                if actual.len() != expected.len() {
                    comparison.exact(
                        &format!("{stem} (point count)"),
                        &actual.len(),
                        &expected.len(),
                    );
                    continue;
                }
                for (index, (&(ax, ay), &(ex, ey))) in actual.iter().zip(expected).enumerate() {
                    comparison.scalar(&format!("{stem}[{index}].x"), ax, ex);
                    comparison.scalar(&format!("{stem}[{index}].y"), ay, ey);
                }
            }
            None => {
                comparison.exact(&format!("{stem} (present in the corpus)"), &false, &true);
            }
        }
    }
    comparison.finish();
}

#[test]
fn the_fixture_names_every_sample_this_test_expects() {
    let fixture: Fixture = alas_testkit::load("geom", "selig");
    // The wing tip's default section (docs/PORTING.md, Geometry); losing this
    // entry from the fixture would silently stop checking the one airfoil the
    // default aircraft actually resolves through this module.
    assert!(fixture.samples.contains_key("naca2410"));
}
