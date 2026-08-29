// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Analyses/Mission/Segments/Conditions/{Basic,Aerodynamics}.py
// and the `expand_rows` machinery in Conditions.py/State.py.
// Upstream: mission analysis model 2.5.2, LGPL-2.1 (relicensed under GPL-2.0-or-later per
// LGPL-2.1 section 3; compatible with this program's AGPL-3.0-or-later).
// Reference: alas @ rust-port-baseline.

//! Everything a mission segment knows about itself at each control point.
//!
//! Upstream this is a nested dictionary of `n x 1` and `n x 3` NumPy arrays
//! that every update method reaches into by name. Here it is a struct of
//! parallel vectors, one entry per control point, which is the same data laid
//! out so that a field that does not exist is a compile error rather than a
//! key that silently reads back zeros.
//!
//! **The container is persistent state, not a return value.** Three of the
//! four `initials` methods and `update_weights` all read what the *previous*
//! iteration left behind -- `initialize_time` shifts the existing time array
//! rather than rebuilding it, and `update_weights` writes rows 1 onward while
//! deliberately leaving row 0 alone. A segment solve is a sequence of
//! iterations over one of these, and rebuilding it per iteration would quietly
//! change what the solver is solving.
//!
//! Scope. Only the fields the reached process chain writes or reads are
//! carried. `Conditions.Aerodynamics()` also declares battery, noise-source,
//! energy and aero-derivative bags: nothing in `Unknown_Throttle`'s or
//! `Constant_Speed_Constant_Altitude`'s chain touches any of them (the battery
//! initializer is unreachable behind a turbofan network, `compute_noise` finds
//! no noise analysis attached, and `aero_derivatives` is `mission analysis model.Methods.skip`),
//! so they are absent rather than carried as vectors of zeros. Likewise the
//! stability bag: `update_stability` runs and
//! `mission analysis model.Analyses.Stability.Fidelity_Zero` reports nothing the mission reads.

use alas_aero::drag_buildup::DragBreakdown;
use alas_aero::lift_surrogate::SurrogateDomainStatus;
use alas_prop::mission_turbofan::ThrustOutput;

/// A three-component vector in one of the mission's frames.
pub type Vector3 = [f64; 3];

/// A direction cosine matrix, row-major.
pub type Matrix3 = [[f64; 3]; 3];

/// The state of one segment at every control point.
///
/// Every vector has [`Conditions::len`] entries. They start at zero, which is
/// what `expand_rows` leaves them at, and are filled in by the segment's
/// `initialize` and then rewritten by each pass of its `iterate`.
#[derive(Debug, Clone, PartialEq)]
pub struct Conditions {
    // -- frames.inertial --------------------------------------------------
    /// Time at each control point, seconds. Absolute along the mission, not
    /// relative to the segment: `initialize_time` shifts it onto the end of
    /// the previous segment.
    pub time_s: Vec<f64>,
    /// Position in the inertial frame, metres. `z` points *down*, so altitude
    /// is its negation.
    pub position_vector_m: Vec<Vector3>,
    /// Inertial velocity, m/s. Also `z`-down.
    pub velocity_vector_m_s: Vec<Vector3>,
    /// Inertial acceleration, m/s^2. Stays zero in a cruise segment, whose
    /// iterate chain omits `update_acceleration`.
    pub acceleration_vector_m_s2: Vec<Vector3>,
    /// Weight as a force in the inertial frame, N.
    pub gravity_force_vector_n: Vec<Vector3>,
    /// Lift + drag + thrust + weight, in the inertial frame, N.
    pub total_force_vector_n: Vec<Vector3>,
    /// Ground distance flown since the start of the mission, metres.
    pub aircraft_range_m: Vec<f64>,

    // -- frames.body ------------------------------------------------------
    /// Body Euler angles relative to the inertial frame, radians, as
    /// `[roll, pitch, yaw]`. Only the pitch is ever set: it is one of the two
    /// unknowns.
    pub body_inertial_rotations_rad: Vec<Vector3>,
    /// The body-to-inertial direction cosine matrix.
    pub transform_body_to_inertial: Vec<Matrix3>,
    /// Thrust in the body frame, N, along `x`.
    pub thrust_force_vector_n: Vec<Vector3>,

