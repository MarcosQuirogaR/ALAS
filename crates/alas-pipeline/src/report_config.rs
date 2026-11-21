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

use alas_config::airports::Airport;
use alas_config::AlasConfig;
use alas_opt::mdo::{
    evaluate_vmc, performance_with_vmc, resolve_improved_climb, EngineOutModel,
    ImprovedClimbInputs, VmcInputs, VmcOutcome, YawAirframe,
};
use alas_perf::performance::ImprovedClimbSelection;

use crate::full_analysis::AnalysisReport;

/// `config` with the computed sea-level airborne minimum control speed at
/// MTOW in `landing_gear.rotation_minimum_control_speed_m_s`, the floor of the
/// forward-CG rotation speed (CS 25.107(e)(1)(ii)); a registered aircraft's
/// speed is capped at its declared `VMC / VSR` as in the speed schedule
/// (`alas_opt::mdo::evaluate_vmc`). Borrowed unchanged when the yaw balance
/// did not resolve, or when the computed `VMCA` does not validate against a
/// published value ([`vmca_validates`]): an unvalidated yaw balance must not
/// move the forward-CG lines.
pub fn config_with_rotation_vmc<'a>(
    config: &'a AlasConfig,
    vmc_at_mtow: &VmcOutcome,
) -> Cow<'a, AlasConfig> {
    let Ok(assessment) = vmc_at_mtow.at_sea_level.as_ref() else {
        return Cow::Borrowed(config);
    };
    let declared = config.performance.vmc_vstall_factor;
    let speed = if vmc_at_mtow.registered && assessment.vmc_over_vsr > declared {
        declared * assessment.vsr_m_s
    } else {
        assessment.vmca_m_s
    };
    if !(speed.is_finite() && speed > 0.0) {
        return Cow::Borrowed(config);
    }
    if !vmca_validates(config, assessment.vmca_m_s) {
        return Cow::Borrowed(config);
    }
    let mut resolved = config.clone();
    resolved.landing_gear.rotation_minimum_control_speed_m_s = Some(speed);
    Cow::Owned(resolved)
}

/// Largest relative difference between the computed and the published
/// sea-level `VMCA` for which the computed value is taken as validated.
pub const VMCA_VALIDATION_TOLERANCE: f64 = 0.05;

/// Whether the computed sea-level `VMCA` (m/s) validates against the type's
/// published one (`landing_gear.published_vmca_m_s`) within
/// [`VMCA_VALIDATION_TOLERANCE`]. A type with no published value does not
/// validate, so its forward rotation line does not take the computed floor.
pub fn vmca_validates(config: &AlasConfig, computed_vmca_m_s: f64) -> bool {
    match config.landing_gear.published_vmca_m_s {
        Some(published) if published > 0.0 && computed_vmca_m_s.is_finite() => {
            (computed_vmca_m_s / published - 1.0).abs() <= VMCA_VALIDATION_TOLERANCE
        }
        _ => false,
    }
}

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
    evaluate_report_vmc_at(
        config,
        report,
        None,
        takeoff_mass_kg,
        landing_mass_kg,
        wing_area_m2,
    )
}

/// [`evaluate_report_vmc`] at `airport`, the aerodrome the speeds are flown at,
/// instead of the configured departure (`None` keeps the departure): `VMCA`
/// depends on the thrust of the operating engine, which follows the density and
/// temperature of the field.
pub fn evaluate_report_vmc_at(
    config: &AlasConfig,
    report: &AnalysisReport,
    airport: Option<&Airport>,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
    wing_area_m2: f64,
) -> VmcOutcome {
    let departure = airport.or_else(|| config.departure_airport_record().ok());
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
        minimum_mass_kg: crate::quick_analysis::report_oew_kg(report),
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
    /// The computed minimum control speed outcome at the analysed take-off
    /// mass: the basis of the speed schedule (`VMC / VSR` of the load case
    /// that is flown).
    pub vmc: VmcOutcome,
    /// The minimum control speed outcome at the maximum take-off weight of
    /// the design, the condition CS 25.149(c)(4) states for the
    /// `VMC <= 1.13 VSR` ceiling (the findings read this one).
    pub vmc_at_mtow: VmcOutcome,
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
    resolve_report_at(
        config,
        report,
        None,
        takeoff_mass_kg,
        landing_mass_kg,
        wing_area_m2,
    )
}

