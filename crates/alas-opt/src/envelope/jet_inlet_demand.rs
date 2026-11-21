// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Engine air flow of the jet-inlet normal-force term at the power setting
//! of each longitudinal-stability condition.
//!
//! CS 25.175 states the thrust each static-stability demonstration is flown
//! at:
//!
//! | Condition | Configuration and speed | Thrust |
//! |---|---|---|
//! | 25.175(a) climb | flaps and gear up, 1.3 V_SR1, MTOW | maximum continuous |
//! | 25.175(b) cruise | flaps and gear up, cruise point, MTOW | level flight: thrust = drag |
//! | maximum operating speed | flaps and gear up, V_MO/M_MO, MTOW | level flight: thrust = drag |
//! | 25.175(d) landing | landing flaps, gear down, V_REF = 1.23 V_SR0, MLW | 3 deg glide path: `T = W (CD/CL cos g - sin g)` |
//!
//! The maximum take-off mass is the heaviest (most thrust, most air) state
//! of each clean condition. The drag is the candidate's clean parabolic
//! polar plus, at landing, the declared flap and gear increments
//! (Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed., sect. 12.6).
//! The maximum continuous thrust comes from the engine deck
//! ([`PropulsionDeck`]); the air flow at each required thrust from the
//! cycle's throttled operating line
//! ([`alas_prop::cycle::operating_line::solve_operating_point_within_range`]:
//! the constant-turbine-flow method, so the flow falls with the turbine inlet
//! temperature as the engine is throttled). A demand above what the cycle
//! delivers at its take-off temperature takes that point (the engine at its
//! rating), one below the throttle range its lowest-temperature point.

use alas_atmo::Atmosphere;
use alas_config::AlasConfig;
use alas_prop::cycle::operating_line::solve_operating_point_within_range;
use alas_prop::cycle::TurbofanCycleInputs;
use alas_prop::system::PropulsionRating;
use alas_stab::neutral_point::{InletCondition, JetInletSpec};

use crate::mdo::propulsion::{max_climb_rate_ft_min, PropulsionDeck};

/// V_REF over the landing-configuration stall speed (CS 25.125(b)(2)).
const VREF_OVER_VSR0: f64 = 1.23;

/// The CS 25.175(a) climb speed over the clean stall speed.
const CLIMB_OVER_VSR1: f64 = 1.3;

/// The CS 25.175(d) glide-path angle, rad.
const GLIDE_PATH_RAD: f64 = 3.0 * std::f64::consts::PI / 180.0;

/// The clean parabolic drag polar `CD = cd0 + k CL^2` the thrust demand
/// reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InletDragPolar {
    /// Zero-lift drag coefficient.
    pub cd0: f64,
    /// Lift-dependent drag factor.
    pub k: f64,
}

