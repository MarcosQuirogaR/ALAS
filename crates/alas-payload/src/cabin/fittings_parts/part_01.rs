// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_config::{CargoDeckConfig, CertifiedExitLayout, DesignRequirements, PassengerCabinConfig};

use super::seating::Seating;
use super::{
    cabin_deck_segments, ceil_div, effective_pair_capacity, min_exit_pairs, monument_fill_order,
    select_exit_type, spread_bay_indices, stack_y, Bay, ExitSpec, MonumentSide, EXIT_TYPES,
    MONUMENT_LEN, SEAT_BOX_H,
};
use crate::cargo::{CargoLoadManager, CargoMassSemantics};
use crate::geometry::CabinGeometry;
use crate::layout::{
    ContainerMeta, DeckItem, ExitMeta, ItemKind, ItemMeta, OverheadBinMeta, OverheadBinType, LOWER,
};

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

/// Longitudinal extent of the loose bulk block the overflow guard places.
const BULK_BLOCK_LEN_M: f64 = 2.0;
/// Its height, which is a lower hold's rather than a container's.
const BULK_BLOCK_HEIGHT_M: f64 = 1.4;
/// How far forward of the cabin's aft end that block sits.
const BULK_BLOCK_INSET_M: f64 = 1.5;
/// Below this the loader treats a position as unloaded, so nothing is drawn or
/// weighed for a container holding a kilogram of nothing.
const MIN_PLACED_MASS_KG: f64 = 1.0;

/// How many monuments a cabin of this size carries.
pub(super) struct MonumentCounts {
    /// Galleys installed.
    pub galleys: i64,
    /// Lavatories installed.
    pub lavatories: i64,
    /// Accessible lavatories installed.
    pub accessible_lavatories: i64,
    /// Wheelchair stowage positions installed.
    pub wheelchair_stowages: i64,
}

