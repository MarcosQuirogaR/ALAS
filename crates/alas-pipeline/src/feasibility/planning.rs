// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Public planning-envelope comparison and frame conversion.

use alas_config::{
    presets, AircraftReferenceData, AlasConfig, CgEnvelopeCondition, CgEnvelopeEvidence,
    PlanningCgEnvelope, PlanningMacReference,
};
use alas_opt::ModelCgEnvelopeAssessment;

use crate::full_analysis::AnalysisReport;

use super::{
    CgEnvelopeAssessment, OperationalEnvelopeAssessment, PlanningCgStatus, PlanningCurveComparison,
};

pub(super) fn assess_public_cg_reference(
    config: &AlasConfig,
    report: &AnalysisReport,
    analyzed_carried_fuel_kg: f64,
    model_cg: Option<&ModelCgEnvelopeAssessment>,
    operational_envelope: Option<&OperationalEnvelopeAssessment>,
) -> CgEnvelopeAssessment {
    let Ok(preset) = presets::get(&config.preset) else {
        return CgEnvelopeAssessment::default();
    };
    let mass_and_cg = preset.reference.planning_cg_envelope.and_then(|envelope| {
        analyzed_mass_and_cg_pct_mac(report, envelope.mac_reference, analyzed_carried_fuel_kg)
    });
    let mut assessment = assess_reference_limits(&preset.reference, mass_and_cg);
    if let Some(envelope) = preset.reference.planning_cg_envelope {
        attach_mac_frame_comparison(&mut assessment, report, envelope.mac_reference);
        // Sweep every named loading state plus every potato
        // and fuel-vector extreme against both curves, not just the one
        // analyzed point above.
        assessment.flight_curve_comparison = sweep_curve(
            &envelope,
            CgEnvelopeCondition::Flight,
            envelope.mac_reference,
            model_cg,
            operational_envelope,
        );
        assessment.ground_curve_comparison = sweep_curve(
            &envelope,
            CgEnvelopeCondition::Ground,
            envelope.mac_reference,
            model_cg,
            operational_envelope,
        );
    }
    assessment
}

/// One point swept against a planning curve: its own label, mass and CG in
/// the manufacturer's planning frame (`x_m` in the primary aircraft body
/// frame, aft of the nose).
struct SweptPoint {
    label: String,
    mass_kg: f64,
    x_m: f64,
}

/// Every named loading state plus every potato and fuel-vector extreme this
/// run evaluated, converted to the manufacturer planning frame's stations
/// . Loading states come from `model_cg` (ledger-or-lumped
/// states); the potato and fuel-vector extremes come from
/// `operational_envelope` (loading-sequence and fuel-vector sweep). Points from either input that this run did not produce are
/// simply absent, not zero-filled.
fn swept_points(
    model_cg: Option<&ModelCgEnvelopeAssessment>,
    operational_envelope: Option<&OperationalEnvelopeAssessment>,
) -> Vec<SweptPoint> {
    let mut points = Vec::new();
    if let Some(model_cg) = model_cg {
        for state in &model_cg.loading_states {
            points.push(SweptPoint {
                label: state.state.label().to_owned(),
                mass_kg: state.mass_kg,
                x_m: state.cg_x_m,
            });
        }
    }
    if let Some(envelope) = operational_envelope {
        // `potato_checks` already carries the potato's own raw stations
        // (two per sampled mass level, forward and aft), computed before
        // the model's own percent-MAC conversion; reused here rather than
        // recomputing from `potato_pct_mac`, which is already in the
        // model's MAC frame, not the manufacturer's.
        for check in &envelope.potato_checks {
            points.push(SweptPoint {
                label: "potato extreme".to_owned(),
                mass_kg: check.mass_kg,
                x_m: check.x_m,
            });
        }
        for check in &envelope.fuel_vector_checks {
            points.push(SweptPoint {
                label: "fuel-vector extreme".to_owned(),
                mass_kg: check.mass_kg,
                x_m: check.x_m,
            });
        }
    }
    points
}

