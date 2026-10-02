// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Propulsion-specific preliminary field estimates with explicit regulatory bases.
//!
//! Torenbeek, Synthesis of Subsonic Airplane Design (1982), Secs. 5.4.5-5.4.6,
//! pp. 167-170, and Appendix K underpin the propeller model. The BFL equation
//! uses an energy-equivalent mean acceleration from the integrated 0-to-V2
//! ground roll in place of the source's 0-to-V1 mean. Rotation and the critical
//! V1 solution remain preliminary approximations. The point OEI gradient at
//! V2 replaces the source's equivalent gradient over takeoff phases 1..2.
//! These are conceptual
//! estimates on a level dry hard runway, without wind, clearway or reverse thrust.
//! They do not establish certified V1, VMCG, brake-energy limits or field distances.

use alas_config::airports::Airport;
use alas_config::PerformanceConfig;

use super::{
    compute_field_performance_at_masses, compute_v_speeds_at_masses, density_ratio,
    FieldPerformance, VSpeeds, G, RHO_SL,
};

/// Modern non-icing reference-speed floor, 14 CFR 25.125(b)(2)(i)(A).
/// Taking VSR0 = VS1g is a preliminary approximation, not a certified stall test.
pub const VREF_OVER_VS1G: f64 = 1.23;

/// Physical and configuration inputs shared by the jet and propeller methods.
#[derive(Debug, Clone, Copy)]
pub struct FieldInputs<'a> {
    /// Departure mass, kg.
    pub takeoff_mass_kg: f64,
    /// Arrival or maximum landing mass, kg.
    pub landing_mass_kg: f64,
    /// Reference wing area, m^2.
    pub wing_area_m2: f64,
    /// Field atmosphere and available distances.
    pub airport: &'a Airport,
    /// Maximum lift coefficient in takeoff configuration.
    pub cl_max_to: f64,
    /// Maximum lift coefficient in landing configuration.
    pub cl_max_land: f64,
    /// Sea-level static installed thrust divided by departure weight.
    pub sea_level_static_tw: f64,
    /// Legacy jet actual-landing-distance coefficient, m/Pa.
    pub landing_distance_factor: f64,
    /// Declared field assumptions and explicit legacy selection.
    pub config: &'a PerformanceConfig,
}

/// Propulsion evidence used by the selected preliminary method.
pub enum FieldPropulsion<'a> {
    /// Raymer FAR-25 jet transport takeoff-parameter correlation.
    Jet,
    /// Torenbeek propeller estimate using installed thrust at every roll speed.
    Propeller {
        /// Number of identical operating engines before the critical failure.
        engine_count: usize,
        /// Total all-engine installed thrust, N, at true airspeed in m/s.
        /// The caller supplies the actual field density and power-rating lapse.
        thrust_n: &'a dyn Fn(f64) -> Result<f64, String>,
        /// Total thrust with one engine unavailable at the stated airspeed.
        /// Supply a callback when the remaining engines use a reserve rating.
        /// None assumes the same rating and scales AEO thrust by (N-1)/N.
        engine_out_thrust_n: Option<&'a dyn Fn(f64) -> Result<f64, String>>,
        /// Low-speed zero-lift drag coefficient in the takeoff configuration.
        /// The caller supplies high-lift and gear increments or states their omission.
        zero_lift_drag_coefficient: f64,
        /// Low-speed quadratic induced-drag factor.
        induced_drag_factor: f64,
        /// Additional OEI drag for trim, gear and failed-engine/propeller state.
        /// Zero requires an explicit conceptual assumption in the calling layer.
        failed_engine_drag_coefficient: f64,
    },
}

