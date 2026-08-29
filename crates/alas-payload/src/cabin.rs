// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/cabin_layout.py
// Reference: alas @ rust-port-baseline.

//! The passenger cabin: what the regulation allows, and where the furniture
//! goes.
//!
//! This module is the rule book the seating engine in [`engine`] reads from.
//! Everything here answers a question the layout cannot make up: how wide the
//! aisle has to be, which door type a body of this diameter takes, how many
//! people those doors may legally evacuate, how many seats fit between the
//! walls, and how a bay divides when more monuments are asked of it than it
//! has room for.
//!
//! # Why the exit capacity caps the seating rather than following from it
//!
//! A real aircraft is exit-limited, not floor-limited. There are only so many
//! places along a deck where a door pair can structurally go, and that count
//! -- not the length of bare floor -- is what decides how many passengers may
//! be carried. Deriving the door count from an already-chosen passenger count
//! inverts the constraint and produces a body with doors every two metres: an
//! A380 seating 1,400 rather than the 853 it is certified for.
//! [`max_certifiable_capacity`] is therefore computed first, from the geometry
//! alone, and the seating truncates against it.
//!
//! # References
//!
//! * FAR/CS-25.807(g): exit classification, per-side capacity and minimum
//!   cutout dimensions.
//! * FAR/CS-25.815: minimum main-aisle width.
//! * FAR/CS-25.817: no more than three seats between any passenger and an
//!   aisle.

mod engine;
mod fittings;
mod seating;

pub use engine::{build_passenger_layout, build_passenger_layout_reference_compatibility};

use alas_config::PassengerCabinConfig;

use crate::geometry::{CabinGeometry, DeckSpec};
use crate::numeric::{floor_div, round_half_even};

/// One FAR/CS-25.807 emergency-exit class: what it may evacuate, and the
/// smallest cutout it may be built at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExitSpec {
    /// The type letter, which is what a deck plan labels the door with.
    pub name: &'static str,
    /// Seats this type is rated for, per fuselage side.
    pub capacity_per_side: i64,
    /// Minimum door width, which lies along the fuselage x-axis in plan.
    pub width_m: f64,
    /// Minimum door height.
    pub height_m: f64,
}

/// The FAR/CS-25.807(g) exit classification.
pub static EXIT_TYPES: [ExitSpec; 7] = [
    ExitSpec {
        name: "A",
        capacity_per_side: 110,
        width_m: 1.07,
        height_m: 1.83,
    },
    ExitSpec {
        name: "B",
        capacity_per_side: 75,
        width_m: 0.81,
        height_m: 1.83,
    },
    ExitSpec {
        name: "C",
        capacity_per_side: 55,
        width_m: 0.76,
        height_m: 1.22,
    },
    ExitSpec {
        name: "I",
        capacity_per_side: 45,
        width_m: 0.61,
        height_m: 1.22,
    },
    ExitSpec {
        name: "II",
        capacity_per_side: 40,
        width_m: 0.51,
        height_m: 1.12,
    },
    ExitSpec {
        name: "III",
        capacity_per_side: 35,
        width_m: 0.51,
        height_m: 0.91,
    },
    ExitSpec {
        name: "IV",
        capacity_per_side: 9,
        width_m: 0.48,
        height_m: 0.66,
    },
];

/// Longitudinal floor a galley or lavatory bay consumes.
pub(crate) const MONUMENT_LEN: f64 = 0.95;

/// The height a seat row is drawn at.
pub(crate) const SEAT_BOX_H: f64 = 1.25;

/// FAR/CS-25.815 minimum main-aisle width up to nineteen passengers.
const AISLE_W_SMALL: f64 = 0.30;

/// FAR/CS-25.815 minimum main-aisle width above that, which is the twenty-inch
/// upper-body clearance and therefore the one that governs at armrest level --
/// where the seats-abreast budget is actually spent.
const AISLE_W_LARGE: f64 = 0.51;

