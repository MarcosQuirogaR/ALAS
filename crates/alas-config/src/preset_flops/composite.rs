// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per-aircraft FLOPS wing `FCOMP` from construction facts (fix lane E1, D5).
//!
//! ## What `FCOMP` is
//!
//! NASA/TM-2017-219627 Vol. I, nomenclature (printed p. 78) and the variable
//! lists under Eq. 33 and the Eq. 43 text: "FCOMP = Composite utilization
//! factor for wing structure ranging from 0.0 for no composites to 1.0 for
//! maximum use of composites". It is a 0..1 index of how composite the wing
//! *structure* is. It is not a measured mass fraction, and no manufacturer
//! publishes it. It enters only the wing: Eq. 33 `W1NIR` carries
//! `(1 - 0.4 FCOMP)`, Eq. 35 `W2` carries `(1 - 0.17 FCOMP)`, Eq. 36 `W3`
//! carries `(1 - 0.3 FCOMP)`. The TM has no fuselage or tail composite
//! factor (Eq. 56 has none), so none is applied here.
//!
//! ## How the point values are chosen
//!
//! The TM gives no map from construction to the index, so the values below
//! are modelling choices made from the **wing-box construction fact only**
//! and never from an OEW comparison:
//!
//! * wing box (covers, spars, ribs) of carbon-fibre composite: 1.0, the
//!   TM's "maximum use of composites" end;
//! * a metallic box: 0.0;
//! * a metallic box with a composite centre box/ribs/rear spar: a partial
//!   value, with the choice stated.
//!
//! Raymer 6th ed. Table 15.4 (p. 580) gives an advanced-composite wing
//! multiplier of 0.85-0.90, i.e. a 10-15 % wing-mass saving, against about
//! 30 % for FCOMP = 1 in the FLOPS wing terms at typical loads. The two
//! disagree by a factor of two to three on the wing saving, so FCOMP = 1 is
//! the **upper** end of the plausible composite credit; the OEW effect is
//! reported by the probe example `oew_composite_probe` of `alas-mass`, and
//! the wing-mass delta at FCOMP 0.5 and 1 is exported as a diagnostic scenario.

/// FLOPS wing `FCOMP` for a registered aircraft, 0..1.
///
/// Sources (manufacturer construction statements; the index mapping is the
/// modelling choice described in the module documentation):
///
/// * **B787-9: 1.0.** Boeing 787 ARFF composite-content diagram (2025 file),
///   p. 3: composite airframe, about 50 % by weight, with composite wing and
///   fuselage. The ALAS structures preset already declares CFRP skins and
///   webs for it (`preset_structures`).
/// * **A220-300: 1.0.** Airbus A220 airframe features (July 2025) and FAST
///   63 (2019): carbon-fibre composite wing (resin-transfer-infused box).
///   The fuselage is aluminium-lithium, so no fuselage credit follows.
/// * **AVE: 1.0.** Notional 777X-class twin whose wing is declared at the
///   777-9 drawing; the 777-9 wing is a carbon-fibre composite wing (Boeing
///   777X product statements). This statement was not re-read in this lane;
///   it is the weakest of the three composite values and is flagged as such.
/// * **A380-800: 0.25, partial.** The A380 wing covers and spars are
///   aluminium; carbon-fibre composite is used in the centre wing box, rear
///   spar and ribs, and the aircraft as a whole is stated at 25 % composites
///   and advanced alloys by structure weight (Airbus launch material, 2005).
///   The 25 % is an airframe-wide figure, not a wing index, so the value is
///   only a partial-credit modelling choice: bracket 0.1-0.4.
/// * **A400M: 0.3.** Kept from the existing declaration (CFRP wing skins
///   mentioned by a secondary source, `military::adjust_structure`).
/// * **A320-200, A340-300, DC-10, B747-400, C919, E195-E2, ATR72-600: 0.**
///   Metallic wing boxes (the A320 and A340 composite empennage is not
///   scaled by FCOMP in FLOPS). ATR72-600: the outer-box material is not
///   established by a retained source, so the metallic endpoint is kept.
pub(super) fn declared_composite_utilization(name: &str) -> f64 {
    match name {
        "B787-9" | "A220-300" | "AVE" => 1.0,
        "A380-800" => 0.25,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_wing_values_follow_the_construction_facts() {
        for name in ["B787-9", "A220-300", "AVE"] {
            assert_eq!(declared_composite_utilization(name), 1.0, "{name}");
        }
        assert_eq!(declared_composite_utilization("A380-800"), 0.25);
        for name in [
            "A320-200",
            "A340-300",
            "DC-10",
            "B747-400",
            "C919",
            "E195-E2",
            "ATR72-600",
        ] {
            assert_eq!(declared_composite_utilization(name), 0.0, "{name}");
        }
    }

    #[test]
    fn every_value_is_inside_the_flops_index_range() {
        for name in crate::presets::available() {
            let value = declared_composite_utilization(name);
            assert!((0.0..=1.0).contains(&value), "{name}: {value}");
        }
    }
}
