// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The optional upper-deck hump and the partial upper deck under it.
//!
//! One law shared by the geometry builder, the sandbox handles, the cabin
//! layout and the mass stations, so a drawn station, a lofted one and the
//! floor the seats stand on cannot disagree.
//!
//! Frame: x aft of the nose tip, z up, metres. The hump is a crown-only
//! rise `r(x)` added to the top line of the body; the keel, the width and
//! the main lobe are unchanged, so a station's height grows by `r` and its
//! centre rises by `r / 2`. With `h` the hump height, `x_s` its start,
//! `x_c0`/`x_c1` the ends of the full-height crown and `x_e` its end:
//!
//! - fore fairing, `t = (x - x_s) / (x_c0 - x_s)`: `r = h (1 - (1 - t)^p)`;
//! - crown, `x_c0 <= x <= x_c1`: `r = h`;
//! - aft fairing, `u = (x - x_c1) / (x_e - x_c1)`: `r = h (1 - u^p)`;
//!
//! zero outside `(x_s, x_e)`. `p` is the fairing exponent: 2 gives a
//! fairing tangent to the crown and meeting the main crown (or the nose
//! line) at a slope `2 h / L_fairing`, which is the windshield line and the
//! aft fairing break of a 747. Every field unset (or an inconsistent set)
//! leaves the body bit-identical to the hump-free loft.
//!
//! # Why the sections stay single-lobe superellipses
//!
//! A hump station is the main section with its crown raised: one ellipse
//! of the full local height. A true double-lobe section (a narrower upper
//! lobe on the main lobe) is not represented, because every consumer of the
//! loft (the wetted-area perimeter fit, the cabin envelope, the CPACS
//! export, the mass stations and the renderers) reads only width, height,
//! centre and exponent. Against the ACAP 747-400 forward cabin section
//! (D6-58326-1 Rev F, section 2.5.1, p. 2-28) the single 6.50 x 8.09 m
//! ellipse over the hump is 5.85 m wide outside at the upper-deck floor
//! against 5.44 m drawn (+8 %), and its perimeter (23.0 m) matches within
//! 1 % an egg section with the same keel, width and crown built from two
//! half-ellipses meeting at the main-lobe centre. The error is the
//! upper-lobe sidewall shape (a wider upper-deck floor, absorbed by the
//! upper-deck width factor of the cabin model), not wetted area.
//!
//! The upper deck is a cabin level of its own only where the hump gives it
//! headroom: its floor sits `upper_deck_floor_height_m` above the main-deck
//! floor and it runs between `upper_deck_start_x_m` and
//! `upper_deck_end_x_m` (default: the full-height crown).

use super::FuselageConfig;

/// Default fairing exponent: tangent to the crown, a slope break at the
/// fairing ends.
pub const DEFAULT_HUMP_FAIRING_EXPONENT: f64 = 2.0;
/// Valid range of the fairing exponent.
pub const HUMP_FAIRING_EXPONENT_RANGE: (f64, f64) = (1.0, 4.0);
/// Main-deck floor of a hump body, as a fraction of the main-lobe internal
/// half-height below the section centre. ESTIMATE from the 747-400 ACAP
/// (D6-58326-1 Rev F): the forward cabin section of p. 2-28 puts the outer
/// hump crown 5.01 m above the main floor (drawing read, +-0.05 m), and the
/// side view of p. 2-14 puts that crown 8.09 m above the keel of a 7.24 m
/// main lobe, so the main floor is 0.54 m below the lobe centre, -0.156 of
/// its 3.47 m internal half-height.
pub const HUMP_MAIN_DECK_FLOOR_FRACTION: f64 = -0.15;
/// Extra loft stations on the fore and aft fairings, so the crown line is
/// resolved where the generated nose and cabin stations are too sparse.
const FORE_FAIRING_STATIONS: usize = 6;
const AFT_FAIRING_STATIONS: usize = 8;

/// The resolved hump, every station ordered and every value finite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UpperDeckHump {
    /// Crown rise above the main crown over the full-height crown, m.
    pub height_m: f64,
    /// Station where the crown starts to rise, m aft of the nose tip.
    pub start_x_m: f64,
    /// Station where the full height is reached.
    pub crown_start_x_m: f64,
    /// Station where the aft fairing starts.
    pub crown_end_x_m: f64,
    /// Station where the crown is back on the main crown.
    pub end_x_m: f64,
    /// Fairing exponent `p`.
    pub fairing_exponent: f64,
}

