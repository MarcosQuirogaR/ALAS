// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/sidecar/schema.py (`_humanize`, `_UNIT_SUFFIXES`).
// Reference: alas @ rust-port-baseline.

//! The label and unit a field gets when it does not state its own.
//!
//! Most configuration fields are named after the quantity they hold and the
//! unit it is in (`wing_area_m2`, `cruise_speed_m_s`) so the interface can
//! read both straight off the identifier, and several hundred fields upstream
//! rely on that rather than repeating themselves. Reproducing the rule is not
//! optional: a field whose derived label differs by so much as its
//! capitalization is a different string to look up in the translation catalog,
//! and would silently fall back to English.
//!
//! Both derivations run at expansion time, so a struct carries its labels as
//! ordinary string literals and pays nothing at run time to have them.

/// Recognized unit suffixes, longest-matching-first as upstream orders them.
///
/// The order is load-bearing: `_m_s2` has to be tested before `_m_s`, which
/// has to be tested before `_s`, or a field in metres per second squared
/// would be labelled as one in seconds. The first match wins and the loop
/// stops, exactly as the reference's does.
///
/// The superscripts are written as escapes because source files in this
/// workspace stay ASCII; the strings they produce are the same ones the
/// reference emits, which is what the labels have to match.
const UNIT_SUFFIXES: &[(&str, &str)] = &[
    ("_kg_m2", "kg/m\u{b2}"),
    ("_m_s2", "m/s\u{b2}"),
    ("_m_s", "m/s"),
    ("_m2", "m\u{b2}"),
    ("_deg", "deg"),
    ("_kg", "kg"),
    ("_pa", "Pa"),
    ("_m", "m"),
    ("_s", "s"),
];

/// The label and unit derived from a field's name.
///
/// The unit suffix is stripped from the label when one matches, and what is
/// left becomes sentence case: underscores to spaces, first character upper,
/// the rest lower. That last part is Python's `str.capitalize`, which lowers
/// the tail rather than leaving it alone, so `Max_thickness_LOC` reads
/// "Max thickness loc" here, as it does upstream.
pub fn humanize(name: &str) -> (String, &'static str) {
    let mut unit = "";
    let mut stem = name;
    for (suffix, symbol) in UNIT_SUFFIXES {
        if let Some(without_suffix) = name.strip_suffix(suffix) {
            unit = symbol;
            stem = without_suffix;
            break;
        }
    }

    let spaced = stem.replace('_', " ");
    let trimmed = spaced.trim();
    let mut characters = trimmed.chars();
    let label = match characters.next() {
        None => String::new(),
        Some(first) => {
            first.to_uppercase().collect::<String>() + &characters.as_str().to_lowercase()
        }
    };

    (label, unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_suffix_becomes_the_unit_and_leaves_the_label() {
        assert_eq!(
            humanize("wing_area_m2"),
            ("Wing area".to_owned(), "m\u{b2}")
        );
        assert_eq!(
            humanize("cruise_speed_m_s"),
            ("Cruise speed".to_owned(), "m/s")
        );
        assert_eq!(
            humanize("wing_loading_kg_m2"),
            ("Wing loading".to_owned(), "kg/m\u{b2}")
        );
    }

    #[test]
    fn the_longest_matching_suffix_wins() {
        // `_m_s2` and `_m_s` both end `_s`, and `_m_s2` also ends `_s2`
        // in spirit; testing them in the listed order is what keeps an
        // acceleration from being labelled as a time.
        assert_eq!(humanize("gust_load_m_s2").1, "m/s\u{b2}");
        assert_eq!(humanize("descent_rate_m_s").1, "m/s");
        assert_eq!(humanize("taxi_time_s").1, "s");
    }

    #[test]
    fn a_name_with_no_unit_suffix_keeps_all_of_itself_in_the_label() {
        assert_eq!(
            humanize("interference_factor_wing"),
            ("Interference factor wing".to_owned(), "")
        );
    }

    #[test]
    fn the_label_is_sentence_case_and_lowers_the_tail() {
        // Python's `str.capitalize` lowercases everything after the first
        // character, and a label that differs in case is a different key in
        // the translation catalog.
        assert_eq!(humanize("OPR_target").0, "Opr target");
    }

    #[test]
    fn a_name_that_is_only_a_unit_suffix_yields_an_empty_label() {
        assert_eq!(humanize("_m"), (String::new(), "m"));
    }
}
