// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! FLAGGED tunnel-data correction (fleet-wide) of the tail-off
//! (wing-body-nacelle) aerodynamic centre, fitted to public low-speed
//! wind-tunnel data of transport configurations.
//!
//! The product's wing-body aerodynamic centre is the wing's own lattice
//! centre plus Torenbeek's body pair (*Synthesis of Subsonic Airplane
//! Design*, 1982, App. E, eqs. E-36 to E-38, printed pp. 479-480) and the
//! nacelle term E-41 (p. 481); Torenbeek calls these potential-flow "minimum
//! effects" (p. 479). No published body, nacelle, viscous or junction method
//! closes the difference to measured transport tail-off data, so this term is
//! an empirical correction: one rule for every aircraft, no per-aircraft
//! factor, reported as its own term.
//!
//! Fit set: measured minus predicted tail-off aerodynamic centre, fractions
//! of the reference mean aerodynamic chord (the trapezoid MAC of each model),
//! each model rebuilt in the
//! product chain (`tests/np_validation_tailoff.rs`):
//!
//! | Configuration (source, figure)                        | sweep c/4 | Re (MAC) | measured | residual |
//! |-------------------------------------------------------|-----------|----------|----------|----------|
//! | NASA TM X-1165 (1965) Fig. 3(a), body B1              | 24.5 deg  | 0.8e6    | 0.145    | -0.049   |
//! | NASA TM X-1165 Fig. 3(c), body B3                     | 24.5 deg  | 0.8e6    | 0.12     | -0.082   |
//! | NASA TM X-1165 Fig. 3(d), body B4                     | 24.5 deg  | 0.8e6    | 0.18     | -0.059   |
//! | C-5A 0.04 scale, NASA CR-134184 (1973) Fig. A-1       | 24.3 deg  | 1.8e6    | 0.14     | -0.049   |
//! | NASA TM X-3149 (1975) Fig. 5                          | 42.3 deg  | 1.4e6    | 0.15     | -0.108   |
//! | NASA TM X-3276 (1975) Fig. 3                          | 42.3 deg  | 1.4e6    | 0.12     | -0.097   |
//! | L-1011-1, NASA CR-172278 (1987) Fig. 33(a), data book | 35 deg    | n.p.     | 0.197    | -0.026   |
//!
//! Least squares in `tan(Lambda_c/4)`: `dh = -0.0189 - 0.0779 tan`, n 7,
//! residual scatter 0.026, slope standard error 0.050, leave-one-out error
//! 0.029 (a constant, -0.067, gives 0.032). Hold-out validation, not fitted:
//! Boeing 747-100 (NASA CR-147641 Fig. 48, 0.167) and DC-10 (NASA CR-3677
//! Fig. 5, 0.245 at Mach 0.6-0.7).
//!
//! Not representative, excluded: the Ames 40x80 ft models NASA TN D-3797
//! (1967) and NACA RM A58H12 (1958) agree with the uncorrected chain to 0.01;
//! both are simple circular bodies without wing nacelles at Reynolds numbers
//! 4.5e6-6.5e6, where configuration and scale cannot be separated. Their
//! validation tests (`tests/np_validation_ames.rs`) keep the uncorrected
//! chain. NASA TP-1580 is excluded (coarse grid, frame transfer, nacelles
//! unmodelled).
//!
//! Validity and handling outside it:
//! - Sweep: the fit spans c/4 sweep 24.3-42.3 deg. Above 42.3 deg the value
//!   is held at 42.3 deg. Below 24.3 deg no row exists: the value at 24.3 deg
//!   fades linearly to zero at 10 deg, so straight wings are uncorrected.
//! - Mach: every fit row is at Mach <= 0.21; the term has no Mach dependence
//!   and is held at its low-speed value at every Mach (the chain's own
//!   Prandtl-Glauert/Goethert terms carry compressibility). Check: the DC-10
//!   at the CR-3677 condition (Mach 0.65) needs -0.064 and gets -0.073.
//! - Clean wing, low wing or high wing, `b_f/b` 0.09-0.13.
//!
//! Checks after correction: the DC-10 preset at Mach 0.65 gives 0.246 against
//! CR-3677's 0.245. The B747-400 rigid clean neutral point (0.609 at Mach 0.2;
//! about 0.56 on the -100 planform) stays about 0.01-0.04 MAC aft of the
//! 0.52-0.55 implied by NASA CR-2144. The narrow-body A220-300 and E195-E2
//! fall about 0.02 below the 0.12-0.17 band of measured narrow-body tail-off
//! data, inside the fit scatter.

