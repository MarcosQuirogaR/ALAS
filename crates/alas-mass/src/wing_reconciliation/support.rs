// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The design-weight basis of the structural diagnostics, and the
//! reference-mode reconciliation that reports instead of aborting.

use alas_config::{AlasConfig, DesignRequirements};
use alas_struct::sizing::CompositeProxyDeclaration;

use crate::wingbox_feedback::{
    reconcile_reference_wing, ReferenceWingMass, SizedWingboxMass, WingExtent, WingboxFeedback,
    WingboxFeedbackError, WingboxFeedbackMode,
};

use crate::wing_inventory::WingNonBoxInventory;

use super::WingReconciliationError;

/// Where the non-box part of the reconciled wing comes from.
///
/// Reference adaptation and the baseline sandbox freeze a measured empirical
/// wing and carry no item list. This verifies a reference remainder, not a
/// candidate's current inventory; product consumers additionally check the
/// candidate box against their current complete-wing mass. Clean-sheet runs
/// build the enumerated [`crate::wing_inventory`] list and check its status.
#[derive(Debug, Clone)]
pub enum StructuralInventory {
    /// Frozen empirical remainder of a registered reference aircraft.
    FrozenReference,
    /// The strength-sized complete-wing box exceeds the frozen empirical
    /// wing, so no non-box remainder exists. The empirical wing is published
    /// unchanged; the excess is a structural-model finding on this aircraft.
    ReferenceExceededBySizedBox {
        /// Frozen empirical complete-wing mass, kg.
        reference_total_kg: f64,
        /// Strength-sized complete-wing box mass, kg.
        sized_box_kg: f64,
    },
    /// The candidate primary box leaves no positive secondary remainder in
    /// the authoritative current complete-wing buildup.
    CurrentWingIncomplete {
        /// Current complete-wing mass, kg, independent of the frozen reference.
        complete_wing_kg: f64,
        /// Candidate complete-wing primary box mass, kg.
        sized_box_kg: f64,
    },
    /// Enumerated, sourced clean-sheet non-box inventory.
    CleanSheet(Box<WingNonBoxInventory>),
}

impl StructuralInventory {
    /// Whether the wing inventory may be presented as complete.
    pub fn is_complete(&self) -> bool {
        match self {
            Self::FrozenReference => true,
            Self::ReferenceExceededBySizedBox { .. } | Self::CurrentWingIncomplete { .. } => false,
            Self::CleanSheet(inventory) => inventory.status().is_complete(),
        }
    }

    /// Check the candidate against the actual mass it is placed within.
    /// A frozen reference remainder proves no inequality for a new candidate.
    pub fn checked_against_current_wing(self, sized_box_kg: f64, complete_wing_kg: f64) -> Self {
        if primary_fits_complete_wing(sized_box_kg, complete_wing_kg) {
            self
        } else {
            Self::CurrentWingIncomplete {
                complete_wing_kg,
                sized_box_kg,
            }
        }
    }
}

/// The design gross mass the structural diagnostics size against: the
/// declared FLOPS `DG` override when the configuration carries one (a
/// weight-variant declaration, or the fixed-aircraft basis
/// `AlasConfig::at_closure_mass` writes), otherwise the takeoff-mass
/// requirement of the case being evaluated: the same rule the FLOPS
/// airframe adapter applies, so the sized box and the FLOPS wing never
/// answer to two different design weights.
pub fn design_gross_mass_kg(config: &AlasConfig) -> f64 {
    config
        .mass_model
        .flops_structure
        .design_gross_mass_kg
        .unwrap_or(config.requirements.mtow_kg)
}

/// Governing structural mass, including the requirement in bounded modes.
/// A lighter declared dispatch basis cannot unload a heavier design wing.
pub fn structural_design_mass_kg(config: &AlasConfig) -> f64 {
    let declared = design_gross_mass_kg(config);
    if !declared.is_finite() || !config.requirements.mtow_kg.is_finite() {
        return f64::NAN;
    }
    if config.optimizer.objective.mtow_sizing == alas_config::MtowSizing::Unconstrained {
        declared
    } else {
        declared.max(config.requirements.mtow_kg)
    }
}

