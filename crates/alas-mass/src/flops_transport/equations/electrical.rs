// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Electrical system group: generation, conversion, distribution, lights and
//! signal devices.
//!
//! # Why equation 106 is not used on a built airliner cabin
//!
//! FLOPS equation 106 (NASA/TM-2017-219627 Vol. I, s. 5.4.5, p. 38) sizes the
//! group on fuselage length and width, engine count and occupants. Against the
//! itemised weights of the commercial transports in Beltramo, Trapp, Kimoto
//! and Marsh, *Parametric Study of Transport Aircraft Systems Cost and
//! Weight*, NASA CR-151970 (1977), Table 5.14, pp. 5-71 and 5-72, it reproduces
//! the DC-9, BAC-111, 727-200 and 747 within 7 % but under-predicts the three
//! trijet wide-bodies by 30 % (actual over equation 106: DC-10-10 1.42,
//! L-1011 1.43, DC-10-40 1.40) and over-predicts the F-28 and the DC-8s by
//! 30-55 % (0.65-0.76):
//! the `FNENG^0.69` term charges the engine count, while the group follows the
//! cabin it lights and powers. The same report correlates the group with the
//! all-coach passenger count instead, which is what this module evaluates.
//!
//! # Relation
//!
//! NASA CR-151970, Fig. 5.29, p. 5-74 (and Table 3.1), commercial aircraft:
//!
//! `W9 = 16.2 Np + 110`, lb,
//!
//! with `Np` the passengers the cabin carries "normalized by assuming all
//! coach seating at a seat pitch of 34 inches" at the cabin's coach abreast
//! (p. 5-12; 6 abreast on the DC-8 and 707, 9 on the DC-10 and L-1011, 10 on
//! the 747): what the aircraft "could carry", not the seats of one cabin
//! mix. ALAS counts it on the cabin shell, all economy at 34 in and capped
//! by the exits (`alas_payload::layout::PayloadLayout::all_coach_capacity`),
//! so a cabin declared below its capacity keeps the `Np` of its shell and a
//! converted freighter takes its airliner's, as it keeps that airliner's
//! systems. The regression is the report's, unchanged, and nothing in it is
//! per aircraft.
//!
//! # Evidence and domain
//!
//! Over the 14 production jets of Table 5.14 the relation has a 16.6 % rms
//! log residual (equation 106: 24.8 %). The trijets are in its fit set, so it
//! was refitted on the narrow-bodies alone (no trijet, 747 or 707):
//! `W9 = 14.39 Np + 337` lb predicts the DC-10-10, L-1011 and DC-10-40 out of
//! sample at 1.04-1.08 of their itemised weights and the 747 at 0.80. A later
//! itemised statement agrees in direction: Obert, *Aerodynamic Design of
//! Transport Aircraft*, 1st edition (2009), Fig. 42.25, p. 581, gives the
//! A320 343 kg of electrical generation and 1,032 kg of distribution
//! (1,375 kg), with a further 200 kg of lighting booked in furnishings,
//! where equation 106 prices the ALAS A320 well below either figure.
//!
//! The report fits it without the 707, the 747 and the SCAT-15 study
//! (p. 5-70): the 747's 4 x 60 kVA generators and dual low-intensity cabin
//! lighting make it 26 % lighter than the line, the DC-10's 3 x 90 kVA and
//! triple high-intensity lighting sit on it. Its production airliners run
//! from the F-28 (`Np` 60) to the trijets (330), all turbofans; a smaller
//! cabin (the Citation 500 on the same figure, `Np` 8, sits 50 % above the
//! line), a propeller transport and an aircraft without a passenger cabin (a
//! production freighter, a custom payload) keep equation 106. Above `Np` 330
//! the line is extrapolated: the 747 is the only datum there and lies 20-26 %
//! below every `Np` law, so the wide-bodies above it carry that uncertainty.
//!
//! FLOPS equation 41 (`pod_mass_kg`) relieves the wing with 13 % of the
//! electrical group at the engine pods. That share is kept as the FLOPS
//! convention whichever relation sizes the group, although this group also
//! carries the cabin lighting; it moves the wing bending mass by tens of kg.

use crate::flops_transport::units::kg;
use crate::flops_transport::{FlopsTransportInputs, PropulsionSizing};

/// The smallest all-coach count among the transport airliners of the NASA
/// CR-151970 commercial regression (the F-28, Table 5.1 and Fig. 5.29).
pub(super) const MIN_COMMERCIAL_ALL_COACH_PASSENGERS: usize = 60;

/// NASA CR-151970 commercial electrical relation, Fig. 5.29, p. 5-74, kg:
/// `W9 = 16.2 Np + 110` lb for an all-coach passenger count `Np`.
pub(super) fn commercial_electrical_kg(all_coach_passengers: usize) -> f64 {
    kg(16.2 * all_coach_passengers as f64 + 110.0)
}

/// The electrical group, kg: the NASA CR-151970 relation for a turbofan
/// transport whose cabin shell seats at least
/// [`MIN_COMMERCIAL_ALL_COACH_PASSENGERS`] all-coach, otherwise FLOPS
/// equation 106 with the scaled engine count `FNENG` and the systems
/// passenger count.
pub(super) fn electrical_group_kg(
    inputs: &FlopsTransportInputs,
    scaled_engines: f64,
    systems_passengers: usize,
) -> f64 {
    match inputs.all_coach_passenger_count {
        Some(all_coach)
            if all_coach >= MIN_COMMERCIAL_ALL_COACH_PASSENGERS
                && matches!(inputs.propulsion_sizing, PropulsionSizing::RatedThrust) =>
        {
            commercial_electrical_kg(all_coach)
        }
        _ => super::electrical_kg(
            inputs.fuselage_length_m,
            inputs.fuselage_width_m,
            inputs.fuselage_count,
            scaled_engines,
            inputs.flight_crew_count,
            systems_passengers,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_units::POUND_MASS;

    /// The regression line against the itemised trijet weights it was fitted
    /// on (Table 5.14: DC-10-10 5,366 lb, L-1011 5,490 lb, both `Np` 330), and
    /// against the 747 the report excluded (5,305 lb at `Np` 435).
    #[test]
    fn the_commercial_relation_reproduces_the_trijets_and_states_the_747_residual() {
        let trijet_lb = commercial_electrical_kg(330) / POUND_MASS;
        assert!((trijet_lb - 5_456.0).abs() < 1e-9);
        assert!((5_366.0 / trijet_lb - 1.0).abs() < 0.02);
        assert!((5_490.0 / trijet_lb - 1.0).abs() < 0.01);
        let jumbo_lb = commercial_electrical_kg(435) / POUND_MASS;
        assert!((5_305.0 / jumbo_lb - 0.741).abs() < 0.001);
    }
}
