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
    /// NASA TP-2969, 1990): transport wings first flown from the middle of
    /// the 1970s to the end of the 1990s (A320, A330/A340). The clean-sheet
    /// default.
    #[default]
    Supercritical,
    /// "Peaky" sections of the 1960s jet transports, with a small supersonic
    /// pocket behind the leading edge closed almost without a shock
    /// (E. Torenbeek, *Advanced Aircraft Design*, 2013, p. 301; DC-9, DC-10,
    /// 747, VC-10): wings first flown between 1960 and the middle of the
    /// 1970s.
    Peaky,
    /// Supercritical sections of the wings designed with three-dimensional
    /// transonic CFD and first flown from 2000 on (A380, 787, A350, A220,
    /// E2): Kroo's "very aggressive" supercritical level (see
    /// [`AirfoilClass::crest_critical_mach_increment`]).
    SupercriticalAdvanced,
}

impl AirfoilClass {
    /// Root and outboard library sections a conventional-class jet transport
    /// draws when its own section coordinates are unpublished: the Boeing 737
    /// root and outboard stations of the UIUC Airfoil Coordinates Database
    /// (`b737a`, `b737d`), the library's only published spanwise station set
    /// from a swept jet-transport wing of the 1960s, the decade of the
    /// "peaky" sections (E. Obert, *Aerodynamic Design of Transport
    /// Aircraft*, IOS Press, 2009, p. 32). Their camber lines are
    /// front-loaded with little rear loading: thin-airfoil `c_m,c/4` -0.013
    /// at the root and -0.022 outboard, maximum camber at 5 and 20 % chord. That
    /// is the loading Obert prints for the sections of the two
    /// conventional-class widebodies, whose presets draw those sections
    /// themselves, digitised (`alas-geom` `digitised_sections`): the DC-10's
    /// peaky basic section has "very little rear loading" (p. 207; its
    /// drawing, Fig. 24.23, p. 208, from AIAA 69-830, digitises to `c_m,c/4`
    /// -0.013) and the 747's stations, which show the influence of peaky
    /// sections (p. 239; Fig. 24.82, p. 243, from NASA SP-347, 2y/b 0.11 to
    /// 0.50), digitise to -0.014 to -0.044. The surrogate supplies the camber
    /// line and shape only: a preset's thickness stations set its t/c, as
    /// multipliers on the drawn sections' own maximum t/c.
    pub const CONVENTIONAL_TRANSPORT_SECTIONS: (&'static str, &'static str) = ("b737a", "b737d");

    /// The Korn technology factor `kappa_A` of this section class: 0.87
    /// conventional (NACA 6-series), 0.95 supercritical (Mason,
    /// *Configuration Aerodynamics*, ch. 7, the Korn equation; Malone and
    /// Mason 1995).
    pub const fn korn_technology_factor(self) -> f64 {
        match self {
            // Mason prints two families; a peaky section has no factor of its
            // own there and keeps the conventional one.
            Self::Conventional | Self::Peaky => 0.87,
            Self::Supercritical | Self::SupercriticalAdvanced => 0.95,
        }
    }

    /// Whether the class is one of the supercritical generations.
    pub const fn is_supercritical(self) -> bool {
        matches!(self, Self::Supercritical | Self::SupercriticalAdvanced)
    }

    /// Increment of the crest-critical Mach number of the Shevell-Kroo
    /// compressibility-drag method over its chart, which is drawn for the
    /// peaky section family.
    ///
    /// - Peaky: 0, the chart itself (I. Kroo, *Aircraft Design: Synthesis and
    ///   Analysis*, Stanford AA241 notes, 2001, sec. 4.3; R. S. Shevell,
    ///   *Fundamentals of Flight*).
    /// - Supercritical: +0.035, the middle of Kroo's "more typically the
    ///   increment is 0.03 to 0.04 above the peaky sections".
    /// - Advanced supercritical: +0.06, Kroo's "a very aggressive
    ///   supercritical section might achieve a drag divergence Mach number
    ///   increment of 0.06", the shift Shevell (1980) found between
    ///   conventional-wing aircraft data and a supercritical wing (both as
    ///   quoted by S. Ciornei, *Mach number, relative thickness, sweep and
    ///   lift coefficient of the wing*, HAW Hamburg, 2005, pp. 43 and 46).
    /// - Conventional: -0.04, the middle of Torenbeek's statement that a
    ///   peaky section's drag-critical Mach number is "approximately .03 and
    ///   .05 higher" than that of a conventional section of the same
    ///   thickness ratio (*Synthesis of Subsonic Airplane Design*, 1982, as
    ///   quoted by Ciornei, p. 32).
    ///
    /// Each value is the printed one or the middle of a printed range, set by
    /// the section generation and by nothing else.
    pub const fn crest_critical_mach_increment(self) -> f64 {
        match self {
            Self::Conventional => -0.04,
            Self::Peaky => 0.0,
            Self::Supercritical => 0.035,
            Self::SupercriticalAdvanced => 0.06,
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
        assert_eq!(AirfoilClass::Peaky.korn_technology_factor(), 0.87);
        assert_eq!(
            AirfoilClass::SupercriticalAdvanced.korn_technology_factor(),
            0.95
        );
    }

    #[test]
    fn the_generation_sets_the_printed_crest_critical_mach_increment() {
        use AirfoilClass::{Conventional, Peaky, Supercritical, SupercriticalAdvanced};
        assert_eq!(Conventional.crest_critical_mach_increment(), -0.04);
        assert_eq!(Peaky.crest_critical_mach_increment(), 0.0);
        assert_eq!(Supercritical.crest_critical_mach_increment(), 0.035);
        assert_eq!(SupercriticalAdvanced.crest_critical_mach_increment(), 0.06);
        assert_eq!(
            serde_json::to_value(SupercriticalAdvanced).ok(),
            Some(serde_json::json!("supercritical_advanced"))
        );
    }
}
