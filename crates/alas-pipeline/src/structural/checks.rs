// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Structural sizing failures and unavailable external-tool diagnostics.
use super::{PatranExportResult, StructuralAnalysisResult, WingboxSizing};
use std::path::Path;

pub(super) fn mesh_mass_failure(deck: &alas_struct::mesh::Deck) -> Option<String> {
    let mass = deck.primary_structural_mass_kg().map(|mass| 2.0 * mass);
    material_mass_failure(mass)
}

fn material_mass_failure(mass: Option<f64>) -> Option<String> {
    mass.is_none_or(|mass| !mass.is_finite() || mass <= 0.0)
        .then(|| {
            format!("FE primary material mass {mass:?} kg is invalid or could not be established")
        })
}

pub(super) fn mesh_mass_diagnostic(
    deck: &alas_struct::mesh::Deck,
    empirical_kg: f64,
) -> Option<String> {
    mass_diagnostic(2.0 * deck.primary_structural_mass_kg()?, empirical_kg)
}

fn mass_diagnostic(actual: f64, empirical_kg: f64) -> Option<String> {
    (actual.is_finite() && actual > 0.0 && empirical_kg.is_finite() && empirical_kg > 0.0).then(|| format!(
        "Structural mass comparison (diagnostic only): FE primary material {actual:.3} kg, empirical FLOPS complete wing {empirical_kg:.3} kg, difference {:+.3} kg. These model inventories are not equivalent; their difference is not a physical constraint.", actual-empirical_kg))
}

pub(super) fn sizing_failure_detail(sizing: &WingboxSizing) -> Option<String> {
    let mut failures = Vec::new();
    if !sizing.rib_spacing_pass() {
        failures.push(format!(
            "installed rib spacing {:.6} m exceeds allowable {:.6} m",
            sizing.installed_rib_spacing_m(),
            sizing.rib_spacing_m
        ));
    }
    if !sizing.strength_margins_pass() {
        failures.push(match sizing.controlling_margin() {
            Some(c) if c.margin.is_finite() => format!(
                "wingbox strength sizing is infeasible: minimum margin {:.6e} at spar {} \
                 (chord fraction {:.3}), station {} (y={:.4} m, eta={:.4})",
                c.margin, c.spar_index, c.chord_fraction, c.station_index, c.y_m, c.eta,
            ),
            Some(c) => format!(
                "wingbox strength sizing produced a non-finite margin at spar {}, station {} \
                 (y={:.4} m, eta={:.4})",
                c.spar_index, c.station_index, c.y_m, c.eta,
            ),
            None => "wingbox strength sizing produced no spar stations".to_owned(),
        });
    }
    (!failures.is_empty()).then(|| failures.join("; "))
}

pub(super) fn missing_patran_result(configured: &str) -> PatranExportResult {
    let detail = if configured.trim().is_empty() {
        "Patran executable absent: configure it under Setup > External Tools".to_owned()
    } else if Path::new(configured).is_dir() {
        format!("Patran installation incomplete at {configured}: executable launcher is missing")
    } else if Path::new(configured).exists()
        || Path::new(configured).parent().is_some_and(Path::is_dir)
    {
        format!("Patran executable path is invalid: {configured} is not a regular executable file")
    } else {
        format!("Patran executable absent at {configured}")
    };
    PatranExportResult {
        status: "error".to_owned(),
        error: Some(detail),
        png_paths: Vec::new(),
    }
}

pub(super) fn structural_error(msg: String, torenbeek: f64) -> StructuralAnalysisResult {
    StructuralAnalysisResult {
        status: "error".to_owned(),
        error: Some(msg),
        wsg: None,
        sizing: None,
        mesh_health: None,
        analysis: None,
        nastran: None,
        nastran95: None,
        patran: None,
        torenbeek_wing_mass_kg: torenbeek,
        wing_mass: None,
        evaluation_inputs: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_struct::mesh::Deck;

    #[test]
    fn pre_solver_gate_does_not_reject_a_mass_discrepancy() {
        assert!(material_mass_failure(Some(40.0)).is_none());
        let diagnostic = mass_diagnostic(40.0, 1.0).unwrap_or_default();
        assert!(diagnostic.contains("40.000 kg"));
        assert!(diagnostic.contains("+39.000 kg"));
        assert!(diagnostic.contains("diagnostic only"));
    }

    #[test]
    fn pre_solver_gate_rejects_invalid_or_empty_material_inventory() {
        assert!(mesh_mass_failure(&Deck::new()).is_some());
        for invalid in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
            assert!(material_mass_failure(Some(invalid)).is_some());
        }
    }
}
