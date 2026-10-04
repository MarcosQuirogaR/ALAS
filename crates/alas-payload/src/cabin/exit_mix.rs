// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The exit arrangement a clean-sheet cabin needs for its seat count.
//!
//! A registered aircraft declares its exits. A clean-sheet brief has none to
//! declare, so its arrangement is derived from the passenger-seat allowance
//! per exit of each type installed in each side of the fuselage in the
//! lead-in of CS-25 25.807(g) (EASA CS-25 Amendment 27; the same table as
//! 14 CFR 25.807(g) as amended by Amendment 25-114), the table
//! [`super::EXIT_TYPES`] carries: Type A 110, B 75, C 55, I 45, II 40 and
//! III 35 seats per pair.
//!
//! The arrangement follows transport practice: floor-level door pairs of one
//! type, forward and aft (at least two pairs, which also meets 25.807(g)(5)
//! and (g)(7)), plus up to two pairs of Type III overwing exits. The door
//! type follows the body: Type A from a 5.0 m diameter (twin-aisle), Type C
//! from 3.6 m, Type I below, the diameter classes the generic proxy already
//! used. Under 25.807(g)(6) the Type III exits together allow at most 70
//! seats, and two Type III exits in each side separated by fewer than three
//! seat rows at most 65; a derived arrangement has no exit stations, so two
//! overwing pairs are taken as adjacent and counted at 65. Above 299 seats no
//! Type III side exit is used (25.807(g)(9) requires larger side exits for
//! high-capacity cabins). Of the arrangements whose allowance covers the
//! seats, the one with the fewest pairs is chosen, then the one with fewer
//! door pairs, then the smallest allowance: an A320-class 3.96 m body with
//! 168 seats gets C-III-III-C (175).
//!
//! The derivation is a regulatory allowance, not an evacuation
//! demonstration (25.803), and no per-aircraft value enters it.

use std::sync::OnceLock;

use alas_config::{CertifiedExitLayout, CertifiedExitPair};

use super::exit_rules::{pair_rating, MAX_EXIT_PAIRS_PER_DECK};

/// Overwing Type III pairs a derived arrangement may carry.
const MAX_OVERWING_PAIRS: usize = 2;

/// Fewest floor-level door pairs: one forward and one aft.
const MIN_DOOR_PAIRS: usize = 2;

/// Largest seating configuration that may still use Type III side exits
/// (CS 25.807(g)(9)).
const MAX_SEATS_WITH_TYPE_III: i64 = 299;

/// Combined seat allowance of all Type III exits (CS 25.807(g)(6)).
const TYPE_III_COMBINED_CAP: i64 = 70;

/// Combined allowance of two Type III exits in each side separated by fewer
/// than three seat rows (CS 25.807(g)(6)), assumed for overwing pairs whose
/// stations are not known.
const ADJACENT_TYPE_III_CAP: i64 = 65;

const SOURCE: &str = "Derived from EASA CS-25 Amdt 27, 25.807(g) passenger-seat allowance per exit (smallest compliant door plus overwing Type III mix; Type III combined allowance per 25.807(g)(6), overwing pairs taken as adjacent)";

/// Door types a derived arrangement may use, largest body first.
const DOOR_TYPES: [&str; 3] = ["A", "C", "I"];

/// The floor-level door type for a body of `diameter_m`.
pub fn door_type_for_diameter(diameter_m: f64) -> &'static str {
    if diameter_m >= 5.0 {
        "A"
    } else if diameter_m >= 3.6 {
        "C"
    } else {
        "I"
    }
}

struct Arrangement {
    door: &'static str,
    doors: usize,
    overwing: usize,
    label: String,
    pairs: Vec<CertifiedExitPair>,
}

impl Arrangement {
    fn new(door: &'static str, doors: usize, overwing: usize) -> Self {
        // Forward doors, the overwing pairs, then the aft doors.
        let forward = doors.div_ceil(2);
        let mut letters: Vec<&'static str> = vec![door; forward];
        letters.extend(std::iter::repeat_n("III", overwing));
        letters.extend(std::iter::repeat_n(door, doors - forward));
        Self {
            door,
            doors,
            overwing,
            label: letters.join("-"),
            pairs: letters
                .into_iter()
                .map(|exit_type| CertifiedExitPair {
                    exit_type,
                    station_m: None,
                })
                .collect(),
        }
    }

    fn allowance(&self) -> i64 {
        let doors: i64 = self.doors as i64 * pair_rating(self.door);
        let type_iii = self.overwing as i64 * pair_rating("III");
        let cap = if self.overwing >= 2 {
            ADJACENT_TYPE_III_CAP
        } else {
            TYPE_III_COMBINED_CAP
        };
        doors + type_iii.min(cap)
    }

    fn layout(&'static self) -> CertifiedExitLayout {
        CertifiedExitLayout {
            label: &self.label,
            pairs: &self.pairs,
            station_body_length_m: None,
            source: SOURCE,
        }
    }
}

