// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reconcile an analytical wingbox with a total-wing mass model.
//!
//! The structural sizing output contains the mass of the modeled box only,
//! while [`crate::breakdown::MassBreakdown::wing`] is a total empirical wing
//! mass. Adding both would count the primary wing structure twice. This
//! module keeps the reconciliation explicit and pure: the caller supplies a
//! sized-box mass and the reference or declared secondary inventory, then
//! applies the returned total and first moment to its normal OEW/CG closure.
//!
//! All masses are kilograms, coordinates are metres in the aircraft geometry
//! frame, and first moments are kilogram-metres. A `SymmetricSemiWing` input
//! is mirrored through the aircraft `y = 0` plane before reconciliation.

/// Whether an input mass describes the complete wing or one symmetric
/// semispan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WingExtent {
    /// Mass and centroid already describe both left and right wing halves.
    FullWing,
    /// Mass and centroid describe one semispan and are mirrored at `y = 0`.
    SymmetricSemiWing,
}

/// Mass and centroid of the analytical primary wingbox.
///
/// `alas-struct::sizing::WingboxSizing` reports a semispan box mass. The
/// caller must mark that value as [`WingExtent::SymmetricSemiWing`] or convert
/// it to a complete-wing value before constructing this type. This type does
/// not call the structural solver, which avoids a dependency cycle and keeps
/// structural sizing provenance at the caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizedWingboxMass {
    /// Modeled structural box mass, kg, over the declared extent.
    pub mass_kg: f64,
    /// Structural box centroid `[x, y, z]`, m, over the declared extent.
    pub centroid_m: [f64; 3],
    /// Spatial extent represented by `mass_kg` and `centroid_m`.
    pub extent: WingExtent,
}

impl SizedWingboxMass {
    /// Construct a complete-wing primary mass input.
    pub const fn full_wing(mass_kg: f64, centroid_m: [f64; 3]) -> Self {
        Self {
            mass_kg,
            centroid_m,
            extent: WingExtent::FullWing,
        }
    }

    /// Construct a one-semispan input for a symmetric wing.
    pub const fn symmetric_semiwing(mass_kg: f64, centroid_m: [f64; 3]) -> Self {
        Self {
            mass_kg,
            centroid_m,
            extent: WingExtent::SymmetricSemiWing,
        }
    }
}

/// Frozen reference total-wing mass used to preserve empirical secondary
/// items during a reference adaptation.
///
/// `total_mass_kg` and `centroid_m` must describe the complete empirical wing
/// and use the same aircraft coordinate frame as the sized-box input. The
/// reference box can be supplied as a symmetric semispan; it is normalized to
/// a complete wing before the residual inventory is derived.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReferenceWingMass {
    /// Existing empirical total wing mass, kg, including box and secondary
    /// items.
    pub total_mass_kg: f64,
    /// Existing empirical total-wing centroid `[x, y, z]`, m.
    pub centroid_m: [f64; 3],
    /// The primary box mass represented by the frozen reference total, with
    /// explicit full-wing or symmetric-semispan extent.
    pub sized_box: SizedWingboxMass,
}

/// Declared non-box wing inventory for a clean-sheet reconciliation.
///
/// This input is intentionally required. A clean-sheet run cannot infer
/// secondary wing items by subtracting an unrelated empirical total without a
/// declared calibration or inventory basis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SecondaryWingMass {
    /// Secondary/non-box wing mass, kg, over the declared extent.
    pub mass_kg: f64,
    /// Secondary/non-box centroid `[x, y, z]`, m.
    pub centroid_m: [f64; 3],
    /// Spatial extent represented by `mass_kg` and `centroid_m`.
    pub extent: WingExtent,
}

impl SecondaryWingMass {
    /// Construct a declared complete-wing secondary inventory.
    pub const fn full_wing(mass_kg: f64, centroid_m: [f64; 3]) -> Self {
        Self {
            mass_kg,
            centroid_m,
            extent: WingExtent::FullWing,
        }
    }

    /// Construct a declared symmetric-semispan secondary inventory.
    pub const fn symmetric_semiwing(mass_kg: f64, centroid_m: [f64; 3]) -> Self {
        Self {
            mass_kg,
            centroid_m,
            extent: WingExtent::SymmetricSemiWing,
        }
    }
}

