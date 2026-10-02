// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Seat rows between declared doors.
//!
//! On a main deck bounded by its doors (`stations`) the seats are
//! walked aft from the forward monument bay, jumping every door cross-aisle
//! and door-side monument bay a row would overlap, with a monument bay at
//! each class boundary. The auto-sizer and the row packer both walk the same
//! floor with this one function, which is what keeps a preset's seat count
//! and its detailed layout in agreement.
//!
//! # Monument bays
//!
//! A cabin carries the galleys and lavatories its passenger count provisions
//! (`fittings::provisioned_monuments`), standing side by side across a bay
//! with the aisles left open. The bays at either end and at each class
//! boundary come with the cabin; when they cannot hold every monument, the
//! remaining bays are charged against the seats at the doors between the
//! first and the last. No length is reserved beyond those bays: this replaces
//! the calibrated service reserve of the generic cabin.

use alas_config::{PassengerCabinConfig, SeatClassConfig};

use super::fittings::{monuments_per_bay, provisioned_monuments};
use super::seating::{seat_row_item, CabinClass};
use super::stations::{clear_of, obstacle_length_within, seat_obstacles, MONUMENT_BAY_LENGTH_M};
use super::{abreast_and_aisles, ceil_div, Bay, MIN_PITCH};
use crate::geometry::{CabinGeometry, DeckSpec};
use crate::layout::DeckItem;
use crate::numeric::round_half_even;

/// Bisection steps of the pitch stretch; 2^-30 of the stretch range is far
/// below a millimetre of row placement.
const STRETCH_ITERATIONS: usize = 30;

/// Slack on the aft end of the seat region, m, so a row ending exactly on it
/// is not lost to rounding.
const END_EPSILON_M: f64 = 1e-9;

/// What one class asks of the walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Demand {
    /// Place this many seats.
    Seats(i64),
    /// Place this many rows.
    Rows(i64),
    /// Place rows until the deck is full.
    Fill,
}

/// One class as the walk reads it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DeclaredClass<'a> {
    /// Its seat geometry.
    pub config: &'a SeatClassConfig,
    /// What it asks for.
    pub demand: Demand,
}

/// One row the walk placed.
#[derive(Debug, Clone, Copy)]
struct WalkRow {
    class: usize,
    x0: f64,
    pitch: f64,
    seats: i64,
    abreast: i64,
    aisles: i64,
}

/// The rows of one walk and the class-boundary bays between them.
struct Walk {
    rows: Vec<WalkRow>,
    class_bays: Vec<f64>,
    seated: i64,
    /// Whether every class got what it asked for before the deck ended.
    complete: bool,
}

/// A declared main deck: its seat region, what blocks it, and its ceiling.
struct DeclaredDeck<'a> {
    g: &'a CabinGeometry,
    deck: &'a DeckSpec,
    start: f64,
    end: f64,
    obstacles: Vec<(f64, f64)>,
    door_bays: Vec<f64>,
    aisle_w: f64,
    cap: i64,
}

impl<'a> DeclaredDeck<'a> {
    /// The deck between `x0` and `x1` (the main-deck segment, end bays
    /// included) with `extra_bays` monument bays at its doors.
    #[allow(clippy::too_many_arguments)] // The deck's own frame and limits.
    fn new(
        g: &'a CabinGeometry,
        deck: &'a DeckSpec,
        x0: f64,
        x1: f64,
        extra_bays: i64,
        aisle_w: f64,
        cap: i64,
    ) -> Self {
        let (obstacles, door_bays) = seat_obstacles(&g.door_stations, extra_bays);
        Self {
            g,
            deck,
            start: x0 + MONUMENT_BAY_LENGTH_M,
            end: x1 - MONUMENT_BAY_LENGTH_M,
            obstacles,
            door_bays,
            aisle_w,
            cap,
        }
    }

    /// Floor left for seat rows once the doors, their bays and `class_bays`
    /// class-boundary bays are taken out.
    fn seat_length(&self, class_bays: usize) -> f64 {
        (self.end
            - self.start
            - obstacle_length_within(&self.obstacles, self.start, self.end)
            - class_bays as f64 * MONUMENT_BAY_LENGTH_M)
            .max(0.0)
    }

