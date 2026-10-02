// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

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
            },
        );
    }
    let fill_order = monument_fill_order(bays.len());

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
                length: length_of(monument),
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
    /// Net baggage and freight mass per compartment, kg, forward to aft.
    pub compartment_masses_kg: Vec<(String, f64)>,
    /// Share of the net hold mass ahead of the wing box, 0 to 1.
    pub forward_fraction: f64,
    /// Mass stowed above every compartment limit, kg.
    pub overload_kg: f64,
}
