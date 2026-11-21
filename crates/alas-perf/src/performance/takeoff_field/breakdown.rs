// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Distance components of a solved take-off field (diagnostics): the ground
//! roll, rotation and air legs of the all-engine and continued take-offs, the
//! accelerate-stop legs, and the climb gradients at `V2`. It re-evaluates the
//! same model at the solution's `VR` and `V1`, so the parts add up to the
//! solved distances (asserted by the tests).

use super::{
    TakeoffField, TakeoffFieldInputs, SCREEN_HEIGHT_M, STOP_ALLOWANCE_S, STOP_TRANSITION_TIME_S,
};
use crate::performance::G;

/// Components of the solved field, all distances in m, gradients as
/// fractions (`(T - D)/W`, gear down, lift equal to weight, at `V2` and the
/// 35 ft screen above the lift-off position of the leg, so in ground effect
/// when the inputs carry it; the `_free_air` ones without).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldBreakdown {
    /// All-engine ground roll to `VR`.
    pub all_engine_roll_m: f64,
    /// All-engine rotation distance.
    pub all_engine_rotation_m: f64,
    /// All-engine air distance from lift-off to 35 ft.
    pub all_engine_air_m: f64,
    /// All-engine lift-off speed, m/s.
    pub all_engine_vlof_m_s: f64,
    /// All-engine climb gradient at `V2`.
    pub all_engine_gradient_v2: f64,
    /// Continued take-off: all-engine roll to `VEF`.
    pub continued_roll_all_engine_m: f64,
    /// Continued take-off: one-engine roll from `VEF` to `VR`.
    pub continued_roll_one_engine_m: f64,
    /// Continued take-off rotation distance.
    pub continued_rotation_m: f64,
    /// Continued take-off air distance to 35 ft.
    pub continued_air_m: f64,
    /// Continued take-off lift-off speed, m/s.
    pub continued_vlof_m_s: f64,
    /// Engine-out climb gradient at `V2`, gear down, at the screen height
    /// (in ground effect when the inputs carry it).
    pub engine_out_gradient_v2: f64,
    /// Engine-out gear-down gradient at `V2` out of ground effect: the
    /// quantity CS 25.121(a) and (b) are defined for.
    pub engine_out_gradient_v2_free_air: f64,
    /// Accelerate-stop: all-engine roll to `VEF`.
    pub stop_roll_all_engine_m: f64,
    /// Accelerate-stop: one-engine roll from `VEF` to `V1`.
    pub stop_roll_one_engine_m: f64,
    /// Accelerate-stop: braking from `V1` at the mean deceleration.
    pub stop_braking_m: f64,
    /// Accelerate-stop: the 2 s allowance and the pilot-action transition at `V1`.
    pub stop_allowance_m: f64,
    /// Pitch margin to tail contact at lift-off, degrees: the tail-strike
    /// attitude less the lift-off attitude, the smaller of the all-engine and
    /// continued take-offs; `None` without a lift-off schedule.
    pub tail_margin_deg: Option<f64>,
}

/// Re-evaluate the legs of `solved` for the same `inputs`.
///
/// # Errors
///
/// A thrust callback failure or a take-off that cannot accelerate.
pub fn field_breakdown(
    inputs: &TakeoffFieldInputs<'_>,
    solved: &TakeoffField,
) -> Result<FieldBreakdown, String> {
    let (model, _, v2) = super::search_model(inputs)?;
    let aeo = model.roll(v2, false)?;
    let oei = model.roll(v2, true)?;
    let (vr, v1, vef) = (solved.vr_m_s, solved.v1_m_s, solved.vef_m_s);
    let (vlof_a, rot_a) = model.rotation(vr, false);
    let (vlof_o, rot_o) = model.rotation(vr, true);
    let screen = |vlof: f64| model.liftoff_rise(vlof).raised(SCREEN_HEIGHT_M);
    let stop_a = inputs.stop_deceleration_g * G;
    Ok(FieldBreakdown {
        all_engine_roll_m: aeo.distance(vr),
        all_engine_rotation_m: rot_a,
        all_engine_air_m: model.air(vlof_a, false),
        all_engine_vlof_m_s: vlof_a,
        all_engine_gradient_v2: model.gradient(v2, false, screen(vlof_a)),
        continued_roll_all_engine_m: aeo.distance(vef),
        continued_roll_one_engine_m: oei.distance(vr) - oei.distance(vef),
        continued_rotation_m: rot_o,
        continued_air_m: model.air(vlof_o, true),
        continued_vlof_m_s: vlof_o,
        engine_out_gradient_v2: model.gradient(v2, true, screen(vlof_o)),
        engine_out_gradient_v2_free_air: model.gradient_free_air(v2, true),
        stop_roll_all_engine_m: aeo.distance(vef),
        stop_roll_one_engine_m: oei.distance(v1) - oei.distance(vef),
        stop_braking_m: v1 * v1 / (2.0 * stop_a),
        stop_allowance_m: (STOP_ALLOWANCE_S + STOP_TRANSITION_TIME_S) * v1,
        tail_margin_deg: model
            .tail_margin_deg(vlof_a)
            .zip(model.tail_margin_deg(vlof_o))
            .map(|(aeo, oei)| aeo.min(oei)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::performance::VSpeeds;

    #[test]
    fn the_legs_add_up_to_the_solved_distances() -> Result<(), String> {
        let weight_n = 78_000.0 * 9.81;
        let aeo = move |_: f64| Ok(0.30 * weight_n);
        let oei = move |_: f64| Ok(0.15 * weight_n);
        let inputs = TakeoffFieldInputs {
            weight_n,
            wing_area_m2: 122.0,
            density_kg_m3: 1.225,
            speeds: VSpeeds {
                v_stall_to_ms: 65.0,
                v_stall_land_ms: 60.0,
                v_mc_ms: 60.0,
                v1_ms: 70.0,
                v_r_ms: 71.5,
                v2_ms: 78.0,
                v_app_ms: 74.0,
                v_td_ms: 69.0,
            },
            rolling_friction: 0.02,
            // Needs data: the accelerate-stop deceleration of the Airbus chart (AFM or Airbus data, not on disk); 0.37 g is the Torenbeek mean.
            stop_deceleration_g: 0.37,
            cd0: 0.04,
            gear_delta_cd: 0.02,
            lift_drag: &|cl| 0.045 * cl * cl,
            ground_effect: None,
            failed_engine_cd: 0.006,
            thrust_aeo_n: &aeo,
            thrust_oei_n: &oei,
            airborne_drag: None,
            liftoff: None,
        };
        let solved = super::super::takeoff_field(&inputs)?;
        let b = field_breakdown(&inputs, &solved)?;
        let all_engine = b.all_engine_roll_m + b.all_engine_rotation_m + b.all_engine_air_m;
        assert!((all_engine / solved.all_engine_m - 1.0).abs() < 1e-9);
        let continued = b.continued_roll_all_engine_m
            + b.continued_roll_one_engine_m
            + b.continued_rotation_m
            + b.continued_air_m;
        assert!((continued / solved.continued_m - 1.0).abs() < 1e-9);
        let stop = b.stop_roll_all_engine_m
            + b.stop_roll_one_engine_m
            + b.stop_braking_m
            + b.stop_allowance_m;
        assert!((stop / solved.accelerate_stop_m - 1.0).abs() < 1e-9);
        Ok(())
    }
}
