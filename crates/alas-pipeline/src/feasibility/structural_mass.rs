// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Structural-mass limits: the payload cap and a registered aircraft's
//! published maximum zero-fuel mass.

use std::collections::HashMap;

use alas_config::{presets, AlasConfig, DesignVector, MassArchitecture};
use alas_mass::breakdown::{OEW_KEYS, PAYLOAD};

use crate::full_analysis::AnalysisReport;

use super::{error, FindingCode, FuelLoadingAssessment, PhysicalFinding};

/// Difference between the two operating-empty masses beyond which the mass
/// closure is reported as not having closed on the modeled buildup, in kg.
///
/// Well below any mass-model disagreement worth calibrating against, and well
/// above the floating-point noise of an eight-term sum.
const OEW_CLOSURE_RESIDUAL_TOLERANCE_KG: f64 = 1.0;

/// The two operating-empty masses a completed run carries, which are not the
/// same number and were previously published as if they were.
///
/// See [`modeled_operating_empty_mass`] for why the distinction matters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct OperatingEmptyMassReconciliation {
    /// Sum of the eight [`OEW_KEYS`] component masses the analysis actually
    /// modeled, kg. This is the quantity a reference OEW is comparable with.
    pub(super) modeled_kg: f64,
    /// `zero_fuel_mass_kg - payload_kg`, kg: what the MTOW mass closure has
    /// left over once payload is removed.
    pub(super) closure_residual_kg: f64,
    /// Which mass architecture produced [`Self::modeled_kg`].
    ///
    /// Carried because an operating-empty mass is meaningless without it: the
    /// production FLOPS buildup and the legacy reference-compatible one are
    /// different models of the same aeroplane and answer differently. The
    /// ATR 72-600's 12,014.6 kg figure in `docs/aircraft-parity.md` and the
    /// 15,257 kg the 2026-09-22 baseline reports are both real outputs of
    /// this workspace; neither document states which architecture produced
    /// it, which is why they cannot currently be reconciled. Every figure
    /// this function returns now states it.
    pub(super) architecture: MassArchitecture,
}

impl OperatingEmptyMassReconciliation {
    /// `closure_residual_kg - modeled_kg`, kg: everything the closure absorbs
    /// into "empty" that the modeled buildup does not contain.
    fn residual_excess_kg(self) -> f64 {
        self.closure_residual_kg - self.modeled_kg
    }

    /// Whether the two figures disagree by more than summation noise.
    fn disagrees(self) -> bool {
        self.residual_excess_kg().abs() > OEW_CLOSURE_RESIDUAL_TOLERANCE_KG
    }
}

/// Reconcile the modeled operating-empty mass with the closure residual.
///
/// `FuelLoadingAssessment::zero_fuel_mass_kg` is `mtow_kg -
/// mtow_closure_fuel_kg`: a *residual* of the takeoff-mass closure, so it
/// absorbs every kilogram the closure carries that the eight-component
/// buildup does not (unusable fuel, and any non-closure of the sizing loop).
/// Subtracting payload from it and calling the result "modeled OEW" therefore
/// publishes a closure residual under the name of a modeled quantity, and a
/// reference-OEW comparison drawn against it is not comparing like with like.
///
/// This is the exact origin of the two ATR 72-600 operating-empty figures the
/// eight-preset baseline prints side by side: 15,257 kg in the matrix table
/// (the modeled buildup) against "modeled OEW 15299.0 kg" in the finding text
/// (the closure residual, 42 kg heavier). Neither number was wrong; they
/// answer different questions and only one of them was labelled.
///
/// Returns `None` when either figure is unavailable or non-finite: a missing
/// datum stays missing rather than defaulting to the other one.
pub(super) fn modeled_operating_empty_mass(
    component_masses: &HashMap<String, f64>,
    zero_fuel_mass_kg: f64,
    payload_kg: Option<f64>,
    architecture: MassArchitecture,
) -> Option<OperatingEmptyMassReconciliation> {
    let mut modeled_kg = 0.0;
    for key in OEW_KEYS {
        let component_kg = component_masses.get(key).copied()?;
        if !component_kg.is_finite() {
            return None;
        }
        modeled_kg += component_kg;
    }
    let payload_kg = payload_kg.filter(|value| value.is_finite())?;
    if !zero_fuel_mass_kg.is_finite() {
        return None;
    }
    Some(OperatingEmptyMassReconciliation {
        modeled_kg,
        closure_residual_kg: zero_fuel_mass_kg - payload_kg,
        architecture,
    })
}

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
        // The message below deliberately reports the zero-fuel excess against
        // the published MZFW. The operating-empty comparison beside it names
        // the *modeled* buildup, not the closure residual the zero-fuel mass
        // carries, and states the residual separately when the two differ:
        // see `modeled_operating_empty_mass`. Reporting one number for both
        // is what produced the ATR 72-600's 15,257 kg / 15,299 kg conflict.
        let reconciliation = modeled_operating_empty_mass(
            &report.component_masses,
            fuel_loading.zero_fuel_mass_kg,
            payload_kg,
            config.mass_model.mass_architecture,
        );
        let calibration_note = match (preset.reference.oew_kg, reconciliation) {
            (Some(reference_oew_kg), Some(reconciliation)) => {
                let residual_note = if reconciliation.disagrees() {
                    format!(
                        "; the takeoff-mass closure carries a further {:.1} kg beyond the modeled \
                         buildup (unusable fuel and any unclosed sizing residual), so the \
                         zero-fuel mass implies {:.1} kg empty and the two must not be quoted \
                         interchangeably",
                        reconciliation.residual_excess_kg(),
                        reconciliation.closure_residual_kg
                    )
                } else {
                    String::new()
                };
                format!(
                    "; modeled OEW {:.1} kg under the {} mass architecture versus reference OEW \
                     {:.1} kg{}",
                    reconciliation.modeled_kg,
                    reconciliation.architecture.as_str(),
                    reference_oew_kg,
                    residual_note
                )
            }
            _ => String::new(),
        };
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

