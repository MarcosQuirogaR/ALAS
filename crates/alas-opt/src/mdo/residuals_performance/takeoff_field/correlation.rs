// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Raymer required-versus-installed `T/W` correlation of the
//! `takeoff_field` residual for turboprops and the legacy replay
//! (`legacy_field_correlations`), which has no rotation.

use alas_config::airports::Airport;
use alas_perf::performance::{density_ratio, tw_takeoff_constraint};

use super::ResidualInputs;
use crate::mdo::types::{ConstraintFamily, ConstraintResidual};
use crate::mdo::ResidualRole;

/// Take-off distance of the Raymer correlation at the installed thrust-to-weight,
/// m: the runway length at which the required static `T/W` equals it (the
/// required value falls with the runway length), by bisection. `None` when
/// no length up to 30 km balances it.
pub(super) fn correlation_distance_m(inputs: &ResidualInputs<'_>) -> Option<f64> {
    let weight_n = inputs.mass_kg * inputs.gravity_m_s2;
    let available = inputs.static_thrust_n / weight_n;
    let sigma = density_ratio(
        inputs.departure.elevation_m,
        inputs.departure.isa_deviation_c,
    );
    let cl_max = inputs.perf.takeoff_field_cl_max(inputs.perf.cl_max_to);
    let required = |toda_m: f64| {
        tw_takeoff_constraint(&[weight_n / inputs.s_ref_m2], toda_m, sigma, cl_max)[0]
    };
    let (mut short, mut long) = (50.0, 30_000.0);
    if !(required(long).is_finite() && required(long) <= available) {
        return None;
    }
    if required(short) <= available {
        return Some(short);
    }
    for _ in 0..60 {
        let middle = 0.5 * (short + long);
        if required(middle) > available {
            short = middle;
        } else {
            long = middle;
        }
    }
    Some(long)
}

pub(super) fn correlation_row(inputs: &ResidualInputs<'_>) -> ConstraintResidual {
    // An improved-climb V2 lengthens the field (same correlation, lower
    // effective CLmax).
    correlation_residual(
        inputs.mass_kg,
        inputs.static_thrust_n,
        inputs.s_ref_m2,
        inputs.gravity_m_s2,
        inputs.departure,
        inputs.perf.takeoff_field_cl_max(inputs.perf.cl_max_to),
        inputs.role,
    )
}

/// Required-versus-installed static `T/W` of the Raymer correlation at
/// `mass_kg`: both sides move with the mass, so a field a light dispatch
/// clears can be missed at the maximum take-off mass.
pub(in crate::mdo::residuals_performance) fn correlation_residual(
    mass_kg: f64,
    static_thrust_n: f64,
    s_ref_m2: f64,
    gravity_m_s2: f64,
    departure: &Airport,
    cl_max_to: f64,
    role: ResidualRole,
) -> ConstraintResidual {
    let weight_n = mass_kg * gravity_m_s2;
    let available_tw = static_thrust_n / weight_n;
    let sigma = density_ratio(departure.elevation_m, departure.isa_deviation_c);
    let required_tw =
        tw_takeoff_constraint(&[weight_n / s_ref_m2], departure.toda_m, sigma, cl_max_to)[0];
    ConstraintResidual::scaled(
        "takeoff_field",
        ConstraintFamily::Performance,
        available_tw,
        required_tw,
        "T/W",
        required_tw - available_tw,
        role,
    )
}
