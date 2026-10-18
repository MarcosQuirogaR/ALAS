// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The configuration a completed analysis reports and gates under.
//!
//! Two declared inputs of `performance` are replaced by what the analysed
//! aircraft implies, so the feasibility gate, the ISA sea-level field
//! reference and the report figures (matching chart, take-off and landing
//! figures) all read the same V speeds:
//!
//! 1. **Maximum lift coefficients.** The sizing closure flew the candidate
//!    with `cl_max_{clean,to,land}` scaled to its wing (`alas_perf::performance::clmax`,
//!    AERL-01); the report reads them back from the candidate's fuel
//!    artifacts. At the registered geometry nothing is scaled and the
//!    declared values stand.
//! 2. **Minimum control speed.** The yaw balance of the built fin and rudder
//!    (`alas_opt::mdo::residuals_vmc`) gives `VMC / VSR`, which enters the
//!    take-off speed rule (`V2 >= 1.10 VMC`, `VR >= 1.05 VMC`, CS 25.107).
//!    The declared `performance.vmc_vstall_factor` is only the fallback and,
//!    for a registered aircraft, the cap. `VSR` depends on `CLmax`, so the
//!    scaled coefficients come first.

use std::borrow::Cow;

use alas_config::{airports, AlasConfig};
use alas_opt::mdo::{
    evaluate_vmc, performance_with_vmc, resolve_improved_climb, EngineOutModel,
    ImprovedClimbInputs, VmcInputs, VmcOutcome, YawAirframe,
};
use alas_perf::performance::ImprovedClimbSelection;

use crate::full_analysis::AnalysisReport;

/// `config` with the maximum lift coefficients the analysed candidate was
/// sized with, or the borrowed original when it kept the declared ones (or
/// no fuel artifacts exist for the report).
pub fn config_with_geometry_clmax<'a>(
    config: &'a AlasConfig,
    report: &AnalysisReport,
) -> Cow<'a, AlasConfig> {
    match report.fuel.artifacts(config, &report.design) {
        Ok(artifacts) => artifacts.config_with_clmax(config),
        Err(_) => Cow::Borrowed(config),
    }
}

/// Evaluate `VMCA` and the crosswind check of the reported airplane at the
/// analysed take-off and landing masses. `config` should already carry the
/// geometry-scaled `CLmax` ([`config_with_geometry_clmax`]).
pub fn evaluate_report_vmc(
    config: &AlasConfig,
    report: &AnalysisReport,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
    wing_area_m2: f64,
) -> VmcOutcome {
    let departure = airports::get(&config.departure_airport).ok();
    let artifacts = report.fuel.artifacts(config, &report.design);
    // An unavailable drag table leaves the feathered-propeller drag unknown,
    // which the evaluation reports as an unavailable `VMC` (the declared
    // ratio then stays in the speed rule).
    let clean_cd0 = |mach: f64, altitude_m: f64, isa_deviation_c: f64| {
        artifacts.as_ref().map_or(f64::NAN, |artifacts| {
            artifacts
                .drag
                .cd_at_atmosphere(0.0, mach, altitude_m, isa_deviation_c)
        })
    };
    evaluate_vmc(&VmcInputs {
        config,
        plane: &report.airplane,
        cg_x_m: report.physical_cg[0],
        takeoff_mass_kg,
        landing_mass_kg,
        s_ref_m2: wing_area_m2,
        static_per_engine_n: None,
        departure,
        clean_cd0: &clean_cd0,
    })
}

/// Everything [`resolve_report`] determined about the report's speed basis.
pub struct ReportResolution<'a> {
    /// The configuration the V speeds are built from.
    pub config: Cow<'a, AlasConfig>,
    /// The computed minimum control speed outcome.
    pub vmc: VmcOutcome,
    /// The improved-climb selection (jets), `None` when it was not evaluated.
    pub improved_climb: Option<ImprovedClimbSelection>,
}