/// Whether the result uses a frozen empirical reference or an explicit
/// clean-sheet secondary inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WingboxFeedbackMode {
    /// Reference total and secondary first moment are frozen from a baseline.
    ReferenceAdaptation,
    /// Both primary and secondary inventories were declared directly.
    CleanSheet,
}

/// Mass and first-moment change relative to a frozen reference wing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingMassCorrection {
    /// Change in total wing mass, kg.
    pub mass_delta_kg: f64,
    /// Change in total wing first moment `[Mx, My, Mz]`, kg m.
    pub moment_delta_kg_m: [f64; 3],
}

/// Reconciled complete-wing mass and first moment.
///
/// The caller should replace only the total wing component and its centroid
/// with this result, then recompute the normal OEW, fuel, dispatch and global
/// CG closure. Other component masses are not part of this result and are
/// therefore preserved by construction. `closure_residual_kg` should be near
/// machine precision; it is returned so the caller can enforce its own
/// closure tolerance rather than silently accepting a broken decomposition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingboxFeedback {
    /// Primary analytical box mass in the complete wing, kg.
    pub primary_mass_kg: f64,
    /// Secondary/non-box wing mass in the complete wing, kg.
    pub secondary_mass_kg: f64,
    /// Secondary/non-box centroid `[x, y, z]`, m.
    pub secondary_centroid_m: [f64; 3],
    /// Secondary/non-box first moment `[Mx, My, Mz]`, kg m.
    pub secondary_first_moment_kg_m: [f64; 3],
    /// Reconciled complete-wing mass, kg.
    pub total_wing_mass_kg: f64,
    /// Reconciled complete-wing centroid `[x, y, z]`, m.
    pub centroid_m: [f64; 3],
    /// Reconciled complete-wing first moment `[Mx, My, Mz]`, kg m.
    pub first_moment_kg_m: [f64; 3],
    /// Difference between total mass and primary-plus-secondary mass, kg.
    pub closure_residual_kg: f64,
    /// Reference correction, present only for reference adaptation.
    pub correction: Option<WingMassCorrection>,
    /// Provenance of the reconciliation.
    pub mode: WingboxFeedbackMode,
}

/// Failure returned when a mass reconciliation cannot be defended from its
/// declared inputs.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum WingboxFeedbackError {
    /// A named scalar or coordinate was not finite.
    #[error("{field} must be finite")]
    NonFinite {
        /// Input name used to identify the invalid value.
        field: &'static str,
    },
    /// A declared physical mass was not strictly positive.
    #[error("{field} must be positive")]
    NonPositive {
        /// Input name used to identify the invalid value.
        field: &'static str,
    },
    /// A complete-wing conversion overflowed or became invalid.
    #[error("{field} is invalid after extent conversion")]
    InvalidConvertedMass {
        /// Input name used to identify the invalid value.
        field: &'static str,
    },
    /// The empirical reference leaves no positive secondary inventory.
    #[error(
        "reference total wing mass ({total_mass_kg} kg) must exceed reference sized-box mass ({primary_mass_kg} kg)"
    )]
    ReferenceHasNoPositiveSecondary {
        /// Frozen complete-wing empirical total, kg.
        total_mass_kg: f64,
        /// Frozen complete-wing primary box mass, kg.
        primary_mass_kg: f64,
    },
    /// The reference total and primary moments imply a nonfinite secondary
    /// first moment or centroid.
    #[error("reference residual secondary first moment is not finite")]
    InvalidReferenceResidual,
    /// Finite input inventories produced a nonfinite reconciled result.
    #[error("reconciled wing mass or first moment is not finite")]
    InvalidReconciledResult,
}

#[derive(Debug, Clone, Copy)]
struct CompleteWingMass {
    mass_kg: f64,
    moment_kg_m: [f64; 3],
}