    // -- frames.wind ------------------------------------------------------
    /// Lift in the wind frame, N, along `-z`.
    pub wind_lift_force_vector_n: Vec<Vector3>,
    /// Drag in the wind frame, N, along `-x`.
    pub wind_drag_force_vector_n: Vec<Vector3>,
    /// The wind-to-inertial transform, as `update_orientations` forms it.
    pub transform_wind_to_inertial: Vec<Matrix3>,

    // -- frames.planet ----------------------------------------------------
    /// Latitude. Upstream integrates a rate in radians per second and then
    /// divides by the degree factor before adding it to a value in radians,
    /// so this quantity is not in a single unit; see
    /// [`super::frames::update_planet_position`].
    pub latitude_deg: Vec<f64>,
    /// Longitude, carrying the same mixed units as the latitude.
    pub longitude_deg: Vec<f64>,

    // -- freestream -------------------------------------------------------
    /// Geometric altitude, metres.
    pub altitude_m: Vec<f64>,
    /// Static pressure, Pa.
    pub pressure_pa: Vec<f64>,
    /// Static temperature, K.
    pub temperature_k: Vec<f64>,
    /// Density, kg/m^3.
    pub density_kg_m3: Vec<f64>,
    /// Speed of sound, m/s.
    pub speed_of_sound_m_s: Vec<f64>,
    /// Dynamic viscosity, kg/(m*s).
    pub dynamic_viscosity_pa_s: Vec<f64>,
    /// Gravitational acceleration at altitude, m/s^2.
    pub gravity_m_s2: Vec<f64>,
    /// Speed, m/s: the magnitude of the inertial velocity.
    pub velocity_m_s: Vec<f64>,
    /// Mach number.
    pub mach: Vec<f64>,
    /// Reynolds number *per metre*, as every consumer of it expects.
    pub reynolds_number_per_m: Vec<f64>,
    /// Dynamic pressure, Pa.
    pub dynamic_pressure_pa: Vec<f64>,

    // -- aerodynamics -----------------------------------------------------
    /// Angle of attack, radians.
    pub angle_of_attack_rad: Vec<f64>,
    /// Side-slip angle, radians. Zero throughout a planar mission, carried
    /// because `update_orientations` computes and packs it.
    pub side_slip_angle_rad: Vec<f64>,
    /// Roll angle, radians. Likewise zero and likewise packed.
    pub roll_angle_rad: Vec<f64>,
    /// Aircraft lift coefficient, *after* the fuselage correction.
    pub lift_coefficient: Vec<f64>,
    /// Aircraft drag coefficient: `drag_breakdown.total`.
    pub drag_coefficient: Vec<f64>,
    /// The whole drag buildup at each point, so a mission that disagrees can
    /// be read back to the component that caused it.
    pub drag_breakdown: Vec<DragBreakdown>,
    /// Each wing's lift coefficient from the surrogate, per control point.
    pub wing_lift_coefficient: Vec<Vec<f64>>,
    /// Each wing's inviscid induced drag coefficient, likewise.
    pub wing_induced_drag_coefficient: Vec<Vec<f64>>,
    /// Surrogate training-domain status for each aerodynamic evaluation.
    ///
    /// The mission compatibility path still uses the reference edge clamp,
    /// but records its distance to the trained rectangle so reporting and
    /// product policy code can reject or label those points explicitly.
    pub surrogate_domain: Vec<SurrogateDomainStatus>,

    // -- propulsion -------------------------------------------------------
    /// Throttle: one of the two unknowns.
    pub throttle: Vec<f64>,
    /// The whole `thrust.outputs` bag at each point.
    pub thrust: Vec<ThrustOutput>,

    // -- weights ----------------------------------------------------------
    /// Vehicle mass, kg, falling through the segment as fuel burns.
    pub total_mass_kg: Vec<f64>,
    /// Fuel flow, kg/s, as a positive rate of mass *loss*.
    pub vehicle_mass_rate_kg_s: Vec<f64>,
}

