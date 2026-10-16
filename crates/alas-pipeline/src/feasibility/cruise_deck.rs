// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cruise-altitude thrust findings from the engine deck (jets and, on the
//! unit model's own in-flight thrust, turboprops).
//!
//! The same evaluation as the optimizer's `cruise_thrust` and
//! `toc_rate_of_climb` residuals (`alas_opt::mdo::deck_cruise_requirements`):
//! the maximum-climb thrust the deck delivers at the design cruise point over
//! the rated static thrust replaces the flat `performance.thrust_lapse`.

use alas_config::AlasConfig;
use alas_opt::mdo::{
    deck_cruise_requirements, highest_cruise_level_m, planned_cruise_requirements, thrust_scatter,
    top_of_climb, DeckCruiseRequirements, EngineOutModel, PlannedCruise, PlannedCruiseLevel,
    ThrustAssessment, TOC_TOLERANCE_M_S,
};

use super::{error, warning, FindingCode, PhysicalFinding};
use crate::full_analysis::AnalysisReport;

/// The mass level cruise is checked at, kg.
///
/// The mean of the analysed take-off mass and the dispatch's landing mass when
/// the run flew a route. Without a settled dispatch (a run with the mission
/// disabled) a report bound to a sized candidate takes the candidate's own
/// mid-cruise mass, the mass the optimizer's `cruise_thrust` row used to
/// accept it, so the search and the reporting verdict judge the same flight
/// state; only a report with neither falls back to the take-off mass.
pub(super) fn level_mass_kg(
    loading: &super::FuelLoadingAssessment,
    report: &AnalysisReport,
) -> f64 {
    let takeoff_mass_kg = loading.analyzed_takeoff_mass_kg;
    loading
        .analyzed_landing_mass_kg
        .filter(|landing_kg| landing_kg.is_finite() && *landing_kg > 0.0)
        .map(|landing_kg| 0.5 * (takeoff_mass_kg + landing_kg.min(takeoff_mass_kg)))
        .or_else(|| {
            report
                .fuel
                .sized_fuel()
                .map(|sized| sized.mid_cruise_mass_kg)
                .filter(|mid_kg| mid_kg.is_finite() && *mid_kg > 0.0)
                .map(|mid_kg| mid_kg.min(takeoff_mass_kg))
        })
        .unwrap_or(takeoff_mass_kg)
}

/// The level-cruise requirement of a jet over the levels the report's mission
/// plan flies (the optimizer's `cruise_thrust` definition,
/// `alas_opt::mdo::planned_cruise_requirements`); or the design-level form at
/// the mid-cruise mass when no frozen plan is carried (a report without a
/// route dispatch, or a plan the model cannot fly at the analysed mass),
/// with the reason in `basis`.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct JetLevelCheck {
    /// The requirement at the governing level (the constraint value).
    pub deck: DeckCruiseRequirements,
    /// What it was evaluated at, for the finding text.
    pub basis: String,
    /// The plan's levels; `None` when the design-level fallback stands.
    pub plan: Option<PlannedLevels>,
}

/// The cruise levels of the mission plan with the highest reachable level.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedLevels {
    /// Every distinct level the plan flies, in flight order, with its mass,
    /// Mach and required static `T/W`.
    pub levels: Vec<PlannedCruiseLevel>,
    /// The governing (worst) level.
    pub governing: PlannedCruiseLevel,
    /// Highest level, m, at which level flight holds at the maximum-cruise
    /// rating at the initial cruise mass and Mach with the installed thrust
    /// (`None` when no level holds).
    pub highest_level_m: Option<f64>,
}

/// Installed static thrust per engine, N: the catalogue rating of a turbofan,
/// the unit model's sea-level normal take-off thrust of a turboprop (the
/// figure the optimizer's `available_tw` and its engine-out model are built
/// from, `alas_opt::mdo::propulsion::turboprop_static_thrust_n`). `None` when the engine
/// binding or the unit model cannot give one.
fn static_thrust_per_engine_n(config: &AlasConfig) -> Option<f64> {
    let thrust_n = match config.geometry.engine.active_model().ok()? {
        alas_config::ActiveEngineModel::Turbofan(_) => config.geometry.engine.thrust_kn() * 1_000.0,
        alas_config::ActiveEngineModel::Turboprop(spec) => {
            alas_opt::mdo::propulsion::turboprop_static_thrust_n(spec).ok()?
        }
    };
    (thrust_n.is_finite() && thrust_n > 0.0).then_some(thrust_n)
}

