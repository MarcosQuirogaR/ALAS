// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Mach dependence of the trimmed induced drag (A1G).
//!
//! The trimmed induced curve is solved at the cruise Mach on the
//! Goethert-stretched lattice, where the tail download and the wing-tail
//! interaction carry the compressibility of the cruise condition. The table is
//! flown at every Mach of the mission, though, and at low speed (climb,
//! second-segment, take-off) the lattice is the incompressible one. The curve
//! solved on the same stretched path at `beta = 1` (Mach zero, same airframe
//! moment, every node re-trimmed) is therefore kept beside the cruise curve and
//! the two are interpolated in the Prandtl-Glauert stretch
//! `s(M) = (1/beta(M) - 1) / (1/beta_cruise - 1)`: zero at `M = 0`, one at the
//! cruise Mach. That is linear interpolation in the stretch factor `1/beta`
//! that moves the lattice's x stations, chords and aerodynamic centre, the
//! quantity the trimmed induced drag depends on; it is not the lift slope
//! (which follows `1/beta` only at high aspect ratio).
//!
//! The interpolation is exact at both ends and measured in between against
//! direct solves (the table of a configuration whose cruise Mach is the Mach
//! under test, re-trimmed on its own stretched lattice) at CL 0.6, 1.0 and 1.3
//! times the design lift, Mach 0.05 up to cruise + 0.05 (`trim_blend_tests`):
//! A320-200 0.6 % to Mach 0.7 and 0.95 % at cruise + 0.05, B747-400 0.3 % to
//! Mach 0.7 and 2.2 % at cruise + 0.05, ATR72-600 0.1 %, of the induced drag
//! (0.3 % of it is 0.1 % of the total drag). Above the cruise Mach the stretch
//! is extrapolated linearly to the table's own top Mach, `cruise +
//! MACH_ABOVE_CRUISE`, and held there: holding the cruise curve instead
//! errs by 5 % on the B747 at cruise + 0.05, and a weight cap would be a
//! constant with no source.

use super::induced::InducedCurve;
use super::MACH_ABOVE_CRUISE;

/// The Mach-zero induced curve and the cruise-Mach excess it is blended
/// against.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub(super) struct MachBlend {
    /// The same trimmed induced curve solved at `beta = 1`.
    pub(super) incompressible: InducedCurve,
    /// Wake energy of the basic loading removed from `incompressible`; zero
    /// when no twist allowance is in effect.
    #[serde(default)]
    pub(super) basic_loading_cd: f64,
    /// `1 / beta_cruise - 1`, positive.
    pub(super) cruise_excess: f64,
    /// The Mach the cruise curve was solved at.
    pub(super) cruise_mach: f64,
}

impl MachBlend {
    /// `(1/beta(mach) - 1) / (1/beta_cruise - 1)`, with the Mach held at the
    /// table's top, `cruise + MACH_ABOVE_CRUISE`.
    pub(super) fn weight(&self, mach: f64) -> f64 {
        let mach = mach.clamp(0.0, self.cruise_mach + MACH_ABOVE_CRUISE);
        let beta = (1.0 - mach * mach).max(1.0e-6).sqrt();
        ((1.0 / beta - 1.0) / self.cruise_excess).max(0.0)
    }

    /// Value and CL slope of the blended curve at `cl` and `mach`, given the
    /// cruise-Mach curve.
    pub(super) fn value_and_slope(&self, cruise: &InducedCurve, cl: f64, mach: f64) -> (f64, f64) {
        let w = self.weight(mach);
        let (c_value, c_slope) = cruise.value_and_slope(cl);
        let (i_value, i_slope) = self.incompressible.value_and_slope(cl);
        (
            i_value + w * (c_value - i_value),
            i_slope + w * (c_slope - i_slope),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blend(cruise_mach: f64) -> MachBlend {
        MachBlend {
            incompressible: InducedCurve {
                pieces: Vec::new(),
                attitude: Vec::new(),
            },
            basic_loading_cd: 0.0,
            cruise_excess: 1.0 / (1.0 - cruise_mach * cruise_mach).sqrt() - 1.0,
            cruise_mach,
        }
    }

    #[test]
    fn the_weight_is_zero_at_rest_one_at_cruise_and_held_at_the_table_top() {
        let b = blend(0.78);
        assert_eq!(b.weight(0.0), 0.0);
        assert!((b.weight(0.78) - 1.0).abs() < 1e-12);
        assert!(b.weight(0.4) > 0.0 && b.weight(0.4) < 0.3);
        let top = b.weight(0.78 + MACH_ABOVE_CRUISE);
        assert!(top > 1.0);
        assert_eq!(b.weight(0.99), top);
        // Monotone in Mach.
        assert!(b.weight(0.5) < b.weight(0.6) && b.weight(0.6) < b.weight(0.7));
    }
}