/// [`resolve_report`] with the improved-climb `V2` resolved at `airport`, the
/// aerodrome the speeds are flown at, instead of the configured departure
/// (`None` keeps the departure). The second-segment margin depends on the
/// air density and temperature of the field, so a field evaluated at another
/// aerodrome (the ISA sea-level reference, a figure of a different airport)
/// must not fly the speed the departure needs.
pub fn resolve_report_at<'a>(
    config: &'a AlasConfig,
    report: &AnalysisReport,
    airport: Option<&Airport>,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
    wing_area_m2: f64,
) -> ReportResolution<'a> {
    resolve_report_at_flap(
        config,
        report,
        airport,
        None,
        takeoff_mass_kg,
        landing_mass_kg,
        wing_area_m2,
    )
}

/// [`resolve_report_at`] flown at take-off flap `flap` (one of
/// `alas_config::presets::takeoff_flap_settings` of `config`) instead of the
/// configured one. The setting is applied after the candidate's `CLmax`
/// scaling, as the ratio of its `CLmax_TO` to the configured one, so the
/// scaling stands; `VMC` and the improved-climb `V2` then follow the setting.
pub fn resolve_report_at_flap<'a>(
    config: &'a AlasConfig,
    report: &AnalysisReport,
    airport: Option<&Airport>,
    flap: Option<&alas_config::presets::TakeoffFlapSetting>,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
    wing_area_m2: f64,
) -> ReportResolution<'a> {
    let scaled = config_with_geometry_clmax(config, report);
    let scaled = match flap {
        Some(setting) => {
            let ratio = setting.cl_max_to / config.performance.cl_max_to;
            let mut flown = scaled.into_owned();
            flown.performance.cl_max_to *= ratio;
            flown.performance.oei_climb_delta_cd = setting.oei_climb_delta_cd;
            flown.performance.takeoff_flap_deflection_deg = Some(setting.deflection_deg);
            Cow::Owned(flown)
        }
        None => scaled,
    };
    let vmc = evaluate_report_vmc_at(
        &scaled,
        report,
        airport,
        takeoff_mass_kg,
        landing_mass_kg,
        wing_area_m2,
    );
    // CS 25.149(c)(4): the ceiling holds at the maximum sea-level take-off
    // weight. The analysed mass is the flown load case, lighter than that
    // for a short route, and `VSR` falls with it while `VMC` (a thrust and
    // fin balance) barely does, so the ratio at the flown mass overstates
    // the ceiling ratio. The sized mass is the design's own maximum take-off
    // weight (the declared one when the report is unsized).
    let mtow_kg = report.analysis_takeoff_mass_kg(config.requirements.mtow_kg);
    let vmc_at_mtow = if (mtow_kg - takeoff_mass_kg).abs() <= 1.0e-9 * mtow_kg.abs() {
        vmc.clone()
    } else {
        evaluate_report_vmc(&scaled, report, mtow_kg, landing_mass_kg, wing_area_m2)
    };
    let with_vmc = match performance_with_vmc(&scaled, &vmc) {
        Cow::Owned(resolved) => Cow::Owned(resolved),
        Cow::Borrowed(_) => scaled,
    };
    let (config, improved_climb) =
        improved_climb(with_vmc, report, airport, takeoff_mass_kg, wing_area_m2);
    ReportResolution {
        config,
        vmc,
        vmc_at_mtow,
        improved_climb,
    }
}

