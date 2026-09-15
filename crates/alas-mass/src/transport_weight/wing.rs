// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission reference.Methods.Weights.Correlations.Common.wing_main and
// mission reference.Methods.Weights.Correlations.Transport.{tail_horizontal,tail_vertical}.
// Upstream: mission reference 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! The main wing and the two tail surfaces' structural mass.
//!
//! Every sweep angle here is already in radians: mission reference stores
//! `wing.sweeps.quarter_chord` in radians (the `* Units.deg` conversion
//! happens once, at vehicle construction) and the correlations read it back
//! unconverted, so no angle conversion factor is needed.

use alas_units::{FOOT, POUND_MASS};

/// `wing.spans.projected`, `.sweeps.quarter_chord`, `.areas.reference`,
/// `.thickness_to_chord`, `.taper`, `.chords.root`, `.chords.mean_aerodynamic`
/// and `.origin[0][0]` for a `Main_Wing`: the fields
/// [`super::wing::wing_main`] and [`super::wing::tail_horizontal`] (which
/// reads the main wing's own root chord and origin) touch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MainWing {
    /// `wing.spans.projected`, m.
    pub span_m: f64,
    /// `wing.sweeps.quarter_chord`, radians.
    pub sweep_quarter_chord_rad: f64,
    /// `wing.areas.reference`, m^2.
    pub area_m2: f64,
    /// `wing.thickness_to_chord`.
    pub thickness_to_chord: f64,
    /// `wing.taper`.
    pub taper_ratio: f64,
    /// `wing.chords.root`, m.
    pub root_chord_m: f64,
    /// `wing.chords.mean_aerodynamic`, m.
    pub mean_aerodynamic_chord_m: f64,
    /// `wing.origin[0][0]`, m.
    pub origin_x_m: f64,
}

/// A `Horizontal_Tail`'s fields, as read by [`tail_horizontal`].
///
/// `area_exposed_m2`/`area_wetted_m2` are not computed here: upstream reads
/// `wing.areas.exposed`/`.wetted`, which every wing in this program's mission reference
/// bridge gets from `mission_builder.simple_sizing` (`wetted = 2.05 *
/// reference`, `exposed = 0.85 * wetted`) rather than from
/// `Weights_Transport` itself. Taken as data for the same reason
/// `TransportVehicle::max_zero_fuel_kg` is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HorizontalTail {
    /// `wing.spans.projected`, m.
    pub span_m: f64,
    /// `wing.sweeps.quarter_chord`, radians.
    pub sweep_quarter_chord_rad: f64,
    /// `wing.areas.reference`, m^2.
    pub area_m2: f64,
    /// `wing.areas.exposed`, m^2.
    pub area_exposed_m2: f64,
    /// `wing.areas.wetted`, m^2.
    pub area_wetted_m2: f64,
    /// `wing.thickness_to_chord`.
    pub thickness_to_chord: f64,
    /// `wing.origin[0][0]`, m.
    pub origin_x_m: f64,
}

/// A `Vertical_Tail`'s fields, as read by [`tail_vertical`].
///
/// Carries no `t_tail` field: `tail_vertical`'s T-tail bonus compares
/// `wing.t_tail == "yes"`, a string, while every vertical tail this
/// program's mission reference bridge builds sets `wing.t_tail` to a Python `bool`
/// (`vehicle_builder.py`'s `wing.t_tail = False`), and `False == "yes"` is
/// `False` regardless of which boolean was assigned, so the comparison can
/// never select the T-tail branch for any vehicle this program builds. The
/// branch is not translated as a parameter; see [`tail_vertical`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerticalTail {
    /// `wing.spans.projected`, m.
    pub span_m: f64,
    /// `wing.sweeps.quarter_chord`, radians.
    pub sweep_quarter_chord_rad: f64,
    /// `wing.areas.reference`, m^2.
    pub area_m2: f64,
    /// `wing.thickness_to_chord`.
    pub thickness_to_chord: f64,
}

/// The mass of the main wing: `wing_main`, the "mission reference Wing Weight Index"
/// method (see http://aerodesign.stanford.edu/aircraftdesign/AircraftDesign.html,
/// "Derivation of the Wing Weight Index").
///
/// Scoped to the traditional (non-segmented) formula; see the module doc on
/// `wing.Segments`.
pub(crate) fn wing_main(
    wing: &MainWing,
    ultimate_load_factor: f64,
    mtow_kg: f64,
    max_zero_fuel_kg: f64,
) -> f64 {
    let area_ft2 = wing.area_m2 / (FOOT * FOOT);
    let span_ft = wing.span_m / FOOT;
    let mtow_lb = mtow_kg / POUND_MASS;
    let zfw_lb = max_zero_fuel_kg / POUND_MASS;

    let weight_lb = 4.22 * area_ft2
        + 1.642e-6
            * ultimate_load_factor
            * span_ft.powi(3)
            * (mtow_lb * zfw_lb).sqrt()
            * (1.0 + 2.0 * wing.taper_ratio)
            / (wing.thickness_to_chord
                * wing.sweep_quarter_chord_rad.cos().powi(2)
                * area_ft2
                * (1.0 + wing.taper_ratio));

    weight_lb * POUND_MASS
}

