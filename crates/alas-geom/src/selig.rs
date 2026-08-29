// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from the zip-reading branch of AirfoilLibrary._init_zip / .get in
// alas/geometry/airfoils.py.
// Upstream coordinate data: UIUC Airfoil Coordinates Database, see
// THIRD-PARTY-NOTICES.md.
// Reference: alas @ rust-port-baseline.

//! Lookup into the embedded Selig airfoil coordinate corpus.
//!
//! The reference implementation reads 1,665 `.dat` coordinate files out of
//! `alas/data/coord_seligFmt.zip` through Python's `zipfile`. This crate
//! embeds the same 1,665 entries as one text file, `data/selig.txt`, so that
//! reading the corpus costs no zip-decompression dependency: each entry is
//! stored behind a delimiter line `@<stem>`, followed by that `.dat` file's
//! bytes verbatim -- including the name/header line the parser below skips
//! over, because storing the parsed numbers instead would retire the
//! upstream reader's "skip a line that fails to parse" behaviour to the
//! extraction script rather than translating it into this module.
//!
//! Two properties of the archive make that delimiter safe, and both are
//! checked against the live archive (not assumed) by
//! `golden/generators/gen_geom_selig.py`, which builds this file: every
//! entry is ASCII, and no line in any entry begins with `@`. A third property
//! the generator checks is that no two stems collide once lowercased, which
//! is what lets [`get`] use a single case-folded map with no ordering to
//! reproduce.
//!
//! This module's job stops at resolving a name to its raw `(x, y)` pairs in
//! file order. `AirfoilLibrary.normalize_coordinates`, which reorders those
//! pairs into upper-TE -> LE -> lower-TE, belongs to a later module,
//! `alas-geom::airfoil_library`, and calls into this one rather than being
//! reproduced here.

use std::collections::HashMap;
use std::sync::OnceLock;

// `env!("CARGO_MANIFEST_DIR")` at compile time is what makes this path
// resolve the same way regardless of the caller's working directory, the
// same idiom `alas_i18n::es` uses for its embedded catalog.
const CORPUS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/selig.txt"));

struct Entry {
    /// The zip entry's stem in its original case, e.g. `"naca2410"`.
    stem: &'static str,
    /// The bytes stored after this entry's `@stem` line, up to the next
    /// one or the end of the corpus -- exactly what `data/selig.txt` holds,
    /// including the header line [`parse_coordinates`] skips. Kept mainly
    /// for `tests/parity_selig.rs`, which hashes it against the archive's
    /// digest manifest to prove the corpus has not drifted.
    raw: &'static str,
    coordinates: Vec<(f64, f64)>,
}

fn corpus() -> &'static HashMap<String, Entry> {
    static ENTRIES: OnceLock<HashMap<String, Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| parse_corpus(CORPUS))
}

/// Split the embedded text into entries at each line that starts with `@`,
/// and parse each entry's coordinates once.
///
/// `data/selig.txt` guarantees every entry ends in a newline, so an
/// entry's content runs from immediately after its own `@stem` line to the
/// byte just before the next `@stem` line (or the end of the file) -- a
/// plain substring, which is what keeps this a slice into `CORPUS` rather
/// than a copy.
fn parse_corpus(text: &'static str) -> HashMap<String, Entry> {
    let mut delimiter_starts = Vec::new();
    if text.starts_with('@') {
        delimiter_starts.push(0);
    }
    for (index, _) in text.match_indices("\n@") {
        delimiter_starts.push(index + 1);
    }

    let mut entries = HashMap::with_capacity(delimiter_starts.len());
    for (position, &start) in delimiter_starts.iter().enumerate() {
        let line_end = text[start..]
            .find('\n')
            .map_or(text.len(), |offset| start + offset);
        let stem = &text[start + 1..line_end];
        let content_start = (line_end + 1).min(text.len());
        let content_end = delimiter_starts
            .get(position + 1)
            .copied()
            .unwrap_or(text.len());
        let raw = &text[content_start..content_end];
        entries.insert(
            stem.to_lowercase(),
            Entry {
                stem,
                raw,
                coordinates: parse_coordinates(raw),
            },
        );
    }
    entries
}

