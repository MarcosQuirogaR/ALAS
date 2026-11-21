// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The takeoff setting of a trimmable horizontal stabiliser, derived from the
//! trim balance at `V2` for the centre of gravity the crew sets it for.
//!
//! Frame, units and signs: stations in metres aft of the nose tip, angles in
//! radians, incidences measured against the fuselage reference line with the
//! leading edge down negative (more download, the nose-up setting `n` of the
//! crews, `n = -i`). Lift coefficients are on their own reference areas; the
//! tail's contribution to the aircraft lift coefficient is `eta (S_h / S) CL_h`.
//!
//! Why derived: the stabiliser is trimmed for the takeoff CG, further
//! nose-up the further forward the CG (Airbus, *Safety First* #09, Feb. 2010,
//! "Incorrect pitch trim setting at takeoff": A330, 4.3 deg nose-up at
//! 26.3 %MAC, 4.1 deg at 26.7 %MAC and 6.4 deg at 22.2 %MAC, about
//! 0.5 deg per %MAC; the Airbus Safety First archive). No per-type take-off trim table
//! (FCOM / AMM) is public, so the setting is the incidence that trims the
//! aircraft with the elevator neutral at `V2`, and the A330 points above
//! validate it (see the `validation` test).
//!
//! Model, elevator neutral, `V2` on the speed rule the takeoff
//! performance flies (`PerformanceConfig::control_free_v2_over_vsr`, the CS
//! 25.107 rule without the control-speed branch: the selected factor, never
//! below `V2MIN = 1.13 V_SR` or `V_R`), `CL_V2 = CL_max,TO / (V2/V_SR)^2`:
//!
//! ```text
//! eta (S_h/S) CL_h = [c Cm_ac,wb + CL_V2 (x_cg - x_ac,wb) + CL_V2 (T/W) a_T]
//!                    / (x_h - x_ac,wb)
//! CL_w             = CL_V2 - eta (S_h/S) CL_h
//! alpha_b          = (CL_w - CL_g) / a_w           (CL_g at alpha_b = 0)
//! epsilon          = 2 CL_w / (pi A)               (no ground effect)
//! i_trim           = CL_h / a_h - alpha_b + epsilon
//! ```
//!
//! The first line is the moment balance about the CG with the weight carried
//! by wing and tail. `CL_g`, the wing-body lift at the level ground
//! attitude, is the one the rotation balance already uses (Torenbeek order of
//! magnitude); with it the model implies a flapped zero-lift angle of
//! `-CL_g / a_w`, about -10 deg for typical values, the take-off flap shift
//! of Raymer, *Aircraft Design: A Conceptual Approach*, sec. 12.4 (-10 deg at
//! the take-off setting). The downwash is the same gradient model the
//! rotation balance uses.
//!
//! The wing is the flapped one: `a_w`, `x_ac,wb` and `Cm_ac,wb` carry the
//! extended chord of the take-off flaps and slats at the take-off flap angle
//! ([`super::extended_chord`]; Raymer 6th ed., printed p. 413), the same
//! extended-chord lift the neutral point's high-lift condition uses, with
//! `CL_0 = CL_g`.
//!
//! Two stated simplifications of the thrust moment: the all-engine take-off
//! thrust over weight is taken at the rotation speed `V_R` (the rotation
//! model's thrust) while the dynamic pressure is that of `V2`, and the arm is the
//! height of the analysed take-off centre of gravity above the mean nacelle
//! centreline, both from the same mass state the rotation boundary uses.

