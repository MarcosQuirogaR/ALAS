// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The take-off configuration's lift at a pitch attitude on the runway, and
//! the tail-strike attitude that bounds it: the inputs of the minimum unstick
//! speed `VMU` of CS 25.107(d).
//!
//! Lift at body pitch `theta` with the main wheels on the ground, take-off
//! flaps, in ground effect:
//!
//! ```text
//! C_L(theta) = [C_L0,wf + (C_L,alpha)_wf theta] * G(h_ac(theta), h_TE(theta))
//! ```
//!
//! `C_L0,wf` is the wing/fuselage lift at zero fuselage angle of attack with
//! the take-off flap (Torenbeek, *Synthesis of Subsonic Airplane Design*, 1982,
//! App. E, eq. E-29 at `alpha_f = 0`), its flap term the take-off flap's lift
//! at zero angle of attack by App. G (eqs. G-3/G-4, G-8, G-9, G-21) at the
//! declared take-off deflection and its chord extension
//! ([`super::support::takeoff_flap_zero_alpha_lift`]), `(C_L,alpha)_wf` eq.
//! E-30, and `G` the ground-effect lift ratio of eq. G-76
//! ([`super::wing_body::ground_effect_lift_ratio`]). The fuselage reference line is
//! level at `theta = 0` (that function's assumption); pitching about the main
//! wheel contact raises a point `d` ahead of it by `d sin(theta)`.
//!
//! At lift-off the wings carry the weight and the main-gear shock absorber
//! is fully extended, so the airframe stands higher over the wheel contact
//! than in the static position the model's ground plane describes (Airbus,
//! *Flight Operations Briefing Notes, Understanding Takeoff Speeds*, p. 3:
//! `VMU` is flown with the tail on the runway). The extension is the travel
//! from the static position to full extension of a stroke sized by the
//! energy balance of Jenkinson, Simpkin and Rhodes, *Civil Jet Aircraft
//! Design*, 1st ed., 1999, ch. 3, p. 48 (the same form as Raymer, *Aircraft
//! Design: A Conceptual Approach*, 6th ed., sec. 11.4, eq. 11.12):
//!
//! ```text
//! S = v^2 / (2 g lambda eta_S) - delta_T eta_T / eta_S
//! ```
//!
//! with the book's typical civil-jet values `v = 3.5 m/s`, `lambda = 2.0`,
//! `eta_S = 0.8`, `eta_T = 0.47`, and the static position at 84 % of the
//! stroke from full extension for large transports (Raymer, 6th ed., sec.
//! 11.4, p. 357). The tyre terms cancel to first order: a tyre is close to a
//! linear spring, so its deflection under the `lambda = 2` landing reaction is
//! twice its static deflection `delta_s`, and the tyre credit removed from
//! the stroke, `0.84 (eta_T / eta_S) 2 delta_s = 0.99 delta_s`, equals the
//! static deflection the unloading tyre gives back at lift-off. The
//! extension is therefore `0.84 v^2 / (2 g lambda eta_S)`.
//!
//! The tail-strike attitude is the model's fuselage contour against the
//! ground with that extension (`tail_scrape_angle_deg`), pitched about the
//! aft-most main-gear station (`LandingGearStationPositions::rotation_pivot_x_m`);
//! published attitudes are a validation table, never an input.
//!
//! Between the static position and lift-off the shock absorber extends along
//! its air spring ([`oleo_extension_fraction`]), so the lift, the wing height
//! and the wheel load of every instant of the rotation are one state on one
//! ground plane: the plane the lift-off lift is built on, carried whole by
//! the take-off integration through [`alas_perf::performance::RunwayAirframe`]
//! (the lift at an attitude and extension, the rise of the aerodynamic centre
//! and of the trailing edge, the tail-strike attitude) and by
//! [`LiftOffLift::ground_effect()`] (the same wing for the drag). A judgement
//! of the design flies every plane `alas_mass::stations::ground_plane_bases`
//! names ([`liftoff_lifts`]): the declared one, or both ends of the documented
//! clearance sample, each end a whole rotation, never channels from different
//! ends.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_mass::stations::GroundPlaneBasis;
use alas_perf::performance::{GroundEffect, RunwayAirframe, WingRise};