/// Compute preliminary distances using a method appropriate to the propulsion.
///
/// The jet TOP coefficient 37.7 already estimates FAR-25 field length (Raymer,
/// 5th ed., Sec. 5.4, Fig. 5.4). No second 1.15 multiplier is applied. Under
/// 14 CFR 25.113(a), 1.15 applies to the all-engine distance, compared against
/// the engine-out distance; it is not a universal balanced-field multiplier.
/// The propeller result applies that factor once to its all-engine distance.
/// `ldr_m` remains actual distance from the 50 ft screen under 25.125(a),
/// before the separate operational landing-field factor.
pub fn compute_field_performance_for_propulsion(
    inputs: FieldInputs<'_>,
    propulsion: FieldPropulsion<'_>,
) -> Result<FieldPerformance, String> {
    validate_inputs(inputs)?;
    let mut field = compute_field_performance_at_masses(
        inputs.takeoff_mass_kg,
        inputs.landing_mass_kg,
        inputs.wing_area_m2,
        inputs.airport,
        inputs.cl_max_to,
        inputs.cl_max_land,
        inputs.sea_level_static_tw,
        inputs.landing_distance_factor,
        inputs.config.bfl_factor,
        inputs.config,
    );
    if inputs.config.legacy_field_correlations {
        return Ok(field);
    }
    field.v_speeds = reference_speeds(inputs)?;
    match propulsion {
        FieldPropulsion::Jet => {
            field.bfl_m = field.todr_m;
            // TOP does not independently resolve accelerate-stop: this remains
            // the same conceptual field proxy and must be labelled by callers.
            field.asd_m = field.todr_m;
        }
        FieldPropulsion::Propeller {
            engine_count,
            thrust_n,
            engine_out_thrust_n,
            zero_lift_drag_coefficient,
            induced_drag_factor,
            failed_engine_drag_coefficient,
        } => {
            let polar = TakeoffPolar {
                zero_lift_drag_coefficient,
                induced_drag_factor,
                failed_engine_drag_coefficient,
            };
            let (takeoff_distance_m, balanced_distance_m) = propeller_takeoff(
                inputs,
                field.v_speeds,
                engine_count,
                thrust_n,
                engine_out_thrust_n,
                polar,
            )?;
            field.todr_m = takeoff_distance_m;
            field.bfl_m = balanced_distance_m;
            // Torenbeek Eq. 5-89 balances go and stop within its mean-acceleration
            // assumptions. The result is still not a certified accelerate-stop.
            field.asd_m = balanced_distance_m;
            field.ldr_m = propeller_landing(inputs, field.v_speeds)?;
        }
    }
    Ok(field)
}

/// Evaluate the propeller actual landing distance independently of OEI takeoff.
///
/// This permits validation of landing evidence for an aircraft whose engine
/// count or engine-out capability is outside the balanced-field model's domain.
/// The distance has the 50 ft screen basis of 25.125(a); no dispatch factor is
/// included, and its evaluation does not imply Part 25 certification eligibility.
pub fn compute_propeller_landing_distance(inputs: FieldInputs<'_>) -> Result<f64, String> {
    validate_inputs(inputs)?;
    propeller_landing(inputs, reference_speeds(inputs)?)
}

fn reference_speeds(inputs: FieldInputs<'_>) -> Result<VSpeeds, String> {
    let mut speeds = compute_v_speeds_at_masses(
        inputs.takeoff_mass_kg,
        inputs.landing_mass_kg,
        inputs.wing_area_m2,
        inputs.airport,
        inputs.cl_max_to,
        inputs.cl_max_land,
        inputs.config,
    );
    speeds.v_app_ms = VREF_OVER_VS1G * speeds.v_stall_land_ms;
    for (name, value) in [
        ("takeoff stall speed", speeds.v_stall_to_ms),
        ("landing stall speed", speeds.v_stall_land_ms),
        ("minimum control speed", speeds.v_mc_ms),
        ("decision speed proxy", speeds.v1_ms),
        ("rotation speed", speeds.v_r_ms),
        ("takeoff safety speed", speeds.v2_ms),
        ("reference approach speed", speeds.v_app_ms),
        ("touchdown speed", speeds.v_td_ms),
    ] {
        positive(name, value)?;
    }
    if speeds.v_r_ms <= speeds.v_stall_to_ms
        || speeds.v1_ms > speeds.v_r_ms
        || speeds.v2_ms < speeds.v_r_ms
    {
        return Err("takeoff schedule must have VR above stall and V1 <= VR <= V2".to_owned());
    }
    if speeds.v_td_ms <= speeds.v_stall_land_ms || speeds.v_td_ms > speeds.v_app_ms {
        return Err("touchdown speed must lie above stall and at or below Vref".to_owned());
    }
    Ok(speeds)
}

fn validate_inputs(inputs: FieldInputs<'_>) -> Result<(), String> {
    for (name, value) in [
        ("takeoff mass", inputs.takeoff_mass_kg),
        ("landing mass", inputs.landing_mass_kg),
        ("wing area", inputs.wing_area_m2),
        ("takeoff CLmax", inputs.cl_max_to),
        ("landing CLmax", inputs.cl_max_land),
        ("static thrust/weight", inputs.sea_level_static_tw),
        ("jet landing factor", inputs.landing_distance_factor),
        (
            "density ratio",
            density_ratio(inputs.airport.elevation_m, inputs.airport.isa_deviation_c),
        ),
    ] {
        positive(name, value)?;
    }
    if inputs.landing_mass_kg > inputs.takeoff_mass_kg {
        return Err("landing mass exceeds takeoff mass".to_owned());
    }
    Ok(())
}

