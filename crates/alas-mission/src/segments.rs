// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Analyses/Mission/Segments/Climb/Unknown_Throttle.py,
// mission analysis model/Analyses/Mission/Segments/Cruise/Constant_Speed_Constant_Altitude.py,
// the two Constant_Speed_Constant_Rate segments, and the methods they name in
// mission analysis model/Methods/Missions/Segments/{Climb,Cruise}/Common.py.
// Upstream: mission analysis model 2.5.2, LGPL-2.1 (relicensed under GPL-2.0-or-later per
// LGPL-2.1 section 3; compatible with this program's AGPL-3.0-or-later).

//! One leg of a mission, and the two-unknown system that flies it.
//!
//! A segment is a boundary-value problem stated as an algebraic one. The
//! trajectory is *given*: a climb is sixteen points evenly spaced in
//! altitude between two altitudes, flown at a fixed true airspeed and a fixed
//! rate; a cruise is sixteen points evenly spaced in time across a fixed
//! distance at a fixed altitude, and what is unknown is how the aeroplane
//! has to be flown to follow it: a throttle and a body angle at each point.
//! Two unknowns per point, two force residuals per point, thirty-two of each
//! over the sixteen points [`crate::Numerics`] discretizes on.
//!
//! [`Segment::initialize`] lays the trajectory down and [`Segment::iterate`]
//! evaluates one candidate: it reconciles the segment with the one before it,
//! applies the unknowns, walks the flow state forward through the atmosphere,
//! the engine and the drag polar, burns the fuel, sums the forces, and reports
//! what did not balance. [`crate::solve`] is what drives it.
//!
//! # Scope
//!
//! The three kinds `mission_setup` builds and no others:
//! `Climb.Constant_Speed_Constant_Rate`,
//! `Cruise.Constant_Speed_Constant_Altitude` and
//! `Descent.Constant_Speed_Constant_Rate`, over the `Unknown_Throttle` process
//! chain the first and third share and the second reproduces almost exactly.
//! A cruise segment's iterate chain omits `update_acceleration` unless it is a level
//! speed change (which has a `speed_profile`).
//! The frozen compatibility path also retains the historical horizontal-force
//! magnitude residual, while product missions use the signed longitudinal
//! component so the root finder can distinguish thrust surplus from deficit.
//!
//! Untranslated because unreachable: `Energy.initialize_battery` (there is no
//! battery behind a turbofan), `Weights.update_weights`' additional-fuel
//! branch (a turbofan network reports no additional fuel rate),
//! `Noise.compute_noise` (no noise analysis is attached, so it returns
//! immediately), `aero_derivatives` (skipped) and `update_stability` (reports nothing read).

pub mod analyses;
pub mod blend;
pub mod common;
pub mod conditions;
pub mod drag;
pub mod error;
pub mod frames;
mod initial;

pub use analyses::{AeroSolution, LegacyTurbofanCompatibility, MissionAnalyses};
pub use conditions::{Conditions, Initials, Matrix3, Vector3};
pub use drag::{MissionDragCoefficients, MissionDragModel, MissionDragSource};
pub use error::SegmentError;

use crate::numerics::Numerics;
use alas_config::mission::{SpeedCaps, SpeedReference};
pub use blend::{
    acceleration_climb_altitude_gain_m, along_path_speed, energy_share_factor, profile_mean_speed,
    profile_speed, ramp_speed, toward_exit_speed, PathEnds,
};
use initial::{INITIAL_CLIMB_BODY_ANGLE_RAD, INITIAL_CRUISE_BODY_ANGLE_RAD, INITIAL_THROTTLE};

/// Smallest vertical rate a rating-limited climb is evaluated at, m/s (about 20 ft/min): the
/// solver's trial rate is held above it so the climb keeps a finite, well-conditioned duration
/// (`dt/dtau = dz / v_z`; a rate of zero is a level leg). A numerical floor, not a performance
/// limit: it sits below the 100 ft/min residual climb of a service ceiling, so a point held on it
/// is a point past the ceiling for the caller whatever its value.
pub(crate) const MIN_RATED_CLIMB_RATE_M_S: f64 = 0.1;

/// Which of the three trajectories a segment flies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SegmentKind {
    /// Climb at a constant true airspeed and a constant rate.
    Climb {
        /// Starting altitude, m. `None` takes the previous segment's final
        /// altitude, which is what every segment but the first does.
        altitude_start_m: Option<f64>,
        /// Ending altitude, m.
        altitude_end_m: f64,
        /// Rate of climb, m/s, positive up.
        climb_rate_m_s: f64,
    },
    /// Cruise at a constant true airspeed and a constant altitude, for a set
    /// ground distance.
    Cruise {
        /// The altitude to hold, m. `None` takes the previous segment's final
        /// altitude, which is what every cruise leg in this mission does.
        altitude_m: Option<f64>,
        /// The distance to cover, m.
        distance_m: f64,
    },
    /// Descend at a constant true airspeed and a constant rate.
    Descent {
        /// Starting altitude, m; `None` as for a climb.
        altitude_start_m: Option<f64>,
        /// Ending altitude, m.
        altitude_end_m: f64,
        /// Rate of descent, m/s, positive *down*.
        descent_rate_m_s: f64,
    },
}

