// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lower-hold rows laid inside holds the source declares by station.

use super::*;

impl CargoLoadManager<'_> {
    /// Rows inside every declared lower hold, or `None` when the preset
    /// declares none (or on the frozen parity frame), so the clean-sheet
    /// front-spar-to-trailing-edge proxy applies.
    ///
    /// Holds named for bulk carry no container rows; a bulk-only hold (loose
    /// [`BULK`] everywhere) fills them with loose positions under `BULK-`
    /// identifiers. An `aft` name selects the `AFT-` identifiers and any other
    /// name the `FWD-` ones. The fit check against
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
        // Rows already numbered under each identifier prefix, so that two
        // declared holds sharing one never repeat an identifier.
        let mut rows_numbered: std::collections::HashMap<&'static str, usize> =
            std::collections::HashMap::new();
        for hold in declared {
            let name = hold.name.to_ascii_lowercase();
            let bulk_hold = name.contains("bulk");
            if bulk_hold && uld.code != BULK.code {
                continue;
            }
            let prefix = if bulk_hold {
                "BULK"
            } else if name.contains("aft") {
                "AFT"
            } else {
                "FWD"
            };
            // A source that prints the number of rows fixes the pitch: the rows share the
            // compartment evenly, whatever clearance the default pitch would have left.
            let (rows, pitch) = match hold.container_rows {
                Some(rows) if rows >= 1 => (
                    (rows as usize).min(MAX_LOWER_ROWS),
                    (hold.x_end_m - hold.x_start_m) / f64::from(rows),
                ),
                _ => (MAX_LOWER_ROWS, pitch),
            };
            // The printed rows are one container long. A longer container
            // (an LD11 or a pallet chosen by the auto selector) would overlap
            // its neighbours, so it takes no position in this hold.
            if hold.container_rows.is_some() && uld.length > pitch + SLOT_FIT_TOLERANCE_M {
                continue;
            }
            let mut x = if hold.container_rows.is_some() {
                hold.x_start_m + pitch / 2.0
            } else {
                hold.x_start_m + uld.length / 2.0
            };
            for _ in 0..rows {
                if x + uld.length / 2.0 > hold.x_end_m + SLOT_FIT_TOLERANCE_M {
                    break;
                }
                let slack = if hold.container_rows.is_some() {
                    PRINTED_ROWS_HEIGHT_SLACK_M
                } else {
                    0.0
                };
                let numbered = rows_numbered.entry(prefix).or_insert(0);
                *numbered += 1;
                placed += self.row_within(
                    &format!("{prefix}-{numbered}-"),
                    low,
                    x,
                    uld,
                    slack,
                    hold.clear_width_at(x),
                );
                x += pitch;
            }
        }
        Some(placed)
    }
}
