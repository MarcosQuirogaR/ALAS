// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The bracketed inverse of the rating-fraction thrust map of the deck.

use super::*;

impl PropulsionDeck {
    /// Bracketed inverse of the rating-fraction thrust map.
    ///
    /// The turbofan deck is linear in the fraction, so the first secant
    /// guess is exact there; the turboprop's propeller surrogate is smooth
    /// and monotone, so regula falsi with a stagnation safeguard converges in
    /// a few evaluations.
    ///
    /// The safeguard is the Illinois down-weighting, and it is load-bearing.
    /// Plain regula falsi retains one endpoint indefinitely on a convex map,
    /// so the bracket shrinks only through the moving side; the earlier form
    /// of this solver additionally clamped each iterate 5 % inside the
    /// bracket, capping the per-iteration contraction at 0.95. On the PW127M
    /// the two together meant every off-rating request ran the full
    /// [`THRUST_SOLVE_MAX_ITERATIONS`] and left through the terminal linear
    /// interpolation rather than converging: measured at **171 us per
    /// `at_thrust` call against 3.2 us per `rated_point`**, about 54 deck
    /// evaluations each time, which made one coupled ATR leg 167 ms against
    /// the A320-200's 0.6 ms. Halving the retained endpoint's residual after
    /// it is retained twice restores superlinear convergence, so the solve
    /// reaches the 1e-6 relative thrust tolerance, usually *inside* it rather
    /// than falling out of the loop, in a handful of evaluations. The deck,
    /// the rating map and the tolerance are unchanged by this.
    pub(super) fn solve_fraction(
        &self,
        flight: FlightCondition,
        required_n: f64,
        cap: PropulsionRating,
        ceiling: OperatingPoint,
    ) -> Result<OperatingPoint, DeckError> {
        let point_at = |fraction: f64| -> Result<OperatingPoint, DeckError> {
            let result = self.evaluate(
                flight,
                PropulsionDemand::RatedFraction {
                    rating: cap,
                    fraction,
                },
            )?;
            Ok(OperatingPoint {
                thrust_n: result.body_force_n[0],
                fuel_flow_kg_s: jet_a_mass_flow(&result)?,
                rating_fraction: fraction,
                rating: cap,
                limit: ThrustLimit::Unlimited,
            })
        };
        let tolerance_n = THRUST_SOLVE_RELATIVE_TOLERANCE * required_n.abs().max(1.0);
        let mut low_fraction = 0.0;
        let mut low = point_at(0.0)?;
        let mut high_fraction = 1.0;
        let mut high = ceiling;
        if !(low.thrust_n <= required_n && required_n <= high.thrust_n) {
            return Err(DeckError::Model(format!(
                "rating map does not bracket {required_n:.1} N ({:.1}..{:.1} N)",
                low.thrust_n, high.thrust_n
            )));
        }
        // Residuals carried separately from the bracket points: the Illinois
        // safeguard down-weights the *retained* side's residual to move the
        // next iterate off it, while the monotonicity check below must still
        // compare against the deck's real thrust and fuel flow.
        let mut low_residual = low.thrust_n - required_n;
        let mut high_residual = high.thrust_n - required_n;
        let mut low_retained = 0_u32;
        let mut high_retained = 0_u32;
        for _ in 0..THRUST_SOLVE_MAX_ITERATIONS {
            let span = high_residual - low_residual;
            let secant = if span > 0.0 {
                low_fraction - low_residual / span * (high_fraction - low_fraction)
            } else {
                0.5 * (low_fraction + high_fraction)
            };
            // Keep the iterate strictly inside the bracket so a flat map
            // cannot pin it to an endpoint. The margin is the smallest that
            // does that.
            let width = high_fraction - low_fraction;
            let fraction = secant
                .max(low_fraction + 1.0e-6 * width)
                .min(high_fraction - 1.0e-6 * width);
            let point = point_at(fraction)?;
            // The rating map must be monotone in both thrust and fuel flow
            // inside the bracket; a deck that is not is an invalid off-design
            // evaluation, not a root to close on numerically.
            let thrust_tolerance = 1.0e-9 * ceiling.thrust_n.abs().max(1.0);
            let flow_tolerance = 1.0e-9 * ceiling.fuel_flow_kg_s.abs().max(1.0e-6);
            if point.thrust_n < low.thrust_n - thrust_tolerance
                || point.thrust_n > high.thrust_n + thrust_tolerance
                || point.fuel_flow_kg_s < low.fuel_flow_kg_s - flow_tolerance
                || point.fuel_flow_kg_s > high.fuel_flow_kg_s + flow_tolerance
            {
                return Err(DeckError::Model(format!(
                    "rating map is not monotone at fraction {fraction:.4}: {:.1} N / {:.5} kg/s between ({:.1} N / {:.5} kg/s) and ({:.1} N / {:.5} kg/s)",
                    point.thrust_n,
                    point.fuel_flow_kg_s,
                    low.thrust_n,
                    low.fuel_flow_kg_s,
                    high.thrust_n,
                    high.fuel_flow_kg_s
                )));
            }
            if (point.thrust_n - required_n).abs() <= tolerance_n {
                return Ok(point);
            }
            if point.thrust_n < required_n {
                low_fraction = fraction;
                low_residual = point.thrust_n - required_n;
                low = point;
                low_retained = 0;
                high_retained += 1;
                if high_retained >= 2 {
                    high_residual *= 0.5;
                }
            } else {
                high_fraction = fraction;
                high_residual = point.thrust_n - required_n;
                high = point;
                high_retained = 0;
                low_retained += 1;
                if low_retained >= 2 {
                    low_residual *= 0.5;
                }
            }
            if high_fraction - low_fraction < 1.0e-12 {
                break;
            }
        }
        // The loop ended on a collapsed bracket or on its iteration cap
        // without a point inside the tolerance. Where the two ends are still
        // within the tolerance band of the request the map is continuous
        // across it and the interpolation below is that point. A wider gap is
        // a jump of the rating map across the request, or a solve that did
        // not converge: no rating fraction is known to deliver the thrust.
        let span = high.thrust_n - low.thrust_n;
        if span.is_nan() || span > 2.0 * tolerance_n {
            return Err(DeckError::RatingSolveNotConverged {
                required_n,
                low_thrust_n: low.thrust_n,
                high_thrust_n: high.thrust_n,
                low_fraction,
                high_fraction,
            });
        }
        let weight = if span > 0.0 {
            ((required_n - low.thrust_n) / span).clamp(0.0, 1.0)
        } else {
            0.5
        };
        Ok(OperatingPoint {
            thrust_n: required_n,
            fuel_flow_kg_s: low.fuel_flow_kg_s
                + weight * (high.fuel_flow_kg_s - low.fuel_flow_kg_s),
            rating_fraction: low_fraction + weight * (high_fraction - low_fraction),
            rating: cap,
            limit: ThrustLimit::Unlimited,
        })
    }
}
