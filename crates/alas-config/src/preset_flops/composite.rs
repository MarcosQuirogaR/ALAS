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
//! ## How FCOMP is derived (declared fraction x sourced credit scale)
//!
//! The TM defines only the endpoints (0 = no composites, 1 = "maximum use of
//! composites"), so the index needs a scale. FCOMP is computed as
//!
//! ```text
//! FCOMP = wing_composite_fraction * FCOMP_ADVANCED_COMPOSITE_WING
//! ```
//!
//! * `wing_composite_fraction` (0..1) is a **declared construction input**
//!   per aircraft, from the wing-box construction statement and never from an
//!   OEW comparison: 1.0 for a carbon-fibre box (covers, spars, ribs), 0.0
//!   for a metallic box, a stated partial value otherwise.
//! * `FCOMP_ADVANCED_COMPOSITE_WING` is the FLOPS index at which the FLOPS
//!   wing terms reproduce Raymer's advanced-composite wing credit. Raymer 6th
//!   ed. Table 15.4 (printed p. 580, "Weights Estimation Fudge Factors"),
//!   Advanced composites, Wing: multiplier 0.85-0.90, midpoint 0.875, i.e. a
//!   12.5 % wing-mass saving (10-15 %). The FLOPS terms at FCOMP = 1 remove
//!   about 30 % of the wing (Eq. 33 x0.6, Eq. 35 x0.83, Eq. 36 x0.7; measured
//!   30.2 % B787-9 and 28.1 % A220-300 at the 725ebf92 geometry). The scale is
//!   one global constant: 0.125 / 0.30 = 0.4167 (Raymer band 0.85..0.90 maps to
//!   0.50..0.33). It is not tuned per aircraft or to any OEW.
//!
//! Before this rule FCOMP = 1.0 was applied (about 30 % saving, 2.4x Raymer's),
//! which moved the B787-9 and A220 OEW by -6.4 % and -3.5 % against the
//! published values (validation correction 1, 2026-10-16).

/// Raymer 6th ed. Table 15.4: advanced-composite wing multiplier midpoint
/// (band 0.85-0.90).
pub(super) const RAYMER_COMPOSITE_WING_MULTIPLIER: f64 = 0.875;
/// Wing-mass saving of the FLOPS wing terms at FCOMP = 1 at typical transport
/// loads (NASA/TM-2017-219627 Eq. 33, 35, 36; measured 28-30 %).
pub(super) const FLOPS_WING_SAVING_AT_FCOMP_1: f64 = 0.30;
/// FLOPS FCOMP that reproduces the Raymer advanced-composite wing credit.
pub(super) const FCOMP_ADVANCED_COMPOSITE_WING: f64 =
    (1.0 - RAYMER_COMPOSITE_WING_MULTIPLIER) / FLOPS_WING_SAVING_AT_FCOMP_1;

/// FLOPS wing `FCOMP` for a registered aircraft, 0..1: the declared composite
/// fraction of the wing structure times the Raymer-consistent credit scale.
pub(super) fn declared_composite_utilization(name: &str) -> f64 {
    declared_wing_composite_fraction(name) * FCOMP_ADVANCED_COMPOSITE_WING
}

/// Declared composite fraction of the wing structure, 0..1 (an input, not a
/// fit).
///
/// Sources (manufacturer construction statements):
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
fn declared_wing_composite_fraction(name: &str) -> f64 {
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
    fn credit_scale_reproduces_the_raymer_band() {
        // Raymer Table 15.4 wing 0.85-0.90 maps inside FCOMP 0.33..0.50.
        assert!((FCOMP_ADVANCED_COMPOSITE_WING - 0.125 / 0.30).abs() < 1e-12);
        assert!((0.10 / 0.30..=0.15 / 0.30).contains(&FCOMP_ADVANCED_COMPOSITE_WING));
    }

    #[test]
    fn composite_wing_values_follow_the_construction_facts() {
        for name in ["B787-9", "A220-300", "AVE"] {
            assert_eq!(
                declared_composite_utilization(name),
                FCOMP_ADVANCED_COMPOSITE_WING,
                "{name}"
            );
        }
        assert_eq!(
            declared_composite_utilization("A380-800"),
            0.25 * FCOMP_ADVANCED_COMPOSITE_WING
        );
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
