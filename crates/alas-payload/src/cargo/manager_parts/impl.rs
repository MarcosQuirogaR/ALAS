// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

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
    fn row(&mut self, sid_prefix: &str, deck: &DeckSpec, x: f64, uld: &'static UldType) -> usize {
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
            .filter(|&y| self.uld_fits(deck, x, y, uld))
            .collect();
        for (i, y) in ys.iter().enumerate() {
            self.slots.push(CargoSlot {
                sid: format!("{sid_prefix}{}", i + 1),
                deck: deck.name,
                x,
                y: *y,
                uld,
                payload: 0.0,
            });
        }
        ys.len()
    }

    /// Whether the complete rigid ULD envelope stays inside both its deck and
    /// the fuselage lining over the full longitudinal footprint.
    fn uld_fits(&self, deck: &DeckSpec, x: f64, y: f64, uld: &UldType) -> bool {
        if !self.geometry.enforces_physical_envelope() {
            return self.geometry.deck_height(deck, x) >= uld.height;
        }
        let z_bottom = self.geometry.floor_z(deck, x);
        let half_length = uld.length * 0.5;
        let deck_clear = [x - half_length, x, x + half_length]
            .into_iter()
            .all(|sample_x| {
                z_bottom >= self.geometry.floor_z(deck, sample_x)
                    && z_bottom + uld.height <= self.geometry.ceil_z(deck, sample_x)
            });

        if !deck_clear {
            return false;
        }

        let fits_orientation = |mirrored| {
            let contour = uld.collision_contour(y, z_bottom, mirrored);
            self.geometry
                .check_polygon_containment(x - half_length, x + half_length, &contour)
                .is_ok()
        };
        fits_orientation(false) || (uld.contour.mirrorable && fits_orientation(true))
    }

    /// Fill the forward and aft lower holds with rows of one container type,
    /// returning how many positions were placed.
    fn fill_lower_holds(&mut self, uld: &'static UldType) -> usize {
        let g = self.geometry;
        let low = &g.lower_deck;
        let pitch = uld.length + LOWER_ROW_GAP_M;
        let (wing_box_start, wing_box_end) = g.wing_box_x_range();
        let mut placed = 0;

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
    fn lower_candidate_slots(&mut self, uld: &'static UldType) -> Vec<CargoSlot> {
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
                .filter_map(|key| super::uld(key))
                .filter(|candidate| candidate.code != requested.code),
        );

        let mut best: Option<(&'static UldType, Vec<CargoSlot>, f64, f64, f64)> = None;
        for candidate in candidates {
            let slots = self.lower_candidate_slots(candidate);
            if slots.is_empty() {
                continue;
            }
            let capacity = slots.len() as f64 * candidate.max_net();
            let volume = slots.len() as f64 * candidate.volume_m3;
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

    /// The main deck if this is a freighter, then the lower holds, then the
    /// one loose bulk position every aircraft has.
    fn build_slots(&mut self) {
        let g = self.geometry;

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
                    .filter_map(|key| super::uld(key))
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
            || (self.uld_fits(&g.lower_deck, bulk_x, 0.0, BULK)
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
                payload: 0.0,
            });
        }
    }

    /// Loaded mass, its longitudinal centre, and how many positions carry it.
    pub fn mass_props(&self) -> (f64, f64, usize) {
        let mut mass = 0.0;
        let mut moment = 0.0;
        let mut used = 0;
        for slot in &self.slots {
            let weight = slot.total_weight();
            if weight > 0.0 {
                mass += weight;
                moment += weight * slot.x;
                used += 1;
            }
        }
        let cg = if mass > 0.0 { moment / mass } else { 0.0 };
        (mass, cg, used)
    }

    /// What every position together could hold, containers excluded.
    pub fn total_capacity(&self) -> f64 {
        self.slots.iter().map(CargoSlot::max_net).sum()
    }

    /// Empty every position.
    pub fn clear(&mut self) {
        for slot in &mut self.slots {
            slot.payload = 0.0;
        }
    }

    /// Distribute `target_mass` of net cargo, trimming toward `target_cg`.
    ///
    /// `priority` ranks the positions, smallest loaded first. `fill_full` loads
    /// each position to its limit in that order; the alternative spreads the
    /// load evenly over every position regardless of the ranking, which is what
    /// the uniform strategy asks for.
    ///
    /// More net payload than the positions can hold is clamped to their net
    /// capacity rather than refused: the caller reports the shortfall, and a
    /// freighter asked for more than it can carry still has a load plan for
    /// what it can. ULD tare is retained in [`Self::mass_props`] for CG and
    /// aircraft mass, but never deducted from this requested net load.
    pub fn solve(
        &mut self,
        target_mass: f64,
        target_cg: f64,
        priority: &dyn Fn(&CargoSlot) -> f64,
        fill_full: bool,
    ) {
        self.solve_with_mass_semantics(
            target_mass,
            target_cg,
            priority,
            fill_full,
            CargoMassSemantics::Net,
        );
    }

    /// Reproduce the frozen loader's gross-target correction for parity
    /// fixtures. Product analyses must use [`Self::solve`], whose request is a
    /// net-cargo quantity.
    pub(crate) fn solve_reference_compatibility(
        &mut self,
        target_mass: f64,
        target_cg: f64,
        priority: &dyn Fn(&CargoSlot) -> f64,
        fill_full: bool,
    ) {
        self.solve_with_mass_semantics(
            target_mass,
            target_cg,
            priority,
            fill_full,
            CargoMassSemantics::ReferenceGross,
        );
    }

    fn solve_with_mass_semantics(
        &mut self,
        target_mass: f64,
        target_cg: f64,
        priority: &dyn Fn(&CargoSlot) -> f64,
        fill_full: bool,
        semantics: CargoMassSemantics,
    ) {
        self.clear();
        if self.slots.is_empty() {
            return;
        }
        let by_priority = self.ranked(priority);
        let target_net_mass = target_mass.max(0.0).min(self.total_capacity());
        let target_closure_mass = match semantics {
            CargoMassSemantics::Net => target_net_mass,
            CargoMassSemantics::ReferenceGross => target_mass.max(0.0).min(self.total_capacity()),
        };

        if fill_full {
            let mut remaining = target_net_mass;
            for &i in &by_priority {
                if remaining <= 0.0 {
                    break;
                }
                let add = remaining.min(self.slots[i].max_net());
                self.slots[i].payload = add;
                remaining -= add;
            }
        } else {
            let per_slot = target_net_mass / self.slots.len() as f64;
            for slot in &mut self.slots {
                slot.payload = per_slot.min(slot.uld.max_net());
            }
        }

        let step = self.config.cg_trim_step_kg;
        for _ in 0..self.config.cg_trim_max_iterations.max(0) {
            let (current_gross_mass, current_cg, _) = self.mass_props();
            let current_closure_mass = match semantics {
                CargoMassSemantics::Net => self.net_payload_mass(),
                CargoMassSemantics::ReferenceGross => current_gross_mass,
            };
            self.correct_total(
                target_closure_mass - current_closure_mass,
                current_cg - target_cg,
            );

            let error = current_cg - target_cg;
            if error.abs() < CG_SETTLED_M
                && (target_closure_mass - current_closure_mass).abs() < MASS_CONVERGED_KG
            {
                break;
            }
            if !self.shift_toward_target(error, current_cg, step, priority) {
                break;
            }
        }
    }

    /// Net cargo currently carried by loaded positions, excluding ULD tare.
    fn net_payload_mass(&self) -> f64 {
        self.slots
            .iter()
            .filter(|slot| slot.payload > super::MIN_LOADED_KG)
            .map(|slot| slot.payload)
            .sum()
    }

    /// Position indices ranked by `priority`, best first.
    ///
    /// The sort is stable, as Python's is, so positions that rank equally stay
    /// in the order the decks were walked -- which is what decides the load
    /// plan whenever a strategy ranks a whole hold alike.
    fn ranked(&self, priority: &dyn Fn(&CargoSlot) -> f64) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.slots.len()).collect();
        order.sort_by(|&a, &b| priority(&self.slots[a]).total_cmp(&priority(&self.slots[b])));
        order
    }

    /// The first position among `indices` with the largest priority value.
    ///
    /// Upstream reaches it by sorting descending and taking the head, and
    /// Python's descending sort is stable, so ties go to whichever position the
    /// decks were walked first rather than last.
    fn worst_ranked(
        &self,
        indices: &[usize],
        priority: &dyn Fn(&CargoSlot) -> f64,
    ) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for &i in indices {
            let value = priority(&self.slots[i]);
            if best.is_none_or(|(_, current)| value > current) {
                best = Some((i, value));
            }
        }
        best.map(|(i, _)| i)
    }

    /// The first position among `indices` with the smallest priority value.
    fn best_ranked(
        &self,
        indices: &[usize],
        priority: &dyn Fn(&CargoSlot) -> f64,
    ) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for &i in indices {
            let value = priority(&self.slots[i]);
            if best.is_none_or(|(_, current)| value < current) {
                best = Some((i, value));
            }
        }
        best.map(|(i, _)| i)
    }

}

include!("part_02.rs");
