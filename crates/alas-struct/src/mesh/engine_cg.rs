// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where a wing-mounted engine's centre of gravity sits in the product deck.
//!
//! The configuration declares each engine's inlet station ahead of the local
//! leading edge (`inlet_x_offset_m`, optionally per engine), the nacelle
//! profile, and the centreline height above the wing reference plane (`z_m`,
//! optionally per engine). The wing torque of [`crate::torque_sources`] already
//! places the engine centre of gravity from exactly these data (mid-nacelle,
//! `ENGINE_CG_NACELLE_FRACTION = 0.5`); the deck uses the same point so the
//! finite-element torsion and mode shapes see the arm the native beam sees.
//! Before this module the deck hung every engine 0.15 local chords ahead of the
//! front-spar grid and 1 m below it, whatever the aircraft declared (a
//! turboprop's gearbox sits about a nacelle length ahead of the box).
//!
//! Frame: X aft, Y outboard, Z up, metres, the deck's basic frame; the wing
//! leading edge and reference-plane height come from
//! [`WingStructureGeometry::x_le`] / [`WingStructureGeometry::z_le`].

use alas_config::EngineConfig;
use alas_geom::wing_structure::WingStructureGeometry;

use crate::torque_sources::ENGINE_CG_NACELLE_FRACTION;

/// Centre of gravity of the engine declared at semi-span station `y_engine`,
/// in the deck frame, or `None` when no declared engine sits at that station
/// or the declaration is not finite.
pub(super) fn declared_engine_cg(
    wsg: &WingStructureGeometry,
    engine_cfg: &EngineConfig,
    y_engine: f64,
) -> Option<[f64; 3]> {
    let index = engine_cfg
        .spanwise_positions_m
        .iter()
        .position(|&y| (y.abs() - y_engine.abs()).abs() < 1.0e-9)?;
    let (lo, hi) = engine_cfg
        .nacelle_profile
        .iter()
        .map(|p| p.0)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
            (lo.min(x), hi.max(x))
        });
    let nacelle_length = if hi > lo { hi - lo } else { 0.0 };
    let cg_ahead =
        engine_cfg.inlet_x_offset_for_engine_m(index) - ENGINE_CG_NACELLE_FRACTION * nacelle_length;
    let eta = (y_engine.abs() / wsg.semi_span).clamp(0.0, 1.0);
    let cg = [
        wsg.x_le(eta) - cg_ahead,
        y_engine.abs(),
        wsg.z_le(eta) + engine_cfg.z_for_engine_m(index),
    ];
    cg.iter().all(|v| v.is_finite()).then_some(cg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_undeclared_station_has_no_declared_cg() {
        let wsg = test_geometry();
        let engine = EngineConfig {
            spanwise_positions_m: vec![5.0, -5.0],
            ..EngineConfig::default()
        };
        assert!(declared_engine_cg(&wsg, &engine, 9.0).is_none());
    }

    #[test]
    fn the_cg_is_ahead_of_the_leading_edge_by_the_declared_inlet_offset() {
        let wsg = test_geometry();
        let engine = EngineConfig {
            spanwise_positions_m: vec![5.0, -5.0],
            inlet_x_offset_m: 1.2,
            nacelle_profile: vec![(0.0, 0.5), (2.0, 0.6)],
            z_m: -0.9,
            ..EngineConfig::default()
        };
        let cg = declared_engine_cg(&wsg, &engine, 5.0).expect("declared engine");
        let eta = 5.0 / wsg.semi_span;
        // 1.2 m inlet offset minus half of the 2 m nacelle: 0.2 m ahead of the LE.
        assert!((cg[0] - (wsg.x_le(eta) - 0.2)).abs() < 1.0e-12);
        assert!((cg[1] - 5.0).abs() < 1.0e-12);
        assert!((cg[2] - (wsg.z_le(eta) - 0.9)).abs() < 1.0e-12);
    }

    fn test_geometry() -> WingStructureGeometry {
        use alas_config::{DesignVector, WingConfig};
        use alas_geom::aircraft::airfoil::Airfoil;
        let foil = Airfoil::from_name("naca2412").expect("4-digit NACA name parses");
        WingStructureGeometry::new(
            &DesignVector::default(),
            &WingConfig::default(),
            &foil,
            &foil,
            &[0.25, 0.70],
            None,
        )
        .expect("two full-span spars is a valid configuration")
    }
}
