// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wing-only aerodynamic entry over the in-process vortex-lattice solver.
//!
//! This module adds no aerodynamic model. It selects a subset of the surfaces
//! [`alas_geom::builder::AircraftBuilder`] already lofts, states the reference
//! quantities and the moment reference explicitly, and drives
//! [`crate::vlm`] over them. Nothing here reads a mission, a payload, a mass
//! breakdown or a trim state, which is what makes a wing analysis possible
//! without a whole-aircraft run.
//!
//! # Frames, signs and units
//!
//! Geometry axes are the frame every coordinate in this module is expressed
//! in: `+x` aft (downstream) from the aircraft geometric datum, `+y` to
//! starboard, `+z` up. Body axes are `+x` forward, `+y` starboard, `+z` down;
//! geometry to body is the 180-degree rotation about `y` that
//! [`crate::operating_point::OperatingPoint::convert_axes`] applies. Wind axes
//! put `+x` along the freestream, so drag is `+F_x` and lift is `-F_z` there.
//!
//! Angle of attack is positive nose-up (freestream arriving from below).
//! The pitching moment is taken about body `y` through the stated moment
//! reference and is positive nose-up; the rolling moment is positive
//! right-wing-down and the yawing moment positive nose-right. Coefficients
//! use the wing reference area, the wing reference span and the wing mean
//! aerodynamic chord, all reported in [`WingReference`].
//!
//! Every quantity is SI: metres, square metres, newtons, newton-metres,
//! kilograms per cubic metre, metres per second, pascals. Angles crossing
//! this interface are degrees; derivatives are per radian and are named so.
//!
//! # What the model is, and what it omits
//!
//! The solver is the incompressible, inviscid thin-surface vortex lattice.
//! Its drag is the near-field induced (vortex) drag of the modelled surfaces
//! alone: no parasite drag, no viscous or profile drag, no wave drag, no
//! Prandtl-Glauert or transonic correction. Mach number is recorded and used
//! to derive true airspeed, never to scale a coefficient. Fuselage, nacelles,
//! pylons, interference, propulsion and control-surface deflections are not
//! represented; the empennage option adds the horizontal and vertical tail
//! exactly as the geometry builder lofts them, with no downwash correction
//! beyond what the lattice itself produces. A result is therefore a statement
//! about the modelled wing (or wing-empennage) configuration, not about an
//! aircraft.

#[path = "wing_analysis_solve.rs"]
mod solve;
#[path = "wing_analysis_types.rs"]
mod types;

pub use solve::{analyse, build_wing_model, WingModel};
pub(crate) use types::{diagnostics, operating_point};
pub use types::{
    AlphaPoint, ResolvedCondition, SpanStation, StabilityOutcome, SurfaceShare, WingAnalysisError,
    WingAnalysisOutcome, WingReference, WingSolveDiagnostics, OMITTED_COMPONENTS,
};

#[cfg(test)]
#[path = "wing_analysis_tests.rs"]
mod tests;

/// Highest angle of attack this entry accepts, in degrees.
///
/// The lattice is a linear, attached-flow model: it returns a number at any
/// angle, and that number stops meaning anything once the section stalls.
/// The bound keeps the interface inside the range where the model is a model.
pub const ALPHA_LIMIT_DEG: f64 = 20.0;

/// Which represented surfaces enter the modelled configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceSet {
    /// The main wing alone.
    WingOnly,
    /// The main wing with the horizontal and vertical tail as lofted.
    WingAndEmpennage,
}

impl SurfaceSet {
    /// Whether the empennage surfaces are part of the configuration.
    pub fn includes_empennage(self) -> bool {
        matches!(self, Self::WingAndEmpennage)
    }
}

/// How the freestream speed is stated by the caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpeedInput {
    /// True airspeed, m/s.
    TrueAirspeed(f64),
    /// Freestream Mach number, from which true airspeed is derived with the
    /// speed of sound of the stated atmosphere.
    Mach(f64),
}

/// How the attitude is stated by the caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AttitudeInput {
    /// Geometric angle of attack, degrees, positive nose-up.
    AngleOfAttack(f64),
    /// Target lift coefficient on the wing reference area; the angle of
    /// attack that reaches it is solved for and reported.
    LiftCoefficient(f64),
}

/// The explicit flight condition one analysis is run at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlightCondition {
    /// Geopotential altitude, m, feeding the standard-atmosphere state.
    pub altitude_m: f64,
    /// Freestream speed statement.
    pub speed: SpeedInput,
    /// Attitude statement.
    pub attitude: AttitudeInput,
}

impl Default for FlightCondition {
    fn default() -> Self {
        Self {
            altitude_m: 10_000.0,
            speed: SpeedInput::Mach(0.5),
            attitude: AttitudeInput::AngleOfAttack(2.0),
        }
    }
}