/// Resolve the report's speed basis: the candidate's scaled `CLmax`, its
/// computed `VMC / VSR` (declared ratio when none could be computed) and, for
/// a jet whose schedule `V2` misses the second-segment gradient, the
/// improved-climb `V2` with its longer field (`alas_opt::mdo::resolve_improved_climb`).
pub fn resolve_report<'a>(
    config: &'a AlasConfig,
    report: &AnalysisReport,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
    wing_area_m2: f64,
) -> ReportResolution<'a> {
    let scaled = config_with_geometry_clmax(config, report);
    let vmc = evaluate_report_vmc(
        &scaled,
        report,
        takeoff_mass_kg,
        landing_mass_kg,
        wing_area_m2,
    );
    let with_vmc = match performance_with_vmc(&scaled, &vmc) {
        Cow::Owned(resolved) => Cow::Owned(resolved),
        Cow::Borrowed(_) => scaled,
    };
    let (config, improved_climb) = improved_climb(with_vmc, report, takeoff_mass_kg, wing_area_m2);
    ReportResolution {
        config,
        vmc,
        improved_climb,
    }
}

/// The improved-climb step of [`resolve_report`], for jets with a resolvable
/// departure field and drag.
fn improved_climb<'a>(
    config: Cow<'a, AlasConfig>,
    report: &AnalysisReport,
    takeoff_mass_kg: f64,
    wing_area_m2: f64,
) -> (Cow<'a, AlasConfig>, Option<ImprovedClimbSelection>) {
    let engine = &config.geometry.engine;
    let n_engines = engine.spanwise_positions_m.len();
    let departure = airports::get(&config.departure_airport).ok();
    let artifacts = report.fuel.artifacts(&config, &report.design).ok();
    let model = EngineOutModel::new(
        &config,
        Some(YawAirframe {
            plane: &report.airplane,
            cg_x_m: report.physical_cg[0],
        }),
        wing_area_m2,
        None,
    )
    .ok()
    .map(|model| model.with_weight_kg(takeoff_mass_kg));
    let (Some(departure), Some(artifacts), Some(model)) = (departure, artifacts, model) else {
        return (config, None);
    };
    if n_engines == 0 || !(takeoff_mass_kg.is_finite() && takeoff_mass_kg > 0.0) {
        return (config, None);
    }
    let installed_tw = n_engines as f64 * engine.thrust_kn() * 1_000.0
        / (takeoff_mass_kg * config.requirements.gravity_m_s2);
    let (resolved, selection) = resolve_improved_climb(
        &config,
        &ImprovedClimbInputs {
            model: &model,
            departure,
            mass_kg: takeoff_mass_kg,
            s_ref_m2: wing_area_m2,
            drag: &artifacts.drag,
            installed_tw_sls: installed_tw,
        },
    );
    match resolved {
        Cow::Owned(resolved) => (Cow::Owned(resolved), selection),
        Cow::Borrowed(_) => (config, selection),
    }
}

