// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

mod containment;
use containment::{contained_width, contained_width_to, monument_height};

/// Lateral footprint of a galley bay.
const GALLEY_WIDTH_M: f64 = 0.85;
/// Lateral footprint of a lavatory bay.
const LAV_WIDTH_M: f64 = 0.90;
/// Passengers per lavatory at the standard provisioning ratio.
const PAX_PER_LAV: i64 = 45;
/// Passengers per galley at the same ratio, before the extra one every cabin
/// carries whatever its size.
const PAX_PER_GALLEY: i64 = 100;
/// Sidewall bins sit over seats rather than the aisle and may hang lower.
const SIDE_BIN_BOTTOM_M: f64 = 1.55;
/// Centre bins sit over a seat block between aisles, retaining more clearance.
const CENTER_BIN_BOTTOM_M: f64 = 1.72;
/// Sidewall pivot-bin depth and height.
const SIDE_BIN_DEPTH_M: f64 = 0.50;
const SIDE_BIN_HEIGHT_M: f64 = 0.40;
/// Centre hinge-bin height; width follows the centre seat block.
const CENTER_BIN_HEIGHT_M: f64 = 0.34;
/// Number of seat rows represented by one preview/layout bin segment.
const BIN_ROWS_PER_SEGMENT: usize = 6;
/// Dedicated wheelchair-stowage footprint.
const WHEELCHAIR_STOWAGE_WIDTH_M: f64 = 0.55;
const WHEELCHAIR_STOWAGE_HEIGHT_M: f64 = 1.05;

/// Longitudinal extent of the loose bulk block the overflow guard places.
pub(super) const BULK_BLOCK_LEN_M: f64 = 2.0;
/// Its height, which is a lower hold's rather than a container's.
pub(super) const BULK_BLOCK_HEIGHT_M: f64 = 1.4;
/// How far forward of the cabin's aft end that block sits.
pub(super) const BULK_BLOCK_INSET_M: f64 = 1.5;
/// Below this the loader treats a position as unloaded, so nothing is drawn or
/// weighed for a container holding a kilogram of nothing.
pub(super) const MIN_PLACED_MASS_KG: f64 = 1.0;

/// How many monuments a cabin of this size carries.
pub(in super::super) struct MonumentCounts {
    /// Galleys installed.
    pub galleys: i64,
    /// Lavatories installed.
    pub lavatories: i64,
    /// Accessible lavatories installed.
    pub accessible_lavatories: i64,
    /// Wheelchair stowage positions installed.
    pub wheelchair_stowages: i64,
    /// Monuments the section could not hold and the layout therefore dropped.
    pub dropped: Vec<String>,
}

/// The galleys and lavatories a cabin of `total_pax` carries: the configured
/// counts, or the provisioning ratios above where a count is zero.
pub(in super::super) fn provisioned_monuments(
    pax: &PassengerCabinConfig,
    total_pax: i64,
) -> (i64, i64) {
    let lavatories = if pax.lavatory_count != 0 {
        pax.lavatory_count
    } else {
        ceil_div(total_pax, PAX_PER_LAV).max(1)
    };
    let galleys = if pax.galley_count != 0 {
        pax.galley_count
    } else {
        (ceil_div(total_pax, PAX_PER_GALLEY) + 1).max(1)
    };
    (galleys, lavatories)
}

/// How many monuments stand side by side across one bay of floor
/// `usable_width_m` wide that has to leave `n_aisles` aisles open; at least
/// one, so a narrow cabin still stacks its monuments rather than losing them.
pub(in super::super) fn monuments_per_bay(usable_width_m: f64, n_aisles: i64, aisle_w: f64) -> i64 {
    let across = usable_width_m - n_aisles.max(1) as f64 * aisle_w;
    let widest = GALLEY_WIDTH_M.max(LAV_WIDTH_M);
    ((across / widest).floor() as i64).max(1)
}