/// Everything `mission_setup` sets on one segment.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentSpec {
    /// The segment's name, which is what a disagreement is reported against.
    pub tag: String,
    /// Which trajectory it flies.
    pub kind: SegmentKind,
    /// True or calibrated airspeed, m/s, per `air_speed_reference`.
    pub air_speed_m_s: f64,
    /// Whether `air_speed_m_s` above is a true airspeed (the legacy, and
    /// still default, semantics: flown unchanged at every control point)
    /// or a calibrated airspeed. A calibrated [`SegmentKind::Climb`] or
    /// [`SegmentKind::Descent`] resolves the true airspeed actually flown at
    /// *each control point's own altitude* from the real ambient pressure
    /// and temperature there ([`alas_atmo::airspeed::true_from_calibrated`]),
    /// so it rises through the climb or falls through the descent instead of
    /// staying constant; a calibrated [`SegmentKind::Cruise`] is not
    /// meaningful (a cruise is already flown at one altitude) and is treated
    /// as true airspeed regardless of this field.
    pub air_speed_reference: SpeedReference,
    /// The CAS/Mach schedule's Mach hold and the MMO/VMO limits a climb or
    /// descent resolves its true airspeed against ([`SpeedCaps`]); all zero
    /// (the default) leaves the speed reference unchanged.
    pub speed_caps: SpeedCaps,
    /// Course over the ground, radians. Zero on every segment this program
    /// builds, and carried because the ground-track integral projects onto it.
    pub true_course_rad: f64,
    /// Deviation from the standard atmosphere, K, applied to every ambient
    /// quantity this segment evaluates: the atmosphere the aerodynamics and
    /// propulsion see at each control point and, in
    /// [`SpeedReference::CalibratedAirspeed`] mode, the pressure and
    /// temperature the calibrated airspeed is resolved against. The product
    /// schedule (`alas-pipeline::mission_stage::schedule`) sets it to the
    /// *departure* airport's ISA deviation on every segment of the mission,
    /// so a flight into a warmer arrival field still flies the departure
    /// deviation; the recorded parity fixtures carry zero.
    pub temperature_deviation_k: f64,
    /// How many control points the segment is discretized on. Sixteen.
    pub number_control_points: usize,
    /// `false` is the product envelope: the command is a fraction of the maximum-climb force and
    /// its ceiling is 1. `true` marks a take-off-rated leg: its command is a share of the take-off
    /// rating's force at each point (1 is `Rating(TakeoffGoAround)`), force and fuel are the deck's
    /// at that share, and its ceiling is 1 too (see `common::update_thrust`).
    pub takeoff_rated: bool,
    /// A [`SegmentKind::Climb`] flown at the maximum-climb rating: the command is 1 at every
    /// point and the vertical rate at each point is settled on it (`solve::converge_root`), so
    /// the rate comes out of the specific excess power `(T - D) V / W` at the scheduled speed
    /// (the total-energy climb of BADA practice) instead of being demanded. `climb_rate_m_s` is
    /// then only the starting guess and the mean rate the leg is planned with. `false` elsewhere.
    pub rating_limited: bool,
    /// Drag coefficient added to the clean-configuration total on the
    /// reference area for a leg flown with flaps, slats or gear extended
    /// (take-off and approach). Zero for a clean leg, the default of every
    /// recorded parity fixture.
    pub configuration_delta_cd: f64,
    /// Sustained load factor: lift is this times the weight (1 on a straight leg;
    /// defined on a level leg only, any other leg is refused).
    pub load_factor: f64,
    /// Speed, m/s, a climb or descent starts at (zero: its own schedule), reaching
    /// the schedule along its path by the total-energy model (see [`blend`]).
    pub entry_speed_m_s: f64,
    /// Speed, m/s, a climb or descent ends at (zero: its own schedule), the same
    /// energy relation read backwards from the end.
    pub exit_speed_m_s: f64,
    /// A level speed change: rows `[fraction of the duration, speed m/s]` from 0
    /// to 1 (empty: one speed, `air_speed_m_s`); the duration follows from the
    /// distance and the profile's mean.
    pub speed_profile: Vec<[f64; 2]>,
}

/// One mission leg: its schedule, its discretization and its state.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// What the segment flies.
    pub spec: SegmentSpec,
    /// Its pseudospectral operators.
    pub numerics: Numerics,
    /// Its state at every control point.
    pub conditions: Conditions,
    /// What it inherits from the segment before it, if any.
    pub initials: Option<Initials>,
    /// The throttle at each control point: the first unknown.
    pub throttle: Vec<f64>,
    /// The body angle at each control point, radians: the second unknown.
    pub body_angle_rad: Vec<f64>,
    /// The two force residuals at each control point, as
    /// `[horizontal, vertical]`.
    pub residuals: Vec<[f64; 2]>,
    /// The vertical rate at each control point of a rating-limited climb, m/s, positive up
    /// (empty on any other leg): what `solve::converge_root` settles so that the command is
    /// the rating at every point.
    pub rate_profile_m_s: Vec<f64>,
}

impl Segment {
    /// Build a segment and run its `initialize` process.
    ///
    /// That is `expand_state`, then
    /// `initialize_differentials_dimensionless`, then the kind's own
    /// `initialize_conditions`, then (for a climb or descent only)
    /// `update_differentials_altitude`, which is what turns a trajectory
    /// discretized in altitude into one that knows how long it takes.
    ///
    /// # Errors
    ///
    /// [`SegmentError::Discretization`] if the operators could not be built,
    /// [`SegmentError::NoStartingAltitude`] if the segment defers its starting
    /// altitude to a predecessor it does not have, and
    /// [`SegmentError::InvalidLoadFactor`] for a load factor that is not at
    /// least 1 or is not on a level leg.
    pub fn new(spec: SegmentSpec, initials: Option<Initials>) -> Result<Self, SegmentError> {
        let points = spec.number_control_points;
        let mut numerics = Numerics {
            number_control_points: i64::try_from(points).unwrap_or(i64::MAX),
            ..Numerics::default()
        };
        numerics.initialize_differentials_dimensionless()?;
        error::check_spec_load_factor(&spec)?;

        let initial_body_angle = match spec.kind {
            SegmentKind::Cruise { .. } => INITIAL_CRUISE_BODY_ANGLE_RAD,
            _ => INITIAL_CLIMB_BODY_ANGLE_RAD,
        };

        let (throttle, body_angle) = if spec.takeoff_rated {
            common::TAKEOFF_RATED_START
        } else {
            (INITIAL_THROTTLE, initial_body_angle)
        };
        let rate_profile_m_s = match spec.kind {
            SegmentKind::Climb { climb_rate_m_s, .. } if spec.rating_limited => {
                vec![climb_rate_m_s; points]
            }
            _ => Vec::new(),
        };
        let mut segment = Self {
            rate_profile_m_s,
            spec,
            numerics,
            conditions: Conditions::expanded(points),
            initials,
            throttle: vec![throttle; points],
            body_angle_rad: vec![body_angle; points],
            residuals: vec![[0.0; 2]; points],
        };
        segment.initialize_conditions()?;
        Ok(segment)
    }

    /// The altitude this segment starts at, resolving the deferred case.
    fn starting_altitude(&self, declared: Option<f64>) -> Result<f64, SegmentError> {
        if let Some(altitude) = declared {
            return Ok(altitude);
        }
        self.initials
            .map(|initials| -initials.position_vector_m[2])
            .ok_or_else(|| SegmentError::NoStartingAltitude {
                tag: self.spec.tag.clone(),
            })
    }

