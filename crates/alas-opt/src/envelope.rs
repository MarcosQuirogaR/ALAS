// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preliminary model-derived center-of-gravity and landing-gear constraints.
//!
//! Product assessment keeps the hard longitudinal-stability floor separate
//! from the optimizer's preferred static margin. It also exposes each gear
//! reaction constraint independently, because a single envelope Boolean
//! cannot identify whether stability, tire capacity, or steering authority
//! governs a loading state. The ground reactions follow two-point static
//! equilibrium as presented by Currey, *Aircraft Landing Gear Design:
//! Principles and Practices*, AIAA, 1988.

mod constraints;
mod ledger_basis;
mod ledger_states;
mod loading;
mod physical_limits;
mod rotation_thrust;
mod support;
#[cfg(test)]
mod tests;
mod types;

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{MassBreakdown, MassCoordinates};
use alas_mass::stations::StationError;
use alas_payload::oew::oew_and_cg;
use alas_perf::landing_gear::{
    size_landing_gear_at_design_state, size_landing_gear_with_group_stations,
};
use constraints::{assess_loading_constraints, LoadingConstraintInputs};
pub use constraints::{ModelCgConstraint, ModelCgConstraintAssessment};
pub use ledger_states::{
    assess_model_cg_envelope_with_ledger, assess_model_cg_envelope_with_ledger_and_landing,
    LedgerLandingState, LedgerLoadingBasis,
};
use loading::{loading_states, operational_loading_states_with_z};
pub use physical_limits::{
    physical_cg_limits, AftLimitGovernance, ForwardLimitGovernance, PhysicalCgLimits,
    PhysicalCgLimitsInput,
};
pub use types::{
    AftCgLimitGovernance, CgEnvelopeResult, ModelCgEnvelopeAssessment, ModelCgEnvelopeError,
    ModelCgLoadingAssessment, ModelCgLoadingState, PhaseLimits, StaticMarginPreferenceAssessment,
};

/// The typed refusal when this aircraft has no main-gear longitudinal station
/// the mass model can supply, `None` when it has one.
///
/// Both envelope paths rebuild the mass model's own gear fallbacks
/// (`nlg_x_fraction` of fuselage length, `mlg_x_fraction_mac` aft of the MAC
/// leading edge) so they can feed `resolved_station_positions` the same
/// numbers `alas_mass::stations` would. The main-gear half of that pair is a
/// wing-mounted gear rule with a stated domain, and the one place that domain
/// is stated is `alas_mass::stations::main_gear_station`. This asks that owner
/// whether a station exists rather than restating the test here, so the two
/// crates cannot drift apart into accepting different aircraft.
///
/// A source-scaled resolution is a published station scaled onto the active
/// fuselage and is admissible on any layout, so the question is only asked
/// when the fallback is what would otherwise stand in, which is also what
/// keeps this off the search's hot path for every registered aircraft that
/// carries an anchor.
///
/// Only [`StationError::MainGearStationNotMeasured`] is reported. The other
/// station failures are degenerate-geometry conditions that both callers
/// already detect through their own missing-surface and finiteness checks.
fn unmeasured_main_gear_station(
    plane: &Airplane,
    config: &AlasConfig,
    gear_stations: &alas_config::LandingGearStationPositions,
) -> Option<StationError> {
    if gear_stations.source_scaled {
        return None;
    }
    match alas_mass::stations::component_stations_with_gear(
        plane,
        &config.geometry,
        &config.requirements,
        &config.mass_model,
        &config.structures,
        &config.landing_gear,
    ) {
        Err(error @ StationError::MainGearStationNotMeasured { .. }) => Some(error),
        _ => None,
    }
}

use support::{
    fuselage_lower_points_aft_of, ground_z_m, oew_cg_z, tail_volume_coefficient, SCISSOR_CL_H_MAX,
    SCISSOR_CM_AC_WB_LANDING, SCISSOR_ETA, SCISSOR_X_AC_WB_FRAC,
};

/// Assess hard model constraints across OEW, analyzed ZFW/TOW, and explicit
/// mid-mission/reserve fuel cases.
///
/// The aerodynamic aft boundary uses the critical (most-forward) neutral
/// point, `critical_x_np`, not the clean single-condition `x_np`.
/// `target_static_margin` is a reported preference only.
///
/// Each state is gated against the mechanisms of its own phase
/// ([`PhaseLimits::for_state`]): bare OEW
/// ([`ModelCgLoadingState::OperatingEmpty`]) keeps only its ground/gear
/// constraints, zero-fuel and the fuel cases get landing trim and the
/// static-margin floor, and takeoff gets rotation and the static-margin floor.
#[allow(clippy::too_many_arguments)] // mirrors the reference-compatible seam beside it
pub fn assess_model_cg_envelope(
    plane: &Airplane,
    masses: &MassBreakdown,
    coords: &MassCoordinates,
    cg_x: f64,
    x_np: f64,
    critical_x_np: f64,
    mac: f64,
    config: &AlasConfig,
) -> Result<ModelCgEnvelopeAssessment, ModelCgEnvelopeError> {
    let (oew_mass, oew_cg_x) = oew_and_cg(masses, coords);
    let oew_z = oew_cg_z(masses, coords);
    let states = operational_loading_states_with_z(
        oew_mass,
        oew_cg_x,
        oew_z,
        masses.payload,
        coords.payload[0],
        coords.payload[2],
        masses.fuel,
        coords.fuel[0],
        coords.fuel[2],
        cg_x,
    );
    ledger_basis::assess_model_cg_envelope_from_states(
        plane,
        states,
        cg_x,
        x_np,
        critical_x_np,
        mac,
        config,
    )
}