/// Reconcile a candidate analytical box against a frozen empirical reference.
///
/// The reference secondary inventory is derived exactly once as
/// `reference total - reference sized box`. Its first moment is derived from
/// the corresponding total and primary moments, then held fixed while the
/// candidate box changes. Therefore the candidate total is the frozen
/// empirical total plus only the candidate-minus-reference box delta; no
/// unrelated wing regression is added a second time.
pub fn reconcile_reference_wing(
    reference: ReferenceWingMass,
    candidate: SizedWingboxMass,
) -> Result<WingboxFeedback, WingboxFeedbackError> {
    let reference_total = complete_total(
        reference.total_mass_kg,
        reference.centroid_m,
        "reference total wing mass",
        "reference total wing centroid",
    )?;
    let reference_primary = complete_primary(reference.sized_box, "reference sized-box")?;
    let candidate_primary = complete_primary(candidate, "candidate sized-box")?;

    if reference_total.mass_kg <= reference_primary.mass_kg {
        return Err(WingboxFeedbackError::ReferenceHasNoPositiveSecondary {
            total_mass_kg: reference_total.mass_kg,
            primary_mass_kg: reference_primary.mass_kg,
        });
    }
    let secondary_mass_kg = reference_total.mass_kg - reference_primary.mass_kg;
    let secondary_moment_kg_m =
        subtract_moment(reference_total.moment_kg_m, reference_primary.moment_kg_m);
    if !secondary_moment_kg_m.iter().all(|value| value.is_finite()) {
        return Err(WingboxFeedbackError::InvalidReferenceResidual);
    }
    let secondary_centroid_m = centroid_from_moment(secondary_mass_kg, secondary_moment_kg_m)
        .map_err(|_| WingboxFeedbackError::InvalidReferenceResidual)?;

    let total_wing_mass_kg = candidate_primary.mass_kg + secondary_mass_kg;
    let first_moment_kg_m = add_moment(candidate_primary.moment_kg_m, secondary_moment_kg_m);
    let centroid_m = centroid_from_moment(total_wing_mass_kg, first_moment_kg_m)?;
    let correction = WingMassCorrection {
        mass_delta_kg: total_wing_mass_kg - reference_total.mass_kg,
        moment_delta_kg_m: subtract_moment(first_moment_kg_m, reference_total.moment_kg_m),
    };
    Ok(WingboxFeedback {
        primary_mass_kg: candidate_primary.mass_kg,
        secondary_mass_kg,
        secondary_centroid_m,
        secondary_first_moment_kg_m: secondary_moment_kg_m,
        total_wing_mass_kg,
        centroid_m,
        first_moment_kg_m,
        closure_residual_kg: total_wing_mass_kg - (candidate_primary.mass_kg + secondary_mass_kg),
        correction: Some(correction),
        mode: WingboxFeedbackMode::ReferenceAdaptation,
    })
}

/// Reconcile a clean-sheet box with an explicitly declared secondary wing
/// inventory.
///
/// No empirical total is inferred in this mode. If the caller cannot provide
/// a defensible secondary mass and centroid, it must keep the wingbox result
/// diagnostic or return its own unsupported-data status instead of calling
/// this function with a fabricated fraction.
pub fn reconcile_clean_sheet_wing(
    primary: SizedWingboxMass,
    secondary: SecondaryWingMass,
) -> Result<WingboxFeedback, WingboxFeedbackError> {
    let primary = complete_primary(primary, "clean-sheet sized-box")?;
    let secondary = complete_secondary(secondary)?;
    let total_wing_mass_kg = primary.mass_kg + secondary.mass_kg;
    let first_moment_kg_m = add_moment(primary.moment_kg_m, secondary.moment_kg_m);
    let centroid_m = centroid_from_moment(total_wing_mass_kg, first_moment_kg_m)?;
    let secondary_centroid_m = centroid_from_moment(secondary.mass_kg, secondary.moment_kg_m)?;
    Ok(WingboxFeedback {
        primary_mass_kg: primary.mass_kg,
        secondary_mass_kg: secondary.mass_kg,
        secondary_centroid_m,
        secondary_first_moment_kg_m: secondary.moment_kg_m,
        total_wing_mass_kg,
        centroid_m,
        first_moment_kg_m,
        closure_residual_kg: total_wing_mass_kg - (primary.mass_kg + secondary.mass_kg),
        correction: None,
        mode: WingboxFeedbackMode::CleanSheet,
    })
}

