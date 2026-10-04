// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/cabin_layout.py (`EXIT_TYPES`, `select_exit_type`)

//! The CS 25.807 exit classification, and what a pair of each type may
//! evacuate.
//!
//! A declared exit arrangement names its pairs by type letter only, and the
//! seat ceiling it imposes is the sum of the CS 25.807(g) ratings of those
//! letters. Nothing downstream holds a rating of its own, so a preset cannot
//! state a ceiling the regulation it cites would not give.

use alas_config::{CertifiedExitLayout, PassengerCabinConfig};

/// One FAR/CS-25.807 emergency-exit class: what it may evacuate, and the
/// smallest cutout it may be built at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExitSpec {
    /// The type letter, which is what a deck plan labels the door with.
    pub name: &'static str,
    /// Seats this type is rated for, per complete exit pair.
    ///
    /// The value is already the rating for the two physical exits in one
    /// pair.  It must not be doubled when the pair's two door cut-outs are
    /// emitted.
    pub capacity_per_pair: i64,
    /// Minimum door width, which lies along the fuselage x-axis in plan.
    pub width_m: f64,
    /// Minimum door height.
    pub height_m: f64,
}

/// The FAR/CS-25.807(g) exit classification.
pub static EXIT_TYPES: [ExitSpec; 7] = [
    ExitSpec {
        name: "A",
        capacity_per_pair: 110,
        width_m: 1.07,
        height_m: 1.83,
    },
    ExitSpec {
        name: "B",
        capacity_per_pair: 75,
        width_m: 0.81,
        height_m: 1.83,
    },
    ExitSpec {
        name: "C",
        capacity_per_pair: 55,
        width_m: 0.76,
        height_m: 1.22,
    },
    ExitSpec {
        name: "I",
        capacity_per_pair: 45,
        width_m: 0.61,
        height_m: 1.22,
    },
    ExitSpec {
        name: "II",
        capacity_per_pair: 40,
        width_m: 0.51,
        height_m: 1.12,
    },
    ExitSpec {
        name: "III",
        capacity_per_pair: 35,
        width_m: 0.51,
        height_m: 0.91,
    },
    ExitSpec {
        name: "IV",
        capacity_per_pair: 9,
        width_m: 0.48,
        height_m: 0.66,
    },
];

/// Door pairs no deck exceeds, whatever its length: no transport aircraft
/// carries more per deck, and the cap is what stops a long body being given a
/// door every spacing interval.
pub(crate) const MAX_EXIT_PAIRS_PER_DECK: i64 = 6;

/// A deck holding more than this many passengers needs two exit pairs rather
/// than one, independently of what the type rating alone would allow.
const TWO_PAIR_THRESHOLD: i64 = 110;

/// The CS 25.807 classification of a type letter.
pub fn exit_spec(letter: &str) -> Option<&'static ExitSpec> {
    EXIT_TYPES.iter().find(|spec| spec.name == letter)
}

/// The CS 25.807(g) rating of a complete pair of `letter` exits, or zero for
/// a letter the regulation does not define: an exit nobody can classify
/// evacuates nobody.
pub fn pair_rating(letter: &str) -> i64 {
    exit_spec(letter).map_or(0, |spec| spec.capacity_per_pair)
}

/// The seat ceiling of a declared arrangement: the sum of its pair ratings.
pub fn layout_rating(layout: &CertifiedExitLayout) -> i64 {
    layout
        .pairs
        .iter()
        .map(|pair| pair_rating(pair.exit_type))
        .sum()
}

/// The highest pair rating of a declared arrangement, at least one so it can
/// divide a passenger count.
pub(crate) fn largest_pair_rating(layout: &CertifiedExitLayout) -> i64 {
    layout
        .pairs
        .iter()
        .map(|pair| pair_rating(pair.exit_type))
        .max()
        .unwrap_or(1)
        .max(1)
}

/// A representative exit type for a body of this diameter: a widebody
/// floor-level door, a narrow or widebody floor-level one, or a small
/// narrowbody overwing hatch.
pub fn select_exit_type(diameter_m: f64) -> &'static ExitSpec {
    let letter = if diameter_m >= 5.0 {
        "A"
    } else if diameter_m >= 3.6 {
        "C"
    } else {
        "III"
    };
    // The three letters are literals of this function's own writing, so the
    // search cannot miss; the fallback keeps the signature infallible.
    exit_spec(letter).unwrap_or(&EXIT_TYPES[0])
}

