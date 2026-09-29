// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Extreme loading sequences and the CG "potato" they bound.
//!
//! [`crate::layout::PayloadLayout`] answers where a *fully loaded* interior's
//! payload sits; this module covers the partial-load case, which a single
//! lumped payload centroid cannot show while the aircraft is only partly
//! loaded. A real loading diagram traces several extreme
//! boarding/loading orders and takes their envelope: partial-load CG can sit
//! well forward or aft of the straight line between the empty and full
//! points, and only a sequence, not a centroid, can show that.
//!
//! # Passenger sequences
//!
//! [`passenger_loading_sequences`] builds four sequences from the actual
//! seat map ([`crate::layout::SeatMeta::blocks`]): window seats first or
//! aisle seats first, each boarded front-to-back or back-to-front, with the
//! opposite category last and middle seats always in between. This is the
//! coarse, per-row model the module doc promises: every seat row is one
//! point, its mass split into window/middle/aisle shares by the row's own
//! block layout ([`seat_category_capacities`]), not by an individual seat's
//! own coordinates, which [`crate::layout::DeckItem`] does not carry. A
//! future per-seat (rather than per-row) mode would refine within a row's
//! own mass share and is not implemented here; it would not change which
//! *rows* board in which order, only how a partly filled row's mass moves
//! within its own row width, so the row-level model is exact for a full row
//! and a documented approximation only for a row seated below capacity.
//!
//! # Cargo sequences
//!
//! [`cargo_loading_sequences`] builds two sequences from the lower-hold
//! items the existing bulk/ULD cargo model already placed
//! ([`crate::cargo`]), split forward/aft of the wing box
//! ([`crate::geometry::CabinGeometry::wing_box_x_range`]): forward-hold-first
//! and aft-hold-first.
//!
//! # The potato boundary
//!
//! [`potato_boundary`] samples every sequence's piecewise-linear mass-vs-CG
//! path at a common set of mass levels and returns the envelope (minimum and
//! maximum CG at each level): the "potato" every extreme-sequence loading
//! diagram plots.

use crate::layout::{DeckItem, ItemKind, ItemMeta, PayloadLayout};

/// One state along a loading sequence: the cumulative aircraft mass and
/// centre of gravity after everything loaded up to this point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadingPoint {
    /// Cumulative aircraft mass at this point, kg. Monotonically
    /// non-decreasing along [`LoadingSequence::points`].
    pub mass_kg: f64,
    /// Cumulative longitudinal centre of gravity at this point, aircraft
    /// body axes (positive aft), m.
    pub x_m: f64,
}

/// One extreme loading order, from the empty (DOW) point to the fully
/// loaded (ZFW) point.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadingSequence {
    /// A stable, human-readable name for this order (report label / id).
    pub name: String,
    /// The cumulative path, starting at the DOW point this sequence was
    /// built from and ending at the same ZFW point every other sequence
    /// built from the same layout ends at.
    pub points: Vec<LoadingPoint>,
}

/// How many of a seat row's abreast seats are windows, middle and aisle
/// seats, from its lateral block layout (outboard block to outboard block).
///
/// A block's outer edge is a window when the block is the first or last in
/// the row (nothing but the cabin wall beyond it) and an aisle seat
/// otherwise (an aisle beyond it, since a row has no third lateral
/// neighbour). A single-seat block therefore counts as one window seat if
/// it is an end block, or one aisle seat if it sits between two aisles (a
/// centre single, e.g. a 1-2-1 business row's middle pair split by a single
/// aisle would not produce this case, but a 1-1-1 layout's centre seat
/// does). Every seat between a block's two edges (`n - 2`, for a block of
/// `n >= 2`) is a middle seat.
///
/// Returns `(window, middle, aisle)` seat counts, which sum to `blocks`'s
/// total exactly.
pub fn seat_category_capacities(blocks: &[i64]) -> (f64, f64, f64) {
    let mut window = 0i64;
    let mut middle = 0i64;
    let mut aisle = 0i64;
    let block_count = blocks.len();
    for (index, &seats) in blocks.iter().enumerate() {
        if seats <= 0 {
            continue;
        }
        let left_is_wall = index == 0;
        let right_is_wall = index + 1 == block_count;
        if seats == 1 {
            if left_is_wall || right_is_wall {
                window += 1;
            } else {
                aisle += 1;
            }
            continue;
        }
        window += i64::from(left_is_wall) + i64::from(right_is_wall);
        aisle += i64::from(!left_is_wall) + i64::from(!right_is_wall);
        middle += seats - 2;
    }
    (window as f64, middle as f64, aisle as f64)
}

