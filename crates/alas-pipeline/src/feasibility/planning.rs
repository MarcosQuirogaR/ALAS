// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Public planning-envelope comparison and frame conversion.

use alas_config::{
    presets, AircraftReferenceData, AlasConfig, CgEnvelopeCondition, CgEnvelopeEvidence,
    PlanningMacReference,
};

use crate::full_analysis::AnalysisReport;

use super::{CgEnvelopeAssessment, PlanningCgStatus};

pub(super) fn assess_public_cg_reference(
    config: &AlasConfig,
    report: &AnalysisReport,
    analyzed_carried_fuel_kg: f64,
) -> CgEnvelopeAssessment {
    let Ok(preset) = presets::get(&config.preset) else {
        return CgEnvelopeAssessment::default();
    };
    let mass_and_cg = preset.reference.planning_cg_envelope.and_then(|envelope| {
        analyzed_mass_and_cg_pct_mac(report, envelope.mac_reference, analyzed_carried_fuel_kg)
    });
    assess_reference_limits(&preset.reference, mass_and_cg)
}

fn analyzed_mass_and_cg_pct_mac(
    report: &AnalysisReport,
    mac_reference: PlanningMacReference,
    analyzed_carried_fuel_kg: f64,
) -> Option<(f64, f64)> {
    let names = [
        alas_mass::breakdown::WING,
        alas_mass::breakdown::H_STAB,
        alas_mass::breakdown::V_STAB,
        alas_mass::breakdown::FUSELAGE,
        alas_mass::breakdown::GEAR,
        alas_mass::breakdown::PROPULSION,
        alas_mass::breakdown::SYSTEMS,
        alas_mass::breakdown::FURNISHINGS,
        alas_mass::breakdown::PAYLOAD,
    ];
    let fuel_name = alas_mass::breakdown::FUEL;
    if !analyzed_carried_fuel_kg.is_finite() || analyzed_carried_fuel_kg < 0.0 {
        return None;
    }
    let mut mass_kg = analyzed_carried_fuel_kg;
    let mut moment_x_kg_m = 0.0;
    for name in names {
        let component_mass = report.component_masses.get(name).copied()?;
        let coordinate = report.mass_coordinates.get(name).copied()?;
        if !component_mass.is_finite() || !coordinate[0].is_finite() {
            return None;
        }
        mass_kg += component_mass.max(0.0);
        moment_x_kg_m += component_mass.max(0.0) * coordinate[0];
    }
    let fuel_coordinate = report.mass_coordinates.get(fuel_name).copied()?;
    if !fuel_coordinate[0].is_finite() {
        return None;
    }
    moment_x_kg_m += analyzed_carried_fuel_kg * fuel_coordinate[0];

    if !mass_kg.is_finite() || mass_kg <= 0.0 || !moment_x_kg_m.is_finite() {
        return None;
    }
    let cg_pct_mac = planning_cg_pct_mac(moment_x_kg_m / mass_kg, mac_reference)?;
    cg_pct_mac.is_finite().then_some((mass_kg, cg_pct_mac))
}

pub(super) fn planning_cg_pct_mac(
    cg_from_aircraft_nose_m: f64,
    reference: PlanningMacReference,
) -> Option<f64> {
    if !cg_from_aircraft_nose_m.is_finite()
        || !reference.lemac_from_aircraft_nose_m.is_finite()
        || !reference.mean_aerodynamic_chord_m.is_finite()
        || reference.mean_aerodynamic_chord_m <= 0.0
    {
        return None;
    }
    Some(
        100.0 * (cg_from_aircraft_nose_m - reference.lemac_from_aircraft_nose_m)
            / reference.mean_aerodynamic_chord_m,
    )
}

pub(super) fn assess_reference_limits(
    reference: &AircraftReferenceData,
    mass_and_cg: Option<(f64, f64)>,
) -> CgEnvelopeAssessment {
    let mut assessment = CgEnvelopeAssessment {
        evidence: reference.cg_evidence,
        ..CgEnvelopeAssessment::default()
    };
    if reference.cg_evidence != CgEnvelopeEvidence::PublicPlanning {
        return assessment;
    }
    let Some(envelope) = reference.planning_cg_envelope else {
        return assessment;
    };
    assessment.source = Some(envelope.source);
    assessment.controlling_document = Some(envelope.controlling_document);

    let Some((mass_kg, cg_pct_mac)) = mass_and_cg else {
        return assessment;
    };
    assessment.mass_kg = Some(mass_kg);
    assessment.cg_pct_mac = Some(cg_pct_mac);

    let Some(limits) = envelope.limits_at(CgEnvelopeCondition::Flight, mass_kg) else {
        return assessment;
    };
    assessment.forward_limit_pct_mac = Some(limits.forward_pct_mac);
    assessment.aft_limit_pct_mac = limits.aft_pct_mac;
    assessment.planning_status = if cg_pct_mac < limits.forward_pct_mac {
        PlanningCgStatus::ForwardLimitViolation
    } else if let Some(aft_limit) = limits.aft_pct_mac {
        if cg_pct_mac > aft_limit {
            PlanningCgStatus::AftLimitViolation
        } else {
            PlanningCgStatus::WithinPublishedLimits
        }
    } else {
        PlanningCgStatus::AftLimitNotPublished
    };
    assessment
}
