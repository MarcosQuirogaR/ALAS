// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Loading-state tables: (CG, mass) pairs for the named load cases.

use super::ModelCgLoadingState;

pub(super) fn loading_states(
    oew_mass: f64,
    oew_cg_x: f64,
    payload_mass: f64,
    payload_cg_x: f64,
    mtow_cg_x: f64,
    fuel_mass: f64,
) -> [(f64, f64); 3] {
    let mzfw_mass = oew_mass + payload_mass;
    let mzfw_cg_x = (oew_mass * oew_cg_x + payload_mass * payload_cg_x) / mzfw_mass.max(1.0);
    let mtow_mass = oew_mass + payload_mass + fuel_mass.max(0.0);

    [
        (oew_cg_x, oew_mass),
        (mzfw_cg_x, mzfw_mass),
        (mtow_cg_x, mtow_mass),
    ]
}

/// Product load cases extending the reference three-point envelope with
/// explicit mid-mission and reserve fuel states, additionally carrying each
/// state's vertical CG (needed for the tip-back boundary's `h_cg`): the same mass-weighted mixing, on the `z` axis, using the
/// same OEW/payload/fuel masses and fuel-fraction schedule so the two axes
/// describe one CG, not two independently interpolated ones.
#[allow(clippy::too_many_arguments)]
pub(super) fn operational_loading_states_with_z(
    oew_mass: f64,
    oew_cg_x: f64,
    oew_cg_z: f64,
    payload_mass: f64,
    payload_cg_x: f64,
    payload_cg_z: f64,
    fuel_mass: f64,
    fuel_cg_x: f64,
    fuel_cg_z: f64,
    mtow_cg_x: f64,
) -> Vec<(ModelCgLoadingState, f64, f64, f64)> {
    let mzfw_mass = oew_mass + payload_mass;
    let mzfw_cg_x = (oew_mass * oew_cg_x + payload_mass * payload_cg_x) / mzfw_mass.max(1.0);
    let mzfw_cg_z = (oew_mass * oew_cg_z + payload_mass * payload_cg_z) / mzfw_mass.max(1.0);
    let with_fuel = |fraction: f64, state: ModelCgLoadingState| {
        let fuel = fuel_mass.max(0.0) * fraction;
        let mass = mzfw_mass + fuel;
        let (cg_x, cg_z) = if fuel > 0.0 {
            (
                (mzfw_mass * mzfw_cg_x + fuel * fuel_cg_x) / mass.max(1.0),
                (mzfw_mass * mzfw_cg_z + fuel * fuel_cg_z) / mass.max(1.0),
            )
        } else {
            (mzfw_cg_x, mzfw_cg_z)
        };
        (state, cg_x, cg_z, mass)
    };
    let mtow_fuel = fuel_mass.max(0.0);
    let mtow_mass = mzfw_mass + mtow_fuel;
    let mtow_cg_z = if mtow_fuel > 0.0 {
        (mzfw_mass * mzfw_cg_z + mtow_fuel * fuel_cg_z) / mtow_mass.max(1.0)
    } else {
        mzfw_cg_z
    };
    vec![
        (
            ModelCgLoadingState::OperatingEmpty,
            oew_cg_x,
            oew_cg_z,
            oew_mass,
        ),
        (
            ModelCgLoadingState::AnalyzedZeroFuel,
            mzfw_cg_x,
            mzfw_cg_z,
            mzfw_mass,
        ),
        with_fuel(0.50, ModelCgLoadingState::OperationalMidMission),
        with_fuel(0.10, ModelCgLoadingState::OperationalReserve),
        (
            ModelCgLoadingState::AnalyzedTakeoff,
            mtow_cg_x,
            mtow_cg_z,
            mtow_mass,
        ),
    ]
}
