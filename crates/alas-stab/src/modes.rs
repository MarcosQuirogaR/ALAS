// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from native aerodynamic model/dynamics/flight_dynamics/airplane.py, `get_modes`.
// Upstream: native aerodynamic model 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! native aerodynamic model's `flight_dynamics.airplane.get_modes`: the closed-form
//! small-perturbation eigenmode approximations (phugoid, short-period, roll
//! subsidence, dutch roll, spiral) from a stability-derivative set, the
//! reference dimensions, the flight condition and an inertia estimate.
//!
//! # Closed-form, given the derivatives
//!
//! Every formula here is `f64` arithmetic over its inputs: Flight Vehicle
//! Aerodynamics Eqs. 9.55-9.68, transcribed. It runs no solve of its own. The
//! [`StabilityAero`] set it consumes *is* produced by a vortex-lattice solve
//! upstream (`alas-aero::vlm`'s [`run_with_stability_derivatives`], through
//! [`crate::dynamics::compute_dynamic_modes`]), but that solve is not part of
//! this function: `get_modes` takes the derivatives as data, so this module is
//! checked at the `closed` tier against fixed derivative sets, independent of
//! where they came from. `docs/PORTING.md` records the split.
//!
//! [`run_with_stability_derivatives`]: alas_aero::vlm::run_with_stability_derivatives
//!
//! # Scope
//!
//! [`StabilityAero`] carries exactly the eleven coefficients `get_modes` reads
//! off the aero dict (`CL`, `CD`, `Cma`, `Cmq`, `Clp`, `CYb`, `Cnb`, `CYr`,
//! `Cnr`, `Clb`, `Clr`): the sweep produces thirty derivatives, and this is
//! the subset the eigenmode formulas touch. The phugoid's
//! `eigenvalue_imag_approx`/`damping_ratio_approx` byproducts are not
//! translated: `compute_dynamic_modes`, the only consumer, reads only the
//! eigenvalue and damping-ratio of each mode.
//!
//! # A recorded upstream quirk, not corrected here
//!
//! The author's own reference eigenvalues (in the module's `__main__`) note
//! that this closed-form set has a factor-of-two error in the phugoid real
//! part against AVL, and disagreements in dutch-roll frequency and the spiral
//! and roll roots. Reproduced faithfully; `docs/PORTING.md` carries the
//! `deviation-candidate` (replace with a state-space eigensolve in P14).

use alas_aero::operating_point::OperatingPoint;
use alas_geom::aircraft::airplane::Airplane;

/// Standard gravity, m/s^2: `get_modes`'s `g=9.81` default, which no call
/// site overrides.
const GRAVITY: f64 = 9.81;

/// The subset of a `run_with_stability_derivatives` result `get_modes` reads:
/// the two base coefficients and the nine stability derivatives the eigenmode
/// formulas use. All derivatives are per-radian / per-nondimensional-rate, as
/// native aerodynamic model's sweep reports them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StabilityAero {
    /// Lift coefficient at the base point: `CL`.
    pub cl: f64,
    /// Drag coefficient at the base point: `CD`.
    pub cd: f64,
    /// Pitching-moment slope with angle of attack: `Cma`.
    pub cma: f64,
    /// Pitching-moment slope with nondimensional pitch rate: `Cmq`.
    pub cmq: f64,
    /// Rolling-moment slope with nondimensional roll rate: `Clp`.
    pub clp: f64,
    /// Side-force slope with sideslip: `CYb`.
    pub cyb: f64,
    /// Yawing-moment slope with sideslip: `Cnb`.
    pub cnb: f64,
    /// Side-force slope with nondimensional yaw rate: `CYr`.
    pub cyr: f64,
    /// Yawing-moment slope with nondimensional yaw rate: `Cnr`.
    pub cnr: f64,
    /// Rolling-moment slope with sideslip: `Clb`.
    pub clb: f64,
    /// Rolling-moment slope with nondimensional yaw rate: `Clr`.
    pub clr: f64,
}

/// The mass and principal moments of inertia `get_modes` reads off a
/// `MassProperties`: upstream's full object, narrowed to the four fields the
/// eigenmode formulas touch (`mass`, `Ixx`, `Iyy`, `Izz`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassProperties {
    /// Total mass, kg.
    pub mass: f64,
    /// Roll moment of inertia about the body X axis, kg.m^2.
    pub ixx: f64,
    /// Pitch moment of inertia about the body Y axis, kg.m^2.
    pub iyy: f64,
    /// Yaw moment of inertia about the body Z axis, kg.m^2.
    pub izz: f64,
}

/// One eigenmode as `get_modes` reports it: the complex eigenvalue split into
/// real and imaginary parts, and the damping ratio the final loop derives from
/// them. An aperiodic mode carries a zero imaginary part.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    /// Real part of the eigenvalue, 1/s. Negative is stable.
    pub eigenvalue_real: f64,
    /// Imaginary part of the eigenvalue, rad/s. Zero for an aperiodic mode.
    pub eigenvalue_imag: f64,
    /// Damping ratio `-real / sqrt(real^2 + imag^2)`.
    pub damping_ratio: f64,
}

/// The five classical longitudinal and lateral-directional modes, in the
/// order `get_modes` builds them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Modes {
    /// The slow longitudinal exchange of speed and altitude.
    pub phugoid: Mode,
    /// The fast longitudinal angle-of-attack oscillation.
    pub short_period: Mode,
    /// The aperiodic roll damping.
    pub roll_subsidence: Mode,
    /// The lateral-directional oscillation.
    pub dutch_roll: Mode,
    /// The slow aperiodic bank/heading divergence or convergence.
    pub spiral: Mode,
}

