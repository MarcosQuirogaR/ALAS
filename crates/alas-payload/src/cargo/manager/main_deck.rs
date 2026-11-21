// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The main deck of a converted freighter in the physical frame: which pallet
//! format and orientation this fuselage takes best, and how far each pallet's
//! netted load is built up under the section over it.

use super::*;

impl CargoLoadManager<'_> {
    /// Lay the main deck with the format that holds the most, returning how
    /// many positions it has.
    ///
    /// A configuration that names a pallet gets that pallet, lengthwise or
    /// turned a quarter turn; one that names none (`AUTO`) gets the best of the
    /// 88- and 96-by-125-inch pallets either way round. The comparison is the
    /// lower holds' one: net capacity at the stowage density, then volume,
    /// tare and type code.
    pub(super) fn select_main_deck_format(&mut self, deck: &DeckSpec) -> usize {
        let candidates: Vec<&'static UldType> = match super::super::uld(&self.config.main_deck_uld)
        {
            Some(requested) => std::iter::once(requested)
                .chain(turned_pallet(requested))
                .collect(),
            None => MAIN_DECK_AUTO_CANDIDATES.to_vec(),
        };
        let density = self
            .config
            .stowage_density_kg_m3
            .unwrap_or(crate::cargo::STOWAGE_DENSITY_KG_M3);
        let mut best: Option<(&'static UldType, Vec<CargoSlot>, f64, f64, f64)> = None;
        for candidate in candidates {
            let start = self.slots.len();
            self.main_deck_rows(deck, candidate);
            let slots = self.slots.split_off(start);
            if slots.is_empty() {
                continue;
            }
            let capacity = slots
                .iter()
                .map(|slot| slot.max_net().min(slot.usable_volume_m3() * density))
                .sum();
            let volume = slots.iter().map(CargoSlot::usable_volume_m3).sum();
            let tare = slots.len() as f64 * candidate.tare_weight;
            let replace = best.as_ref().is_none_or(
                |(best_type, _, best_capacity, best_volume, best_tare)| {
                    candidate_is_better(
                        (candidate, capacity, volume, tare),
                        (best_type, *best_capacity, *best_volume, *best_tare),
                    )
                },
            );
            if replace {
                best = Some((candidate, slots, capacity, volume, tare));
            }
        }
        let Some((_, slots, _, _, _)) = best else {
            return 0;
        };
        let placed = slots.len();
        self.slots.extend(slots);
        placed
    }

    /// Rows of `uld` down the main deck, from the cabin start aft. Neighbouring
    /// pallets along the deck keep the clearance they keep side by side
    /// ([`SLOT_GAP_M`]): one rule for the gap between adjacent positions.
    ///
    /// Two choices come from the fit, not from the aircraft. The first row slides
    /// aft, up to one pitch, to the first station where it takes as many
    /// positions across as the row behind it, instead of standing single where
    /// the nose section is a few centimetres short. And a pallet turned a quarter
    /// turn ([`turned_pallet`]) is stood ahead of the first row where the nose
    /// section takes it, as the drawn freighter arrangements do.
    fn main_deck_rows(&mut self, deck: &DeckSpec, uld: &'static UldType) {
        let g = self.geometry;
        let pitch = uld.length + SLOT_GAP_M;
        let first = g.cabin_start_x + MAIN_DECK_INSET_M + uld.length / 2.0;
        // A freighter's pallet deck runs aft to the pressure bulkhead, not to the end
        // of the passenger cabin: the section fit decides where a position stands.
        let x_end = g.cabin_end_x + crate::cargo::AFT_BULKHEAD_TAILCONE_FRACTION * g.tailcone_len
            - uld.length / 2.0;
        let across_at = |manager: &mut Self, x: f64| -> usize {
            let before = manager.slots.len();
            let across = manager.main_deck_row("", deck, x, uld);
            manager.slots.truncate(before);
            across
        };
        let mut start = first;
        for step in 0..=((pitch / ROW_SLIDE_STEP_M).floor() as usize) {
            let x = first + step as f64 * ROW_SLIDE_STEP_M;
            if across_at(self, x) >= across_at(self, x + pitch) {
                start = x;
                break;
            }
        }
        if let Some(turned) = turned_pallet(uld).filter(|turned| turned.length < uld.length) {
            let forward = g.x_min + crate::cargo::FLIGHT_DECK_BULKHEAD_M + turned.length / 2.0;
            let limit = start - uld.length / 2.0 - SLOT_GAP_M - turned.length / 2.0;
            let mut x = forward;
            while x <= limit {
                if let Some(height) = self.main_deck_height(deck, x, 0.0, turned) {
                    self.slots.push(CargoSlot {
                        sid: "MD-0-1".to_owned(),
                        deck: deck.name,
                        x,
                        y: 0.0,
                        uld: turned,
                        realized_height_m: height,
                        realized_width_m: turned.width,
                        payload: 0.0,
                        contour_volume_m3: Some(pallet_load_area_m2(turned) * height),
                        net_limit_kg: None,
                    });
                    break;
                }
                x += ROW_SLIDE_STEP_M;
            }
        }
        let mut x = start;
        for row in 0..MAX_MAIN_ROWS {
            if x > x_end {
                break;
            }
            self.main_deck_row(&format!("MD-{}-", row + 1), deck, x, uld);
            x += pitch;
        }
    }

    /// One transverse row at `x`: as many positions across as stand there side
    /// by side, centred on the aircraft axis, so a row where the section
    /// tapers below two across still takes one on the centre line.
    fn main_deck_row(
        &mut self,
        sid_prefix: &str,
        deck: &DeckSpec,
        x: f64,
        uld: &'static UldType,
    ) -> usize {
        let widest = transverse_centers(self.geometry.usable_width(deck, x), uld.width).len();
        for across in (1..=widest).rev() {
            let pitch = uld.width + SLOT_GAP_M;
            let centers: Vec<f64> = (0..across)
                .map(|index| (index as f64 - (across - 1) as f64 * 0.5) * pitch)
                .collect();
            let heights: Option<Vec<f64>> = centers
                .iter()
                .map(|&y| self.main_deck_height(deck, x, y, uld))
                .collect();
            let Some(heights) = heights else {
                continue;
            };
            for (index, (&y, height)) in centers.iter().zip(heights).enumerate() {
                self.slots.push(CargoSlot {
                    sid: format!("{sid_prefix}{}", index + 1),
                    deck: deck.name,
                    x,
                    y,
                    uld,
                    realized_height_m: height,
                    realized_width_m: uld.width,
                    payload: 0.0,
                    contour_volume_m3: is_pallet(uld).then(|| pallet_load_area_m2(uld) * height),
                    net_limit_kg: None,
                });
            }
            return across;
        }
        0
    }

    /// The height a main-deck position at (`x`, `y`) is loaded to, or `None`
    /// where it cannot stand: a pallet's built-up load follows the section,
    /// a rigid container needs its whole envelope clear.
    fn main_deck_height(&self, deck: &DeckSpec, x: f64, y: f64, uld: &UldType) -> Option<f64> {
        let g = self.geometry;
        if is_pallet(uld) {
            pallet_build_height(
                g,
                deck,
                x,
                y,
                uld,
                self.config.main_deck_door_clear_height_m,
            )
        } else if uld_fits(g, deck, x, y, uld, 0.0) {
            Some(realized_height(g, deck, x, uld).unwrap_or(uld.height))
        } else {
            None
        }
    }
}