/// A complete wing must contain its positive primary box and a positive
/// secondary remainder (devices, controls and non-box structure).
pub fn primary_fits_complete_wing(sized_box_kg: f64, complete_wing_kg: f64) -> bool {
    sized_box_kg.is_finite()
        && complete_wing_kg.is_finite()
        && sized_box_kg > 0.0
        && complete_wing_kg > sized_box_kg
}

/// The requirements with `mtow_kg` replaced by [`design_gross_mass_kg`], for
/// the strength-sizing loads that read the takeoff mass as the design weight.
pub(super) fn design_requirements(config: &AlasConfig) -> DesignRequirements {
    let mut requirements = config.requirements.clone();
    requirements.mtow_kg = structural_design_mass_kg(config);
    requirements
}

/// The mass of a sized box over the complete wing, whatever extent it was
/// sized on.
fn complete_box_mass_kg(sized_box: &SizedWingboxMass) -> f64 {
    match sized_box.extent {
        WingExtent::FullWing => sized_box.mass_kg,
        WingExtent::SymmetricSemiWing => 2.0 * sized_box.mass_kg,
    }
}

type ReconciledReference = (
    WingboxFeedback,
    Option<ReferenceWingMass>,
    StructuralInventory,
    Option<CompositeProxyDeclaration>,
);

