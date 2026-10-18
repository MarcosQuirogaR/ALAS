// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Consistency checks of the layout block against the topology.

use super::topology::{FuselageKind, GearKind};
use super::UavSpec;

pub(super) fn layout(spec: &UavSpec, problems: &mut Vec<String>) {
    let l = &spec.master.layout;
    if l.fuselage.is_some() && spec.topology.fuselage == FuselageKind::None {
        problems.push("layout.fuselage is set but the fuselage kind is 'none'".into());
    }
    if let Some(f) = &l.fuselage {
        if 2.0 * f.wall_m >= f.width_m.min(f.height_m) {
            problems.push("layout.fuselage: walls leave no interior".into());
        }
        let mut last = 0.0_f64;
        let mut names = std::collections::BTreeSet::new();
        for b in &f.bays {
            if !(b.x0_frac >= 0.0 && b.x1_frac <= 1.0 && b.x1_frac > b.x0_frac) {
                problems.push(format!("bay '{}': needs 0 <= x0 < x1 <= 1", b.name));
            }
            if b.x0_frac < last - 1e-12 {
                problems.push(format!("bay '{}' overlaps the previous bay", b.name));
            }
            if !names.insert(b.name.as_str()) || b.name.is_empty() {
                problems.push(format!("bay name '{}' is empty or duplicated", b.name));
            }
            last = b.x1_frac;
        }
    }
    if let Some(g) = &l.gear {
        match spec.topology.gear {
            GearKind::Belly => {
                problems.push("layout.gear is set but the gear kind is belly".into())
            }
            GearKind::Taildragger if g.aux_x_m <= g.main_x_m => {
                problems.push("taildragger: the tail wheel must be aft of the main axle".into());
            }
            GearKind::Tricycle if g.aux_x_m >= g.main_x_m => {
                problems.push("tricycle: the nose wheel must be forward of the main axle".into());
            }
            _ => {}
        }
    }
}