impl UpperDeckHump {
    /// The crown rise at station `x_m`, m (zero outside the hump).
    #[must_use]
    pub fn crown_rise_m(&self, x_m: f64) -> f64 {
        let p = self.fairing_exponent;
        if !(x_m > self.start_x_m && x_m < self.end_x_m) {
            0.0
        } else if x_m < self.crown_start_x_m {
            let t = (x_m - self.start_x_m) / (self.crown_start_x_m - self.start_x_m);
            self.height_m * (1.0 - (1.0 - t).powf(p))
        } else if x_m <= self.crown_end_x_m {
            self.height_m
        } else {
            let u = (x_m - self.crown_end_x_m) / (self.end_x_m - self.crown_end_x_m);
            self.height_m * (1.0 - u.powf(p))
        }
    }

    /// The extra loft stations of the hump: the fairings sampled evenly,
    /// both ends and both crown corners included.
    #[must_use]
    pub fn loft_stations_m(&self) -> Vec<f64> {
        let fore = self.crown_start_x_m - self.start_x_m;
        let aft = self.end_x_m - self.crown_end_x_m;
        let mut xs: Vec<f64> = (0..=FORE_FAIRING_STATIONS)
            .map(|k| self.start_x_m + fore * k as f64 / FORE_FAIRING_STATIONS as f64)
            .collect();
        xs.extend(
            (0..=AFT_FAIRING_STATIONS)
                .map(|k| self.crown_end_x_m + aft * k as f64 / AFT_FAIRING_STATIONS as f64),
        );
        xs
    }
}

/// The upper deck under the hump.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UpperDeck {
    /// Forward end of the usable upper-deck floor, m aft of the nose tip.
    pub start_x_m: f64,
    /// Aft end of the usable upper-deck floor.
    pub end_x_m: f64,
    /// Upper-deck floor above the main-deck floor, m.
    pub floor_height_m: f64,
}

/// One body station of the parametric law at an arbitrary station.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyStation {
    /// Station, m aft of the nose tip.
    pub x_m: f64,
    /// Section centre height, m.
    pub z_m: f64,
    /// Full width, m.
    pub width_m: f64,
    /// Full height, m.
    pub height_m: f64,
    /// Superellipse exponent.
    pub shape: f64,
}

fn finite(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite())
}

impl FuselageConfig {
    /// The hump in effect: `None` unless the height is positive and all four
    /// stations are set, finite and ordered `start < crown start <= crown end
    /// < end` with `start >= 0`. An unset or non-finite exponent takes
    /// [`DEFAULT_HUMP_FAIRING_EXPONENT`]; a set one is clamped to its range.
    #[must_use]
    pub fn upper_deck_hump(&self) -> Option<UpperDeckHump> {
        let height_m = finite(self.hump_height_m).filter(|h| *h > 0.0)?;
        let start_x_m = finite(self.hump_start_x_m).filter(|x| *x >= 0.0)?;
        let crown_start_x_m = finite(self.hump_crown_start_x_m).filter(|x| *x > start_x_m)?;
        let crown_end_x_m = finite(self.hump_crown_end_x_m).filter(|x| *x >= crown_start_x_m)?;
        let end_x_m = finite(self.hump_end_x_m).filter(|x| *x > crown_end_x_m)?;
        let (lo, hi) = HUMP_FAIRING_EXPONENT_RANGE;
        let fairing_exponent = finite(self.hump_fairing_exponent)
            .map_or(DEFAULT_HUMP_FAIRING_EXPONENT, |p| p.clamp(lo, hi));
        Some(UpperDeckHump {
            height_m,
            start_x_m,
            crown_start_x_m,
            crown_end_x_m,
            end_x_m,
            fairing_exponent,
        })
    }

    /// The crown rise at station `x_m`, m; zero without a hump.
    #[must_use]
    pub fn hump_crown_rise_m(&self, x_m: f64) -> f64 {
        self.upper_deck_hump()
            .map_or(0.0, |hump| hump.crown_rise_m(x_m))
    }

    /// `(z, height)` of a generated station at `x_m` with its crown raised by
    /// the hump; returned unchanged (bit for bit) where the rise is zero.
    #[must_use]
    pub fn raise_crown(&self, x_m: f64, z_m: f64, height_m: f64) -> (f64, f64) {
        let rise = self.hump_crown_rise_m(x_m);
        if rise == 0.0 {
            (z_m, height_m)
        } else {
            (z_m + 0.5 * rise, height_m + rise)
        }
    }