fn complete_primary(
    input: SizedWingboxMass,
    field: &'static str,
) -> Result<CompleteWingMass, WingboxFeedbackError> {
    complete_mass(input.mass_kg, input.centroid_m, input.extent, field)
}

fn complete_secondary(input: SecondaryWingMass) -> Result<CompleteWingMass, WingboxFeedbackError> {
    complete_mass(
        input.mass_kg,
        input.centroid_m,
        input.extent,
        "clean-sheet secondary wing mass",
    )
}

fn complete_total(
    mass_kg: f64,
    centroid_m: [f64; 3],
    mass_field: &'static str,
    centroid_field: &'static str,
) -> Result<CompleteWingMass, WingboxFeedbackError> {
    if !mass_kg.is_finite() {
        return Err(WingboxFeedbackError::NonFinite { field: mass_field });
    }
    if mass_kg <= 0.0 {
        return Err(WingboxFeedbackError::NonPositive { field: mass_field });
    }
    if !centroid_m.iter().all(|value| value.is_finite()) {
        return Err(WingboxFeedbackError::NonFinite {
            field: centroid_field,
        });
    }
    let moment_kg_m = scale_moment(centroid_m, mass_kg);
    if !moment_kg_m.iter().all(|value| value.is_finite()) {
        return Err(WingboxFeedbackError::InvalidReconciledResult);
    }
    Ok(CompleteWingMass {
        mass_kg,
        moment_kg_m,
    })
}

fn complete_mass(
    mass_kg: f64,
    centroid_m: [f64; 3],
    extent: WingExtent,
    field: &'static str,
) -> Result<CompleteWingMass, WingboxFeedbackError> {
    if !mass_kg.is_finite() {
        return Err(WingboxFeedbackError::NonFinite { field });
    }
    if mass_kg <= 0.0 {
        return Err(WingboxFeedbackError::NonPositive { field });
    }
    if !centroid_m.iter().all(|value| value.is_finite()) {
        return Err(WingboxFeedbackError::NonFinite { field });
    }
    let (complete_mass_kg, complete_centroid_m) = match extent {
        WingExtent::FullWing => (mass_kg, centroid_m),
        WingExtent::SymmetricSemiWing => {
            let complete_mass_kg = mass_kg * 2.0;
            if !complete_mass_kg.is_finite() || complete_mass_kg <= 0.0 {
                return Err(WingboxFeedbackError::InvalidConvertedMass { field });
            }
            (complete_mass_kg, [centroid_m[0], 0.0, centroid_m[2]])
        }
    };
    let moment_kg_m = scale_moment(complete_centroid_m, complete_mass_kg);
    if !moment_kg_m.iter().all(|value| value.is_finite()) {
        return Err(WingboxFeedbackError::InvalidConvertedMass { field });
    }
    Ok(CompleteWingMass {
        mass_kg: complete_mass_kg,
        moment_kg_m,
    })
}

fn scale_moment(centroid_m: [f64; 3], mass_kg: f64) -> [f64; 3] {
    [
        mass_kg * centroid_m[0],
        mass_kg * centroid_m[1],
        mass_kg * centroid_m[2],
    ]
}