/// The tightest row spacing the layout will lay out at, whatever a class asks
/// for. A pitch of zero would place rows on top of each other forever.
pub(crate) const MIN_PITCH: f64 = 0.30;

/// The narrowest seat footprint the abreast count is computed against, for the
/// same reason [`MIN_PITCH`] exists.
pub(crate) const MIN_SEAT_WIDTH: f64 = 0.30;

/// Seats a single aisle may separate a passenger from, times two, plus the
/// aisle-side pair: FAR/CS-25.817's three-seat rule caps a single-aisle row at
/// six abreast and a twin-aisle row at twelve.
const MAX_ABREAST_SINGLE_AISLE: i64 = 6;
/// The twin-aisle counterpart of [`MAX_ABREAST_SINGLE_AISLE`].
const MAX_ABREAST_TWIN_AISLE: i64 = 12;

/// Door pairs no deck exceeds, whatever its length: no transport aircraft
/// carries more per deck, and the cap is what stops a long body being given a
/// door every spacing interval.
const MAX_EXIT_PAIRS_PER_DECK: i64 = 6;

/// A deck holding more than this many passengers needs two exit pairs rather
/// than one, independently of what the type rating alone would allow.
const TWO_PAIR_THRESHOLD: i64 = 110;

/// FAR/CS-25.815 minimum main-aisle width for a passenger count.
pub fn required_aisle_width(n_pax: i64) -> f64 {
    if n_pax <= 19 {
        AISLE_W_SMALL
    } else {
        AISLE_W_LARGE
    }
}

/// The aisle width to lay out with: the configured override where there is
/// one, and otherwise the regulatory minimum for this many passengers.
pub fn resolve_aisle_width(pax: &PassengerCabinConfig, n_pax: i64) -> f64 {
    if pax.aisle_width_m > 0.0 {
        pax.aisle_width_m
    } else {
        required_aisle_width(n_pax)
    }
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
    EXIT_TYPES
        .iter()
        .find(|spec| spec.name == letter)
        .unwrap_or(&EXIT_TYPES[0])
}

/// A stretch of one deck available for seating and exits.
#[derive(Debug, Clone, Copy)]
pub struct DeckSegment<'a> {
    /// The deck this stretch belongs to.
    pub deck: &'a DeckSpec,
    /// Forward end.
    pub x0: f64,
    /// Aft end.
    pub x1: f64,
}

/// The longitudinal stretch of each passenger deck available to lay out in.
///
/// The seating engine and the fast auto-sizer both read this rather than each
/// deriving "available cabin length" for themselves, since a preset that
/// promised a seat count the detailed layout could not place would be reported
/// on the cabin page and never flown.
///
/// The main deck runs a quarter of the way into the tailcone and the upper
/// deck does not: an upper deck ends where the crown starts curving down,
/// while the main floor carries on into the taper.
pub fn cabin_deck_segments(g: &CabinGeometry) -> Vec<DeckSegment<'_>> {
    g.passenger_decks
        .iter()
        .map(|deck| {
            if deck.name == crate::layout::UPPER {
                DeckSegment {
                    deck,
                    x0: g.cabin_start_x + 3.0,
                    x1: g.cabin_end_x,
                }
            } else {
                DeckSegment {
                    deck,
                    x0: g.cabin_start_x + 0.5,
                    x1: g.cabin_end_x + 0.25 * g.tailcone_len,
                }
            }
        })
        .collect()
}

/// Cabin floor reserved for galleys, lavatories, closets and crew rest
/// *beyond* the inter-class monument bays, for one deck stretch.
///
/// This is what makes a premium cabin genuinely less dense rather than letting
/// economy floor-fill whatever the premium classes leave over. It scales with
/// length, at roughly the CS-25 provisioning ratio, and far more steeply with
/// premium content: first and business cabins carry dedicated galleys, coat
/// closets and crew-rest bunks that an all-economy cabin does not, which is the
/// dominant reason a three-class 787-9 seats about 222 where the same shell
/// all-economy seats about 410.
///
/// The two coefficients are calibrated so the shipped presets reproduce the
/// published seat counts of the aircraft they are named after.
pub fn service_reserve_len(deck_length: f64, mix: &[(&str, f64)]) -> f64 {
    let share = |name: &str| {
        mix.iter()
            .find(|(other, _)| *other == name)
            .map_or(0.0, |&(_, fraction)| fraction)
    };
    let premium_share = share("First") + share("Business");
    deck_length.max(0.0) * (0.045 + 0.45 * premium_share)
}

