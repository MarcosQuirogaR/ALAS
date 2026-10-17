// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The design mass every load-dependent consumer reads.

use alas_config::AlasConfig;
use alas_perf::performance::{build_vn_diagram, VnDiagramData};

use crate::full_analysis::AnalysisReport;

/// The configuration with its mass ledger closed at the mass the report was
/// evaluated at ([`AnalysisReport::analysis_takeoff_mass_kg`]).
///
/// The structural loads and the V-n envelope both read this configuration, so
/// a sized design is never judged at the declared MTOW limit of a lighter
/// aircraft. A registered aircraft keeps its declared design gross and landing
/// masses for the structure; a clean-sheet design follows the closure mass.
/// A report bound to a sized mass follows the takeoff-mass sizing plan
/// (`AlasConfig::at_sized_closure_mass`), under which the MTOW band and
/// payload-adjusted modes design every structure at the closure; an unsized
/// (baseline) report ignores the sizing mode.
pub fn design_mass_config(config: &AlasConfig, report: &AnalysisReport) -> AlasConfig {
    match report.sized_takeoff_mass_kg() {
        Some(closure_mass_kg) => config.at_sized_closure_mass(closure_mass_kg),
        None => config.at_closure_mass(config.requirements.mtow_kg),
    }
}

/// The maximum landing mass, kg, a report is checked against.
///
/// An unsized (baseline) report reads the declared limit
/// (`AlasConfig::landing_mass_limit_kg`). A report bound to a sized mass under
/// a plan that designs the structure at the closure (the MTOW band and
/// payload-adjusted modes) reads the design landing mass of that closure
/// (`AlasConfig::design_landing_mass_at_closure`), the `WLDG` the structure
/// was actually sized for.
pub fn landing_mass_limit_kg(config: &AlasConfig, report: &AnalysisReport) -> f64 {
    let closure_mass_kg = report.sized_takeoff_mass_kg().filter(|_| {
        config.mtow_plan().structural_basis == alas_config::StructuralBasis::ClosureMass
    });
    if closure_mass_kg.is_some() {
        if let Some(design_kg) = report.design_landing_mass_kg() {
            return design_kg;
        }
    }
    match closure_mass_kg {
        Some(closure_mass_kg) => config.design_landing_mass_at_closure(closure_mass_kg),
        None => config.landing_mass_limit_kg(config.requirements.mtow_kg),
    }
}

/// The destination landing mass, kg, of the design mission the maximum landing
/// mass of [`landing_mass_limit_kg`] was derived from, or `None` when that
/// limit is a declared one.
///
/// A sized closure derives its design landing mass from its own design
/// mission (`WLDG = max(mlw_fraction x closure, landing mass of the
/// mission)`), so that mission's arrival is the quantity the limit refers to
/// and the one the search constrains in its `landing_mass` residual. The
/// route the report flies afterwards can be longer than the design range and
/// carries proportionally more contingency fuel; its arrival mass is reported
/// against the same limit, but it is not the mission the limit was built on.
pub fn design_mission_landing_mass_kg(config: &AlasConfig, report: &AnalysisReport) -> Option<f64> {
    let closure_derived = report.sized_takeoff_mass_kg().is_some()
        && config.mtow_plan().structural_basis == alas_config::StructuralBasis::ClosureMass
        && report.design_landing_mass_kg().is_some();
    closure_derived
        .then(|| {
            report
                .fuel
                .sized_fuel()
                .map(|sized| sized.design_landing_mass_kg)
        })
        .flatten()
        .filter(|mass_kg| mass_kg.is_finite() && *mass_kg > 0.0)
}