/// One seat row's longitudinal station and its mass split by seat category.
struct RowShare {
    x_m: f64,
    window_kg: f64,
    middle_kg: f64,
    aisle_kg: f64,
}

/// Every seat row's station and category mass split, in placement order.
///
/// A row's `filled` occupancy is spread across window/middle/aisle in the
/// same proportion as its full-row `abreast` capacity
/// ([`seat_category_capacities`]): the layout engines do not record which
/// individual abreast seats within a partly filled row are the ones
/// occupied, so this is the least-biased split available from what
/// [`crate::layout::SeatMeta`] actually carries.
fn row_shares(layout: &PayloadLayout) -> Vec<RowShare> {
    layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::SeatRow)
        .filter_map(|item| {
            let ItemMeta::Seat(seat) = &item.meta else {
                return None;
            };
            if seat.abreast <= 0 || item.mass <= 0.0 {
                return None;
            }
            let (window_cap, middle_cap, aisle_cap) = seat_category_capacities(&seat.blocks);
            let abreast = seat.abreast as f64;
            Some(RowShare {
                x_m: item.x,
                window_kg: item.mass * window_cap / abreast,
                middle_kg: item.mass * middle_cap / abreast,
                aisle_kg: item.mass * aisle_cap / abreast,
            })
        })
        .collect()
}

/// Build the cumulative path for one ordering of category tiers over rows
/// sorted by `front_to_back`.
fn accumulate(
    rows: &[RowShare],
    front_to_back: bool,
    dow: LoadingPoint,
    tier: impl Fn(&RowShare) -> f64,
) -> (f64, f64, Vec<LoadingPoint>) {
    let mut ordered: Vec<&RowShare> = rows.iter().collect();
    ordered.sort_by(|a, b| {
        if front_to_back {
            a.x_m.total_cmp(&b.x_m)
        } else {
            b.x_m.total_cmp(&a.x_m)
        }
    });
    let mut mass_kg = dow.mass_kg;
    let mut moment_kg_m = dow.mass_kg * dow.x_m;
    let mut points = Vec::with_capacity(ordered.len());
    for row in ordered {
        let increment_kg = tier(row);
        if increment_kg <= 0.0 {
            continue;
        }
        mass_kg += increment_kg;
        moment_kg_m += increment_kg * row.x_m;
        points.push(LoadingPoint {
            mass_kg,
            x_m: moment_kg_m / mass_kg,
        });
    }
    (mass_kg, moment_kg_m, points)
}

/// One [`LoadingSequence`] boarding every row's `first` category mass
/// (front-to-back or back-to-front), then `middle`, then `second`.
fn passenger_sequence(
    name: &str,
    rows: &[RowShare],
    front_to_back: bool,
    dow: LoadingPoint,
    first: impl Fn(&RowShare) -> f64,
    second: impl Fn(&RowShare) -> f64,
) -> LoadingSequence {
    let mut points = vec![dow];
    let (mass_1, moment_1, points_1) = accumulate(rows, front_to_back, dow, &first);
    points.extend(points_1);
    let carried = LoadingPoint {
        mass_kg: mass_1,
        x_m: if mass_1 > 0.0 {
            moment_1 / mass_1
        } else {
            dow.x_m
        },
    };
    let (mass_2, moment_2, points_2) =
        accumulate(rows, front_to_back, carried, |row| row.middle_kg);
    points.extend(points_2);
    let carried = LoadingPoint {
        mass_kg: mass_2,
        x_m: if mass_2 > 0.0 {
            moment_2 / mass_2
        } else {
            carried.x_m
        },
    };
    let (_, _, points_3) = accumulate(rows, front_to_back, carried, &second);
    points.extend(points_3);
    LoadingSequence {
        name: name.to_owned(),
        points,
    }
}

