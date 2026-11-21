// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The operating point behind [`PropulsionSystemModel::evaluate`]: the thrust,
//! fuel flow, achieved demand and active limits of one request, before the
//! validity verdict and the result's bookkeeping (resource and state lists,
//! provenance). [`PropulsionSystemModel::evaluate_point`] stops here, which is
//! what a mission solver's inner loop reads.

use super::*;

/// One solved operating point of the deck.
pub(super) struct SolvedPoint {
    /// Net thrust, N.
    pub(super) thrust_n: f64,
    /// Jet-A mass flow, kg/s.
    pub(super) fuel_flow_kg_s: f64,
    /// Demand actually achieved.
    pub(super) achieved_demand: PropulsionDemand,
    /// Limits active at the point.
    pub(super) active_limits: Vec<ActiveLimit>,
}

impl EmpiricalTurbofanModel {
    /// Solve `request` to its thrust and fuel flow, with the errors
    /// [`PropulsionSystemModel::evaluate`] reports.
    pub(super) fn solve_point(
        &self,
        request: &PropulsionRequest,
    ) -> Result<SolvedPoint, PropulsionError> {
        if request.mode != OperatingMode::Normal {
            return Err(PropulsionError::UnsupportedMode(request.mode));
        }
        if request.failure != FailureState::None {
            return Err(PropulsionError::UnsupportedFailureState);
        }
        let loads = installation::checked_loads(request.loads)?;
        validate_flight(request.flight)?;
        let (rating_utilization, thrust_n, achieved_demand, active_limits) = match request.demand {
            PropulsionDemand::NormalizedForce(requested_fraction)
                if requested_fraction.is_finite() && (0.0..=1.0).contains(&requested_fraction) =>
            {
                self.normalized_force_point(request.flight, requested_fraction)
            }
            demand => {
                let (utilization, thrust) =
                    self.rating_fraction_and_thrust(request.flight, demand)?;
                (utilization, thrust, demand, Vec::new())
            }
        };
        // Corrected thrust FN/(delta FN*) of the rating the utilization is a
        // share of (the rated thrust over the utilization, before any bleed
        // loss), the abscissa of the flight SFC loops. Flight idle has no
        // rating basis: its fuel is the idle chart flow and its shape the ICAO
        // one.
        let full_rating_x = match request.demand {
            PropulsionDemand::Rating(PropulsionRating::FlightIdle)
            | PropulsionDemand::RatedFraction {
                rating: PropulsionRating::FlightIdle,
                ..
            } => None,
            _ => {
                let delta = request.flight.pressure_pa / 101_325.0;
                let corrected = thrust_n / (delta * self.deck.takeoff_thrust_n);
                (rating_utilization > 0.0 && corrected > 0.0 && corrected.is_finite())
                    .then_some(corrected / rating_utilization)
            }
        };
        // A take-off rating with customer bleed loses thrust (installation.rs).
        let thrust_n = self.takeoff_bleed_thrust_n(request, thrust_n);
        if !thrust_n.is_finite() {
            return Err(PropulsionError::InvalidInput {
                field: "empirical turbofan thrust",
                value: thrust_n,
            });
        }
        // Fuel scheduling must use utilization of the active flight rating,
        // not thrust divided by sea-level-static takeoff thrust. The latter
        // mistakes a fully commanded high-altitude engine for part power and
        // has no thermodynamic meaning.
        let rating_utilization = rating_utilization.clamp(MINIMUM_UTILIZATION, 1.0);
        // Fuel flow is TSFC (interpolated, not absolute flow) times the thrust,
        // the part-power shape and the installed-to-uninstalled load factor.
        let shape = tsfc_shape(
            rating_utilization,
            request.flight.mach,
            full_rating_x,
            &self.sfc_loops,
            self.deck.part_power_fuel_flow_ratios,
        );
        let installation = self.installation_sfc_factor(request.flight, loads);
        // The engine never burns less than at flight idle: where the net
        // thrust is at or below zero (the idle ram-drag region) the TSFC-times-
        // thrust form vanishes and the idle chart flow is the fuel.
        let fuel_flow = (self.full_rating_tsfc_kg_n_s(request.flight) * thrust_n * shape)
            .max(self.idle_fuel_flow_kg_s(request.flight))
            * installation;
        if !fuel_flow.is_finite() || fuel_flow < 0.0 {
            return Err(PropulsionError::NonFiniteOutput(
                "ICAO-LTO fuel interpolation",
            ));
        }
        Ok(SolvedPoint {
            thrust_n,
            fuel_flow_kg_s: fuel_flow,
            achieved_demand,
            active_limits,
        })
    }
}
