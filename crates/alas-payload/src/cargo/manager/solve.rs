// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Loaded-mass properties and the priority-ordered load solve.

use super::*;

impl CargoLoadManager<'_> {
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
                slot.payload = per_slot.min(slot.max_net());
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
            .filter(|slot| slot.payload > super::super::MIN_LOADED_KG)
            .map(|slot| slot.payload)
            .sum()
    }

    /// Position indices ranked by `priority`, best first.
    ///
    /// The sort is stable, as Python's is, so positions that rank equally stay
    /// in the order the decks were walked, which is what decides the load
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
    pub(super) fn worst_ranked(
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
    pub(super) fn best_ranked(
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