/// Every arrangement a derivation may choose, built once so the layouts it
/// returns can borrow their pair lists for the life of the program.
fn arrangements() -> &'static [Arrangement] {
    static TABLE: OnceLock<Vec<Arrangement>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let max_doors = usize::try_from(MAX_EXIT_PAIRS_PER_DECK).unwrap_or(MIN_DOOR_PAIRS);
        DOOR_TYPES
            .iter()
            .flat_map(|&door| {
                (MIN_DOOR_PAIRS..=max_doors).flat_map(move |doors| {
                    (0..=MAX_OVERWING_PAIRS).map(move |overwing| (door, doors, overwing))
                })
            })
            .map(|(door, doors, overwing)| Arrangement::new(door, doors, overwing))
            .collect()
    })
}

/// The smallest CS 25.807(g) compliant arrangement for `seats` passengers on
/// a body of `diameter_m`, or `None` when no arrangement within the deck's
/// door limit covers them (or the inputs are not physical).
pub fn derived_exit_layout(diameter_m: f64, seats: i64) -> Option<CertifiedExitLayout> {
    if !(diameter_m.is_finite() && diameter_m > 0.0) || seats <= 0 {
        return None;
    }
    let door = door_type_for_diameter(diameter_m);
    let overwing_allowed = seats <= MAX_SEATS_WITH_TYPE_III;
    arrangements()
        .iter()
        .filter(|a| a.door == door && (overwing_allowed || a.overwing == 0))
        .filter(|a| a.allowance() >= seats)
        .min_by_key(|a| (a.doors + a.overwing, a.doors, a.allowance()))
        .map(Arrangement::layout)
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cabin::layout_rating;

    /// The CS 25.807(g) allowance of a derived layout, Type III cap included.
    fn allowance(layout: &CertifiedExitLayout) -> i64 {
        arrangements()
            .iter()
            .find(|a| a.label == layout.label)
            .map(Arrangement::allowance)
            .unwrap()
    }

    #[test]
    fn an_a320_class_body_gets_two_doors_and_two_adjacent_overwing_pairs() {
        // 55 + 55 doors and two adjacent Type III pairs capped at 65
        // (25.807(g)(6)) allow 175 seats, five fewer than the uncapped 180.
        for seats in [168, 175] {
            let layout = derived_exit_layout(3.96, seats).unwrap();
            assert_eq!(layout.label, "C-III-III-C", "{seats}");
            assert_eq!(allowance(&layout), 175);
            assert_eq!(layout_rating(&layout), 180);
        }
        // One seat over trades an overwing pair for a third door pair.
        let more = derived_exit_layout(3.96, 176).unwrap();
        assert_eq!(more.label, "C-C-III-C");
        assert_eq!(allowance(&more), 200);
    }

    #[test]
    fn the_type_iii_exits_never_allow_more_than_the_combined_cap() {
        for a in arrangements() {
            let type_iii = a.overwing as i64 * pair_rating("III");
            let doors = a.doors as i64 * pair_rating(a.door);
            assert!(a.allowance() - doors <= TYPE_III_COMBINED_CAP);
            assert!(a.allowance() - doors <= type_iii);
        }
    }

    #[test]
    fn the_allowance_always_covers_the_seats_with_the_fewest_pairs() {
        for diameter in [3.0, 3.96, 6.2] {
            for seats in 1..=600 {
                let Some(layout) = derived_exit_layout(diameter, seats) else {
                    continue;
                };
                assert!(allowance(&layout) >= seats, "{diameter} m, {seats}");
                assert!(layout.pairs.len() >= MIN_DOOR_PAIRS);
                // No admissible arrangement with fewer pairs covers the seats.
                let door = door_type_for_diameter(diameter);
                let fewest = arrangements()
                    .iter()
                    .filter(|a| a.door == door)
                    .filter(|a| seats <= MAX_SEATS_WITH_TYPE_III || a.overwing == 0)
                    .filter(|a| a.allowance() >= seats)
                    .map(|a| a.pairs.len())
                    .min()
                    .unwrap();
                assert_eq!(fewest, layout.pairs.len(), "{diameter} m, {seats}");
            }
        }
    }

    #[test]
    fn small_bodies_use_type_i_doors_and_large_seat_counts_drop_type_iii() {
        let regional = derived_exit_layout(3.01, 146).unwrap();
        assert_eq!(regional.label, "I-III-III-I");
        assert_eq!(allowance(&regional), 155);
        let widebody = derived_exit_layout(6.2, 350).unwrap();
        assert!(widebody.pairs.iter().all(|pair| pair.exit_type == "A"));
        assert!(allowance(&widebody) >= 350);
    }

    #[test]
    fn unphysical_inputs_and_unreachable_counts_derive_nothing() {
        assert!(derived_exit_layout(f64::NAN, 100).is_none());
        assert!(derived_exit_layout(3.96, 0).is_none());
        // Above 299 seats only door pairs count: six Type I pairs allow 270.
        assert!(derived_exit_layout(3.0, 1_000).is_none());
    }
}