/// The CS-25.807-legal passenger ceiling, per deck and in total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckCapacities {
    /// Each passenger deck's ceiling, in the order the decks are laid out.
    pub per_deck: Vec<(&'static str, i64)>,
    /// The sum over the decks.
    pub total: i64,
}

impl DeckCapacities {
    /// One deck's ceiling, falling back to the total for a deck that was not
    /// laid out -- upstream's `deck_caps.get(name, deck_caps["total"])`.
    pub fn for_deck(&self, name: &str) -> i64 {
        self.per_deck
            .iter()
            .find(|(deck, _)| *deck == name)
            .map_or(self.total, |&(_, seats)| seats)
    }
}

/// The maximum passenger count CS-25.807 will certify for this body.
///
/// Each deck gets one exit pair per [`PassengerCabinConfig::min_exit_pair_spacing_m`]
/// of its length, at least one and at most [`MAX_EXIT_PAIRS_PER_DECK`], and the
/// ceiling is what those pairs are rated to evacuate.
///
/// Only Type A is derated. Its nominal 110 per side is far above what a real
/// evacuation demonstration achieves once several such doors interact over long
/// widebody aisles -- the 787-9's four Type-A pairs give an exit limit of 420,
/// not the naive 880 -- while the smaller types, with shorter aisles behind
/// them, were found to track their nominal rating.
pub fn max_certifiable_capacity(g: &CabinGeometry, pax: &PassengerCabinConfig) -> DeckCapacities {
    let spec = select_exit_type(g.diameter_m);
    let mut cap_per_side = spec.capacity_per_side as f64;
    if spec.name == "A" {
        cap_per_side *= pax.exit_capacity_realism_factor.max(0.01);
    }
    let spacing = pax.min_exit_pair_spacing_m.max(1.0);

    let mut per_deck = Vec::new();
    let mut total = 0;
    for segment in cabin_deck_segments(g) {
        let deck_len = (segment.x1 - segment.x0).max(0.0);
        let n_pairs = (floor_div(deck_len, spacing) as i64).clamp(1, MAX_EXIT_PAIRS_PER_DECK);
        let deck_cap = (n_pairs as f64 * cap_per_side * 2.0) as i64;
        per_deck.push((segment.deck.name, deck_cap));
        total += deck_cap;
    }
    DeckCapacities { per_deck, total }
}

/// Seats abreast and aisle count for one class at station `x`.
///
/// FAR/CS-25.817 allows at most three seats between any passenger and an
/// aisle, so a single aisle admits six abreast and two admit twelve. Whichever
/// legal option seats more in the local floor width wins, and a tie goes to the
/// single aisle -- a second aisle that adds no seats has only spent floor.
pub fn abreast_and_aisles(
    class: &alas_config::SeatClassConfig,
    deck: &DeckSpec,
    g: &CabinGeometry,
    aisle_w: f64,
    x: f64,
) -> (i64, i64) {
    let seat_w = class.width_m.max(MIN_SEAT_WIDTH);
    if class.abreast > 0 {
        let aisles = if class.abreast <= MAX_ABREAST_SINGLE_AISLE {
            1
        } else {
            2
        };
        return (class.abreast, aisles);
    }
    let usable = g.usable_width(deck, x);
    let n_single = (floor_div(usable - aisle_w, seat_w) as i64).min(MAX_ABREAST_SINGLE_AISLE);
    let n_twin = (floor_div(usable - 2.0 * aisle_w, seat_w) as i64).min(MAX_ABREAST_TWIN_AISLE);
    if n_twin > n_single {
        (n_twin.max(1), 2)
    } else {
        (n_single.max(1), 1)
    }
}