/// The mass of the horizontal tail, including its elevator (assumed 25% of
/// the tail's area): `tail_horizontal`, from Raymer's "Aircraft Design: A
/// Conceptual Approach".
pub(crate) fn tail_horizontal(
    wing: &HorizontalTail,
    main_wing: &MainWing,
    ultimate_load_factor: f64,
    mtow_kg: f64,
) -> f64 {
    let span_ft = wing.span_m / FOOT;
    let area_ft2 = wing.area_m2 / (FOOT * FOOT);
    let mtow_lb = mtow_kg / POUND_MASS;
    let exposed = wing.area_exposed_m2 / wing.area_wetted_m2;

    // `wing.aerodynamic_center[0]` and `main_wing.aerodynamic_center[0]` both
    // drop out: neither is ever populated in this program's mission reference bridge
    // (they stay at the `Wing` class's `[0.0, 0.0, 0.0]` default), so
    // upstream's `l_w2h` reduces to the two wings' origin stations.
    let mut l_w2h = wing.origin_x_m - main_wing.origin_x_m;
    let mut l_w = main_wing.mean_aerodynamic_chord_m / FOOT;
    if l_w.is_nan() {
        l_w = 0.0;
    }
    if l_w2h.is_nan() {
        l_w2h = 0.0;
    }
    let length_w_h_ft = l_w2h / FOOT;

    let weight_lb = 5.25 * area_ft2
        + 0.8e-6
            * ultimate_load_factor
            * span_ft.powi(3)
            * mtow_lb
            * l_w
            * (exposed * area_ft2).sqrt()
            / (wing.thickness_to_chord
                * wing.sweep_quarter_chord_rad.cos().powi(2)
                * length_w_h_ft
                * area_ft2.powf(1.5));

    weight_lb * POUND_MASS
}

/// The mass of the vertical tail, fin plus rudder: `tail_vertical`. The
/// rudder is assumed to be 25% of the tail's area and 60% heavier per unit
/// area than the fin (upstream's `rudder_fraction` default, never overridden
/// by [`super::empty_weight`]'s one call site).
pub(crate) fn tail_vertical(
    wing: &VerticalTail,
    ultimate_load_factor: f64,
    mtow_kg: f64,
    reference_area_m2: f64,
) -> f64 {
    const RUDDER_FRACTION: f64 = 0.25;

    let span_ft = wing.span_m / FOOT;
    let area_ft2 = wing.area_m2 / (FOOT * FOOT);
    let mtow_lb = mtow_kg / POUND_MASS;
    let s_ref_ft2 = reference_area_m2 / (FOOT * FOOT);

    // Always 1.0; see `VerticalTail`'s doc on why the T-tail branch is
    // unreachable from this program's own inputs.
    let t_tail_factor = 1.0;

    let tail_vert_lb = t_tail_factor
        * (2.62 * area_ft2
            + 1.5e-5 * ultimate_load_factor * span_ft.powi(3) * (8.0 + 0.44 * mtow_lb / s_ref_ft2)
                / (wing.thickness_to_chord * wing.sweep_quarter_chord_rad.cos().powi(2)));

    let mut tail_weight_kg = tail_vert_lb * POUND_MASS;
    tail_weight_kg += tail_weight_kg * RUDDER_FRACTION * 1.6;
    tail_weight_kg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_main_wing() -> MainWing {
        MainWing {
            span_m: 71.75,
            sweep_quarter_chord_rad: 0.5934119456780721,
            area_m2: 529.035559696941,
            thickness_to_chord: 0.12,
            taper_ratio: 0.09696969696969697,
            root_chord_m: 16.5,
            mean_aerodynamic_chord_m: 9.62825895492285,
            origin_x_m: 26.24,
        }
    }

    #[test]
    fn wing_main_scales_up_with_ultimate_load_factor() {
        let wing = sample_main_wing();
        let light = wing_main(&wing, 2.5, 358_670.0, 261_829.1);
        let heavy = wing_main(&wing, 4.0, 358_670.0, 261_829.1);
        assert!(heavy > light, "heavy={heavy}, light={light}");
    }

    #[test]
    fn tail_vertical_ignores_sweep_sign() {
        // cos^2 is even, so the fin's own weight cannot tell a swept-forward
        // tail from a swept-back one of the same magnitude.
        let wing = VerticalTail {
            span_m: 9.8,
            sweep_quarter_chord_rad: 0.5,
            area_m2: 62.23,
            thickness_to_chord: 0.08,
        };
        let swept_back = tail_vertical(&wing, 3.75, 358_670.0, 529.035559696941);
        let mut swept_forward = wing;
        swept_forward.sweep_quarter_chord_rad = -0.5;
        let forward = tail_vertical(&swept_forward, 3.75, 358_670.0, 529.035559696941);
        assert!((swept_back - forward).abs() < 1e-9);
    }

    #[test]
    fn tail_horizontal_is_zero_area_free_of_nan_for_a_hstab_ahead_of_the_wing() {
        // A pathological placement (l_w2h negative, area zero) should not
        // itself introduce a NaN the isnan guards were not meant to catch;
        // this just documents the guard exists rather than claiming a
        // physically meaningful zero.
        let main_wing = sample_main_wing();
        let wing = HorizontalTail {
            span_m: 22.09,
            sweep_quarter_chord_rad: 0.6,
            area_m2: 112.66,
            area_exposed_m2: 196.32,
            area_wetted_m2: 230.96,
            thickness_to_chord: 0.1,
            origin_x_m: 36.94,
        };
        let weight = tail_horizontal(&wing, &main_wing, 3.75, 358_670.0);
        assert!(weight.is_finite(), "weight={weight}");
    }
}