    /// Walk the classes aft at `stretch` times their pitch.
    fn walk(&self, classes: &[DeclaredClass<'_>], stretch: f64) -> Walk {
        let mut walk = Walk {
            rows: Vec::new(),
            class_bays: Vec::new(),
            seated: 0,
            complete: true,
        };
        let mut x = self.start;
        'classes: for (index, class) in classes.iter().enumerate() {
            if index > 0 {
                x = clear_of(&self.obstacles, x, MONUMENT_BAY_LENGTH_M);
                walk.class_bays.push(x + 0.5 * MONUMENT_BAY_LENGTH_M);
                x += MONUMENT_BAY_LENGTH_M;
            }
            let pitch = class.config.pitch_m.max(MIN_PITCH) * stretch;
            let (mut seats_left, mut rows_left) = match class.demand {
                Demand::Seats(seats) => (seats.max(0), i64::MAX),
                Demand::Rows(rows) => (i64::MAX, rows.max(0)),
                Demand::Fill => (i64::MAX, i64::MAX),
            };
            while seats_left > 0 && rows_left > 0 {
                if walk.seated >= self.cap {
                    walk.complete = matches!(class.demand, Demand::Fill);
                    break 'classes;
                }
                x = clear_of(&self.obstacles, x, pitch);
                if x + pitch > self.end + END_EPSILON_M {
                    walk.complete = matches!(class.demand, Demand::Fill);
                    break 'classes;
                }
                let (abreast, aisles) =
                    abreast_and_aisles(class.config, self.deck, self.g, self.aisle_w, x);
                let seats = abreast.min(seats_left).min(self.cap - walk.seated);
                walk.rows.push(WalkRow {
                    class: index,
                    x0: x,
                    pitch,
                    seats,
                    abreast,
                    aisles,
                });
                walk.seated += seats;
                seats_left -= seats;
                rows_left -= 1;
                x += pitch;
            }
        }
        walk
    }

