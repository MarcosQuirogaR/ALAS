// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! ISA sea-level field performance at the certification reference masses.
//!
//! Published aircraft-characteristics charts quote take-off field length at
//! maximum take-off mass and landing field length and reference speed at
//! maximum landing mass, on an ISA sea-level runway. The feasibility gate
//! evaluates the *departure and arrival airports* at the analysed masses, so
//! neither number exists anywhere in the pipeline. This module evaluates the
//! same `alas_perf` field-performance model on a synthetic ISA sea-level
//! aerodrome so the two can be compared, without touching the gate.
//!
//! Every quantity is a preliminary-design estimate from the app's empirical
//! matching-chart correlations (Raymer, *Aircraft Design: A Conceptual
//! Approach*, Ch. 5 and 17), not a certificated distance.

use alas_config::airports::Airport;
use alas_config::{ActiveEngineModel, AlasConfig, TurbopropEngineSpec};
use alas_opt::mdo::propulsion::turboprop_unit_model;
use alas_perf::performance::{
    assess_oei_climb, compute_field_performance_at_masses, compute_v_speeds_at_masses,
    far25_oei_gradient, OeiClimbStatus, OeiV2Condition,
};
use alas_prop::turboprop::Pw127mRating;

/// ISA sea-level air density, kg/m^3 (ISO 2533 / ICAO Doc 7488).
const ISA_SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;

/// Landing reference speed over the 1 g landing-configuration stall speed:
/// 14 CFR 25.125(b)(2)(i), `V_REF >= 1.23 V_SR`, with `V_SR` taken as the 1 g
/// stall speed `V_S1g` the model computes. This is deliberately not the
/// approach-speed factor in `PerformanceConfig`, which is a calibration knob.
/// 14 CFR 25.103 defines `V_SR >= V_S1g`, so taking `V_SR = V_S1g` slightly
/// understates `V_REF`.
pub const VREF_OVER_VS1G: f64 = 1.23;

/// Share of the landing field length the actual landing distance from 50 ft
/// may use on a dry runway: 14 CFR 121.195(b) for turbine transports (EU Air
/// Ops CAT.POL.A.230 uses the same 60 %). Airport-planning landing field
/// lengths are the actual distance divided by this share (Raymer, *Aircraft
/// Design: A Conceptual Approach*, 5th ed., 2012, Sec. 5.4 after eq. 5.11,
/// takes the FAR 25 field length as 1.667 times the distance).
pub const DRY_LANDING_DISTANCE_SHARE: f64 = 0.6;

/// Second-segment climb evidence the model supports. The app has no
/// available-gradient model: only a *required* T/W is evaluated.
#[derive(Debug, Clone, PartialEq)]
pub struct OeiSecondSegmentReference {
    /// Required minimum gross gradient, 14 CFR 25.121(b), when the engine
    /// count is on the Part 25 table.
    pub required_gradient: Option<f64>,
    /// Required in-flight-equivalent T/W to hold that gradient, when defined.
    pub required_inflight_tw: Option<f64>,
    /// Required installed sea-level T/W, only when the condition-specific
    /// evidence the assessor demands is configured.
    pub required_sls_tw: Option<f64>,
    /// The assessor's evidence status.
    pub status: OeiClimbStatus,
    /// What the numbers are, in words.
    pub basis: &'static str,
}

