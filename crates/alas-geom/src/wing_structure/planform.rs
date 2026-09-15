// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Planform, dihedral and airfoil-surface sampling: everything a caller
//! reads about a spanwise station before any spar or rib-length logic
//! enters: `local_chord`/`x_le`/`z_le`/`rib_vector`/`le_direction`/
//! `airfoil_zu_zl`/`spar_height`.

use super::support::clamped_interp;
use super::WingStructureGeometry;

impl WingStructureGeometry {
    // planform / dihedral (piecewise-linear root -> break -> tip)

    /// Local chord at `eta`, by linear interpolation root -> break -> tip.
    pub fn local_chord(&self, eta: f64) -> f64 {
        if eta <= self.break_eta {
            let t = eta / self.break_eta.max(1e-9);
            self.c_root * (1.0 - t) + self.c_break * t
        } else {
            let t = (eta - self.break_eta) / (1.0 - self.break_eta).max(1e-9);
            self.c_break * (1.0 - t) + self.c_tip * t
        }
    }

    /// Leading-edge X offset at `eta`, by linear interpolation.
    pub fn x_le(&self, eta: f64) -> f64 {
        if eta <= self.break_eta {
            let t = eta / self.break_eta.max(1e-9);
            self.dx_break * t
        } else {
            let t = (eta - self.break_eta) / (1.0 - self.break_eta).max(1e-9);
            self.dx_break + (self.dx_tip - self.dx_break) * t
        }
    }

    /// Leading-edge Z offset at `eta`, by linear interpolation.
    pub fn z_le(&self, eta: f64) -> f64 {
        if eta <= self.break_eta {
            let t = eta / self.break_eta.max(1e-9);
            self.z_root * (1.0 - t) + self.z_break * t
        } else {
            let t = (eta - self.break_eta) / (1.0 - self.break_eta).max(1e-9);
            self.z_break * (1.0 - t) + self.z_tip * t
        }
    }

    /// Unit chordwise direction in the XY plane: streamwise at the root
    /// (the clamped wall must be a clean streamwise cut), perpendicular to
    /// the local leading edge everywhere else.
    pub fn rib_vector(&self, eta: f64) -> (f64, f64) {
        if eta <= 1e-9 {
            return (1.0, 0.0);
        }
        let sweep = if eta <= self.break_eta {
            self.sweep_in
        } else {
            self.sweep_out
        };
        (sweep.cos(), -sweep.sin())
    }

    /// Unit leading-edge tangent direction in the XY plane, independent of
    /// [`WingStructureGeometry::rib_vector`]: a self-consistency check (a
    /// realized rib cut should come out perpendicular to this everywhere
    /// except the root rib, which is deliberately streamwise instead).
    pub fn le_direction(&self, eta: f64) -> (f64, f64) {
        let sweep = if eta <= self.break_eta {
            self.sweep_in
        } else {
            self.sweep_out
        };
        let dx = sweep.tan();
        let dy = 1.0;
        let norm = dx.hypot(dy);
        (dx / norm, dy / norm)
    }

    // airfoil surface at an arbitrary (eta, x/c)

    /// Upper/lower surface height (fraction of local chord) at `xc_frac`.
    ///
    /// `eta <= break_eta` returns the root section verbatim, matching
    /// `AircraftBuilder` reusing the same morphed root section for both the
    /// root and break wing cross-sections, no interpolation needed
    /// inboard. `eta > break_eta` linearly blends toward the tip section,
    /// matching native aerodynamic model's own linear interpolation between the break and
    /// tip cross-sections.
    pub fn airfoil_zu_zl(&self, eta: f64, xc_frac: f64) -> (f64, f64) {
        let xc = xc_frac.clamp(0.0, 1.0);
        let zu_root = clamped_interp(xc, &self.root_xu, &self.root_zu);
        let zl_root = clamped_interp(xc, &self.root_xl, &self.root_zl);
        if eta <= self.break_eta {
            return (zu_root, zl_root);
        }
        let zu_tip = clamped_interp(xc, &self.tip_xu, &self.tip_zu);
        let zl_tip = clamped_interp(xc, &self.tip_xl, &self.tip_zl);
        let blend = ((eta - self.break_eta) / (1.0 - self.break_eta).max(1e-9)).clamp(0.0, 1.0);
        (
            zu_root * (1.0 - blend) + zu_tip * blend,
            zl_root * (1.0 - blend) + zl_tip * blend,
        )
    }

    /// Free web height (extrados - intrados) at `(eta, xc_frac)`, in metres.
    pub fn spar_height(&self, eta: f64, xc_frac: f64) -> f64 {
        let (zu, zl) = self.airfoil_zu_zl(eta, xc_frac);
        (zu - zl) * self.local_chord(eta)
    }
}