/// Seats abreast alone, for the callers that do not need the aisle count.
pub fn abreast(
    class: &alas_config::SeatClassConfig,
    deck: &DeckSpec,
    g: &CabinGeometry,
    aisle_w: f64,
    x: f64,
) -> i64 {
    abreast_and_aisles(class, deck, g, aisle_w, x).0
}

/// Split `n` seats abreast into the lateral blocks the aisles divide them into.
///
/// A single aisle splits near-evenly (3-3, 3-2). Twin aisles put up to three in
/// each outboard block, which is 25.817's window-side limit, and the remainder
/// in the centre (3-4-3, 2-3-2); a count too small to leave a centre block
/// falls back to the single-aisle split.
pub fn seat_blocks(n: i64, n_aisles: i64) -> Vec<i64> {
    if n <= 1 {
        return vec![n];
    }
    let even_split = || vec![(n + 1) / 2, n / 2];
    if n_aisles <= 1 {
        return even_split();
    }
    let outboard = if n >= 8 { 3 } else { ((n - 1) / 3).max(1) };
    let center = n - 2 * outboard;
    if center < 1 {
        return even_split();
    }
    vec![outboard, center, outboard]
}

/// The order galleys, lavatories and exits visit the monument bays in.
///
/// Real cabin layouts anchor monuments at the very front and the very back
/// first and only add mid-cabin ones once those are full, so the order is
/// front, back, second-from-front, second-from-back, and so on. A plain
/// front-to-back fill would pile everything into the forward bays and leave
/// the rear of the cabin bare whenever fewer items are asked for than there
/// are bays.
pub fn monument_fill_order(n_bays: usize) -> Vec<usize> {
    let mut order = Vec::with_capacity(n_bays);
    if n_bays == 0 {
        return order;
    }
    let (mut lo, mut hi) = (0usize, n_bays - 1);
    while lo < hi {
        order.push(lo);
        order.push(hi);
        lo += 1;
        hi -= 1;
    }
    if lo == hi {
        order.push(lo);
    }
    order
}

/// `n_items` bay indices spread evenly front to rear across `n_bays`.
///
/// Emergency exits need this rather than [`monument_fill_order`]: the pair
/// count is usually below the bay count, and that function's fixed traversal is
/// meant to be *cycled*, so any partial prefix of it drifts toward the front
/// instead of spacing itself out. This anchors the ends and lands on the true
/// middle bay for a middling count.
pub fn spread_bay_indices(n_items: i64, n_bays: usize) -> Vec<usize> {
    if n_bays == 0 || n_items <= 0 {
        return Vec::new();
    }
    let n_items = (n_items as usize).min(n_bays);
    if n_items == 1 {
        return vec![0];
    }
    let mut out: Vec<usize> = Vec::with_capacity(n_items);
    for i in 0..n_items {
        let exact = (i * (n_bays - 1)) as f64 / (n_items - 1) as f64;
        let mut idx = round_half_even(exact) as usize;
        while out.contains(&idx) && idx < n_bays - 1 {
            idx += 1;
        }
        out.push(idx);
    }
    out
}

/// A monument anchor: a station on a deck, and how much of each side's
/// half-width the monuments stacked there have already consumed.
#[derive(Debug, Clone, PartialEq)]
pub struct Bay {
    /// Longitudinal centre.
    pub x: f64,
    /// Which deck it is on.
    pub deck: &'static str,
    /// Usable floor width at this station.
    pub width: f64,
    /// Floor already taken by galleys, measured in from the starboard wall.
    pub galley_depth: f64,
    /// Floor already taken by lavatories, measured in from the port wall.
    pub lav_depth: f64,
}

impl Bay {
    /// An empty bay at a station.
    pub fn new(x: f64, deck: &'static str, width: f64) -> Self {
        Self {
            x,
            deck,
            width,
            galley_depth: 0.0,
            lav_depth: 0.0,
        }
    }
}

