// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/dynamics.py
// Reference: alas @ rust-port-baseline.

//! Longitudinal and lateral-directional dynamic-mode analysis: the two inputs
//! `alas/physics/dynamics.py` supplies to native aerodynamic model's eigenmode solve: a
//! gross-geometry inertia estimate and the vortex-lattice stability-derivative
//! set, and the small dataclass it wraps the result in.
//!
//! Distinct from [`crate::trim`] (static trim/CG/neutral-point): this answers
//! "how does the aircraft respond over time to a disturbance", through the
//! classical small-perturbation eigenmodes (phugoid, short-period, dutch roll,
//! roll subsidence, spiral).
//!
//! # Two computations, one tier
//!
//! [`estimate_inertia`] is closed-form geometry: radii of gyration off the
//! span and fuselage length, times the mass. It touches no solve and agrees
//! far tighter than the row's tier.
//!
//! [`compute_dynamic_modes`] runs a fresh
//! [`vlm::run_with_stability_derivatives`] sweep on the built aircraft and
//! hands the result to [`crate::modes::get_modes`]. Every derivative it feeds
//! `get_modes` is a forward difference of two dense VLM AIC solves, so the
//! eigenvalues it reports inherit that solve's tier: `linalg`, the same
//! construction `alas-stab::trim` and `alas-aero::vlm` carry, and the
//! finite differencing amplifies the sub-`linalg` LAPACK-vs-Gaussian residual
//! besides. The row is a single `linalg`, as `alas-stab::trim` is: half of it
//! is closed-form and could bear a tighter tier, but adding that second tier
//! is a ledger decision, and the ledger keeps this row single. `get_modes`'s
//! own closed-form-ness is checked at `closed` in `alas-stab::modes`, on fixed
//! derivative sets, precisely so this end-to-end path is the only place the
//! solve enters. `docs/PORTING.md` records it.
//!
//! # Resolution
//!
//! Upstream constructs the sweep at `spanwise_resolution=1` and leaves
//! `chordwise_resolution` at the `VortexLatticeMethod` default of 10: a
//! deliberate choice its own docstring benchmarks (spanwise 1 vs 4 moves the
//! phugoid/short-period eigenvalues under 1% and dutch-roll ~12%, at a tenth
//! the wall time). This port makes the same call.

use std::f64::consts::PI;

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::{self, VlmError};
use alas_geom::aircraft::airplane::Airplane;

use crate::modes::{self, MassProperties, StabilityAero};

/// The radius-of-gyration fraction of span for roll inertia: `rx = 0.25 b`.
const RX_SPAN_FRACTION: f64 = 0.25;
/// The radius-of-gyration fraction of fuselage length for pitch inertia:
/// `ry = 0.38 L`.
const RY_LENGTH_FRACTION: f64 = 0.38;
/// The radius-of-gyration fraction of fuselage length for yaw inertia:
/// `rz = 0.40 L`.
const RZ_LENGTH_FRACTION: f64 = 0.40;

/// The `chordwise_resolution` the `VortexLatticeMethod` constructor defaults
/// to and which `compute_dynamic_modes` leaves unset; it sets only
/// `spanwise_resolution=1`. See the module doc for why that resolution is a
/// deliberate benchmarked choice.
const CHORDWISE_RESOLUTION: usize = 10;

/// A radius-of-gyration estimate of `(Ixx, Iyy, Izz)` in kg.m^2:
/// `estimate_inertia`.
///
/// `rx = 0.25 span`, `ry = 0.38 fuselage_length`, `rz = 0.40 fuselage_length`;
/// each inertia is `mass * r^2`. A well-established conceptual-design
/// approximation for transport aircraft, used before a real structural mass
/// distribution exists, not a substitute for a mass-properties model.
pub fn estimate_inertia(plane: &Airplane, mass_kg: f64) -> (f64, f64, f64) {
    // Dynamic roll inertia is an aircraft-level reference quantity.  Use the
    // same lateral/Y span that normalizes the aerodynamic derivatives rather
    // than the wing's legacy unfolded YZ path.
    estimate_inertia_with_span(plane, mass_kg, plane.b_ref)
}

/// Frozen translation/parity inertia estimate using the historical unfolded
/// main-wing span. Product dynamic analyses use [`estimate_inertia`], whose
/// aircraft-level span is the selected lateral/Y reference.
pub fn estimate_inertia_reference_compatibility(plane: &Airplane, mass_kg: f64) -> (f64, f64, f64) {
    let span = plane
        .wings
        .first()
        .map(|wing| wing.unfolded_span())
        .unwrap_or(plane.b_ref);
    estimate_inertia_with_span(plane, mass_kg, span)
}

