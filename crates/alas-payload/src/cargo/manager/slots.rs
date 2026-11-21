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
        let density = manager
            .config
            .stowage_density_kg_m3
            .unwrap_or(crate::cargo::STOWAGE_DENSITY_KG_M3);
        manager.limit_by_volume(density);
        manager
    }

    /// The positions with the mass limits of the frozen reference loader only:
    /// no stowage-density volume limit, as the reference-compatibility replay
    /// reproduces the frozen fixture.
    pub(crate) fn new_mass_limited(geometry: &'g CabinGeometry, config: CargoDeckConfig) -> Self {
        let mut manager = Self {
            geometry,
            config,
            slots: Vec::new(),
            lower_uld: LOWER_DECK_DEFAULT,
        };
        manager.build_slots();
        manager
    }

    /// A position holds at most its usable volume at `density_kg_m3`: a
    /// container's mass limit alone would let an air-cargo container take
    /// several times the density any cargo or baggage stows at.
    fn limit_by_volume(&mut self, density_kg_m3: f64) {
        // The hold compartments (declared, or integrated over the positions) are what
        // loose bulk fills: each bulk position takes its share of its compartment's
        // volume by length.
        let compartments = crate::cargo::derive_hold_compartments(
            self.geometry,
            &self.slots,
            None,
            &self.config.hold_compartments,
        );
        let compartment_of = |x: f64| compartments.iter().position(|c| c.contains(x));
        let bulk_length_in = |index: usize| -> f64 {
            self.slots
                .iter()
                .filter(|slot| slot.uld.code == BULK.code && compartments[index].contains(slot.x))
                .map(|slot| slot.uld.length)
                .sum()
        };
        // Containers standing in the same compartment take their own volume out of it.
        let container_volume_in = |index: usize| -> f64 {
            self.slots
                .iter()
                .filter(|slot| slot.uld.code != BULK.code && compartments[index].contains(slot.x))
                .map(CargoSlot::usable_volume_m3)
                .sum()
        };
        let shares: Vec<Option<f64>> = self
            .slots
            .iter()
            .map(|slot| {
                if slot.uld.code != BULK.code {
                    return None;
                }
                let index = compartment_of(slot.x)?;
                let length = bulk_length_in(index);
                // Loose bulk fills the stowage share of the net volume the built
                // hold encloses (crate::cargo::BULK_STOWAGE_FACTOR); a declared
                // compartment volume is already the usable one.
                let stowage = if compartments[index].volume_declared {
                    1.0
                } else {
                    crate::cargo::BULK_STOWAGE_FACTOR
                };
                (length > 0.0).then(|| {
                    (compartments[index].volume_m3 - container_volume_in(index)).max(0.0)
                        * stowage
                        * slot.uld.length
                        / length
                })
            })
            .collect();
        for (slot, share_m3) in self.slots.iter_mut().zip(shares) {
            // A military 463L pallet carries its declared rated load (4,536 kg on
            // 13.6 m3, 333 kg/m3: dense equipment); the civil default density does
            // not apply to a declared freight hold.
            if slot.uld.code == "463" {
                continue;
            }
            // A loose-bulk position conforms to the hold: it holds its share of the
            // compartment's volume, not the rectangular block's nominal volume, which
            // covers about half of it.
            let volume_m3 = share_m3.unwrap_or_else(|| slot.usable_volume_m3());
            // The same share is the position's usable volume: loose freight fills the
            // compartment cross-section, not the floor-width strip of the nominal block
            // (a 0.74 m hold floor made a 30 m3 hold report 7.6 m3), less the stowage
            // losses: the usable volume the airport-planning manuals publish for bulk
            // holds, against the water volume they print beside it.
            if share_m3.is_some() {
                slot.contour_volume_m3 = share_m3;
            }
            let by_volume = volume_m3 * density_kg_m3;
            slot.net_limit_kg = Some(slot.net_limit_kg.map_or(by_volume, |l| l.min(by_volume)));
        }
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
        self.row_with_slack(sid_prefix, deck, x, uld, 0.0)
    }

    /// [`Self::row`] where the lowest ceiling over a position may fall `height_slack_m`
    /// short of the container height.
    pub(super) fn row_with_slack(
        &mut self,
        sid_prefix: &str,
        deck: &DeckSpec,
        x: f64,
        uld: &'static UldType,
        height_slack_m: f64,
    ) -> usize {
        self.row_within(sid_prefix, deck, x, uld, height_slack_m, None)
    }

    /// [`Self::row_with_slack`] inside a clear width the source draws, m,
    /// when it is narrower than the deck: fewer containers stand across.
    pub(super) fn row_within(
        &mut self,
        sid_prefix: &str,
        deck: &DeckSpec,
        x: f64,
        uld: &'static UldType,
        height_slack_m: f64,
        clear_width_m: Option<f64>,
    ) -> usize {
        let usable = self.geometry.usable_width(deck, x);
        let usable = clear_width_m.map_or(usable, |width| usable.min(width));
        let candidates = if self.geometry.enforces_physical_envelope() {
            let mut centers =
                transverse_centers(usable, realized_width(self.geometry, deck, x, uld));
            // A symmetric contour wider at the top than its footprint cannot
            // stand beside a neighbour: the tops would overlap. Only the
            // mirrored, chamfer-outboard pairs interlock.
            if !uld.contour.mirrorable && uld.top_width > uld.width + 1e-9 && centers.len() > 1 {
                centers = vec![0.0];
            }
            // A mirrored pair interlocks (chamfers outboard, inboard faces
            // vertical); a third container between them would have its overhanging
            // top inside its neighbour. Two across at most.
            if uld.contour.mirrorable && uld.top_width > uld.width + 1e-9 && centers.len() > 2 {
                let pitch = realized_width(self.geometry, deck, x, uld) + SLOT_GAP_M;
                centers = vec![-0.5 * pitch, 0.5 * pitch];
            }
            centers
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
            .filter(|&y| uld_fits(self.geometry, deck, x, y, uld, height_slack_m))
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
                realized_width_m: if self.geometry.enforces_physical_envelope() {
                    realized_width(self.geometry, deck, x, uld)
                } else {
                    uld.width
                },
                payload: 0.0,
                contour_volume_m3: None,
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
        Some(freight_deck_spec(self.geometry, &hold))
    }

    /// Lay a freight hold's pallets in a single row down the floor and the
    /// ramp, returning how many positions fit.
    ///
    /// A pallet never straddles the ramp hinge: the floor takes as many as
    /// fit ahead of it and the ramp as many as fit aft of it, each ramp
    /// position limited to its share of the declared ramp load. The number
    /// across follows the narrower of the declared clear width and the lining's
    /// chord at the floor, not the outer loft.
    fn build_freight_hold_slots(&mut self) -> usize {
        let Some(hold) = self.config.freight_hold.filter(|hold| hold.is_valid()) else {
            return 0;
        };
        let Some(deck) = self.freight_deck() else {
            return 0;
        };
        let uld = crate::cargo::military_pallet::pallet_for_hold(
            uld_or(&self.config.main_deck_uld, MAIN_DECK_DEFAULT),
            hold.clear_width_m,
        );
        // A flat pallet is stacked to the height the hold has, not above it:
        // at most the declared clear height, and at each position the clear
        // height of the crown over its footprint, which falls away from the
        // station the declared height is given at.
        let declared_height = uld.height.min(hold.clear_height_m);
        let pitch = uld.length + MAIN_ROW_GAP_M;
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
                let loaded = UldType {
                    height: declared_height.min(crate::cargo::headroom::footprint_headroom(
                        self.geometry,
                        &deck,
                        x,
                        uld.length,
                    )),
                    ..*uld
                };
                // A pallet stands on the floor, so the width it has is the
                // narrower of the declared clear width at the floor and the
                // lining's chord at the floor, not the chord at the armrests
                // that an airliner's seats are held to. The declared width
                // alone let a pallet stand wider than the lofted lining at the
                // floor (the C-295's 88 in pallet on a 2.09 m chord).
                let chord = self
                    .geometry
                    .lining_chord_at_z(x, self.geometry.floor_z(&deck, x));
                let clear_width = hold.clear_width_m.min(chord);
                let across = (((clear_width + SLOT_GAP_M) / (uld.width + SLOT_GAP_M)).floor()
                    as i64)
                    .clamp(0, MAX_ACROSS);
                let centers: Vec<f64> = (0..across)
                    .map(|index| {
                        (index as f64 - (across - 1) as f64 * 0.5) * (uld.width + SLOT_GAP_M)
                    })
                    .collect();
                for (index, &y) in centers.iter().enumerate() {
                    if loaded.width > clear_width + SLOT_FIT_TOLERANCE_M || loaded.height <= 0.0 {
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
                        realized_height_m: loaded.height,
                        realized_width_m: uld.width,
                        payload: 0.0,
                        contour_volume_m3: None,
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
            if let Some(main_deck) = g
                .passenger_decks
                .iter()
                .find(|deck| deck.name == MAIN)
                .filter(|_| g.enforces_physical_envelope())
            {
                self.select_main_deck_format(main_deck);
            } else if let Some(main_deck) = g.passenger_decks.iter().find(|deck| deck.name == MAIN)
            {
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

        // In the bulk compartment the source declares, at its middle; otherwise
        // a fixed inset forward of the cabin's aft end. On a tapered tail the
        // inset can fall aft of the declared compartment, where the hold is
        // already too low (the A350-900's 1.91 m bulk compartment, FR82 to FR85).
        let declared_bulk = self.config.hold_compartments.iter().find(|hold| {
            hold.is_valid()
                && hold.deck == alas_config::HoldDeck::Lower
                && hold.name.to_ascii_lowercase().contains("bulk")
        });
        let bulk_x = declared_bulk.map_or(g.cabin_end_x - BULK_INSET_M, |hold| {
            0.5 * (hold.x_start_m + hold.x_end_m)
        });
        // The frozen compatibility grid always carries its loose bulk
        // position, drawn clamped to the hold height. A physical load plan
        // requires the bulk footprint to fit, and never double-books hold
        // space a rigid container slot already claims.
        let bulk_admitted = !g.enforces_physical_envelope()
            || (uld_fits(g, &g.lower_deck, bulk_x, 0.0, BULK, 0.0)
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
                realized_width_m: if g.enforces_physical_envelope() {
                    realized_width(g, &g.lower_deck, bulk_x, BULK)
                } else {
                    BULK.width
                },
                payload: 0.0,
                contour_volume_m3: None,
                net_limit_kg: None,
            });
        }
    }
}

/// The deck a freight hold's pallets stand on: the floor is placed so that the
/// declared clear height closes under the interior crown at the middle of the
/// floor. A freighter's section is drawn on this deck and not on the airliner
/// main-deck floor.
pub fn freight_deck_spec(geometry: &CabinGeometry, hold: &FreightHoldConfig) -> DeckSpec {
    let x_mid = hold.floor_start_x_m + 0.5 * hold.floor_length_m;
    let half_height = geometry.internal_half_height(x_mid);
    DeckSpec {
        name: MAIN,
        floor_frac: (half_height - hold.clear_height_m) / half_height,
        ceil_frac: 1.0,
        width_factor: 1.0,
        is_passenger: false,
    }
}