/// Field performance and reference speeds on an ISA sea-level runway.
#[derive(Debug, Clone, PartialEq)]
pub struct IsaSeaLevelFieldReference {
    /// Maximum take-off mass used, kg.
    pub takeoff_mass_kg: f64,
    /// Maximum landing mass used, kg.
    pub landing_mass_kg: f64,
    /// All-engine installed sea-level thrust the T/W rests on, N.
    pub static_thrust_n: f64,
    /// Where that thrust comes from.
    pub static_thrust_basis: &'static str,
    /// Static T/W at the take-off mass (thrust over `m g`).
    pub static_tw: f64,
    /// FAR 25 take-off field length, m: the `alas_perf` take-off-parameter
    /// correlation `37.7 (W/S)/(sigma CL_max,TO T/W)` [ft, psf] itself.
    ///
    /// That is Raymer's take-off correlation (*Aircraft Design: A Conceptual
    /// Approach*, 5th ed., 2012, Sec. 5.4, Fig. 5.4), which he reads directly
    /// as the FAR 25 take-off field length: already the 14 CFR 25.113 /
    /// balanced-field length, so it needs no further factor to compare with an
    /// airport-planning take-off field length (Loftin, NASA RP-1060, 1980,
    /// gives the same form with 37.5).
    pub takeoff_field_length_m: f64,
    /// Definition of `takeoff_field_length_m`.
    pub takeoff_field_length_basis: &'static str,
    /// The model's balanced field length, m: `bfl_factor` times the
    /// correlation above, the quantity the feasibility gate and matching chart
    /// use. Kept separate because that factor applies a balanced-field
    /// correction on top of a correlation that is already a field length.
    pub model_balanced_field_length_m: f64,
    /// Definition of `model_balanced_field_length_m`.
    pub model_balanced_field_length_basis: &'static str,
    /// FAR 25 factored landing field length, m: `landing_distance_m / 0.6`
    /// ([`DRY_LANDING_DISTANCE_SHARE`]), comparable with airport-planning
    /// landing field lengths.
    pub landing_field_length_m: f64,
    /// Definition of `landing_field_length_m`.
    pub landing_field_length_basis: &'static str,
    /// Unfactored landing distance from 50 ft, m: the `alas_perf`
    /// correlation `K (W/S)/(sigma CL_max,land)` at the landing mass.
    ///
    /// `K` (`performance.k_land`, 0.58-0.63 m/Pa in the presets, 0.60 by
    /// default) is an actual-distance coefficient, not a field-length one.
    /// Raymer eq. 5.11 gives the unfactored distance
    /// `S = 80 (W/S)/(sigma CL_max) + S_a` [ft, psf] with `S_a` = 1000 ft for
    /// an airliner, and the FAR 25 field length as 1.667 times that.
    /// 80 ft/psf is 0.51 m/Pa; with `S_a` folded in at a transport wing
    /// loading near 5 kPa and `CL_max` near 2.8 the effective proportional
    /// coefficient is about 0.68 m/Pa. The FAR field-length form
    /// `S_FL = 0.3 V_A^2` [ft, kt] with `V_A = 1.3 V_S` (Loftin, NASA RP-1060,
    /// 1980) is 0.95 m/Pa, and 0.6 of it is 0.57 m/Pa. K = 0.60 m/Pa sits with
    /// the unfactored distances, so it is exported here and the field length
    /// above is this divided by 0.6.
    pub landing_distance_m: f64,
    /// 1 g landing-configuration stall speed at the landing mass, m/s.
    pub vs1g_landing_m_s: f64,
    /// `V_REF = 1.23 V_S1g`, m/s.
    pub vref_m_s: f64,
    /// Engine-out second-segment climb evidence.
    pub oei: OeiSecondSegmentReference,
}

/// All-engine installed sea-level thrust, N, and its basis.
///
/// Mirrors the gate's resolution (`feasibility::static_thrust`, which is
/// private to that module): the catalogue rating for a turbofan, the
/// propeller model's ground-roll mean thrust at `V_LOF/sqrt(2)` for a
/// turboprop, whose static thrust would flatter the field length. At ISA sea
/// level the lift-off equivalent airspeed is the lift-off speed itself.
fn sea_level_static_thrust_n(
    config: &AlasConfig,
    lift_off_speed_m_s: f64,
) -> Result<(f64, &'static str), String> {
    let engine = &config.geometry.engine;
    let n_engines = engine.spanwise_positions_m.len();
    if n_engines == 0 {
        return Err("no engine is installed on this aircraft".to_owned());
    }
    match engine.active_model().map_err(|error| error.to_string())? {
        ActiveEngineModel::Turbofan(_) => Ok((
            n_engines as f64 * engine.thrust_kn() * 1_000.0,
            "certificated static jet rating",
        )),
        ActiveEngineModel::Turboprop(spec) => Ok((
            propeller_roll_mean_thrust_n(spec, n_engines, lift_off_speed_m_s)?,
            "propeller model ground-roll mean thrust at V_LOF/sqrt(2)",
        )),
    }
}

