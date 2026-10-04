// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Structural results and their evaluated-design provenance.

use std::path::PathBuf;

use alas_config::AlasConfig;
use alas_geom::wing_structure::WingStructureGeometry;
use alas_struct::analytical::StructuralAnalysisReport;
use alas_struct::mesh::MeshHealthReport;
use alas_struct::nastran::NastranResults;
use alas_struct::sizing::WingboxSizing;

use super::WingMassComparison;
use crate::full_analysis::AnalysisReport;

/// Exact inputs associated with a downstream structural result.
///
/// Configuration includes the structural design mass and declared relief.
/// Wing geometry also captures imported or modified airfoil coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluationInputs {
    config: AlasConfig,
    design: alas_config::DesignVector,
    wing: Option<alas_geom::aircraft::wing::Wing>,
    material_allowables_pa: Vec<(i64, f64)>,
    shell_identities: Vec<(i64, i64, i64)>,
}

impl EvaluationInputs {
    pub(crate) fn new(config: &AlasConfig, report: &AnalysisReport) -> Self {
        Self {
            config: config.clone(),
            design: report.design,
            wing: report.airplane.wings.first().cloned(),
            material_allowables_pa: Vec::new(),
            shell_identities: Vec::new(),
        }
    }

    pub(crate) fn from_deck(
        config: &AlasConfig,
        report: &AnalysisReport,
        deck: &alas_struct::mesh::Deck,
    ) -> Self {
        let mut inputs = Self::new(config, report);
        let cfg = &config.structures;
        let materials: Vec<_> = [
            &cfg.skin_material,
            &cfg.spar_web_material,
            &cfg.spar_cap_material,
            &cfg.rib_material,
        ]
        .into_iter()
        .filter_map(|name| alas_config::materials::get(name).ok())
        .collect();
        for submitted in deck.materials() {
            // Match only the declared materials against actual MAT1 cards.
            // If two declarations have identical elastic/density properties,
            // use their smaller allowable rather than guessing an identity.
            let allowable = materials
                .iter()
                .filter(|material| {
                    material.e_pa == submitted.e
                        && material.nu == submitted.nu
                        && material.rho_kg_m3 == submitted.rho
                })
                .map(|material| alas_struct::allowables::bending_allowable_pa(material))
                .fold(f64::INFINITY, f64::min);
            if allowable.is_finite() && allowable > 0.0 {
                inputs
                    .material_allowables_pa
                    .push((submitted.mid, allowable));
            }
        }
        for shell in deck.quads().iter().chain(deck.trias()) {
            if let Some(property) = deck.shell_properties().iter().find(|p| p.pid == shell.pid) {
                inputs
                    .shell_identities
                    .push((shell.eid, shell.pid, property.mid1));
            }
        }
        inputs
    }

    pub(crate) fn material_allowable_pa(&self, mid: i64) -> Option<f64> {
        self.material_allowables_pa
            .iter()
            .find(|(id, _)| *id == mid)
            .map(|(_, value)| *value)
    }

    pub(crate) fn shell_identities(&self) -> &[(i64, i64, i64)] {
        &self.shell_identities
    }

    pub(crate) fn matches(
        &self,
        config: &AlasConfig,
        design: &alas_config::DesignVector,
        report: &AnalysisReport,
    ) -> bool {
        self.config == *config
            && self.design == *design
            && report.design == *design
            && self.wing.as_ref() == report.airplane.wings.first()
    }
}

/// Complete structural analysis outcomes for a pipeline run.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralAnalysisResult {
    /// Overall structural status (`"ok"`, `"not_run"`, or `"error"`).
    pub status: String,
    /// Failure message if an error occurred.
    pub error: Option<String>,
    /// The rib/spar wingbox geometry the sizing and mesh were built from.
    pub wsg: Option<WingStructureGeometry>,
    /// Sized wingbox internal structural dimensions and masses.
    pub sizing: Option<WingboxSizing>,
    /// Finite-element mesh geometric health validation metrics.
    pub mesh_health: Option<MeshHealthReport>,
    /// Closed-form analytical deflection, stress, and natural frequency estimates.
    pub analysis: Option<StructuralAnalysisReport>,
    /// NASTRAN finite-element solution results, if executed.
    pub nastran: Option<NastranResults>,
    /// Independent NASA NASTRAN-95 solution results, when run alongside MSC.
    pub nastran95: Option<NastranResults>,
    /// Patran deformation renders, if an external export was performed.
    pub patran: Option<PatranExportResult>,
    /// FLOPS complete-wing mass of the mass ledger, in kg, for comparison.
    pub torenbeek_wing_mass_kg: f64,
    /// The wing masses of this run, shared with the feasibility findings.
    pub wing_mass: Option<WingMassComparison>,
    /// Inputs of this evaluation; absent results cannot replace native gates.
    pub evaluation_inputs: Option<EvaluationInputs>,
}

/// Output of the optional Patran deformation-render export.
///
/// The Python result stores an insertion-ordered dictionary of load-case names
/// to PNG paths. A vector preserves that order without introducing a map whose
/// iteration order would differ from the render order.
#[derive(Debug, Clone, PartialEq)]
pub struct PatranExportResult {
    /// Export state (`"not_run"`, `"ok"`, or `"error"`).
    pub status: String,
    /// Failure or partial-export detail, when present. Error text preserves
    /// the boundary category (absent, incomplete, invalid, launch, timeout,
    /// or artifact validation) so the GUI can provide recovery guidance.
    pub error: Option<String>,
    /// Load-case names and the corresponding rendered PNG paths.
    pub png_paths: Vec<(String, PathBuf)>,
}

impl Default for PatranExportResult {
    fn default() -> Self {
        Self {
            status: "not_run".to_owned(),
            error: None,
            png_paths: Vec::new(),
        }
    }
}

impl Default for StructuralAnalysisResult {
    fn default() -> Self {
        Self {
            status: "not_run".to_owned(),
            error: None,
            wsg: None,
            sizing: None,
            mesh_health: None,
            analysis: None,
            nastran: None,
            nastran95: None,
            patran: None,
            torenbeek_wing_mass_kg: f64::NAN,
            wing_mass: None,
            evaluation_inputs: None,
        }
    }
}
