// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

impl<'g> CargoLoadManager<'g> {
    /// Build the positions this fuselage and configuration admit.
    pub fn new(geometry: &'g CabinGeometry, config: CargoDeckConfig) -> Self {
        let mut manager = Self {
            geometry,
            config,
            slots: Vec::new(),
            lower_uld: LOWER_DECK_DEFAULT,
        };
        manager.build_slots();
        manager
    }

    /// Place a transverse row of containers across a deck at station `x`,
    /// returning how many positions it produced.
    ///
    /// The fit check is what makes this return nothing rather than something
    /// unloadable: a station whose cross-section cannot take the container in
    /// width or in height gets no positions at all.
    pub(super) fn row(
        &mut self,
        sid_prefix: &str,
        deck: &DeckSpec,
        x: f64,
        uld: &'static UldType,
    ) -> usize {
        let usable = self.geometry.usable_width(deck, x);
        let candidates = if self.geometry.enforces_physical_envelope() {
            transverse_centers(usable, uld.width)
        } else {
            match (floor_div(usable, uld.width) as i64).clamp(0, MAX_ACROSS) {
                0 => Vec::new(),
                1 => vec![0.0],
                2 => vec![
                    -(uld.width / 2.0 + SLOT_GAP_M),
                    uld.width / 2.0 + SLOT_GAP_M,
                ],
                _ => vec![-(uld.width + SLOT_GAP_M), 0.0, uld.width + SLOT_GAP_M],
            }
        };
        let ys: Vec<f64> = candidates
            .into_iter()
            .filter(|&y| uld_fits(self.geometry, deck, x, y, uld))
            .collect();
        for (i, y) in ys.iter().enumerate() {
            self.slots.push(CargoSlot {
                sid: format!("{sid_prefix}{}", i + 1),
                deck: deck.name,
                x,
                y: *y,
                uld,
                realized_height_m: if self.geometry.enforces_physical_envelope() {
                    realized_height(self.geometry, deck, x, uld).unwrap_or(uld.height)
                } else {
                    uld.height
                },
                payload: 0.0,
                net_limit_kg: None,
            });
        }
        ys.len()
    }

    /// Fill the forward and aft lower holds with rows of one container type,
    /// returning how many positions were placed.
    fn fill_lower_holds(&mut self, uld: &'static UldType) -> usize {
        if let Some(placed) = self.fill_declared_holds(uld) {
            return placed;
        }
        let (g, mut placed) = (self.geometry, 0);
        let (low, pitch) = (&g.lower_deck, uld.length + LOWER_ROW_GAP_M);
        let (wing_box_start, wing_box_end) = g.lower_hold_gap_x_range();
        let mut x = g.cabin_start_x + FWD_HOLD_INSET_M + uld.length / 2.0;
        for i in 0..MAX_LOWER_ROWS {
            if x > wing_box_start - uld.length / 2.0 {
                break;
            }
            placed += self.row(&format!("FWD-{}-", i + 1), low, x, uld);
            x += pitch;
        }

        let mut x = wing_box_end + uld.length / 2.0 + AFT_HOLD_INSET_M;
        let x_end = g.cabin_end_x - uld.length / 2.0;
        for i in 0..MAX_LOWER_ROWS {
            if x > x_end {
                break;
            }
            placed += self.row(&format!("AFT-{}-", i + 1), low, x, uld);
            x += pitch;
        }
        placed
    }

    /// Generate and detach one complete uniform-format lower-hold layout.
    pub(super) fn lower_candidate_slots(&mut self, uld: &'static UldType) -> Vec<CargoSlot> {
        let start = self.slots.len();
        self.fill_lower_holds(uld);
        self.slots.split_off(start)
    }

