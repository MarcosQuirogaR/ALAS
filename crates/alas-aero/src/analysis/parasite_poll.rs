// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The jet-transport parasite build-up of Poll and Schumann.
//!
//! D. I. A. Poll and U. Schumann, "An estimation method for the fuel burn and
//! other performance characteristics of civil transport aircraft during
//! cruise: Part 2, determining the aircraft's characteristic parameters",
//! *The Aeronautical Journal* 125 (1284), 2021, Appendix B (pp. 41-43). Each
//! component contributes `CF_i (FF)_i (FIFS)_i S_wet,i / S_ref`, the product
//! `FIFS` of the interference factor `FI` and the secondary-drag factor `FS`
//! (excrescences, leakage, protuberances, fairings, pylons and the
//! afterbody's own drag), with the skin friction `CF_i = 0.0269 / Re_i^0.14`
//! on the component's own length (Eq. 7 with Eqs. B-5, B-7, B-9, B-11, B-13).
//! The fuselage form factor is Torenbeek's (Eq. B-3); the wing, tail and
//! nacelle factors are those of Jenkinson et al. (Eqs. B-4, B-6, B-8, B-10,
//! B-12). With them the paper reproduces Obert's trimmed flight-test zero-lift
//! drag of 26 civil transports (Obert, *Aerodynamic Design of Transport
//! Aircraft*, 2009, Fig. 40.17) to about +-10 % (pp. 12-14), from regional
//! and business jets with rear-mounted engines (CRJ, E135/E145, MD-80, B717,
//! Gulfstream V) to the A380. The paper's overall factor `Fo = 0.98` (Eq.
//! B-14) is its fit "for best agreement with the flight data" and is not
//! applied: the sum stands as the method's own.
//!
//! Its sample holds no propeller aircraft, so the turboprop keeps the
//! Torenbeek and Raymer build-up of [`super::parasite`] with its own items and
//! the propeller row of Raymer's leakage and protuberance allowance.

/// `(FIFS)_w` of the wing, Eq. B-6.
pub const WING_FIFS: f64 = 1.06;
/// `(FIFS)_ht` and `(FIFS)_vt` of the tail surfaces, Eqs. B-8 and B-10.
pub const TAIL_FIFS: f64 = 1.20;
/// `(FIFS)_f` of the fuselage, Eq. B-4.
pub const FUSELAGE_FIFS: f64 = 1.10;
/// `(FF FI FS)_nac` of a nacelle, Eq. B-12: form, interference and
/// secondary drag of the pod and its pylon in one factor.
pub const NACELLE_FF_FI_FS: f64 = 1.44;

/// Mean skin-friction coefficient `CF = 0.0269 / Re^0.14` (Eq. 7), Re on
/// the component's own length; no Mach term (the data are at Mach 0.5 to
/// 0.6, below compressibility, p. 13).
#[must_use]
pub fn skin_friction(reynolds: f64) -> f64 {
    0.0269 / reynolds.powf(0.14)
}

/// Wing form factor `1 + 2.9 (t/c) cos^2(sweep)`, Eq. B-6, `t/c` the mean
/// thickness ratio and `sweep_deg` the sweep of the maximum-thickness line.
#[must_use]
pub fn wing_form_factor(thickness: f64, sweep_deg: f64) -> f64 {
    1.0 + 2.9 * thickness * sweep_deg.to_radians().cos().powi(2)
}

/// Horizontal-tail form factor, Eq. B-8: the paper's value 1.20. Its
/// expression `1 + 3.5 (t/c)` gives that value at t/c 0.057 only (1.32 to 1.42
/// at the 0.09 to 0.12 of transport tails), so the printed values are the ones
/// the paper's comparison with Obert's flight data was made with, and are
/// used for every horizontal surface.
pub const HORIZONTAL_TAIL_FF: f64 = 1.20;
/// Vertical-tail form factor, Eq. B-10: the paper's value 1.18 (see
/// [`HORIZONTAL_TAIL_FF`]), for the fin and its dorsal.
pub const VERTICAL_TAIL_FF: f64 = 1.18;

/// Fuselage form factor `1 + 2.2 / lambda^1.5 + 3.8 / lambda^3`, Eq. B-3,
/// `lambda` the length over the equivalent diameter (Eq. B-1); 1 for a
/// degenerate body.
#[must_use]
pub fn fuselage_form_factor(fineness: f64) -> f64 {
    if fineness.is_finite() && fineness > 0.0 {
        1.0 + 2.2 / fineness.powf(1.5) + 3.8 / fineness.powi(3)
    } else {
        1.0
    }
}

/// Reynolds length of a lifting surface, Eqs. B-7, B-9 and B-11: the mean
/// geometric chord, planform area over span (fin: over height).
#[must_use]
pub fn mean_chord(area_m2: f64, span_m: f64) -> f64 {
    if span_m > 0.0 {
        area_m2 / span_m
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_appendix_b_factors_hold_their_printed_values() {
        assert!((HORIZONTAL_TAIL_FF * TAIL_FIFS - 1.44).abs() < 1e-12);
        assert!((wing_form_factor(0.12, 0.0) - 1.348).abs() < 1e-12);
        assert!((wing_form_factor(0.12, 60.0) - 1.087).abs() < 1e-12);
        // Fineness 10: 1 + 2.2/31.623 + 3.8/1000.
        assert!((fuselage_form_factor(10.0) - 1.073_370).abs() < 1e-6);
        assert!((skin_friction(1e8) - 0.0269 * 1e8_f64.powf(-0.14)).abs() < 1e-15);
        assert!((mean_chord(120.0, 30.0) - 4.0).abs() < 1e-12);
    }
}