/// Sweep every point [`swept_points`] can build against one curve
/// (`condition`) of `envelope`, returning the worst (smallest) margin found
/// and which point produced it. `None` when no point's mass fell inside the
/// published table.
fn sweep_curve(
    envelope: &PlanningCgEnvelope,
    condition: CgEnvelopeCondition,
    mac_reference: PlanningMacReference,
    model_cg: Option<&ModelCgEnvelopeAssessment>,
    operational_envelope: Option<&OperationalEnvelopeAssessment>,
) -> Option<PlanningCurveComparison> {
    let mut worst: Option<PlanningCurveComparison> = None;
    let mut points_swept = 0usize;
    for point in swept_points(model_cg, operational_envelope) {
        let Some(limits) = envelope.limits_at(condition, point.mass_kg) else {
            continue;
        };
        let Some(cg_pct_mac) = planning_cg_pct_mac(point.x_m, mac_reference) else {
            continue;
        };
        points_swept += 1;
        let forward_margin_pct_mac = cg_pct_mac - limits.forward_pct_mac;
        let margin_pct_mac = limits.aft_pct_mac.map_or(forward_margin_pct_mac, |aft| {
            forward_margin_pct_mac.min(aft - cg_pct_mac)
        });
        if worst
            .as_ref()
            .is_none_or(|current| margin_pct_mac < current.worst_margin_pct_mac)
        {
            worst = Some(PlanningCurveComparison {
                worst_margin_pct_mac: margin_pct_mac,
                worst_point_label: point.label,
                worst_point_mass_kg: point.mass_kg,
                worst_point_cg_pct_mac: cg_pct_mac,
                points_swept: 0,
            });
        }
    }
    worst.map(|comparison| PlanningCurveComparison {
        points_swept,
        ..comparison
    })
}