/// Design sink speed of the stroke balance, m/s: "v = 3.5 m/s ... typical"
/// (Jenkinson, Simpkin and Rhodes, 1999, ch. 3, p. 48).
const STROKE_SINK_SPEED_M_S: f64 = 3.5;
/// Reaction factor of the stroke balance, g: "lambda = 2.0 ... typical"
/// (ibid.).
const STROKE_REACTION_FACTOR: f64 = 2.0;
/// Oleo-pneumatic strut absorption efficiency: "can be assumed to be 0.8"
/// (ibid.).
const STROKE_STRUT_EFFICIENCY: f64 = 0.8;
/// Static position of a large transport's main gear, fraction of the stroke
/// from full extension (Raymer, 6th ed., sec. 11.4, p. 357).
const STATIC_STROKE_FRACTION: f64 = 0.84;

/// Air-spring load of the main-gear shock absorber at full extension over
/// its load in the static position: the air curve of a typical main landing
/// gear strut, which rises from the fully extended position through the
/// static load at maximum taxi weight towards the zero-volume closure
/// (Niu, *Airframe Structural Design*, 1988, Fig. 3.9.5, p. 72), read off
/// the figure as the ratio of the two ordinates over its absolute-zero
/// pressure axis (no scale is printed; reading +-0.02).
const EXTENDED_AIR_LOAD_RATIO: f64 = 0.18;

/// Shock-absorber extension from the static position, as a fraction of the
/// full extension [`liftoff_oleo_extension_m`], at a main-gear load of
/// `wheel_load` times the static load.
///
/// The strut carries its load by the pressure of compressed gas (Raymer,
/// *Aircraft Design: A Conceptual Approach*, 6th ed., sec. 11.4, p. 357), a
/// gas spring: at constant temperature (Boyle) the load is inversely
/// proportional to the gas volume, `F (x0 - x) = const` with `x` the closure
/// and `x0` the zero-volume closure. With `r` the extended-to-static load
/// ratio this gives the extension from the static position
///
/// ```text
/// e(w) = r / (1 - r) (1 / w - 1),   w >= r;   e = 1 (full extension), w <= r
/// ```
///
/// The isothermal hyperbola reproduces the air curve of the figure the ratio
/// is read from (its static point at 0.87 of the stroke and its zero-volume
/// closure at 1.08 strokes give `r = 0.20`, and its intermediate ordinates
/// within 0.03 of the static load), and the unloading of a rotation lasts
/// seconds, not the fraction of a second of a landing stroke. The extension
/// of the stroke balance (module note) carries the tyre's share already
/// cancelled, so the whole extension follows the strut's law. With half the
/// weight on the wings the extension is 0.22 of full (a law linear in the
/// load would give 0.5); it is complete once the wings carry 82 % of the
/// weight.
#[must_use]
pub fn oleo_extension_fraction(wheel_load: f64) -> f64 {
    let r = EXTENDED_AIR_LOAD_RATIO;
    if wheel_load <= r {
        return 1.0;
    }
    (r / (1.0 - r) * (1.0 / wheel_load - 1.0)).clamp(0.0, 1.0)
}

/// Extension of the main-gear shock absorber from the static position to
/// full extension at lift-off, m (see the module note).
#[must_use]
pub fn liftoff_oleo_extension_m() -> f64 {
    STATIC_STROKE_FRACTION * STROKE_SINK_SPEED_M_S * STROKE_SINK_SPEED_M_S
        / (2.0 * alas_atmo::STANDARD_GRAVITY * STROKE_REACTION_FACTOR * STROKE_STRUT_EFFICIENCY)
}

