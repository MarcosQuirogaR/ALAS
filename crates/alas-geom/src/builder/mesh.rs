// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What `n_subdivisions` means, which is not the same thing in both contracts.
//!
//! Product geometry reads it as an absolute spanwise panel count across a
//! whole surface, so two planforms that differ only by whether a station
//! exists are still panelled alike and a design vector cannot change the mesh
//! underneath a search. The reference replay reads it as the per-section
//! multiplier the frozen Python builder used, because the geometry fixtures
//! are that mesh and would otherwise be compared against a different one.
//!
//! `aircraft::spanwise` documents the panel allocation and the evidence for
//! it; this module only decides which of the two rules applies.

use alas_config::GeometryConfig;

use crate::aircraft::wing::{SpacingFunction, SubdivideSectionsError, Wing};

use super::{n_subdivisions_usize, GeometryContract};

/// Restore the reference's spanwise ratios on `geometry`.
///
/// The values live on [`GeometryConfig`] so the optimizer's own reference
/// replay restores exactly the same mesh without a second copy of them.
pub(super) fn restore_reference_ratios(geometry: &mut GeometryConfig) {
    geometry.restore_reference_spanwise_mesh();
}

/// Mesh `wing` spanwise by whichever rule `contract` means.
///
/// # Errors
///
/// [`SubdivideSectionsError`] from the underlying mesher.
pub(super) fn for_contract(
    contract: GeometryContract,
    wing: &Wing,
    n_subdivisions: i64,
) -> Result<Wing, SubdivideSectionsError> {
    let n = n_subdivisions_usize(n_subdivisions);
    match contract {
        GeometryContract::Product => wing.mesh_spanwise(n, SpacingFunction::Linspace),
        GeometryContract::ReferenceCompatibility => {
            wing.subdivide_sections(n, SpacingFunction::Linspace)
        }
    }
}