/// Everything the trim balance and the rotation tail lift need.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TakeoffTrimModel {
    /// Lift coefficient at `V2`, `CL_max,TO / (V2/V_SR)^2` ([`v2_over_vsr`]).
    pub cl_v2: f64,
    /// Wing-body lift coefficient at the level ground attitude.
    pub cl_ground_attitude: f64,
    /// Lift coefficient of the climb-trim requirement,
    /// `CL_max,TO / 1.3^2` ([`CLIMB_TRIM_SPEED_OVER_VSR`]).
    pub cl_climb_trim: f64,
    /// Wing-body lift coefficient at zero fuselage angle of attack with the
    /// take-off flaps, out of ground effect (the climb is flown clear of it).
    pub cl_zero_alpha_free_air: f64,
    /// [`Self::cm_ac_wb`] with the extended-chord term on the free-air
    /// zero-attitude lift.
    pub cm_ac_wb_free_air: f64,
    /// Wing lift-curve slope with the take-off flaps' extended chord, per
    /// radian.
    pub wing_lift_curve_slope_per_rad: f64,
    /// Wing aspect ratio.
    pub wing_aspect_ratio: f64,
    /// Tail lift-curve slope, per radian.
    pub tail_lift_curve_slope_per_rad: f64,
    /// Horizontal-tail area over wing reference area.
    pub tail_area_ratio: f64,
    /// Tail dynamic-pressure efficiency.
    pub eta: f64,
    /// Wing-body aerodynamic-centre station with the take-off flaps'
    /// extended chord, m.
    pub x_ac_wb_m: f64,
    /// Horizontal-tail aerodynamic-centre station, m.
    pub x_h_ac_m: f64,
    /// Mean aerodynamic chord, m.
    pub mac_m: f64,
    /// Wing-body pitching-moment coefficient about its aerodynamic centre in
    /// the take-off configuration (about the flapped centre).
    pub cm_ac_wb: f64,
    /// All-engine take-off thrust over weight at the rotation speed; `NaN`
    /// takes no thrust moment.
    pub thrust_to_weight: f64,
    /// Height of the centre of gravity above the thrust line, m (positive:
    /// thrust below the CG, a nose-up moment).
    pub thrust_arm_m: f64,
    /// Built (chord-weighted mean) tail incidence, rad.
    pub built_incidence_rad: f64,
    /// Largest nose-up stabiliser setting, deg (mechanical travel); `None`
    /// is unlimited.
    pub max_nose_up_deg: Option<f64>,
    /// Downwash at the tail at the rotation attitude in ground effect, rad.
    pub rotation_downwash_rad: f64,
    /// Elevator term `tau_e delta_e` at full up-elevator, rad (negative).
    pub rotation_elevator_term_rad: f64,
    /// Largest download coefficient the tail supports, positive.
    pub rotation_section_cap: f64,
}

impl TakeoffTrimModel {
    /// Tail incidence that trims the aircraft at `V2` with the CG at
    /// `x_cg_m`, rad (negative for download). `NaN` for invalid inputs.
    #[must_use]
    pub fn trim_incidence_rad(&self, x_cg_m: f64) -> f64 {
        self.incidence_at(self.cl_v2, self.cl_ground_attitude, self.cm_ac_wb, x_cg_m)
    }

    /// Tail incidence that trims the aircraft at the climb-trim condition of
    /// CS 25.161(c)(1) (`1.3 V_SR1`, take-off flaps, out of ground effect) with
    /// the CG at `x_cg_m`, rad. `NaN` for invalid inputs.
    #[must_use]
    pub fn climb_trim_incidence_rad(&self, x_cg_m: f64) -> f64 {
        self.incidence_at(
            self.cl_climb_trim,
            self.cl_zero_alpha_free_air,
            self.cm_ac_wb_free_air,
            x_cg_m,
        )
    }

    /// The elevator-neutral trim incidence at lift coefficient `cl`, with the
    /// wing-body lift `cl_zero_alpha` at zero fuselage angle of attack and the
    /// moment `cm_ac_wb` about its aerodynamic centre (module balance).
    fn incidence_at(&self, cl: f64, cl_zero_alpha: f64, cm_ac_wb: f64, x_cg_m: f64) -> f64 {
        let arm = self.x_h_ac_m - self.x_ac_wb_m;
        let eta_r = self.eta * self.tail_area_ratio;
        if !(arm > 0.0 && eta_r > 0.0) {
            return f64::NAN;
        }
        // Thrust below the CG is a nose-up moment of `T/W * arm` weights; in
        // lift-coefficient terms (`q S = W / CL_V2`) it adds `CL_V2 T/W arm`.
        let thrust_moment = if self.thrust_to_weight.is_finite() && self.thrust_arm_m.is_finite() {
            cl * self.thrust_to_weight * self.thrust_arm_m
        } else {
            0.0
        };
        let tail_share =
            (self.mac_m * cm_ac_wb + cl * (x_cg_m - self.x_ac_wb_m) + thrust_moment) / arm;
        let cl_tail = tail_share / eta_r;
        let cl_wing = cl - tail_share;
        let alpha_body = (cl_wing - cl_zero_alpha) / self.wing_lift_curve_slope_per_rad;
        let downwash = 2.0 * cl_wing / (std::f64::consts::PI * self.wing_aspect_ratio);
        cl_tail / self.tail_lift_curve_slope_per_rad - alpha_body + downwash
    }

