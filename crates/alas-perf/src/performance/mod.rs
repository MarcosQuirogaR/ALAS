// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/performance.py
// Reference: alas @ rust-port-baseline.

//! Low-speed performance and matching-chart calculations.
//!
//! The matching chart turns a set of requirements into a design point in
//! thrust-to-weight versus wing-loading space, bounded by four constraints:
//! cruise ([`constraints::tw_cruise_constraint`]), engine-out second-segment
//! climb ([`constraints::tw_oei_climb_constraint`]), take-off field length
//! ([`constraints::tw_takeoff_constraint`]) and landing field length
//! ([`constraints::ws_landing_limit`]). [`constraints::build_matching_chart`]
//! assembles all four across a set of aerodromes. The FAR-25 V-speed schedule
//! ([`speeds::compute_v_speeds`]) and the estimated field distances
//! ([`speeds::compute_field_performance`]) are derived from the same empirical
//! constants so the numbers a design reports agree with the boundary it was
//! sized against. [`envelope::breguet_range_m`] and
//! [`envelope::build_vn_diagram`] round out the surface with cruise range and
//! the flight envelope.
//!
//! Every formula is SI in and SI out unless a name says otherwise. The
//! empirical constants (37.7, K = 0.60) are Raymer's regression coefficients
//! for jet transports (*Aircraft Design: A Conceptual Approach*, 5th ed., Ch.
//! 17 & 21), originally calibrated in US customary units; the unit-conversion
//! factors below are applied inline where the formula reaches them.
//!
//! Scoped out of this row and recorded in `docs/PORTING.md`:
//! `wing_fuel_volume_m3` (reads a built [`alas_geom`]-side `Wing`, which this
//! crate does not yet depend on), `payload_range_diagram` and
//! `fuel_volume_check` (both orchestrate a full-analysis report object that
//! only exists in P10), and `static_thrust_to_weight` (an `ALASConfig`
//! accessor with an exception fallback, an app-layer concern). Everything
//! whose inputs already exist in P4 is here.
//!
//! [`alas_geom`]: https://docs.rs/alas-geom

// A test asserts on values it loaded from a fixture it controls, so a failed
// unwrap or expect there is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod constraints;
pub mod envelope;
pub mod speeds;

pub use constraints::{
    build_matching_chart, tw_cruise_constraint, tw_oei_climb_constraint, tw_takeoff_constraint,
    ws_landing_limit, MatchingChartData,
};
pub use envelope::{breguet_range_m, build_vn_diagram, VnDiagramData};
pub use speeds::{compute_field_performance, compute_v_speeds, FieldPerformance, VSpeeds};

use alas_atmo::Atmosphere;

/// Standard gravity as `performance.py` writes it (`_G`), a two-decimal
/// figure rather than the CODATA `9.80665`; kept as-is so the reproduced
/// numbers match rather than being a hundredth of a per-cent off.
pub(crate) const G: f64 = 9.81;
/// Pascals to pounds-force per square foot (`_PA_TO_PSF`).
pub(crate) const PA_TO_PSF: f64 = 0.020885;
/// Metres to feet (`_M_TO_FT`).
pub(crate) const M_TO_FT: f64 = 3.28084;
/// Sea-level ISA density, kg/m^3, the matching chart references wing loading
/// against.
pub(crate) const RHO_SL: f64 = 1.225;

/// FAR 25.121 second-segment climb minimum gross gradient by engine count:
/// twin 2.4%, tri-jet 2.7%, quad 3.0% (`FAR25_OEI_GRADIENT`).
///
/// Upstream stores this as a plain `dict`; its consumers read it with `.get`
/// and supply their own fallback for any other engine count, which is why this
/// returns `None` rather than a default. The matching chart itself takes the
/// gradient as an argument -- this table is the selection its caller
/// (`objective.py`, a later row) applies before calling in.
pub fn far25_oei_gradient(n_engines: i64) -> Option<f64> {
    match n_engines {
        2 => Some(0.024),
        3 => Some(0.027),
        4 => Some(0.030),
        _ => None,
    }
}

/// Density ratio sigma = rho/rho0 at field elevation with an ISA offset --
/// `density_ratio`.
///
/// The ISA offset is added to the model temperature and the density is
/// recovered from the ideal-gas law at a hardcoded `287.05` J/(kg*K), *not*
/// through [`Atmosphere::density`]: upstream writes the constant out at two
/// decimals and applies the offset by hand, so reproducing its number means
/// reproducing both choices rather than reaching for the crate's own more
/// precise gas constant.
pub fn density_ratio(elevation_m: f64, delta_isa_c: f64) -> f64 {
    let atmo = Atmosphere::new(elevation_m);
    let temp_k = atmo.temperature() + delta_isa_c;
    let rho = atmo.pressure() / (287.05 * temp_k);
    rho / RHO_SL
}

/// `numpy.linspace(start, stop, num)` with the default inclusive endpoint.
///
/// The interior points are `start + step * i`; the last is set to `stop`
/// exactly, which is what NumPy does and what keeps the axis endpoints
/// bit-identical rather than a rounding of `start + step * (num - 1)`.
pub(crate) fn linspace(start: f64, stop: f64, num: i64) -> Vec<f64> {
    if num <= 0 {
        return Vec::new();
    }
    if num == 1 {
        return vec![start];
    }
    let n = num as usize;
    let step = (stop - start) / (num - 1) as f64;
    let mut values: Vec<f64> = (0..n).map(|i| start + step * i as f64).collect();
    values[n - 1] = stop;
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_oei_gradient_table_is_the_far25_schedule_and_nothing_else() {
        assert_eq!(far25_oei_gradient(2), Some(0.024));
        assert_eq!(far25_oei_gradient(3), Some(0.027));
        assert_eq!(far25_oei_gradient(4), Some(0.030));
        // A count with no certified figure has no entry; the caller supplies
        // the fallback, as upstream's `.get(n, default)` does.
        assert_eq!(far25_oei_gradient(1), None);
        assert_eq!(far25_oei_gradient(6), None);
    }

    #[test]
    fn linspace_places_both_endpoints_exactly() {
        let grid = linspace(2000.0, 10000.0, 5);
        assert_eq!(grid.len(), 5);
        assert_eq!(grid[0], 2000.0);
        assert_eq!(grid[4], 10000.0);
        assert_eq!(grid[2], 6000.0);
    }

    #[test]
    fn a_higher_field_is_thinner_air() {
        // The density ratio falls with elevation; a hot day thins it further.
        assert!(density_ratio(2400.0, 0.0) < density_ratio(0.0, 0.0));
        assert!(density_ratio(2400.0, 15.0) < density_ratio(2400.0, 0.0));
    }
}
