// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from native aerodynamic model/dynamics/flight_dynamics/airplane.py, `get_modes`.
// Upstream: native aerodynamic model 4.2.8, MIT.

//! native aerodynamic model's `flight_dynamics.airplane.get_modes`: the closed-form
//! small-perturbation eigenmode approximations (phugoid, short-period, roll
//! subsidence, dutch roll, spiral) from a stability-derivative set, the
//! reference dimensions, the flight condition and an inertia estimate.
//!
//! # Closed-form, given the derivatives
//!
//! Every formula here is `f64` arithmetic over its inputs: Flight Vehicle
//! Aerodynamics Eqs. 9.55-9.68, transcribed, except the dutch roll (below).
//! It runs no solve of its own. The
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
//! and roll roots. The phugoid, spiral and roll roots are reproduced
//! faithfully; `docs/PORTING.md` carries the `deviation-candidate` (replace
//! with a state-space eigensolve in P14).
//!
//! # Dutch roll: the two-degree-of-freedom approximation, not upstream's
//!
//! The dutch roll is the pair of roots of the sideslip/yaw-rate approximation
//! (rolling freedom suppressed): Nelson, *Flight Stability and Automatic
//! Control*, 2nd ed., chapter 5, "Dutch roll approximation". Stability axes
//! (x along the trim velocity, z down), small perturbations about steady
//! level flight, side-force and yawing-moment equations with `p = phi = 0`:
//!
//! ```text
//! d(beta)/dt = (Y_beta / u0) beta - (1 - Y_r / u0) r
//! d(r)/dt    =  N_beta beta + N_r r
//! ```
//!
//! with the dimensional derivatives (`Q` the dynamic pressure; coefficients
//! per radian and per `r b / (2 u0)`):
//!
//! ```text
//! Y_beta = Q S C_Ybeta / m            [m/s^2]
//! Y_r    = Q S b C_Yr / (2 m u0)      [m/s]
//! N_beta = Q S b C_nbeta / I_zz       [1/s^2]
//! N_r    = Q S b^2 C_nr / (2 I_zz u0) [1/s]
//! ```
//!
//! The characteristic equation is
//! `lambda^2 - (Y_beta/u0 + N_r) lambda + N_beta + (Y_beta N_r - N_beta Y_r)/u0 = 0`,
//! so `sigma = (Y_beta/u0 + N_r) / 2`,
//! `omega_n^2 = N_beta + (Y_beta N_r - N_beta Y_r)/u0` and the damped
//! frequency is `sqrt(omega_n^2 - sigma^2)`. Upstream evaluates
//! `N_r + Y_beta/(2 u0)` for the real part (the yaw-damping term not halved)
//! and reports `omega_n` as the imaginary part; that pair is not a root of
//! the quadratic. Validity: the approximation neglects the dihedral effect,
//! so it over-predicts the damping when `|C_lbeta|` is large against
//! `C_nbeta`; it is a handling-qualities estimate, not an eigensolve.

use alas_aero::operating_point::OperatingPoint;
use alas_geom::aircraft::airplane::Airplane;