    /// The CG station, m, at which the elevator-neutral trim of the climb
    /// requirement needs exactly the nose-up stop (`None` without a finite
    /// stop or slope). CS 25.161(c)(1) asks for longitudinal trim in the climb
    /// with the take-off flaps "at a speed not more than 1.3 V_SR1": the
    /// aeroplane must be trimmable at `1.3 V_SR1`, not at every lower speed,
    /// so the limit is taken there, gear up and out of ground effect, and not
    /// at `V2`, where the pilot holds the elevator and no trim is required.
    /// The unclamped setting [`Self::climb_trim_incidence_rad`] is linear in
    /// the CG, so two evaluations fix the root. A CG further forward cannot
    /// be trimmed.
    #[must_use]
    pub fn stop_limited_cg_x_m(&self) -> Option<f64> {
        let stop_deg = self
            .max_nose_up_deg
            .filter(|deg| deg.is_finite() && *deg > 0.0)?;
        let setting = |x: f64| -self.climb_trim_incidence_rad(x).to_degrees();
        let (x0, x1) = (self.x_ac_wb_m, self.x_ac_wb_m + self.mac_m);
        let (s0, s1) = (setting(x0), setting(x1));
        let slope = (s1 - s0) / (x1 - x0);
        if !(s0.is_finite() && s1.is_finite() && slope < 0.0) {
            return None;
        }
        Some(x0 + (stop_deg - s0) / slope)
    }

    /// The nose-up stabiliser setting the crew selects for a CG at `x_cg_m`,
    /// deg: the trim incidence, at least the built incidence's download and
    /// at most the mechanical travel; never negative (a stabiliser is not
    /// set nose-down to remove download the CG needs).
    #[must_use]
    pub fn nose_up_setting_deg(&self, x_cg_m: f64) -> f64 {
        let trim_deg = -self.trim_incidence_rad(x_cg_m).to_degrees();
        if !trim_deg.is_finite() {
            return f64::NAN;
        }
        let setting = trim_deg.max(0.0);
        self.max_nose_up_deg
            .filter(|limit| limit.is_finite() && *limit > 0.0)
            .map_or(setting, |limit| setting.min(limit))
    }

    /// Tail lift coefficient at rotation with the stabiliser at its nose-up
    /// stop and full up-elevator, negative: the tail's maximum downlift,
    /// which Roskam's take-off rotation check takes at the most forward CG
    /// (*Airplane Design*), limited to the tail's stall
    /// ([`Self::rotation_section_cap`]). Without a stop the stall alone
    /// limits it.
    #[must_use]
    pub fn rotation_lift_coefficient_at_stop(&self) -> f64 {
        match self.max_nose_up_deg {
            Some(stop_deg) => self.rotation_lift_at_setting(stop_deg),
            None => -self.rotation_section_cap,
        }
    }

    /// Tail lift coefficient at rotation with the stabiliser at
    /// `nose_up_deg` and full up-elevator, negative; `NaN` for a non-finite
    /// setting or tail input (`f64::max` would return the cap for a `NaN`).
    fn rotation_lift_at_setting(&self, nose_up_deg: f64) -> f64 {
        if !nose_up_deg.is_finite() {
            return f64::NAN;
        }
        let incidence =
            super::rotation::takeoff_incidence_rad(self.built_incidence_rad, Some(nose_up_deg));
        let linear = super::rotation::tail_lift_coefficient(
            self.tail_lift_curve_slope_per_rad,
            incidence,
            self.rotation_downwash_rad,
            1.0,
            self.rotation_elevator_term_rad,
        );
        if linear.is_nan() {
            return f64::NAN;
        }
        linear.max(-self.rotation_section_cap)
    }
}