/// Which side of the cabin a monument packs in from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonumentSide {
    /// Galleys, which take the positive-y side.
    Galley,
    /// Lavatories, which take the negative-y side.
    Lav,
}

/// The offset from the centreline and the drawn width of the next monument
/// packed inward from the wall on one side of `bay`.
///
/// Items of the same side pack back to back from the wall, the first one's
/// outer edge flush with the usable width. Once a bay holds more than its
/// half-width can fit at full size -- routine once the monument count exceeds
/// the bay count, see [`monument_fill_order`] -- the item is *narrowed* to
/// whatever room is left rather than moved: kept at full width it would either
/// cross the centreline into the other side's territory or, clamped there,
/// overlap the item already stacked behind it.
pub fn stack_y(bay: &mut Bay, side: MonumentSide, item_width: f64) -> (f64, f64) {
    let half = bay.width / 2.0;
    let consumed = match side {
        MonumentSide::Galley => &mut bay.galley_depth,
        MonumentSide::Lav => &mut bay.lav_depth,
    };
    let depth = consumed.min(half);
    let available = half - depth;
    let drawn_width = item_width.min(available).max(0.0);
    *consumed = depth + drawn_width;
    let outer_edge = half - depth;
    (outer_edge - drawn_width / 2.0, drawn_width)
}

