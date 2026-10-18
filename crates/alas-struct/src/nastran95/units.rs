// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The unit system the NASTRAN-95 product deck is written in.
//!
//! # Why not metres
//!
//! NASTRAN-95's `CQUAD4` module (`Q4NRMS`, called from `QUAD4S`) rejects an
//! element as `ILLEGAL GEOMETRY OR CONNECTIONS` (user fatal message 2224) when
//! the squared magnitude of the cross product of the two isoparametric tangent
//! vectors at a corner, `|dX/dxi x dX/deta|^2`, is not above the *absolute*
//! constant `1.0E-11` in the model's own length unit to the fourth power. The
//! solver was written for inch models. In metres a trailing-edge rib panel
//! 28 mm long and 0.47 mm high (half-edges 14 mm and 0.24 mm,
//! `|V1 x V2|^2` of about 1.1e-11) sits on the limit, so any wing whose box
//! closes towards the trailing edge (B747-400, DC-10, E195-E2 in the
//! 2026-10-07 matrix: 11, 12 and 2 elements) fails before a stiffness is
//! formed. The check has no relative form.
//!
//! # Why not millimetres
//!
//! The obvious cure, mm - tonne - N - MPa, was tried and rejected. NASTRAN-95
//! is single precision and its results move with the length unit. The same
//! A320 wingbox (200 stations by 50 chordwise points, identical physics,
//! solved at deck length unit `L` per metre, force in newtons throughout):
//!
//! | `L` (deck units / m) | tip pull-up [m] | f1 [Hz] | max centroid von Mises [MPa] |
//! |---|---|---|---|
//! | 1 (SI)      | 2.885 | 1.84 | 498 |
//! | 10          | 2.872 | 1.85 | 503 |
//! | 100         | 2.793 | 1.88 | 376 |
//! | 1000        | 2.783 | 1.89 | 364 |
//! | 10000       | 2.767 | 1.90 | 435 |
//!
//! The independent double-precision MSC Nastran solution of the same mesh gives
//! 2.928 m, 1.83 Hz and 502 MPa. Agreement is best for `L` of 1 and 10; from
//! `L` of 100 upward the shell stress drops by a quarter.
//!
//! # Where the scale dependence comes from
//!
//! It is not the solver's arithmetic. `closed_form.rs` solves a clamped box
//! beam with the product card writers and compares it with beam theory at
//! `L` = 1, 10, 100 and 1000: tip-load, self-weight (`GRAV`) and `CRBE3`-spider
//! load cases agree with the closed form to 1.1 % (deflection) and 0.2 %
//! (flange stress), and the results do not move with `L` (spread at most
//! 0.002 %). `tests/nastran95_wingbox_scale.rs` then solves one product
//! wingbox mesh (1423 `CQUAD4`) at each scale: with the mesh's rivet and
//! engine `RBE3`s (independent components `123456`) the pull-up tip moves
//! 5.182 / 5.202 / 5.144 / 5.133 m and the peak von Mises 1496 / 1508 / 1500 /
//! 1400 MPa, with the largest element changes in the load-introduction zone;
//! with the same cards reduced to translations (`123`) the tip is
//! 5.0881 / 5.0886 / 5.0884 / 5.0877 m and the peak 1303 / 1304 / 1306 /
//! 1301 MPa (within 0.4 %). An `RBE3` that averages rotational independent
//! components weights rotations and translations equally, so its effective
//! stiffness depends on the length unit by formulation. The dependence is a
//! property of that idealisation (MSC Nastran has it too, at its fixed SI
//! unit), not a defect of the 1995 solver. Agreement with MSC at `L` of 1 and
//! 10 therefore says that the rivet weighting resembles MSC's SI weighting
//! there, not that those units are validated: the peak stress itself differs
//! by 14 % between the `123456` and `123` idealisations at `L` = 10.
//!
//! # Decimetre - 10 kg - newton
//!
//! `L` of 10 keeps the agreement above and raises the degeneracy test's
//! margin by `10^4` (the quantity scales with the fourth power of length), so
//! the 28 mm by 0.47 mm panel is 1.1e-7 against 1e-11. The set is coherent:
//! a length unit of 0.1 m and a mass unit of 10 kg make one deck force unit
//! `10 kg * 0.1 m / s^2 = 1 N`, so loads need no conversion; stress and
//! modulus are in units of 100 Pa. Time is unchanged, so eigenvalues
//! (rad/s)^2 and frequencies are unit independent. The readers in
//! [`super::analysis`] convert displacements and stresses back to SI.
//!
//! The modern (MSC) dialect always stays in SI.