/// Installed static thrust over `weight_n`, on the basis of
/// [`static_thrust_per_engine_n`]; NaN when it is unavailable.
pub(super) fn static_tw_over(config: &AlasConfig, weight_n: f64) -> f64 {
    static_thrust_per_engine_n(config).map_or(f64::NAN, |thrust_n| {
        config.geometry.engine.spanwise_positions_m.len() as f64 * thrust_n / weight_n
    })
}

/// [`JetLevelCheck`] of the report at its analysed take-off weight.
pub(super) fn jet_level_check(
    config: &AlasConfig,
    report: &AnalysisReport,
    wing_loading_pa: f64,
    weight_n: f64,
    level_mass_kg: f64,
) -> Option<JetLevelCheck> {
    let req = &config.requirements;
    let s_ref_m2 = weight_n / wing_loading_pa;
    let mass_kg = weight_n / req.gravity_m_s2;
    let model =
        EngineOutModel::new(config, None, s_ref_m2, static_thrust_per_engine_n(config)).ok()?;
    let artifacts = report.fuel.artifacts(config, &report.design).ok()?;
    match planned_cruise_requirements(&model, config, &artifacts, mass_kg, s_ref_m2) {
        Ok(PlannedCruise {
            worst,
            worst_level,
            levels,
        }) => {
            let highest_level_m = highest_cruise_level_m(
                &model,
                &artifacts.drag,
                req.gravity_m_s2,
                &levels[0],
                (mass_kg, s_ref_m2),
                static_tw_over(config, weight_n),
            );
            let basis = format!(
                "the plan's {} cruise level(s), governing {:.0} m at Mach {:.3} and {:.0} kg",
                levels.len(),
                worst_level.altitude_m,
                worst_level.mach,
                worst_level.mass_kg
            );
            Some(JetLevelCheck {
                deck: worst,
                basis,
                plan: Some(PlannedLevels {
                    levels,
                    governing: worst_level,
                    highest_level_m,
                }),
            })
        }
        Err(reason) => {
            let deck = deck_cruise_requirements(
                &model,
                &artifacts.drag,
                req,
                mass_kg,
                level_mass_kg,
                s_ref_m2,
            )
            .ok()?;
            Some(JetLevelCheck {
                deck,
                basis: format!("the design level at the mid-cruise weight (no plan: {reason})"),
                plan: None,
            })
        }
    }
}

/// The level-cruise and top-of-climb thrust checks of a reported design with
/// the scatter of the engine's altitude thrust, for the matrix validation
/// block (passing or not).
#[derive(Debug, Clone, PartialEq)]
pub struct ThrustChecks {
    /// Scatter of the engine's altitude thrust as a fraction (0 with a
    /// cruise or climb datum, the chart scatter otherwise).
    pub scatter: f64,
    /// Level flight at the mid-cruise weight, required and available
    /// installed sea-level-static T/W; or why it could not be assessed.
    pub level: Result<ThrustAssessment, String>,
    /// The mission plan's cruise levels behind `level`, with the highest
    /// reachable level; `None` when the design-level fallback stood.
    pub plan: Option<PlannedLevels>,
    /// The 300 ft/min residual climb at the initial cruise level, required
    /// and available maximum-climb thrust, N; or why it could not be assessed.
    pub top_of_climb: Result<ThrustAssessment, String>,
}

/// Evaluate the [`ThrustChecks`] of a jet at the report's analysed masses (the
/// same masses and inputs as the feasibility findings).
pub fn thrust_checks(
    config: &AlasConfig,
    report: &AnalysisReport,
    feasibility: &super::FeasibilityReport,
) -> ThrustChecks {
    let scatter = thrust_scatter(config);
    let loading = &feasibility.fuel_loading;
    let wing_area_m2 = report
        .geometry_summary
        .get("wing_area_m2")
        .copied()
        .unwrap_or(report.airplane.s_ref);
    let takeoff_mass_kg = loading.analyzed_takeoff_mass_kg;
    let weight_n = takeoff_mass_kg * config.requirements.gravity_m_s2;
    let mid_mass_kg = level_mass_kg(loading, report);
    let static_tw = static_tw_over(config, weight_n);
    let check = jet_level_check(
        config,
        report,
        weight_n / wing_area_m2,
        weight_n,
        mid_mass_kg,
    )
    .ok_or_else(|| "the engine deck or the carried drag cannot be evaluated".to_owned());
    let plan = check.as_ref().ok().and_then(|check| check.plan.clone());
    let level = check.map(|check| ThrustAssessment::new(check.deck.level_tw, static_tw, scatter));
    let top_of_climb = report
        .fuel
        .artifacts(config, &report.design)
        .and_then(|artifacts| top_of_climb(config, &artifacts))
        .map(|toc| toc.thrust_assessment(scatter));
    ThrustChecks {
        scatter,
        level,
        plan,
        top_of_climb,
    }
}