/// The four extreme passenger loading sequences from `layout`'s actual seat
/// map, starting at `dow` (the empty aircraft's mass and CG).
///
/// Returns an empty vector when `layout` carries no seat rows (a freighter
/// or an empty cabin).
pub fn passenger_loading_sequences(
    layout: &PayloadLayout,
    dow: LoadingPoint,
) -> Vec<LoadingSequence> {
    let rows = row_shares(layout);
    if rows.is_empty() {
        return Vec::new();
    }
    let window = |row: &RowShare| row.window_kg;
    let aisle = |row: &RowShare| row.aisle_kg;
    vec![
        passenger_sequence(
            "window-first, front-to-back",
            &rows,
            true,
            dow,
            window,
            aisle,
        ),
        passenger_sequence(
            "window-first, back-to-front",
            &rows,
            false,
            dow,
            window,
            aisle,
        ),
        passenger_sequence(
            "aisle-first, front-to-back",
            &rows,
            true,
            dow,
            aisle,
            window,
        ),
        passenger_sequence(
            "aisle-first, back-to-front",
            &rows,
            false,
            dow,
            aisle,
            window,
        ),
    ]
}

/// Whether a lower-hold cargo item sits forward or aft of the wing box.
fn is_forward_of(item: &DeckItem, wing_box_mid_x: f64) -> bool {
    item.x < wing_box_mid_x
}

/// The two extreme cargo loading sequences from `layout`'s already-placed
/// lower-hold bulk/ULD positions ([`crate::cargo`]), split forward/aft of
/// `wing_box_x_range`, starting at `dow`.
///
/// Main-deck freighter positions are out of scope (the split concerns
/// the lower holds a passenger or combi aircraft splits fore and
/// aft of the wing box; a main-deck freighter has no such split). Returns an
/// empty vector when `layout` carries no lower-deck cargo items.
pub fn cargo_loading_sequences(
    layout: &PayloadLayout,
    wing_box_x_range: (f64, f64),
    dow: LoadingPoint,
) -> Vec<LoadingSequence> {
    let wing_box_mid_x = (wing_box_x_range.0 + wing_box_x_range.1) / 2.0;
    let items: Vec<&DeckItem> = layout
        .items
        .iter()
        .filter(|item| {
            item.deck == crate::layout::LOWER
                && item.mass > 0.0
                && matches!(item.kind, ItemKind::Uld | ItemKind::Bag)
        })
        .collect();
    if items.is_empty() {
        return Vec::new();
    }
    let sequence = |name: &str, forward_first: bool| -> LoadingSequence {
        let mut ordered: Vec<&&DeckItem> = items.iter().collect();
        ordered.sort_by(|a, b| {
            let a_forward = is_forward_of(a, wing_box_mid_x);
            let b_forward = is_forward_of(b, wing_box_mid_x);
            match (a_forward == forward_first, b_forward == forward_first) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.x.total_cmp(&b.x),
            }
        });
        let mut mass_kg = dow.mass_kg;
        let mut moment_kg_m = dow.mass_kg * dow.x_m;
        let mut points = vec![dow];
        for item in ordered {
            mass_kg += item.mass;
            moment_kg_m += item.mass * item.x;
            points.push(LoadingPoint {
                mass_kg,
                x_m: moment_kg_m / mass_kg,
            });
        }
        LoadingSequence {
            name: name.to_owned(),
            points,
        }
    };
    vec![
        sequence("forward-hold-first", true),
        sequence("aft-hold-first", false),
    ]
}

