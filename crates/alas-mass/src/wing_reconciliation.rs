// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared primary-box and non-box structural inventory diagnostics.
//!
//! The strength-sized primary box is reconciled with the non-box remainder:
//! the enumerated clean-sheet inventory in
//! [`crate::wing_inventory`], or the frozen empirical remainder of a
//! registered reference aircraft in reference-adaptation and baseline-sandbox
//! modes. That reconciliation also produces the wing's first moment, so the
//! diagnostic wing mass and centroid come from one calculation.
//!
//! This lives here, below both `alas-opt` and `alas-pipeline`, for the same
//! reason [`crate::product_stations`] does: search and delivery must assess
//! the same sized structure. The authoritative FLOPS complete-wing mass
//! remains unchanged. Current inventory completeness also requires that the
//! candidate box leaves a positive remainder within that mass, independently
//! of the diagnostic reference reconciliation.

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

use crate::wingbox_feedback::{ReferenceWingMass, WingboxFeedback};

mod fuel_relief;
pub use fuel_relief::{
    declared_integral_wing_fuel_kg_m, declared_wing_fuel_case, DeclaredWingFuelCase,
};
mod support;
pub use support::{
    design_gross_mass_kg, primary_fits_complete_wing, structural_design_mass_kg,
    StructuralInventory,
};

/// Why a candidate's wing could not be reconciled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WingReconciliationError {
    /// The main wing could not be strength-sized, or the sized box was not a
    /// physically admissible structure.
    #[error("the main wing could not be strength-sized on this geometry")]
    StructuralSizing,
    /// The frozen reference aircraft's own mass buildup did not resolve.
    #[error("the reference aircraft's wing mass and coordinates did not resolve")]
    MassCoordinates,
}

/// The reconciled structural diagnostic beside the authoritative wing buildup.
#[derive(Debug, Clone)]
pub struct WingReconciliation {
    /// Total wing mass and first moment of the reconciled group.
    pub feedback: WingboxFeedback,
    /// The frozen empirical reference, when this design space uses one, so a
    /// caller iterating passes does not rebuild it per pass.
    pub reference: Option<ReferenceWingMass>,
    /// Provenance of the non-box inventory behind `inventory_complete`.
    pub inventory: StructuralInventory,
    /// Primary structural sizing declaration/provenance carried when composite
    /// materials are sized under an effective isotropic proxy.
    pub primary_declaration: Option<alas_struct::sizing::CompositeProxyDeclaration>,
}

impl WingReconciliation {
    /// Whether the wing total represents a complete primary plus secondary
    /// inventory.
    pub fn inventory_complete(&self) -> bool {
        self.inventory.is_complete()
    }

    /// Sizing declaration of the primary structural wingbox, if composite.
    pub fn primary_declaration(&self) -> Option<&alas_struct::sizing::CompositeProxyDeclaration> {
        self.primary_declaration.as_ref()
    }
}

/// Reconcile the strength-sized primary box with the non-box remainder for
/// one built candidate.
///
/// `reference` lets a caller that already resolved the frozen empirical
/// reference for this configuration pass it back in rather than rebuilding it.
///
/// # Errors
///
/// [`WingReconciliationError`] when the wing cannot be strength-sized or the
/// reference aircraft's own buildup does not resolve.
pub fn reconcile(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    reference: Option<ReferenceWingMass>,
) -> Result<WingReconciliation, WingReconciliationError> {
    let model = config.analysis_mass_model(config.requirements.mtow_kg);
    let (masses, _, _) = crate::breakdown::run_mass_analysis_with_model_checked_product_with_gear(
        plane,
        &config.requirements,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&model),
        None,
        crate::breakdown::MassCoordinateModel::ReferenceCompatibility,
        &config.landing_gear,
    )
    .map_err(|_| WingReconciliationError::MassCoordinates)?;
    reconcile_with_wing_mass(config, design, plane, reference, masses.wing)
}

/// Reconcile with the authoritative current complete-wing mass already built
/// by a product consumer, avoiding a second empirical mass evaluation.
///
/// # Errors
///
/// The same sizing/reference errors as [`reconcile`].
pub fn reconcile_with_wing_mass(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    reference: Option<ReferenceWingMass>,
    complete_wing_kg: f64,
) -> Result<WingReconciliation, WingReconciliationError> {
    let design_box = size_design_wing_box(config, design, plane)?;
    reconcile_with_design_box(config, plane, reference, complete_wing_kg, design_box)
}

/// [`reconcile_with_wing_mass`] for a box the caller already sized with
/// [`size_design_wing_box`] on the same configuration, design and aircraft.
///
/// # Errors
///
/// The reference errors of [`reconcile`]; the sizing has already succeeded.
pub fn reconcile_with_design_box(
    config: &AlasConfig,
    plane: &Airplane,
    reference: Option<ReferenceWingMass>,
    complete_wing_kg: f64,
    design_box: DesignWingBox,
) -> Result<WingReconciliation, WingReconciliationError> {
    let (feedback, reference, inventory, primary_declaration) =
        reconcile_structural_wing(config, plane, reference, design_box)?;
    let inventory =
        inventory.checked_against_current_wing(feedback.primary_mass_kg, complete_wing_kg);
    Ok(WingReconciliation {
        feedback,
        reference,
        inventory,
        primary_declaration,
    })
}

mod geometry;
mod reconcile;
use reconcile::reconcile_structural_wing;
pub use reconcile::{
    clean_sheet_secondary, main_wing, size_design_wing_box, sized_primary_wing, DesignWingBox,
};
