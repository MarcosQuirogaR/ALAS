// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The extended-chord lift of the deployed flaps and slats in the take-off and
//! landing trims: the wing the neutral point's high-lift condition sees
//! ([`alas_stab::neutral_point::extended_chord_lift`]), at the trim's own flap
//! angle.
//!
//! An extending flap raises the wing's lift-curve slope by about the ratio of
//! the extended to the original wing area (Raymer, *Aircraft Design: A
//! Conceptual Approach*, 6th ed., printed p. 413): an increment `delta a_w`,
//! `delta = sum (c'/c - 1) S_run / S` over the flap and slat runs, whose lift
//! acts at the quarter chord of the extended sections, `x_e`. The flap's
//! `c'/c` at the trim's angle is its Jenkinson Fig. 8.11 curve
//! ([`alas_config::HighLiftChordExtension::flap_extension_at`]); the slat's
//! is its one deployed value (Fig. 8.10).
//!
//! A wing-body of slope `a_wb`, aerodynamic centre `x_ac` and moment `Cm_ac`
//! about it, with lift `CL_0` at zero body attitude, carries the added lift
//! `delta a_w alpha = k (CL - CL_0)` at `x_e`:
//!
//! ```text
//! a'     = a_wb + delta a_w
//! k      = delta a_w / a'
//! x_ac'  = x_ac + k (x_e - x_ac)
//! Cm_ac' = Cm_ac + k CL_0 (x_e - x_ac) / c
//! ```
//!
//! so that `c Cm_ac + CL (x - x_ac) - k (CL - CL_0)(x_e - x_ac) = c Cm_ac' +
//! CL (x - x_ac')` about any station `x`. `x_ac'` is the lift-weighted centre
//! of the neutral point's high-lift condition; the zero-attitude lift, which
//! the slope increment does not carry, keeps acting where it did.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_stab::neutral_point::{extended_chord_lift, HighLiftDevices};

/// Flap deflection of the take-off trim, degrees: the declared take-off
/// setting, else the class value
/// ([`alas_config::presets::DEFAULT_TAKEOFF_FLAP_DEG`], the one the take-off
/// polar uses).
pub(super) fn takeoff_flap_deg(config: &AlasConfig) -> f64 {
    config
        .performance
        .takeoff_flap_deflection_deg
        .filter(|deg| deg.is_finite() && *deg > 0.0)
        .unwrap_or(alas_config::presets::DEFAULT_TAKEOFF_FLAP_DEG)
}

/// Flap deflection of the landing trims, degrees: the angle the stored chord
/// extension is read at, so the landing wing is the neutral point's.
pub(super) fn landing_flap_deg(config: &AlasConfig) -> f64 {
    config
        .performance
        .high_lift_chord_extension
        .flap_landing_deg
}

/// `(delta, x_e)` of the declared devices with the flap at `flap_deg`: the
/// slope increment as a fraction of the wing slope and the station its lift
/// acts at (geometry axes), m. `None` when nothing extends the chord.
pub(super) fn extended_lift(
    plane: &Airplane,
    config: &AlasConfig,
    flap_deg: f64,
) -> Option<(f64, f64)> {
    let extension = config.performance.high_lift_chord_extension;
    let mut devices = HighLiftDevices::declared(extension, &config.control_surfaces)?;
    devices.flap_extension = extension.flap_extension_at(flap_deg).max(0.0);
    devices.flap_deflection_deg = flap_deg;
    let lift = extended_chord_lift(plane, &devices);
    (lift.slope_increment > 0.0 && lift.x_acting_m.is_finite())
        .then_some((lift.slope_increment, lift.x_acting_m))
}

/// A wing-body with the extended-chord lift added (module docs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FlappedWingBody {
    /// Lift-curve slope `a'`, per rad.
    pub(super) slope: f64,
    /// Aerodynamic-centre shift `x_ac' - x_ac`, m (aft positive).
    pub(super) ac_shift_m: f64,
    /// Moment-coefficient increment `Cm_ac' - Cm_ac`, on `S` and `c`.
    pub(super) cm_ac_shift: f64,
}

impl FlappedWingBody {
    /// The wing-body of slope `slope_wb`, centre `x_ac_m` and zero-attitude
    /// lift `cl_zero` with `extended` (from [`extended_lift`]) on a wing of
    /// slope `slope_w`, both per rad; `mac_m` the reference chord. Unchanged
    /// without an extension or with a non-positive slope.
    pub(super) fn new(
        slope_wb: f64,
        slope_w: f64,
        x_ac_m: f64,
        mac_m: f64,
        cl_zero: f64,
        extended: Option<(f64, f64)>,
    ) -> Self {
        let unchanged = Self {
            slope: slope_wb,
            ac_shift_m: 0.0,
            cm_ac_shift: 0.0,
        };
        let Some((delta, x_e)) = extended else {
            return unchanged;
        };
        let added = delta * slope_w;
        let slope = slope_wb + added;
        if !(added > 0.0 && slope > 0.0 && mac_m > 0.0) {
            return unchanged;
        }
        let k = added / slope;
        Self {
            slope,
            ac_shift_m: k * (x_e - x_ac_m),
            cm_ac_shift: k * cl_zero * (x_e - x_ac_m) / mac_m,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flapped_wing_body_moves_the_moment_of_the_added_lift_only() {
        // a_wb 5, a_w 4.5, delta 0.2 at 3 m aft of x_ac on a 6 m chord.
        let (x_ac, c, cl0) = (20.0, 6.0, 0.8);
        let flapped = FlappedWingBody::new(5.0, 4.5, x_ac, c, cl0, Some((0.2, x_ac + 3.0)));
        assert!((flapped.slope - 5.9).abs() < 1e-12);
        let k = 0.9 / 5.9;
        assert!((flapped.ac_shift_m - 3.0 * k).abs() < 1e-12);
        // Same moment about any station at any lift as the added lift at x_e.
        for (x, cl) in [(18.0, 1.6), (21.5, 2.2), (25.0, cl0)] {
            let added = k * (cl - cl0);
            let direct = c * -0.3 + cl * (x - x_ac) - added * 3.0;
            let shifted = c * (-0.3 + flapped.cm_ac_shift) + cl * (x - x_ac - flapped.ac_shift_m);
            assert!((direct - shifted).abs() < 1e-12, "{x} {cl}");
        }
        let none = FlappedWingBody::new(5.0, 4.5, x_ac, c, cl0, None);
        assert_eq!(
            (none.slope, none.ac_shift_m, none.cm_ac_shift),
            (5.0, 0.0, 0.0)
        );
    }
}