/// Deck numbers per SI unit for the two quantities that define a coherent set
/// (length and mass; time is the second in both).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitScale {
    /// Deck length units per metre.
    pub length: f64,
    /// Deck mass units per kilogram.
    pub mass: f64,
}

impl UnitScale {
    /// Metre - kilogram - newton - pascal: the historical NASTRAN-95 deck and
    /// the modern dialect.
    pub const SI: Self = Self {
        length: 1.0,
        mass: 1.0,
    };

    /// Decimetre - 10 kg - newton: the product NASTRAN-95 deck.
    pub const DECIMETRE: Self = Self {
        length: 10.0,
        mass: 0.1,
    };

    /// Deck force units per newton (`mass * length / s^2`).
    pub fn force(self) -> f64 {
        self.mass * self.length
    }

    /// Deck stress or modulus units per pascal.
    pub fn stress(self) -> f64 {
        self.force() / (self.length * self.length)
    }

    /// Deck density units per kg/m^3.
    pub fn density(self) -> f64 {
        self.mass / self.length.powi(3)
    }

    /// Deck acceleration units per m/s^2.
    pub fn acceleration(self) -> f64 {
        self.length
    }

    /// Deck area units per m^2.
    pub fn area(self) -> f64 {
        self.length * self.length
    }

    /// Deck second-moment units per m^4.
    pub fn inertia(self) -> f64 {
        self.length.powi(4)
    }
}

#[cfg(test)]
mod tests {
    use super::UnitScale;

    #[test]
    fn decimetre_set_is_coherent_with_the_newton_unchanged() {
        let u = UnitScale::DECIMETRE;
        assert!((u.force() - 1.0).abs() < 1e-15);
        // 71 GPa is 7.1e8 deck stress units (100 Pa); 2810 kg/m^3 is 2.81e-3
        // (10 kg)/dm^3; g is 98.0665 dm/s^2.
        assert!((71.0e9 * u.stress() - 7.1e8).abs() < 1.0);
        assert!((2810.0 * u.density() - 2.81e-1).abs() < 1e-12);
        assert!((9.80665 * u.acceleration() - 98.0665).abs() < 1e-12);
        assert!((u.area() - 100.0).abs() < 1e-12 && (u.inertia() - 1.0e4).abs() < 1e-9);
        let si = UnitScale::SI;
        assert!((si.force() - 1.0).abs() < 1e-15 && (si.stress() - 1.0).abs() < 1e-15);
    }

    #[test]
    fn the_degeneracy_margin_grows_with_the_fourth_power_of_length() {
        // |V1 x V2|^2 has dimension length^4: a 28 mm x 0.47 mm half-edge
        // panel is at the solver's 1e-11 limit in metres and 1e4 above it in
        // decimetres.
        let (v1, v2) = (0.014_f64, 0.000_235_f64);
        let metres = (v1 * v2).powi(2);
        assert!(metres < 1.5e-11 && metres > 0.5e-11);
        let decimetres = ((v1 * 10.0) * (v2 * 10.0)).powi(2);
        assert!((decimetres / metres - 1.0e4).abs() < 1e-6);
        assert!(decimetres > 1.0e-11 * 1.0e3);
    }
}