/// Standard gravity, m/s^2 (9.80665, the program-wide constant). Upstream's
/// `get_modes` defaults to the two-decimal `g=9.81`; only the phugoid
/// frequency term reads it.
const GRAVITY: f64 = alas_atmo::STANDARD_GRAVITY;

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

    // Dutch roll: roots of the sideslip/yaw-rate quadratic (module doc).
    // `sigma = (N_r + Y_beta/u0) / 2`, 1/s; `omega_n^2`, 1/s^2, is
    // `N_beta + (Y_beta N_r - N_beta Y_r)/u0` with `Q/u0^2 = density/2`.
    let n_r = qs * (b * b) / (2.0 * izz * u0) * aero.cnr;
    let y_beta_over_u0 = qs / (m * u0) * aero.cyb;
    let dutch_roll_sigma = 0.5 * (n_r + y_beta_over_u0);
    let dutch_roll_omega_n_squared = qs * b / izz
        * (aero.cnb + (density * s * b / (4.0 * m) * (aero.cyb * aero.cnr - aero.cnb * aero.cyr)));
    let (dutch_roll_real, dutch_roll_imag) = mode_info(
        dutch_roll_sigma,
        dutch_roll_omega_n_squared - dutch_roll_sigma * dutch_roll_sigma,
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

    /// A twin-jet transport at low speed, round numbers: reference area
    /// 120 m^2, span 34 m, mass 60 000 kg, `I_zz` 3.8e6 kg.m^2, 100 m/s at
    /// sea level.
    fn hand_case(cnb: f64) -> Modes {
        let airplane = Airplane {
            name: "hand case".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: Vec::new(),
            fuselages: Vec::new(),
            s_ref: 120.0,
            c_ref: 4.0,
            b_ref: 34.0,
        };
        let op_point = OperatingPoint::new(
            alas_atmo::Atmosphere::new(0.0),
            100.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        let mass = MassProperties {
            mass: 60_000.0,
            ixx: 1.2e6,
            iyy: 3.0e6,
            izz: 3.8e6,
        };
        let aero = StabilityAero {
            cl: 0.8,
            cd: 0.06,
            cma: -1.5,
            cmq: -20.0,
            clp: -0.5,
            cyb: -0.9,
            cnb,
            cyr: 0.5,
            cnr: -0.2,
            clb: -0.15,
            clr: 0.2,
        };
        get_modes(&airplane, &op_point, &mass, &aero)
    }

    /// The four dimensional derivatives of the hand case, formed here from
    /// their definitions, independently of `get_modes`:
    /// `(Y_beta/u0, Y_r/u0, N_beta, N_r)`.
    fn hand_case_derivatives(cnb: f64) -> (f64, f64, f64, f64) {
        let (s, b, m, izz, u0) = (120.0, 34.0, 60_000.0, 3.8e6, 100.0);
        let q = 0.5 * alas_atmo::Atmosphere::new(0.0).density() * u0 * u0;
        (
            q * s * -0.9 / m / u0,
            q * s * b * 0.5 / (2.0 * m * u0) / u0,
            q * s * b * cnb / izz,
            q * s * b * b * -0.2 / (2.0 * izz * u0),
        )
    }

    #[test]
    fn dutch_roll_is_the_root_pair_of_the_sideslip_yaw_quadratic() {
        let (y_beta, y_r, n_beta, n_r) = hand_case_derivatives(0.12);
        let modes = hand_case(0.12);
        let (real, imag) = (
            modes.dutch_roll.eigenvalue_real,
            modes.dutch_roll.eigenvalue_imag,
        );
        assert!(imag > 0.0, "the hand case is oscillatory");

        // Half the trace of [[Y_beta/u0, -(1 - Y_r/u0)], [N_beta, N_r]]: both
        // damping terms are halved.
        let trace = y_beta + n_r;
        assert!((real - 0.5 * trace).abs() < 1e-12);
        // The determinant is omega_n^2, and |lambda|^2 of a complex pair is
        // the determinant: the imaginary part is the damped frequency.
        let determinant = y_beta * n_r + (1.0 - y_r) * n_beta;
        assert!((real * real + imag * imag - determinant).abs() < 1e-12);
        // lambda satisfies the characteristic equation (real and imaginary
        // parts of lambda^2 - trace lambda + determinant).
        assert!((real * real - imag * imag - trace * real + determinant).abs() < 1e-12);
        assert!((2.0 * real * imag - trace * imag).abs() < 1e-12);
        // zeta = -trace / (2 omega_n).
        let zeta = -trace / (2.0 * determinant.sqrt());
        assert!((modes.dutch_roll.damping_ratio - zeta).abs() < 1e-12);
    }

    #[test]
    fn aperiodic_dutch_roll_reports_the_least_stable_real_root() {
        // Directionally unstable: the determinant is negative, the roots are
        // real and the larger one is trace/2 + sqrt(trace^2/4 - determinant).
        let (y_beta, y_r, n_beta, n_r) = hand_case_derivatives(-0.05);
        let modes = hand_case(-0.05);
        let trace = y_beta + n_r;
        let determinant = y_beta * n_r + (1.0 - y_r) * n_beta;
        assert!(determinant < 0.0);
        let root = 0.5 * trace + (0.25 * trace * trace - determinant).sqrt();
        assert_eq!(modes.dutch_roll.eigenvalue_imag, 0.0);
        assert!((modes.dutch_roll.eigenvalue_real - root).abs() < 1e-12);
        assert!(root > 0.0);
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