/// The engine state of every neutral-point condition of a turbofan
/// configuration; `None` without a turbofan, or when the polar, the masses or
/// the engine cycle give no usable state.
#[must_use]
pub fn jet_inlet_spec(
    config: &AlasConfig,
    polar: InletDragPolar,
    s_ref_m2: f64,
) -> Option<JetInletSpec> {
    let engine = &config.geometry.engine;
    let spec = engine.turbofan.as_ref()?;
    let n_engines = engine.spanwise_positions_m.len();
    let perf = &config.performance;
    let req = &config.requirements;
    let g = req.gravity_m_s2;
    let mtow_kg = req.mtow_kg;
    let mlw_kg = config.landing_mass_limit_kg(mtow_kg);
    if n_engines == 0
        || !(polar.cd0.is_finite() && polar.cd0 > 0.0 && polar.k.is_finite() && polar.k > 0.0)
        || !(s_ref_m2.is_finite() && s_ref_m2 > 0.0)
        || !(mtow_kg.is_finite() && mtow_kg > 0.0 && mlw_kg.is_finite() && mlw_kg > 0.0)
    {
        return None;
    }
    let n = n_engines as f64;
    let rated_thrust_n = spec.rated_thrust_kn * 1_000.0;
    let flow_at = |mach: f64,
                   altitude_m: f64,
                   thrust_per_engine_n: f64|
     -> Option<InletCondition> {
        let design = TurbofanCycleInputs {
            mach,
            altitude_m,
            bypass_ratio: spec.bypass_ratio,
            overall_pressure_ratio: spec.overall_pressure_ratio,
            fan_pressure_ratio: spec.fan_pressure_ratio,
            turbine_inlet_temperature_k: spec.turbine_inlet_temp_k,
        };
        let cfg = &config.propulsion_cycle;
        let point =
            solve_operating_point_within_range(&design, rated_thrust_n, thrust_per_engine_n, cfg)
                .ok()?;
        (point.mass_flow_kg_s.is_finite() && point.mass_flow_kg_s > 0.0).then_some(InletCondition {
            mass_flow_kg_s: point.mass_flow_kg_s,
            mach,
            altitude_m,
        })
    };
    // Level flight at (mach, altitude) and the maximum take-off mass.
    let level = |mach: f64, altitude_m: f64| -> Option<InletCondition> {
        let atmosphere = Atmosphere::new(altitude_m);
        let speed = mach * atmosphere.speed_of_sound();
        let q = 0.5 * atmosphere.density() * speed * speed;
        let cl = mtow_kg * g / (q * s_ref_m2);
        let drag = q * s_ref_m2 * (polar.cd0 + polar.k * cl * cl);
        flow_at(mach, altitude_m, drag / n)
    };
    let sea_level = Atmosphere::new(0.0);
    let speed_at =
        |mass_kg: f64, cl: f64| (2.0 * mass_kg * g / (sea_level.density() * s_ref_m2 * cl)).sqrt();

    // 25.175(d): landing flaps, gear down, V_REF, 3 deg glide path.
    let cl_land = perf.cl_max_land / (VREF_OVER_VSR0 * VREF_OVER_VSR0);
    let v_ref = speed_at(mlw_kg, cl_land);
    let cd_land = polar.cd0
        + polar.k * cl_land * cl_land
        + perf.landing_flap_delta_cd
        + perf.landing_gear_delta_cd;
    let glide_thrust_n =
        mlw_kg * g * (cd_land / cl_land * GLIDE_PATH_RAD.cos() - GLIDE_PATH_RAD.sin());
    let approach = flow_at(v_ref / sea_level.speed_of_sound(), 0.0, glide_thrust_n / n)?;

    // 25.175(a): clean, 1.3 V_SR1, maximum continuous thrust.
    let cl_climb = perf.cl_max_clean / (CLIMB_OVER_VSR1 * CLIMB_OVER_VSR1);
    let v_climb = speed_at(mtow_kg, cl_climb);
    let deck = PropulsionDeck::from_engine(
        engine,
        req.cruise_mach,
        req.cruise_altitude_m,
        max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s),
    )
    .ok()?;
    let mct_total_n = deck
        .flight_condition(0.0, v_climb, g, 0.0)
        .and_then(|flight| deck.rated_point(flight, PropulsionRating::MaximumContinuous))
        .ok()?
        .thrust_n;
    let climb = flow_at(v_climb / sea_level.speed_of_sound(), 0.0, mct_total_n / n)?;

    let cruise = level(req.cruise_mach, req.cruise_altitude_m)?;
    let max_operating = config
        .max_operating_condition()
        .and_then(|(mach, altitude_m)| level(mach, altitude_m));
    Some(JetInletSpec {
        climb,
        approach,
        cruise,
        max_operating,
    })
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_jet_preset_has_an_engine_state_at_each_condition() {
        let polar = InletDragPolar {
            cd0: 0.020,
            k: 0.045,
        };
        for name in [
            "A320-200", "A340-300", "A380-800", "B747-400", "E195-E2", "LJ60",
        ] {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .expect("preset config");
            let preset = alas_config::presets::get(name).expect("preset");
            let s_ref = alas_geom::builder::AircraftBuilder::new(Some(preset.geometry.clone()))
                .build(Some(&preset.design_vector), true)
                .expect("preset builds")
                .s_ref;
            let spec = jet_inlet_spec(&config, polar, s_ref).expect(name);
            // The landing glide passes less air than maximum continuous climb.
            assert!(
                spec.approach.mass_flow_kg_s < spec.climb.mass_flow_kg_s,
                "{name}: {spec:?}"
            );
            for c in [spec.climb, spec.approach, spec.cruise] {
                assert!(c.mass_flow_kg_s > 0.0 && c.mach > 0.1, "{name}: {c:?}");
            }
        }
        let atr = AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" })).unwrap();
        assert!(jet_inlet_spec(&atr, polar, 61.0).is_none());
    }
}