    /// Physical-mode lower-hold choice. Capacity dominates nominal volume;
    /// equal capacity and volume prefer less installed tare, then type code.
    fn select_lower_format(&mut self) {
        let requested = uld_or(&self.config.lower_deck_uld, LOWER_DECK_DEFAULT);
        let mut candidates = vec![requested];
        candidates.extend(
            LOWER_HOLD_AUTO_CANDIDATES
                .iter()
                .filter_map(|key| super::super::uld(key))
                .filter(|candidate| candidate.code != requested.code),
        );

        let mut best: Option<(&'static UldType, Vec<CargoSlot>, f64, f64, f64)> = None;
        for candidate in candidates {
            let slots = self.lower_candidate_slots(candidate);
            if slots.is_empty() {
                continue;
            }
            let capacity = slots.iter().map(CargoSlot::max_net).sum();
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

        if let Some((selected, slots, _, _, _)) = best {
            self.lower_uld = selected;
            self.slots.extend(slots);
        } else {
            self.lower_uld = requested;
        }
    }

    /// The deck a freight hold's pallets stand on, or `None` without a
    /// declared (valid) freight hold.
    ///
    /// The floor is not the airliner main-deck floor at the cabin axis: it is
    /// placed so that the declared clear height closes under the interior
    /// crown at the middle of the floor.
    pub fn freight_deck(&self) -> Option<DeckSpec> {
        let hold = self.config.freight_hold.filter(|hold| hold.is_valid())?;
        let x_mid = hold.floor_start_x_m + 0.5 * hold.floor_length_m;
        let half_height = self.geometry.internal_half_height(x_mid);
        Some(DeckSpec {
            name: MAIN,
            floor_frac: (half_height - hold.clear_height_m) / half_height,
            ceil_frac: 1.0,
            width_factor: 1.0,
            is_passenger: false,
        })
    }

    /// Lay a freight hold's pallets in a single row down the floor and the
    /// ramp, returning how many positions fit.
    ///
    /// A pallet never straddles the ramp hinge: the floor takes as many as
    /// fit ahead of it and the ramp as many as fit aft of it, each ramp
    /// position limited to its share of the declared ramp load. The number
    /// across follows the declared clear width, not the outer loft.
    fn build_freight_hold_slots(&mut self) -> usize {
        let Some(hold) = self.config.freight_hold.filter(|hold| hold.is_valid()) else {
            return 0;
        };
        let Some(deck) = self.freight_deck() else {
            return 0;
        };
        let uld = uld_or(&self.config.main_deck_uld, MAIN_DECK_DEFAULT);
        let pitch = uld.length + MAIN_ROW_GAP_M;
        let across = (((hold.clear_width_m + SLOT_GAP_M) / (uld.width + SLOT_GAP_M)).floor()
            as i64)
            .clamp(0, MAX_ACROSS);
        let centers: Vec<f64> = (0..across)
            .map(|index| (index as f64 - (across - 1) as f64 * 0.5) * (uld.width + SLOT_GAP_M))
            .collect();
        let zones = [
            ("MD", hold.floor_start_x_m, hold.ramp_start_x_m(), false),
            ("RAMP", hold.ramp_start_x_m(), hold.aft_end_x_m(), true),
        ];
        let mut ramp_slots = Vec::new();
        let mut placed = 0;
        for (prefix, x_start, x_end, on_ramp) in zones {
            let mut x = x_start + 0.5 * uld.length;
            let mut row = 0;
            while x + 0.5 * uld.length <= x_end + SLOT_FIT_TOLERANCE_M && row < MAX_MAIN_ROWS {
                row += 1;
                for (index, &y) in centers.iter().enumerate() {
                    if !uld_fits(self.geometry, &deck, x, y, uld) {
                        continue;
                    }
                    if on_ramp {
                        ramp_slots.push(self.slots.len());
                    }
                    self.slots.push(CargoSlot {
                        sid: format!("{prefix}-{row}-{}", index + 1),
                        deck: MAIN,
                        x,
                        y,
                        uld,
                        realized_height_m: uld.height,
                        payload: 0.0,
                        net_limit_kg: None,
                    });
                    placed += 1;
                }
                x += pitch;
            }
        }
        if let Some(limit) = hold.ramp_load_limit_kg {
            let share = limit / ramp_slots.len().max(1) as f64;
            for index in ramp_slots {
                self.slots[index].net_limit_kg = Some(share);
            }
        }
        placed
    }

    /// The main deck if this is a freighter, then the lower holds, then the
    /// one loose bulk position every aircraft has.
    fn build_slots(&mut self) {
        let g = self.geometry;

        // A declared freight hold is the whole payload space: no underfloor
        // hold and no loose bulk position.
        if self.config.use_main_deck && self.build_freight_hold_slots() > 0 {
            return;
        }

        if self.config.use_main_deck {
            if let Some(main_deck) = g.passenger_decks.iter().find(|deck| deck.name == MAIN) {
                let uld = uld_or(&self.config.main_deck_uld, MAIN_DECK_DEFAULT);
                let pitch = uld.length + MAIN_ROW_GAP_M;
                let mut x = g.cabin_start_x + MAIN_DECK_INSET_M + uld.length / 2.0;
                let x_end = g.cabin_end_x - uld.length / 2.0;
                for i in 0..MAX_MAIN_ROWS {
                    if x > x_end {
                        break;
                    }
                    self.row(&format!("MD-{}-", i + 1), main_deck, x, uld);
                    x += pitch;
                }
            }
        }

        if g.enforces_physical_envelope() && self.config.lower_deck_uld.eq_ignore_ascii_case("auto")
        {
            self.select_lower_format();
        } else if g.enforces_physical_envelope()
            && self.config.lower_deck_uld.eq_ignore_ascii_case("BLK")
        {
            // A bulk-only hold has no installed container loading system.
            // Dimensional acceptance of an AKH cannot add that equipment.
            self.lower_uld = BULK;
            self.fill_lower_holds(BULK);
        } else {
            self.lower_uld = uld_or(&self.config.lower_deck_uld, LOWER_DECK_DEFAULT);
            let mut candidates = vec![self.lower_uld];
            candidates.extend(
                LOWER_HOLD_FALLBACKS
                    .iter()
                    .filter_map(|key| super::super::uld(key))
                    .filter(|entry| entry.code != self.lower_uld.code),
            );
            for candidate in candidates {
                if self.fill_lower_holds(candidate) > 0 {
                    self.lower_uld = candidate;
                    break;
                }
            }
        }

        let bulk_x = g.cabin_end_x - BULK_INSET_M;
        // The frozen compatibility grid always carries its loose bulk
        // position, drawn clamped to the hold height. A physical load plan
        // requires the bulk footprint to fit, and never double-books hold
        // space a rigid container slot already claims.
        let bulk_admitted = !g.enforces_physical_envelope()
            || (uld_fits(g, &g.lower_deck, bulk_x, 0.0, BULK)
                && !self.slots.iter().any(|slot| {
                    slot.deck == g.lower_deck.name
                        && (slot.x - bulk_x).abs() < (slot.uld.length + BULK.length) * 0.5
                        && slot.y.abs() < (slot.uld.width + BULK.width) * 0.5
                }));
        if bulk_admitted {
            self.slots.push(CargoSlot {
                sid: "BULK".to_owned(),
                deck: g.lower_deck.name,
                x: bulk_x,
                y: 0.0,
                uld: BULK,
                realized_height_m: if g.enforces_physical_envelope() {
                    realized_height(g, &g.lower_deck, bulk_x, BULK).unwrap_or(BULK.height)
                } else {
                    BULK.height
                },
                payload: 0.0,
                net_limit_kg: None,
            });
        }
    }
}
