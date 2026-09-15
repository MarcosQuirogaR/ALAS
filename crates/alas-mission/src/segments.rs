// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Analyses/Mission/Segments/Climb/Unknown_Throttle.py,
// mission analysis model/Analyses/Mission/Segments/Cruise/Constant_Speed_Constant_Altitude.py,
// the two Constant_Speed_Constant_Rate segments, and the methods they name in
// mission analysis model/Methods/Missions/Segments/{Climb,Cruise}/Common.py.
// Upstream: mission analysis model 2.5.2, LGPL-2.1 (relicensed under GPL-2.0-or-later per
// LGPL-2.1 section 3; compatible with this program's AGPL-3.0-or-later).
// Reference: alas @ rust-port-baseline.

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
//! The one structural difference between them, beyond which `initialize`
//! runs, is that a cruise segment's iterate chain omits `update_acceleration`.
//! The frozen compatibility path also retains the historical horizontal-force
//! magnitude residual, while product missions use the signed longitudinal
//! component so the root finder can distinguish thrust surplus from deficit.
//!
//! Untranslated because unreachable: `Energy.initialize_battery` (there is no
//! battery behind a turbofan), `Weights.update_weights`' additional-fuel
//! branch (a turbofan network reports no additional fuel rate),
//! `Noise.compute_noise` (no noise analysis is attached, so it returns
//! immediately), `aero_derivatives` (`mission analysis model.Methods.skip`) and
//! `update_stability` (it runs, and reports nothing this mission reads).

pub mod analyses;
pub mod common;
pub mod conditions;
pub mod frames;

pub use analyses::{AeroSolution, LegacyTurbofanCompatibility, MissionAnalyses};
pub use conditions::{Conditions, Initials, Matrix3, Vector3};

use crate::numerics::Numerics;
use alas_config::mission::SpeedReference;
use alas_math::ChebyshevError;

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
}

/// Why a segment could not be set up.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SegmentError {
    /// The pseudospectral operators could not be built.
    #[error("the discretization operators could not be built: {0}")]
    Discretization(#[from] ChebyshevError),
    /// The segment's starting altitude was left to the previous segment and
    /// there is no previous segment. Upstream raises an `AttributeError`
    /// here; a library here reports rather than panicking.
    #[error("segment {tag} takes its starting altitude from a predecessor it does not have")]
    NoStartingAltitude {
        /// The segment that could not start.
        tag: String,
    },
    /// A calibrated airspeed and a control point's real ambient state implied
    /// an invalid or supersonic condition.
    #[error("segment {tag} calibrated airspeed at control point {point}: {source}")]
    InvalidCalibratedAirspeed {
        /// The segment that could not resolve its airspeed.
        tag: String,
        /// Which control point failed.
        point: usize,
        /// Why.
        source: alas_atmo::airspeed::AirspeedError,
    },
    /// A resolved airspeed at a control point did not exceed the segment's
    /// vertical rate, so no real horizontal velocity exists there. Checked
    /// per control point in calibrated-airspeed mode because true airspeed
    /// varies along the ramp; a true-airspeed segment is checked once, before
    /// construction, by its caller.
    #[error(
        "segment {tag} vertical rate {vertical_velocity_m_s} m/s at or above the {resolved_speed_m_s} m/s airspeed resolved at control point {point}"
    )]
    VerticalRateExceedsAirspeed {
        /// The segment that could not fly its ramp.
        tag: String,
        /// Which control point failed.
        point: usize,
        /// The configured vertical rate, m/s.
        vertical_velocity_m_s: f64,
        /// The true airspeed resolved at that control point, m/s.
        resolved_speed_m_s: f64,
    },
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
}

/// The throttle every segment's unknowns start at, `ones_row(1) * 0.5`.
const INITIAL_THROTTLE: f64 = 0.5;

/// The body angle a climb or descent segment's unknowns start at, radians:
/// `ones_row(1) * 3.0 * Units.degrees`.
const INITIAL_CLIMB_BODY_ANGLE_RAD: f64 = 3.0 * std::f64::consts::PI / 180.0;

