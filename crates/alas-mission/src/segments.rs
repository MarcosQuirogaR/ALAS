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
//! trajectory is *given* -- a climb is sixteen points evenly spaced in
//! altitude between two altitudes, flown at a fixed true airspeed and a fixed
//! rate; a cruise is sixteen points evenly spaced in time across a fixed
//! distance at a fixed altitude -- and what is unknown is how the aeroplane
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
//! runs, is that a cruise segment's iterate chain omits `update_acceleration`
//! and forms its horizontal residual from the *magnitude* of the horizontal
//! force rather than from its `x` component; both are reproduced.
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
    /// True airspeed, m/s.
    pub air_speed_m_s: f64,
    /// Course over the ground, radians. Zero on every segment this program
    /// builds, and carried because the ground-track integral projects onto it.
    pub true_course_rad: f64,
    /// Deviation from the standard atmosphere, K. Zero on every segment: the
    /// departure airport's ISA deviation is set on the *airport*, which only
    /// a ground segment reads.
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
    /// `initialize_conditions`, then -- for a climb or descent only --
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
                self.lay_down_ramp(&nodes, start, altitude_end_m, air_speed, -climb_rate_m_s);
                self.update_differentials_altitude();
            }
            SegmentKind::Descent {
                altitude_start_m,
                altitude_end_m,
                descent_rate_m_s,
            } => {
                let start = self.starting_altitude(altitude_start_m)?;
                self.lay_down_ramp(&nodes, start, altitude_end_m, air_speed, descent_rate_m_s);
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
    fn lay_down_ramp(
        &mut self,
        nodes: &[f64],
        start_altitude_m: f64,
        end_altitude_m: f64,
        air_speed_m_s: f64,
        vertical_velocity_m_s: f64,
    ) {
        let horizontal =
            (air_speed_m_s * air_speed_m_s - vertical_velocity_m_s * vertical_velocity_m_s).sqrt();
        for (point, &node) in nodes.iter().enumerate() {
            let altitude = node * (end_altitude_m - start_altitude_m) + start_altitude_m;
            self.conditions.velocity_vector_m_s[point][0] = horizontal;
            self.conditions.velocity_vector_m_s[point][2] = vertical_velocity_m_s;
            self.conditions.position_vector_m[point][2] = -altitude;
            self.conditions.altitude_m[point] = altitude;
        }
    }

    /// `update_differentials_altitude`: how long a ramp discretized in
    /// altitude takes.
    ///
    /// The overall time is the dimensionless integration operator's last row,
    /// scaled by the altitude change, contracted against the reciprocal of the
    /// vertical velocity -- a quadrature of `dz / vz` -- and the node grid is
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

        self.update_residuals();
    }

    /// The force that did not balance, per unit mass.
    ///
    /// A climb or descent compares the `x` and `z` forces against the
    /// accelerations the trajectory implies. A cruise has no acceleration term
    /// -- its chain never computed one -- and takes the *magnitude* of the
    /// horizontal force instead of its `x` component, which makes its
    /// horizontal residual one-sided: a thrust deficit and a thrust surplus
    /// both read positive.
    fn update_residuals(&mut self) {
        for point in 0..self.conditions.len() {
            let force = self.conditions.total_force_vector_n[point];
            let mass = self.conditions.total_mass_kg[point];
            let acceleration = self.conditions.acceleration_vector_m_s2[point];

            self.residuals[point] = match self.spec.kind {
                SegmentKind::Cruise { .. } => [
                    (force[0] * force[0] + force[1] * force[1]).sqrt() / mass,
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
}
