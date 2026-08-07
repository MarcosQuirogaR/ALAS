// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/performance.py
// Reference: alas @ rust-port-baseline.

//! The four matching-chart constraint curves and the assembled chart.

use alas_atmo::Atmosphere;
use alas_config::airports::Airport;
use alas_config::PerformanceConfig;

use super::{linspace, M_TO_FT, PA_TO_PSF};

/// Cruise thrust-to-weight constraint, sea-level static -- `tw_cruise_constraint`.
///
/// Solves level flight (`L = W`, `T = D`) at the cruise design point and
/// converts the required altitude `T/W` to sea-level static through a fixed
/// thrust lapse `eta`:
///
/// ```text
/// T/W0 = [q*CD0/(W/S) + k*(W/S)/q] / eta
/// ```
///
/// Evaluated for every wing loading in `ws_pa` (Pa); returns one `T/W0` each.
pub fn tw_cruise_constraint(
    ws_pa: &[f64],
    cd0: f64,
    k: f64,
    cruise_mach: f64,
    cruise_altitude_m: f64,
    thrust_lapse: f64,
) -> Vec<f64> {
    let atmo = Atmosphere::new(cruise_altitude_m);
    let speed = cruise_mach * atmo.speed_of_sound();
    let q = 0.5 * atmo.density() * speed * speed;
    ws_pa
        .iter()
        .map(|&ws| (q * cd0 / ws + k * ws / q) / thrust_lapse)
        .collect()
}

/// Engine-out second-segment climb thrust-to-weight, constant in wing loading
/// -- `tw_oei_climb_constraint`.
///
/// FAR 25.121 / CS 25.121 requires `N/(N-1)` engines to sustain a minimum
/// climb gradient with one inoperative, evaluated at `cl_climb` in take-off
/// configuration; `delta_cd_to_config` is the flap/gear parasite-drag
/// increment on the clean `cd0`. A single-engine aircraft is not subject to
/// the rule and returns `0.0`.
pub fn tw_oei_climb_constraint(
    cd0: f64,
    k: f64,
    n_engines: i64,
    oei_gradient: f64,
    cl_climb: f64,
    delta_cd_to_config: f64,
) -> f64 {
    if n_engines < 2 {
        return 0.0;
    }
    let cd_to_config = cd0 + delta_cd_to_config + k * cl_climb * cl_climb;
    let ld_to = cl_climb / cd_to_config;
    let factor = n_engines as f64 / (n_engines - 1) as f64;
    factor * (1.0 / ld_to + oei_gradient)
}

/// Take-off field-length thrust-to-weight constraint -- `tw_takeoff_constraint`
/// (Raymer Ch. 17, empirical).
///
/// ```text
/// T/W = 37.7 * (W/S [psf]) / (sigma * CL_max_TO * TODA [ft])
/// ```
///
/// The `37.7` is Raymer's regression coefficient for jet transports. Evaluated
/// for every wing loading in `ws_pa` (Pa) against one take-off distance
/// available `toda_m` and density ratio `sigma`.
pub fn tw_takeoff_constraint(ws_pa: &[f64], toda_m: f64, sigma: f64, cl_max_to: f64) -> Vec<f64> {
    let toda_ft = toda_m * M_TO_FT;
    ws_pa
        .iter()
        .map(|&ws| 37.7 * (ws * PA_TO_PSF) / (sigma * cl_max_to * toda_ft))
        .collect()
}

/// Maximum wing loading [Pa] the landing-distance constraint allows --
/// `ws_landing_limit`.
///
/// ```text
/// (W/S)_max = LDA * sigma * CL_max_land / K
/// ```
pub fn ws_landing_limit(lda_m: f64, sigma: f64, cl_max_land: f64, k_factor: f64) -> f64 {
    lda_m * sigma * cl_max_land / k_factor
}

/// Pre-computed constraint curves ready for plotting -- `MatchingChartData`.
///
/// `tw_takeoff` and `ws_land_limits` are keyed by aerodrome name in the order
/// the aerodromes were supplied (upstream's insertion-ordered `dict`); a
/// `Vec` of pairs keeps that order without an ordered-map dependency.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchingChartData {
    /// Wing-loading axis, Pa.
    pub ws_pa: Vec<f64>,
    /// Cruise `T/W0` curve.
    pub tw_cruise: Vec<f64>,
    /// Engine-out climb `T/W0` (constant).
    pub tw_oei_climb: f64,
    /// Take-off `T/W0` curve per aerodrome, in supplied order.
    pub tw_takeoff: Vec<(String, Vec<f64>)>,
    /// Maximum `W/S` [Pa] per aerodrome, in supplied order.
    pub ws_land_limits: Vec<(String, f64)>,
    /// Design wing loading [Pa], if the aircraft weight and area are known.
    pub design_ws_pa: Option<f64>,
    /// Design `T/W0`, if supplied.
    pub design_tw: Option<f64>,
}