/// Distribute the galleys and lavatories across every bay.
///
/// A bay that receives more than one stacks them laterally inward from the
/// wall as a galley complex rather than placing two things at the same
/// coordinates, and galleys and lavatories take opposite sides of the
/// centreline.
pub(in super::super) fn place_monuments(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    bays: &mut [Bay],
    total_pax: i64,
    max_aisles: i64,
    enable_accessibility: bool,
) -> (Vec<DeckItem>, MonumentCounts) {
    let (galleys, lavatories) = provisioned_monuments(pax, total_pax);
    // A cabin bounded by declared doors draws each monument at its sourced
    // length; the generic cabin keeps its single bay length.
    let declared = !g.door_stations.is_empty();
    let length_of = |kind: MonumentKind| {
        if declared {
            kind.length_m()
        } else {
            MONUMENT_LEN
        }
    };

    let mut items = Vec::new();
    if bays.is_empty() {
        return (
            items,
            MonumentCounts {
                galleys,
                lavatories,
                accessible_lavatories: 0,
                wheelchair_stowages: 0,
                dropped: Vec::new(),
            },
        );
    }
    let fill_order = monument_fill_order(bays.len());
    // The passage a monument bay leaves open beside its monuments: the
    // cabin's own CS 25.815 aisle (12 in for a cabin of 19 or fewer seats).
    let passage = crate::cabin::resolve_aisle_width(pax, total_pax);
    let mut dropped = Vec::new();
    let (mut placed_galleys, mut placed_lavatories) = (0_i64, 0_i64);

    for (side, count, width, label, kind, monument) in [
        (
            MonumentSide::Galley,
            galleys,
            GALLEY_WIDTH_M,
            "Galley",
            ItemKind::Galley,
            MonumentKind::Galley,
        ),
        (
            MonumentSide::Lav,
            lavatories,
            LAV_WIDTH_M,
            "Lav",
            ItemKind::Lav,
            MonumentKind::Lavatory,
        ),
    ] {
        for i in 0..count.max(0) {
            let first = i as usize % fill_order.len();
            // A monument stands full height, so a bay is no wider than the
            // section at the monument's top corners and both ends. A bay the
            // section leaves whole keeps the stacking rule (the monument
            // narrows to the room left). A bay it narrows never takes a
            // sliver: the monument goes to the next bay in fill order that
            // holds it whole, and is dropped and reported when none does.
            //
            // A narrowed bay holds the monument whole when the width the
            // section leaves there, less what both walls already carry and
            // the CS 25.815 passage, takes it: a narrow cabin puts a single
            // monument against one wall and keeps the passage beside it (the
            // ATR 72-600 aft lavatory beside the aft baggage, factsheet 2020
            // p.22), so the monument may reach past the centreline when the
            // other wall leaves that floor free. Holding each wall to half
            // the bay instead dropped every monument of a section narrower
            // than twice the monument width.
            let mut chosen = None;
            let mut whole = None;
            for step in 0..fill_order.len() {
                let index = fill_order[(first + step) % fill_order.len()];
                let Some(deck) = g
                    .passenger_decks
                    .iter()
                    .find(|d| d.name == bays[index].deck)
                else {
                    continue;
                };
                let contained = contained_width(g, deck, bays[index].x, length_of(monument));
                let narrowed = contained < bays[index].width;
                // The stacking rule keeps each side to its half, so it only
                // applies while the other wall's monuments stay on theirs.
                let other_side = match side {
                    MonumentSide::Galley => bays[index].lav_depth,
                    MonumentSide::Lav => bays[index].galley_depth,
                };
                if step == 0 && !narrowed && other_side <= 0.5 * bays[index].width + 1e-9 {
                    whole = Some(index);
                }
                if chosen.is_none() && holds_across(&bays[index], contained, width, passage) {
                    chosen = Some(index);
                }
            }
            let Some(bay_index) = whole.or(chosen) else {
                dropped.push(format!(
                    "{label} not placed: the section is too narrow at every bay"
                ));
                continue;
            };
            let packs_across = whole.is_none();
            let bay = &mut bays[bay_index];
            let Some(deck) = g.passenger_decks.iter().find(|d| d.name == bay.deck) else {
                continue;
            };
            let bay_x = bay.x;
            let bay_deck = bay.deck;
            bay.width = bay
                .width
                .min(contained_width(g, deck, bay_x, length_of(monument)));
            let (offset, drawn_width) = if packs_across {
                pack_across(bay, side, width)
            } else {
                stack_y(bay, side, width)
            };
            let y = match side {
                MonumentSide::Galley => offset,
                MonumentSide::Lav => -offset,
            };
            let height = monument_height(
                g,
                deck,
                bay_x,
                length_of(monument),
                y.abs() + 0.5 * drawn_width,
            );
            match side {
                MonumentSide::Galley => placed_galleys += 1,
                MonumentSide::Lav => placed_lavatories += 1,
            }
            items.push(DeckItem {
                kind,
                deck: bay_deck,
                x: bay_x,
                y,
                z: g.floor_z(deck, bay_x) + 0.5 * height,
                length: length_of(monument),
                width: drawn_width,
                mass: 0.0,
                height,
                label: label.to_owned(),
                meta: ItemMeta::None,
            });
        }
    }
    let mut accessible_lavatories = 0;
    if enable_accessibility && max_aisles >= 2 {
        if let Some(index) = accessible_lav_candidate(&items) {
            let free = free_inward_width(&items, index);
            let lav = &mut items[index];
            lav.kind = ItemKind::AccessibleLav;
            lav.label = "Accessible lav".to_owned();
            if g.passenger_decks.iter().any(|deck| deck.name == lav.deck) {
                // Widened inward from the wall, into the floor the aisle side
                // of it has free, never across a neighbour or through the skin.
                let outer = lav.y.abs() + 0.5 * lav.width;
                let widened = lav.width.max(1.45_f64.min(free));
                lav.y = lav.y.signum() * (outer - 0.5 * widened);
                lav.width = widened;
                accessible_lavatories = 1;
            }
        }
    }

    let mut wheelchair_stowages = 0;
    if enable_accessibility && total_pax >= 100 {
        if let Some(bay) = bays.iter_mut().max_by(|a, b| {
            let available = |candidate: &Bay| {
                candidate.width * 0.5 - candidate.galley_depth.min(candidate.width * 0.5)
            };
            available(a).total_cmp(&available(b))
        }) {
            if let Some(deck) = g.passenger_decks.iter().find(|deck| deck.name == bay.deck) {
                // Contained against the lining at its own top, as every
                // other monument is at the seat box's.
                bay.width = bay.width.min(contained_width_to(
                    g,
                    deck,
                    bay.x,
                    length_of(MonumentKind::Galley),
                    WHEELCHAIR_STOWAGE_HEIGHT_M,
                ));
                let (offset, width) =
                    stack_y(bay, MonumentSide::Galley, WHEELCHAIR_STOWAGE_WIDTH_M);
                if width > 0.2 {
                    items.push(DeckItem {
                        kind: ItemKind::WheelchairStowage,
                        deck: bay.deck,
                        x: bay.x,
                        y: offset,
                        z: g.item_z(deck, bay.x, WHEELCHAIR_STOWAGE_HEIGHT_M),
                        // Sits in a galley's bay, so no longer than its sourced
                        // length on a declared-door deck (the generic bay
                        // length otherwise): longer, it reaches the next row.
                        length: length_of(MonumentKind::Galley),
                        width,
                        mass: 0.0,
                        height: g.clamp_height(deck, bay.x, WHEELCHAIR_STOWAGE_HEIGHT_M),
                        label: "Wheelchair stowage".to_owned(),
                        meta: ItemMeta::None,
                    });
                    wheelchair_stowages = 1;
                }
            }
        }
    }

    (
        items,
        MonumentCounts {
            galleys: placed_galleys,
            lavatories: placed_lavatories,
            accessible_lavatories,
            wheelchair_stowages,
            dropped,
        },
    )
}