fn propeller_roll_mean_thrust_n(
    spec: &TurbopropEngineSpec,
    n_engines: usize,
    lift_off_speed_m_s: f64,
) -> Result<f64, String> {
    let field = turboprop_unit_model(spec)
        .field_performance(
            ISA_SEA_LEVEL_DENSITY_KG_M3,
            Pw127mRating::NormalTakeoff,
            lift_off_speed_m_s,
        )
        .map_err(|error| format!("propeller ground-roll thrust unavailable: {error}"))?;
    let thrust_n = n_engines as f64 * field.mean_ground_roll_thrust_per_engine_n;
    if thrust_n.is_finite() && thrust_n > 0.0 {
        Ok(thrust_n)
    } else {
        Err(format!(
            "propeller ground-roll mean thrust {thrust_n} N is unusable"
        ))
    }
}

/// Evaluate [`IsaSeaLevelFieldReference`] for `config`.
///
/// `takeoff_mass_kg` is MTOW and `landing_mass_kg` is MLW. `cd0` and `k` are
/// the parabolic-polar fit used for the engine-out assessment.
pub fn isa_sea_level_field_reference(
    config: &AlasConfig,
    wing_area_m2: f64,
    cd0: f64,
    k: f64,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
) -> Result<IsaSeaLevelFieldReference, String> {
    let usable = |value: f64| value.is_finite() && value > 0.0;
    if !(usable(wing_area_m2) && usable(takeoff_mass_kg) && usable(landing_mass_kg)) {
        return Err("wing area and reference masses must be finite and positive".to_owned());
    }
    // ISA sea level, standard day. The runway length is irrelevant to the
    // required distances; it only has to exist to build the aerodrome.
    let airport = Airport::custom("ISA sea level", 0.0, 10_000.0, 10_000.0, 0.0, 0.0, 0.0);
    let perf = &config.performance;
    let speeds = compute_v_speeds_at_masses(
        takeoff_mass_kg,
        landing_mass_kg,
        wing_area_m2,
        &airport,
        perf.cl_max_to,
        perf.cl_max_land,
        perf,
    );
    let (static_thrust_n, static_thrust_basis) = sea_level_static_thrust_n(config, speeds.v_r_ms)?;
    let static_tw = static_thrust_n / (takeoff_mass_kg * config.requirements.gravity_m_s2);
    let field = compute_field_performance_at_masses(
        takeoff_mass_kg,
        landing_mass_kg,
        wing_area_m2,
        &airport,
        perf.cl_max_to,
        perf.cl_max_land,
        static_tw,
        perf.k_land,
        perf.bfl_factor,
        perf,
    );

    let n_engines = config.geometry.engine.spanwise_positions_m.len() as i64;
    let required_gradient = far25_oei_gradient(n_engines);
    let oei_condition = OeiV2Condition {
        departure_elevation_m: 0.0,
        departure_isa_deviation_c: 0.0,
        v2_over_vstall: speeds.v2_ms / speeds.v_stall_to_ms,
        condition_to_sls_thrust_ratio: perf.oei_condition_to_sls_thrust_ratio,
        asymmetric_trim_cd: perf.oei_asymmetric_trim_cd,
        windmilling_cd: perf.oei_windmilling_cd,
    };
    let assessment = assess_oei_climb(
        cd0,
        k,
        n_engines,
        required_gradient.unwrap_or(perf.oei_gradient),
        perf.oei_climb_cl,
        perf.oei_climb_delta_cd,
        perf.cl_max_to,
        Some(oei_condition),
    );
    Ok(IsaSeaLevelFieldReference {
        takeoff_mass_kg,
        landing_mass_kg,
        static_thrust_n,
        static_thrust_basis,
        static_tw,
        takeoff_field_length_m: field.todr_m,
        takeoff_field_length_basis: "FAR 25 take-off field length: Raymer take-off-parameter \
             correlation 37.7 (W/S)/(sigma CLmax_TO T/W) [ft, psf] at MTOW, ISA sea level \
             (Aircraft Design, Sec. 5.4, Fig. 5.4), which is already a balanced-field-type \
             length; no bfl_factor applied",
        model_balanced_field_length_m: field.bfl_m,
        model_balanced_field_length_basis: "alas_perf bfl_factor x the FAR 25 take-off \
             field-length correlation, as the feasibility gate uses it; the factor is applied \
             on top of a correlation that is already a field length",
        landing_field_length_m: field.ldr_m / DRY_LANDING_DISTANCE_SHARE,
        landing_field_length_basis: "FAR 25 factored dry landing field length: unfactored \
             k_land (W/S)/(sigma CLmax_land) landing distance at MLW, ISA sea level, divided \
             by 0.6 (14 CFR 121.195(b); Raymer eq. 5.11, x1.667)",
        landing_distance_m: field.ldr_m,
        vs1g_landing_m_s: speeds.v_stall_land_ms,
        vref_m_s: VREF_OVER_VS1G * speeds.v_stall_land_ms,
        oei: OeiSecondSegmentReference {
            required_gradient,
            required_inflight_tw: assessment.required_inflight_tw,
            required_sls_tw: assessment.required_sls_tw,
            status: assessment.status,
            basis: "required T/W to hold the 14 CFR 25.121(b) gross gradient; the model has no \
                 available-gradient calculation, so no available gradient is exported",
        },
    })
}