/// Speed of the climb-trim requirement over the reference stall speed:
/// CS 25.161(c)(1) (Amdt 28; 14 CFR 25.161(c)(1) is the same clause), a climb
/// at maximum continuous power "at a speed not more than 1.3 V_SR1" with the
/// landing gear retracted and the flaps in the take-off position.
pub(super) const CLIMB_TRIM_SPEED_OVER_VSR: f64 = 1.3;

/// `V2 / V_SR` of the takeoff-trim balance: the one the takeoff performance
/// flies, so the trim sees the same `V2` (for the ATR, whose `CL_max,TO` is
/// recovered from its published `V2` minimum on this rule, that `V2`). The
/// control-speed branch is left out as in the rotation speed: no preset
/// carries a sourced `V_MCA`, and the lowest `V2` is the most demanding trim.
pub(super) fn v2_over_vsr(config: &alas_config::AlasConfig) -> f64 {
    config.performance.control_free_v2_over_vsr()
}

/// The takeoff-trim model of `plane` when it has a trimmable stabiliser
/// whose setting is not fixed by the user: `config.landing_gear.
/// trimmable_stabilizer` set and `takeoff_stabilizer_nose_up_deg` unset.
/// `None` otherwise (fixed stabiliser, user-set setting, or no tail).
// One named physical input per term of the trim balance.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub(super) fn takeoff_trim_model(
    plane: &alas_geom::aircraft::airplane::Airplane,
    config: &alas_config::AlasConfig,
    rotation_tail: Option<super::rotation::RotationTailLift>,
    s_ref: f64,
    mac_m: f64,
    x_mac_le_m: f64,
    x_h_ac_m: f64,
    tail_area_ratio: f64,
    cl_ground_attitude: f64,
    thrust_to_weight: f64,
    thrust_arm_m: f64,
) -> Option<TakeoffTrimModel> {
    let gear = &config.landing_gear;
    if !gear.trimmable_stabilizer || gear.takeoff_stabilizer_nose_up_deg.is_some() {
        return None;
    }
    let tail = rotation_tail?;
    let wing = plane.wings.first()?;
    // On the reference wing: the plan buried in the body is not reference area.
    let wing_aspect_ratio = plane.b_ref * plane.b_ref / s_ref;
    let clean_slope = alas_stab::static_stability::datcom(
        wing_aspect_ratio,
        wing.mean_sweep_angle(0.25).to_radians(),
        wing.taper_ratio(),
        0.2,
    );
    let x_ac_clean_m = x_mac_le_m + super::support::wing_body_ac_frac(plane, config) * mac_m;
    let flapped = super::extended_chord::FlappedWingBody::new(
        clean_slope,
        clean_slope,
        x_ac_clean_m,
        mac_m,
        cl_ground_attitude,
        super::extended_chord::extended_lift(
            plane,
            config,
            super::extended_chord::takeoff_flap_deg(config),
        ),
    );
    let wing_slope = flapped.slope;
    // The climb is flown out of ground effect: the zero-attitude lift without
    // the ground's gain, and the extended-chord moment term on that lift.
    let cl_zero_alpha_free_air =
        super::support::takeoff_free_air_lift(plane, config, s_ref).unwrap_or(cl_ground_attitude);
    let cm_ac_takeoff = super::support::takeoff_cm_ac_wb(plane, config, s_ref);
    let cm_ac_shift_free_air = if cl_ground_attitude > 0.0 {
        flapped.cm_ac_shift * cl_zero_alpha_free_air / cl_ground_attitude
    } else {
        flapped.cm_ac_shift
    };
    let model = TakeoffTrimModel {
        cl_v2: config.performance.cl_max_to / v2_over_vsr(config).powi(2),
        cl_ground_attitude,
        cl_climb_trim: config.performance.cl_max_to / CLIMB_TRIM_SPEED_OVER_VSR.powi(2),
        cl_zero_alpha_free_air,
        cm_ac_wb_free_air: cm_ac_takeoff + cm_ac_shift_free_air,
        wing_lift_curve_slope_per_rad: wing_slope,
        wing_aspect_ratio,
        tail_lift_curve_slope_per_rad: tail.lift_curve_slope_per_rad,
        tail_area_ratio,
        eta: super::support::tail_efficiency(plane),
        x_ac_wb_m: x_ac_clean_m + flapped.ac_shift_m,
        x_h_ac_m,
        mac_m,
        cm_ac_wb: cm_ac_takeoff + flapped.cm_ac_shift,
        thrust_to_weight,
        thrust_arm_m,
        built_incidence_rad: tail.built_incidence_rad,
        max_nose_up_deg: Some(super::landing_download::stabilizer_nose_up_stop_deg(config)),
        rotation_downwash_rad: tail.downwash_rad,
        rotation_elevator_term_rad: tail.elevator_term_rad,
        rotation_section_cap: tail.section_cap,
    };
    let finite = [
        model.cl_v2,
        wing_slope,
        wing_aspect_ratio,
        tail.lift_curve_slope_per_rad,
        model.x_ac_wb_m,
        x_h_ac_m,
    ]
    .iter()
    .all(|value| value.is_finite());
    (finite && wing_slope > 0.0 && tail_area_ratio > 0.0).then_some(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A transport-like model: the setting grows as the CG moves forward.
    fn model() -> TakeoffTrimModel {
        TakeoffTrimModel {
            cl_v2: 1.8,
            cl_ground_attitude: 0.9,
            cl_climb_trim: 1.5,
            cl_zero_alpha_free_air: 0.8,
            cm_ac_wb_free_air: -0.15,
            wing_lift_curve_slope_per_rad: 5.0,
            wing_aspect_ratio: 9.0,
            tail_lift_curve_slope_per_rad: 3.2,
            tail_area_ratio: 0.2,
            eta: 0.9,
            x_ac_wb_m: 30.0,
            x_h_ac_m: 62.0,
            mac_m: 7.0,
            cm_ac_wb: -0.15,
            thrust_to_weight: 0.0,
            thrust_arm_m: 0.0,
            built_incidence_rad: 0.0,
            max_nose_up_deg: None,
            rotation_downwash_rad: 0.03,
            rotation_elevator_term_rad: -0.05,
            rotation_section_cap: 1.5,
        }
    }

    #[test]
    fn a_more_forward_cg_gets_a_more_nose_up_setting() {
        let m = model();
        let aft = m.nose_up_setting_deg(29.0);
        let fwd = m.nose_up_setting_deg(27.0);
        assert!(fwd > aft, "{fwd} {aft}");
    }

    /// The take-off trim limit is the CG at which the elevator-neutral climb
    /// trim (1.3 V_SR1, free air) needs exactly the stop; a CG forward of it
    /// needs more.
    #[test]
    fn the_stop_limited_cg_is_where_the_unclamped_trim_equals_the_stop() {
        let mut m = model();
        m.max_nose_up_deg = Some(8.0);
        let x = m.stop_limited_cg_x_m().expect("a finite stop and slope");
        let unclamped = |x: f64| -m.climb_trim_incidence_rad(x).to_degrees();
        assert!((unclamped(x) - 8.0).abs() < 1.0e-9);
        assert!(unclamped(x - 0.5) > 8.0 && unclamped(x + 0.5) < 8.0);
        m.max_nose_up_deg = None;
        assert!(m.stop_limited_cg_x_m().is_none());
    }

    #[test]
    fn the_travel_limit_clamps_and_never_goes_nose_down() {
        let mut m = model();
        m.max_nose_up_deg = Some(2.0);
        assert!(m.nose_up_setting_deg(20.0) <= 2.0 + 1.0e-12);
        let aft = m.nose_up_setting_deg(60.0);
        assert!(aft >= 0.0);
    }

    /// The rotation check's tail lift is the download at the stop, at least
    /// the crew's trim setting gives, and never past the stall cap; with no
    /// stop the cap alone limits it.
    #[test]
    fn the_rotation_lift_at_the_stop_is_the_maximum_downlift_within_the_cap() {
        let mut m = model();
        m.max_nose_up_deg = Some(4.0);
        let at_stop = m.rotation_lift_coefficient_at_stop();
        let at_trim = m.rotation_lift_at_setting(m.nose_up_setting_deg(31.0));
        assert!(at_stop <= at_trim, "{at_stop} {at_trim}");
        assert!(at_stop >= -m.rotation_section_cap - 1.0e-12);
        m.max_nose_up_deg = Some(90.0);
        assert!((m.rotation_lift_coefficient_at_stop() + m.rotation_section_cap).abs() < 1.0e-12);
        m.max_nose_up_deg = None;
        assert!((m.rotation_lift_coefficient_at_stop() + m.rotation_section_cap).abs() < 1.0e-12);
        m.max_nose_up_deg = Some(4.0);
        m.tail_lift_curve_slope_per_rad = f64::NAN;
        assert!(m.rotation_lift_coefficient_at_stop().is_nan());
    }

    /// Builds the trim model of a registered preset the way the envelope does.
    fn preset_model(name: &str) -> Option<(TakeoffTrimModel, f64, f64)> {
        use alas_geom::builder::AircraftBuilder;
        let config = alas_config::AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .expect("a registered preset configures");
        let registered = alas_config::presets::get(name).expect("a registered preset");
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&registered.design_vector), true)
            .expect("a registered preset builds");
        let fuselage = &plane.fuselages[0];
        let ground = super::super::support::ground_z_m(fuselage, &config);
        let s_ref = plane.wings.first()?.reference_area();
        let cl_ground = super::super::support::takeoff_ground_lift(&plane, &config, s_ref);
        let tail =
            super::super::rotation::rotation_tail_lift(&plane, &config, ground, cl_ground, 78.0)?;
        let frame = plane.mac_frame()?;
        // The CG height is approximated by the fuselage axis height at mid length.
        let h_cg = fuselage.xsecs[fuselage.xsecs.len() / 2].xyz_c[2] - ground;
        let thrust = super::super::rotation_thrust::RotationThrustModel::new(
            &plane,
            &config,
            ground,
            super::super::rotation_thrust::rotation_lift_coefficient(&config.performance),
        )
        .at_mass(config.requirements.mtow_kg);
        let model = takeoff_trim_model(
            &plane,
            &config,
            Some(tail),
            s_ref,
            frame.chord_m,
            frame.x_lemac_m,
            super::super::support::horizontal_tail_ac_x_m(&plane),
            super::super::support::tail_area_ratio(&plane, s_ref),
            cl_ground,
            thrust.thrust_to_weight,
            h_cg - thrust.thrust_line_height_m,
        )?;
        Some((model, frame.x_lemac_m, frame.chord_m))
    }

    // The validation against Airbus Safety First #09 (A330) uses the production
    // trim model: `mdo::residuals::ths_validation_tests`.

    /// Validation probe against Airbus Safety First #09 (A330): 6.4 deg at
    /// 22.2 %MAC, 4.3 deg at 26.3, 4.1 deg at 26.7. Prints the derived
    /// settings of every trimmable preset at the same CGs.
    #[test]
    #[ignore = "diagnostic table of all registered presets; prints"]
    // A probe prints its numbers; it asserts nothing.
    #[allow(clippy::print_stdout)]
    fn registered_preset_takeoff_trim_table() {
        for name in alas_config::presets::available() {
            let Some((model, le, mac)) = preset_model(name) else {
                println!("{name}: no trimmable stabiliser");
                continue;
            };
            let at = |pct: f64| {
                -model
                    .trim_incidence_rad(le + pct / 100.0 * mac)
                    .to_degrees()
            };
            println!(
                "{name}: nose-up trim 22.2%={:.2} 26.3%={:.2} 26.7%={:.2} deg; slope {:.3} deg/%MAC; CL_V2 {:.2} a_w {:.2} a_h {:.2} S_h/S {:.3}",
                at(22.2),
                at(26.3),
                at(26.7),
                (at(22.2) - at(26.7)) / 4.5,
                model.cl_v2,
                model.wing_lift_curve_slope_per_rad,
                model.tail_lift_curve_slope_per_rad,
                model.tail_area_ratio
            );
        }
    }
}