// Tests assert on the fixtures they built here, so a failed expect is the
// assertion failing rather than a library invariant breaking.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_mass::breakdown::{
        FURNISHINGS, FUSELAGE, GEAR, H_STAB, PROPULSION, SYSTEMS, V_STAB, WING,
    };

    /// The eight modeled component masses of the ATR 72-600 as the
    /// 2026-09-22 eight-preset baseline recorded them
    /// (`preset-validation-harness/baseline/preset_acceptance_matrix.json`,
    /// `ATR72-600.model_audit.mass_balance.component_masses_kg`). They sum to
    /// the 15,257 kg the matrix table prints in its OEW column.
    fn atr72_600_baseline_components() -> HashMap<String, f64> {
        [
            (WING, 2_220.105_546_708_234_7),
            (H_STAB, 264.888_687_638_446_87),
            (V_STAB, 204.141_449_040_145_86),
            (FUSELAGE, 3_309.661_139_161_921),
            (GEAR, 1_177.145_497_903_563),
            (PROPULSION, 2_132.267_709_668_914_4),
            (SYSTEMS, 2_061.779_910_679_156_3),
            (FURNISHINGS, 3_887.002_512_085_494),
        ]
        .into_iter()
        .map(|(name, mass_kg)| (name.to_owned(), mass_kg))
        .collect()
    }

    /// The reconciliation under the production architecture, which is what
    /// the 2026-09-22 baseline ran.
    fn reconcile(
        component_masses: &HashMap<String, f64>,
        zero_fuel_mass_kg: f64,
        payload_kg: Option<f64>,
    ) -> Option<OperatingEmptyMassReconciliation> {
        modeled_operating_empty_mass(
            component_masses,
            zero_fuel_mass_kg,
            payload_kg,
            MassArchitecture::PureFlopsTransportV1,
        )
    }

    /// The two ATR 72-600 operating-empty figures the baseline published
    /// beside each other must now come out as two named quantities rather
    /// than one number quoted twice: the modeled buildup at 15,257 kg and the
    /// takeoff-mass closure residual at 15,299 kg, 42 kg apart.
    ///
    /// The regression is on the distinction, not on either value: this fixes
    /// which question the reported number answers and does not choose an OEW.
    #[test]
    fn the_atr72_600_closure_residual_is_not_the_modeled_operating_empty_mass() {
        // MTOW 23,000 kg less the 701.008 kg closure fuel the baseline
        // recorded, against its 7,000 kg modeled payload.
        let zero_fuel_mass_kg = 23_000.0 - 701.007_547_114_124_5;
        let reconciliation = reconcile(
            &atr72_600_baseline_components(),
            zero_fuel_mass_kg,
            Some(7_000.0),
        )
        .expect("every ATR component mass is present and finite");

        // A figure without its architecture is not reconcilable against
        // another document's figure; the baseline ran the production one.
        assert_eq!(
            reconciliation.architecture,
            MassArchitecture::PureFlopsTransportV1
        );

        assert!(
            (reconciliation.modeled_kg - 15_256.992_452_885_875).abs() < 1.0e-6,
            "modeled buildup {} kg",
            reconciliation.modeled_kg
        );
        assert!(
            (reconciliation.closure_residual_kg - 15_298.992_452_885_875).abs() < 1.0e-6,
            "closure residual {} kg",
            reconciliation.closure_residual_kg
        );
        assert!(
            (reconciliation.residual_excess_kg() - 42.0).abs() < 1.0e-6,
            "residual excess {} kg",
            reconciliation.residual_excess_kg()
        );
        assert!(reconciliation.disagrees());
    }

    /// A closure that does land on the modeled buildup reports no residual,
    /// so the extra sentence appears only where there is something to say.
    #[test]
    fn a_closed_mass_budget_reports_no_residual() {
        let components = atr72_600_baseline_components();
        let modeled_kg: f64 = OEW_KEYS
            .iter()
            .map(|key| components.get(*key).copied().unwrap_or_default())
            .sum();
        let reconciliation = reconcile(&components, modeled_kg + 7_000.0, Some(7_000.0))
            .expect("a closed budget reconciles");
        assert!(!reconciliation.disagrees());
        assert!(reconciliation.residual_excess_kg().abs() < 1.0e-9);
    }

    /// A missing or non-finite input leaves the comparison unavailable rather
    /// than defaulting one figure to the other.
    #[test]
    fn a_missing_component_or_payload_leaves_the_reconciliation_unavailable() {
        let mut components = atr72_600_baseline_components();
        assert_eq!(reconcile(&components, 22_299.0, None), None);
        assert_eq!(reconcile(&components, f64::NAN, Some(7_000.0)), None);
        components.remove(GEAR);
        assert_eq!(reconcile(&components, 22_299.0, Some(7_000.0)), None);
        let mut broken = atr72_600_baseline_components();
        broken.insert(WING.to_owned(), f64::NAN);
        assert_eq!(reconcile(&broken, 22_299.0, Some(7_000.0)), None);
    }
}