    /// The hump-free parametric body at any station of a body of length
    /// `length_m`: the nose law ahead of the cabin start, the constant cabin
    /// section, then the aft-body law.
    #[must_use]
    pub fn body_station(&self, x_m: f64, length_m: f64) -> BodyStation {
        let aft_start = self.aft_body_start_m(length_m);
        if x_m < self.cabin_start_x_m && self.cabin_start_x_m > 0.0 {
            let s = self.nose_station(x_m / self.cabin_start_x_m);
            BodyStation {
                x_m,
                z_m: s.z_m,
                width_m: s.width_m,
                height_m: s.height_m,
                shape: s.shape,
            }
        } else if x_m <= aft_start || length_m <= aft_start {
            BodyStation {
                x_m,
                z_m: self.cabin_z_m,
                width_m: self.diameter_m,
                height_m: self.effective_height_m(),
                shape: 2.0,
            }
        } else {
            let s = self.aft_body_station((x_m - aft_start) / (length_m - aft_start), length_m);
            BodyStation {
                x_m,
                z_m: s.z_m,
                width_m: s.width_m,
                height_m: s.height_m,
                shape: 2.0,
            }
        }
    }

    /// The extra hump loft stations strictly inside a body of length
    /// `length_m`, crown raised; empty without a hump.
    #[must_use]
    pub fn hump_loft_stations(&self, length_m: f64) -> Vec<BodyStation> {
        let Some(hump) = self.upper_deck_hump() else {
            return Vec::new();
        };
        hump.loft_stations_m()
            .into_iter()
            .filter(|x| *x > 0.0 && *x < length_m)
            .map(|x| {
                let base = self.body_station(x, length_m);
                let (z_m, height_m) = self.raise_crown(x, base.z_m, base.height_m);
                BodyStation {
                    z_m,
                    height_m,
                    ..base
                }
            })
            .collect()
    }

    /// The upper deck in effect: `None` without a hump or without a positive
    /// floor height. Unset ends take the full-height crown; the ends are
    /// clamped into the hump and must leave a positive length.
    #[must_use]
    pub fn upper_deck(&self) -> Option<UpperDeck> {
        let hump = self.upper_deck_hump()?;
        let floor_height_m = finite(self.upper_deck_floor_height_m).filter(|h| *h > 0.0)?;
        let start_x_m = finite(self.upper_deck_start_x_m)
            .unwrap_or(hump.crown_start_x_m)
            .clamp(hump.start_x_m, hump.end_x_m);
        let end_x_m = finite(self.upper_deck_end_x_m)
            .unwrap_or(hump.crown_end_x_m)
            .clamp(hump.start_x_m, hump.end_x_m);
        (end_x_m > start_x_m).then_some(UpperDeck {
            start_x_m,
            end_x_m,
            floor_height_m,
        })
    }

    /// Outer width of the hump section at the upper-deck floor, m, at the
    /// middle of the upper deck: the single-ellipse section of the module
    /// doc with the main floor at [`HUMP_MAIN_DECK_FLOOR_FRACTION`] of the
    /// outer half-height. Zero without an upper deck.
    #[must_use]
    pub fn upper_deck_floor_width_m(&self) -> f64 {
        let Some(deck) = self.upper_deck() else {
            return 0.0;
        };
        let half_height = 0.5 * self.effective_height_m();
        let rise = self.hump_crown_rise_m(0.5 * (deck.start_x_m + deck.end_x_m));
        let floor_above_centre =
            HUMP_MAIN_DECK_FLOOR_FRACTION * half_height + deck.floor_height_m - 0.5 * rise;
        let n = floor_above_centre / (half_height + 0.5 * rise);
        if n.abs() >= 1.0 {
            0.0
        } else {
            self.diameter_m * (1.0 - n * n).sqrt()
        }
    }

    /// The station of a cabin-distributed group placed at `fraction` of the
    /// installed main cabin `[cabin_start_m, cabin_start_m + cabin_len_m]`,
    /// moved toward the upper deck by floor-area weighting (main floor taken
    /// as `cabin_len_m` x diameter, upper floor as its length x
    /// [`Self::upper_deck_floor_width_m`]). Exactly the main-cabin station
    /// without an upper deck.
    #[must_use]
    pub fn furnished_floor_x_m(&self, cabin_start_m: f64, cabin_len_m: f64, fraction: f64) -> f64 {
        let main_x = cabin_start_m + fraction * cabin_len_m;
        let Some(deck) = self.upper_deck() else {
            return main_x;
        };
        let main_area = cabin_len_m * self.diameter_m;
        let upper_area = (deck.end_x_m - deck.start_x_m) * self.upper_deck_floor_width_m();
        if !(main_area > 0.0 && upper_area > 0.0) {
            return main_x;
        }
        let upper_x = 0.5 * (deck.start_x_m + deck.end_x_m);
        (main_area * main_x + upper_area * upper_x) / (main_area + upper_area)
    }
}