/// Assemble every matching-chart constraint curve for a set of aerodromes --
/// `build_matching_chart`.
///
/// Each `Option` argument falls back to a fresh [`PerformanceConfig`]'s field
/// of the same name, exactly as upstream's `None`-defaulted keywords do -- one
/// place the defaults live, so an omitted argument cannot drift from the
/// configuration. `tw_design` alone has no configuration counterpart and is
/// passed straight through. `n_ws_points` sets the wing-loading resolution;
/// `ws_min_pa`/`ws_max_pa` set its range.
#[allow(clippy::too_many_arguments)] // mirrors upstream's own keyword signature
pub fn build_matching_chart(
    cd0: f64,
    k: f64,
    cruise_mach: f64,
    cruise_altitude_m: f64,
    mtow_kg: f64,
    wing_area_m2: f64,
    n_engines: i64,
    airports: &[Airport],
    cl_max_to: Option<f64>,
    cl_max_land: Option<f64>,
    thrust_lapse: Option<f64>,
    oei_gradient: Option<f64>,
    k_land: Option<f64>,
    oei_climb_cl: Option<f64>,
    oei_climb_delta_cd: Option<f64>,
    tw_design: Option<f64>,
    n_ws_points: i64,
    ws_min_pa: Option<f64>,
    ws_max_pa: Option<f64>,
) -> MatchingChartData {
    let defaults = PerformanceConfig::default();
    let cl_max_to = cl_max_to.unwrap_or(defaults.cl_max_to);
    let cl_max_land = cl_max_land.unwrap_or(defaults.cl_max_land);
    let thrust_lapse = thrust_lapse.unwrap_or(defaults.thrust_lapse);
    let oei_gradient = oei_gradient.unwrap_or(defaults.oei_gradient);
    let k_land = k_land.unwrap_or(defaults.k_land);
    let oei_climb_cl = oei_climb_cl.unwrap_or(defaults.oei_climb_cl);
    let oei_climb_delta_cd = oei_climb_delta_cd.unwrap_or(defaults.oei_climb_delta_cd);
    let ws_min_pa = ws_min_pa.unwrap_or(defaults.ws_min_pa);
    let ws_max_pa = ws_max_pa.unwrap_or(defaults.ws_max_pa);

    let ws_pa = linspace(ws_min_pa, ws_max_pa, n_ws_points);

    let tw_cruise =
        tw_cruise_constraint(&ws_pa, cd0, k, cruise_mach, cruise_altitude_m, thrust_lapse);
    let tw_oei_climb = tw_oei_climb_constraint(
        cd0,
        k,
        n_engines,
        oei_gradient,
        oei_climb_cl,
        oei_climb_delta_cd,
    );

    let mut tw_takeoff: Vec<(String, Vec<f64>)> = Vec::with_capacity(airports.len());
    let mut ws_land_limits: Vec<(String, f64)> = Vec::with_capacity(airports.len());
    for airport in airports {
        let sigma = super::density_ratio(airport.elevation_m, airport.isa_deviation_c);
        tw_takeoff.push((
            airport.name.clone(),
            tw_takeoff_constraint(&ws_pa, airport.toda_m, sigma, cl_max_to),
        ));
        ws_land_limits.push((
            airport.name.clone(),
            ws_landing_limit(airport.lda_m, sigma, cl_max_land, k_land),
        ));
    }

    // Upstream's `mtow_kg and wing_area_m2` is a truthiness test: a zero on
    // either side leaves the design point undefined rather than dividing.
    let design_ws_pa = if mtow_kg != 0.0 && wing_area_m2 != 0.0 {
        Some(mtow_kg * super::G / wing_area_m2)
    } else {
        None
    };

    MatchingChartData {
        ws_pa,
        tw_cruise,
        tw_oei_climb,
        tw_takeoff,
        ws_land_limits,
        design_ws_pa,
        design_tw: tw_design,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_engine_aircraft_has_no_oei_climb_constraint() {
        // FAR 25.121 does not apply below two engines, so the curve collapses
        // to zero rather than dividing by `N - 1 = 0`.
        assert_eq!(
            tw_oei_climb_constraint(0.02, 0.045, 1, 0.024, 1.2, 0.025),
            0.0
        );
    }

    #[test]
    fn the_takeoff_constraint_rises_with_wing_loading() {
        // A more heavily loaded wing needs more thrust off the same runway.
        let tw = tw_takeoff_constraint(&[3000.0, 6000.0], 3500.0, 1.0, 1.8);
        assert!(tw[1] > tw[0]);
    }

    #[test]
    fn omitted_keywords_fall_back_to_the_performance_config_defaults() {
        // Passing None for every config-backed keyword must reproduce passing
        // PerformanceConfig::default()'s own fields explicitly.
        let airports: [Airport; 0] = [];
        let d = PerformanceConfig::default();
        let with_none = build_matching_chart(
            0.02, 0.045, 0.78, 10668.0, 79_000.0, 122.0, 2, &airports, None, None, None, None,
            None, None, None, None, 5, None, None,
        );
        let with_explicit = build_matching_chart(
            0.02,
            0.045,
            0.78,
            10668.0,
            79_000.0,
            122.0,
            2,
            &airports,
            Some(d.cl_max_to),
            Some(d.cl_max_land),
            Some(d.thrust_lapse),
            Some(d.oei_gradient),
            Some(d.k_land),
            Some(d.oei_climb_cl),
            Some(d.oei_climb_delta_cd),
            None,
            5,
            Some(d.ws_min_pa),
            Some(d.ws_max_pa),
        );
        assert_eq!(with_none, with_explicit);
    }
}