/// One level of the loading-sequence envelope: at `mass_kg`, the most
/// forward and most aft CG any of the sampled sequences reaches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PotatoPoint {
    /// The common mass level, kg.
    pub mass_kg: f64,
    /// The most forward (smallest x) CG any sequence reaches at this mass,
    /// aircraft body axes, m.
    pub min_x_m: f64,
    /// The most aft (largest x) CG any sequence reaches at this mass,
    /// aircraft body axes, m.
    pub max_x_m: f64,
}

/// Linearly interpolate `sequence`'s CG at `mass_kg`; `None` if `sequence`
/// has no points, or if `mass_kg` lies outside this sequence's own
/// `[first.mass_kg, last.mass_kg]` span (beyond a small floating-point
/// tolerance).
///
/// Earlier versions clamped an out-of-span query to the nearest endpoint,
/// which let a sequence that ends below the shared ZFW (e.g. a cargo-only
/// sequence with no passenger mass added) hold its *last* CG flat at every
/// higher mass level a longer sequence still reaches -- silently
/// manufacturing a boundary at masses this sequence never actually visited.
/// Returning `None` instead makes the caller ([`potato_boundary`]) exclude
/// this sequence from every mass level it does not cover, so the envelope
/// at a given mass only ever reflects sequences that actually reach it.
fn interpolate_x(sequence: &LoadingSequence, mass_kg: f64) -> Option<f64> {
    const TOLERANCE_KG: f64 = 1.0e-6;
    let points = &sequence.points;
    let first = points.first()?;
    let last = points.last()?;
    if mass_kg < first.mass_kg - TOLERANCE_KG || mass_kg > last.mass_kg + TOLERANCE_KG {
        return None;
    }
    if mass_kg <= first.mass_kg {
        return Some(first.x_m);
    }
    if mass_kg >= last.mass_kg {
        return Some(last.x_m);
    }
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if mass_kg >= a.mass_kg && mass_kg <= b.mass_kg {
            let span = b.mass_kg - a.mass_kg;
            if span <= 0.0 {
                return Some(a.x_m);
            }
            let blend = (mass_kg - a.mass_kg) / span;
            return Some(a.x_m + blend * (b.x_m - a.x_m));
        }
    }
    Some(last.x_m)
}

/// The CG envelope (potato) of `sequences`, at `n_levels` mass levels evenly
/// spaced between the lowest DOW mass and the highest ZFW mass any sequence
/// carries.
///
/// Every physical sequence built by this module shares the same DOW and ZFW
/// mass (all load the same total payload from the same starting point), so
/// in the ordinary case this samples from one shared endpoint to the other;
/// sequences from different starting points are still accepted; the
/// envelope is simply wider at the shared range's edges.
///
/// Returns an empty vector for `n_levels == 0` or an empty `sequences`.
///
/// A mass level where no sequence reaches (per [`interpolate_x`]'s own
/// exclusion of out-of-span queries) is omitted from the result rather than
/// synthesized from whichever sequence happens to hold that mass flat: see
/// [`interpolate_x`]'s doc comment for the composition bug this avoids.
pub fn potato_boundary(sequences: &[LoadingSequence], n_levels: usize) -> Vec<PotatoPoint> {
    if n_levels == 0 || sequences.is_empty() {
        return Vec::new();
    }
    let min_mass_kg = sequences
        .iter()
        .filter_map(|sequence| sequence.points.first().map(|point| point.mass_kg))
        .fold(f64::INFINITY, f64::min);
    let max_mass_kg = sequences
        .iter()
        .filter_map(|sequence| sequence.points.last().map(|point| point.mass_kg))
        .fold(f64::NEG_INFINITY, f64::max);
    if !(min_mass_kg.is_finite() && max_mass_kg.is_finite()) || max_mass_kg < min_mass_kg {
        return Vec::new();
    }
    (0..n_levels)
        .filter_map(|index| {
            let mass_kg = if n_levels == 1 {
                max_mass_kg
            } else {
                min_mass_kg + (max_mass_kg - min_mass_kg) * index as f64 / (n_levels - 1) as f64
            };
            let mut min_x_m = f64::INFINITY;
            let mut max_x_m = f64::NEG_INFINITY;
            for sequence in sequences {
                if let Some(x_m) = interpolate_x(sequence, mass_kg) {
                    min_x_m = min_x_m.min(x_m);
                    max_x_m = max_x_m.max(x_m);
                }
            }
            if !(min_x_m.is_finite() && max_x_m.is_finite()) {
                return None;
            }
            Some(PotatoPoint {
                mass_kg,
                min_x_m,
                max_x_m,
            })
        })
        .collect()
}

