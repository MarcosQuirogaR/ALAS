// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Beam section reduction and offset serialization shared by both solver dialects.

use super::super::section;
use super::{card, ContinuationTags, Deck, Field};
use crate::mesh::Pbarl;
use std::collections::HashMap;

/// The `CBAR`s and the `PBAR`s reduced from their `PBARL` sections.
///
/// Both dialects get `PBAR`, not just NASTRAN-95: modelling the caps as the
/// identical explicit beam on both sides is what lets the cross-solver
/// comparison isolate the shell and eigensolver differences, and the reduction
/// [`super::section`] performs is validated against a modern `PBARL` on its own.
/// Field 9 (the mesh's `OFFT` string) is dropped, because this dialect's
/// `CBAR` spends that column on an integer flag and reads the vector directly.
pub(super) fn bars(out: &mut String, tags: &mut ContinuationTags, deck: &Deck) {
    let sections: HashMap<i64, &Pbarl> = deck.bar_properties.iter().map(|p| (p.pid, p)).collect();
    let mut emitted = std::collections::HashSet::new();
    for bar in &deck.bars {
        let mut fields = vec![
            Field::Int(bar.eid),
            Field::Int(bar.pid),
            Field::Int(bar.ga),
            Field::Int(bar.gb),
            Field::Real(bar.x[0]),
            Field::Real(bar.x[1]),
            Field::Real(bar.x[2]),
        ];
        if bar.offset_a != [0.0; 3] || bar.offset_b != [0.0; 3] {
            // Field 9 is the old dialect's flag (blank), then PA/PB and
            // basic-frame end offsets. All mesh GRID displacement frames are 0.
            fields.extend([Field::Blank, Field::Blank, Field::Blank]);
            fields.extend(
                bar.offset_a
                    .iter()
                    .chain(&bar.offset_b)
                    .map(|&v| Field::Real(v)),
            );
        }
        card(out, tags, "CBAR", fields);
        if !emitted.insert(bar.pid) {
            continue;
        }
        let Some(property) = sections.get(&bar.pid) else {
            continue;
        };
        // A section that cannot be reduced is omitted rather than emitted wrong, which
        // makes the end-to-end run fail loudly on a bar with no property.
        let constants = match property.section {
            "I" => section::i_section(&property.dim),
            "BAR" => section::rectangle(&property.dim),
            _ => None,
        };
        if let Some(bar_constants) = constants {
            let mut fields = vec![
                Field::Int(property.pid),
                Field::Int(property.mid),
                Field::Real(bar_constants.area),
                Field::Real(bar_constants.i1),
                Field::Real(bar_constants.i2),
                Field::Real(bar_constants.j),
            ];
            if let [width, thickness] = property.dim.as_slice() {
                // Product rectangle stress recovery at its four corners.
                // Coordinates are element y (thickness), z (width), in metres.
                fields.extend([Field::Blank, Field::Blank]);
                for (y, z) in [(0.5, 0.5), (-0.5, 0.5), (-0.5, -0.5), (0.5, -0.5)] {
                    fields.extend([Field::Real(y * thickness), Field::Real(z * width)]);
                }
            }
            card(out, tags, "PBAR", fields);
        }
    }
}