/// Whether `bay`, whose section leaves `contained` m at the monument's
/// height, holds one more monument `width` wide whole: the width left once
/// both walls' monuments and the `passage` beside them are taken out.
fn holds_across(bay: &Bay, contained: f64, width: f64, passage: f64) -> bool {
    let free = bay.width.min(contained) - bay.galley_depth - bay.lav_depth - passage;
    free >= width - 1e-9
}

/// The offset from the centreline and the width of a monument packed inward
/// from its own wall in a bay [`holds_across`] admitted it to: drawn whole,
/// reaching past the centreline when its wall's share of the floor is the
/// larger one. `bay.width` is the section's width there.
fn pack_across(bay: &mut Bay, side: MonumentSide, width: f64) -> (f64, f64) {
    let consumed = match side {
        MonumentSide::Galley => &mut bay.galley_depth,
        MonumentSide::Lav => &mut bay.lav_depth,
    };
    let outer_edge = 0.5 * bay.width - *consumed;
    *consumed += width;
    (outer_edge - 0.5 * width, width)
}

/// How far inboard of its outer edge the lavatory at `index` can grow before
/// it meets another lavatory on its side of the bay or the aisle.
fn free_inward_width(items: &[DeckItem], index: usize) -> f64 {
    let lav = &items[index];
    let outer = lav.y.abs() + 0.5 * lav.width;
    let stop = items
        .iter()
        .enumerate()
        .filter(|(i, other)| {
            *i != index
                && other.kind == ItemKind::Lav
                && other.deck == lav.deck
                && (other.x - lav.x).abs() < 1e-9
                && other.y * lav.y > 0.0
                && other.y.abs() < lav.y.abs()
        })
        .map(|(_, other)| other.y.abs() + 0.5 * other.width)
        .fold(0.0_f64, f64::max);
    outer - stop.max(0.5 * MIN_BAY_AISLE_M)
}

