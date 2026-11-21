// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The horizontal tail's dynamic-pressure ratio from the wing wake.
//!
//! USAF DATCOM (1978) Sec. 4.4.1 after Silverstein and Katzoff (NACA Report
//! 648), as printed in Roskam, *Airplane Design* Part VI, Sec. 8.1.5.2,
//! Eqs. 8.34-8.40, for jet airplanes: the wing sheds a viscous wake whose
//! centre-line momentum deficit falls with distance and whose half-width
//! grows with it,
//!
//! `q_h / q = 1 - [2.42 sqrt(CD0_w) / (x_h/c_w + 0.30)] cos^2(pi z_h / (2 z_w))`
//! for `|z_h| < z_w`, and `1` outside the wake,
//!
//! `z_w = 0.68 c_w sqrt(CD0_w (x_h/c_w + 0.15))`,
//! `z_h = z_t - x_h sin(alpha - eps_cl)`, `eps_cl = 1.62 CL_w / (pi A)`.
//!
//! `x_h` runs from the trailing edge of the wing root chord to the tail's
//! aerodynamic centre (the origin the DATCOM afterbody method of
//! [`super::downwash`] uses), `z_t` is the height of that centre above the
//! root trailing edge in body axes, `c_w` the wing's mean aerodynamic chord,
//! `CD0_w` the wing's zero-lift drag on its own area (the parasite buildup's
//! main-wing term), `alpha` the body attitude and `A` the wing aspect ratio.
//! A tail above the wake (a T-tail) therefore sits at `q_h / q = 1`; a tail
//! near the wake centre line loses the centre-line deficit.
//!
//! The body attitude comes from the wing-alone lattice at the cruise lift
//! coefficient: the lift curve of two level incompressible solves, its slope
//! multiplied by the finite-wing compressible slope ratio of
//! [`super::lift_slope`] (DATCOM 4.1.3.2, aspect ratio and half-chord sweep
//! of the wing). No wake is evaluated at `M >= 1`, outside that rule.
//!
//! Limits: the method is for jets without a propeller slipstream on the tail
//! (the slipstream increment of Roskam Eq. 8.41 is added by
//! [`super::tail_efficiency`]) and for an attached wake (cruise lift
//! coefficients, not stall). It does not include the fuselage boundary layer
//! or a nacelle wake: no primary source gives a method for them.

use alas_atmo::Atmosphere;

use super::performance::HSTAB_NAME;
use super::AeroAnalysis;
use crate::vlm::VlmSystem;

/// Name of the main wing among the airplane's surfaces.
const MAIN_WING_NAME: &str = "Main Wing";

/// Chord fraction of the tail's aerodynamic centre.
const TAIL_AC_CHORD_FRACTION: f64 = 0.25;

/// Angles of attack, deg, of the two level wing-alone solves whose lift curve
/// fixes the body attitude.
const ATTITUDE_PROBE_ALPHA_DEG: [f64; 2] = [0.0, 4.0];

/// Silverstein-Katzoff centre-line deficit coefficient (Roskam Eq. 8.38).
const DEFICIT_COEFFICIENT: f64 = 2.42;
/// Offset of the deficit's virtual origin, chords (Roskam Eq. 8.36).
const DEFICIT_ORIGIN_CHORDS: f64 = 0.30;
/// Wake half-width coefficient (Roskam Eq. 8.40).
const HALF_WIDTH_COEFFICIENT: f64 = 0.68;
/// Offset of the half-width's virtual origin, chords (Roskam Eq. 8.40).
const HALF_WIDTH_ORIGIN_CHORDS: f64 = 0.15;
/// Wake-centre-line downwash coefficient (Roskam Eq. 8.39).
const CENTRE_LINE_DOWNWASH: f64 = 1.62;

/// The wake model's inputs and result at one cruise state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TailWake {
    /// `q_h / q`, dimensionless, in (0, 1].
    pub ratio: f64,
    /// Wing zero-lift drag coefficient on its own area (`CD0_w`).
    pub wing_cd0: f64,
    /// Centre-line dynamic-pressure deficit `(Delta q / q)_max`.
    pub centre_line_deficit: f64,
    /// Wake half-width `z_w`, m.
    pub half_width_m: f64,
    /// Tail height above the wake centre line `z_h`, m.
    pub height_above_wake_m: f64,
    /// Distance `x_h` from the root-chord trailing edge to the tail, m.
    pub arm_m: f64,
    /// Body attitude at the cruise lift, deg.
    pub alpha_deg: f64,
    /// Wake centre-line downwash `eps_cl`, deg.
    pub centre_line_downwash_deg: f64,
}

