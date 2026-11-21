// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The answers one root solve has already had from the propulsion deck and
//! the drag polar, per control point.
//!
//! `hybrd` builds its Jacobian by forward differences (`fdjac1`): one residual
//! evaluation per unknown, each moving that unknown alone. A segment's
//! unknowns are each control point's throttle and body angle. A point's
//! thrust request reads only its own throttle and flight condition, and its
//! aerodynamics request only its own angle of attack and flight condition, so
//! in every Jacobian column but one a point asks again, bit for bit, what it
//! asked at the base point. The memo answers such a request with the answer
//! it already had. The deck and the polar are pure functions of their
//! request, so the residuals, the Jacobian and the solve are unchanged; only
//! the repeated evaluations go.
//!
//! A memo holds the borrow of the analyses it was filled under, so it cannot
//! answer for another deck or polar, and [`crate::solve::converge_root`]
//! keeps one for a single solve.

use alas_atmo::Us1976Values;
use alas_prop::mission_turbofan::ThrustOutput;
use alas_prop::system::PropulsionDemand;

use crate::segments::{AeroSolution, MissionAnalyses, Segment};

impl Segment {
    /// [`Segment::iterate_with`] with no memo: every request is evaluated.
    pub fn iterate(&mut self, analyses: &MissionAnalyses) {
        self.iterate_with(&mut PointMemo::off(analyses));
    }
}

/// Answers kept per control point and kind: the base point's and a perturbed
/// one's, for up to two demands one point makes.
const SLOTS: usize = 4;

/// One control point's latest answers, the oldest replaced first.
struct Slots<K, V> {
    entries: [Option<(K, V)>; SLOTS],
    next: usize,
}

impl<K: PartialEq, V: Clone> Slots<K, V> {
    fn new() -> Self {
        Self {
            entries: std::array::from_fn(|_| None),
            next: 0,
        }
    }

    /// The held answer to `key`, or `evaluate`'s, which is then held.
    fn answer(&mut self, key: K, evaluate: impl FnOnce() -> V) -> V {
        if let Some((_, value)) = self.entries.iter().flatten().find(|(held, _)| *held == key) {
            return value.clone();
        }
        let value = evaluate();
        self.entries[self.next] = Some((key, value.clone()));
        self.next = (self.next + 1) % SLOTS;
        value
    }
}

/// Per-point answers of one root solve (see the module documentation).
pub struct PointMemo<'a> {
    analyses: &'a MissionAnalyses,
    thrust: Vec<Slots<[u64; 17], (ThrustOutput, bool)>>,
    aerodynamics: Vec<Slots<[u64; 4], AeroSolution>>,
}

impl<'a> PointMemo<'a> {
    /// A memo for `points` control points, answering for `analyses`.
    pub fn new(analyses: &'a MissionAnalyses, points: usize) -> Self {
        Self {
            analyses,
            thrust: (0..points).map(|_| Slots::new()).collect(),
            aerodynamics: (0..points).map(|_| Slots::new()).collect(),
        }
    }

    /// No memo: every request is evaluated.
    pub fn off(analyses: &'a MissionAnalyses) -> Self {
        Self::new(analyses, 0)
    }