/// Distribute the galleys and lavatories across every bay.
///
/// A bay that receives more than one stacks them laterally inward from the
/// wall as a galley complex rather than placing two things at the same
/// coordinates, and galleys and lavatories take opposite sides of the
/// centreline.
pub(super) fn place_monuments(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    bays: &mut [Bay],
    total_pax: i64,
    max_aisles: i64,
    enable_accessibility: bool,
) -> (Vec<DeckItem>, MonumentCounts) {
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

    let mut items = Vec::new();
    if bays.is_empty() {
        return (
            items,
            MonumentCounts {
                galleys,
                lavatories,
                accessible_lavatories: 0,
                wheelchair_stowages: 0,
            },
        );
    }
    let fill_order = monument_fill_order(bays.len());

    for (side, count, width, label, kind) in [
        (
            MonumentSide::Galley,
            galleys,
            GALLEY_WIDTH_M,
            "Galley",
            ItemKind::Galley,
        ),
        (
            MonumentSide::Lav,
            lavatories,
            LAV_WIDTH_M,
            "Lav",
            ItemKind::Lav,
        ),
    ] {
        for i in 0..count.max(0) {
            let bay_index = fill_order[i as usize % fill_order.len()];
            let bay = &mut bays[bay_index];
            // Bays are only ever carved out of a passenger deck, so a bay
            // naming a deck the geometry does not carry cannot arise.
            let Some(deck) = g.passenger_decks.iter().find(|d| d.name == bay.deck) else {
                continue;
            };
            let bay_x = bay.x;
            let bay_deck = bay.deck;
            let (offset, drawn_width) = stack_y(bay, side, width);
            let y = match side {
                MonumentSide::Galley => offset,
                MonumentSide::Lav => -offset,
            };
            items.push(DeckItem {
                kind,
                deck: bay_deck,
                x: bay_x,
                y,
                z: g.item_z(deck, bay_x, SEAT_BOX_H),
                length: MONUMENT_LEN,
                width: drawn_width,
                mass: 0.0,
                height: g.clamp_height(deck, bay_x, SEAT_BOX_H),
                label: label.to_owned(),
                meta: ItemMeta::None,
            });
        }
    }
    let mut accessible_lavatories = 0;
    if enable_accessibility && max_aisles >= 2 {
        if let Some(lav) = items.iter_mut().find(|item| item.kind == ItemKind::Lav) {
            lav.kind = ItemKind::AccessibleLav;
            lav.label = "Accessible lav".to_owned();
            if let Some(deck) = g.passenger_decks.iter().find(|deck| deck.name == lav.deck) {
                lav.width = lav.width.max(1.45).min(g.usable_width(deck, lav.x) * 0.5);
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
                let (offset, width) =
                    stack_y(bay, MonumentSide::Galley, WHEELCHAIR_STOWAGE_WIDTH_M);
                if width > 0.2 {
                    items.push(DeckItem {
                        kind: ItemKind::WheelchairStowage,
                        deck: bay.deck,
                        x: bay.x,
                        y: offset,
                        z: g.item_z(deck, bay.x, 1.05),
                        length: MONUMENT_LEN,
                        width,
                        mass: 0.0,
                        height: g.clamp_height(deck, bay.x, 1.05),
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
            galleys,
            lavatories,
            accessible_lavatories,
            wheelchair_stowages,
        },
    )
}

/// Build longitudinally merged sidewall and centre overhead-bin runs.
///
/// One bin object per row makes a widebody preview need hundreds of extra
/// solids. Six-row segments preserve the visible breaks while keeping the
/// interactive scene small enough to orbit smoothly.
pub(super) fn place_overhead_bins(g: &CabinGeometry, seats: &[DeckItem]) -> Vec<DeckItem> {
    let mut bins = Vec::new();
    for deck in &g.passenger_decks {
        let mut rows: Vec<&DeckItem> = seats
            .iter()
            .filter(|item| item.kind == ItemKind::SeatRow && item.deck == deck.name)
            .collect();
        rows.sort_by(|a, b| a.x.total_cmp(&b.x));
        for segment in rows.chunks(BIN_ROWS_PER_SEGMENT) {
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
            let crown_width = g.usable_width_at_z(x, side_z);
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

/// The emergency exits, and how many pairs were installed.
pub(super) struct Exits {
    /// The door cutouts, in placement order.
    pub items: Vec<DeckItem>,
    /// The type every door was drawn at.
    pub exit_type: &'static str,
    /// Pairs installed, summed over the passenger decks.
    pub pairs: i64,
    /// Sum of the ratings of the installed complete exit pairs.
    pub capacity_total: i64,
}

/// Size and place one exit-pair set per passenger deck.
///
/// A double-decker's upper deck needs its own evacuation route, so this runs
/// per deck rather than once for the aircraft, and each deck is sized from the
/// passengers actually seated on it after the capacity cap -- never from the
/// raw requested total, which is what a deck that could not seat them all would
/// otherwise be given doors for.
pub(super) fn place_exits(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    seating: &Seating,
    product_exit_capacity: bool,
    source_exit_layout: Option<CertifiedExitLayout>,
) -> Exits {
    let default_spec = select_exit_type(g.diameter_m);
    let default_capacity_per_pair = if product_exit_capacity {
        effective_pair_capacity(default_spec, pax)
    } else {
        // Frozen Python compatibility treated the pair table as a side
        // quantity.  Keep that historical branch isolated from the product
        // path's corrected complete-pair unit.
        default_spec.capacity_per_pair
    };
    let mut items = Vec::new();
    let mut pairs = 0i64;
    let mut capacity_total = 0i64;

    for segment in cabin_deck_segments(g) {
        let deck = segment.deck;
        let deck_pax = seating.on_deck(deck.name);
        if deck_pax <= 0 {
            continue;
        }
        let n_pairs = if let Some(source_exit_layout) = source_exit_layout {
            let max_source_pair_capacity = source_exit_layout
                .pairs
                .iter()
                .map(|pair| pair.capacity_per_pair.max(1))
                .max()
                .unwrap_or(default_capacity_per_pair.max(1));
            ceil_div(deck_pax, max_source_pair_capacity)
                .max(min_exit_pairs(deck_pax))
                .clamp(1, source_exit_layout.pairs.len().max(1) as i64)
        } else {
            min_exit_pairs(deck_pax).max(ceil_div(deck_pax, default_capacity_per_pair.max(1)))
        };

        let mut deck_bays: Vec<&Bay> = seating
            .bays
            .iter()
            .filter(|bay| bay.deck == deck.name)
            .collect();
        deck_bays.sort_by(|a, b| a.x.total_cmp(&b.x));
        // A source arrangement has a fixed pair order.  Use the lower bay at
        // an exact midpoint instead of the generic banker's rounding: for
        // four model bays and three source pairs, rounding 1.5 upward would
        // select stations [0, 2, 3] and leave an avoidable over-spacing gap,
        // while [0, 1, 3] preserves the same physical bay candidates.
        let bay_indices = if source_exit_layout.is_some() {
            source_spread_bay_indices(n_pairs, deck_bays.len())
        } else {
            spread_bay_indices(n_pairs, deck_bays.len())
        };
        let mut exit_xs: Vec<f64> = bay_indices
            .into_iter()
            .map(|i| deck_bays[i].x)
            .collect();

        // More pairs than there are bays to hang them on: the remainder is
        // spread along the whole segment instead, a metre in from each end.
        if n_pairs > exit_xs.len() as i64 {
            let extra = n_pairs - exit_xs.len() as i64;
            let ex0 = segment.x0 - MONUMENT_LEN + 1.0;
            let ex1 = segment.x1 + MONUMENT_LEN - 1.0;
            exit_xs.extend((0..extra).map(|i| ex0 + (ex1 - ex0) * (i as f64 + 0.5) / extra as f64));
        }

        for (pair_index, xe) in exit_xs.into_iter().enumerate() {
            let pair_spec = source_exit_layout
                .and_then(|layout| layout.pairs.get(pair_index))
                .and_then(|pair| find_exit_spec(pair.exit_type))
                .unwrap_or(default_spec);
            let pair_capacity = source_exit_layout
                .and_then(|layout| layout.pairs.get(pair_index))
                .map_or(
                    if product_exit_capacity {
                        default_capacity_per_pair
                    } else {
                        // The frozen summary recorded two side ratings per
                        // pair; this conversion is deliberately confined to
                        // the compatibility branch.
                        default_capacity_per_pair * 2
                    },
                    |pair| pair.capacity_per_pair.max(0),
                );
            capacity_total += pair_capacity;
            let half = g.usable_width(deck, xe) / 2.0 + g.wall;
            for side in [-1.0, 1.0] {
                items.push(DeckItem {
                    kind: ItemKind::Exit,
                    deck: deck.name,
                    x: xe,
                    y: side * half,
                    z: g.item_z(deck, xe, pair_spec.height_m),
                    length: pair_spec.width_m,
                    width: 0.25,
                    mass: 0.0,
                    height: g.clamp_height(deck, xe, pair_spec.height_m),
                    label: format!("Type {}", pair_spec.name),
                    meta: ItemMeta::Exit(ExitMeta {
                        exit_type: pair_spec.name,
                        door_w: pair_spec.width_m,
                        door_h: pair_spec.height_m,
                    }),
                });
            }
        }
        pairs += n_pairs;
    }

    Exits {
        items,
        exit_type: source_exit_layout.map_or(default_spec.name, |layout| layout.label),
        pairs,
        capacity_total,
    }
}

/// Look up a source arrangement's class in the generic drawing dimensions.
fn find_exit_spec(name: &str) -> Option<&'static ExitSpec> {
    EXIT_TYPES.iter().find(|spec| spec.name == name)
}

/// Spread a registered source arrangement across existing bay stations.
///
/// The source sequence fixes the number and order of exit pairs, while the
/// model's monument/seat pass supplies the available stations.  Choosing the
/// lower integer at an exact midpoint keeps a source pair away from the aft
/// endpoint when the candidate count is even; the registered A220/A320
/// layouts then satisfy CS-25's 18.3 m adjacent-exit spacing check without
/// inventing a longitudinal station.  The final spacing assertion belongs in
/// the source-specific acceptance test because this helper has no fuselage
/// frame or regulatory applicability context.
fn source_spread_bay_indices(n_items: i64, n_bays: usize) -> Vec<usize> {
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
        let mut idx = exact.floor() as usize;
        while out.contains(&idx) && idx < n_bays - 1 {
            idx += 1;
        }
        out.push(idx);
    }
    out
}

/// What went into the holds.
pub(super) struct Baggage {
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
}
