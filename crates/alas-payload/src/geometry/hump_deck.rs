// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The decks of a body with a partial upper deck under an upper-deck hump.
//!
//! The sampled sections include the hump, a crown-only rise `r(x)`; the
//! main lobe the main deck and the hold sit in is the section with that rise
//! taken off (centre `zc - r / 2`, height `h - r`), so the main floor stays
//! level under the hump. The upper deck stands `floor_height_m` above the
//! main floor and its ceiling follows the raised crown; under it, the main
//! deck's ceiling stops one deck separation below the upper floor. The
//! physical envelope checks keep reading the full section.
//!
//! Frame: x aft of the nose tip, z up, metres.

use super::{CabinGeometry, DeckSpec, MIN_DECK_SEPARATION_FRAC};

impl CabinGeometry {
    /// The hump's crown rise at `x`, m; zero without a partial upper deck.
    /// Interpolated like the sections themselves, so the main lobe between
    /// two stations is the straight loft of the two main-lobe sections.
    pub(super) fn crown_rise_m(&self, x: f64) -> f64 {
        if self.crown_rises.is_empty() {
            0.0
        } else {
            crate::numeric::interp(x, &self.x_stations, &self.crown_rises)
        }
    }

    /// The main lobe's section centre at `x`.
    pub fn main_lobe_zc_at(&self, x: f64) -> f64 {
        self.zc_at(x) - 0.5 * self.crown_rise_m(x)
    }

    /// The main-deck floor at `x`, the datum of the upper floor.
    fn main_floor_z(&self, x: f64) -> f64 {
        let main = self
            .passenger_decks
            .first()
            .map_or(0.0, |deck| deck.floor_frac);
        self.main_lobe_zc_at(x) + main * self.internal_half_height(x)
    }

    /// Whether the upper-deck floor spans station `x`.
    fn under_upper_floor(&self, x: f64) -> bool {
        self.upper_deck
            .is_some_and(|deck| (deck.start_x_m..=deck.end_x_m).contains(&x))
    }

    /// The upper-deck floor at `x`, or `None` without an upper deck.
    pub fn upper_floor_z(&self, x: f64) -> Option<f64> {
        self.upper_deck
            .map(|deck| self.main_floor_z(x) + deck.floor_height_m)
    }

    pub(super) fn hump_floor_z(&self, deck: &DeckSpec, x: f64) -> f64 {
        match self.upper_floor_z(x) {
            Some(upper) if deck.name == crate::layout::UPPER => upper,
            _ => self.main_lobe_zc_at(x) + deck.floor_frac * self.internal_half_height(x),
        }
    }

    pub(super) fn hump_ceil_z(&self, deck: &DeckSpec, x: f64) -> f64 {
        let b = self.internal_half_height(x);
        let lobe = self.main_lobe_zc_at(x) + deck.ceil_frac * b;
        if deck.name == crate::layout::UPPER {
            return lobe + self.crown_rise_m(x);
        }
        match self.upper_floor_z(x) {
            Some(upper) if deck.name == crate::layout::MAIN && self.under_upper_floor(x) => {
                lobe.min(upper - MIN_DECK_SEPARATION_FRAC * b)
            }
            _ => lobe,
        }
    }
}

// A failed expect in a test is the assertion failing on a registered preset.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use crate::layout::{LOWER, MAIN, UPPER};
    use crate::CabinGeometry;
    use alas_config::presets;
    use alas_geom::builder::AircraftBuilder;

    fn b747() -> CabinGeometry {
        let preset = presets::get("B747-400").expect("preset");
        let plane = AircraftBuilder::new(Some(preset.geometry.clone()))
            .build(Some(&preset.design_vector), false)
            .expect("builds");
        CabinGeometry::new(&plane, &preset.geometry, 0.15).expect("cabin frame")
    }

    #[test]
    fn the_b747_gets_a_level_main_deck_and_a_partial_upper_deck() {
        let g = b747();
        assert!(!g.is_double_deck);
        let names: Vec<_> = g.passenger_decks.iter().map(|d| d.name).collect();
        assert_eq!(names, [MAIN, UPPER]);
        assert_eq!(g.lower_deck.name, LOWER);
        let (main, upper) = (&g.passenger_decks[0], &g.passenger_decks[1]);
        // The main floor is level under and aft of the hump.
        let aft = g.floor_z(main, 40.0);
        // old 8.0 dropped: the measured 10.6 m nose (was 6.0 m) puts x = 8 m in
        // the nose, where the section centre line still rises.
        for x in [12.0, 16.0, 22.0, 27.0, 33.0] {
            assert!((g.floor_z(main, x) - aft).abs() < 1e-9, "main floor at {x}");
        }
        // ACAP D6-58326-1 Rev F p. 2-29: 2.73 m floor to floor.
        let floor = g.floor_z(upper, 16.0);
        assert!((floor - aft - 2.73).abs() < 1e-9);
        // Standing headroom on the upper deck and under it.
        let upper_room = g.deck_height(upper, 16.0);
        assert!(upper_room > 1.8 && upper_room < 2.3, "upper {upper_room}");
        let main_room = g.deck_height(main, 16.0);
        assert!(main_room > 2.2 && main_room < 2.73, "main {main_room}");
        assert!(g.ceil_z(main, 16.0) < floor);
        // The upper floor is inside the section and narrower than the main.
        let width = g.usable_width(upper, 16.0);
        assert!(
            width > 3.4 && width < g.usable_width(main, 16.0),
            "upper {width}"
        );
    }
}