/// Physically compose two sequences end to end: `second` must already start
/// (its first point) at `first`'s own last point -- i.e. `second` was built
/// with `first`'s end point as its own `dow` argument. Returns `first`'s
/// points followed by `second`'s points with that shared junction point not
/// duplicated.
///
/// This is how a cargo sequence and a passenger sequence are chained into
/// one sequence that starts at the aircraft's true DOW and ends at the true
/// ZFW (DOW plus *all* payload, not just one category): loading cargo alone
/// or boarding passengers alone each only carries the aircraft to a partial
/// mass, so neither one, by itself, is a physical loading order for a
/// mixed-payload aircraft. Composing both orders (cargo-then-passengers and
/// passengers-then-cargo, in both directions each) is what
/// [`crate::loading_sequence`]'s callers are expected to build before
/// calling [`potato_boundary`] on a mixed-payload layout.
#[must_use]
pub fn concat_sequences(
    name: &str,
    first: &LoadingSequence,
    second: &LoadingSequence,
) -> LoadingSequence {
    let mut points = first.points.clone();
    if let (Some(junction), Some(second_first)) = (points.last().copied(), second.points.first()) {
        let same_junction = (junction.mass_kg - second_first.mass_kg).abs() < 1.0e-6
            && (junction.x_m - second_first.x_m).abs() < 1.0e-9;
        let tail = if same_junction {
            &second.points[1..]
        } else {
            &second.points[..]
        };
        points.extend_from_slice(tail);
    } else {
        points.extend_from_slice(&second.points);
    }
    LoadingSequence {
        name: name.to_owned(),
        points,
    }
}

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap or expect there is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{ItemKind, ItemMeta, Mode, PayloadLayout, SeatMeta};
    use crate::layout::{LayoutSummary, MAIN};

    fn seat_row(x: f64, mass_kg: f64, blocks: Vec<i64>) -> DeckItem {
        let abreast: i64 = blocks.iter().sum();
        DeckItem {
            kind: ItemKind::SeatRow,
            deck: MAIN,
            x,
            y: 0.0,
            z: 0.0,
            length: 0.8,
            width: 3.5,
            mass: mass_kg,
            height: 1.2,
            label: "row".to_owned(),
            meta: ItemMeta::Seat(SeatMeta {
                cls: "economy",
                abreast,
                filled: abreast,
                deck: MAIN,
                aisles: (blocks.len() as i64 - 1).max(0),
                blocks,
                seat_w: 0.46,
                aisle_w: 0.5,
            }),
        }
    }

    fn narrowbody_layout(rows: usize) -> PayloadLayout {
        let items: Vec<DeckItem> = (0..rows)
            .map(|index| seat_row(10.0 + index as f64, 3.0 * 84.0, vec![3, 3]))
            .collect();
        let (total_mass, cg_x, cg_y) = crate::layout::mass_properties(&items);
        PayloadLayout {
            mode: Mode::Passenger,
            items,
            total_mass,
            cg_x,
            cg_y,
            summary: LayoutSummary::Passenger(Box::new(crate::layout::PassengerSummary {
                total_pax: rows as i64 * 6,
                seated_pax: rows as i64 * 6,
                unseated_pax: 0,
                classes: vec![("economy", rows as i64 * 6)],
                lavatories: 0,
                galleys: 0,
                accessible_lavatories: 0,
                wheelchair_stowages: 0,
                exit_type: "III",
                exit_pairs: 0,
                exit_capacity: 0,
                max_certifiable_capacity: rows as i64 * 6,
                geometric_capacity: rows as i64 * 6,
                source_capacity_cap: None,
                source_exit_layout: None,
                capacity_binding: "geometry",
                payload_t: 0.0,
                seat_mass_t: 0.0,
                bag_mass_t: 0.0,
                belly_cargo_t: 0.0,
                hold_capacity_t: 0.0,
                hold_used_t: 0.0,
                hold_ulds: 0,
                aisle_width_m: 0.5,
                max_abreast: 6,
                n_aisles: 1,
                deck_utilization: Vec::new(),
                cg_pct_mac: 0.0,
                double_deck: false,
            })),
        }
    }

    #[test]
    fn a_3_3_row_splits_two_window_two_aisle_two_middle() {
        let (window, middle, aisle) = seat_category_capacities(&[3, 3]);
        assert_eq!((window, middle, aisle), (2.0, 2.0, 2.0));
    }

    #[test]
    fn a_2_4_2_row_has_no_window_in_the_centre_block() {
        let (window, middle, aisle) = seat_category_capacities(&[2, 4, 2]);
        // Ends: one window + one aisle seat each (n=2 blocks at the wall).
        // Centre block (n=4, both edges face an aisle): two aisle, two middle.
        assert_eq!((window, middle, aisle), (2.0, 2.0, 4.0));
    }

    #[test]
    fn all_sequences_conserve_total_mass_and_end_at_the_same_zfw_point() {
        let layout = narrowbody_layout(20);
        let dow = LoadingPoint {
            mass_kg: 40_000.0,
            x_m: 17.0,
        };
        let sequences = passenger_loading_sequences(&layout, dow);
        assert_eq!(sequences.len(), 4);
        let zfw_mass_kg = dow.mass_kg + layout.total_mass;
        for sequence in &sequences {
            let last = sequence.points.last().expect("every sequence has points");
            assert!(
                (last.mass_kg - zfw_mass_kg).abs() < 1.0e-6,
                "{}: {} != {zfw_mass_kg}",
                sequence.name,
                last.mass_kg
            );
            assert!((sequence.points.first().unwrap().mass_kg - dow.mass_kg).abs() < 1.0e-9);
        }
        let first_zfw_x = sequences[0].points.last().unwrap().x_m;
        for sequence in &sequences[1..] {
            assert!((sequence.points.last().unwrap().x_m - first_zfw_x).abs() < 1.0e-6);
        }
    }

    #[test]
    fn window_first_front_to_back_is_forward_of_back_to_front_at_mid_load() {
        let layout = narrowbody_layout(20);
        let dow = LoadingPoint {
            mass_kg: 40_000.0,
            x_m: 17.0,
        };
        let sequences = passenger_loading_sequences(&layout, dow);
        let front_to_back = &sequences[0];
        let back_to_front = &sequences[1];
        // Halfway through boarding by mass, front-to-back has filled the
        // forward rows' windows; back-to-front has filled the aft rows'.
        let half_mass_kg = dow.mass_kg + layout.total_mass / 2.0;
        let forward_x = interpolate_x(front_to_back, half_mass_kg).unwrap();
        let aft_x = interpolate_x(back_to_front, half_mass_kg).unwrap();
        assert!(
            forward_x < aft_x,
            "front-to-back CG {forward_x} should be forward of back-to-front {aft_x}"
        );
    }

    #[test]
    fn the_potato_boundary_widens_between_the_shared_endpoints() {
        let layout = narrowbody_layout(20);
        let dow = LoadingPoint {
            mass_kg: 40_000.0,
            x_m: 17.0,
        };
        let sequences = passenger_loading_sequences(&layout, dow);
        let boundary = potato_boundary(&sequences, 11);
        assert_eq!(boundary.len(), 11);
        assert!((boundary[0].min_x_m - boundary[0].max_x_m).abs() < 1.0e-6);
        assert!((boundary[10].min_x_m - boundary[10].max_x_m).abs() < 1.0e-6);
        let mid = &boundary[5];
        assert!(mid.max_x_m > mid.min_x_m);
    }

    #[test]
    fn no_seat_rows_yields_no_passenger_sequences() {
        let layout = narrowbody_layout(0);
        let dow = LoadingPoint {
            mass_kg: 40_000.0,
            x_m: 17.0,
        };
        assert!(passenger_loading_sequences(&layout, dow).is_empty());
    }

    /// A sequence that ends below a mass level must not hold its last CG
    /// flat at that level: it must be excluded, not clamped. This is the
    /// exact composition bug the potato-boundary lower-bound-stuck-at-DOW
    /// defect traced to (`interpolate_x` must not clamp: a short cargo-only
    /// sequence then held its low endpoint flat across every higher mass a
    /// longer passenger sequence still reached).
    #[test]
    fn a_sequence_below_a_mass_level_does_not_contribute_there() {
        let short = LoadingSequence {
            name: "short".to_owned(),
            points: vec![
                LoadingPoint {
                    mass_kg: 40_000.0,
                    x_m: 17.0,
                },
                LoadingPoint {
                    mass_kg: 45_000.0,
                    x_m: 18.0,
                },
            ],
        };
        assert!(interpolate_x(&short, 50_000.0).is_none());
        assert!(interpolate_x(&short, 45_000.0).is_some());
        assert!(interpolate_x(&short, 39_999.0).is_none());
    }

    /// [`potato_boundary`] on a short and a long sequence must only report
    /// levels at least one sequence actually spans, and at a shared level
    /// must not include the short sequence's stale endpoint.
    #[test]
    fn potato_boundary_excludes_a_shorter_sequences_stale_endpoint() {
        let short = LoadingSequence {
            name: "short".to_owned(),
            points: vec![
                LoadingPoint {
                    mass_kg: 40_000.0,
                    x_m: 10.0,
                },
                LoadingPoint {
                    mass_kg: 45_000.0,
                    x_m: 10.0,
                },
            ],
        };
        let long = LoadingSequence {
            name: "long".to_owned(),
            points: vec![
                LoadingPoint {
                    mass_kg: 40_000.0,
                    x_m: 10.0,
                },
                LoadingPoint {
                    mass_kg: 60_000.0,
                    x_m: 20.0,
                },
            ],
        };
        let boundary = potato_boundary(&[short, long], 5);
        // Every level is bracketed by [40_000, 60_000] (the widest span);
        // beyond 45_000 only `long` contributes, so min == max there
        // (no artificial widening from the short sequence's stale x).
        for point in &boundary {
            if point.mass_kg > 45_000.0 + 1.0 {
                assert!(
                    (point.min_x_m - point.max_x_m).abs() < 1.0e-9,
                    "mass {}: min {} max {} should coincide once the short \
                     sequence has dropped out",
                    point.mass_kg,
                    point.min_x_m,
                    point.max_x_m
                );
            }
        }
    }

    /// [`concat_sequences`] must produce one continuous path from `first`'s
    /// own DOW to `second`'s own end, without duplicating the shared
    /// junction point, when `second` was built starting at `first`'s end.
    #[test]
    fn concat_sequences_chains_without_duplicating_the_junction() {
        let cargo = LoadingSequence {
            name: "cargo".to_owned(),
            points: vec![
                LoadingPoint {
                    mass_kg: 40_000.0,
                    x_m: 17.0,
                },
                LoadingPoint {
                    mass_kg: 42_000.0,
                    x_m: 16.5,
                },
            ],
        };
        let junction = *cargo.points.last().unwrap();
        let passenger = LoadingSequence {
            name: "pax".to_owned(),
            points: vec![
                junction,
                LoadingPoint {
                    mass_kg: 50_000.0,
                    x_m: 17.2,
                },
            ],
        };
        let combined = concat_sequences("cargo + pax", &cargo, &passenger);
        assert_eq!(combined.points.len(), 3);
        assert_eq!(combined.points[0], cargo.points[0]);
        assert_eq!(combined.points[1], junction);
        assert_eq!(combined.points[2], passenger.points[1]);
    }
}