/// The lift of the take-off configuration against runway pitch attitude, and
/// the attitude at which the tail touches, main gear fully extended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiftOffLift {
    /// `C_L0,wf` in free air at zero fuselage angle, take-off flap.
    pub cl_zero_alpha: f64,
    /// The take-off flap's share of [`Self::cl_zero_alpha`].
    pub flap_zero_alpha_lift: f64,
    /// `(C_L,alpha)_wf`, per radian.
    pub cl_alpha_wf_per_rad: f64,
    /// MAC quarter-chord height over the ground at `theta = 0`, gear
    /// extended, m.
    pub quarter_chord_height_m: f64,
    /// MAC trailing-edge height over the ground at `theta = 0`, gear
    /// extended, m.
    pub trailing_edge_height_m: f64,
    /// Rotation-pivot station minus the MAC quarter-chord station, m (positive
    /// when the quarter chord is ahead of the gear).
    pub quarter_chord_ahead_m: f64,
    /// Rotation-pivot station minus the MAC trailing-edge station, m.
    pub trailing_edge_ahead_m: f64,
    /// Main-gear extension from the static position, m.
    pub oleo_extension_m: f64,
    /// Fuselage-contour tail-strike attitude, gear extended, degrees.
    pub tail_strike_deg: f64,
    /// The same contour on the static ground plane (gear compressed),
    /// degrees, for the validation table.
    pub static_tail_strike_deg: Option<f64>,
    ground_effect: GroundEffectScalars,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct GroundEffectScalars {
    area_m2: f64,
    span_m: f64,
    half_chord_cos: f64,
    cl_alpha_wing: f64,
}

impl LiftOffLift {
    /// `C_L` at body pitch `theta_deg` on the runway, in ground effect, gear
    /// fully extended (lift-off).
    #[must_use]
    pub fn cl_ground(&self, theta_deg: f64) -> f64 {
        self.cl_ground_at(theta_deg, 1.0)
    }

    /// `C_L` at body pitch `theta_deg` on the runway, in ground effect, the
    /// shock absorber `extension` of the way from the static position (0) to
    /// full extension (1).
    #[must_use]
    pub fn cl_ground_at(&self, theta_deg: f64, extension: f64) -> f64 {
        let theta = theta_deg.to_radians();
        let free_air = self.cl_alpha_wf_per_rad.mul_add(theta, self.cl_zero_alpha);
        let (h_ac, h_te) = self.wing_heights_m(theta_deg, extension);
        free_air * self.ground_effect.ratio(free_air, h_ac, h_te)
    }

    /// Heights of the MAC quarter chord and trailing edge over the ground
    /// plane, m, at body pitch `theta_deg` about the main-wheel contact with
    /// the shock absorber `extension` of the way to full extension: a point
    /// `d` ahead of the wheels and `h` above the plane level stands at
    /// `h cos(theta) + d sin(theta)`.
    #[must_use]
    pub fn wing_heights_m(&self, theta_deg: f64, extension: f64) -> (f64, f64) {
        let theta = theta_deg.to_radians();
        let lowered = (1.0 - extension.clamp(0.0, 1.0)) * self.oleo_extension_m;
        let h_ac = (self.quarter_chord_height_m - lowered)
            .mul_add(theta.cos(), self.quarter_chord_ahead_m * theta.sin());
        let h_te = (self.trailing_edge_height_m - lowered)
            .mul_add(theta.cos(), self.trailing_edge_ahead_m * theta.sin());
        (h_ac, h_te)
    }

    /// The `VMU` lift coefficient and whether it is geometry-limited: the
    /// lift at the tail-strike attitude when it is below `cl_max_to` (the
    /// tail reaches the runway first, CS 25.107(d) and (e)(1)(iv)(B)), else
    /// `cl_max_to` (the wing stalls first).
    #[must_use]
    pub fn unstick_lift(&self, cl_max_to: f64) -> (f64, bool) {
        let at_tail = self.cl_ground(self.tail_strike_deg);
        if at_tail < cl_max_to {
            (at_tail, true)
        } else {
            (cl_max_to, false)
        }
    }

