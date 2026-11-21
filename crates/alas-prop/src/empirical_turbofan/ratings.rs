// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rated-thrust evaluation of the empirical turbofan deck: take-off (Mach
//! limited, floored at maximum climb), maximum climb (Jenkinson charts) and
//! the flat-rating break.

use super::*;

impl EmpiricalTurbofanModel {
    /// Take-off rating thrust, N: the Jenkinson take-off chart ratio (Figs.
    /// 9.18(a)-9.21(a), held at its Mach limit, see
    /// [TAKEOFF_CORRELATION_MAXIMUM_MACH]) times the static take-off thrust
    /// and the flat-rating break, floored at the maximum-climb rating so that
    /// the rating order flight idle < maximum climb <= take-off holds at every
    /// Mach and altitude (the floor binds only where the take-off charts
    /// have been extended above their highest printed curve).
    ///
    /// The engine anchors are not applied to it: the ratio is exactly 1 at
    /// sea-level static, so the installed static thrust is the rated one.
    pub(super) fn takeoff_available_n(&self, flight: FlightCondition) -> f64 {
        let delta = flight.pressure_pa / 101_325.0;
        let mach = flight.mach.min(TAKEOFF_CORRELATION_MAXIMUM_MACH);
        let ratio = self.climb_reference.table.takeoff_ratio(mach, delta);
        let chart_n = ratio
            * self.deck.takeoff_thrust_n
            * flat_rating_thrust_factor(self.deck.flat_rating_temperature_k, flight);
        let floor_n = self.climb_available_n(flight, self.deck.max_climb_rate_ft_min);
        chart_n.max(floor_n).max(0.0)
    }

    /// Maximum-climb thrust, N, at `flight`: the chart ratio times the
    /// static take-off thrust and the flat-rating break.
    ///
    /// `climb_rate_ft_min` is accepted for the callers that carry the
    /// representative climb rate; the Jenkinson charts have no climb-rate
    /// term (the earlier Bartel-Young exponent moved thrust by 0.5-2.5 %),
    /// so it does not change the result.
    pub(super) fn climb_available_n(
        &self,
        flight: FlightCondition,
        _climb_rate_ft_min: Option<f64>,
    ) -> f64 {
        flat_rating_thrust_factor(self.deck.flat_rating_temperature_k, flight)
            * self.climb_correlation_n(flight)
    }

    fn climb_correlation_n(&self, flight: FlightCondition) -> f64 {
        let ratio = self
            .climb_reference
            .table
            .ratio(flight.mach, flight.pressure_pa / 101_325.0);
        (ratio * self.deck.takeoff_thrust_n).max(0.0)
    }

    /// Flight-idle **net** thrust, N, at `flight`: the Jenkinson descent-thrust
    /// ratio `FN / FN*` ([super::flight_idle]) times the installed static
    /// take-off thrust. Gross idle thrust minus the inlet ram drag, so it is
    /// negative where the ram drag wins (low height, high Mach number). The
    /// flat-rating break does not apply: an idling engine is not at a
    /// temperature limit.
    pub(super) fn idle_net_thrust_n(&self, flight: FlightCondition) -> f64 {
        self.deck.takeoff_thrust_n
            * self
                .idle
                .net_thrust_ratio(flight.mach, flight.pressure_pa / 101_325.0)
    }

    /// Flight-idle fuel flow, kg/s, at `flight` before the installation
    /// factor: the engine's ICAO idle fuel flow at sea-level static (the 7 %
    /// point of its LTO cycle) carried to the condition by the shape of
    /// Jenkinson's descent fuel-flow chart ([super::flight_idle]).
    pub(super) fn idle_fuel_flow_kg_s(&self, flight: FlightCondition) -> f64 {
        self.idle_fuel_static_kg_s
            * self
                .idle
                .fuel_flow_over_static(flight.mach, flight.pressure_pa / 101_325.0)
    }

    /// The operating point of a normalized-force command `fraction` in [0, 1]:
    /// `(utilization, force, achieved demand, active limits)`.
    ///
    /// The command is a fraction of the maximum-climb force, floored at the
    /// flight-idle force: the contract the mission solvers read (the floor is
    /// reported as the `flight-idle-thrust` limit, so a request below it keeps
    /// a gradient there). The floor is the sourced net idle force where it is
    /// positive. Where it is negative (low height, high Mach number: the ram
    /// drag exceeds the idle gross thrust) the command range stops at zero
    /// force; the net drag below zero is the `Rating(FlightIdle)` point and
    /// the capability minimum, which a consumer that solves for a thrust
    /// (`alas-opt`'s deck) uses.
    pub(super) fn normalized_force_point(
        &self,
        flight: FlightCondition,
        fraction: f64,
    ) -> (f64, f64, PropulsionDemand, Vec<ActiveLimit>) {
        let maximum_climb = self.climb_available_n(flight, self.deck.max_climb_rate_ft_min);
        let requested_thrust_n = fraction * maximum_climb;
        let idle_thrust = self.idle_net_thrust_n(flight).max(0.0);
        if requested_thrust_n >= idle_thrust {
            return (
                fraction,
                requested_thrust_n,
                PropulsionDemand::NormalizedForce(fraction),
                Vec::new(),
            );
        }
        // Below flight idle the engine delivers idle thrust, so fuel is
        // scheduled from the achieved idle utilization of the maximum-climb
        // rating, not from the request: every saturated request burns the
        // same idle fuel.
        let achieved = if maximum_climb > 0.0 {
            idle_thrust / maximum_climb
        } else {
            fraction
        };
        (
            achieved,
            idle_thrust,
            PropulsionDemand::NormalizedForce(achieved),
            vec![ActiveLimit {
                name: "flight-idle-thrust".to_owned(),
                utilization: 1.0,
            }],
        )
    }

    /// The catalogue part-power cruise thrust over the maximum-climb rating at
    /// the reference point; above 1 the two inputs disagree by the excess
    /// (the TSFC anchor then treats the catalogue figure as full rating).
    pub fn catalogue_cruise_over_maximum_climb(&self) -> f64 {
        self.climb_reference.catalogue_cruise_over_maximum_climb
    }

    /// The engine anchor factor applied to the Jenkinson chart maximum-climb
    /// table, or `None` when the engine has no published cruise datum and the
    /// chart is used unscaled (a flagged evidence gap).
    pub fn engine_scale(&self) -> Option<f64> {
        self.climb_reference.engine_scale
    }
}