/// The improved-climb step of [`resolve_report`], for jets with a resolvable
/// departure field and drag.
fn improved_climb<'a>(
    config: Cow<'a, AlasConfig>,
    report: &AnalysisReport,
    airport: Option<&Airport>,
    takeoff_mass_kg: f64,
    wing_area_m2: f64,
) -> (Cow<'a, AlasConfig>, Option<ImprovedClimbSelection>) {
    let engine = &config.geometry.engine;
    let n_engines = engine.spanwise_positions_m.len();
    let departure = airport.or_else(|| config.departure_airport_record().ok());
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
            aspect_ratio: alas_opt::mdo::climb_out_aspect_ratio(
                &config,
                report.airplane.b_ref,
                report.airplane.s_ref,
            ),
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

/// [`config_for_report`] with the improved-climb `V2` resolved at `airport`
/// ([`resolve_report_at`]): the configuration for a field evaluated there.
pub fn config_for_report_at<'a>(
    config: &'a AlasConfig,
    report: &AnalysisReport,
    airport: &Airport,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
    wing_area_m2: f64,
) -> Cow<'a, AlasConfig> {
    resolve_report_at(
        config,
        report,
        Some(airport),
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
    fn the_improved_climb_speed_is_resolved_at_the_aerodrome_the_field_is_flown_at() {
        // The registered A320 at MTOW needs the improved-climb
        // V2 at its 610 m departure (ISA), not at ISA sea level, where the
        // second segment has more thrust margin; a field evaluated at sea
        // level must not fly the departure's speed.
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
        let design = config.configured_nominal_design();
        let report = report(&config, &design);
        // With the Raymer flap drag and the flap span loading the A320 meets the gradient at
        // the schedule speed at MTOW, so the mass is raised until the 610 m departure first
        // needs a higher speed.
        let area = report.airplane.s_ref;
        let mut mass = config.requirements.mtow_kg;
        let at = |elevation_m: f64, mass: f64| {
            let landing = config.landing_mass_limit_kg(mass);
            let airport = Airport::custom("field", elevation_m, 10_000.0, 10_000.0, 0.0, 0.0, 0.0);
            resolve_report_at(&config, &report, Some(&airport), mass, landing, area)
                .improved_climb
                .expect("jet selection")
        };
        let mut scale = 1.0;
        while !at(609.6, mass).improved && scale < 1.6 {
            scale += 0.02;
            mass = scale * config.requirements.mtow_kg;
        }
        let (sea, high) = (at(0.0, mass), at(609.6, mass));
        assert!(!sea.improved, "{sea:?}");
        assert!(high.improved, "{high:?}");
        assert!(high.v2_over_vsr > sea.v2_over_vsr);
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
        // Declared estimate: V2 = 1.10 x 1.13 = 1.243 VSR. Computed: the
        // registered A320 flies the CS 25.107 rule, V2 = max(1.13 VSR, 1.10
        // VMC, VR) with the computed VMC far below 1.13 / 1.10 VSR, raised to
        // the improved-climb speed the second segment needs at MTOW.
        assert!(
            (v2_ratio(&config) - 1.243).abs() < 1e-9,
            "{}",
            v2_ratio(&config)
        );
        let expected = resolved
            .performance
            .improved_climb_v2_over_vsr
            .unwrap_or(1.13)
            .max(1.13);
        assert!(
            (v2_ratio(&resolved) - expected).abs() < 1e-9 && v2_ratio(&resolved) < 1.243,
            "{} against {expected}",
            v2_ratio(&resolved)
        );
    }

    #[test]
    fn the_vmc_ceiling_ratio_is_taken_at_the_maximum_takeoff_weight_not_the_flown_mass() {
        // CS 25.149(c)(4): the 1.13 VSR ceiling is shown at the maximum
        // sea-level take-off weight. A light flown mass lowers VSR (about
        // the square root of the mass) and leaves VMC nearly unchanged, so
        // the ratio at the flown mass is higher than the certified one.
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
        let design = config.configured_nominal_design();
        let analysis = report(&config, &design);
        let mtow = config.requirements.mtow_kg;
        let light = 0.6 * mtow;
        let resolution = resolve_report(
            &config,
            &analysis,
            light,
            config.landing_mass_limit_kg(light),
            analysis.airplane.s_ref,
        );
        let ratio = |outcome: &VmcOutcome| {
            outcome
                .at_sea_level
                .as_ref()
                .map(|assessment| assessment.vmc_over_vsr)
                .unwrap()
        };
        let at_mtow = evaluate_report_vmc(
            &config_with_geometry_clmax(&config, &analysis),
            &analysis,
            mtow,
            config.landing_mass_limit_kg(light),
            analysis.airplane.s_ref,
        );
        assert!((ratio(&resolution.vmc_at_mtow) - ratio(&at_mtow)).abs() < 1.0e-12);
        assert!(
            ratio(&resolution.vmc) > 1.1 * ratio(&resolution.vmc_at_mtow),
            "flown {} against MTOW {}",
            ratio(&resolution.vmc),
            ratio(&resolution.vmc_at_mtow)
        );
    }

    #[test]
    fn a_second_segment_that_no_speed_meets_flies_the_best_climb_speed_and_is_flagged() {
        // The B787-9 above 1.2 MTOW (raised until it misses) the gradient at the schedule speed and
        // at the best-climb speed too: the best-climb speed is flown and charged
        // (continuous with the meeting case) and the selection records that
        // improved climb cannot meet it.
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "B787-9"})).unwrap();
        let design = config.configured_nominal_design();
        let analysis = report(&config, &design);
        let resolve = |scale: f64| {
            let mass = scale * config.requirements.mtow_kg;
            resolve_report(
                &config,
                &analysis,
                mass,
                config.landing_mass_limit_kg(mass),
                analysis.airplane.s_ref,
            )
        };
        // The mass is raised until no speed meets the gradient.
        let mut scale = 1.2;
        let mut resolution = resolve(scale);
        while resolution.improved_climb.is_some_and(|s| s.meets) && scale < 2.0 {
            scale += 0.05;
            resolution = resolve(scale);
        }
        let selection = resolution.improved_climb.expect("jet selection");
        assert!(selection.improved && !selection.meets, "{selection:?}");
        assert!(selection.v2_over_vsr > 1.3 && selection.required < selection.required_at_schedule);
        assert_eq!(
            resolution.config.performance.improved_climb_v2_over_vsr,
            Some(selection.v2_over_vsr)
        );
        assert!(resolution.config.performance.improved_climb_field_factor() > 1.0);
        // The A320 flies the CS 25.107 schedule (1.13 VSR; the yaw-balance VMC
        // lifts it to about 1.16): once the mass is raised until it first needs a speed
        // above the schedule, a speed below the best-climb limit meets the CS 25.121(b)
        // gradient. (At MTOW it meets it at the schedule speed.)
        let a320 = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
        let design = a320.configured_nominal_design();
        let analysis = report(&a320, &design);
        let resolve = |scale: f64| {
            let mass = scale * a320.requirements.mtow_kg;
            resolve_report(
                &a320,
                &analysis,
                mass,
                a320.landing_mass_limit_kg(mass),
                analysis.airplane.s_ref,
            )
        };
        let mut scale = 1.0;
        let mut resolution = resolve(scale);
        while !resolution.improved_climb.is_some_and(|s| s.improved) && scale < 1.6 {
            scale += 0.02;
            resolution = resolve(scale);
        }
        let selection = resolution.improved_climb.expect("selection");
        assert!(selection.improved && selection.meets, "{selection:?}");
        assert!(
            selection.v2_over_vsr > selection.schedule_v2_over_vsr && selection.v2_over_vsr < 1.30,
            "{selection:?}"
        );
    }

    #[test]
    fn the_rotation_vmc_floor_needs_a_validated_vmca() {
        let a320 = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
        let published = a320
            .landing_gear
            .published_vmca_m_s
            .expect("A320 VMCA 110 kt");
        assert!(vmca_validates(&a320, published * 1.04));
        assert!(vmca_validates(&a320, published * 0.96));
        assert!(!vmca_validates(&a320, published * 1.06));
        assert!(!vmca_validates(&a320, published * 0.87));
        assert!(!vmca_validates(&a320, f64::NAN));
        let b747 = AlasConfig::from_value(&serde_json::json!({"preset": "B747-400"})).unwrap();
        assert!(b747.landing_gear.published_vmca_m_s.is_none());
        assert!(!vmca_validates(&b747, 68.1));
    }
}
