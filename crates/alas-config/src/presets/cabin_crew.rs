// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Minimum cabin crew of a seated cabin, deck by deck.
//!
//! EASA Air Operations ORO.CC.100(b): the minimum number of cabin crew is the
//! greater of the number established at type certification for the cabin
//! configuration used, and one cabin crew member for every 50, or fraction
//! of 50, passenger seats installed on the same deck. The established
//! numbers below are the type-certificate data sheet tables, per deck, for
//! the exit arrangement each preset declares and without optional
//! modifications: a row that needs a modification the preset does not carry
//! is left out. A cabin with fewer seats than a row is inside that row's
//! certified configuration, so it takes the smallest row that covers it; the
//! EASA cabin crew FAQ allows a lower number only through a supplemental
//! type certificate.

/// The largest maximum operational passenger seating configuration flown
/// without cabin crew: ORO.CC.100(a) assigns at least one above 19.
pub const MAX_SEATS_WITHOUT_CABIN_CREW: usize = 19;

/// The deck a type-certificate crew row applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrewDeck {
    /// The main passenger deck.
    Main,
    /// The upper passenger deck of a double-deck body.
    Upper,
}

/// One more cabin crew member a row asks for when more than `seats` installed
/// seats on its deck stand forward of the door at `station_m`, m aft of the
/// nose tip of the source body.
#[derive(Debug, Clone, Copy)]
struct ForwardDoorCrew {
    station_m: f64,
    seats: usize,
}

/// One type-certificate row: up to `max_seats` passengers on `deck`, `crew`
/// cabin crew members, and the extra member a door may ask for.
#[derive(Debug, Clone, Copy)]
struct CrewRow {
    deck: CrewDeck,
    max_seats: usize,
    crew: usize,
    forward_door: Option<ForwardDoorCrew>,
}

const fn row(deck: CrewDeck, max_seats: usize, crew: usize) -> CrewRow {
    CrewRow {
        deck,
        max_seats,
        crew,
        forward_door: None,
    }
}

const fn with_forward_door(row: CrewRow, station_m: f64, seats: usize) -> CrewRow {
    CrewRow {
        forward_door: Some(ForwardDoorCrew { station_m, seats }),
        ..row
    }
}

/// The type-certificate cabin crew rows of a registered preset, ascending in
/// seats on each deck; empty where none is declared.
fn certified_rows(preset: &str) -> &'static [CrewRow] {
    use CrewDeck::{Main, Upper};
    match preset {
        // EASA TCDS A.064 Issue 62, minimum cabin crew: 180 passengers on the
        // basic C-III-III-C arrangement, four. The 150-seat row with three
        // needs modification 150364, which the preset does not declare.
        "A320-200" => const { &[row(Main, 180, 4)] },
        // EASA TCDS A.015 Issue 28 item 18: 375 passengers on the basic
        // A-A-I-A arrangement, eight. The A-A-A-A rows are another
        // arrangement, the 440-seat one needs modification 40161.
        "A340-300" => const { &[row(Main, 375, 8)] },
        // EASA TCDS A.151 III.2.4: eight, nine above 400 passengers.
        "A350-900" => const { &[row(Main, 400, 8), row(Main, 440, 9)] },
        // EASA TCDS A.110 Issue 17 item 3.9: upper deck six up to 300
        // passengers, seven up to 330; main deck ten up to 500, eleven up to
        // 538. The six take one more at the forward stair when more than 30
        // installed seats stand forward of door U1, 20.94 m aft of the nose
        // (Airbus A380 Aircraft Characteristics, FIGURE-2-7-0-991-002-A01).
        "A380-800" => {
            const {
                &[
                    with_forward_door(row(Upper, 300, 6), 20.94, 30),
                    row(Upper, 330, 7),
                    row(Main, 500, 10),
                    row(Main, 538, 11),
                ]
            }
        }
        // EASA TCDS IM.A.115 Issue 30, 787-9 on the A-A-A-A arrangement:
        // eight up to 400 passengers, nine up to 420.
        "B787-9" => const { &[row(Main, 400, 8), row(Main, 420, 9)] },
        // EASA TCDS IM.A.196 Issue 19 section 3 item 19, 747-400: main deck
        // eleven up to 550 passengers; upper deck on Type A exits two up to
        // 100, three up to 110.
        "B747-400" => const { &[row(Main, 550, 11), row(Upper, 100, 2), row(Upper, 110, 3)] },
        _ => &[],
    }
}