/// The configuration the report's V speeds are built from: the candidate's
/// scaled `CLmax`, its computed `VMC / VSR` and its improved-climb `V2`
/// ([`resolve_report`]).
pub fn config_for_report<'a>(
    config: &'a AlasConfig,
    report: &AnalysisReport,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
    wing_area_m2: f64,
) -> Cow<'a, AlasConfig> {
    resolve_report(
        config,
        report,
        takeoff_mass_kg,
        landing_mass_kg,
        wing_area_m2,
    )
    .config
}
// A test builds curated presets, so a failed unwrap is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::full_analysis::FullAnalysis;
    use alas_config::DesignVector;
    use alas_perf::performance::compute_v_speeds_at_masses;

    fn report(config: &AlasConfig, design: &DesignVector) -> AnalysisReport {
        FullAnalysis::new(config.clone())
            .run(design, true)
            .unwrap_or_else(|error| panic!("report: {error}"))
    }

    #[test]
    fn the_registered_geometry_keeps_the_declared_clmax() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
        let design = config.configured_nominal_design();
        let report = report(&config, &design);
        let scaled = config_with_geometry_clmax(&config, &report);
        assert!(matches!(scaled, Cow::Borrowed(_)));
    }

    #[test]
    fn a_moved_wing_reports_with_the_clmax_it_was_sized_with() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
        let nominal = config.configured_nominal_design();
        let swept = DesignVector {
            sweep_deg: nominal.sweep_deg + 4.0,
            ..nominal
        };
        let report = report(&config, &swept);
        let artifacts = report.fuel.artifacts(&config, &report.design).unwrap();
        let set = artifacts.clmax.expect("the swept wing is scaled");
        let scaled = config_with_geometry_clmax(&config, &report);
        assert_eq!(scaled.performance.cl_max_to, set.takeoff);
        assert_eq!(scaled.performance.cl_max_land, set.landing);
        assert_eq!(scaled.performance.cl_max_clean, set.clean);
        // More sweep lowers every coefficient (cos law and flap hinge sweep).
        assert!(set.clean < config.performance.cl_max_clean);
        assert!(set.takeoff < config.performance.cl_max_to);
        assert!(set.landing < config.performance.cl_max_land);
    }

    #[test]
    fn the_report_speed_schedule_uses_the_computed_vmc_not_the_declared_one() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
        let design = config.configured_nominal_design();
        let report = report(&config, &design);
        let mass = config.requirements.mtow_kg;
        let landing = config.landing_mass_limit_kg(mass);
        let area = report.airplane.s_ref;
        let resolved = config_for_report(&config, &report, mass, landing, area);
        let declared = config.performance.vmc_vstall_factor;
        let computed = resolved.performance.vmc_vstall_factor;
        assert!(computed < declared, "{computed} vs declared {declared}");
        let airport = alas_config::airports::Airport::custom(
            "ISA sea level",
            0.0,
            10_000.0,
            10_000.0,
            0.0,
            0.0,
            0.0,
        );
        let v2_ratio = |config: &AlasConfig| {
            let perf = &config.performance;
            let speeds = compute_v_speeds_at_masses(
                mass,
                landing,
                area,
                &airport,
                perf.cl_max_to,
                perf.cl_max_land,
                perf,
            );
            speeds.v2_ms / speeds.v_stall_to_ms
        };
        // Declared estimate: V2 = 1.10 x 1.13 = 1.243 VSR; computed: the
        // configured 1.20 VSR governs.
        assert!(
            (v2_ratio(&config) - 1.243).abs() < 1e-9,
            "{}",
            v2_ratio(&config)
        );
        assert!(
            (v2_ratio(&resolved) - 1.20).abs() < 1e-9,
            "{}",
            v2_ratio(&resolved)
        );
    }

    #[test]
    fn a_second_segment_that_no_speed_meets_keeps_the_schedule_speed_and_is_flagged() {
        // The B787-9 at 1.2 MTOW (with the Jenkinson take-off charts it meets
        // the gradient at the schedule speed up to about 1.09 MTOW and at the
        // best-climb speed up to about 1.14 MTOW) misses the gradient at the
        // schedule speed and at the best-climb speed too: the schedule stands
        // (no field charge) and the selection records that improved climb
        // cannot meet it.
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "B787-9"})).unwrap();
        let design = config.configured_nominal_design();
        let analysis = report(&config, &design);
        let mass = 1.2 * config.requirements.mtow_kg;
        let resolution = resolve_report(
            &config,
            &analysis,
            mass,
            config.landing_mass_limit_kg(mass),
            analysis.airplane.s_ref,
        );
        let selection = resolution.improved_climb.expect("jet selection");
        assert!(selection.improved && !selection.meets, "{selection:?}");
        assert!(selection.v2_over_vsr > 1.3 && selection.required < selection.required_at_schedule);
        assert!(resolution
            .config
            .performance
            .improved_climb_v2_over_vsr
            .is_none());
        assert_eq!(
            resolution.config.performance.improved_climb_field_factor(),
            1.0
        );
        // The A320 meets at the schedule: nothing to report.
        let a320 = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
        let design = a320.configured_nominal_design();
        let analysis = report(&a320, &design);
        let mass = a320.requirements.mtow_kg;
        let resolution = resolve_report(
            &a320,
            &analysis,
            mass,
            a320.landing_mass_limit_kg(mass),
            analysis.airplane.s_ref,
        );
        assert!(!resolution.improved_climb.expect("selection").improved);
    }
}
