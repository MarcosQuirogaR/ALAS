// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The rating and the fraction of it that a propulsion demand asks of the PW127M surrogate.

use super::*;

impl Atr72TurbopropSystem {
    pub(super) fn rating_and_fraction(
        &self,
        demand: PropulsionDemand,
        active_count: usize,
    ) -> Result<(Pw127mRating, f64), PropulsionError> {
        // The automatic reserve rating applies to exactly one engine
        // inoperative; for the two-engine ATR that is one active unit.
        let takeoff = if active_count + 1 == self.unit_count() {
            Pw127mRating::MaximumTakeoffReserve
        } else {
            Pw127mRating::NormalTakeoff
        };
        match demand {
            PropulsionDemand::NormalizedForce(value)
                if value.is_finite() && (0.0..=1.0).contains(&value) =>
            {
                // A normalized throttle is a fraction of the maximum-climb
                // force with all engines running (the turbofan deck's
                // convention), not of the take-off rating: a full command in
                // climb or cruise must not draw take-off power (PROP-13). The
                // one-engine-inoperative case keeps the automatic reserve.
                if active_count + 1 == self.unit_count() {
                    Ok((takeoff, value))
                } else {
                    Ok((Pw127mRating::MaximumClimb, value))
                }
            }
            PropulsionDemand::NormalizedForce(value) => Err(PropulsionError::InvalidInput {
                field: "normalized propulsion demand",
                value,
            }),
            PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround) => Ok((takeoff, 1.0)),
            PropulsionDemand::Rating(PropulsionRating::MaximumContinuous) => {
                Ok((Pw127mRating::MaximumContinuous, 1.0))
            }
            PropulsionDemand::Rating(PropulsionRating::MaximumClimb) => {
                Ok((Pw127mRating::MaximumClimb, 1.0))
            }
            PropulsionDemand::Rating(PropulsionRating::Cruise) => {
                Ok((Pw127mRating::MaximumCruise, 1.0))
            }
            PropulsionDemand::Rating(PropulsionRating::FlightIdle) => {
                Ok((Pw127mRating::FlightIdleSurrogate, 1.0))
            }
            PropulsionDemand::RatedFraction { rating, fraction }
                if fraction.is_finite() && (0.0..=1.0).contains(&fraction) =>
            {
                let (rating, _) =
                    self.rating_and_fraction(PropulsionDemand::Rating(rating), active_count)?;
                Ok((rating, fraction))
            }
            PropulsionDemand::RatedFraction { fraction, .. } => {
                Err(PropulsionError::InvalidInput {
                    field: "rated force fraction",
                    value: fraction,
                })
            }
            PropulsionDemand::RequiredBodyForceN(_) => Err(PropulsionError::UnsupportedDemand(
                "ATR turboprop surrogate has no inverse force solver",
            )),
        }
    }
}
