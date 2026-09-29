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
pub fn design_mass_config(config: &AlasConfig, report: &AnalysisReport) -> AlasConfig {
    config.at_closure_mass(report.analysis_takeoff_mass_kg(config.requirements.mtow_kg))
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