    /// The walk at the largest pitch stretch that still places every class,
    /// so a cabin seating fewer than the floor holds spreads its rows over
    /// the whole deck rather than bunching them forward. A walk that cannot
    /// place everything at the declared pitch is returned unstretched.
    fn walk_stretched(&self, classes: &[DeclaredClass<'_>]) -> Walk {
        let base = self.walk(classes, 1.0);
        if !base.complete {
            return base;
        }
        let used: f64 = base.rows.iter().map(|row| row.pitch).sum();
        let room = self.seat_length(classes.len().saturating_sub(1));
        if used <= 0.0 || room <= used {
            return base;
        }
        let (mut lo, mut hi) = (1.0, room / used);
        let mut best = base;
        for _ in 0..STRETCH_ITERATIONS {
            let mid = 0.5 * (lo + hi);
            let trial = self.walk(classes, mid);
            if trial.complete {
                lo = mid;
                best = trial;
            } else {
                hi = mid;
            }
        }
        best
    }
}

/// Monument bays a cabin of `seated` passengers needs beyond the two end bays
/// and its `class_bays` class-boundary bays.
#[allow(clippy::too_many_arguments)] // The provisioning and the bay width.
fn extra_monument_bays(
    g: &CabinGeometry,
    deck: &DeckSpec,
    pax: &PassengerCabinConfig,
    seated: i64,
    class_bays: usize,
    aisle_w: f64,
    n_aisles: i64,
) -> i64 {
    let (galleys, lavatories) = provisioned_monuments(pax, seated);
    let x_mid = g.door_stations.iter().map(|door| door.x).sum::<f64>()
        / g.door_stations.len().max(1) as f64;
    let per_bay = monuments_per_bay(g.usable_width(deck, x_mid), n_aisles, aisle_w);
    let needed = ceil_div(galleys.max(0) + lavatories.max(0), per_bay);
    (needed - 2 - class_bays as i64).max(0)
}

/// Aisles of the deck's widest class at mid-cabin: what a monument bay has to
/// leave open.
fn deck_aisles(
    g: &CabinGeometry,
    deck: &DeckSpec,
    classes: &[DeclaredClass<'_>],
    aisle_w: f64,
    x: f64,
) -> i64 {
    classes
        .iter()
        .map(|class| abreast_and_aisles(class.config, deck, g, aisle_w, x).1)
        .max()
        .unwrap_or(1)
}

/// Seats per class a declared main deck holds under a floor-length class
/// mix, as the auto-sizer counts them.
///
/// Each class but the last gets its share of the seat floor rounded to whole
/// rows, and the last fills what is left, which is the generic counter's
/// rule (`crate::build::count_deck`). The monument bays depend on the count
/// they serve, so the deck is walked once without door bays and again with
/// the bays that count provisions.
#[allow(clippy::too_many_arguments)] // The deck, its class mix and limits.
pub(crate) fn count_declared_deck(
    g: &CabinGeometry,
    deck: &DeckSpec,
    x0: f64,
    x1: f64,
    pax: &PassengerCabinConfig,
    classes: &[(&'static str, &SeatClassConfig, f64)],
    aisle_w: f64,
    cap: i64,
) -> Vec<(&'static str, i64)> {
    let class_bays = classes.len().saturating_sub(1);
    let pass = |extra_bays: i64| {
        let declared_deck = DeclaredDeck::new(g, deck, x0, x1, extra_bays, aisle_w, cap);
        let l_seating = declared_deck.seat_length(class_bays);
        let declared: Vec<DeclaredClass<'_>> = classes
            .iter()
            .enumerate()
            .map(|(index, &(_, config, share))| {
                let demand = if index + 1 == classes.len() {
                    Demand::Fill
                } else {
                    let pitch = config.pitch_m.max(MIN_PITCH);
                    Demand::Rows((round_half_even(share * l_seating / pitch) as i64).max(0))
                };
                DeclaredClass { config, demand }
            })
            .collect();
        let walk = declared_deck.walk(&declared, 1.0);
        let mut seats: Vec<(&'static str, i64)> =
            classes.iter().map(|&(name, _, _)| (name, 0)).collect();
        for row in &walk.rows {
            seats[row.class].1 += row.seats;
        }
        (seats, walk.seated, declared)
    };
    let (first, seated, declared) = pass(0);
    let x_mid = 0.5 * (x0 + x1);
    let n_aisles = deck_aisles(g, deck, &declared, aisle_w, x_mid);
    let extra = extra_monument_bays(g, deck, pax, seated, class_bays, aisle_w, n_aisles);
    if extra == 0 {
        return first;
    }
    pass(extra).0
}

/// What the row packer placed on a declared main deck.
pub(crate) struct DeclaredSeating {
    /// The seat rows.
    pub items: Vec<DeckItem>,
    /// The monument bays: both ends, the class boundaries and the doors.
    pub bays: Vec<Bay>,
    /// Seats placed.
    pub seated: i64,
    /// Fraction of the seat region the block spans.
    pub utilization: f64,
    /// The widest row placed.
    pub max_abreast: i64,
    /// The most aisles any row needed.
    pub max_aisles: i64,
}

/// Pack the remaining seats of `classes` onto a declared main deck.
#[allow(clippy::too_many_arguments)] // The deck, its classes and limits.
pub(crate) fn place_declared_deck(
    g: &CabinGeometry,
    deck: &DeckSpec,
    x0: f64,
    x1: f64,
    pax: &PassengerCabinConfig,
    classes: &mut [CabinClass],
    total_pax: i64,
    aisle_w: f64,
    cap: i64,
) -> DeclaredSeating {
    let present: Vec<usize> = (0..classes.len())
        .filter(|&index| classes[index].remaining > 0)
        .collect();
    let configs: Vec<SeatClassConfig> = present
        .iter()
        .map(|&index| classes[index].config.clone())
        .collect();
    let declared: Vec<DeclaredClass<'_>> = present
        .iter()
        .zip(&configs)
        .map(|(&index, config)| DeclaredClass {
            config,
            demand: Demand::Seats(classes[index].remaining),
        })
        .collect();
    let class_bays = declared.len().saturating_sub(1);
    let n_aisles = deck_aisles(g, deck, &declared, aisle_w, 0.5 * (x0 + x1));
    let extra = extra_monument_bays(g, deck, pax, total_pax, class_bays, aisle_w, n_aisles);
    let declared_deck = DeclaredDeck::new(g, deck, x0, x1, extra, aisle_w, cap);
    // A fuselage without an under-floor hold stows its baggage on the main
    // deck, so the floor the rows leave is the compartments' and the rows keep
    // their declared pitch rather than spreading over the whole deck.
    let walk = if crate::cargo::lacks_underfloor_hold(g) {
        declared_deck.walk(&declared, 1.0)
    } else {
        declared_deck.walk_stretched(&declared)
    };

    let mut items = Vec::with_capacity(walk.rows.len());
    let (mut max_abreast, mut max_aisles) = (0, 1);
    for row in &walk.rows {
        let class = &mut classes[present[row.class]];
        items.push(seat_row_item(
            g,
            deck,
            class,
            row.x0,
            row.pitch,
            row.seats,
            (row.abreast, row.aisles),
            aisle_w,
        ));
        class.remaining -= row.seats;
        class.seated += row.seats;
        max_abreast = max_abreast.max(row.abreast);
        max_aisles = max_aisles.max(row.aisles);
    }

    let bay_at = |x: f64| Bay::new(x, deck.name, g.usable_width(deck, x));
    let mut bays = vec![bay_at(x0 + 0.5 * MONUMENT_BAY_LENGTH_M)];
    bays.extend(walk.class_bays.iter().map(|&x| bay_at(x)));
    bays.extend(declared_deck.door_bays.iter().map(|&x| bay_at(x)));
    bays.push(bay_at(x1 - 0.5 * MONUMENT_BAY_LENGTH_M));

    let span = declared_deck.end - declared_deck.start;
    let utilization = match walk.rows.last() {
        Some(last) if span > 0.0 => {
            ((last.x0 + last.pitch - declared_deck.start) / span).clamp(0.0, 1.0)
        }
        _ => 0.0,
    };
    DeclaredSeating {
        items,
        bays,
        seated: walk.seated,
        utilization,
        max_abreast,
        max_aisles,
    }
}

// These are assertions over registered presets the test loads; a failed
// unwrap or expect is the assertion failing, not a library invariant.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests;
