// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The jet take-off field length of the optimiser constraint and the reports.
//!
//! One definition: [`alas_perf::performance::takeoff_field`] (CS 25.113(a)
//! with the CS 25.109 accelerate-stop balance) fed by the engine deck thrust
//! at every speed (take-off rating, Jenkinson take-off chart), the failed
//! engine's windmilling plus trim drag from the shared engine-out evidence
//! builder, and the candidate drag in the low-speed quadratic form of
//! [`field_polar`]. The optimiser residual `takeoff_field`, the airport
//! feasibility finding, the ISA sea-level field reference and the report
//! inputs all call [`jet_takeoff_field`].

use alas_config::airports::Airport;
use alas_config::{AlasConfig, PerformanceConfig};
use alas_perf::performance::{
    compute_v_speeds_at_masses, density_ratio, takeoff_field, tw_takeoff_constraint, TakeoffField,
    TakeoffFieldInputs, VSpeeds,
};

use super::improved_climb::{resolve_improved_climb, ImprovedClimbInputs};
use super::oei_deck::{EngineOutCondition, EngineOutModel, EngineOutRating, YawAirframe};
use crate::mdo::sizing::SizingOutcome;
use crate::mdo::types::{CandidateDrag, ConstraintFamily, ConstraintResidual};
use crate::mdo::ResidualRole;

/// Standard gravity used for the weight, m/s^2 (the program-wide `g0`).
const G: f64 = alas_atmo::STANDARD_GRAVITY;

/// Low-speed quadratic field polar `(CD0, k)` of a candidate's drag model.
///
/// Three samples of the airport-base drag (no lift-dependent viscous term)
/// at the field atmosphere and `mach`; a positive linear lift term is bounded
/// conservatively over `0..CLmax_TO`, a negative one receives no credit.
///
/// # Errors
///
/// The fit is not finite and positive.
pub fn field_polar(
    drag: &CandidateDrag,
    perf: &PerformanceConfig,
    mach: f64,
    airport: &Airport,
) -> Result<(f64, f64), String> {
    let cd = |cl| {
        drag.cd_airport_base_at_atmosphere(cl, mach, airport.elevation_m, airport.isa_deviation_c)
    };
    let (c0, c_half, c_one) = (cd(0.0), cd(0.5), cd(1.0));
    let k = 2.0 * (c_one - 2.0 * c_half + c0);
    let c1 = c_one - c0 - k;
    let cd0 = c0 + c1.max(0.0) * perf.cl_max_to;
    if !(cd0.is_finite() && cd0 > 0.0 && k.is_finite() && k > 0.0) {
        return Err(
            "candidate field polar needs finite positive CD0 and induced factor".to_owned(),
        );
    }
    Ok((cd0, k))
}

/// The speed schedule of the field-length row at the design gross mass, with
/// the improved-climb `V2` resolved at that mass (`base` is the configuration
/// with the computed `VMC` and no improved-climb override). `None` when the
/// engine-out evidence cannot be bound.
pub(super) fn design_mass_speeds(
    base: &AlasConfig,
    outcome: &SizingOutcome,
    departure: &Airport,
    s_ref_m2: f64,
    gravity_m_s2: f64,
) -> Option<VSpeeds> {
    let sized = &outcome.sized;
    let mass_kg = sized.design_gross_mass_kg;
    let static_thrust_n = outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0;
    let model = EngineOutModel::new(
        base,
        Some(YawAirframe {
            plane: &outcome.plane,
            cg_x_m: outcome.cg_x,
        }),
        s_ref_m2,
        Some(outcome.static_thrust_kn * 1_000.0),
    )
    .ok()?
    .with_weight_kg(mass_kg);
    let (config, _) = resolve_improved_climb(
        base,
        &ImprovedClimbInputs {
            model: &model,
            departure,
            mass_kg,
            s_ref_m2,
            drag: &sized.fuel_artifacts.drag,
            installed_tw_sls: static_thrust_n / (mass_kg * gravity_m_s2),
        },
    );
    let perf = &config.performance;
    Some(compute_v_speeds_at_masses(
        mass_kg,
        sized.design_landing_mass_kg,
        s_ref_m2,
        departure,
        perf.cl_max_to,
        perf.cl_max_land,
        perf,
    ))
}