    /// Lay down the trajectory: `initialize_conditions` plus, for a climb or
    /// descent, `update_differentials_altitude`.
    fn initialize_conditions(&mut self) -> Result<(), SegmentError> {
        let nodes = self.numerics.dimensionless.control_points.clone();
        let air_speed = self.spec.air_speed_m_s;

        match self.spec.kind {
            SegmentKind::Climb {
                altitude_start_m,
                altitude_end_m,
                climb_rate_m_s,
            } => {
                error::check_vertical_rate(&self.spec.tag, "climb", climb_rate_m_s)?;
                let start = self.starting_altitude(altitude_start_m)?;
                // z points down, so a *climb* rate is a negative z velocity.
                self.lay_down_ramp(&nodes, start, altitude_end_m, air_speed, -climb_rate_m_s)?;
                common::update_differentials_altitude(
                    &mut self.conditions,
                    &self.numerics.dimensionless,
                );
            }
            SegmentKind::Descent {
                altitude_start_m,
                altitude_end_m,
                descent_rate_m_s,
            } => {
                error::check_vertical_rate(&self.spec.tag, "descent", descent_rate_m_s)?;
                let start = self.starting_altitude(altitude_start_m)?;
                self.lay_down_ramp(&nodes, start, altitude_end_m, air_speed, descent_rate_m_s)?;
                common::update_differentials_altitude(
                    &mut self.conditions,
                    &self.numerics.dimensionless,
                );
            }
            SegmentKind::Cruise {
                altitude_m,
                distance_m,
            } => {
                let altitude = self.starting_altitude(altitude_m)?;
                // A cruise segment is discretized in time rather than in
                // altitude, so it sets its own time here and has no
                // `update_differentials_altitude` step.
                let initial_time = self.conditions.time_s[0];
                let profile = self.spec.speed_profile.clone();
                let row = &self.numerics.dimensionless.integrate[nodes.len() - 1];
                let mean_speed = blend::profile_mean_speed(&profile, &nodes, row, air_speed);
                // A leg with a speed profile differentiates its velocity in
                // time, so it needs a positive span; a constant-speed leg of
                // zero distance is a leg of zero duration.
                let differentiated = !profile.is_empty();
                error::check_cruise_span(&self.spec.tag, distance_m, mean_speed, differentiated)?;
                let final_time = distance_m / mean_speed + initial_time;
                for (point, &node) in nodes.iter().enumerate() {
                    self.conditions.altitude_m[point] = altitude;
                    self.conditions.position_vector_m[point][2] = -altitude;
                    self.conditions.velocity_vector_m_s[point][0] =
                        blend::profile_speed(&profile, node, air_speed);
                    self.conditions.time_s[point] =
                        node * (final_time - initial_time) + initial_time;
                }
            }
        }
        Ok(())
    }

    /// The shared body of the two `Constant_Speed_Constant_Rate` initializers.
    ///
    /// The horizontal velocity is what is left of the airspeed once the
    /// vertical component is taken out of it, so a steeper climb at the same
    /// airspeed covers less ground.
    /// In [`SpeedReference::CalibratedAirspeed`] mode, `air_speed_m_s` is resolved
    /// to a true airspeed *at each control point's own altitude* (real ambient
    /// pressure and temperature, this segment's ISA deviation), so the true
    /// airspeed, the horizontal velocity and the along-track acceleration the
    /// residual solve sees vary continuously along the ramp.
    /// An `entry_speed_m_s` is changed along the path (see [`blend`]).
    fn lay_down_ramp(
        &mut self,
        nodes: &[f64],
        start_altitude_m: f64,
        end_altitude_m: f64,
        air_speed_m_s: f64,
        vertical_velocity_m_s: f64,
    ) -> Result<(), SegmentError> {
        let reference = self.spec.air_speed_reference;
        let temperature_deviation_k = self.spec.temperature_deviation_k;
        let tag = self.spec.tag.clone();
        let nominal_speed = |altitude: f64, point: usize| -> Result<f64, SegmentError> {
            if reference == SpeedReference::TrueAirspeed
                && self.spec.speed_caps == SpeedCaps::default()
            {
                return Ok(air_speed_m_s);
            }
            let atmosphere = alas_atmo::us1976_compute_values(altitude, temperature_deviation_k);
            self.spec
                .speed_caps
                .true_airspeed_m_s(
                    reference,
                    air_speed_m_s,
                    atmosphere.pressure_pa,
                    atmosphere.temperature_k,
                )
                .map_err(|source| SegmentError::InvalidCalibratedAirspeed {
                    tag: self.spec.tag.clone(),
                    point,
                    source,
                })
        };
        let ends = blend::PathEnds::new(
            [self.spec.entry_speed_m_s, self.spec.exit_speed_m_s],
            [start_altitude_m, end_altitude_m],
            [
                nominal_speed(start_altitude_m, 0)?,
                nominal_speed(end_altitude_m, nodes.len() - 1)?,
            ],
        );
        for (point, &node) in nodes.iter().enumerate() {
            let altitude = node * (end_altitude_m - start_altitude_m) + start_altitude_m;
            let nominal_m_s = nominal_speed(altitude, point)?;
            let true_air_speed_m_s = blend::ramp_speed(&ends, altitude, nominal_m_s);
            let horizontal_sq = true_air_speed_m_s * true_air_speed_m_s
                - vertical_velocity_m_s * vertical_velocity_m_s;
            if !horizontal_sq.is_finite() || horizontal_sq <= 0.0 {
                return Err(SegmentError::VerticalRateExceedsAirspeed {
                    tag: tag.clone(),
                    point,
                    vertical_velocity_m_s,
                    resolved_speed_m_s: true_air_speed_m_s,
                });
            }
            self.conditions.velocity_vector_m_s[point][0] = horizontal_sq.sqrt();
            self.conditions.velocity_vector_m_s[point][2] = vertical_velocity_m_s;
            self.conditions.position_vector_m[point][2] = -altitude;
            self.conditions.altitude_m[point] = altitude;
        }
        Ok(())
    }

    /// Whether this is a climb flown at the maximum-climb rating (see
    /// [`SegmentSpec::rating_limited`]): its vertical rate is settled on the rating.
    #[must_use]
    pub fn is_rating_limited(&self) -> bool {
        self.spec.rating_limited && matches!(self.spec.kind, SegmentKind::Climb { .. })
    }

    /// The vertical rate at each control point, m/s, positive up.
    #[must_use]
    pub fn vertical_rates_m_s(&self) -> Vec<f64> {
        self.conditions
            .velocity_vector_m_s
            .iter()
            .map(|velocity| -velocity[2])
            .collect()
    }

    /// The energy share of each control point: the fraction of the specific excess power that
    /// goes into climbing, `f = [1 + (V/g) dV/dh]^-1` (BADA 3.8 User Manual, Sec. 3.2,
    /// eq. 3.2-5). One where the speed is constant or the ratio is not defined.
    #[must_use]
    pub fn energy_shares(&self) -> Vec<f64> {
        let conditions = &self.conditions;
        let speeds = &conditions.velocity_m_s;
        let altitudes = &conditions.altitude_m;
        self.numerics
            .dimensionless
            .differentiate
            .iter()
            .enumerate()
            .map(|(point, row)| {
                let dv: f64 = row.iter().zip(speeds).map(|(d, v)| d * v).sum();
                let dh: f64 = row.iter().zip(altitudes).map(|(d, h)| d * h).sum();
                let gain = 1.0 + speeds[point] / conditions.gravity_m_s2[point] * dv / dh;
                if gain.is_finite() && gain > 0.0 {
                    1.0 / gain
                } else {
                    1.0
                }
            })
            .collect()
    }

    /// The specific excess power at each control point, m/s: the vertical rate over the energy
    /// share, the rate the aircraft would climb at holding its speed. A service ceiling is where
    /// this falls to the residual climb, whether or not the leg also accelerates.
    #[must_use]
    pub fn specific_excess_power_m_s(&self) -> Vec<f64> {
        self.vertical_rates_m_s()
            .iter()
            .zip(self.energy_shares())
            .map(|(rate, share)| rate / share)
            .collect()
    }