/// Reconcile a candidate box against the frozen empirical wing of a
/// registered aircraft.
///
/// When the strength-sized box alone outweighs the empirical wing there is no
/// non-box remainder to freeze. That is a finding about the structural model
/// on this aircraft (its loads, materials and gauges against the FLOPS
/// correlation) and not a reason to abandon the mass evaluation: the
/// empirical wing is published unchanged, the box is reported beside it, and
/// [`StructuralInventory::ReferenceExceededBySizedBox`] marks the inventory
/// incomplete so the feasibility residual carries the finding. Every other
/// reconciliation failure is still a structural-sizing error.
pub(super) fn reconcile_against_reference(
    reference: ReferenceWingMass,
    candidate_primary: SizedWingboxMass,
    primary_declaration: Option<CompositeProxyDeclaration>,
) -> Result<ReconciledReference, WingReconciliationError> {
    match reconcile_reference_wing(reference, candidate_primary) {
        Ok(feedback) => Ok((
            feedback,
            Some(reference),
            StructuralInventory::FrozenReference,
            primary_declaration,
        )),
        Err(WingboxFeedbackError::ReferenceHasNoPositiveSecondary { .. }) => {
            let reference_total_kg = reference.total_mass_kg;
            let sized_box_kg = complete_box_mass_kg(&candidate_primary);
            let first_moment_kg_m = [
                reference.centroid_m[0] * reference_total_kg,
                reference.centroid_m[1] * reference_total_kg,
                reference.centroid_m[2] * reference_total_kg,
            ];
            let feedback = WingboxFeedback {
                primary_mass_kg: sized_box_kg,
                secondary_mass_kg: 0.0,
                secondary_centroid_m: reference.centroid_m,
                secondary_first_moment_kg_m: [0.0; 3],
                total_wing_mass_kg: reference_total_kg,
                centroid_m: reference.centroid_m,
                first_moment_kg_m,
                // Negative by construction: the box exceeds the wing it is
                // supposed to be part of, by this much.
                closure_residual_kg: reference_total_kg - sized_box_kg,
                correction: None,
                mode: WingboxFeedbackMode::ReferenceAdaptation,
            };
            Ok((
                feedback,
                Some(reference),
                StructuralInventory::ReferenceExceededBySizedBox {
                    reference_total_kg,
                    sized_box_kg,
                },
                primary_declaration,
            ))
        }
        Err(_) => Err(WingReconciliationError::StructuralSizing),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_heavier_than_the_empirical_wing_is_reported_not_fatal() {
        // A semi-wing box of 6,000 kg is a 12,000 kg complete box against a
        // 8,000 kg empirical wing: the A320-class case that must be reported
        // by name rather than abort the fixed-aircraft path with an opaque
        // structural-sizing error.
        let sized_box = SizedWingboxMass::symmetric_semiwing(6_000.0, [19.0, 6.0, -1.0]);
        let reference = ReferenceWingMass {
            total_mass_kg: 8_000.0,
            centroid_m: [18.8, 0.0, -1.2],
            sized_box,
        };
        let (feedback, kept, inventory, _) =
            reconcile_against_reference(reference, sized_box, None)
                .unwrap_or_else(|error| panic!("{error}"));
        assert!(kept.is_some());
        assert!(!inventory.is_complete());
        let StructuralInventory::ReferenceExceededBySizedBox {
            reference_total_kg,
            sized_box_kg,
        } = inventory
        else {
            panic!("the exceeded reference must be reported by name");
        };
        assert_eq!(reference_total_kg, 8_000.0);
        assert_eq!(sized_box_kg, 12_000.0);
        // The published wing is the empirical wing, unchanged, at its own
        // centroid; the box is carried beside it as the finding.
        assert_eq!(feedback.total_wing_mass_kg, 8_000.0);
        assert_eq!(feedback.centroid_m, [18.8, 0.0, -1.2]);
        assert_eq!(feedback.primary_mass_kg, 12_000.0);
        assert_eq!(feedback.secondary_mass_kg, 0.0);
        assert_eq!(feedback.closure_residual_kg, -4_000.0);
    }

    #[test]
    fn a_box_lighter_than_the_empirical_wing_reconciles_as_before() {
        let sized_box = SizedWingboxMass::symmetric_semiwing(1_500.0, [19.0, 6.0, -1.0]);
        let reference = ReferenceWingMass {
            total_mass_kg: 8_000.0,
            centroid_m: [18.8, 0.0, -1.2],
            sized_box,
        };
        let (feedback, _, inventory, _) = reconcile_against_reference(reference, sized_box, None)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(inventory.is_complete());
        assert!((feedback.total_wing_mass_kg - 8_000.0).abs() < 1.0e-9);
        assert!((feedback.secondary_mass_kg - 5_000.0).abs() < 1.0e-9);
    }

    #[test]
    fn frozen_reference_does_not_verify_an_overweight_candidate_inventory() {
        let reference = ReferenceWingMass {
            total_mass_kg: 10_000.0,
            centroid_m: [20.0, 0.0, 0.0],
            sized_box: SizedWingboxMass::full_wing(7_000.0, [19.0, 0.0, 0.0]),
        };
        let candidate = SizedWingboxMass::full_wing(12_000.0, [21.0, 0.0, 0.0]);
        let (feedback, _, inventory, _) = reconcile_against_reference(reference, candidate, None)
            .unwrap_or_else(|error| panic!("{error}"));
        // Additive frozen-secondary reconciliation is a diagnostic, not the
        // current FLOPS total. Its 3 t remainder plus the new box is 15 t.
        assert_eq!(feedback.total_wing_mass_kg, 15_000.0);
        let inventory = inventory.checked_against_current_wing(feedback.primary_mass_kg, 10_000.0);
        assert!(!inventory.is_complete());
        assert!(matches!(
            inventory,
            StructuralInventory::CurrentWingIncomplete {
                complete_wing_kg: 10_000.0,
                sized_box_kg: 12_000.0,
            }
        ));
    }

    #[test]
    fn current_inventory_requires_a_positive_secondary_remainder() {
        for total in [7_000.0, 12_000.0, f64::NAN, f64::INFINITY] {
            assert!(!StructuralInventory::FrozenReference
                .checked_against_current_wing(12_000.0, total)
                .is_complete());
        }
        assert!(StructuralInventory::FrozenReference
            .checked_against_current_wing(12_000.0, 15_000.0)
            .is_complete());
    }
}
