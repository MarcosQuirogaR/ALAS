// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The seat part of the cabin mass, by class, so the statement
//! can place it on the seat rows of the cabin layout.
//!
//! Each method's own per-seat charge: FLOPS equation 110 (NASA/TM-2017-219627
//! Vol. I) 112, 78 and 44 lb per first, business and tourist seat and 127 lb per
//! flight-crew seat; the regional relation 15 kg per passenger seat. The LTH
//! operating items list the passenger seats ("Passagiersitze", Pape 2018 pp.
//! 22-23, eqs. 2.15-2.16) without a mass, so the seats are Torenbeek's typical
//! seat weights (1982, Table 3-2, printed p. 76, per seat of a unit): first a de
//! luxe single, business a de luxe double, tourist an economy triple, in the
//! medium/long-haul column for LTH long haul and the short-haul column
//! otherwise; capped at what the LTH total leaves after crew, oil and unusable
//! fuel. The class-to-seat-type mapping is this module's reading. The
//! executive-jet method seats every passenger in Table 3-2's executive single
//! (normal, 18.1 kg); its flight-crew seats are inside Table 8-12's flight-deck
//! accommodations, so none is placed separately.

use super::units::kg;

/// Torenbeek Table 3-2 seat mass, kg per seat, `[first, business, tourist]`:
/// medium/long haul 21.3, 31.8 / 2, 29.9 / 3; short haul 18.1, 27.2 / 2, 27.2 / 3.
const TABLE_3_2_LONG_KG: [f64; 3] = [21.3, 31.8 / 2.0, 29.9 / 3.0];
const TABLE_3_2_SHORT_KG: [f64; 3] = [18.1, 27.2 / 2.0, 27.2 / 3.0];

/// FLOPS equation 110 furnishing per flight-crew seat, lb.
pub(super) const FLIGHT_DECK_SEAT_LB: f64 = 127.0;

/// A FLOPS equation 110 passenger class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatClass {
    /// First class.
    First,
    /// Business class.
    Business,
    /// Tourist class; premium economy counts here, as the cabin's FLOPS
    /// split does (`alas_config::ResolvedPassengerCounts`).
    Tourist,
}

impl SeatClass {
    /// Every class, in the order of [`SeatMasses::passenger_kg`].
    pub const ALL: [Self; 3] = [Self::First, Self::Business, Self::Tourist];

    /// The class of a cabin-layout class name.
    pub fn from_cabin_class(name: &str) -> Self {
        match name {
            "First" => Self::First,
            "Business" => Self::Business,
            _ => Self::Tourist,
        }
    }

    /// Position of this class in [`Self::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// FLOPS equation 110 furnishing per seat, lb.
    fn furnishing_lb(self) -> f64 {
        match self {
            Self::First => 112.0,
            Self::Business => 78.0,
            Self::Tourist => 44.0,
        }
    }
}

/// FLOPS equation 110 passenger-seat terms, lb, for `[first, business, tourist]`.
pub(super) fn passenger_seats_lb(counts: [usize; 3]) -> f64 {
    SeatClass::ALL
        .iter()
        .map(|class| class.furnishing_lb() * counts[class.index()] as f64)
        .sum()
}

/// The seat part of the cabin mass, kg.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Default)]
pub struct SeatMasses {
    /// Passenger seats by class, in the order of [`SeatClass::ALL`], kg.
    pub passenger_kg: [f64; 3],
    /// Flight-crew seats, kg (FLOPS only; inside the other relations' totals).
    pub flight_deck_kg: f64,
    /// Whether the passenger seats are inside the operating items (LTH)
    /// rather than inside furnishings.
    pub in_operating_items: bool,
}

/// The seat masses of one cabin method. `counts` are the FLOPS class counts
/// `[first, business, tourist]`; `lth_remainder_kg` is what the LTH operating
/// items leave after crew, oil and unusable fuel (`None` off LTH).
pub(super) fn seat_masses(
    method: alas_config::CabinEquipmentMethod,
    counts: [usize; 3],
    flight_crew: usize,
    lth_remainder_kg: Option<f64>,
    long_haul: bool,
) -> SeatMasses {
    let flops =
        SeatClass::ALL.map(|class| kg(class.furnishing_lb() * counts[class.index()] as f64));
    match method {
        alas_config::CabinEquipmentMethod::FlopsTransportV1 => SeatMasses {
            passenger_kg: flops,
            flight_deck_kg: kg(FLIGHT_DECK_SEAT_LB * flight_crew as f64),
            in_operating_items: false,
        },
        alas_config::CabinEquipmentMethod::ExecutiveJetV1 => SeatMasses {
            passenger_kg: counts
                .map(|count| super::executive_cabin::EXECUTIVE_SEAT_KG * count as f64),
            ..SeatMasses::default()
        },
        alas_config::CabinEquipmentMethod::RegionalTurbopropV1 => SeatMasses {
            passenger_kg: counts
                .map(|count| super::equations::regional_cabin::SEAT_ALLOWANCE_KG * count as f64),
            ..SeatMasses::default()
        },
        alas_config::CabinEquipmentMethod::LthCivilTransportV1 => {
            let per_seat = if long_haul {
                TABLE_3_2_LONG_KG
            } else {
                TABLE_3_2_SHORT_KG
            };
            let seats =
                SeatClass::ALL.map(|class| per_seat[class.index()] * counts[class.index()] as f64);
            let total: f64 = seats.iter().sum();
            let room = lth_remainder_kg.unwrap_or(0.0).max(0.0);
            let scale = if total > room { room / total } else { 1.0 };
            SeatMasses {
                passenger_kg: seats.map(|mass| scale * mass),
                flight_deck_kg: 0.0,
                in_operating_items: true,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::CabinEquipmentMethod as Method;

    #[test]
    fn flops_seats_are_the_equation_110_seat_terms() {
        let seats = seat_masses(Method::FlopsTransportV1, [10, 20, 100], 2, None, true);
        assert!((seats.passenger_kg[0] - kg(1120.0)).abs() < 1e-9);
        assert!((seats.passenger_kg[2] - kg(4400.0)).abs() < 1e-9);
        assert!((seats.flight_deck_kg - kg(254.0)).abs() < 1e-9);
        assert!(!seats.in_operating_items);
    }

    #[test]
    fn lth_seats_are_table_3_2_and_never_exceed_the_operating_item_remainder() {
        let long = seat_masses(Method::LthCivilTransportV1, [1, 2, 3], 2, Some(1.0e6), true);
        assert!(
            (long.passenger_kg[1] - 31.8).abs() < 1e-9
                && (long.passenger_kg[2] - 29.9).abs() < 1e-9
        );
        let short = seat_masses(
            Method::LthCivilTransportV1,
            [1, 0, 0],
            2,
            Some(1.0e6),
            false,
        );
        assert!((short.passenger_kg[0] - 18.1).abs() < 1e-9);
        let seats = seat_masses(
            Method::LthCivilTransportV1,
            [0, 0, 300],
            2,
            Some(900.0),
            true,
        );
        assert!((seats.passenger_kg.iter().sum::<f64>() - 900.0).abs() < 1e-9);
        assert!(seats.in_operating_items && seats.flight_deck_kg == 0.0);
    }

    #[test]
    fn premium_economy_is_a_tourist_seat() {
        assert_eq!(SeatClass::from_cabin_class("Premium"), SeatClass::Tourist);
        assert_eq!(SeatClass::from_cabin_class("Business").index(), 1);
    }
}
