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
//! Jet distances use the Raymer matching-chart correlations; propeller
//! distances use Torenbeek's force/energy and balanced-field method with the
//! installed speed-dependent thrust deck. These are conceptual estimates.

use alas_config::airports::Airport;
use alas_config::{ActiveEngineModel, AlasConfig, TurbopropEngineSpec};
use alas_opt::mdo::propulsion::turboprop_unit_model;
use alas_perf::performance::{
    assess_oei_climb, compute_v_speeds_at_masses, far25_oei_gradient, oei_cl_at_v2, OeiClimbStatus,
    OeiV2Condition,
};
use alas_prop::turboprop::Pw127mRating;

use crate::full_analysis::AnalysisReport;

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
/// Ops CAT.POL.A.230 uses 60 % for jets and 70 % for turboprops). This
/// constant describes the jet and historical replay convention; corrected
/// propeller callers use their configured share. Airport-planning landing field
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
    /// Conceptual take-off field length, m. For jets, the take-off-parameter
    /// correlation `37.7 (W/S)/(sigma CL_max,TO T/W)` [ft, psf] itself.
    ///
    /// That is Raymer's take-off correlation (*Aircraft Design: A Conceptual
    /// Approach*, 5th ed., 2012, Sec. 5.4, Fig. 5.4), which he reads directly
    /// as the FAR 25 take-off field length: already the 14 CFR 25.113 /
    /// balanced-field length, so it needs no further factor to compare with an
    /// airport-planning take-off field length (Loftin, NASA RP-1060, 1980,
    /// gives the same form with 37.5). Propellers instead use the larger of
    /// the Torenbeek BFL approximation and the once-factored AEO estimate.
    pub takeoff_field_length_m: f64,
    /// Definition of `takeoff_field_length_m`.
    pub takeoff_field_length_basis: &'static str,
    /// Conceptual balanced field length, m: the jet field correlation itself
    /// or Torenbeek's propeller balanced-field approximation. Historical
    /// replay alone applies the configurable legacy multiplier.
    pub model_balanced_field_length_m: f64,
    /// Definition of `model_balanced_field_length_m`.
    pub model_balanced_field_length_basis: &'static str,
    /// Factored dry landing field length, m: actual landing distance divided
    /// by the declared planning share (60% jet, 70% propeller by default).
    pub landing_field_length_m: f64,
    /// Definition of `landing_field_length_m`.
    pub landing_field_length_basis: &'static str,
    /// Unfactored landing distance from 50 ft, m. Propellers use Torenbeek's
    /// air-phase energy and touchdown braking balance. Jets retain the
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
/// turboprop. That mean remains a reported legacy/OEI diagnostic; the corrected
/// propeller field distance integrates the actual thrust curve separately.
/// At ISA sea level the lift-off equivalent airspeed is the lift-off speed itself.
pub fn sea_level_static_thrust_n(
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

/// Project the shared clean drag onto a single field operating point.
///
/// The engine-out assessor evaluates `cd0 + k CL^2` at one selected CL only.
/// Returning `(CD_at_CL, 0)` preserves that drag exactly, including the
/// shifted induced loading and actual atmospheric Reynolds number. This
/// point projection must not be used as a polar over other lift coefficients.
pub fn candidate_field_polar(
    config: &AlasConfig,
    report: &AnalysisReport,
    cl: f64,
    mach: f64,
    altitude_m: f64,
    isa_deviation_c: f64,
) -> Result<(f64, f64), String> {
    if !cl.is_finite()
        || cl <= 0.0
        || !mach.is_finite()
        || mach <= 0.0
        || !altitude_m.is_finite()
        || !isa_deviation_c.is_finite()
    {
        return Err("field drag needs positive CL/Mach and finite atmosphere inputs".to_owned());
    }
    let artifacts = report.fuel.artifacts(config, &report.design)?;
    let cd = artifacts
        .drag
        .cd_at_atmosphere(cl, mach, altitude_m, isa_deviation_c);
    if !cd.is_finite() || cd <= 0.0 {
        return Err("shared candidate field drag is not finite and positive".to_owned());
    }
    Ok((cd, 0.0))
}

/// ISA sea-level field reference using shared candidate drag at the V2 point.
///
/// The engine-out assessment uses the clean drag projection at V2. Propeller
/// distances use a separate low-speed quadratic from the same candidate
/// model, with field high-lift and failed-engine increments added separately.
pub fn report_isa_sea_level_field_reference(
    config: &AlasConfig,
    report: &AnalysisReport,
    wing_area_m2: f64,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
) -> Result<IsaSeaLevelFieldReference, String> {
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
    let cl = oei_cl_at_v2(perf.cl_max_to, speeds.v2_ms / speeds.v_stall_to_ms)
        .ok_or_else(|| "field V2 lift coefficient is invalid".to_owned())?;
    let atmosphere = alas_atmo::us1976_compute_values(0.0, 0.0);
    let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
    let (cd0, k) = candidate_field_polar(config, report, cl, mach, 0.0, 0.0)?;
    let field_polar = if matches!(
        config.geometry.engine.active_model(),
        Ok(ActiveEngineModel::Turboprop(_))
    ) && !perf.legacy_field_correlations
    {
        Some(crate::field_performance::report_field_polar(
            config, report, mach, &airport,
        )?)
    } else {
        None
    };
    reference_with_field_polar(
        config,
        wing_area_m2,
        cd0,
        k,
        takeoff_mass_kg,
        landing_mass_kg,
        field_polar,
    )
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
    reference_with_field_polar(
        config,
        wing_area_m2,
        cd0,
        k,
        takeoff_mass_kg,
        landing_mass_kg,
        None,
    )
}

// OEI point drag and the optional propeller field polar serve distinct conditions.
#[allow(clippy::too_many_arguments)]
fn reference_with_field_polar(
    config: &AlasConfig,
    wing_area_m2: f64,
    cd0: f64,
    k: f64,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
    field_polar: Option<(f64, f64)>,
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
    let (field_cd0, field_k) = field_polar.unwrap_or((cd0, k));
    let field = crate::field_performance::calculate(
        config,
        &airport,
        wing_area_m2,
        takeoff_mass_kg,
        landing_mass_kg,
        static_tw,
        field_cd0,
        field_k,
    )?;
    let propeller = matches!(
        config.geometry.engine.active_model(),
        Ok(ActiveEngineModel::Turboprop(_))
    ) && !perf.legacy_field_correlations;

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
    let cl_v2 = oei_cl_at_v2(perf.cl_max_to, oei_condition.v2_over_vstall)
        .ok_or_else(|| "field V2 lift coefficient is invalid".to_owned())?;
    let assessment = assess_oei_climb(
        cd0,
        k,
        n_engines,
        required_gradient.unwrap_or(perf.oei_gradient),
        cl_v2,
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
        takeoff_field_length_basis: if propeller {
            "Torenbeek ch.5/App.K propeller field estimate with speed-dependent installed thrust; \
             max of 115% AEO distance and eq.5-89 balanced-field approximation. Dry, level, still \
             air; normal AEO rating and installed automatic takeoff reserve on the remaining engines; unconfigured failed-engine \
             and asymmetric drag assumed zero. Conceptual, not certificated."
        } else {
            "FAR 25 take-off field length: Raymer take-off-parameter \
             correlation 37.7 (W/S)/(sigma CLmax_TO T/W) [ft, psf] at MTOW, ISA sea level \
             (Aircraft Design, Sec. 5.4, Fig. 5.4), which is already a balanced-field-type \
             length; no bfl_factor applied"
        },
        model_balanced_field_length_m: field.bfl_m,
        model_balanced_field_length_basis: if perf.legacy_field_correlations {
            "historical bfl_factor times the jet field correlation; duplicate margin retained for replay"
        } else if propeller {
            "Torenbeek eq.5-89 conceptual balance with integrated ground-roll equivalent acceleration, \
             V2 thrust lapse and configured stop deceleration; no certified V1/ASDA claim"
        } else {
            "Raymer FAR 25 TOP field correlation; 25.113(a)(2) does not require another 1.15 \
             on an already factored field length; accelerate-stop is not independently resolved"
        },
        landing_field_length_m: field.ldr_m
            / crate::field_performance::dry_landing_distance_share(config),
        landing_field_length_basis: if propeller {
            "Torenbeek eqs.5-93/5-94 air energy plus braking, no reverse credit, at MLW; \
             divided by configured dry planning share (CAT.POL.A.230(a)(2): 0.70 default, \
             FAR 121.195(b): select 0.60); 25.125 actual distance starts at 50 ft"
        } else {
            "FAR 25 factored dry landing field length: unfactored \
             k_land (W/S)/(sigma CLmax_land) landing distance at MLW, ISA sea level, divided \
             by 0.6 (14 CFR 121.195(b); Raymer eq. 5.11, x1.667)"
        },
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
    fn field_drag_reads_the_shared_table_at_the_actual_reynolds_number() {
        for name in ["ATR72-600", "AVE"] {
            let config = AlasConfig::from_value(&json!({ "preset": name })).expect("preset");
            let design = alas_config::presets::get(name)
                .expect("preset")
                .design_vector;
            let mut report = crate::full_analysis::FullAnalysis::new(config.clone())
                .run(&design, true)
                .expect("full analysis");
            let artifacts = report
                .fuel
                .artifacts(&config, &design)
                .expect("candidate drag");
            let table = artifacts.drag.table().expect("native table");
            // The report fit is not a field drag source. Poison all of it so
            // a dropped linear term or copied cruise intercept cannot pass.
            report.polar_fit.cd0 = f64::NAN;
            report.polar_fit.c1 = f64::NAN;
            report.polar_fit.k = f64::NAN;
            let (cl, mach, altitude_m) = (1.1, 0.22, 2_100.0);
            let mut drag_coefficients = Vec::new();
            for deviation_c in [-20.0, 0.0, 17.0] {
                let atmosphere = alas_atmo::us1976_compute_values(altitude_m, deviation_c);
                let reynolds_per_m =
                    atmosphere.density_kg_m3 * mach * atmosphere.speed_of_sound_m_s
                        / atmosphere.dynamic_viscosity_pa_s;
                let expected = table.cd0_at_reynolds_per_m(mach, reynolds_per_m)
                    + table.induced_cd(cl)
                    + table.wave_cd(cl, mach);
                let (cd0, k) =
                    candidate_field_polar(&config, &report, cl, mach, altitude_m, deviation_c)
                        .expect("field drag");
                assert_eq!((cd0 + k * cl * cl).to_bits(), expected.to_bits());
                assert!(cd0.is_finite() && cd0 > 0.0);
                drag_coefficients.push(cd0);
            }
            // At fixed Mach/CL, warmer air lowers Reynolds number and raises
            // turbulent parasite drag; the induced/wave terms do not change.
            assert!(drag_coefficients.windows(2).all(|cd| cd[0] < cd[1]));
        }
    }

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
        // The jet TOP correlation already estimates field length; 25.113 does
        // not add another all-engine margin to that field-length estimate.
        assert!(
            (reference.model_balanced_field_length_m / reference.takeoff_field_length_m - 1.0)
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
        let design = alas_config::presets::get("ATR72-600")
            .expect("preset")
            .design_vector;
        let report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&design, true)
            .expect("analysis");
        let reference = report_isa_sea_level_field_reference(
            &config,
            &report,
            report.airplane.s_ref,
            mtow,
            0.95 * mtow,
        )
        .expect("turboprop field reference");
        assert!(reference.static_thrust_basis.contains("propeller"));
        assert!(reference.static_tw > 0.0 && reference.takeoff_field_length_m.is_finite());
    }
}