    /// Run one pass of the iterate chain and leave the residuals behind.
    ///
    /// The order is `Unknown_Throttle.__defaults__`'s, and it is load-bearing:
    /// the mass is integrated *after* the fuel flow that burns it and *before*
    /// the forces that weigh it, and the orientations are formed before the
    /// thrust that is rotated through them. `memo` answers a thrust or
    /// aerodynamics request it already had ([`crate::PointMemo`]).
    pub fn iterate_with(&mut self, memo: &mut crate::PointMemo<'_>) {
        let analyses = memo.analyses();
        // initials
        frames::initialize_time(&mut self.conditions, self.initials.as_ref());
        common::initialize_weights(
            &mut self.conditions,
            analyses.takeoff_mass_kg,
            self.initials.as_ref(),
        );
        frames::initialize_inertial_position(&mut self.conditions, self.initials.as_ref());
        frames::initialize_planet_position(&mut self.conditions, self.initials.as_ref());

        // unknowns
        let rating_limited = self.is_rating_limited();
        for point in 0..self.conditions.len() {
            if rating_limited {
                // The vertical rate at the point is the profile's (`solve` settles it on the
                // rating), taken out of the scheduled airspeed as `lay_down_ramp` does (z down).
                let velocity = self.conditions.velocity_vector_m_s[point];
                let speed = velocity[0].hypot(velocity[2]);
                let rate =
                    self.rate_profile_m_s[point].clamp(MIN_RATED_CLIMB_RATE_M_S, 0.99 * speed);
                self.conditions.velocity_vector_m_s[point][0] =
                    (speed * speed - rate * rate).sqrt();
                self.conditions.velocity_vector_m_s[point][2] = -rate;
            }
            self.conditions.throttle[point] = self.throttle[point];
            self.conditions.body_inertial_rotations_rad[point][1] = self.body_angle_rad[point];
        }
        if rating_limited {
            // The time at each point follows from the rates just set: `t_i = t_0 + sum_j
            // I_ij dz / v_z,j` over the dimensionless integration operator (the constant-rate
            // `update_differentials_altitude` spreads one span linearly, which a varying rate
            // does not).
            let conditions = &mut self.conditions;
            let points = conditions.len();
            let dz =
                conditions.position_vector_m[points - 1][2] - conditions.position_vector_m[0][2];
            let initial_time = conditions.time_s[0];
            for point in 0..points {
                let elapsed: f64 = self.numerics.dimensionless.integrate[point]
                    .iter()
                    .zip(&conditions.velocity_vector_m_s)
                    .map(|(&weight, velocity)| weight * dz / velocity[2])
                    .sum();
                conditions.time_s[point] = initial_time + elapsed;
            }
        }

        // conditions
        self.numerics
            .update_differentials_time(&self.conditions.time_s);
        if rating_limited {
            // Altitude is linear in the node and time is not, so the time operators follow the
            // chain rule: `dt/dtau = dz / v_z` at each point, `d/dt = (d/dtau) / (dt/dtau)` row
            // by row and `int dt = int (dt/dtau) dtau` column by column.
            let conditions = &self.conditions;
            let points = conditions.len();
            let dz =
                conditions.position_vector_m[points - 1][2] - conditions.position_vector_m[0][2];
            let dt_dtau: Vec<f64> = conditions
                .velocity_vector_m_s
                .iter()
                .map(|velocity| dz / velocity[2])
                .collect();
            let dimensionless = &self.numerics.dimensionless;
            for row in 0..points {
                for column in 0..points {
                    self.numerics.time.differentiate[row][column] =
                        dimensionless.differentiate[row][column] / dt_dtau[row];
                    self.numerics.time.integrate[row][column] =
                        dimensionless.integrate[row][column] * dt_dtau[column];
                }
            }
        }
        let differentiate = &self.numerics.time.differentiate;
        if rating_limited {
            frames::update_tangential_acceleration(&mut self.conditions, differentiate);
        } else if !matches!(self.spec.kind, SegmentKind::Cruise { .. })
            || !self.spec.speed_profile.is_empty()
        {
            frames::update_acceleration(&mut self.conditions, differentiate);
        }
        common::update_altitude(&mut self.conditions);
        let atmosphere = common::update_atmosphere(
            &mut self.conditions,
            analyses,
            self.spec.temperature_deviation_k,
        );
        common::update_gravity(&mut self.conditions);
        common::update_freestream(&mut self.conditions);
        frames::update_orientations(&mut self.conditions);
        common::update_thrust(
            &mut self.conditions,
            memo,
            &atmosphere,
            self.spec.takeoff_rated,
        );
        common::update_aerodynamics(&mut self.conditions, memo, self.spec.configuration_delta_cd);
        common::update_weights(&mut self.conditions, &self.numerics.time.integrate);
        frames::update_forces(&mut self.conditions, self.spec.load_factor);
        frames::update_planet_position(
            &mut self.conditions,
            &self.numerics.time.integrate,
            self.spec.true_course_rad,
        );

        self.update_residuals(analyses.signed_cruise_force_residual);
    }

    /// The force that did not balance, per unit mass.
    ///
    /// A climb or descent compares the `x` and `z` forces against the
    /// accelerations the trajectory implies. A cruise has no acceleration term
    /// (its chain never computed one) unless it is a level speed change, whose
    /// acceleration enters the signed residual on every path. Product analyses
    /// use the signed `x` component; the frozen compatibility path retains the
    /// historical horizontal-force magnitude residual for steady cruise.
    fn update_residuals(&mut self, signed_cruise_force_residual: bool) {
        for point in 0..self.conditions.len() {
            let force = self.conditions.total_force_vector_n[point];
            let mass = self.conditions.total_mass_kg[point];
            let acceleration = self.conditions.acceleration_vector_m_s2[point];

            self.residuals[point] = match self.spec.kind {
                SegmentKind::Cruise { .. } => [
                    if signed_cruise_force_residual || !self.spec.speed_profile.is_empty() {
                        force[0] / mass - acceleration[0]
                    } else {
                        (force[0] * force[0] + force[1] * force[1]).sqrt() / mass
                    },
                    force[2] / mass,
                ],
                _ => [
                    force[0] / mass - acceleration[0],
                    force[2] / mass - acceleration[2],
                ],
            };
        }
    }

    /// Everything after convergence: the ground track.
    ///
    /// `update_stability` runs here upstream and reports nothing this mission
    /// reads, `aero_derivatives` is `skip`, and `compute_noise` returns
    /// immediately because no noise analysis is attached.
    pub fn finalize(&mut self) {
        frames::integrate_inertial_horizontal_position(
            &mut self.conditions,
            &self.numerics.time.integrate,
            self.spec.true_course_rad,
        );
    }