/// The cabin crew one deck needs for `seats` installed seats, of which
/// `seats_forward_of(station)` stand forward of a station.
fn deck_minimum(
    rows: &[CrewRow],
    deck: CrewDeck,
    seats: usize,
    seats_forward_of: &dyn Fn(CrewDeck, f64) -> usize,
) -> usize {
    if seats == 0 {
        return 0;
    }
    let per_fifty = seats.div_ceil(50);
    let mut deck_rows = rows.iter().filter(|r| r.deck == deck);
    let covering = deck_rows.clone().find(|r| r.max_seats >= seats);
    // Above every row the cabin is outside the certified configurations;
    // keep the largest established number rather than drop below it.
    let established = covering.or_else(|| deck_rows.next_back()).map_or(0, |r| {
        let door = r
            .forward_door
            .filter(|door| seats_forward_of(deck, door.station_m) > door.seats);
        r.crew + usize::from(door.is_some())
    });
    established.max(per_fifty)
}

/// Minimum cabin crew for a cabin of `main_deck_seats` and
/// `upper_deck_seats` installed seats on the registered `preset` (any name
/// without rows takes the one-per-50-seats rule deck by deck), with no seat
/// counted forward of a door a row names.
pub fn minimum_cabin_crew(preset: &str, main_deck_seats: usize, upper_deck_seats: usize) -> usize {
    minimum_cabin_crew_with_doors(preset, main_deck_seats, upper_deck_seats, &|_, _| 0)
}

/// [`minimum_cabin_crew`] where `seats_forward_of(deck, station_m)` counts
/// the installed seats on `deck` forward of `station_m` (m aft of the nose
/// tip of the source body), for a row that adds a member at a door.
pub fn minimum_cabin_crew_with_doors(
    preset: &str,
    main_deck_seats: usize,
    upper_deck_seats: usize,
    seats_forward_of: &dyn Fn(CrewDeck, f64) -> usize,
) -> usize {
    let rows = certified_rows(preset);
    deck_minimum(rows, CrewDeck::Main, main_deck_seats, seats_forward_of)
        + deck_minimum(rows, CrewDeck::Upper, upper_deck_seats, seats_forward_of)
}

#[cfg(test)]
mod tests {
    use super::minimum_cabin_crew;

    #[test]
    fn each_deck_takes_its_own_type_certificate_row() {
        // A380: 199 upper (six) and 356 main (ten), where one per 50 seats
        // over the whole aircraft gives twelve.
        assert_eq!(minimum_cabin_crew("A380-800", 356, 199), 16);
        assert_eq!(minimum_cabin_crew("A380-800", 520, 320), 18);
    }

    #[test]
    fn the_a380_upper_deck_takes_one_more_at_the_stair_above_30_seats_forward_of_u1() {
        use super::{minimum_cabin_crew_with_doors, CrewDeck};
        let forward = |count: usize| {
            move |deck: CrewDeck, station: f64| {
                assert!((station - 20.94).abs() < 1e-9);
                if deck == CrewDeck::Upper {
                    count
                } else {
                    0
                }
            }
        };
        assert_eq!(
            minimum_cabin_crew_with_doors("A380-800", 356, 199, &forward(30)),
            16
        );
        assert_eq!(
            minimum_cabin_crew_with_doors("A380-800", 356, 199, &forward(31)),
            17
        );
        // The seven of the 301-330 row carry no asterisk.
        assert_eq!(
            minimum_cabin_crew_with_doors("A380-800", 356, 320, &forward(60)),
            17
        );
    }

    #[test]
    fn a_smaller_cabin_keeps_the_crew_of_the_row_that_covers_it() {
        assert_eq!(minimum_cabin_crew("A340-300", 335, 0), 8);
        assert_eq!(minimum_cabin_crew("A320-200", 150, 0), 4);
        assert_eq!(minimum_cabin_crew("B787-9", 290, 0), 8);
        assert_eq!(minimum_cabin_crew("A350-900", 410, 0), 9);
    }

    #[test]
    fn without_rows_each_deck_takes_one_per_fifty_seats() {
        assert_eq!(minimum_cabin_crew("E195-E2", 132, 0), 3);
        // 60 + 60 seats: two per deck, where the whole-aircraft count is three.
        assert_eq!(minimum_cabin_crew("clean-sheet", 60, 60), 4);
        assert_eq!(minimum_cabin_crew("clean-sheet", 0, 0), 0);
    }
}
