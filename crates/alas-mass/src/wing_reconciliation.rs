// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one wing-mass basis both product consumers build on.
//!
//! The complete wing group is not the empirical Torenbeek total on its own.
//! It is the strength-sized primary box reconciled with the non-box
//! remainder: the enumerated clean-sheet inventory in
//! [`crate::wing_inventory`], or the frozen empirical remainder of a
//! registered reference aircraft in reference-adaptation and baseline-sandbox
//! modes. That reconciliation also produces the wing's first moment, so the
//! wing mass and the wing centroid come from one calculation rather than two.
//!
//! This lives here, below both `alas-opt` and `alas-pipeline`, for the same
//! reason [`crate::product_stations`] does. While the reconciliation was
//! private to the optimizer, the search sized every candidate against the
//! reconciled wing while the final report published the empirical total for
//! the same aircraft -- about 1.2 t apart on the r5 nominal finalist, with the
//! fuel closure silently absorbing the difference. Two numbers for one wing is
//! not a reporting detail: it is the operating empty mass the run is
//! ultimately judged on.
//!
//! Nothing here is new physics. The bodies are the optimizer's own, moved
//! down a layer so both callers reach the same one.

use alas_config::design_variables::DesignVector;
use alas_config::optimizer::DesignMode;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;
use alas_geom::builder::AircraftBuilder;
use alas_struct::sizing::{size_wingbox, WingboxSizing};

use crate::breakdown::{
    run_mass_analysis_with_model_checked_product_with_gear, MassCoordinateModel,
};
use crate::flops_transport::structure::{FlopsWingInputs, WingBendingFactor};
use crate::torenbeek::{
    mass_wing_with_control_surface_area, wing_secondary_mass_breakdown_with_control_surface_area,
    WingSecondaryMassBreakdown,
};
use crate::wing_inventory::{
    build_wing_inventory, FixedNonBoxStructure, MovableSurface, TorenbeekWingGroup,
    WingInventoryInputs, WingMovableSurfaces, WingNonBoxInventory,
};
use crate::wingbox_feedback::{
    reconcile_clean_sheet_wing, reconcile_reference_wing, ReferenceWingMass, SizedWingboxMass,
    WingboxFeedback,
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

/// Where the non-box part of the reconciled wing comes from.
///
/// Reference adaptation and the baseline sandbox freeze a measured empirical
/// wing, so their non-box inventory is complete by construction and carries no
/// item list. Clean-sheet runs build the enumerated [`crate::wing_inventory`]
/// list and are complete only when that list is.
#[derive(Debug, Clone)]
pub enum StructuralInventory {
    /// Frozen empirical remainder of a registered reference aircraft.
    FrozenReference,
    /// Enumerated, sourced clean-sheet non-box inventory.
    CleanSheet(Box<WingNonBoxInventory>),
}

impl StructuralInventory {
    /// Whether the wing inventory may be presented as complete.
    pub fn is_complete(&self) -> bool {
        match self {
            Self::FrozenReference => true,
            Self::CleanSheet(inventory) => inventory.status().is_complete(),
        }
    }
}

/// The reconciled wing every product consumer should publish.
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
    let (feedback, reference, inventory, primary_declaration) =
        reconcile_structural_wing(config, design, plane, reference)?;
    Ok(WingReconciliation {
        feedback,
        reference,
        inventory,
        primary_declaration,
    })
}

include!("wing_reconciliation/geometry.rs");
include!("wing_reconciliation/reconcile.rs");