    /// The analyses this memo answers for.
    pub fn analyses(&self) -> &'a MissionAnalyses {
        self.analyses
    }

    /// The deck's answer to `demand` at control point `point`
    /// ([`MissionAnalyses::thrust_with_domain`] for a normalized force).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn thrust(
        &mut self,
        point: usize,
        atmosphere: &Us1976Values,
        altitude_m: f64,
        velocity_m_s: f64,
        mach: f64,
        gravity_m_s2: f64,
        demand: PropulsionDemand,
    ) -> (ThrustOutput, bool) {
        let analyses = self.analyses;
        let evaluate = || {
            analyses.thrust_for_demand(
                atmosphere,
                altitude_m,
                velocity_m_s,
                mach,
                gravity_m_s2,
                demand,
            )
        };
        let Some(slots) = self.thrust.get_mut(point) else {
            return evaluate();
        };
        let mut key = [0_u64; 17];
        let numbers = [
            atmosphere.pressure_pa,
            atmosphere.temperature_k,
            atmosphere.density_kg_m3,
            atmosphere.speed_of_sound_m_s,
            atmosphere.dynamic_viscosity_pa_s,
            atmosphere.kinematic_viscosity_m2_s,
            atmosphere.thermal_conductivity_w_m_k,
            atmosphere.prandtl_number,
            altitude_m,
            velocity_m_s,
            mach,
            gravity_m_s2,
        ];
        for (bits, number) in key.iter_mut().zip(numbers) {
            *bits = number.to_bits();
        }
        key[12..].copy_from_slice(&demand_bits(demand));
        slots.answer(key, evaluate)
    }

    /// [`MissionAnalyses::aerodynamics`] at control point `point`.
    pub(crate) fn aerodynamics(
        &mut self,
        point: usize,
        angle_of_attack_rad: f64,
        mach: f64,
        temperature_k: f64,
        reynolds_number_per_m: f64,
    ) -> AeroSolution {
        let analyses = self.analyses;
        let evaluate = || {
            analyses.aerodynamics(
                angle_of_attack_rad,
                mach,
                temperature_k,
                reynolds_number_per_m,
            )
        };
        let Some(slots) = self.aerodynamics.get_mut(point) else {
            return evaluate();
        };
        let key = [
            angle_of_attack_rad,
            mach,
            temperature_k,
            reynolds_number_per_m,
        ]
        .map(f64::to_bits);
        slots.answer(key, evaluate)
    }
}

/// A demand as bit patterns: its kind, its numbers and its rating.
fn demand_bits(demand: PropulsionDemand) -> [u64; 5] {
    match demand {
        PropulsionDemand::NormalizedForce(fraction) => [0, fraction.to_bits(), 0, 0, 0],
        PropulsionDemand::RequiredBodyForceN([x, y, z]) => {
            [1, x.to_bits(), y.to_bits(), z.to_bits(), 0]
        }
        PropulsionDemand::Rating(rating) => [2, 0, 0, 0, rating as u64],
        PropulsionDemand::RatedFraction { rating, fraction } => {
            [3, fraction.to_bits(), 0, 0, rating as u64]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_answer_is_returned_only_for_the_identical_key() {
        let mut slots = Slots::<[u64; 1], f64>::new();
        let mut evaluations = 0;
        let mut ask = |slots: &mut Slots<[u64; 1], f64>, x: f64| {
            slots.answer([x.to_bits()], || {
                evaluations += 1;
                x * 2.0
            })
        };
        assert_eq!(ask(&mut slots, 1.0), 2.0);
        assert_eq!(ask(&mut slots, 1.0), 2.0);
        assert_eq!(ask(&mut slots, -0.0), -0.0);
        assert_eq!(ask(&mut slots, 0.0), 0.0);
        // 1.0, -0.0 and 0.0: three evaluations, the repeated 1.0 answered.
        assert_eq!(evaluations, 3);
    }

    #[test]
    fn the_oldest_answer_makes_room_and_is_evaluated_again() {
        let mut slots = Slots::<[u64; 1], u64>::new();
        let mut evaluations = 0_u64;
        let mut ask = |slots: &mut Slots<[u64; 1], u64>, key: u64| {
            slots.answer([key], || {
                evaluations += 1;
                key
            })
        };
        for key in 0..=SLOTS as u64 {
            assert_eq!(ask(&mut slots, key), key);
        }
        // Key 0 was replaced by the fifth key; the newest are still held.
        assert_eq!(ask(&mut slots, SLOTS as u64), SLOTS as u64);
        assert_eq!(ask(&mut slots, 0), 0);
        assert_eq!(evaluations, SLOTS as u64 + 2);
    }

    #[test]
    fn every_demand_kind_and_rating_has_its_own_bits() {
        use alas_prop::system::PropulsionRating as R;
        let demands = [
            PropulsionDemand::NormalizedForce(0.5),
            PropulsionDemand::RequiredBodyForceN([0.5, 0.0, 0.0]),
            PropulsionDemand::Rating(R::Cruise),
            PropulsionDemand::Rating(R::MaximumClimb),
            PropulsionDemand::RatedFraction {
                rating: R::TakeoffGoAround,
                fraction: 0.5,
            },
            PropulsionDemand::RatedFraction {
                rating: R::FlightIdle,
                fraction: 0.5,
            },
        ];
        for (i, a) in demands.iter().enumerate() {
            for (j, b) in demands.iter().enumerate() {
                assert_eq!(demand_bits(*a) == demand_bits(*b), i == j, "{a:?} {b:?}");
            }
        }
    }
}