/// Inputs of [`jet_takeoff_field`].
pub struct JetFieldInputs<'a> {
    /// Performance configuration (friction, braking, drag increments).
    pub perf: &'a PerformanceConfig,
    /// Engine deck and airframe evidence, weight bound.
    pub engine_out: &'a EngineOutModel,
    /// Departure aerodrome (elevation and ISA deviation).
    pub airport: &'a Airport,
    /// Reference wing area, m^2.
    pub wing_area_m2: f64,
    /// Take-off mass, kg.
    pub mass_kg: f64,
    /// Installed sea-level static thrust of all engines, N.
    pub static_thrust_n: f64,
    /// Clean field polar `(CD0, k)` from [`field_polar`].
    pub polar: (f64, f64),
    /// Speed schedule at this mass and field (improved-climb V2 included).
    pub speeds: VSpeeds,
}

/// Solve the CS 25.113(a) field length of a jet.
///
/// # Errors
///
/// An engine-deck or drag-evidence failure, a missing asymmetric trim drag,
/// or a take-off the physics cannot complete (as text).
pub fn jet_takeoff_field(inputs: &JetFieldInputs<'_>) -> Result<TakeoffField, String> {
    let perf = inputs.perf;
    let airport = inputs.airport;
    let model = inputs.engine_out;
    let n = model.n_engines();
    if n < 2 {
        return Err("balanced field length needs at least two engines".to_owned());
    }
    let atmosphere = alas_atmo::us1976_compute_values(airport.elevation_m, airport.isa_deviation_c);
    let at = |tas_m_s| EngineOutCondition {
        altitude_m: airport.elevation_m,
        isa_deviation_c: airport.isa_deviation_c,
        tas_m_s,
    };
    let (cd0, k) = inputs.polar;
    let point = model.point(
        at(inputs.speeds.v2_ms),
        EngineOutRating::TakeoffGoAround,
        cd0,
    )?;
    let trim = point.asymmetric_trim_cd.ok_or_else(|| {
        "jet field length needs the asymmetric trim drag: configure oei_asymmetric_trim_cd \
         or evaluate with an airframe that has a fin"
            .to_owned()
    })?;
    let aeo = |v: f64| {
        let ratio = model
            .thrust(at(v), EngineOutRating::TakeoffGoAround, false)?
            .1;
        Ok(ratio * inputs.static_thrust_n)
    };
    let oei = |v: f64| aeo(v).map(|thrust| thrust * (n - 1) as f64 / n as f64);
    takeoff_field(&TakeoffFieldInputs {
        weight_n: inputs.mass_kg * G,
        wing_area_m2: inputs.wing_area_m2,
        density_kg_m3: atmosphere.density_kg_m3,
        speeds: inputs.speeds,
        rolling_friction: perf.propeller_takeoff_rolling_friction,
        stop_deceleration_g: perf.propeller_takeoff_stop_deceleration_g,
        cd0: cd0 + perf.oei_climb_delta_cd,
        gear_delta_cd: perf.takeoff_gear_delta_cd,
        induced_factor: k,
        failed_engine_cd: point.windmilling_cd + trim,
        thrust_aeo_n: &aeo,
        thrust_oei_n: &oei,
    })
}

/// Everything the `takeoff_field` residual reads.
pub(super) struct ResidualInputs<'a> {
    pub(super) perf: &'a PerformanceConfig,
    pub(super) engine_out: Option<&'a EngineOutModel>,
    pub(super) drag: &'a CandidateDrag,
    pub(super) departure: &'a Airport,
    pub(super) speeds: Option<VSpeeds>,
    pub(super) s_ref_m2: f64,
    /// Design gross mass (the mass the field is certified at), kg.
    pub(super) mass_kg: f64,
    /// Installed sea-level static thrust of all engines, N.
    pub(super) static_thrust_n: f64,
    pub(super) gravity_m_s2: f64,
    pub(super) role: ResidualRole,
}