fn estimate_inertia_with_span(plane: &Airplane, mass_kg: f64, span: f64) -> (f64, f64, f64) {
    let fus = &plane.fuselages[0];
    let fus_len = fus.xsecs[fus.xsecs.len() - 1].xyz_c[0] - fus.xsecs[0].xyz_c[0];
    let rx = RX_SPAN_FRACTION * span;
    let ry = RY_LENGTH_FRACTION * fus_len;
    let rz = RZ_LENGTH_FRACTION * fus_len;
    (
        mass_kg * (rx * rx),
        mass_kg * (ry * ry),
        mass_kg * (rz * rz),
    )
}

/// One eigenmode of the linearized small-perturbation dynamics:
/// `DynamicMode`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynamicMode {
    /// The mode's name, one of the five [`DynamicModes`] field names.
    pub name: &'static str,
    /// Real part of the eigenvalue, 1/s.
    pub eigenvalue_real: f64,
    /// Imaginary part of the eigenvalue, rad/s.
    pub eigenvalue_imag: f64,
    /// Damping ratio.
    pub damping_ratio: f64,
    /// Undamped-frequency period `2 pi / |eigenvalue|`, s; `0.0` for a purely
    /// real (aperiodic) mode.
    pub period_s: f64,
    /// Whether the mode is stable (`eigenvalue_real < 0`).
    pub stable: bool,
}

/// The five dynamic modes `compute_dynamic_modes` reports, in the order
/// `get_modes` builds them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynamicModes {
    /// The slow longitudinal speed/altitude exchange.
    pub phugoid: DynamicMode,
    /// The fast longitudinal angle-of-attack oscillation.
    pub short_period: DynamicMode,
    /// The aperiodic roll damping.
    pub roll_subsidence: DynamicMode,
    /// The lateral-directional oscillation.
    pub dutch_roll: DynamicMode,
    /// The slow aperiodic bank/heading mode.
    pub spiral: DynamicMode,
}

/// Wrap one [`modes::Mode`] as a [`DynamicMode`], adding the period and
/// stability flag `compute_dynamic_modes` derives: the body of its
/// `for key, m in raw.items()` loop.
fn wrap(name: &'static str, mode: &modes::Mode) -> DynamicMode {
    let re = mode.eigenvalue_real;
    let im = mode.eigenvalue_imag;
    let wn = re.hypot(im);
    let period_s = if wn > 0.0 { 2.0 * PI / wn } else { 0.0 };
    DynamicMode {
        name,
        eigenvalue_real: re,
        eigenvalue_imag: im,
        damping_ratio: mode.damping_ratio,
        period_s,
        stable: re < 0.0,
    }
}

/// The longitudinal and lateral-directional dynamic modes at `op_point`:
/// `compute_dynamic_modes`.
///
/// Runs a fresh [`vlm::run_with_stability_derivatives`] sweep (six VLM
/// solves) and hands its derivatives to [`crate::modes::get_modes`], then
/// wraps each eigenmode with its period and stability flag.
///
/// # Errors
///
/// See [`VlmError`]: any of the six underlying solves can fail the way one can.
pub fn compute_dynamic_modes(
    plane: &Airplane,
    op_point: &OperatingPoint,
    mass_props: &MassProperties,
) -> Result<DynamicModes, VlmError> {
    compute_dynamic_modes_with_reference_mode(plane, op_point, mass_props, false)
}

/// Frozen translation/parity form of [`compute_dynamic_modes`].
///
/// The historical fixture uses the legacy rotational-origin and forward
/// finite-difference policy. Product callers use the projected-reference
/// geometry and the product derivative policy through [`compute_dynamic_modes`].
pub fn compute_dynamic_modes_reference_compatibility(
    plane: &Airplane,
    op_point: &OperatingPoint,
    mass_props: &MassProperties,
) -> Result<DynamicModes, VlmError> {
    compute_dynamic_modes_with_reference_mode(plane, op_point, mass_props, true)
}