/// Record how far the built model's own MAC reference sits from the published
/// planning one.
///
/// The comparison this module performs mixes two frames on purpose and has
/// never said so: the moment sum is built from the **model's** component
/// stations, and the percentage it is converted to is referred to the
/// **manufacturer's** published leading edge and chord. That is the only
/// conversion available (the published table is stated in percent of the real
/// aeroplane's chord), but it is exact only while the two chords coincide.
///
/// The offset is therefore reported beside the verdict rather than corrected
/// away or used to move a vertex: which of the two references is wrong for a
/// given preset is a source-reconciliation question, not something this
/// function may decide.
fn attach_mac_frame_comparison(
    assessment: &mut CgEnvelopeAssessment,
    report: &AnalysisReport,
    reference: PlanningMacReference,
) {
    let published_chord_m = reference.mean_aerodynamic_chord_m;
    if !published_chord_m.is_finite() || published_chord_m <= 0.0 {
        return;
    }
    assessment.published_mac_chord_m = Some(published_chord_m);

    let model_chord_m = report.airplane.c_ref;
    // Canonical MAC frame: the main wing (largest projected
    // area) and its own `mac_station()` leading edge, never reconstructed
    // from `aerodynamic_center(0.25) - 0.25 * c_ref`.
    let model_leading_edge_x_m = report.airplane.mac_frame().map(|frame| frame.x_lemac_m);
    if !model_chord_m.is_finite() || model_chord_m <= 0.0 {
        return;
    }
    assessment.model_mac_length_difference_m = Some(model_chord_m - published_chord_m);
    if !reference.lemac_from_aircraft_nose_m.is_finite() {
        return;
    }
    // Both stations are in the primary aircraft body frame: metres aft of the
    // fuselage nose tip, x positive aft.
    assessment.model_mac_leading_edge_offset_m = model_leading_edge_x_m
        .filter(|value| value.is_finite())
        .map(|model_x_m| model_x_m - reference.lemac_from_aircraft_nose_m);
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

// Tests assert on the assessments they built here, so a failed expect is the
// assertion failing rather than a library invariant breaking.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    /// The A220-300 planning chord, the only one registered in the workspace:
    /// 148.86 in, from the Aircraft Recovery Publication's own frame table.
    const A220_300_PUBLISHED_CHORD_M: f64 = 148.86 * 0.0254;

    fn assessment_with_frame(
        offset_m: Option<f64>,
        length_difference_m: Option<f64>,
    ) -> CgEnvelopeAssessment {
        CgEnvelopeAssessment {
            published_mac_chord_m: Some(A220_300_PUBLISHED_CHORD_M),
            model_mac_leading_edge_offset_m: offset_m,
            model_mac_length_difference_m: length_difference_m,
            ..CgEnvelopeAssessment::default()
        }
    }

    /// The A220-300 is the one preset that can fail the planning check, and
    /// its analyzed percentage is built from model stations but referred to
    /// Airbus' published chord. A datum offset of a tenth of a metre is
    /// already 2.6 % MAC of that chord (comparable with the margin the
    /// baseline's 41.352 % / 37.089 % exceedance turns on), so it must be
    /// reported as a frame disagreement rather than absorbed.
    #[test]
    fn a_datum_offset_larger_than_one_percent_of_the_chord_is_a_frame_disagreement() {
        let assessment = assessment_with_frame(Some(0.10), Some(0.0));
        assert!(assessment.mac_references_disagree());
        let shift = assessment
            .mac_datum_shift_pct_mac()
            .expect("both the offset and the chord are known");
        assert!(
            (shift - 100.0 * 0.10 / A220_300_PUBLISHED_CHORD_M).abs() < 1.0e-9,
            "{shift} % MAC"
        );
        assert!(shift > 2.6 && shift < 2.7, "{shift} % MAC");
    }

    /// A model whose chord and datum both land on the published ones reports
    /// no disagreement, so the note appears only where there is one.
    #[test]
    fn a_matching_frame_reports_no_disagreement() {
        let assessment = assessment_with_frame(Some(0.001), Some(0.001));
        assert!(!assessment.mac_references_disagree());
    }

    /// A chord-length difference alone is a scale error and is caught on its
    /// own axis: the built A220-300 chord is 3.7498 m against the published
    /// 3.7810 m, about 0.031 m, which is inside the length band and must not
    /// on its own be called a disagreement.
    #[test]
    fn the_built_a220_300_chord_difference_alone_stays_inside_the_length_band() {
        let built_chord_m = 3.749_750_794_529_884_2;
        let difference_m = built_chord_m - A220_300_PUBLISHED_CHORD_M;
        assert!(
            difference_m.abs() < 0.02 * A220_300_PUBLISHED_CHORD_M,
            "{difference_m} m"
        );
        let assessment = assessment_with_frame(None, Some(difference_m));
        assert!(!assessment.mac_references_disagree());
        // A large enough scale error is still caught.
        assert!(assessment_with_frame(None, Some(0.5)).mac_references_disagree());
    }

    /// With no registered planning envelope there is no published chord and
    /// therefore nothing to disagree with; the comparison stays silent rather
    /// than defaulting to "agrees".
    #[test]
    fn an_unregistered_envelope_reports_neither_agreement_nor_a_shift() {
        let assessment = CgEnvelopeAssessment::default();
        assert!(!assessment.mac_references_disagree());
        assert_eq!(assessment.mac_datum_shift_pct_mac(), None);
    }

    /// The A220-300 is the one registered preset with a
    /// published planning envelope. Sweeping every named loading state plus
    /// every potato/fuel-vector extreme against both curves must report a
    /// point count on each curve of at least the five named loading states
    /// (every one of them falls inside the table's mass range for this
    /// preset), and the worst point/margin must be internally consistent
    /// (the reported margin is recomputed from the reported point's own
    /// mass/CG and the same curve, matching within floating-point noise).
    #[test]
    fn the_a220_300_sweep_covers_every_loading_state_on_both_curves() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A220-300" }))
            .expect("A220-300 preset config");
        let preset = presets::get("A220-300").expect("registered preset");
        let report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&preset.design_vector, true)
            .expect("A220-300 must analyze");
        let feasibility = crate::feasibility::assess_physical_feasibility(
            &config,
            &preset.design_vector,
            &report,
            None,
        );
        let model_cg = feasibility
            .model_cg
            .as_ref()
            .expect("A220-300 produces a model CG assessment");
        let operational_envelope = feasibility.operational_envelope.as_ref();
        let envelope = preset
            .reference
            .planning_cg_envelope
            .expect("A220-300 registers a planning envelope");

        for condition in [CgEnvelopeCondition::Flight, CgEnvelopeCondition::Ground] {
            let comparison = sweep_curve(
                &envelope,
                condition,
                envelope.mac_reference,
                Some(model_cg),
                operational_envelope,
            )
            .unwrap_or_else(|| panic!("{condition:?} curve must sweep at least one point"));
            assert!(
                comparison.points_swept >= model_cg.loading_states.len(),
                "{condition:?}: swept {} points, expected at least the {} named loading states",
                comparison.points_swept,
                model_cg.loading_states.len()
            );
            let limits = envelope
                .limits_at(condition, comparison.worst_point_mass_kg)
                .expect("the worst point's own mass must lie inside the published table");
            let forward_margin = comparison.worst_point_cg_pct_mac - limits.forward_pct_mac;
            let expected_margin = limits.aft_pct_mac.map_or(forward_margin, |aft| {
                forward_margin.min(aft - comparison.worst_point_cg_pct_mac)
            });
            assert!(
                (comparison.worst_margin_pct_mac - expected_margin).abs() < 1.0e-9,
                "{condition:?}: reported margin {} does not match recomputed margin {} for \
                 point {:?}",
                comparison.worst_margin_pct_mac,
                expected_margin,
                comparison.worst_point_label
            );
        }
    }
}