fn positive(name: &str, value: f64) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(format!("{name} must be finite and positive, got {value}"))
    }
}

#[derive(Clone, Copy)]
struct TakeoffPolar {
    zero_lift_drag_coefficient: f64,
    induced_drag_factor: f64,
    failed_engine_drag_coefficient: f64,
}

fn propeller_takeoff(
    inputs: FieldInputs<'_>,
    speeds: VSpeeds,
    engine_count: usize,
    thrust_n: &dyn Fn(f64) -> Result<f64, String>,
    engine_out_thrust_n: Option<&dyn Fn(f64) -> Result<f64, String>>,
    polar: TakeoffPolar,
) -> Result<(f64, f64), String> {
    if engine_count < 2 {
        return Err("balanced propeller field estimate requires at least two engines".to_owned());
    }
    let config = inputs.config;
    for (name, value) in [
        (
            "rolling friction",
            config.propeller_takeoff_rolling_friction,
        ),
        (
            "stopping deceleration",
            config.propeller_takeoff_stop_deceleration_g,
        ),
        (
            "takeoff inertia distance",
            config.propeller_takeoff_inertia_distance_m,
        ),
        ("induced drag factor", polar.induced_drag_factor),
    ] {
        positive(name, value)?;
    }
    for value in [
        polar.zero_lift_drag_coefficient,
        polar.failed_engine_drag_coefficient,
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err("takeoff drag increments must be finite and nonnegative".to_owned());
        }
    }
    let weight_n = inputs.takeoff_mass_kg * G;
    let sigma = density_ratio(inputs.airport.elevation_m, inputs.airport.isa_deviation_c);
    let density_kg_m3 = RHO_SL * sigma;
    let cl_v2 = inputs.cl_max_to * (speeds.v_stall_to_ms / speeds.v2_ms).powi(2);
    // Torenbeek p. 167 uses twice the CL minimizing CD - mu CL. For a
    // quadratic polar this is mu/k, capped before lift exceeds weight at V2.
    let cl_roll =
        (config.propeller_takeoff_rolling_friction / polar.induced_drag_factor).min(cl_v2);
    let cd0 = polar.zero_lift_drag_coefficient;
    let cd_roll = cd0 + polar.induced_drag_factor * cl_roll.powi(2);
    let acceleration_g = |speed_m_s: f64| -> Result<f64, String> {
        let thrust = thrust_n(speed_m_s)?;
        positive("installed takeoff thrust", thrust)?;
        let dynamic_pressure_pa = 0.5 * density_kg_m3 * speed_m_s.powi(2);
        let acceleration = thrust / weight_n
            - config.propeller_takeoff_rolling_friction
            - dynamic_pressure_pa * inputs.wing_area_m2 / weight_n
                * (cd_roll - config.propeller_takeoff_rolling_friction * cl_roll);
        positive("ground-roll acceleration / g", acceleration)?;
        Ok(acceleration)
    };
    let roll_distance_m = ground_roll_distance_m(speeds.v2_ms, acceleration_g)?;
    let equivalent_acceleration_g = speeds.v2_ms.powi(2) / (2.0 * G * roll_distance_m);
    let climb_drag_ratio = (cd0 + polar.induced_drag_factor * cl_v2.powi(2)) / cl_v2;
    let thrust_v2 = thrust_n(speeds.v2_ms)?;
    let all_engine_gradient = thrust_v2 / weight_n - climb_drag_ratio;
    positive("all-engine climb gradient", all_engine_gradient)?;
    let oei_thrust_v2 = match engine_out_thrust_n {
        Some(evaluate) => evaluate(speeds.v2_ms)?,
        None => thrust_v2 * (engine_count - 1) as f64 / engine_count as f64,
    };
    positive("installed engine-out thrust at V2", oei_thrust_v2)?;
    let engine_out_gradient =
        oei_thrust_v2 / weight_n - climb_drag_ratio - polar.failed_engine_drag_coefficient / cl_v2;
    if !engine_out_gradient.is_finite() || engine_out_gradient <= 0.0 {
        return Err(format!(
            "engine-out climb gradient {engine_out_gradient:.6} must be positive: \
             AEO thrust at V2 {thrust_v2:.1} N, OEI thrust {oei_thrust_v2:.1} N, \
             weight {weight_n:.1} N, \
             V2 {:.3} m/s, CL at V2 {cl_v2:.6}, takeoff CD0 {cd0:.6}, \
             induced factor {:.6}, climb D/W {climb_drag_ratio:.6}, \
             failed-engine delta CD {:.6}, engines {engine_count}",
            speeds.v2_ms, polar.induced_drag_factor, polar.failed_engine_drag_coefficient
        ));
    }
    let screen_height_m = 35.0 * 0.3048;
    let stop_g = config.propeller_takeoff_stop_deceleration_g;
    let inertia_m = config.propeller_takeoff_inertia_distance_m / sigma.sqrt();
    // Torenbeek Eq. 5-89, p. 169, before the jet-specific Eq. 5-91
    // simplification. Replacing the source's 0-to-V1 mean by the integrated
    // 0-to-V2 energy mean retains thrust lapse but is a preliminary extension.
    // The point V2 OEI gradient also approximates the source's equivalent
    // gradient over phases 1..2; no phase-resolved OEI trajectory is solved.
    let balanced_distance_m = (speeds.v2_ms.powi(2) / (2.0 * G) + screen_height_m)
        / (1.0 + engine_out_gradient / stop_g)
        * (1.0 / equivalent_acceleration_g + 1.0 / stop_g)
        + inertia_m;
    // The energy balance with VLOF = V2 supplies the all-engine screen
    // distance, assuming instantaneous rotation. The BFL inertia allowance
    // belongs only to Eq. 5-89. 25.113(a)(2) applies to this candidate once.
    let factored_all_engine_m = 1.15 * (roll_distance_m + screen_height_m / all_engine_gradient);
    Ok((
        factored_all_engine_m.max(balanced_distance_m),
        balanced_distance_m,
    ))
}

