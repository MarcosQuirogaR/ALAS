// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Load planning on a placed load: move whole containers between positions.
//!
//! A load plan chooses where the loaded containers stand, not what they hold:
//! the container set, every container's net load and therefore the gross
//! payload (tare included) stay exactly as the trim loop left them. A loaded
//! container may move to an empty position of the same container type that can
//! carry its load, or exchange positions with another loaded container of that
//! type when each position can carry the other's load. A move is taken when it
//! brings the load's centre of gravity nearer the target, and the search stops
//! when none does.
//!
//! The exchange matters when every position of a type is taken, or when the
//! empty ones lie on the wrong side of the target: a light and a heavy
//! container trading places shift the moment by `(w_a - w_b)(x_b - x_a)`,
//! which no move into an empty position can reach.

use super::super::MIN_LOADED_KG;
use super::CargoLoadManager;

impl CargoLoadManager<'_> {
    /// Relocate loaded containers toward `target_cg` (m, aircraft x) without
    /// changing the container set or any load. Returns how many containers
    /// moved (an exchange moves two).
    pub fn relocate_same_set(&mut self, target_cg: f64) -> usize {
        let mut moved = 0;
        for _ in 0..4 * self.slots.len() + 4 {
            let (mass, cg, _) = self.mass_props();
            if mass <= 0.0 || !cg.is_finite() || !target_cg.is_finite() {
                break;
            }
            let error = (cg - target_cg).abs();
            // (error, from, to, exchange)
            let mut best: Option<(f64, usize, usize, bool)> = None;
            for from in 0..self.slots.len() {
                let source = &self.slots[from];
                if source.payload <= MIN_LOADED_KG {
                    continue;
                }
                let weight = source.total_weight();
                for to in 0..self.slots.len() {
                    let dest = &self.slots[to];
                    if to == from
                        || dest.uld.code != source.uld.code
                        || source.payload > dest.max_net()
                    {
                        continue;
                    }
                    let exchange = dest.payload > MIN_LOADED_KG;
                    // An exchange is weighed once per pair, and the source
                    // position must carry the destination's load in turn.
                    if exchange && (to < from || dest.payload > source.max_net()) {
                        continue;
                    }
                    let shifted = if exchange {
                        weight - dest.total_weight()
                    } else {
                        weight
                    };
                    let new_cg = cg + shifted * (dest.x - source.x) / mass;
                    let new_error = (new_cg - target_cg).abs();
                    if new_error + 1.0e-9 < best.map_or(error, |(e, ..)| e) {
                        best = Some((new_error, from, to, exchange));
                    }
                }
            }
            let Some((_, from, to, exchange)) = best else {
                break;
            };
            let load = self.slots[from].payload;
            self.slots[from].payload = if exchange {
                self.slots[to].payload
            } else {
                0.0
            };
            self.slots[to].payload = load;
            moved += if exchange { 2 } else { 1 };
        }
        moved
    }
}

#[cfg(test)]
// Failed expectations and unwraps here are failed test assertions.
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::super::CargoLoadManager;
    use crate::geometry::CabinGeometry;
    use alas_config::{CargoDeckConfig, GeometryConfig};
    use alas_geom::builder::AircraftBuilder;

    #[test]
    fn config_default_carries_no_plan_target() {
        assert!(CargoDeckConfig::default().plan_payload_cg_x_m.is_none());
    }

    #[test]
    fn a_full_row_reaches_its_target_by_exchanging_containers() {
        let builder = AircraftBuilder::new(Some(GeometryConfig::default()));
        let plane = builder.build(None, false).expect("default aircraft builds");
        let geometry =
            CabinGeometry::new(&plane, &builder.geometry, 0.15).expect("default cabin samples");
        let mut manager = CargoLoadManager::new(&geometry, CargoDeckConfig::default());
        let code = manager.lower_uld.code;
        let mut row: Vec<usize> = (0..manager.slots.len())
            .filter(|&i| manager.slots[i].uld.code == code)
            .collect();
        row.sort_by(|&a, &b| manager.slots[a].x.total_cmp(&manager.slots[b].x));
        assert!(row.len() >= 3, "the default lower hold holds a row");
        // Every position of the type loaded, so no container can move into an
        // empty one: the heavy load sits forward, the target is the aft end.
        let capacity = row
            .iter()
            .map(|&i| manager.slots[i].max_net())
            .fold(f64::INFINITY, f64::min);
        let (light, heavy) = (0.1 * capacity, 0.8 * capacity);
        for &i in &row {
            manager.slots[i].payload = light;
        }
        manager.slots[row[0]].payload = heavy;
        let mut loads: Vec<f64> = manager.slots.iter().map(|s| s.payload).collect();
        let (mass, cg, _) = manager.mass_props();
        let target = manager.slots[*row.last().unwrap()].x;

        let moved = manager.relocate_same_set(target);

        let (mass_after, cg_after, _) = manager.mass_props();
        assert!(
            moved >= 2,
            "an exchange moves two containers, moved {moved}"
        );
        assert!((cg_after - target).abs() < (cg - target).abs() - 1.0e-6);
        assert!((mass_after - mass).abs() < 1.0e-9);
        let mut loads_after: Vec<f64> = manager.slots.iter().map(|s| s.payload).collect();
        loads.sort_by(f64::total_cmp);
        loads_after.sort_by(f64::total_cmp);
        assert_eq!(loads, loads_after, "the container set is unchanged");
    }
}
