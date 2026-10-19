// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lower-hold rows laid inside holds the source declares by station.

use super::*;

impl CargoLoadManager<'_> {
    /// Rows inside every declared lower hold, or `None` when the preset
    /// declares none (or on the frozen parity frame), so the clean-sheet
    /// front-spar-to-trailing-edge proxy applies.
    ///
    /// Holds named for bulk carry no rows, an `aft` name selects the `AFT-`
    /// identifiers and any other name the `FWD-` ones. The fit check against
    /// the fuselage contour decides how many positions of each row survive.
    pub(super) fn fill_declared_holds(&mut self, uld: &'static UldType) -> Option<usize> {
        let g = self.geometry;
        if !g.enforces_physical_envelope() {
            return None;
        }
        let declared: Vec<_> = self
            .config
            .hold_compartments
            .iter()
            .filter(|hold| hold.is_valid() && hold.deck == alas_config::HoldDeck::Lower)
            .cloned()
            .collect();
        if declared.is_empty() {
            return None;
        }
        let low = &g.lower_deck;
        let pitch = uld.length + LOWER_ROW_GAP_M;
        let mut placed = 0;
        for hold in declared {
            let name = hold.name.to_ascii_lowercase();
            if name.contains("bulk") {
                continue;
            }
            let prefix = if name.contains("aft") { "AFT" } else { "FWD" };
            let mut x = hold.x_start_m + uld.length / 2.0;
            for i in 0..MAX_LOWER_ROWS {
                if x + uld.length / 2.0 > hold.x_end_m + SLOT_FIT_TOLERANCE_M {
                    break;
                }
                placed += self.row(&format!("{prefix}-{}-", i + 1), low, x, uld);
                x += pitch;
            }
        }
        Some(placed)
    }
}