/// Level-flight and top-of-climb findings for a jet, or `None` when the deck
/// or the carried drag cannot be evaluated (the caller then keeps the flat
/// lapse form and says so).
pub(super) fn jet_findings(
    config: &AlasConfig,
    report: &AnalysisReport,
    static_tw: f64,
    wing_loading_pa: f64,
    weight_n: f64,
    level_mass_kg: f64,
) -> Option<Vec<PhysicalFinding>> {
    let JetLevelCheck { deck, basis, .. } =
        jet_level_check(config, report, wing_loading_pa, weight_n, level_mass_kg)?;
    let mut findings = Vec::new();
    let scatter = thrust_scatter(config);
    let assessment = ThrustAssessment::new(deck.level_tw, static_tw, scatter);
    if !required_level_misses(&deck, static_tw) {
        return Some(findings);
    }
    // A datum engine (scatter 0) keeps the hard check; otherwise a shortfall
    // inside the chart scatter is a flagged warning.
    if scatter > 0.0 && assessment.shortfall() <= scatter {
        findings.push(warning(
            FindingCode::ThrustWithinChartScatter,
            assessment.describe(&format!("level cruise at {basis} (static T/W)"), "T/W"),
            Some(static_tw),
            Some(deck.level_tw),
            "T/W",
        ));
    } else {
        findings.push(error(
            FindingCode::ThrustMarginViolation,
            format!(
                "level cruise at {basis} needs static T/W {:.4} (maximum-cruise thrust is {:.3} of static at that point, from the engine deck), but the configured rating provides {static_tw:.4}",
                deck.level_tw, deck.cruise_thrust_ratio
            ),
            Some(static_tw),
            Some(deck.level_tw),
            "T/W",
        ));
    }
    Some(findings)
}

fn required_level_misses(deck: &DeckCruiseRequirements, static_tw: f64) -> bool {
    deck.level_tw.is_finite() && static_tw < deck.level_tw
}

/// The top-of-climb finding: the design mission's residual climb at the initial
/// cruise level it selects must hold 300 ft/min at the top-of-climb mass.
/// An error when it does not; a warning when the mission model cannot be
/// evaluated.
pub(super) fn toc_findings(config: &AlasConfig, report: &AnalysisReport) -> Vec<PhysicalFinding> {
    if !config.performance.cruise_thrust_from_deck {
        return Vec::new();
    }
    let Ok(artifacts) = report.fuel.artifacts(config, &report.design) else {
        return Vec::new();
    };
    let turboprop = matches!(
        config.geometry.engine.active_model(),
        Ok(alas_config::ActiveEngineModel::Turboprop(_))
    );
    let scatter = thrust_scatter(config);
    match top_of_climb(config, &artifacts) {
        Ok(toc) if toc.margin_m_s() < -TOC_TOLERANCE_M_S => {
            let assessment = toc.thrust_assessment(scatter);
            // A datum engine (scatter 0) keeps the hard check; otherwise a
            // shortfall inside the chart scatter is a flagged warning.
            if scatter > 0.0 && assessment.shortfall() <= scatter {
                return vec![warning(
                    FindingCode::ThrustWithinChartScatter,
                    assessment.describe(
                        &format!(
                            "the design mission's 300 ft/min residual climb at its initial cruise level ({:.0} m), maximum-climb thrust at the top-of-climb mass",
                            toc.altitude_m
                        ),
                        "N",
                    ),
                    Some(assessment.available),
                    Some(assessment.required),
                    "N",
                )];
            }
            vec![(if turboprop { warning } else { error })(
                FindingCode::ThrustMarginViolation,
                format!(
                    "the design mission cannot hold a 300 ft/min residual climb at its initial cruise level ({:.0} m): {:.3} m/s against {:.3} m/s at maximum-climb thrust and the top-of-climb mass",
                    toc.altitude_m,
                    toc.rate_m_s,
                    alas_perf::performance::TOC_RESIDUAL_CLIMB_M_S
                ),
                Some(toc.rate_m_s),
                Some(alas_perf::performance::TOC_RESIDUAL_CLIMB_M_S),
                "m/s",
            )]
        }
        Ok(_) => Vec::new(),
        Err(reason) => vec![warning(
            FindingCode::FieldPerformanceUnavailable,
            format!("the top-of-climb residual climb could not be assessed: {reason}"),
            None,
            None,
            "",
        )],
    }
}