/// The landing-mass findings of a report.
///
/// With a closure-derived limit, the hard check compares the design mission's
/// arrival ([`design_mission_landing_mass_kg`]) with the limit, the same
/// quantity and the same strict comparison as the search's `landing_mass`
/// residual. The flown route's arrival mass is compared with the same limit
/// and a longer route's excess is a warning with both masses, because the
/// limit was derived from the design mission and not from that route. Against
/// a declared limit the flown route's arrival is the hard check, as before.
pub(super) fn landing_mass_findings(
    design_mission_kg: Option<f64>,
    route_arrival_kg: Option<f64>,
    limit_kg: f64,
    findings: &mut Vec<super::PhysicalFinding>,
) {
    const EXCEEDS: &str = "analyzed arrival mass exceeds the configured maximum landing mass";
    match design_mission_kg {
        Some(design_kg) => {
            if design_kg > limit_kg {
                findings.push(super::error(
                    super::FindingCode::LandingMassLimitViolation,
                    EXCEEDS,
                    Some(design_kg),
                    Some(limit_kg),
                    "kg",
                ));
            }
            if let Some(route_kg) = route_arrival_kg.filter(|mass_kg| *mass_kg > limit_kg) {
                findings.push(super::warning(
                    super::FindingCode::LandingMassLimitViolation,
                    "the flown route arrives above the design landing mass, which is derived from the design mission; the design mission itself lands within it",
                    Some(route_kg),
                    Some(limit_kg),
                    "kg",
                ));
            }
        }
        None => {
            if let Some(route_kg) = route_arrival_kg.filter(|mass_kg| *mass_kg > limit_kg) {
                findings.push(super::error(
                    super::FindingCode::LandingMassLimitViolation,
                    EXCEEDS,
                    Some(route_kg),
                    Some(limit_kg),
                    "kg",
                ));
            }
        }
    }
}

/// The V-n diagram of the analysed aircraft: the built reference area at the
/// structural design mass of [`design_vn_mass_kg`], the mass the wing-box loads
/// are sized for.
pub fn design_vn_diagram(config: &AlasConfig, report: &AnalysisReport) -> VnDiagramData {
    let design = design_mass_config(config, report);
    let mut requirements = design.requirements.clone();
    requirements.mtow_kg = design_vn_mass_kg(config, report);
    build_vn_diagram(
        report.airplane.s_ref,
        &requirements,
        &design.performance,
        design.requirements.cruise_altitude_m,
    )
}

/// Design mass, kg, the V-n diagram of [`design_vn_diagram`] is evaluated at:
/// the structural design gross mass of [`design_mass_config`] (the declared
/// design gross weight of a registered aircraft, the closure mass of a
/// clean-sheet design), so the envelope and the structural loads agree.
pub fn design_vn_mass_kg(config: &AlasConfig, report: &AnalysisReport) -> f64 {
    alas_opt::mdo::structural_feasibility::structural_design_mass_kg(&design_mass_config(
        config, report,
    ))
}

#[cfg(test)]
mod tests {
    use super::super::FindingSeverity;
    use super::*;

    fn severities(
        design: Option<f64>,
        route: Option<f64>,
        limit: f64,
    ) -> Vec<(FindingSeverity, Option<f64>)> {
        let mut findings = Vec::new();
        landing_mass_findings(design, route, limit, &mut findings);
        findings
            .into_iter()
            .map(|finding| (finding.severity, finding.actual))
            .collect()
    }

    #[test]
    fn a_design_mission_on_its_own_limit_passes_and_a_longer_route_is_a_warning() {
        // 2026-10-15 150-seat brief: the design mission lands on its limit
        // exactly, the 3,048 nmi route 10.4 kg above it.
        let limit = 58_749.194_106_56;
        assert!(severities(Some(limit), None, limit).is_empty());
        assert_eq!(
            severities(Some(limit), Some(58_759.557), limit),
            vec![(FindingSeverity::Warning, Some(58_759.557))]
        );
    }

    #[test]
    fn a_design_mission_above_its_limit_is_an_error_without_a_margin() {
        let limit = 58_749.0;
        assert_eq!(
            severities(Some(limit + 1.0e-9), None, limit),
            vec![(FindingSeverity::Error, Some(limit + 1.0e-9))]
        );
    }

    #[test]
    fn a_declared_limit_keeps_the_flown_route_as_the_hard_check() {
        assert_eq!(
            severities(None, Some(66_001.0), 66_000.0),
            vec![(FindingSeverity::Error, Some(66_001.0))]
        );
        assert!(severities(None, Some(65_999.0), 66_000.0).is_empty());
        assert!(severities(None, None, 66_000.0).is_empty());
    }
}
