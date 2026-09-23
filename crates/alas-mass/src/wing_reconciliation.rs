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
//! reason [`crate::product_stations`] does: the optimizer's search and the
//! final report must read one wing mass. Sizing candidates against the
//! reconciled wing while reporting the empirical total puts two numbers on one
//! wing (about 1.2 t apart on a nominal finalist), and the fuel closure
//! silently absorbs the difference in the operating empty mass the run is
//! judged on.

use alas_config::design_variables::DesignVector;
use alas_config::optimizer::DesignMode;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;
use alas_geom::builder::AircraftBuilder;
use alas_struct::sizing::WingboxSizing;

use crate::breakdown::{
    run_mass_analysis_with_model_checked_product_with_gear, MassCoordinateModel,
};
use crate::torenbeek::{
    mass_wing_with_control_surface_area, wing_secondary_mass_breakdown_with_control_surface_area,
};
use crate::wing_inventory::{
    build_wing_inventory, MovableSurface, TorenbeekWingGroup, WingInventoryInputs,
    WingMovableSurfaces, WingNonBoxInventory,
};
use crate::wingbox_feedback::{
    reconcile_clean_sheet_wing, ReferenceWingMass, SizedWingboxMass, WingboxFeedback,
};

mod fuel_relief;
pub use fuel_relief::{
    declared_integral_wing_fuel_kg_m, declared_wing_fuel_case, DeclaredWingFuelCase,
};
mod geometry;
use geometry::{
    configured_surface_area, fixed_non_box_structure, flops_wing_inputs, surface_centroid,
    validate_secondary_breakdown,
};
mod support;
pub use support::{design_gross_mass_kg, StructuralInventory};
use support::{design_requirements, reconcile_against_reference};

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

include!("wing_reconciliation/reconcile.rs");
