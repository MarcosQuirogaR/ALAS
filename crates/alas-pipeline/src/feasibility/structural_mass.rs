// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Structural-mass limits: the payload cap and a registered aircraft's
//! published maximum zero-fuel mass.

use alas_config::{presets, AlasConfig, DesignVector};
use alas_mass::breakdown::PAYLOAD;

use crate::full_analysis::AnalysisReport;

use super::{error, FindingCode, FuelLoadingAssessment, PhysicalFinding};

pub(super) fn append_structural_mass_findings(
    config: &AlasConfig,
    design: &DesignVector,
    report: &AnalysisReport,
    fuel_loading: &FuelLoadingAssessment,
    findings: &mut Vec<PhysicalFinding>,
) {
    let tolerance = config.requirements.mtow_kg.abs().max(1.0) * 1.0e-10;

    let payload_kg = report.component_masses.get(PAYLOAD).copied();
    let structural_limit_kg = config.requirements.max_structural_payload_kg;
    if structural_limit_kg.is_finite() && structural_limit_kg > 0.0 {
        if let Some(payload_kg) = payload_kg {
            if payload_kg.is_finite() && payload_kg > structural_limit_kg + tolerance {
                findings.push(error(
                    FindingCode::StructuralPayloadLimitViolation,
                    format!(
                        "modeled payload exceeds the configured structural payload limit by {:.3} kg",
                        payload_kg - structural_limit_kg
                    ),
                    Some(payload_kg),
                    Some(structural_limit_kg),
                    "kg",
                ));
            }
        }
    }

    // Published weight limits are valid only for the unchanged registered
    // design. A modified design may use the same preset name while having a
    // different geometry or mass buildup, so do not apply the source value to
    // that notional case.
    let Ok(preset) = presets::get(&config.preset) else {
        return;
    };
    if *design != preset.design_vector {
        return;
    }
    let Some(mzfw_kg) = preset.reference.mzfw_kg else {
        return;
    };
    if mzfw_kg.is_finite()
        && fuel_loading.zero_fuel_mass_kg.is_finite()
        && fuel_loading.zero_fuel_mass_kg > mzfw_kg + tolerance
    {
        let modeled_oew_kg = match (preset.reference.oew_kg, payload_kg) {
            (Some(reference_oew_kg), Some(payload_kg)) => Some((
                // The message below deliberately reports the zero-fuel
                // excess, while this local value lets us name the calibration
                // mismatch when reference OEW evidence exists.
                fuel_loading.zero_fuel_mass_kg - payload_kg,
                reference_oew_kg,
            )),
            _ => None,
        };
        let calibration_note = modeled_oew_kg
            .map(|(modeled, reference)| {
                format!(
                    "; modeled OEW {:.1} kg versus reference OEW {:.1} kg",
                    modeled, reference
                )
            })
            .unwrap_or_default();
        findings.push(error(
            FindingCode::MaximumZeroFuelWeightViolation,
            format!(
                "modeled zero-fuel mass exceeds the published MZFW by {:.3} kg{}; reduce payload or calibrate the mass model before using this load case",
                fuel_loading.zero_fuel_mass_kg - mzfw_kg,
                calibration_note
            ),
            Some(fuel_loading.zero_fuel_mass_kg),
            Some(mzfw_kg),
            "kg",
        ));
    }
}