fn add_moment(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn subtract_moment(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn centroid_from_moment(
    mass_kg: f64,
    moment_kg_m: [f64; 3],
) -> Result<[f64; 3], WingboxFeedbackError> {
    if !mass_kg.is_finite() || mass_kg <= 0.0 {
        return Err(WingboxFeedbackError::InvalidConvertedMass {
            field: "reconciled total wing mass",
        });
    }
    let centroid_m = [
        moment_kg_m[0] / mass_kg,
        moment_kg_m[1] / mass_kg,
        moment_kg_m[2] / mass_kg,
    ];
    if centroid_m.iter().all(|value| value.is_finite()) {
        Ok(centroid_m)
    } else {
        Err(WingboxFeedbackError::InvalidReconciledResult)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_reference_candidate_preserves_total_and_first_moment() {
        let reference = ReferenceWingMass {
            total_mass_kg: 10_000.0,
            centroid_m: [12.0, 0.0, 1.5],
            sized_box: SizedWingboxMass::full_wing(7_000.0, [10.0, 0.0, 1.0]),
        };
        let feedback = reconcile_reference_wing(reference, reference.sized_box).unwrap();

        assert_eq!(feedback.mode, WingboxFeedbackMode::ReferenceAdaptation);
        assert_eq!(feedback.total_wing_mass_kg, reference.total_mass_kg);
        assert_eq!(feedback.centroid_m, reference.centroid_m);
        assert_eq!(feedback.first_moment_kg_m, [120_000.0, 0.0, 15_000.0]);
        assert_eq!(feedback.primary_mass_kg, 7_000.0);
        assert_eq!(feedback.secondary_mass_kg, 3_000.0);
        assert_eq!(
            feedback.secondary_first_moment_kg_m,
            [50_000.0, 0.0, 8_000.0]
        );
        assert_eq!(
            feedback.secondary_centroid_m,
            [50_000.0 / 3_000.0, 0.0, 8_000.0 / 3_000.0]
        );
        assert!(feedback.closure_residual_kg.abs() < 1.0e-12);
        let correction = feedback.correction.unwrap();
        assert!(correction.mass_delta_kg.abs() < 1.0e-12);
        assert!(correction
            .moment_delta_kg_m
            .iter()
            .all(|value| value.abs() < 1.0e-10));
    }

    #[test]
    fn reference_feedback_applies_only_the_sized_box_delta() {
        let reference = ReferenceWingMass {
            total_mass_kg: 10_000.0,
            centroid_m: [12.0, 0.0, 1.5],
            sized_box: SizedWingboxMass::full_wing(7_000.0, [10.0, 0.0, 1.0]),
        };
        let candidate = SizedWingboxMass::full_wing(8_000.0, [12.0, 0.0, 2.0]);
        let feedback = reconcile_reference_wing(reference, candidate).unwrap();

        // Frozen secondary inventory is 3,000 kg at x=16.666... m. The
        // candidate total is the reference total plus the 1,000 kg box delta,
        // rather than 10,000 + 8,000 kg.
        assert_eq!(feedback.total_wing_mass_kg, 11_000.0);
        assert_eq!(feedback.primary_mass_kg, 8_000.0);
        assert_eq!(feedback.secondary_mass_kg, 3_000.0);
        assert_eq!(feedback.first_moment_kg_m, [146_000.0, 0.0, 24_000.0]);
        assert_eq!(feedback.correction.unwrap().mass_delta_kg, 1_000.0);
        assert_eq!(
            feedback.correction.unwrap().moment_delta_kg_m,
            [26_000.0, 0.0, 9_000.0]
        );
        assert_eq!(
            feedback.centroid_m,
            [146_000.0 / 11_000.0, 0.0, 24_000.0 / 11_000.0]
        );
    }

    #[test]
    fn symmetric_semispan_inputs_are_mirrored_once() {
        let feedback = reconcile_clean_sheet_wing(
            SizedWingboxMass::symmetric_semiwing(4_000.0, [10.0, 3.0, 1.0]),
            SecondaryWingMass::full_wing(2_000.0, [15.0, 0.0, 2.0]),
        )
        .unwrap();

        assert_eq!(feedback.primary_mass_kg, 8_000.0);
        assert_eq!(feedback.secondary_mass_kg, 2_000.0);
        assert_eq!(feedback.total_wing_mass_kg, 10_000.0);
        assert_eq!(feedback.centroid_m, [11.0, 0.0, 1.2]);
    }

    #[test]
    fn clean_sheet_requires_and_adds_declared_secondary_inventory() {
        let feedback = reconcile_clean_sheet_wing(
            SizedWingboxMass::full_wing(7_000.0, [10.0, 0.0, 1.0]),
            SecondaryWingMass::full_wing(1_500.0, [20.0, 0.0, 3.0]),
        )
        .unwrap();

        assert_eq!(feedback.mode, WingboxFeedbackMode::CleanSheet);
        assert_eq!(feedback.total_wing_mass_kg, 8_500.0);
        assert_eq!(feedback.first_moment_kg_m, [100_000.0, 0.0, 11_500.0]);
        assert_eq!(
            feedback.secondary_first_moment_kg_m,
            [30_000.0, 0.0, 4_500.0]
        );
        assert_eq!(
            feedback.centroid_m,
            [100_000.0 / 8_500.0, 0.0, 11_500.0 / 8_500.0]
        );
        assert!(feedback.correction.is_none());
        assert!(feedback.closure_residual_kg.abs() < 1.0e-12);
    }

    /// Relative first-moment closure of a reconciled wing against the sum over
    /// its two declared parts, per axis.
    fn moment_closure(feedback: &WingboxFeedback, primary_moment_kg_m: [f64; 3]) -> f64 {
        (0..3)
            .map(|axis| {
                let parts = primary_moment_kg_m[axis] + feedback.secondary_first_moment_kg_m[axis];
                let scale = feedback.first_moment_kg_m[axis]
                    .abs()
                    .max(parts.abs())
                    .max(1.0);
                (feedback.first_moment_kg_m[axis] - parts).abs() / scale
            })
            .fold(0.0, f64::max)
    }

    #[test]
    fn a_symmetric_semiwing_is_mirrored_to_exactly_twice_its_mass_at_y_zero() {
        // One semispan at y = +4.5 m mirrors to a complete wing of exactly
        // twice the mass, on the aircraft centreline, with x and z unchanged.
        let primary = SizedWingboxMass::symmetric_semiwing(3_500.0, [18.5, 4.5, -0.75]);
        let feedback = reconcile_clean_sheet_wing(
            primary,
            SecondaryWingMass::full_wing(1.0, [18.5, 0.0, -0.75]),
        )
        .unwrap();
        assert_eq!(feedback.primary_mass_kg, 7_000.0);
        // The primary first moment is the mirrored mass times the mirrored
        // centroid: x and z survive, y cancels exactly.
        let primary_moment = [
            feedback.first_moment_kg_m[0] - feedback.secondary_first_moment_kg_m[0],
            feedback.first_moment_kg_m[1] - feedback.secondary_first_moment_kg_m[1],
            feedback.first_moment_kg_m[2] - feedback.secondary_first_moment_kg_m[2],
        ];
        assert_eq!(primary_moment, [7_000.0 * 18.5, 0.0, 7_000.0 * -0.75]);
        assert_eq!(feedback.centroid_m[1], 0.0);
        assert!((feedback.centroid_m[0] - 18.5).abs() < 1.0e-12);
        assert!((feedback.centroid_m[2] + 0.75).abs() < 1.0e-12);

        // Declaring the already-mirrored complete wing gives the same result,
        // so the factor of two is applied exactly once.
        let complete = reconcile_clean_sheet_wing(
            SizedWingboxMass::full_wing(7_000.0, [18.5, 0.0, -0.75]),
            SecondaryWingMass::full_wing(1.0, [18.5, 0.0, -0.75]),
        )
        .unwrap();
        assert_eq!(complete.total_wing_mass_kg, feedback.total_wing_mass_kg);
        assert_eq!(complete.first_moment_kg_m, feedback.first_moment_kg_m);
    }

    #[test]
    fn both_reconciliations_conserve_the_first_moment_of_their_parts() {
        // Reference adaptation: the candidate box moves in mass and position
        // while the frozen secondary inventory stays put.
        let reference = ReferenceWingMass {
            total_mass_kg: 9_400.0,
            centroid_m: [17.25, 0.0, -0.4],
            sized_box: SizedWingboxMass::symmetric_semiwing(3_100.0, [16.4, 4.2, -0.55]),
        };
        for (mass_kg, centroid) in [
            (3_100.0, [16.4, 4.2, -0.55]),
            (3_650.0, [17.9, 4.6, -0.30]),
            (2_450.0, [15.1, 3.9, -0.80]),
        ] {
            let candidate = SizedWingboxMass::symmetric_semiwing(mass_kg, centroid);
            let feedback = reconcile_reference_wing(reference, candidate).unwrap();
            let primary_moment = [
                2.0 * mass_kg * centroid[0],
                0.0,
                2.0 * mass_kg * centroid[2],
            ];
            assert!(
                moment_closure(&feedback, primary_moment) < 1.0e-9,
                "reference closure {}",
                moment_closure(&feedback, primary_moment)
            );
            assert!(feedback.closure_residual_kg.abs() < 1.0e-9);
            assert!(
                (feedback.primary_mass_kg + feedback.secondary_mass_kg
                    - feedback.total_wing_mass_kg)
                    .abs()
                    < 1.0e-9
            );
        }

        // Clean sheet: both parts are declared, so the reconciled moment is
        // their exact sum.
        let secondary = SecondaryWingMass::full_wing(2_050.0, [19.8, 0.0, 0.15]);
        for (mass_kg, centroid) in [(3_100.0, [16.4, 4.2, -0.55]), (4_400.0, [17.2, 5.0, 0.25])] {
            let feedback = reconcile_clean_sheet_wing(
                SizedWingboxMass::symmetric_semiwing(mass_kg, centroid),
                secondary,
            )
            .unwrap();
            let primary_moment = [
                2.0 * mass_kg * centroid[0],
                0.0,
                2.0 * mass_kg * centroid[2],
            ];
            assert!(moment_closure(&feedback, primary_moment) < 1.0e-9);
            assert_eq!(
                feedback.secondary_first_moment_kg_m,
                [2_050.0 * 19.8, 0.0, 2_050.0 * 0.15]
            );
        }
    }

    #[test]
    fn a_reference_box_that_only_moves_leaves_the_total_frozen_and_shifts_the_centroid() {
        let reference = ReferenceWingMass {
            total_mass_kg: 10_000.0,
            centroid_m: [12.0, 0.0, 1.5],
            sized_box: SizedWingboxMass::full_wing(7_000.0, [10.0, 0.0, 1.0]),
        };
        // Same box mass, moved 0.8 m aft and 0.2 m up.
        let candidate = SizedWingboxMass::full_wing(7_000.0, [10.8, 0.0, 1.2]);
        let feedback = reconcile_reference_wing(reference, candidate).unwrap();

        assert_eq!(feedback.total_wing_mass_kg, reference.total_mass_kg);
        let correction = feedback.correction.unwrap();
        assert!(correction.mass_delta_kg.abs() < 1.0e-12);
        // The whole wing moves by the box's share of the shift: 7,000/10,000.
        assert!((feedback.centroid_m[0] - (12.0 + 0.7 * 0.8)).abs() < 1.0e-12);
        assert!((feedback.centroid_m[2] - (1.5 + 0.7 * 0.2)).abs() < 1.0e-12);
        assert!((correction.moment_delta_kg_m[0] - 7_000.0 * 0.8).abs() < 1.0e-9);
        assert!((correction.moment_delta_kg_m[2] - 7_000.0 * 0.2).abs() < 1.0e-9);
        assert!(moment_closure(&feedback, [7_000.0 * 10.8, 0.0, 7_000.0 * 1.2]) < 1.0e-9);
    }

    #[test]
    fn invalid_and_double_counting_inputs_are_rejected() {
        let reference = ReferenceWingMass {
            total_mass_kg: 10_000.0,
            centroid_m: [12.0, 0.0, 1.5],
            sized_box: SizedWingboxMass::full_wing(7_000.0, [10.0, 0.0, 1.0]),
        };
        assert!(matches!(
            reconcile_reference_wing(
                reference,
                SizedWingboxMass::full_wing(f64::NAN, [10.0, 0.0, 1.0])
            ),
            Err(WingboxFeedbackError::NonFinite { .. })
        ));
        assert!(matches!(
            reconcile_reference_wing(
                ReferenceWingMass {
                    total_mass_kg: 7_000.0,
                    ..reference
                },
                reference.sized_box,
            ),
            Err(WingboxFeedbackError::ReferenceHasNoPositiveSecondary { .. })
        ));
        assert!(matches!(
            reconcile_clean_sheet_wing(
                reference.sized_box,
                SecondaryWingMass::full_wing(0.0, [20.0, 0.0, 2.0]),
            ),
            Err(WingboxFeedbackError::NonPositive { .. })
        ));
        assert!(matches!(
            reconcile_clean_sheet_wing(
                SizedWingboxMass::full_wing(7_000.0, [f64::INFINITY, 0.0, 1.0]),
                SecondaryWingMass::full_wing(1_000.0, [20.0, 0.0, 2.0]),
            ),
            Err(WingboxFeedbackError::NonFinite { .. })
        ));
    }
}