/// `get_mode_info(sigma, omega_squared)`: split a second-order characteristic
/// into an eigenvalue. When `omega_squared > 0` the mode is oscillatory
/// (`real = sigma`, `imag = sqrt(omega_squared)`); otherwise it is aperiodic
/// and the root adds to the real part.
fn mode_info(sigma: f64, omega_squared: f64) -> (f64, f64) {
    // Upstream's `np.abs(omega_squared + 1e-100) ** 0.5`; the 1e-100 keeps the
    // root's derivative finite at zero and is negligible to the value.
    let root = (omega_squared + 1e-100).abs().sqrt();
    if omega_squared > 0.0 {
        (sigma, root)
    } else {
        (sigma + root, 0.0)
    }
}

/// The damping ratio `-real / sqrt(real^2 + imag^2)` `get_modes`'s final loop
/// assigns every mode, including the roll and spiral roots, whose earlier
/// literal damping is overwritten here.
fn damping_ratio(real: f64, imag: f64) -> f64 {
    -real / (real * real + imag * imag).sqrt()
}

/// The five small-perturbation eigenmodes at `op_point`: `get_modes`.
///
/// `aero` is the stability-derivative set (upstream's `aero` dict), `mass` the
/// inertia estimate (upstream's `MassProperties`). Reference dimensions come
/// from `airplane`, and the flight condition (dynamic pressure, airspeed,
/// density) from `op_point`.
pub fn get_modes(
    airplane: &Airplane,
    op_point: &OperatingPoint,
    mass: &MassProperties,
    aero: &StabilityAero,
) -> Modes {
    let q = op_point.dynamic_pressure();
    let s = airplane.s_ref;
    let c = airplane.c_ref;
    let b = airplane.b_ref;
    let qs = q * s;
    let m = mass.mass;
    let ixx = mass.ixx;
    let iyy = mass.iyy;
    let izz = mass.izz;
    let u0 = op_point.velocity;
    let density = op_point.atmosphere.density();
    let g = GRAVITY;

    let cxu = -2.0 * aero.cd;
    let czu = -2.0 * aero.cl;

    let x_u = qs / m / u0 * cxu;
    let z_u = qs / m / u0 * czu;
    let m_w = qs * c / iyy / u0 * aero.cma;
    let m_q = qs * c / iyy * c / (2.0 * u0) * aero.cmq;

    // Longitudinal modes.
    let (phugoid_real, phugoid_imag) = mode_info(x_u / 2.0, -(x_u * x_u) / 4.0 - g * z_u / u0);
    let (short_period_real, short_period_imag) =
        mode_info(0.5 * m_q, -(m_q * m_q) / 4.0 - u0 * m_w);

    // Lateral modes. `(b * b)` is upstream's `b**2`, grouped as one factor.
    let roll_real = qs * (b * b) / (2.0 * ixx * u0) * aero.clp;
    let roll_imag = 0.0;

    let (dutch_roll_real, dutch_roll_imag) = mode_info(
        qs * (b * b) / (2.0 * izz * u0) * (aero.cnr + izz / (m * (b * b)) * aero.cyb),
        qs * b / izz
            * (aero.cnb
                + (density * s * b / (4.0 * m) * (aero.cyb * aero.cnr - aero.cnb * aero.cyr))),
    );

    let spiral_parameter = aero.cnr - aero.cnb * aero.clr / aero.clb;
    let spiral_real = qs * (b * b) / (2.0 * izz * u0) * spiral_parameter;
    let spiral_imag = 0.0;

    Modes {
        phugoid: Mode {
            eigenvalue_real: phugoid_real,
            eigenvalue_imag: phugoid_imag,
            damping_ratio: damping_ratio(phugoid_real, phugoid_imag),
        },
        short_period: Mode {
            eigenvalue_real: short_period_real,
            eigenvalue_imag: short_period_imag,
            damping_ratio: damping_ratio(short_period_real, short_period_imag),
        },
        roll_subsidence: Mode {
            eigenvalue_real: roll_real,
            eigenvalue_imag: roll_imag,
            damping_ratio: damping_ratio(roll_real, roll_imag),
        },
        dutch_roll: Mode {
            eigenvalue_real: dutch_roll_real,
            eigenvalue_imag: dutch_roll_imag,
            damping_ratio: damping_ratio(dutch_roll_real, dutch_roll_imag),
        },
        spiral: Mode {
            eigenvalue_real: spiral_real,
            eigenvalue_imag: spiral_imag,
            damping_ratio: damping_ratio(spiral_real, spiral_imag),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_info_is_oscillatory_when_omega_squared_is_positive() {
        // The root becomes the imaginary part; the real part is sigma alone.
        let (real, imag) = mode_info(-0.3, 4.0);
        assert!((real - -0.3).abs() < 1e-12);
        assert!((imag - 2.0).abs() < 1e-12);
    }

    #[test]
    fn mode_info_folds_the_root_into_the_real_part_when_aperiodic() {
        // omega_squared <= 0: no oscillation, the root adds to the real part.
        let (real, imag) = mode_info(-0.3, -4.0);
        assert!((real - (-0.3 + 2.0)).abs() < 1e-12);
        assert_eq!(imag, 0.0);
    }

    #[test]
    fn damping_ratio_is_plus_or_minus_one_for_a_purely_real_root() {
        // A stable aperiodic root damps to +1, an unstable one to -1: the
        // value get_modes's final loop assigns, overwriting the literal 1 the
        // roll mode is first given.
        assert!((damping_ratio(-0.5, 0.0) - 1.0).abs() < 1e-12);
        assert!((damping_ratio(0.5, 0.0) - -1.0).abs() < 1e-12);
    }
}
