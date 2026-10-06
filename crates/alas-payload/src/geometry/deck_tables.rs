// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// Whether a body of this declared height and diameter is a double-decker.
pub(super) fn is_double_deck(height_m: Option<f64>, diameter_m: f64) -> bool {
    height_m.is_some_and(|h| h >= diameter_m * 1.15)
}

/// The passenger decks and the lower hold for a body of this shape.
///
/// A double-decker's main deck sits low and its upper deck high, each taking
/// roughly half the section; a single-deck body's main deck takes nearly all
/// of it and the hold takes the bottom fifth.
pub(super) fn decks(height_m: Option<f64>, diameter_m: f64) -> (Vec<DeckSpec>, DeckSpec) {
    if is_double_deck(height_m, diameter_m) {
        (
            vec![
                DeckSpec {
                    name: crate::layout::MAIN,
                    // Keep a full passenger cabin above the cargo ceiling;
                    // the small upward shift also leaves the lower hold a
                    // realistic ULD bay instead of a one-container-wide slit.
                    floor_frac: -0.20,
                    // A380-class upper floors leave roughly two metres of
                    // clear cabin height for the seat block and overhead bins.
                    ceil_frac: 0.32,
                    width_factor: 0.95,
                    is_passenger: true,
                },
                DeckSpec {
                    name: crate::layout::UPPER,
                    floor_frac: 0.38,
                    // The upper deck follows the crown; the remaining top
                    // shell is the structural/insulation margin, not cabin
                    // floor that can be sold as seats.
                    ceil_frac: 0.95,
                    width_factor: 0.80,
                    is_passenger: true,
                },
            ],
            DeckSpec {
                name: crate::layout::LOWER,
                floor_frac: -0.70,
                ceil_frac: -0.26,
                // Passenger widebody lower holds are arranged as two
                // half-width LD-family positions across the bay in the
                // published A380 loading plans. Keep a structural/rail
                // margin instead of filling the raw fuselage chord; tapered
                // nose/tail stations still reduce this to one position.
                width_factor: 0.72,
                is_passenger: false,
            },
        )
    } else {
        (
            vec![DeckSpec {
                name: crate::layout::MAIN,
                floor_frac: 0.0,
                ceil_frac: 0.95,
                width_factor: 0.97,
                is_passenger: true,
            }],
            DeckSpec {
                name: crate::layout::LOWER,
                floor_frac: -0.71,
                ceil_frac: -0.02 - MIN_DECK_SEPARATION_FRAC,
                // A narrowbody remains one ULD across, while a 5.6 to 6.0 m
                // widebody gets the two-across lower-hold arrangement seen
                // in aircraft cargo plans.
                width_factor: 0.90,
                is_passenger: false,
            },
        )
    }
}

/// The decks of a single-deck body with a partial upper deck under an
/// upper-deck hump (747 type). The upper deck's floor and ceiling are not
/// fractions of the section (see `hump_deck`), so its two fractions are the
/// main-deck convention and are not read.
///
/// - Main deck floor: [`alas_config::HUMP_MAIN_DECK_FLOOR_FRACTION`]
///   (ESTIMATE from the 747-400 ACAP cross-section, see there).
/// - Upper-deck width factor 0.75: the ACAP 747-400 upper-deck sidewall
///   linings are 4.14 m apart at seat height (D6-58326-1 Rev F, section
///   2.5.1, p. 2-28, drawing read) against the 5.5 m wall-inset chord the
///   single-ellipse hump section gives at the upper floor.
/// - Lower hold: the single-deck table's floor, its ceiling under the
///   lowered main floor; 0.48 of the 3.47 m internal half-height leaves
///   1.67 m, the 64 in (1.63 m) LD-1 of the ACAP plus clearance.
pub(super) fn hump_decks() -> (Vec<DeckSpec>, DeckSpec) {
    let main_floor = alas_config::HUMP_MAIN_DECK_FLOOR_FRACTION;
    (
        vec![
            DeckSpec {
                name: crate::layout::MAIN,
                floor_frac: main_floor,
                ceil_frac: 0.95,
                width_factor: 0.97,
                is_passenger: true,
            },
            DeckSpec {
                name: crate::layout::UPPER,
                floor_frac: main_floor,
                ceil_frac: 0.95,
                width_factor: 0.75,
                is_passenger: true,
            },
        ],
        DeckSpec {
            name: crate::layout::LOWER,
            floor_frac: -0.71,
            ceil_frac: main_floor - 0.02 - MIN_DECK_SEPARATION_FRAC,
            width_factor: 0.90,
            is_passenger: false,
        },
    )
}

/// Original Python deck table, retained only for explicit parity evidence.
pub(super) fn reference_decks(height_m: Option<f64>, diameter_m: f64) -> (Vec<DeckSpec>, DeckSpec) {
    if is_double_deck(height_m, diameter_m) {
        (
            vec![
                DeckSpec {
                    name: crate::layout::MAIN,
                    floor_frac: -0.48,
                    ceil_frac: -0.02,
                    width_factor: 0.95,
                    is_passenger: true,
                },
                DeckSpec {
                    name: crate::layout::UPPER,
                    floor_frac: 0.04,
                    ceil_frac: 0.55,
                    width_factor: 0.80,
                    is_passenger: true,
                },
            ],
            DeckSpec {
                name: crate::layout::LOWER,
                floor_frac: -0.95,
                ceil_frac: -0.50,
                width_factor: 0.55,
                is_passenger: false,
            },
        )
    } else {
        (
            vec![DeckSpec {
                name: crate::layout::MAIN,
                floor_frac: -0.18,
                ceil_frac: 0.95,
                width_factor: 0.97,
                is_passenger: true,
            }],
            DeckSpec {
                name: crate::layout::LOWER,
                floor_frac: -0.95,
                ceil_frac: -0.20,
                width_factor: 0.60,
                is_passenger: false,
            },
        )
    }
}