/// `math.ceil(a / b)` for the non-negative counts upstream applies it to.
pub(crate) fn ceil_div(a: i64, b: i64) -> i64 {
    if b <= 0 {
        return 0;
    }
    (a + b - 1) / b
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

// A test asserts on geometry it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::SeatClassConfig;

    #[test]
    fn the_aisle_widens_at_twenty_passengers() {
        // 25.815 changes rule exactly there, and a business jet either side of
        // the boundary is a different aircraft to lay out.
        assert_eq!(required_aisle_width(19), 0.30);
        assert_eq!(required_aisle_width(20), 0.51);
    }

    #[test]
    fn an_explicit_aisle_width_wins_over_the_regulatory_minimum() {
        let mut pax = PassengerCabinConfig {
            aisle_width_m: 0.9,
            ..Default::default()
        };
        assert_eq!(resolve_aisle_width(&pax, 300), 0.9);
        pax.aisle_width_m = 0.0;
        assert_eq!(resolve_aisle_width(&pax, 300), 0.51);
    }

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
    fn a_declared_abreast_count_chooses_its_own_aisle_count() {
        // Six abreast is the most one aisle may serve, so seven needs two.
        let g_unused_x = 0.0;
        let deck = DeckSpec {
            name: crate::layout::MAIN,
            floor_frac: -0.18,
            ceil_frac: 0.95,
            width_factor: 0.97,
            is_passenger: true,
        };
        let geometry = probe_geometry();
        for (declared, expected_aisles) in [(4, 1), (6, 1), (7, 2), (10, 2)] {
            let class = SeatClassConfig {
                abreast: declared,
                ..Default::default()
            };
            assert_eq!(
                abreast_and_aisles(&class, &deck, &geometry, 0.51, g_unused_x),
                (declared, expected_aisles)
            );
        }
    }

    #[test]
    fn a_single_aisle_splits_evenly_and_a_twin_aisle_keeps_three_outboard() {
        assert_eq!(seat_blocks(6, 1), vec![3, 3]);
        assert_eq!(seat_blocks(5, 1), vec![3, 2]);
        assert_eq!(seat_blocks(10, 2), vec![3, 4, 3]);
        assert_eq!(seat_blocks(9, 2), vec![3, 3, 3]);
        assert_eq!(seat_blocks(7, 2), vec![2, 3, 2]);
        assert_eq!(seat_blocks(1, 2), vec![1]);
    }

    #[test]
    fn a_twin_aisle_row_too_narrow_for_a_centre_block_falls_back_to_two() {
        // Two abreast with two aisles would leave nothing in the middle, and
        // a zero-wide centre block is not a block.
        assert_eq!(seat_blocks(2, 2), vec![1, 1]);
        assert_eq!(seat_blocks(4, 2), vec![1, 2, 1]);
    }

    #[test]
    fn monuments_fill_the_ends_of_the_cabin_before_the_middle() {
        assert_eq!(monument_fill_order(1), vec![0]);
        assert_eq!(monument_fill_order(4), vec![0, 3, 1, 2]);
        assert_eq!(monument_fill_order(5), vec![0, 4, 1, 3, 2]);
        assert_eq!(monument_fill_order(0), Vec::<usize>::new());
    }

    #[test]
    fn exits_spread_across_the_bays_rather_than_bunching_forward() {
        assert_eq!(spread_bay_indices(1, 5), vec![0]);
        assert_eq!(spread_bay_indices(2, 5), vec![0, 4]);
        assert_eq!(spread_bay_indices(3, 5), vec![0, 2, 4]);
        // More exits than bays is capped at one per bay rather than doubling up.
        assert_eq!(spread_bay_indices(9, 3), vec![0, 1, 2]);
        assert!(spread_bay_indices(3, 0).is_empty());
    }

    #[test]
    fn a_crowded_bay_narrows_its_monuments_instead_of_overlapping_them() {
        let mut bay = Bay::new(10.0, crate::layout::MAIN, 4.0);
        let (y1, w1) = stack_y(&mut bay, MonumentSide::Galley, 0.85);
        let (y2, w2) = stack_y(&mut bay, MonumentSide::Galley, 0.85);
        assert_eq!(w1, 0.85);
        assert_eq!(w2, 0.85);
        // Back to back: the second item's outer edge is the first one's inner.
        assert!((y1 - w1 / 2.0 - (y2 + w2 / 2.0)).abs() < 1e-12);

        // The third would cross the centreline at full width, so it is cut
        // down to the 0.3 m the half-width has left rather than moved.
        let (_y3, w3) = stack_y(&mut bay, MonumentSide::Galley, 0.85);
        assert!(
            (w3 - 0.3).abs() < 1e-12,
            "the third galley was drawn at {w3} m"
        );
        let (_y4, w4) = stack_y(&mut bay, MonumentSide::Galley, 0.85);
        assert_eq!(w4, 0.0);
    }

    #[test]
    fn the_two_sides_of_a_bay_stack_independently() {
        // Galleys take the starboard half and lavatories the port half, so a
        // galley must not narrow the lavatory facing it.
        let mut bay = Bay::new(10.0, crate::layout::MAIN, 3.0);
        stack_y(&mut bay, MonumentSide::Galley, 0.85);
        let (_y, w) = stack_y(&mut bay, MonumentSide::Lav, 0.90);
        assert_eq!(w, 0.90);
    }

    #[test]
    fn a_deck_capacity_names_the_total_for_a_deck_it_does_not_carry() {
        let caps = DeckCapacities {
            per_deck: vec![("main", 400)],
            total: 400,
        };
        assert_eq!(caps.for_deck("main"), 400);
        assert_eq!(caps.for_deck("upper"), 400);
    }

    #[test]
    fn only_the_premium_classes_reserve_service_floor() {
        // An all-economy deck still carries galleys, at the flat provisioning
        // ratio; a premium cabin carries several times as much.
        let economy = service_reserve_len(40.0, &[("Economy", 1.0)]);
        let premium = service_reserve_len(40.0, &[("Business", 0.4), ("Economy", 0.6)]);
        assert!((economy - 40.0 * 0.045).abs() < 1e-12);
        assert!(premium > 4.0 * economy);
        assert_eq!(service_reserve_len(-5.0, &[]), 0.0);
    }

    fn probe_geometry() -> CabinGeometry {
        use alas_config::GeometryConfig;
        use alas_geom::builder::AircraftBuilder;
        let builder = AircraftBuilder::new(Some(GeometryConfig::default()));
        let plane = builder
            .build(None, false)
            .expect("the default aircraft builds");
        CabinGeometry::new(&plane, &builder.geometry, 0.15)
            .expect("a built aircraft has a fuselage and a wing")
    }
}