/// Effective capacity for one pair on the generic product path.
///
/// The serialized `exit_capacity_realism_factor` predates the pair-unit
/// correction and stores half of the intended Type-A utilization (`0.478`).
/// Doubling that legacy field here makes the conversion explicit and keeps the
/// resulting utilization bounded at one.  Smaller exit classes use their
/// complete-pair table rating directly.
pub(crate) fn effective_pair_capacity(spec: &ExitSpec, pax: &PassengerCabinConfig) -> i64 {
    if spec.name != "A" {
        return spec.capacity_per_pair.max(0);
    }
    let legacy_factor = if pax.exit_capacity_realism_factor.is_finite() {
        pax.exit_capacity_realism_factor.max(0.0)
    } else {
        0.0
    };
    let pair_utilization = (2.0 * legacy_factor).clamp(0.0, 1.0);
    (spec.capacity_per_pair as f64 * pair_utilization).floor() as i64
}

/// Whether a deck holding this many passengers needs a second exit pair
/// regardless of what one pair is rated for.
pub(crate) fn min_exit_pairs(deck_pax: i64) -> i64 {
    if deck_pax > TWO_PAIR_THRESHOLD {
        2
    } else {
        1
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{presets, CertifiedExitPair};

    #[test]
    fn the_exit_type_follows_the_fuselage_diameter() {
        assert_eq!(select_exit_type(5.9).name, "A");
        assert_eq!(select_exit_type(4.0).name, "C");
        assert_eq!(select_exit_type(3.0).name, "III");
        // The boundaries themselves belong to the larger type.
        assert_eq!(select_exit_type(5.0).name, "A");
        assert_eq!(select_exit_type(3.6).name, "C");
    }

    #[test]
    fn pair_ratings_are_the_cs_25_807_g_table() {
        // CS 25.807(g)(1)-(7): Type A 110, B 75, C 55, I 45, II 40, III 35
        // per pair; Type IV 9.
        for (letter, rating) in [
            ("A", 110),
            ("B", 75),
            ("C", 55),
            ("I", 45),
            ("II", 40),
            ("III", 35),
            ("IV", 9),
        ] {
            assert_eq!(pair_rating(letter), rating, "Type {letter}");
        }
        assert_eq!(pair_rating("Z"), 0);
    }

    #[test]
    fn larger_exits_are_rated_for_more_and_need_more_opening() {
        // The table is ordered by type; a type that evacuates more people
        // never has a smaller minimum opening than one that evacuates fewer.
        for pair in EXIT_TYPES.windows(2) {
            assert!(pair[0].capacity_per_pair > pair[1].capacity_per_pair);
            assert!(pair[0].width_m >= pair[1].width_m);
            assert!(pair[0].height_m >= pair[1].height_m);
        }
    }

    #[test]
    fn adding_an_exit_pair_never_lowers_the_ceiling() {
        static ONE: [CertifiedExitPair; 1] = [CertifiedExitPair {
            exit_type: "C",
            station_m: None,
        }];
        static TWO: [CertifiedExitPair; 2] = [
            CertifiedExitPair {
                exit_type: "C",
                station_m: None,
            },
            CertifiedExitPair {
                exit_type: "III",
                station_m: None,
            },
        ];
        let layout = |pairs: &'static [CertifiedExitPair]| CertifiedExitLayout {
            label: "",
            pairs,
            station_body_length_m: None,
            source: "",
        };
        assert_eq!(layout_rating(&layout(&ONE)), 55);
        assert_eq!(layout_rating(&layout(&TWO)), 90);
        assert!(layout_rating(&layout(&TWO)) > layout_rating(&layout(&ONE)));
    }

    #[test]
    fn every_registered_exit_letter_is_a_cs_25_807_type_and_covers_the_certified_maximum() {
        for name in presets::available() {
            let preset = presets::get(name).unwrap();
            let Some(layout) = preset.reference.certified_exit_layout else {
                continue;
            };
            for pair in layout.pairs {
                assert!(
                    exit_spec(pair.exit_type).is_some(),
                    "{name}: exit letter {} is not a CS 25.807 type",
                    pair.exit_type
                );
            }
            // A certified maximum above what the declared exits are rated
            // for would be an arrangement that could not be certified.
            if let Some(certified) = preset.reference.certified_max_seats {
                assert!(
                    layout_rating(&layout) >= certified,
                    "{name}: exits rated {} below the certified {certified}",
                    layout_rating(&layout)
                );
            }
        }
    }

    #[test]
    fn the_narrowbody_exit_sums_are_their_certified_maxima() {
        // EASA TCDS: A220-300 baseline C-III-C 145; A320-200 C-III-III-C 180.
        let a220 = presets::get("A220-300").unwrap();
        let a320 = presets::get("A320-200").unwrap();
        assert_eq!(
            layout_rating(&a220.reference.certified_exit_layout.unwrap()),
            145
        );
        assert_eq!(
            layout_rating(&a320.reference.certified_exit_layout.unwrap()),
            180
        );
    }
}