/// The angle-of-attack sweep the polar and the moment curve are built on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlphaSweep {
    /// First angle, degrees.
    pub min_deg: f64,
    /// Last angle, degrees.
    pub max_deg: f64,
    /// Number of evaluated angles, endpoints included.
    pub points: usize,
}

impl Default for AlphaSweep {
    fn default() -> Self {
        Self {
            min_deg: -4.0,
            max_deg: 10.0,
            points: 8,
        }
    }
}

impl AlphaSweep {
    /// The exact angles this sweep evaluates, in order.
    pub fn values(&self) -> Vec<f64> {
        if self.points <= 1 {
            return vec![self.min_deg];
        }
        let step = (self.max_deg - self.min_deg) / (self.points - 1) as f64;
        (0..self.points)
            .map(|index| self.min_deg + step * index as f64)
            .collect()
    }
}

/// Everything one wing analysis run is defined by, apart from the geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingAnalysisInputs {
    /// Modelled surface set.
    pub surfaces: SurfaceSet,
    /// Flight condition.
    pub condition: FlightCondition,
    /// Moment reference point in geometry axes, m; the point moments and
    /// stability derivatives are taken about, normally the stated CG.
    pub moment_reference_m: [f64; 3],
    /// Spanwise panel multiplier on each surface's own subdivision.
    pub spanwise_resolution: usize,
    /// Chordwise panels per strip.
    pub chordwise_resolution: usize,
    /// Angle-of-attack sweep for the polar and moment curve.
    pub sweep: AlphaSweep,
}

impl Default for WingAnalysisInputs {
    fn default() -> Self {
        Self {
            surfaces: SurfaceSet::WingOnly,
            condition: FlightCondition::default(),
            moment_reference_m: [0.0, 0.0, 0.0],
            spanwise_resolution: 1,
            chordwise_resolution: 8,
            sweep: AlphaSweep::default(),
        }
    }
}

impl WingAnalysisInputs {
    /// Every reason this run cannot be submitted, in reading order.
    ///
    /// The bounds are the validity domain of the entry, not of the airframe:
    /// an angle outside the attached-flow range, a speed outside the
    /// atmosphere model, or a panel count the lattice cannot condition.
    pub fn validate(&self) -> Vec<String> {
        let mut findings = Vec::new();
        let condition = &self.condition;
        if !condition.altitude_m.is_finite() || !(-500.0..=25_000.0).contains(&condition.altitude_m)
        {
            findings.push("Altitude must be between -500 m and 25000 m.".to_owned());
        }
        match condition.speed {
            SpeedInput::TrueAirspeed(value) => {
                if !value.is_finite() || !(1.0..=400.0).contains(&value) {
                    findings.push("True airspeed must be between 1 m/s and 400 m/s.".to_owned());
                }
            }
            SpeedInput::Mach(value) => {
                if !value.is_finite() || !(0.01..=0.95).contains(&value) {
                    findings.push("Mach number must be between 0.01 and 0.95.".to_owned());
                }
            }
        }
        match condition.attitude {
            AttitudeInput::AngleOfAttack(value) => {
                if !value.is_finite() || value.abs() > ALPHA_LIMIT_DEG {
                    findings
                        .push("Angle of attack must be within plus or minus 20 deg.".to_owned());
                }
            }
            AttitudeInput::LiftCoefficient(value) => {
                if !value.is_finite() || !(-1.5..=2.5).contains(&value) {
                    findings
                        .push("Target lift coefficient must be between -1.5 and 2.5.".to_owned());
                }
            }
        }
        if !self
            .moment_reference_m
            .iter()
            .all(|value| value.is_finite())
        {
            findings.push("The moment reference point must be finite.".to_owned());
        }
        if !(1..=2).contains(&self.spanwise_resolution) {
            findings.push("Spanwise panel multiplier must be 1 or 2.".to_owned());
        }
        if !(1..=16).contains(&self.chordwise_resolution) {
            findings.push("Chordwise panel count must be between 1 and 16.".to_owned());
        }
        if !(2..=41).contains(&self.sweep.points) {
            findings.push("The alpha sweep must have between 2 and 41 points.".to_owned());
        }
        if !self.sweep.min_deg.is_finite()
            || !self.sweep.max_deg.is_finite()
            || self.sweep.min_deg >= self.sweep.max_deg
            || self.sweep.min_deg.abs() > ALPHA_LIMIT_DEG
            || self.sweep.max_deg.abs() > ALPHA_LIMIT_DEG
        {
            findings.push(
                "The alpha sweep must be an increasing range within plus or minus 20 deg."
                    .to_owned(),
            );
        }
        findings
    }
}
