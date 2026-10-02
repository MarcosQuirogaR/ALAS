// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! [`AirfoilClass`]: the declared section technology of the main wing.
//!
//! The Korn drag-divergence relation `M_dd + CL/10 + t/c = kappa_A` carries
//! the section technology in one factor, `kappa_A` (as given by
//! W. H. Mason, *Configuration Aerodynamics*, Virginia Tech course notes,
//! ch. 7 "Transonic Aerodynamics of Airfoils and Wings", the Korn equation;
//! B. Malone and W. H. Mason, "Multidisciplinary Optimization in Aircraft
//! Design Using Analytic Technology Models", *J. Aircraft* 32(2), 1995,
//! 431-438). Both give `kappa_A = 0.87` for a NACA 6-series (conventional)
//! section and `0.95` for a supercritical section. The factor is therefore a
//! consequence of which kind of section the wing carries, declared with the
//! aircraft, rather than a free coefficient.

use serde::{Deserialize, Serialize};

/// Section technology of the main wing, which fixes the Korn technology
/// factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AirfoilClass {
    /// NACA 6-series-era sections with a suction peak near the leading edge:
    /// transport wings designed before supercritical sections entered
    /// service.
    Conventional,
    /// Supercritical (aft-loaded, flat-roof) sections after Whitcomb and the
    /// NASA SC(2) family (C. D. Harris, "NASA Supercritical Airfoils",
    /// NASA TP-2969, 1990). The clean-sheet default: every current
    /// transonic transport wing carries them.
    #[default]
    Supercritical,
}

impl AirfoilClass {
    /// The Korn technology factor `kappa_A` of this section class: 0.87
    /// conventional (NACA 6-series), 0.95 supercritical (Mason,
    /// *Configuration Aerodynamics*, ch. 7, the Korn equation; Malone and
    /// Mason 1995).
    pub const fn korn_technology_factor(self) -> f64 {
        match self {
            Self::Conventional => 0.87,
            Self::Supercritical => 0.95,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_class_sets_the_published_korn_technology_factor() {
        assert_eq!(AirfoilClass::Conventional.korn_technology_factor(), 0.87);
        assert_eq!(AirfoilClass::Supercritical.korn_technology_factor(), 0.95);
        assert_eq!(AirfoilClass::default(), AirfoilClass::Supercritical);
    }
}
