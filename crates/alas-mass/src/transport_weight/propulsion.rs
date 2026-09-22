// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission reference.Methods.Weights.Correlations.Propulsion.engine_jet.engine_jet
// and .integrated_propulsion.integrated_propulsion.
// Upstream: mission reference 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! The jet-engine dry weight correlation and the integrated-propulsion
//! scale-up that [`super::empty_weight`] reaches on the `New mission reference` path.
//!
//! `empty_weight` picks this pair only when a network is a `Turbofan`,
//! `Turbojet_Super` or `Propulsor_Surrogate` *and* carries no `total_weight`
//! override, which is the case for every vehicle this program's mission reference bridge
//! builds. The FLOPS (`total_prop_flops`) and Raymer (`total_prop_Raymer`)
//! propulsion-weight paths are not translated.

use alas_units::{POUND_FORCE, POUND_MASS};

/// The dry weight of one jet engine given its sea-level static thrust:
/// `engine_jet`, a correlation over a set of production engines.
pub(crate) fn engine_jet(sealevel_static_thrust_n: f64) -> f64 {
    let thrust_sls_lbf = sealevel_static_thrust_n / POUND_FORCE;
    (0.4054 * thrust_sls_lbf.powf(0.9255)) * POUND_MASS
}

/// The whole propulsion system's weight: `integrated_propulsion`. Upstream
/// assumes the installed system (engines, exhaust, reversers, starting,
/// controls, lubrication, fuel system, nacelles and pylons) is a fixed 60%
/// heavier than the dry engines alone (`engine_wt_factor` default 1.6, never
/// overridden by [`super::empty_weight`]).
pub(crate) fn integrated_propulsion(engine_jet_kg: f64, num_engines: f64) -> f64 {
    const ENGINE_WEIGHT_FACTOR: f64 = 1.6;
    engine_jet_kg * num_engines * ENGINE_WEIGHT_FACTOR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrated_propulsion_is_sixty_percent_above_the_dry_engines() {
        let dry = engine_jet(600_000.0);
        let installed = integrated_propulsion(dry, 2.0);
        assert!((installed - dry * 2.0 * 1.6).abs() < 1e-9);
    }

    #[test]
    fn engine_jet_grows_with_thrust() {
        let small = engine_jet(200_000.0);
        let large = engine_jet(600_000.0);
        assert!(large > small, "large={large}, small={small}");
    }
}
