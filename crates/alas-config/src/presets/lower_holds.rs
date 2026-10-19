// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lower-hold extents declared by station for the registered widebodies whose
//! airport-planning documents draw their cargo compartments to scale.
//!
//! Frame: metres aft of the nose tip. Each start station is read off the
//! printed compartment drawing at the scale of its own printed dimensions
//! (the drawing's overall fuselage length, cross-checked against the printed
//! compartment lengths), so a start carries about +-0.3 m of read
//! uncertainty. The compartment lengths are the printed ones. Volumes are not
//! declared: the enclosed volume of the built deck is computed over the
//! extent, so the total hold volume stays a prediction that can be compared
//! with the printed usable volumes.
//!
//! Not declared, with reason: DC-10-30 (ACAP PDF pp.31-32 draw the holds
//! schematically, not to scale, and print no compartment length) and
//! B747-400 (ACAP PDF p.49 prints container counts, no compartment length or
//! station). Those keep the clean-sheet front-spar proxy.

use crate::{HoldCompartmentConfig, HoldDeck};

fn hold(name: &str, x_start_m: f64, length_m: f64) -> HoldCompartmentConfig {
    HoldCompartmentConfig {
        name: name.to_owned(),
        x_start_m,
        x_end_m: x_start_m + length_m,
        volume_m3: None,
        max_net_kg: None,
        deck: HoldDeck::Lower,
    }
}

/// The declared lower holds of a registered preset, if its sources draw them.
pub fn declared_lower_holds(preset: &str) -> Option<Vec<HoldCompartmentConfig>> {
    match preset {
        // airbus_2025_a340ac.pdf PDF p.61 (FIGURE-2-6-1-991-005-A01, A340-300
        // row): forward 15.02 m, aft 12.32 m, bulk 4.04 m; starts scaled on
        // the 63.66 m drawn fuselage. Door centres p.68: forward cargo door
        // 9.53 m, aft cargo door 44.36 m, bulk door 47.35 m, each inside its
        // compartment as drawn here.
        "A340-300" => Some(vec![
            hold("Forward hold", 8.41, 15.02),
            hold("Aft hold", 34.44, 12.32),
            hold("Bulk hold", 46.90, 4.04),
        ]),
        // airbus_2025_a380ac.pdf PDF p.52 (FIGURE-2-6-0-991-001-A01):
        // forward 17.4 m (FR17 to FR44), forward-aft T-shaped 9.7 m and rear
        // aft 8.2 m (FR57 to FR86, 17.9 m together), bulk 2.8 m (FR86 to
        // FR90); starts scaled on the 72.72 m drawn fuselage. The 8 m between
        // FR44 and FR57 is the wing-box and gear zone.
        "A380-800" => Some(vec![
            hold("Forward hold", 7.77, 17.4),
            hold("Aft hold", 33.60, 17.9),
            hold("Bulk hold", 51.50, 2.8),
        ]),
        // boeing_2025_787acap.pdf PDF p.30 (section 2.6.2, 787-9): forward
        // 51 ft 6 in (15.72 m), aft 49 ft 9 in (15.16 m); starts scaled on
        // the printed dimension lines from the nose tip. Door stations p.32:
        // forward cargo door 11.00 m, aft cargo door 43.31 m.
        "B787-9" => Some(vec![
            hold("Forward hold", 7.36, 15.72),
            hold("Aft hold", 33.07, 15.16),
        ]),
        _ => None,
    }
}
