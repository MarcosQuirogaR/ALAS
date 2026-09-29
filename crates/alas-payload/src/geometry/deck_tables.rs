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