    /// The runway attitude at which the lift in ground effect is `cl`,
    /// degrees, in `[0, tail_strike_deg]`: `0` when the level airframe
    /// already carries `cl`, the tail-strike attitude when it cannot reach
    /// it.
    #[must_use]
    pub fn attitude_deg(&self, cl: f64) -> f64 {
        let (mut lo, mut hi) = (0.0, self.tail_strike_deg);
        if self.cl_ground(lo) >= cl {
            return lo;
        }
        if self.cl_ground(hi) <= cl {
            return hi;
        }
        for _ in 0..50 {
            let mid = 0.5 * (lo + hi);
            if self.cl_ground(mid) < cl {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// Rise of the MAC quarter chord and trailing edge over their static,
    /// level heights (gear compressed at the take-off weight, `theta = 0`) at
    /// body pitch `theta_deg` and shock-absorber `extension`: the kinematics
    /// [`Self::cl_ground_at`] reads, for the drag of the same wing.
    #[must_use]
    pub fn wing_rise(&self, theta_deg: f64, extension: f64) -> WingRise {
        let (h_ac, h_te) = self.wing_heights_m(theta_deg, extension);
        let (static_ac, static_te) = self.wing_heights_m(0.0, 0.0);
        WingRise {
            aerodynamic_centre_m: h_ac - static_ac,
            trailing_edge_m: h_te - static_te,
        }
    }

    /// The wing over this lift's ground plane for the ground-effect reduction
    /// of the take-off drag: span, mean geometric chord `S/b`, and the static,
    /// level heights of the MAC quarter chord and trailing edge, the plane,
    /// wing and points [`Self::cl_ground_at`] and the tail-strike attitude
    /// use. `None` for degenerate geometry.
    #[must_use]
    pub fn ground_effect(&self) -> Option<GroundEffect> {
        let scalars = self.ground_effect;
        let (h_ac, h_te) = self.wing_heights_m(0.0, 0.0);
        GroundEffect::new(scalars.span_m, scalars.area_m2 / scalars.span_m, h_ac, h_te)
    }
}

impl GroundEffectScalars {
    fn ratio(&self, lift: f64, h_ac: f64, h_te: f64) -> f64 {
        super::wing_body::ground_effect_lift_ratio_of(
            self.area_m2,
            self.span_m,
            self.half_chord_cos,
            self.cl_alpha_wing,
            lift,
            h_ac,
            h_te,
        )
    }
}

/// The lift-off lift as the take-off integration's runway airframe: one
/// geometry for the lift, the wing heights, the shock absorber and the
/// tail-strike attitude.
impl RunwayAirframe for LiftOffLift {
    fn lift_coefficient(&self, theta_deg: f64, extension: f64) -> f64 {
        self.cl_ground_at(theta_deg, extension)
    }

    fn wing_rise(&self, theta_deg: f64, extension: f64) -> WingRise {
        LiftOffLift::wing_rise(self, theta_deg, extension)
    }

    fn extension(&self, wheel_load: f64) -> f64 {
        oleo_extension_fraction(wheel_load)
    }

    fn tail_strike_deg(&self) -> f64 {
        self.tail_strike_deg
    }

    fn liftoff_attitude_deg(&self, cl: f64) -> f64 {
        self.attitude_deg(cl)
    }
}

/// [`LiftOffLift`] of `plane` on its nominal ground plane (the declared
/// plane, or the class nominal clearance for reports and drawings, which
/// judge nothing); `None` without a wing, a fuselage, finite wing-body terms,
/// a main-gear station or a fuselage contour aft of it. A judgement of the
/// design takes [`liftoff_lifts`].
#[must_use]
pub fn liftoff_lift(plane: &Airplane, config: &AlasConfig) -> Option<LiftOffLift> {
    lift_over(plane, config, |fuselage| {
        Some(super::support::ground_z_m(fuselage, config))
    })
}

/// [`LiftOffLift`] of `plane` on the ground plane of `basis` (see
/// [`liftoff_lift`] for `None`).
#[must_use]
pub fn liftoff_lift_on(
    plane: &Airplane,
    config: &AlasConfig,
    basis: GroundPlaneBasis,
) -> Option<LiftOffLift> {
    lift_over(plane, config, |_| {
        super::support::ground_z_on(basis, plane, config)
    })
}

/// The lift-off lifts a judgement of `plane` flies the take-off field on, one
/// per ground plane `alas_mass::stations::ground_plane_bases` names (the
/// declared plane, or both ends of the documented clearance sample), each a
/// whole rotation on its own plane. Empty when the lift builds on no plane
/// (the field is then flown without a lift-off schedule, as for an airframe
/// without the geometry).
///
/// # Errors
///
/// A take-off field evidence gap when the lift builds on some planes and not
/// on the others: flying the built planes alone, or none, would drop `VMU`
/// and the rotation on a plane the judgement must take.
pub fn liftoff_lifts(plane: &Airplane, config: &AlasConfig) -> Result<Vec<LiftOffLift>, String> {
    every_plane(
        alas_mass::stations::ground_plane_bases(&config.landing_gear)
            .iter()
            .map(|&basis| (basis, liftoff_lift_on(plane, config, basis))),
    )
}

/// All the lifts, none, or the evidence gap of a partial set (see
/// [`liftoff_lifts`]).
fn every_plane(
    lifts: impl Iterator<Item = (GroundPlaneBasis, Option<LiftOffLift>)>,
) -> Result<Vec<LiftOffLift>, String> {
    let (built, missing): (Vec<_>, Vec<_>) = lifts.partition(|(_, lift)| lift.is_some());
    if built.is_empty() || missing.is_empty() {
        return Ok(built.into_iter().filter_map(|(_, lift)| lift).collect());
    }
    Err(format!(
        "take-off field evidence gap: the lift-off lift builds on the {:?} ground plane but \
         not on {:?}; every plane of an undeclared clearance is flown whole",
        built.iter().map(|(basis, _)| *basis).collect::<Vec<_>>(),
        missing.iter().map(|(basis, _)| *basis).collect::<Vec<_>>(),
    ))
}

/// [`LiftOffLift`] of `plane` with the static ground plane `static_ground_z`
/// gives for its fuselage.
fn lift_over(
    plane: &Airplane,
    config: &AlasConfig,
    static_ground_z: impl FnOnce(&Fuselage) -> Option<f64>,
) -> Option<LiftOffLift> {
    let main = plane.wings.first()?;
    let fuselage = plane.fuselages.first()?;
    let s_ref = main.reference_area();
    let terms = super::wing_body::wing_body_terms(plane, config)?;
    let moment = super::support::LandingPitchingMoment {
        flap_zero_alpha_lift: super::support::takeoff_flap_zero_alpha_lift(plane, config, s_ref)?,
        ..super::support::takeoff_pitching_moment(plane, config, s_ref)?
    };
    let cl_zero_alpha = terms.zero_alpha_lift(&moment);
    let extension = liftoff_oleo_extension_m();
    let static_ground_z = static_ground_z(fuselage)?;
    let ground_z = static_ground_z - extension;
    let mac = main.carried_box();
    let (ac, te) = (mac.aerodynamic_center(0.25), mac.aerodynamic_center(1.0));
    let x_mlg = rotation_pivot_m(plane, config)?;
    let contour = super::support::lower_contour_aft_of(plane, fuselage, x_mlg);
    let tail_scrape = |z: f64| {
        alas_perf::landing_gear::geometry::tail_scrape_angle_deg(&contour, x_mlg, z)
            .filter(|deg| deg.is_finite() && *deg > 0.0)
    };
    let lift = LiftOffLift {
        cl_zero_alpha,
        flap_zero_alpha_lift: moment.flap_zero_alpha_lift,
        cl_alpha_wf_per_rad: terms.cl_alpha_wf,
        quarter_chord_height_m: ac[2] - ground_z,
        trailing_edge_height_m: te[2] - ground_z,
        quarter_chord_ahead_m: x_mlg - ac[0],
        trailing_edge_ahead_m: x_mlg - te[0],
        oleo_extension_m: extension,
        tail_strike_deg: tail_scrape(ground_z)?,
        static_tail_strike_deg: tail_scrape(static_ground_z),
        ground_effect: GroundEffectScalars {
            area_m2: plane.s_ref,
            span_m: plane.b_ref,
            half_chord_cos: main.mean_sweep_angle(0.5).to_radians().cos(),
            cl_alpha_wing: terms.cl_alpha_wing,
        },
    };
    (lift.cl_zero_alpha.is_finite()
        && lift.cl_alpha_wf_per_rad.is_finite()
        && lift.cl_alpha_wf_per_rad > 0.0)
        .then_some(lift)
}

/// The main-gear contact the airframe pitches about on the runway, m: the
/// aft-most main station, the envelope's tail-scrape and tip-back pivot.
fn rotation_pivot_m(plane: &Airplane, config: &AlasConfig) -> Option<f64> {
    let fuselage = plane.fuselages.first()?;
    let (start, end) = (
        fuselage.xsecs.first()?.xyz_c[0],
        fuselage.xsecs.last()?.xyz_c[0],
    );
    let frame = plane.mac_frame()?;
    let length = end - start;
    let stations = config.landing_gear.resolved_station_positions(
        start + length * config.mass_model.nlg_x_fraction,
        frame.x_lemac_m + config.mass_model.mlg_x_fraction_mac * frame.chord_m,
        start,
        length,
    );
    let pivot_x_m = stations.rotation_pivot_x_m();
    pivot_x_m.is_finite().then_some(pivot_x_m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_oleo_extension_is_the_static_share_of_the_civil_jet_stroke() {
        // 0.84 * 3.5^2 / (2 * 9.80665 * 2.0 * 0.8) = 0.328 m.
        let extension = liftoff_oleo_extension_m();
        assert!((extension - 0.3279).abs() < 5e-4, "{extension}");
    }

    fn a320() -> Option<(Airplane, AlasConfig)> {
        use alas_geom::builder::AircraftBuilder;
        let name = "A320-200";
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).ok()?;
        let registered = alas_config::presets::get(name).ok()?;
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&registered.design_vector), true)
            .ok()?;
        Some((plane, config))
    }

    #[test]
    fn extending_the_gear_raises_the_tail_strike_attitude() {
        let Some((plane, config)) = a320() else {
            panic!("A320-200 preset must build");
        };
        let lift = liftoff_lift(&plane, &config).expect("A320 lift-off lift");
        let compressed = lift.static_tail_strike_deg.expect("static contour");
        assert!(lift.tail_strike_deg > compressed + 0.5, "{lift:?}");
    }

    #[test]
    fn lift_rises_with_attitude_and_the_inverse_recovers_it() {
        let Some((plane, config)) = a320() else {
            panic!("A320-200 preset must build");
        };
        let lift = liftoff_lift(&plane, &config).expect("A320 lift-off lift");
        let (low, high) = (lift.cl_ground(2.0), lift.cl_ground(8.0));
        assert!(high > low, "{low} {high}");
        let theta = lift.attitude_deg(lift.cl_ground(6.0));
        assert!((theta - 6.0).abs() < 1e-6, "{theta}");
        let (cl, limited) = lift.unstick_lift(10.0);
        assert!(limited && (cl - lift.cl_ground(lift.tail_strike_deg)).abs() < 1e-12);
        let (cl, limited) = lift.unstick_lift(0.5);
        assert!(!limited && (cl - 0.5).abs() < 1e-12);
    }

    #[test]
    fn the_air_spring_extends_the_strut_along_boyles_hyperbola() {
        assert!(oleo_extension_fraction(1.0).abs() < 1e-12);
        assert!((oleo_extension_fraction(EXTENDED_AIR_LOAD_RATIO) - 1.0).abs() < 1e-12);
        assert!((oleo_extension_fraction(0.0) - 1.0).abs() < 1e-12);
        // r / (1 - r) (1 / 0.5 - 1) = 0.18 / 0.82 = 0.2195.
        assert!((oleo_extension_fraction(0.5) - 0.2195).abs() < 1e-4);
        let loads = [1.0, 0.8, 0.6, 0.4, 0.2];
        let extensions = loads.map(oleo_extension_fraction);
        assert!(
            extensions.windows(2).all(|pair| pair[1] > pair[0]),
            "{extensions:?}"
        );
    }

    #[test]
    fn the_wing_rises_with_the_attitude_and_the_extension() {
        let Some((plane, config)) = a320() else {
            panic!("A320-200 preset must build");
        };
        let lift = liftoff_lift(&plane, &config).expect("A320 lift-off lift");
        let level = lift.wing_rise(0.0, 0.0);
        assert!(level.aerodynamic_centre_m.abs() < 1e-12);
        assert!(level.trailing_edge_m.abs() < 1e-12);
        let extended = lift.wing_rise(0.0, 1.0);
        assert!((extended.aerodynamic_centre_m - lift.oleo_extension_m).abs() < 1e-12);
        assert!((extended.trailing_edge_m - lift.oleo_extension_m).abs() < 1e-12);
        let (h_ac, h_te) = lift.wing_heights_m(0.0, 1.0);
        assert!((h_ac - lift.quarter_chord_height_m).abs() < 1e-12);
        assert!((h_te - lift.trailing_edge_height_m).abs() < 1e-12);
        // The quarter chord stands ahead of the main wheels, so it rises
        // with the attitude; the lift reads the same heights.
        let pitched = lift.wing_rise(lift.tail_strike_deg, 1.0);
        assert!(pitched.aerodynamic_centre_m > extended.aerodynamic_centre_m);
        assert!((lift.cl_ground(6.0) - lift.cl_ground_at(6.0, 1.0)).abs() < 1e-15);
        assert!(lift.cl_ground_at(6.0, 0.0) != lift.cl_ground_at(6.0, 1.0));
    }

    #[test]
    fn the_drag_ground_effect_is_the_static_level_wing() {
        let Some((plane, config)) = a320() else {
            panic!("A320-200 preset must build");
        };
        let lift = liftoff_lift(&plane, &config).expect("A320 lift-off lift");
        let wing = lift.ground_effect().expect("A320 ground effect");
        let (h_ac, h_te) = lift.wing_heights_m(0.0, 0.0);
        assert!((wing.wing_height_m - h_ac).abs() < 1e-12);
        assert!((wing.trailing_edge_height_m - h_te).abs() < 1e-12);
    }

    #[test]
    fn an_undeclared_clearance_flies_both_ends_whole() {
        let Some((plane, mut config)) = a320() else {
            panic!("A320-200 preset must build");
        };
        config.landing_gear.fuselage_ground_clearance_m = None;
        let lifts = liftoff_lifts(&plane, &config).expect("both ends build");
        let [low, high] = lifts.as_slice() else {
            panic!("two ground planes: {lifts:?}");
        };
        assert!(high.quarter_chord_height_m > low.quarter_chord_height_m);
        assert!(
            high.tail_strike_deg > low.tail_strike_deg,
            "{low:?} {high:?}"
        );
    }

    #[test]
    fn a_lift_on_some_planes_only_is_an_evidence_gap() {
        let Some((plane, config)) = a320() else {
            panic!("A320-200 preset must build");
        };
        let lift = liftoff_lift(&plane, &config).expect("A320 lift-off lift");
        let [low, high] = GroundPlaneBasis::ENDS;
        let partial = every_plane([(low, Some(lift)), (high, None)].into_iter());
        let error = partial.expect_err("a partial set is a gap");
        assert!(error.contains("evidence gap"), "{error}");
        let none = every_plane([(low, None), (high, None)].into_iter());
        assert!(none.is_ok_and(|lifts| lifts.is_empty()));
        let all = every_plane([(low, Some(lift)), (high, Some(lift))].into_iter());
        assert!(all.is_ok_and(|lifts| lifts.len() == 2));
    }
}