    /// The propulsion deck's own statement, at each control point of the
    /// published trajectory, of whether it answered inside its evidence
    /// domain.
    ///
    /// The demand is the one [`common::update_thrust`] evaluates at the
    /// point: the published command as a normalized force, or as a share of
    /// the take-off rating on a take-off-rated leg. Nothing of the segment is
    /// changed: this reads the deck again at the flown points.
    pub fn propulsion_validity(
        &self,
        analyses: &MissionAnalyses,
    ) -> Vec<Result<alas_prop::system::ValidityStatus, alas_prop::system::PropulsionError>> {
        use alas_prop::system::{PropulsionDemand, PropulsionRating};
        let conditions = &self.conditions;
        let takeoff_rated = analyses.enforce_throttle_envelope && self.spec.takeoff_rated;
        (0..conditions.len())
            .map(|point| {
                let altitude_m = conditions.altitude_m[point];
                let atmosphere = analyses.atmosphere(altitude_m, self.spec.temperature_deviation_k);
                let commanded = conditions.throttle[point];
                let evaluated = if analyses.enforce_throttle_envelope {
                    commanded.clamp(0.0, 1.0)
                } else {
                    commanded
                };
                let demand = if takeoff_rated {
                    PropulsionDemand::RatedFraction {
                        rating: PropulsionRating::TakeoffGoAround,
                        fraction: evaluated,
                    }
                } else {
                    PropulsionDemand::NormalizedForce(evaluated)
                };
                analyses.propulsion_validity(
                    &atmosphere,
                    altitude_m,
                    conditions.velocity_m_s[point],
                    conditions.mach[point],
                    conditions.gravity_m_s2[point],
                    demand,
                )
            })
            .collect()
    }

    /// The unknowns as the vector the root finder searches over.
    ///
    /// `pack_array` ravels each `n x 1` array in Fortran order and
    /// concatenates them in the order `__defaults__` declared them, so this is
    /// every throttle followed by every body angle.
    pub fn pack_unknowns(&self) -> Vec<f64> {
        let mut packed = self.throttle.clone();
        packed.extend_from_slice(&self.body_angle_rad);
        packed
    }

    /// Read a search point back into the two unknown arrays.
    ///
    /// A vector of the wrong length is ignored rather than panicking; the
    /// solver is the only caller and it round-trips its own packing.
    pub fn unpack_unknowns(&mut self, packed: &[f64]) {
        let points = self.conditions.len();
        if packed.len() != 2 * points {
            return;
        }
        self.throttle.copy_from_slice(&packed[..points]);
        self.body_angle_rad.copy_from_slice(&packed[points..]);
    }

    /// The residuals as the vector the root finder drives to zero.
    ///
    /// `residuals.forces` is an `n x 2` array, and `pack_array` ravels it in
    /// Fortran order: every horizontal residual, then every vertical one.
    /// Interleaving them instead would give the solver the same information in
    /// a different order and a different Jacobian, and it would converge
    /// somewhere else.
    pub fn pack_residuals(&self) -> Vec<f64> {
        let mut packed: Vec<f64> = self.residuals.iter().map(|forces| forces[0]).collect();
        packed.extend(self.residuals.iter().map(|forces| forces[1]));
        packed
    }

    /// What the segment hands to the segment after it.
    pub fn initials_for_next(&self) -> Initials {
        let last = self.conditions.len().saturating_sub(1);
        Initials {
            time_s: self.conditions.time_s[last],
            total_mass_kg: self.conditions.total_mass_kg[last],
            position_vector_m: self.conditions.position_vector_m[last],
            aircraft_range_m: self.conditions.aircraft_range_m[last],
            latitude_deg: self.conditions.latitude_deg[last],
            longitude_deg: self.conditions.longitude_deg[last],
        }
    }
}