fn compute_dynamic_modes_with_reference_mode(
    plane: &Airplane,
    op_point: &OperatingPoint,
    mass_props: &MassProperties,
    reference_compatibility: bool,
) -> Result<DynamicModes, VlmError> {
    let derivatives = if reference_compatibility {
        vlm::run_with_stability_derivatives_reference_compatibility(
            plane,
            op_point,
            1,
            CHORDWISE_RESOLUTION,
        )?
    } else {
        vlm::run_with_stability_derivatives(plane, op_point, 1, CHORDWISE_RESOLUTION)?
    };

    // The eleven coefficients get_modes reads, pulled out of the sweep's
    // thirty. CL/CD are the base run; the rest are the like-named derivative
    // (e.g. Cmq is d Cm / d q, Clb is d Cl / d beta).
    let aero = StabilityAero {
        cl: derivatives.base.cl_lift,
        cd: derivatives.base.cd_drag,
        cma: derivatives.d_alpha.cm_pitch,
        cmq: derivatives.d_q.cm_pitch,
        clp: derivatives.d_p.cl_roll,
        cyb: derivatives.d_beta.cy_side,
        cnb: derivatives.d_beta.cn_yaw,
        cyr: derivatives.d_r.cy_side,
        cnr: derivatives.d_r.cn_yaw,
        clb: derivatives.d_beta.cl_roll,
        clr: derivatives.d_r.cl_roll,
    };

    let raw = modes::get_modes(plane, op_point, mass_props, &aero);

    Ok(DynamicModes {
        phugoid: wrap("phugoid", &raw.phugoid),
        short_period: wrap("short_period", &raw.short_period),
        roll_subsidence: wrap("roll_subsidence", &raw.roll_subsidence),
        dutch_roll: wrap("dutch_roll", &raw.dutch_roll),
        spiral: wrap("spiral", &raw.spiral),
    })
}

// A test asserts on values it constructed here directly, so a failed unwrap or
// expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec};
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    /// A minimal airplane whose fuselage runs from `x = 0` to `x = fus_len` and
    /// whose single non-symmetric wing spans `span` in Y, just enough for
    /// [`estimate_inertia`] to read.
    fn probe(fus_len: f64, span: f64) -> Airplane {
        let naca = Airfoil::from_name("naca0012").expect("valid 4-digit NACA name");
        let fuselage = Fuselage::new(
            "Fuselage",
            vec![
                FuselageXSec::new([0.0, 0.0, 0.0], Some(1.0), None, None, 2.0).unwrap(),
                FuselageXSec::new([fus_len, 0.0, 0.0], Some(1.0), None, None, 2.0).unwrap(),
            ],
        );
        let wing = Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 1.0, 0.0, naca.clone()),
                WingXSec::new([0.0, span, 0.0], 1.0, 0.0, naca),
            ],
            false,
        );
        let b_ref = wing.reference_span();
        Airplane {
            name: "Inertia Probe".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: vec![wing],
            fuselages: vec![fuselage],
            s_ref: 1.0,
            c_ref: 1.0,
            b_ref,
        }
    }

    #[test]
    fn estimate_inertia_applies_the_radius_of_gyration_fractions() {
        let plane = probe(20.0, 10.0);
        let (ixx, iyy, izz) = estimate_inertia(&plane, 1000.0);
        // rx = 0.25*10 = 2.5, ry = 0.38*20 = 7.6, rz = 0.40*20 = 8.0.
        assert!((ixx - 1000.0 * 2.5 * 2.5).abs() < 1e-9, "ixx={ixx}");
        assert!((iyy - 1000.0 * 7.6 * 7.6).abs() < 1e-6, "iyy={iyy}");
        assert!((izz - 1000.0 * 8.0 * 8.0).abs() < 1e-9, "izz={izz}");
    }

    #[test]
    fn estimate_inertia_scales_linearly_with_mass() {
        let plane = probe(18.0, 9.0);
        let (ixx1, iyy1, izz1) = estimate_inertia(&plane, 100.0);
        let (ixx2, iyy2, izz2) = estimate_inertia(&plane, 300.0);
        assert!((ixx2 - 3.0 * ixx1).abs() < 1e-9);
        assert!((iyy2 - 3.0 * iyy1).abs() < 1e-9);
        assert!((izz2 - 3.0 * izz1).abs() < 1e-9);
    }

    #[test]
    fn a_zero_eigenvalue_gives_a_zero_period() {
        // The `wn > 0` false branch: a mode whose eigenvalue is exactly zero
        // has no frequency, so its period is 0.0 rather than a division by
        // zero. No real geometry reaches this (the parity fixture cannot)
        // so it is stated here.
        let zero = modes::Mode {
            eigenvalue_real: 0.0,
            eigenvalue_imag: 0.0,
            damping_ratio: f64::NAN,
        };
        let wrapped = wrap("degenerate", &zero);
        assert_eq!(wrapped.period_s, 0.0);
        assert!(!wrapped.stable, "a zero real part is not < 0");
    }

    #[test]
    fn an_aperiodic_mode_still_reports_a_finite_period() {
        // A purely real, nonzero eigenvalue has zero imaginary part but a
        // nonzero magnitude, so period = 2*pi/|eigenvalue|, not zero. This
        // is why the roll and spiral modes carry a period in the fixture.
        let real = modes::Mode {
            eigenvalue_real: -0.5,
            eigenvalue_imag: 0.0,
            damping_ratio: 1.0,
        };
        let wrapped = wrap("roll", &real);
        assert!((wrapped.period_s - 2.0 * PI / 0.5).abs() < 1e-12);
        assert!(wrapped.stable);
    }
}