/// Analysis-summary key of the neutral-point shift this correction causes,
/// percent of the reference MAC (negative: forward); reports flag it.
pub const TUNNEL_AC_SUMMARY_KEY: &str = "np_tunnel_ac_correction_pct_mac";

/// Intercept of the tunnel-data correction line, fraction of the reference
/// mean aerodynamic chord (negative: forward).
pub const TUNNEL_AC_INTERCEPT: f64 = -0.0189;

/// Slope of the tunnel-data correction line per unit `tan(Lambda_c/4)`.
pub const TUNNEL_AC_SLOPE_PER_TAN: f64 = -0.0779;

/// Smallest c/4 sweep in the fit set, deg.
pub const TUNNEL_AC_SWEEP_MIN_DEG: f64 = 24.27;

/// Largest c/4 sweep in the fit set, deg.
pub const TUNNEL_AC_SWEEP_MAX_DEG: f64 = 42.33;

/// C/4 sweep at and below which the correction is zero, deg.
pub const TUNNEL_AC_SWEEP_ZERO_DEG: f64 = 10.0;

/// The tunnel-data correction (fleet-wide) of the wing-body aerodynamic
/// centre for a wing of quarter-chord sweep `sweep_c4_rad`, fraction of the
/// reference mean aerodynamic chord; see the module documentation for the
/// sources, fit and validity. Zero for a non-finite sweep.
#[must_use]
pub fn tunnel_ac_correction(sweep_c4_rad: f64) -> f64 {
    if !sweep_c4_rad.is_finite() {
        return 0.0;
    }
    let deg = sweep_c4_rad.abs().to_degrees();
    let line = |d: f64| TUNNEL_AC_INTERCEPT + TUNNEL_AC_SLOPE_PER_TAN * d.to_radians().tan();
    if deg >= TUNNEL_AC_SWEEP_MIN_DEG {
        return line(deg.min(TUNNEL_AC_SWEEP_MAX_DEG));
    }
    let fade =
        (deg - TUNNEL_AC_SWEEP_ZERO_DEG) / (TUNNEL_AC_SWEEP_MIN_DEG - TUNNEL_AC_SWEEP_ZERO_DEG);
    line(TUNNEL_AC_SWEEP_MIN_DEG) * fade.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_correction_follows_the_fit_inside_the_data_and_is_held_or_faded_outside() {
        let at = |deg: f64| tunnel_ac_correction(deg.to_radians());
        assert!((at(35.0) - (-0.0189 - 0.0779 * 35f64.to_radians().tan())).abs() < 1e-12);
        assert_eq!(at(50.0), at(TUNNEL_AC_SWEEP_MAX_DEG));
        assert_eq!(at(5.0), 0.0);
        let mid = 0.5 * (TUNNEL_AC_SWEEP_ZERO_DEG + TUNNEL_AC_SWEEP_MIN_DEG);
        assert!((at(mid) - 0.5 * at(TUNNEL_AC_SWEEP_MIN_DEG)).abs() < 1e-12);
        assert!(at(42.0) < at(25.0) && at(25.0) < 0.0);
        assert_eq!(tunnel_ac_correction(f64::NAN), 0.0);
    }
}