// A test constructs the segments it asserts on, so a failed expect is the
// assertion failing rather than a library invariant being broken.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn climb_spec() -> SegmentSpec {
        SegmentSpec {
            tag: "test_climb".to_owned(),
            kind: SegmentKind::Climb {
                altitude_start_m: Some(1000.0),
                altitude_end_m: 5000.0,
                climb_rate_m_s: 10.0,
            },
            air_speed_m_s: 150.0,
            air_speed_reference: SpeedReference::TrueAirspeed,
            speed_caps: alas_config::mission::SpeedCaps::default(),
            true_course_rad: 0.0,
            temperature_deviation_k: 0.0,
            number_control_points: 16,
            takeoff_rated: false,
            rating_limited: false,
            configuration_delta_cd: 0.0,
            load_factor: 1.0,
            entry_speed_m_s: 0.0,
            exit_speed_m_s: 0.0,
            speed_profile: Vec::new(),
        }
    }

    #[test]
    fn a_climb_is_discretized_between_its_two_altitudes() {
        let segment = Segment::new(climb_spec(), None).expect("a declared start altitude");
        let altitude = &segment.conditions.altitude_m;
        assert_eq!(altitude[0], 1000.0);
        assert_eq!(altitude[15], 5000.0);
        assert!(altitude.windows(2).all(|pair| pair[0] < pair[1]));
        // z points down.
        assert_eq!(segment.conditions.position_vector_m[0][2], -1000.0);
    }

    // The horizontal speed is not the airspeed: the climb rate comes out of
    // it. A port that set `v_x = air_speed` would fly the same profile in the
    // same time and cover more ground, which nothing but this would catch.
    #[test]
    fn the_climb_rate_is_taken_out_of_the_airspeed_not_added_to_it() {
        let segment = Segment::new(climb_spec(), None).expect("a declared start altitude");
        let velocity = segment.conditions.velocity_vector_m_s[0];
        assert_eq!(velocity[2], -10.0);
        let expected: f64 = 150.0_f64.mul_add(150.0, -100.0).sqrt();
        assert!((velocity[0] - expected).abs() < 1e-12);
        assert!(velocity[0] < 150.0);
    }

    // 4000 m at 10 m/s is 400 seconds, whatever quadrature is used to get
    // there: the check that `update_differentials_altitude` is contracting
    // `dz / vz` and not something adjacent to it.
    #[test]
    fn a_constant_rate_climb_takes_the_altitude_change_over_the_rate() {
        let segment = Segment::new(climb_spec(), None).expect("a declared start altitude");
        let span = segment.conditions.time_s[15] - segment.conditions.time_s[0];
        assert!((span - 400.0).abs() < 1e-9, "span was {span}");
    }

    #[test]
    fn a_constant_speed_cruise_of_zero_distance_has_zero_duration() {
        let spec = SegmentSpec {
            tag: "test_cruise".to_owned(),
            kind: SegmentKind::Cruise {
                altitude_m: Some(11_000.0),
                distance_m: 0.0,
            },
            air_speed_m_s: 250.0,
            ..climb_spec()
        };
        let segment = Segment::new(spec, None).expect("a declared altitude");
        assert!(segment.conditions.time_s.iter().all(|&time| time == 0.0));
        assert!(segment
            .conditions
            .velocity_vector_m_s
            .iter()
            .all(|velocity| velocity[0] == 250.0));
    }

    #[test]
    fn a_cruise_is_discretized_in_time_across_its_distance() {
        let spec = SegmentSpec {
            tag: "test_cruise".to_owned(),
            kind: SegmentKind::Cruise {
                altitude_m: Some(11_000.0),
                distance_m: 250_000.0,
            },
            air_speed_m_s: 250.0,
            ..climb_spec()
        };
        let segment = Segment::new(spec, None).expect("a declared altitude");
        assert!(segment
            .conditions
            .altitude_m
            .iter()
            .all(|&altitude| altitude == 11_000.0));
        assert_eq!(segment.conditions.time_s[0], 0.0);
        assert!((segment.conditions.time_s[15] - 1000.0).abs() < 1e-9);
        // No vertical velocity, so no vertical component to take out of the
        // airspeed either.
        assert_eq!(segment.conditions.velocity_vector_m_s[0][0], 250.0);
        assert_eq!(segment.conditions.velocity_vector_m_s[0][2], 0.0);
    }

    #[test]
    fn a_deferred_start_altitude_without_a_predecessor_is_an_error_not_a_panic() {
        let spec = SegmentSpec {
            kind: SegmentKind::Climb {
                altitude_start_m: None,
                altitude_end_m: 5000.0,
                climb_rate_m_s: 10.0,
            },
            ..climb_spec()
        };
        assert_eq!(
            Segment::new(spec, None).err(),
            Some(SegmentError::NoStartingAltitude {
                tag: "test_climb".to_owned()
            })
        );
    }

    #[test]
    fn a_deferred_start_altitude_is_taken_from_the_previous_segment() {
        let spec = SegmentSpec {
            kind: SegmentKind::Climb {
                altitude_start_m: None,
                altitude_end_m: 5000.0,
                climb_rate_m_s: 10.0,
            },
            ..climb_spec()
        };
        let initials = Initials {
            position_vector_m: [12_000.0, 0.0, -2500.0],
            ..Initials::default()
        };
        let segment = Segment::new(spec, Some(initials)).expect("an inherited start altitude");
        assert_eq!(segment.conditions.altitude_m[0], 2500.0);
    }

    // The two unknowns are packed one array after the other rather than
    // interleaved, and the two residuals likewise. Both orderings solve the
    // same physics and neither takes the same path, so the ordering is pinned.
    #[test]
    fn the_unknowns_and_residuals_pack_array_after_array() {
        let mut segment = Segment::new(climb_spec(), None).expect("a declared start altitude");
        let packed = segment.pack_unknowns();
        assert_eq!(packed.len(), 32);
        assert!(packed[..16].iter().all(|&value| value == INITIAL_THROTTLE));
        assert!(packed[16..]
            .iter()
            .all(|&value| value == INITIAL_CLIMB_BODY_ANGLE_RAD));

        let mut probe: Vec<f64> = (0..32).map(f64::from).collect();
        segment.unpack_unknowns(&probe);
        assert_eq!(segment.throttle[3], 3.0);
        assert_eq!(segment.body_angle_rad[0], 16.0);
        probe.truncate(31);
        segment.unpack_unknowns(&probe);
        assert_eq!(segment.throttle[3], 3.0, "a short vector is ignored");

        segment.residuals[0] = [1.0, 2.0];
        segment.residuals[1] = [3.0, 4.0];
        let residuals = segment.pack_residuals();
        assert_eq!(residuals[0], 1.0);
        assert_eq!(residuals[1], 3.0);
        assert_eq!(residuals[16], 2.0);
        assert_eq!(residuals[17], 4.0);
    }

    // A cruise segment starts its search from a different body angle than a
    // climb does. MINPACK's whole path is a function of where it starts, so
    // the two constants are not interchangeable.
    #[test]
    fn cruise_and_climb_start_their_search_from_different_body_angles() {
        let climb = Segment::new(climb_spec(), None).expect("a declared start altitude");
        let cruise_spec = SegmentSpec {
            kind: SegmentKind::Cruise {
                altitude_m: Some(11_000.0),
                distance_m: 250_000.0,
            },
            ..climb_spec()
        };
        let cruise = Segment::new(cruise_spec, None).expect("a declared altitude");
        assert_eq!(climb.body_angle_rad[0], INITIAL_CLIMB_BODY_ANGLE_RAD);
        assert_eq!(cruise.body_angle_rad[0], INITIAL_CRUISE_BODY_ANGLE_RAD);
        assert_ne!(climb.body_angle_rad[0], cruise.body_angle_rad[0]);
    }

    // A level speed change follows its profile table: the duration is the
    // distance over the mean speed of the table and the node speeds interpolate it.
    #[test]
    fn a_cruise_with_a_speed_profile_follows_the_table() {
        let spec = SegmentSpec {
            kind: SegmentKind::Cruise {
                altitude_m: Some(3_000.0),
                distance_m: 21_000.0,
            },
            air_speed_m_s: 80.0,
            // Fast at first, then settling onto 130 m/s: mean 115 m/s.
            speed_profile: vec![[0.0, 80.0], [0.5, 125.0], [1.0, 130.0]],
            ..climb_spec()
        };
        let segment = Segment::new(spec, None).expect("a declared altitude");
        let c = &segment.conditions;
        let last = c.len() - 1;
        assert!((c.velocity_vector_m_s[0][0] - 80.0).abs() < 1.0e-12);
        assert!((c.velocity_vector_m_s[last][0] - 130.0).abs() < 1.0e-12);
        // The duration is the one in which the segment's own quadrature of the
        // table flies the distance (close to the analytic mean of 115 m/s).
        let duration = c.time_s[last] - c.time_s[0];
        assert!((duration - 21_000.0 / 115.0).abs() < 0.01 * duration);
        let row = &segment.numerics.dimensionless.integrate[last];
        let flown: f64 = row
            .iter()
            .zip(&c.velocity_vector_m_s)
            .map(|(weight, velocity)| weight * velocity[0] * duration)
            .sum();
        assert!((flown - 21_000.0).abs() < 1.0e-6);
        for point in 0..c.len() {
            let fraction = (c.time_s[point] - c.time_s[0]) / duration;
            let expected =
                blend::profile_speed(&[[0.0, 80.0], [0.5, 125.0], [1.0, 130.0]], fraction, 0.0);
            assert!((c.velocity_vector_m_s[point][0] - expected).abs() < 1.0e-9);
        }
    }

    // Speed changes along a climb or descent follow BADA 3.8 eq. 3.2-6 with the
    // energy share factor: d(V^2/2)/dh = g (1 - f) / f.
    #[test]
    fn an_entry_speed_changes_along_the_path_by_the_energy_share_factor() {
        // Accelerating in a descent (f = 1.7) from 120 m/s towards 150 m/s.
        let spec = SegmentSpec {
            kind: SegmentKind::Descent {
                altitude_start_m: Some(6_000.0),
                altitude_end_m: 1_000.0,
                descent_rate_m_s: 8.0,
            },
            air_speed_m_s: 150.0,
            entry_speed_m_s: 120.0,
            ..climb_spec()
        };
        let segment = Segment::new(spec, None).expect("a declared start");
        let c = &segment.conditions;
        let speed = |i: usize| c.velocity_vector_m_s[i][0].hypot(c.velocity_vector_m_s[i][2]);
        let last = c.len() - 1;
        assert!(
            (speed(0) - 120.0).abs() < 1.0e-9,
            "starts at the entry speed"
        );
        assert!(
            (speed(last) - 150.0).abs() < 1.0e-9,
            "ends on its own schedule"
        );
        let g = alas_atmo::STANDARD_GRAVITY;
        for point in 0..=last {
            let along = (120.0_f64.powi(2)
                + 2.0 * g * (1.0 - 1.7) / 1.7 * (c.altitude_m[point] - 6_000.0))
                .sqrt();
            assert!(
                (speed(point) - along.min(150.0)).abs() < 1.0e-9,
                "point {point}"
            );
        }
        // Decelerating in a descent uses f = 0.3: much less altitude per m/s.
        let spec = SegmentSpec {
            kind: SegmentKind::Descent {
                altitude_start_m: Some(6_000.0),
                altitude_end_m: 1_000.0,
                descent_rate_m_s: 8.0,
            },
            air_speed_m_s: 120.0,
            entry_speed_m_s: 150.0,
            ..climb_spec()
        };
        let segment = Segment::new(spec, None).expect("a declared start");
        let c = &segment.conditions;
        let speed = |i: usize| c.velocity_vector_m_s[i][0].hypot(c.velocity_vector_m_s[i][2]);
        assert!((speed(0) - 150.0).abs() < 1.0e-9);
        assert!((speed(c.len() - 1) - 120.0).abs() < 1.0e-9);
    }

    #[test]
    fn product_cruise_residual_preserves_the_sign_of_a_thrust_deficit() {
        let spec = SegmentSpec {
            tag: "cruise_sign_probe".to_owned(),
            kind: SegmentKind::Cruise {
                altitude_m: Some(10_000.0),
                distance_m: 1_000.0,
            },
            air_speed_m_s: 250.0,
            air_speed_reference: SpeedReference::TrueAirspeed,
            speed_caps: alas_config::mission::SpeedCaps::default(),
            true_course_rad: 0.0,
            temperature_deviation_k: 0.0,
            number_control_points: 2,
            takeoff_rated: false,
            rating_limited: false,
            configuration_delta_cd: 0.0,
            load_factor: 1.0,
            entry_speed_m_s: 0.0,
            exit_speed_m_s: 0.0,
            speed_profile: Vec::new(),
        };
        let mut segment = Segment::new(spec, None).expect("declared cruise altitude");
        segment.conditions.total_force_vector_n[0] = [-100.0, 0.0, 0.0];
        segment.conditions.total_mass_kg[0] = 10.0;

        segment.update_residuals(true);
        assert_eq!(segment.residuals[0][0], -10.0);

        segment.update_residuals(false);
        assert_eq!(segment.residuals[0][0], 10.0);
    }

    /// International knot in m/s.
    const KNOT: f64 = 1852.0 / 3600.0;

    /// 170 KCAS (the ATR 72-600 factsheet climb speed) flown as a
    /// calibrated climb from a 610 m field (Madrid-Barajas' elevation) to
    /// 4000 m at 6 m/s.
    fn cas_climb_spec() -> SegmentSpec {
        SegmentSpec {
            tag: "cas_climb".to_owned(),
            kind: SegmentKind::Climb {
                altitude_start_m: Some(610.0),
                altitude_end_m: 4000.0,
                climb_rate_m_s: 6.0,
            },
            air_speed_m_s: 170.0 * KNOT,
            air_speed_reference: SpeedReference::CalibratedAirspeed,
            speed_caps: alas_config::mission::SpeedCaps::default(),
            true_course_rad: 0.0,
            temperature_deviation_k: 0.0,
            number_control_points: 16,
            takeoff_rated: false,
            rating_limited: false,
            configuration_delta_cd: 0.0,
            load_factor: 1.0,
            entry_speed_m_s: 0.0,
            exit_speed_m_s: 0.0,
            speed_profile: Vec::new(),
        }
    }

    fn true_air_speed_at(segment: &Segment, point: usize) -> f64 {
        let v = segment.conditions.velocity_vector_m_s[point];
        v[0].hypot(v[2])
    }

    // The configured number is a calibrated airspeed, and it has to be *the*
    // calibrated airspeed at every control point: recovering CAS from the
    // laid-down true airspeed and the node's own ambient state must give the
    // configured value back, while the true airspeed itself rises with
    // altitude. A port that resolved CAS once (at the top, bottom or middle)
    // and flew that constant would pass a "TAS is above CAS" check and fail
    // this one.
    #[test]
    fn a_calibrated_climb_holds_the_configured_cas_at_every_control_point() {
        let spec = cas_climb_spec();
        let cas = spec.air_speed_m_s;
        let segment = Segment::new(spec, None).expect("a valid subsonic calibrated climb");
        let mut previous_tas = 0.0;
        for point in 0..16 {
            let tas = true_air_speed_at(&segment, point);
            let altitude = segment.conditions.altitude_m[point];
            let atmosphere = alas_atmo::us1976_compute_values(altitude, 0.0);
            let recovered = alas_atmo::airspeed::calibrated_from_true(
                tas,
                atmosphere.pressure_pa,
                atmosphere.temperature_k,
            )
            .expect("a subsonic true airspeed has a calibrated airspeed");
            assert!(
                (recovered - cas).abs() <= 1.0e-9 * cas,
                "point {point} at {altitude} m: recovered {recovered} m/s CAS from {tas} m/s TAS, configured {cas}"
            );
            assert!(
                tas > previous_tas,
                "true airspeed did not rise with altitude at point {point}: {tas} <= {previous_tas}"
            );
            previous_tas = tas;
        }
        // The elevated field's first node is already faster than CAS: the
        // pressure at 610 m is below sea-level pressure.
        assert!(true_air_speed_at(&segment, 0) > cas);
        // Constant vertical rate throughout: only the horizontal component
        // carries the variation.
        assert!(segment
            .conditions
            .velocity_vector_m_s
            .iter()
            .all(|v| (v[2] + 6.0).abs() < 1.0e-12));
    }

    // The residual solve compares forces against the trajectory's
    // acceleration. A calibrated climb accelerates along track, and the
    // spectral acceleration has to be the derivative of the velocity actually
    // laid down: integrating it over the segment must return exactly the
    // change in horizontal velocity, and it must be positive at every node
    // (the true airspeed rises monotonically). The legacy true-airspeed climb
    // must keep its zero acceleration, or every recorded parity fixture would
    // move.
    #[test]
    fn a_calibrated_climb_carries_a_consistent_positive_along_track_acceleration() {
        let mut segment = Segment::new(cas_climb_spec(), None).expect("a valid calibrated climb");
        segment
            .numerics
            .update_differentials_time(&segment.conditions.time_s);
        frames::update_acceleration(
            &mut segment.conditions,
            &segment.numerics.time.differentiate,
        );
        let velocity = &segment.conditions.velocity_vector_m_s;
        let acceleration = &segment.conditions.acceleration_vector_m_s2;
        for (point, a) in acceleration.iter().enumerate() {
            assert!(
                a[0] > 0.0,
                "along-track acceleration at point {point} is {} m/s^2, expected positive",
                a[0]
            );
            assert!(
                a[2].abs() < 1.0e-9,
                "vertical acceleration at {point} is {}",
                a[2]
            );
        }
        let last_row = segment
            .numerics
            .time
            .integrate
            .last()
            .expect("an operator row");
        let integrated: f64 = last_row
            .iter()
            .zip(acceleration)
            .map(|(&weight, a)| weight * a[0])
            .sum();
        let delta_vx = velocity[15][0] - velocity[0][0];
        assert!(
            delta_vx > 0.0 && (integrated - delta_vx).abs() <= 1.0e-9 * delta_vx,
            "integrated acceleration {integrated} m/s does not return the velocity change {delta_vx} m/s"
        );
        // Order-of-magnitude sanity on the physics, not a fitted number: a
        // 170 KCAS climb at 6 m/s gains roughly 5% TAS per 1000 m, so the
        // along-track acceleration is a few hundredths of a m/s^2.
        assert!(acceleration.iter().all(|a| a[0] < 0.1));

        let mut legacy = Segment::new(climb_spec(), None).expect("a declared start altitude");
        legacy
            .numerics
            .update_differentials_time(&legacy.conditions.time_s);
        frames::update_acceleration(&mut legacy.conditions, &legacy.numerics.time.differentiate);
        assert!(legacy
            .conditions
            .acceleration_vector_m_s2
            .iter()
            .all(|a| a[0].abs() < 1.0e-9 && a[2].abs() < 1.0e-9));
    }

    // The ambient state the calibrated airspeed is resolved against is the
    // node's own: the field elevation (MSL, not "zero because it is the
    // ground") and the segment's ISA deviation both have to move the true
    // airspeed the right way.
    #[test]
    fn a_calibrated_speed_is_resolved_against_the_field_elevation_and_isa_deviation() {
        let elevated = Segment::new(cas_climb_spec(), None).expect("a valid calibrated climb");
        let sea_level_spec = SegmentSpec {
            kind: SegmentKind::Climb {
                altitude_start_m: Some(0.0),
                altitude_end_m: 4000.0,
                climb_rate_m_s: 6.0,
            },
            ..cas_climb_spec()
        };
        let sea_level = Segment::new(sea_level_spec, None).expect("a valid calibrated climb");
        let cas = cas_climb_spec().air_speed_m_s;
        // At sea level, standard day, CAS and TAS coincide by definition.
        assert!((true_air_speed_at(&sea_level, 0) - cas).abs() <= 1.0e-9 * cas);
        // At 610 m the same CAS is a faster true airspeed, and exactly the
        // one the shared conversion gives at that pressure and temperature.
        let ambient = alas_atmo::us1976_compute_values(610.0, 0.0);
        let expected = alas_atmo::airspeed::true_from_calibrated(
            cas,
            ambient.pressure_pa,
            ambient.temperature_k,
        )
        .expect("subsonic");
        let actual = true_air_speed_at(&elevated, 0);
        assert!((actual - expected).abs() <= 1.0e-9 * expected);
        assert!(actual > true_air_speed_at(&sea_level, 0));

        // A warmer day (positive ISA deviation) at the same field lowers the
        // density, so the same CAS is a faster TAS again.
        let warm_spec = SegmentSpec {
            temperature_deviation_k: 15.0,
            ..cas_climb_spec()
        };
        let warm = Segment::new(warm_spec, None).expect("a valid calibrated climb");
        assert!(true_air_speed_at(&warm, 0) > actual);
        let warm_ambient = alas_atmo::us1976_compute_values(610.0, 15.0);
        let warm_expected = alas_atmo::airspeed::true_from_calibrated(
            cas,
            warm_ambient.pressure_pa,
            warm_ambient.temperature_k,
        )
        .expect("subsonic");
        assert!((true_air_speed_at(&warm, 0) - warm_expected).abs() <= 1.0e-9 * warm_expected);
    }

    // A calibrated airspeed outside the conversion's validity domain is a
    // typed setup error naming the control point, not a NaN velocity that
    // the solver discovers later; and a resolved true airspeed that does not
    // exceed the vertical rate is the same typed error a true-airspeed ramp
    // raises.
    #[test]
    fn an_unresolvable_calibrated_speed_is_rejected_at_setup() {
        let supersonic = SegmentSpec {
            kind: SegmentKind::Climb {
                altitude_start_m: Some(9000.0),
                altitude_end_m: 12_000.0,
                climb_rate_m_s: 5.0,
            },
            air_speed_m_s: 330.0,
            ..cas_climb_spec()
        };
        match Segment::new(supersonic, None) {
            Err(SegmentError::InvalidCalibratedAirspeed { tag, .. }) => {
                assert_eq!(tag, "cas_climb");
            }
            other => panic!("a 330 m/s CAS climb to 12 km must be rejected, got {other:?}"),
        }
        let not_a_number = SegmentSpec {
            air_speed_m_s: f64::NAN,
            ..cas_climb_spec()
        };
        assert!(matches!(
            Segment::new(not_a_number, None),
            Err(SegmentError::InvalidCalibratedAirspeed { .. })
        ));
        let too_steep = SegmentSpec {
            kind: SegmentKind::Climb {
                altitude_start_m: Some(0.0),
                altitude_end_m: 1000.0,
                climb_rate_m_s: 60.0,
            },
            air_speed_m_s: 50.0,
            ..cas_climb_spec()
        };
        assert!(matches!(
            Segment::new(too_steep, None),
            Err(SegmentError::VerticalRateExceedsAirspeed { point: 0, .. })
        ));
    }

    // A zero, negative or non-finite climb or descent rate is a typed error,
    // not an infinite or NaN segment time.
    #[test]
    fn a_non_positive_vertical_rate_is_rejected() {
        for rate in [0.0, -5.0, f64::NAN, f64::INFINITY] {
            let climb = SegmentSpec {
                kind: SegmentKind::Climb {
                    altitude_start_m: Some(0.0),
                    altitude_end_m: 1000.0,
                    climb_rate_m_s: rate,
                },
                air_speed_m_s: 100.0,
                air_speed_reference: SpeedReference::TrueAirspeed,
                speed_caps: alas_config::mission::SpeedCaps::default(),
                ..cas_climb_spec()
            };
            assert!(
                matches!(
                    Segment::new(climb, None),
                    Err(SegmentError::NonPositiveVerticalRate { kind: "climb", .. })
                ),
                "climb rate {rate}"
            );
        }
        let descent = SegmentSpec {
            kind: SegmentKind::Descent {
                altitude_start_m: Some(1000.0),
                altitude_end_m: 0.0,
                descent_rate_m_s: 0.0,
            },
            air_speed_m_s: 100.0,
            air_speed_reference: SpeedReference::TrueAirspeed,
            speed_caps: alas_config::mission::SpeedCaps::default(),
            ..cas_climb_spec()
        };
        assert!(matches!(
            Segment::new(descent, None),
            Err(SegmentError::NonPositiveVerticalRate {
                kind: "descent",
                ..
            })
        ));
    }
}