// A failed expect in a test is the assertion failing on its own fixture.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn b747_like() -> FuselageConfig {
        FuselageConfig {
            diameter_m: 6.5,
            height_m: Some(7.24),
            cabin_z_m: 0.2,
            hump_height_m: Some(0.85),
            hump_start_x_m: Some(5.4),
            hump_crown_start_x_m: Some(11.25),
            hump_crown_end_x_m: Some(20.5),
            hump_end_x_m: Some(29.5),
            upper_deck_floor_height_m: Some(2.73),
            ..FuselageConfig::default()
        }
    }

    #[test]
    fn an_unset_or_inconsistent_hump_leaves_every_station_alone() {
        let plain = FuselageConfig::default();
        assert!(plain.upper_deck_hump().is_none());
        assert!(plain.hump_loft_stations(70.0).is_empty());
        assert_eq!(plain.raise_crown(10.0, 0.2, 6.2), (0.2, 6.2));
        assert!(plain.upper_deck().is_none());
        assert_eq!(plain.furnished_floor_x_m(6.0, 50.0, 0.5), 6.0 + 0.5 * 50.0);
        // A height alone, or stations out of order, is no hump.
        let partial = FuselageConfig {
            hump_height_m: Some(1.0),
            ..FuselageConfig::default()
        };
        assert!(partial.upper_deck_hump().is_none());
        let reversed = FuselageConfig {
            hump_crown_end_x_m: Some(10.0),
            ..b747_like()
        };
        assert!(reversed.upper_deck_hump().is_none());
    }

    #[test]
    fn the_rise_is_continuous_and_flat_on_the_crown() {
        let hump = b747_like().upper_deck_hump().expect("hump");
        assert_eq!(hump.crown_rise_m(5.4), 0.0);
        assert_eq!(hump.crown_rise_m(29.5), 0.0);
        assert_eq!(hump.crown_rise_m(15.0), 0.85);
        for x in [11.25, 20.5] {
            assert!((hump.crown_rise_m(x) - 0.85).abs() < 1e-12);
            assert!((hump.crown_rise_m(x - 1e-7) - 0.85).abs() < 1e-6);
            assert!((hump.crown_rise_m(x + 1e-7) - 0.85).abs() < 1e-6);
        }
        assert!(hump.crown_rise_m(5.4 + 1e-7) < 1e-6);
        assert!(hump.crown_rise_m(29.5 - 1e-7) < 1e-6);
        // Monotone up the fore fairing and down the aft one.
        let mut previous = 0.0;
        for k in 1..=100 {
            let r = hump.crown_rise_m(5.4 + 5.85 * f64::from(k) / 100.0);
            assert!(r >= previous - 1e-12);
            previous = r;
        }
    }

    #[test]
    fn the_hump_raises_only_the_crown() {
        let f = b747_like();
        let length = 68.63;
        for station in f.hump_loft_stations(length) {
            let base = f.body_station(station.x_m, length);
            let keel = station.z_m - 0.5 * station.height_m;
            let base_keel = base.z_m - 0.5 * base.height_m;
            assert!(
                (keel - base_keel).abs() < 1e-12,
                "keel moved at {}",
                station.x_m
            );
            assert_eq!(station.width_m, base.width_m);
            let crown = station.z_m + 0.5 * station.height_m;
            let base_crown = base.z_m + 0.5 * base.height_m;
            assert!((crown - base_crown - f.hump_crown_rise_m(station.x_m)).abs() < 1e-12);
        }
    }

    #[test]
    fn the_upper_deck_pulls_the_furnishings_forward() {
        let f = b747_like();
        let deck = f.upper_deck().expect("upper deck");
        assert_eq!((deck.start_x_m, deck.end_x_m), (11.25, 20.5));
        let width = f.upper_deck_floor_width_m();
        assert!(width > 4.5 && width < 6.5, "upper floor width {width}");
        let main = 6.0 + 0.5 * 45.0;
        let x = f.furnished_floor_x_m(6.0, 45.0, 0.5);
        assert!(x < main && x > 0.5 * (11.25 + 20.5));
    }
}