/// The lavatory that can best be made accessible: the one with the most room
/// to grow inward.
fn accessible_lav_candidate(items: &[DeckItem]) -> Option<usize> {
    (0..items.len())
        .filter(|&i| items[i].kind == ItemKind::Lav)
        .max_by(|&a, &b| free_inward_width(items, a).total_cmp(&free_inward_width(items, b)))
}

/// Narrowest aisle a monument bay is assumed to keep open, m (CS 25.815 at
/// 20 or more seats).
const MIN_BAY_AISLE_M: f64 = 0.51;

/// Build longitudinally merged sidewall and centre overhead-bin runs.
///
/// One bin object per row makes a widebody preview need hundreds of extra
/// solids. Six-row segments preserve the visible breaks while keeping the
/// interactive scene small enough to orbit smoothly.
pub(in super::super) fn place_overhead_bins(
    g: &CabinGeometry,
    seats: &[DeckItem],
) -> Vec<DeckItem> {
    let mut bins = Vec::new();
    for deck in &g.passenger_decks {
        let mut rows: Vec<&DeckItem> = seats
            .iter()
            .filter(|item| item.kind == ItemKind::SeatRow && item.deck == deck.name)
            .collect();
        rows.sort_by(|a, b| a.x.total_cmp(&b.x));
        // A run is at most six rows and never spans a gap between two seat
        // stretches (a monument bay, a door or a declared zone): a monument
        // stands to the ceiling there.
        let mut segments: Vec<&[&DeckItem]> = Vec::new();
        let mut start = 0;
        for index in 1..=rows.len() {
            let breaks = index == rows.len()
                || index - start == BIN_ROWS_PER_SEGMENT
                || rows[index].x - rows[index - 1].x
                    > 1.25 * rows[index].length.max(rows[index - 1].length);
            if breaks {
                segments.push(&rows[start..index]);
                start = index;
            }
        }
        for segment in segments {
            let (Some(first), Some(last)) = (segment.first(), segment.last()) else {
                continue;
            };
            let x0 = first.x - first.length * 0.5;
            let x1 = last.x + last.length * 0.5;
            let x = 0.5 * (x0 + x1);
            let floor = g.floor_z(deck, x);
            let ceiling = g.ceil_z(deck, x);
            let available_height = ceiling - floor;
            let side_height =
                SIDE_BIN_HEIGHT_M.min((available_height - SIDE_BIN_BOTTOM_M).max(0.0));
            if side_height < 0.18 {
                continue;
            }
            let side_z = floor + SIDE_BIN_BOTTOM_M + side_height * 0.5;
            // The run is a straight solid: it has to fit the section at both
            // ends and at its top and bottom edges, not only at its middle.
            let crown_width = [x0, x, x1]
                .iter()
                .flat_map(|&xi| {
                    [side_z - 0.5 * side_height, side_z + 0.5 * side_height]
                        .map(|z| g.lining_chord_at_z(xi, z))
                })
                .fold(f64::INFINITY, f64::min);
            if crown_width > 2.0 * SIDE_BIN_DEPTH_M {
                let side_y = (crown_width - SIDE_BIN_DEPTH_M) * 0.5;
                for sign in [-1.0, 1.0] {
                    bins.push(DeckItem {
                        kind: ItemKind::OverheadBin,
                        deck: deck.name,
                        x,
                        y: sign * side_y,
                        z: side_z,
                        length: (x1 - x0).max(0.2),
                        width: SIDE_BIN_DEPTH_M,
                        mass: 0.0,
                        height: side_height,
                        label: "Sidewall pivot bin".to_owned(),
                        meta: ItemMeta::OverheadBin(OverheadBinMeta {
                            bin_type: OverheadBinType::Sidewall,
                        }),
                    });
                }
            }

            let center_meta = segment.iter().find_map(|row| match &row.meta {
                ItemMeta::Seat(meta) if meta.aisles >= 2 && meta.blocks.len() >= 3 => Some(meta),
                _ => None,
            });
            if let Some(meta) = center_meta {
                let center_width = (meta.blocks[1].max(1) as f64 * meta.seat_w * 0.72)
                    .clamp(0.65, 1.80)
                    .min((crown_width - 2.0 * SIDE_BIN_DEPTH_M).max(0.0));
                let center_height =
                    CENTER_BIN_HEIGHT_M.min((available_height - CENTER_BIN_BOTTOM_M).max(0.0));
                if center_width > 0.5 && center_height >= 0.16 {
                    bins.push(DeckItem {
                        kind: ItemKind::OverheadBin,
                        deck: deck.name,
                        x,
                        y: 0.0,
                        z: floor + CENTER_BIN_BOTTOM_M + center_height * 0.5,
                        length: (x1 - x0).max(0.2),
                        width: center_width,
                        mass: 0.0,
                        height: center_height,
                        label: "Center hinge bin".to_owned(),
                        meta: ItemMeta::OverheadBin(OverheadBinMeta {
                            bin_type: OverheadBinType::Center,
                        }),
                    });
                }
            }
        }
    }
    bins
}