/// Reproduce the zip-branch of `AirfoilLibrary.get`: skip the header line,
/// then read each remaining line's first two whitespace-separated fields as
/// floats, silently dropping any line where that fails -- matching Python's
/// `try: ... except ValueError: continue`, which drops the whole line rather
/// than keeping whichever of the two fields did parse.
fn parse_coordinates(raw: &str) -> Vec<(f64, f64)> {
    let mut coordinates = Vec::new();
    for line in raw.lines().skip(1) {
        let mut fields = line.split_whitespace();
        let (Some(x_field), Some(y_field)) = (fields.next(), fields.next()) else {
            continue;
        };
        if let (Ok(x), Ok(y)) = (x_field.parse::<f64>(), y_field.parse::<f64>()) {
            coordinates.push((x, y));
        }
    }
    coordinates
}

/// Look up an entry by name, case-insensitively.
///
/// Returns the stem in its original case and its `(x, y)` coordinate pairs
/// in file order -- not normalized into any particular winding, which is
/// `AirfoilLibrary.normalize_coordinates`'s job in a later module.
pub fn get(name: &str) -> Option<(&'static str, &'static [(f64, f64)])> {
    corpus()
        .get(&name.to_lowercase())
        .map(|entry| (entry.stem, entry.coordinates.as_slice()))
}

/// The stored bytes for `name`'s entry, exactly as `data/selig.txt` holds
/// them: after its `@name` delimiter line, including the header line [`get`]
/// skips. Exists for `tests/parity_selig.rs`, which hashes every entry
/// against the archive's digest manifest; ordinary callers want [`get`].
pub fn raw(name: &str) -> Option<(&'static str, &'static str)> {
    corpus()
        .get(&name.to_lowercase())
        .map(|entry| (entry.stem, entry.raw))
}

/// Every stem the corpus indexes, in its original case, sorted.
///
/// Part of what `AirfoilLibrary.get_available_airfoils` needs: it merges
/// this list with the built-in named sections `alas-geom::airfoil_library`
/// will add.
pub fn stems() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = corpus().values().map(|entry| entry.stem).collect();
    names.sort_unstable();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_is_case_insensitive() {
        let (stem_lower, coords_lower) = get("naca2410").expect("naca2410 is in the corpus");
        let (stem_upper, coords_upper) = get("NACA2410").expect("NACA2410 is in the corpus");
        let (stem_mixed, coords_mixed) = get("Naca2410").expect("Naca2410 is in the corpus");
        assert_eq!(stem_lower, "naca2410");
        assert_eq!(stem_upper, stem_lower);
        assert_eq!(stem_mixed, stem_lower);
        assert_eq!(coords_upper, coords_lower);
        assert_eq!(coords_mixed, coords_lower);
    }

    #[test]
    fn get_returns_none_for_a_name_the_corpus_does_not_have() {
        // The reference archive spells the NACA 0012 stem "n0012"; the
        // canonical name is not in it, and this module reports that rather
        // than guessing -- native aerodynamic model's NACA generator is a later branch of
        // `AirfoilLibrary.get`, not this module's concern.
        assert!(get("naca0012").is_none());
        assert!(get("not-a-real-airfoil-stem").is_none());
    }

    #[test]
    fn a_comment_line_that_fails_float_parsing_is_skipped_not_fatal() {
        // "30p-30n" is a multi-element flap deck whose remaining lines
        // include several "# ..." comments (a slat/flap geometry table) that
        // do not parse as two floats. Those lines drop silently and every
        // numeric line around them still resolves.
        let (_, coords) = get("30p-30n").expect("30p-30n is in the corpus");
        assert_eq!(coords.len(), 664);
    }

    #[test]
    fn an_entry_with_no_trailing_newline_in_the_archive_still_parses_its_last_line() {
        // "e374" is one of 5 archive entries whose last coordinate line has
        // no trailing newline; data/selig.txt terminates it with one so the
        // next entry's `@stem` line starts cleanly, and that terminator must
        // not swallow or duplicate the line it closes.
        let (_, coords) = get("e374").expect("e374 is in the corpus");
        let last = *coords.last().expect("e374 has at least one point");
        assert_eq!(last, (1.0, 0.0));
    }

    #[test]
    fn stems_are_sorted_and_original_case_is_preserved() {
        let names = stems();
        assert_eq!(names.len(), 1665);
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        assert!(names.contains(&"naca2410"));
    }

    #[test]
    fn raw_includes_the_header_line_that_get_skips() {
        let (stem, raw_text) = raw("naca2410").expect("naca2410 is in the corpus");
        assert_eq!(stem, "naca2410");
        assert!(raw_text
            .lines()
            .next()
            .expect("has a header line")
            .contains("2410"));
    }
}
