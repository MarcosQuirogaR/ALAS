// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

impl<'g> CargoLoadManager<'g> {
    /// Put the total mass back on target, adding on the light side of the
    /// balance error and removing from the heavy side.
    fn correct_total(&mut self, mut diff: f64, error: f64) {
        if diff.abs() <= MASS_CORRECTION_KG {
            return;
        }
        let aft_first = if diff > 0.0 { error < 0.0 } else { error > 0.0 };
        let mut candidates: Vec<usize> = (0..self.slots.len()).collect();
        candidates.sort_by(|&a, &b| {
            if aft_first {
                self.slots[b].x.total_cmp(&self.slots[a].x)
            } else {
                self.slots[a].x.total_cmp(&self.slots[b].x)
            }
        });

        for &i in &candidates {
            if diff.abs() < MASS_SETTLED_KG {
                break;
            }
            if diff > 0.0 {
                let space = self.slots[i].max_net() - self.slots[i].payload;
                if space > 0.0 {
                    let add = diff.min(space);
                    self.slots[i].payload += add;
                    diff -= add;
                }
            } else if self.slots[i].payload > 0.0 {
                let removed = (-diff).min(self.slots[i].payload);
                self.slots[i].payload -= removed;
                diff += removed;
            }
        }
    }

    /// Move one step of load from the heavy side toward the light side.
    ///
    /// Returns whether there was anywhere to move it from and to: when there is
    /// not, the balance is as good as this set of positions can make it and the
    /// loop has nothing left to try.
    fn shift_toward_target(
        &mut self,
        error: f64,
        current_cg: f64,
        step: f64,
        priority: &dyn Fn(&CargoSlot) -> f64,
    ) -> bool {
        // A position exactly at the current centre of gravity is in neither
        // list: moving load to or from it would not shift the balance.
        let heavy_side_is_aft = error > 0.0;
        let on_heavy_side = |x: f64| {
            if heavy_side_is_aft {
                x > current_cg
            } else {
                x < current_cg
            }
        };
        let source: Vec<usize> = (0..self.slots.len())
            .filter(|&i| on_heavy_side(self.slots[i].x) && self.slots[i].payload > 0.0)
            .collect();
        let destination: Vec<usize> = (0..self.slots.len())
            .filter(|&i| {
                !on_heavy_side(self.slots[i].x)
                    && self.slots[i].x != current_cg
                    && self.slots[i].payload < self.slots[i].max_net()
            })
            .collect();
        let (Some(from), Some(to)) = (
            self.worst_ranked(&source, priority),
            self.best_ranked(&destination, priority),
        ) else {
            return false;
        };

        let amount = step
            .min(self.slots[from].payload)
            .min(self.slots[to].max_net() - self.slots[to].payload);
        self.slots[from].payload -= amount;
        self.slots[to].payload += amount;
        true
    }
}