/// The body angle a cruise segment's unknowns start at, radians:
/// `ones_row(1) * 1.0 * Units.deg`. A different number from the climb's, and
/// the starting point is what MINPACK's whole path is a function of.
const INITIAL_CRUISE_BODY_ANGLE_RAD: f64 = std::f64::consts::PI / 180.0;

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
    /// and [`SegmentError::NoStartingAltitude`] if the segment defers its
    /// starting altitude to a predecessor it does not have.
    pub fn new(spec: SegmentSpec, initials: Option<Initials>) -> Result<Self, SegmentError> {
        let points = spec.number_control_points;
        let mut numerics = Numerics {
            number_control_points: i64::try_from(points).unwrap_or(i64::MAX),
            ..Numerics::default()
        };
        numerics.initialize_differentials_dimensionless()?;

        let initial_body_angle = match spec.kind {
            SegmentKind::Cruise { .. } => INITIAL_CRUISE_BODY_ANGLE_RAD,
            _ => INITIAL_CLIMB_BODY_ANGLE_RAD,
        };

        let mut segment = Self {
            spec,
            numerics,
            conditions: Conditions::expanded(points),
            initials,
            throttle: vec![INITIAL_THROTTLE; points],
            body_angle_rad: vec![initial_body_angle; points],
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
                let start = self.starting_altitude(altitude_start_m)?;
                // z points down, so a *climb* rate is a negative z velocity.
                self.lay_down_ramp(&nodes, start, altitude_end_m, air_speed, -climb_rate_m_s)?;
                self.update_differentials_altitude();
            }
            SegmentKind::Descent {
                altitude_start_m,
                altitude_end_m,
                descent_rate_m_s,
            } => {
                let start = self.starting_altitude(altitude_start_m)?;
                self.lay_down_ramp(&nodes, start, altitude_end_m, air_speed, descent_rate_m_s)?;
                self.update_differentials_altitude();
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
                let final_time = distance_m / air_speed + initial_time;
                for (point, &node) in nodes.iter().enumerate() {
                    self.conditions.altitude_m[point] = altitude;
                    self.conditions.position_vector_m[point][2] = -altitude;
                    self.conditions.velocity_vector_m_s[point][0] = air_speed;
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
    ///
    /// In [`SpeedReference::CalibratedAirspeed`] mode, `air_speed_m_s` is
    /// resolved to a true airspeed independently *at each control point's own
    /// altitude*, against the real ambient pressure and temperature there
    /// ([`alas_atmo::us1976_compute_values`] at this segment's own ISA
    /// deviation), so the true airspeed, and with it the horizontal
    /// velocity and, through [`crate::segments::frames::update_acceleration`]
    /// downstream, the along-track acceleration the residual solve sees,
    /// varies continuously along the ramp instead of being one constant
    /// value for the whole segment. This is the genuine per-live-altitude
    /// resolution the mission-scope integration calls for, not a
    /// several-constant-sub-segment approximation of it.
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
        for (point, &node) in nodes.iter().enumerate() {
            let altitude = node * (end_altitude_m - start_altitude_m) + start_altitude_m;
            let true_air_speed_m_s = match reference {
                SpeedReference::TrueAirspeed => air_speed_m_s,
                SpeedReference::CalibratedAirspeed => {
                    let atmosphere =
                        alas_atmo::us1976_compute_values(altitude, temperature_deviation_k);
                    alas_atmo::airspeed::true_from_calibrated(
                        air_speed_m_s,
                        atmosphere.pressure_pa,
                        atmosphere.temperature_k,
                    )
                    .map_err(|source| {
                        SegmentError::InvalidCalibratedAirspeed {
                            tag: tag.clone(),
                            point,
                            source,
                        }
                    })?
                }
            };
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

    /// `update_differentials_altitude`: how long a ramp discretized in
    /// altitude takes.
    ///
    /// The overall time is the dimensionless integration operator's last row,
    /// scaled by the altitude change, contracted against the reciprocal of the
    /// vertical velocity (a quadrature of `dz / vz`) and the node grid is
    /// then that time. Written exactly as upstream contracts it, because the
    /// factor `dz` is folded into the operator row before the contraction
    /// rather than after.
    fn update_differentials_altitude(&mut self) {
        let points = self.conditions.len();
        if points == 0 {
            return;
        }
        let dz = self.conditions.position_vector_m[points - 1][2]
            - self.conditions.position_vector_m[0][2];
        let last_row = &self.numerics.dimensionless.integrate[points - 1];
        let span: f64 = last_row
            .iter()
            .zip(&self.conditions.velocity_vector_m_s)
            .map(|(&weight, velocity)| (weight * dz) * (1.0 / velocity[2]))
            .sum();

        let initial_time = self.conditions.time_s[0];
        for (point, &node) in self
            .numerics
            .dimensionless
            .control_points
            .clone()
            .iter()
            .enumerate()
        {
            self.conditions.time_s[point] = initial_time + node * span;
        }
    }

    /// Run one pass of the iterate chain and leave the residuals behind.
    ///
    /// The order is `Unknown_Throttle.__defaults__`'s, and it is load-bearing:
    /// the mass is integrated *after* the fuel flow that burns it and *before*
    /// the forces that weigh it, and the orientations are formed before the
    /// thrust that is rotated through them.
    pub fn iterate(&mut self, analyses: &MissionAnalyses) {
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
        for point in 0..self.conditions.len() {
            self.conditions.throttle[point] = self.throttle[point];
            self.conditions.body_inertial_rotations_rad[point][1] = self.body_angle_rad[point];
        }

        // conditions
        self.numerics
            .update_differentials_time(&self.conditions.time_s);
        if !matches!(self.spec.kind, SegmentKind::Cruise { .. }) {
            frames::update_acceleration(&mut self.conditions, &self.numerics.time.differentiate);
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
        common::update_thrust(&mut self.conditions, analyses, &atmosphere);
        common::update_aerodynamics(&mut self.conditions, analyses);
        common::update_weights(&mut self.conditions, &self.numerics.time.integrate);
        frames::update_forces(&mut self.conditions);
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
    /// accelerations the trajectory implies. A cruise has no acceleration term:
    /// its chain never computed one. Product analyses use the signed `x`
    /// component; the frozen compatibility path retains the historical
    /// horizontal-force magnitude residual.
    fn update_residuals(&mut self, signed_cruise_force_residual: bool) {
        for point in 0..self.conditions.len() {
            let force = self.conditions.total_force_vector_n[point];
            let mass = self.conditions.total_mass_kg[point];
            let acceleration = self.conditions.acceleration_vector_m_s2[point];

            self.residuals[point] = match self.spec.kind {
                SegmentKind::Cruise { .. } => [
                    if signed_cruise_force_residual {
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
            true_course_rad: 0.0,
            temperature_deviation_k: 0.0,
            number_control_points: 16,
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
            true_course_rad: 0.0,
            temperature_deviation_k: 0.0,
            number_control_points: 2,
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
            true_course_rad: 0.0,
            temperature_deviation_k: 0.0,
            number_control_points: 16,
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
}
