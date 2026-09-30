// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Systems-and-furnishings group of a regional turboprop transport.
//!
//! Selected by [`alas_config::CabinEquipmentMethod::RegionalTurbopropV1`]:
//! a shaft-power installation below 40 t maximum takeoff mass. The LTH
//! relations are fitted on four turbofan aircraft of 52-233 t and the FLOPS
//! transport fits on 1940s-1970s jet and military aircraft; neither population
//! contains a light turboprop cabin.
//!
//! # Relation
//!
//! Torenbeek's systems group for a twin-engine propeller aircraft,
//!
//! `W_sys = k_equip MTOW + 0.768 k_fc MTOW^(2/3)`, kilograms,
//!
//! with `k_equip = 0.11` (twin-engine propeller transport) and
//! `k_fc = 0.88`, the flight-control factor for a transport of the ATR class,
//! which accounts for how the surface controls are actuated, plus an allowance
//! of 15 kg per passenger seat for seats and other mass not itemized in the
//! breakdown.
//!
//! Source: D. Scholz, HAW Hamburg aircraft design lecture notes, Sect. 8.2,
//! eq. 8.2.18 and 8.2.20, which is a **secondary** source reproducing E.
//! Torenbeek, *Synthesis of Subsonic Airplane Design*, Delft University Press,
//! 1982. **The primary source was not verified**, and the constants are used
//! as the secondary source states them. The lecture notes evaluate the
//! relation at MTOW 23,296 kg to 3,113.8 kg, which the unit test reproduces.
//! This is a class-level relation; nothing in it is tuned to an aircraft.
//!
//! # Boundary and double counting
//!
//! `W_sys` is a whole systems-and-furnishings group. It therefore replaces the
//! **entire** FLOPS systems group, furnishings included, and the FLOPS
//! operating items are kept unchanged. The ledger still needs one row per
//! FLOPS system, so the eight non-furnishings FLOPS terms (surface controls,
//! APU, instruments, hydraulics, electrical, avionics, air conditioning, anti
//! icing) are reported as computed and the furnishings row is the remainder
//!
//! `furnishings = W_sys + 15 n_seats - sum(eight terms)`.
//!
//! The group total is then exactly `W_sys + 15 n_seats` and is independent of
//! how the eight terms move: **this row absorbs every change in the other
//! eight**, so improving one of them does not change the group mass. If the
//! eight terms ever exceed the Torenbeek group the remainder is negative and
//! evaluation is refused rather than clamped.

use crate::flops_transport::FlopsTransportInputError;

/// Torenbeek 1982 `k_equip` for a twin-engine propeller transport, as quoted
/// in the secondary source.
const K_EQUIP: f64 = 0.11;
/// Coefficient of the `MTOW^(2/3)` term of the same equation.
const K_MTOW_TWO_THIRDS: f64 = 0.768;
/// Torenbeek flight-control factor for a transport of the ATR class, as
/// quoted in the secondary source (surface-control actuation).
const K_FLIGHT_CONTROL: f64 = 0.88;
/// Seat and unitemized mass per passenger seat, kg, as quoted in the
/// secondary source ("an average value for each seat weight is 15 kg").
const SEAT_ALLOWANCE_KG: f64 = 15.0;

/// Torenbeek systems-and-furnishings group `W_sys`, kg, for a maximum takeoff
/// mass in kilograms.
pub(super) fn systems_group_kg(mtow_kg: f64) -> f64 {
    K_EQUIP * mtow_kg + K_MTOW_TWO_THIRDS * K_FLIGHT_CONTROL * mtow_kg.powf(2.0 / 3.0)
}

/// Furnishings ledger row, kg: `W_sys + 15 n_seats` less the eight
/// non-furnishings FLOPS systems terms.
///
/// # Errors
///
/// Returns [`FlopsTransportInputError`] when the remainder is negative, that
/// is when the eight FLOPS terms alone exceed the Torenbeek group.
pub(super) fn furnishings_remainder_kg(
    mtow_kg: f64,
    seats: usize,
    non_furnishings_systems_kg: f64,
) -> Result<f64, FlopsTransportInputError> {
    let group = systems_group_kg(mtow_kg) + SEAT_ALLOWANCE_KG * seats as f64;
    let remainder = group - non_furnishings_systems_kg;
    if remainder.is_finite() && remainder >= 0.0 {
        Ok(remainder)
    } else {
        Err(FlopsTransportInputError {
            field: "regional_cabin_furnishings_remainder",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reproduces_the_published_worked_example() {
        let mass = systems_group_kg(23_296.0);
        assert!((mass - 3_113.842).abs() < 0.05, "got {mass}");
    }

    #[test]
    fn seat_allowance_matches_the_published_70_seat_value() {
        let residual = furnishings_remainder_kg(23_296.0, 70, 0.0).unwrap_or(f64::NAN);
        assert!((residual - (3_113.842 + 1_050.0)).abs() < 0.05);
    }

    #[test]
    fn remainder_absorbs_the_other_terms() {
        let a = furnishings_remainder_kg(23_000.0, 72, 1_000.0).unwrap_or(f64::NAN);
        let b = furnishings_remainder_kg(23_000.0, 72, 1_250.0).unwrap_or(f64::NAN);
        assert!((a - b - 250.0).abs() < 1e-9);
    }

    #[test]
    fn negative_remainder_is_an_error_not_a_clamp() {
        let error = furnishings_remainder_kg(23_000.0, 72, 50_000.0);
        assert_eq!(
            error,
            Err(FlopsTransportInputError {
                field: "regional_cabin_furnishings_remainder"
            })
        );
        assert!(furnishings_remainder_kg(23_000.0, 72, f64::NAN).is_err());
    }
}