fn ground_roll_distance_m(
    terminal_speed_m_s: f64,
    acceleration_g: impl Fn(f64) -> Result<f64, String>,
) -> Result<f64, String> {
    // Composite Simpson integration of Torenbeek Eqs. 5-73/5-74; the fixed
    // even grid is a numerical resolution, not a physical coefficient.
    const INTERVALS: usize = 256;
    let step = terminal_speed_m_s / INTERVALS as f64;
    let integrand = |speed| -> Result<f64, String> { Ok(speed / (G * acceleration_g(speed)?)) };
    let mut integral = integrand(0.0)? + integrand(terminal_speed_m_s)?;
    for index in 1..INTERVALS {
        let simpson_weight = if index % 2 == 0 { 2.0 } else { 4.0 };
        integral += simpson_weight * integrand(index as f64 * step)?;
    }
    Ok(integral * step / 3.0)
}

fn propeller_landing(inputs: FieldInputs<'_>, speeds: VSpeeds) -> Result<f64, String> {
    let config = inputs.config;
    positive(
        "landing mean excess drag/weight",
        config.propeller_landing_mean_drag_to_weight,
    )?;
    positive(
        "landing mean deceleration",
        config.propeller_landing_deceleration_g,
    )?;
    // Torenbeek Eqs. 5-93/5-94, p. 170. The published mean stopping
    // deceleration includes inertia; adding a separate brake delay doubles it.
    let air_distance_m = (50.0 * 0.3048
        + (speeds.v_app_ms.powi(2) - speeds.v_td_ms.powi(2)) / (2.0 * G))
        / config.propeller_landing_mean_drag_to_weight;
    let roll_distance_m =
        speeds.v_td_ms.powi(2) / (2.0 * G * config.propeller_landing_deceleration_g);
    Ok(air_distance_m + roll_distance_m)
}

#[cfg(test)]
mod tests {
    use super::{ground_roll_distance_m, G};

    #[test]
    fn integrated_roll_matches_constant_acceleration_energy_balance() -> Result<(), String> {
        let terminal_speed_m_s = 60.0_f64;
        let acceleration_g = 0.20;
        let integrated = ground_roll_distance_m(terminal_speed_m_s, |_| Ok(acceleration_g))?;
        let energy_distance = terminal_speed_m_s.powi(2) / (2.0 * G * acceleration_g);
        // Simpson integrates this linear integrand exactly except for rounding.
        assert!((integrated / energy_distance - 1.0).abs() < 1.0e-13);
        Ok(())
    }
}