// The tests build curated presets, so a failed expect is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::AlasConfig;
    use serde_json::json;

    #[test]
    fn a320_reference_is_finite_and_ordered() {
        let config = AlasConfig::from_value(&json!({ "preset": "A320-200" })).expect("preset");
        let mtow = config.requirements.mtow_kg;
        let reference =
            isa_sea_level_field_reference(&config, 122.6, 0.02, 0.04, mtow, 0.92 * mtow)
                .expect("field reference");
        for value in [
            reference.static_tw,
            reference.takeoff_field_length_m,
            reference.model_balanced_field_length_m,
            reference.landing_field_length_m,
            reference.landing_distance_m,
            reference.vref_m_s,
        ] {
            assert!(value.is_finite() && value > 0.0, "{reference:?}");
        }
        // The FAR take-off field length is the correlation itself; the model
        // BFL is the configured factor on top of it.
        assert!(
            (reference.model_balanced_field_length_m / reference.takeoff_field_length_m
                - config.performance.bfl_factor)
                .abs()
                < 1e-12
        );
        // The factored landing field length is the actual distance over 0.6.
        assert!(
            (reference.landing_field_length_m / reference.landing_distance_m - 1.0 / 0.6).abs()
                < 1e-12
        );
        assert!((reference.vref_m_s / reference.vs1g_landing_m_s - VREF_OVER_VS1G).abs() < 1e-12);
    }

    #[test]
    fn the_turboprop_uses_the_propeller_roll_mean_thrust() {
        let config = AlasConfig::from_value(&json!({ "preset": "ATR72-600" })).expect("preset");
        let mtow = config.requirements.mtow_kg;
        let reference = isa_sea_level_field_reference(&config, 61.0, 0.03, 0.05, mtow, 0.95 * mtow)
            .expect("turboprop field reference");
        assert!(reference.static_thrust_basis.contains("propeller"));
        assert!(reference.static_tw > 0.0 && reference.takeoff_field_length_m.is_finite());
    }
}
