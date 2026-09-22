// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission reference.Methods.Weights.Correlations.Common.payload.payload.
// Upstream: mission reference 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! Passenger, baggage and cargo payload masses.

use alas_units::POUND_MASS;

use super::PayloadBreakdown;

/// The FAA per-passenger body weight allowance, 195 lb: upstream's
/// `wt_passenger` default, never overridden by [`super::empty_weight`].
const PASSENGER_MASS_LB: f64 = 195.0;
/// The per-passenger baggage allowance, 30 lb: upstream's `wt_baggage`
/// default, never overridden.
const BAGGAGE_MASS_LB: f64 = 30.0;

/// The payload mass: `payload` (FAA passenger-weight guidelines).
pub(crate) fn payload(passenger_count: f64, cargo_kg: f64) -> PayloadBreakdown {
    let passengers_kg = PASSENGER_MASS_LB * POUND_MASS * passenger_count;
    let baggage_kg = BAGGAGE_MASS_LB * POUND_MASS * passenger_count;
    PayloadBreakdown {
        passengers_kg,
        baggage_kg,
        cargo_kg,
        total_kg: passengers_kg + baggage_kg + cargo_kg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_totals_passengers_baggage_and_cargo() {
        let p = payload(350.0, 1234.0);
        assert!((p.total_kg - (p.passengers_kg + p.baggage_kg + p.cargo_kg)).abs() < 1e-9);
    }

    #[test]
    fn payload_scales_linearly_with_passenger_count() {
        let one = payload(1.0, 0.0);
        let ten = payload(10.0, 0.0);
        assert!((ten.passengers_kg - 10.0 * one.passengers_kg).abs() < 1e-9);
        assert!((ten.baggage_kg - 10.0 * one.baggage_kg).abs() < 1e-9);
    }
}