/// Roskam Eqs. 8.36 and 8.38: `(q_h / q, centre-line deficit)` of a tail at
/// `x_h_chords` wing chords behind the root trailing edge and `z_h_m` above
/// the wake centre line, for a wake of half-width `half_width_m`.
#[must_use]
pub fn wake_dynamic_pressure_ratio(
    wing_cd0: f64,
    x_h_chords: f64,
    z_h_m: f64,
    half_width_m: f64,
) -> (f64, f64) {
    let deficit = DEFICIT_COEFFICIENT * wing_cd0.max(0.0).sqrt()
        / (x_h_chords.max(0.0) + DEFICIT_ORIGIN_CHORDS);
    if z_h_m.abs() >= half_width_m {
        return (1.0, deficit);
    }
    let profile = (std::f64::consts::FRAC_PI_2 * z_h_m / half_width_m).cos();
    (1.0 - deficit * profile * profile, deficit)
}

impl AeroAnalysis<'_> {
    /// The tail's dynamic-pressure ratio in the main wing's wake at the cruise
    /// `mach`, `altitude_m` and wing lift coefficient `cl_wing`; `None`
    /// without a main wing and a horizontal stabilizer, or when the lattice
    /// solves fail.
    pub fn horizontal_tail_wake(
        &self,
        mach: f64,
        altitude_m: f64,
        cl_wing: f64,
    ) -> Option<TailWake> {
        let plane = self.plane;
        let wing = plane.wings.iter().find(|w| w.name == MAIN_WING_NAME)?;
        let tail = plane.wings.iter().find(|w| w.name == HSTAB_NAME)?;
        let atmosphere = Atmosphere::new(altitude_m);
        let breakdown = self.parasite_breakdown(mach, altitude_m, Some(&atmosphere), None);
        let wing_cd0 = breakdown
            .components
            .iter()
            .find(|component| component.name == MAIN_WING_NAME)?
            .cd
            * plane.s_ref
            / wing.reference_area();
        let chord = wing.mean_aerodynamic_chord();
        let (root_le, root_chord) = wing.design_root();
        let root_twist_rad = wing.xsecs.first()?.twist.to_radians();
        let te = [
            root_le[0] + root_chord * root_twist_rad.cos(),
            root_le[2] - root_chord * root_twist_rad.sin(),
        ];
        let ac = tail.aerodynamic_center(TAIL_AC_CHORD_FRACTION);
        let arm_m = ac[0] - te[0];
        let height_m = ac[2] - te[1];
        if !(arm_m > 0.0 && chord > 0.0 && wing_cd0 > 0.0) {
            return None;
        }
        let alpha_deg = self.body_attitude_deg(mach, cl_wing)?;
        let aspect_ratio = wing.aspect_ratio();
        let eps_rad = CENTRE_LINE_DOWNWASH * cl_wing / (std::f64::consts::PI * aspect_ratio);
        let height_above_wake_m = height_m - arm_m * (alpha_deg.to_radians() - eps_rad).sin();
        let x_chords = arm_m / chord;
        let half_width_m = HALF_WIDTH_COEFFICIENT
            * chord
            * (wing_cd0 * (x_chords + HALF_WIDTH_ORIGIN_CHORDS)).sqrt();
        let (ratio, centre_line_deficit) =
            wake_dynamic_pressure_ratio(wing_cd0, x_chords, height_above_wake_m, half_width_m);
        ratio.is_finite().then_some(TailWake {
            ratio,
            wing_cd0,
            centre_line_deficit,
            half_width_m,
            height_above_wake_m,
            arm_m,
            alpha_deg,
            centre_line_downwash_deg: eps_rad.to_degrees(),
        })
    }

    /// Body attitude, deg, at which the wing alone carries `cl_wing` at
    /// `mach`: two incompressible level solves, slope times the finite-wing
    /// compressible ratio; `None` outside that ratio's domain (`M >= 1`).
    fn body_attitude_deg(&self, mach: f64, cl_wing: f64) -> Option<f64> {
        let wing = self.plane.wings.iter().find(|w| w.name == MAIN_WING_NAME)?;
        let mut wing_alone = self.plane.clone();
        wing_alone.wings = vec![wing.clone()];
        let (spanwise, chordwise) = self.mesh_resolution();
        let system = {
            let mut cache = self.vlm_cache.lock().ok()?;
            VlmSystem::assemble_cached(&wing_alone, spanwise, chordwise, &mut cache).ok()?
        };
        let solve = |alpha_deg: f64| -> Option<f64> {
            let op = Self::level_op_point(Atmosphere::new(0.0), 100.0, alpha_deg);
            Some(system.solve(&op).ok()?.cl_lift)
        };
        let [a0, a1] = ATTITUDE_PROBE_ALPHA_DEG;
        let (cl0, cl1) = (solve(a0)?, solve(a1)?);
        let slope_per_deg =
            (cl1 - cl0) / (a1 - a0) * self.main_wing_compressible_slope_ratio(mach)?;
        (slope_per_deg > 0.0).then(|| a0 + (cl_wing - cl0) / slope_per_deg)
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tail_outside_the_wake_sees_the_free_stream() {
        let (ratio, deficit) = wake_dynamic_pressure_ratio(0.012, 3.5, 1.5, 0.6);
        assert_eq!(ratio, 1.0);
        assert!(deficit > 0.0);
    }

    #[test]
    fn a_tail_on_the_centre_line_loses_the_whole_deficit() {
        let (ratio, deficit) = wake_dynamic_pressure_ratio(0.012, 3.5, 0.0, 0.6);
        // 2.42 sqrt(0.012) / (3.5 + 0.30) = 0.06976
        assert!((deficit - 0.069_76).abs() < 5.0e-5, "deficit {deficit}");
        assert!((ratio - (1.0 - deficit)).abs() < 1e-15);
    }

    #[test]
    fn the_profile_is_continuous_at_the_wake_edge_and_monotonic() {
        let (edge, _) = wake_dynamic_pressure_ratio(0.012, 3.5, 0.6 - 1e-12, 0.6);
        assert!((edge - 1.0).abs() < 1e-9);
        let mut previous = 0.0;
        for k in 0..=10 {
            let (r, _) = wake_dynamic_pressure_ratio(0.012, 3.5, 0.06 * f64::from(k), 0.6);
            assert!(r >= previous);
            previous = r;
        }
    }

    fn a320() -> (
        alas_geom::aircraft::airplane::Airplane,
        alas_config::AlasConfig,
    ) {
        use alas_geom::builder::AircraftBuilder;
        let entry = alas_config::presets::get("A320-200").expect("preset");
        let plane = AircraftBuilder::new(Some(entry.geometry.clone()))
            .build(Some(&entry.design_vector), true)
            .expect("builds");
        let config =
            alas_config::AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"}))
                .expect("config");
        (plane, config)
    }

    fn wake_of(
        plane: &alas_geom::aircraft::airplane::Airplane,
        config: &alas_config::AlasConfig,
    ) -> TailWake {
        let aero = AeroAnalysis::new(
            plane,
            AeroAnalysis::quarter_chord_sweep_deg(plane, 25.0),
            Some(config.geometry.clone()),
            Some(config.resolved_drag_model()),
            Some(config.analysis.clone()),
        );
        aero.horizontal_tail_wake(0.78, 11_000.0, 0.55)
            .expect("a wake")
    }

    #[test]
    fn the_attitude_takes_the_finite_wing_slope_and_stops_at_sonic_speed() {
        let (plane, config) = a320();
        let aero = AeroAnalysis::new(
            &plane,
            AeroAnalysis::quarter_chord_sweep_deg(&plane, 25.0),
            Some(config.geometry.clone()),
            Some(config.resolved_drag_model()),
            Some(config.analysis.clone()),
        );
        // The probes start at zero angle, so the attitudes at one lift
        // coefficient are in the inverse ratio of the slopes.
        let low = aero.body_attitude_deg(0.0, 0.55).expect("attitude");
        let cruise = aero.body_attitude_deg(0.78, 0.55).expect("attitude");
        let incidence_free = |alpha: f64| alpha - aero.body_attitude_deg(0.0, 0.0).unwrap_or(0.0);
        let ratio = aero
            .main_wing_compressible_slope_ratio(0.78)
            .expect("inside the domain");
        assert!(ratio > 1.2 && ratio < 1.0 / (1.0 - 0.78_f64 * 0.78).sqrt());
        assert!(cruise < low);
        assert!(incidence_free(cruise) > 0.0 && incidence_free(low) > 0.0);
        assert!(aero.body_attitude_deg(1.0, 0.55).is_none());
        assert!(aero.horizontal_tail_wake(1.2, 11_000.0, 0.55).is_none());
    }

    #[test]
    fn a_tail_lowered_onto_the_wake_centre_line_takes_the_centre_line_deficit() {
        let (mut plane, config) = a320();
        let high = wake_of(&plane, &config);
        assert!(high.height_above_wake_m.abs() > high.half_width_m);
        assert_eq!(high.ratio, 1.0);
        for wing in plane
            .wings
            .iter_mut()
            .filter(|w| w.name == "Horizontal Stabilizer")
        {
            for xsec in &mut wing.xsecs {
                xsec.xyz_le[2] -= high.height_above_wake_m;
            }
        }
        let low = wake_of(&plane, &config);
        assert!(
            low.height_above_wake_m.abs() < 1e-6,
            "{}",
            low.height_above_wake_m
        );
        assert!((low.ratio - (1.0 - low.centre_line_deficit)).abs() < 1e-9);
        // A few percent: the Silverstein-Katzoff wake is thin.
        assert!(low.ratio > 0.85 && low.ratio < 0.99, "{}", low.ratio);
        assert!((low.alpha_deg - high.alpha_deg).abs() < 1e-9);
    }
}