/// What went into the holds.
pub(in super::super) struct Baggage {
    /// The containers and the bulk block, in placement order.
    pub items: Vec<DeckItem>,
    /// Checked baggage asked for.
    pub bag_mass: f64,
    /// Revenue freight that fitted alongside it.
    pub belly_cargo: f64,
    /// What the holds could take.
    pub hold_capacity: f64,
    /// What went into them, the overflow block included.
    pub hold_used: f64,
    /// Containers used.
    pub hold_ulds: i64,
    /// The installed lower holds and baggage compartments.
    pub installed: crate::cargo::CargoCapacity,
    /// Net baggage and freight mass per compartment, kg, forward to aft.
    pub compartment_masses_kg: Vec<(String, f64)>,
    /// Share of the net hold mass ahead of the wing box, 0 to 1.
    pub forward_fraction: f64,
    /// Mass stowed above every compartment limit, kg.
    pub overload_kg: f64,
}

// Assertions over bays constructed here; a failed unwrap is the assertion.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    /// A section 1.6 m wide holds one 0.90 m lavatory beside a 0.51 m
    /// passage, though the lavatory is wider than half the section, and the
    /// lavatory is drawn whole against its wall, reaching past the centreline.
    #[test]
    fn a_narrow_bay_takes_one_monument_whole_beside_the_passage() {
        let mut bay = Bay::new(10.0, crate::layout::MAIN, 1.6);
        assert!(holds_across(&bay, 1.6, LAV_WIDTH_M, 0.51));
        let (offset, width) = pack_across(&mut bay, MonumentSide::Lav, LAV_WIDTH_M);
        assert!((width - LAV_WIDTH_M).abs() < 1e-12);
        // Outer edge on the wall (0.8 m), inner edge 0.1 m past the centreline.
        assert!((offset - 0.35).abs() < 1e-12, "{offset}");
        assert!((bay.lav_depth - LAV_WIDTH_M).abs() < 1e-12);
        // What is left (1.6 - 0.9 - 0.51 = 0.19 m) holds no galley.
        assert!(!holds_across(&bay, 1.6, GALLEY_WIDTH_M, 0.51));
    }

    /// The section at the monument's height bounds the bay, not the floor
    /// width it was laid out with.
    #[test]
    fn the_contained_width_bounds_the_bay() {
        let bay = Bay::new(10.0, crate::layout::MAIN, 2.5);
        assert!(holds_across(&bay, 2.5, GALLEY_WIDTH_M, 0.51));
        assert!(!holds_across(&bay, 1.3, GALLEY_WIDTH_M, 0.51));
        assert!(holds_across(&bay, 1.3, GALLEY_WIDTH_M, 0.30));
    }
}