/// Reproduce the frozen reference CG-envelope Boolean and exceedance.
///
/// This compatibility path retains the upstream use of the preferred static
/// margin as a boundary so the exact fixture remains meaningful. Product
/// analyses call [`assess_model_cg_envelope`] instead.
pub fn check_cg_envelope(
    plane: &Airplane,
    masses: &MassBreakdown,
    coords: &MassCoordinates,
    cg_x: f64,
    x_np: f64,
    mac: f64,
    config: &AlasConfig,
) -> CgEnvelopeResult {
    let req = &config.requirements;
    let mm = &config.mass_model;

    let (oew_mass, oew_cg_x) = oew_and_cg(masses, coords);

    // The second mass-analysis pass may replace the requested/lumped payload
    // with the detailed cabin or cargo layout. The envelope is checked against
    // that same physical loading state; using the request here
    // gives MZFW and MTOW a different payload from the CG being checked.
    let payload_mass = masses.payload;
    let fuel_mass = masses.fuel;
    let loading_states = loading_states(
        oew_mass,
        oew_cg_x,
        payload_mass,
        coords.payload[0],
        cg_x,
        fuel_mass,
    );
    let mtow_mass = loading_states[2].1;

    let x_wing_ac_val = if !plane.wings.is_empty() {
        plane.wings[0].aerodynamic_center(0.25)[0]
    } else {
        0.0
    };
    let x_mac_le = x_wing_ac_val - 0.25 * mac;

    let to_pct = |x_val: f64| -> f64 { ((x_val - x_mac_le) / mac.max(0.001)) * 100.0 };

    let np_pct = to_pct(x_np);
    let aero_aft_lim = np_pct - req.target_static_margin * 100.0;
    let aero_fwd_lim = aero_aft_lim - req.cg_range_pct_mac;

    let nlg_x_frac = mm.nlg_x_fraction;
    let mlg_x_frac_mac = mm.mlg_x_fraction_mac;
    let pct_nlg_min = mm.pct_load_nlg_min;

    let fus = &plane.fuselages[0];
    let fus_start_x = fus.xsecs.first().map(|x| x.xyz_c[0]).unwrap_or(0.0);
    let fus_end_x = fus.xsecs.last().map(|x| x.xyz_c[0]).unwrap_or(0.0);
    let fus_len = fus_end_x - fus_start_x;

    let fallback_x_nlg = fus_start_x + fus_len * nlg_x_frac;
    let fallback_x_mlg = x_mac_le + mlg_x_frac_mac * mac;
    let gear_stations = config.landing_gear.resolved_station_positions(
        fallback_x_nlg,
        fallback_x_mlg,
        fus_start_x,
        fus_len,
    );
    // The same refusal as [`assess_model_cg_envelope`]. This path reports a
    // Boolean and an exceedance rather than a typed error, so it fails closed:
    // an aircraft with no measured main-gear station has no compliant state
    // to report. No exceedance magnitude is claimed, because none was
    // measured (a missing datum is not a distance past a limit) and the
    // violation Boolean is what marks the candidate rejected. The frozen
    // reference fixture is a low-wing aircraft whose fallback stands, so its
    // replayed values are unchanged.
    if unmeasured_main_gear_station(plane, config, &gear_stations).is_some() {
        return CgEnvelopeResult {
            violation: true,
            worst_exceedance: 0.0,
        };
    }
    let x_nlg = gear_stations.x_nlg_m;
    let x_mlg = gear_stations.x_mlg_m;
    let wheelbase = x_mlg - x_nlg;

    let aero_fwd_lim_x = x_mac_le + aero_fwd_lim / 100.0 * mac;
    let aero_aft_lim_x = x_mac_le + aero_aft_lim / 100.0 * mac;
    let fus_diam = config.geometry.fuselage.diameter_m;

    let gear_layout = size_landing_gear_with_group_stations(
        mtow_mass,
        x_nlg,
        x_mlg,
        aero_fwd_lim_x,
        aero_aft_lim_x,
        fus_diam,
        fus_diam * 1.1,
        &gear_stations.main_gear_x_m,
        &config.landing_gear,
    );
    let pct_nlg_max = gear_layout.pct_load_nlg_max;
    let pct_mlg_max = gear_layout.pct_load_mlg_max;

    let load_nlg_max = mtow_mass * pct_nlg_max;
    let load_mlg_max = mtow_mass * pct_mlg_max;
    let load_nlg_min = mtow_mass * pct_nlg_min;

    let mut worst_exc: f64 = 0.0;
    let mut violation = false;

    for (cg_val, w_state) in loading_states {
        let w_safe = w_state.max(1.0);
        let cg_pct = to_pct(cg_val);

        let nlg_strength_limit = to_pct(x_mlg - (load_nlg_max * wheelbase / w_safe));
        let mlg_strength_limit = to_pct(x_nlg + (load_mlg_max * wheelbase / w_safe));
        let min_nose_load_limit = to_pct(x_mlg - (load_nlg_min * wheelbase / w_safe));

        let fwd_lim_dynamic = aero_fwd_lim.max(nlg_strength_limit);
        let aft_lim_dynamic = aero_aft_lim
            .min(mlg_strength_limit)
            .min(min_nose_load_limit);

        let mut exc = 0.0;
        if cg_pct < fwd_lim_dynamic - 0.01 {
            exc = (fwd_lim_dynamic - cg_pct) / 100.0;
        } else if cg_pct > aft_lim_dynamic + 0.01 {
            exc = (cg_pct - aft_lim_dynamic) / 100.0;
        }

        if exc > 0.0 {
            violation = true;
            worst_exc = worst_exc.max(exc);
        }
    }

    CgEnvelopeResult {
        violation,
        worst_exceedance: worst_exc,
    }
}

// Tests assert on inputs they constructed here, so a failed expect or panic is
// the assertion failing rather than a library invariant breaking.