/// The `takeoff_field` residual.
///
/// Jets: the CS 25.113 field length against the take-off distance available,
/// in metres (violation = `(TOFL - TODA) / TOFL`). Turboprops and the
/// translated replay keep the Raymer required-versus-installed `T/W`
/// correlation (`legacy_field_correlations`; the turboprop field is judged on
/// the Torenbeek integration in the pipeline). A jet whose field cannot be
/// evaluated because the continued take-off has no climb gradient is an
/// infinite violation; a missing evidence term is a Diagnostic gap.
pub(super) fn takeoff_field_row(inputs: &ResidualInputs<'_>) -> ConstraintResidual {
    let perf = inputs.perf;
    let departure = inputs.departure;
    let physical = !perf.legacy_field_correlations
        && inputs.engine_out.is_some_and(|model| !model.is_turboprop());
    let (Some(model), Some(speeds), true) = (inputs.engine_out, inputs.speeds, physical) else {
        return correlation_row(inputs);
    };
    let atmosphere =
        alas_atmo::us1976_compute_values(departure.elevation_m, departure.isa_deviation_c);
    let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
    let gap = || {
        ConstraintResidual::direct(
            "takeoff_field_evidence_gap",
            ConstraintFamily::Performance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            ResidualRole::Diagnostic,
        )
    };
    let Ok(polar) = field_polar(inputs.drag, perf, mach, departure) else {
        return gap();
    };
    match jet_takeoff_field(&JetFieldInputs {
        perf,
        engine_out: model,
        airport: departure,
        wing_area_m2: inputs.s_ref_m2,
        mass_kg: inputs.mass_kg,
        static_thrust_n: inputs.static_thrust_n,
        polar,
        speeds,
    }) {
        Ok(field) => ConstraintResidual::scaled(
            "takeoff_field",
            ConstraintFamily::Performance,
            departure.toda_m,
            field.tofl_m,
            "m",
            field.tofl_m - departure.toda_m,
            inputs.role,
        ),
        Err(reason) if reason.contains("asymmetric trim") => gap(),
        Err(reason) => ConstraintResidual::direct(
            "takeoff_field",
            ConstraintFamily::Performance,
            departure.toda_m,
            f64::NAN,
            "m",
            f64::NAN,
            f64::INFINITY,
            inputs.role,
        )
        .with_detail(reason),
    }
}

fn correlation_row(inputs: &ResidualInputs<'_>) -> ConstraintResidual {
    // An improved-climb V2 lengthens the field (same correlation, lower
    // effective CLmax).
    correlation_residual(
        inputs.mass_kg,
        inputs.static_thrust_n,
        inputs.s_ref_m2,
        inputs.gravity_m_s2,
        inputs.departure,
        inputs.perf.takeoff_field_cl_max(inputs.perf.cl_max_to),
        inputs.role,
    )
}

/// Required-versus-installed static `T/W` of the Raymer correlation at
/// `mass_kg`: both sides move with the mass, so a field a light dispatch
/// clears can be missed at the maximum take-off mass.
pub(super) fn correlation_residual(
    mass_kg: f64,
    static_thrust_n: f64,
    s_ref_m2: f64,
    gravity_m_s2: f64,
    departure: &Airport,
    cl_max_to: f64,
    role: ResidualRole,
) -> ConstraintResidual {
    let weight_n = mass_kg * gravity_m_s2;
    let available_tw = static_thrust_n / weight_n;
    let sigma = density_ratio(departure.elevation_m, departure.isa_deviation_c);
    let required_tw =
        tw_takeoff_constraint(&[weight_n / s_ref_m2], departure.toda_m, sigma, cl_max_to)[0];
    ConstraintResidual::scaled(
        "takeoff_field",
        ConstraintFamily::Performance,
        available_tw,
        required_tw,
        "T/W",
        required_tw - available_tw,
        role,
    )
}