impl Conditions {
    /// A container sized for `points` control points, every array zeroed.
    ///
    /// This is `expand_rows`: upstream resizes each declared `1 x m` array to
    /// `n x m` once, at `expand_state`, before any segment is evaluated.
    pub fn expanded(points: usize) -> Self {
        Self {
            time_s: vec![0.0; points],
            position_vector_m: vec![[0.0; 3]; points],
            velocity_vector_m_s: vec![[0.0; 3]; points],
            acceleration_vector_m_s2: vec![[0.0; 3]; points],
            gravity_force_vector_n: vec![[0.0; 3]; points],
            total_force_vector_n: vec![[0.0; 3]; points],
            aircraft_range_m: vec![0.0; points],
            body_inertial_rotations_rad: vec![[0.0; 3]; points],
            transform_body_to_inertial: vec![[[0.0; 3]; 3]; points],
            thrust_force_vector_n: vec![[0.0; 3]; points],
            wind_lift_force_vector_n: vec![[0.0; 3]; points],
            wind_drag_force_vector_n: vec![[0.0; 3]; points],
            transform_wind_to_inertial: vec![[[0.0; 3]; 3]; points],
            latitude_deg: vec![0.0; points],
            longitude_deg: vec![0.0; points],
            altitude_m: vec![0.0; points],
            pressure_pa: vec![0.0; points],
            temperature_k: vec![0.0; points],
            density_kg_m3: vec![0.0; points],
            speed_of_sound_m_s: vec![0.0; points],
            dynamic_viscosity_pa_s: vec![0.0; points],
            gravity_m_s2: vec![0.0; points],
            velocity_m_s: vec![0.0; points],
            mach: vec![0.0; points],
            reynolds_number_per_m: vec![0.0; points],
            dynamic_pressure_pa: vec![0.0; points],
            angle_of_attack_rad: vec![0.0; points],
            side_slip_angle_rad: vec![0.0; points],
            roll_angle_rad: vec![0.0; points],
            lift_coefficient: vec![0.0; points],
            drag_coefficient: vec![0.0; points],
            drag_breakdown: Vec::new(),
            wing_lift_coefficient: vec![Vec::new(); points],
            wing_induced_drag_coefficient: vec![Vec::new(); points],
            surrogate_domain: vec![SurrogateDomainStatus::default(); points],
            throttle: vec![0.0; points],
            thrust: Vec::new(),
            total_mass_kg: vec![0.0; points],
            vehicle_mass_rate_kg_s: vec![0.0; points],
        }
    }

    /// How many control points this segment carries.
    pub fn len(&self) -> usize {
        self.time_s.len()
    }

    /// Whether the container has no control points.
    pub fn is_empty(&self) -> bool {
        self.time_s.is_empty()
    }
}

/// What a segment inherits from the segment flown before it.
///
/// Upstream a segment holds a live reference to the previous segment's whole
/// `State`, and the four `initials` methods each read one *last row* out of
/// it. Those five numbers are the entire coupling between two segments, so
/// they are what is carried here -- which is also what makes one segment
/// reproducible from a fixture without re-solving every segment before it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Initials {
    /// Time at the end of the previous segment, seconds.
    pub time_s: f64,
    /// Vehicle mass at the end of the previous segment, kg.
    pub total_mass_kg: f64,
    /// Inertial position at the end of the previous segment, metres.
    pub position_vector_m: Vector3,
    /// Ground distance flown at the end of the previous segment, metres.
    pub aircraft_range_m: f64,
    /// Latitude at the end of the previous segment.
    pub latitude_deg: f64,
    /// Longitude at the end of the previous segment.
    pub longitude_deg: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_expanded_container_is_zeroed_at_the_requested_length() {
        let conditions = Conditions::expanded(16);
        assert_eq!(conditions.len(), 16);
        assert!(!conditions.is_empty());
        assert!(conditions.altitude_m.iter().all(|&value| value == 0.0));
        assert_eq!(conditions.position_vector_m[3], [0.0; 3]);
        // The two analysis-output vectors are filled by the analyses rather
        // than sized here: nothing reads them before `update_aerodynamics`
        // and `update_thrust` have written them.
        assert!(conditions.drag_breakdown.is_empty());
        assert!(conditions.thrust.is_empty());
    }

    #[test]
    fn a_zero_point_container_reports_itself_empty() {
        let conditions = Conditions::expanded(0);
        assert!(conditions.is_empty());
        assert_eq!(conditions.len(), 0);
    }
}
