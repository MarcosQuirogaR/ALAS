// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_config::{FuselageSectionError, TransportPlanformError, WingSectionError};
use alas_math::CubicSplineError;

use crate::aircraft::fuselage::FuselageXSecError;
use crate::aircraft::wing::SubdivideSectionsError;

/// Why [`super::AircraftBuilder::build`] could not assemble an
/// [`crate::aircraft::airplane::Airplane`].
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BuildError {
    /// [`crate::airfoil_library::AirfoilLibrary::get`] did not resolve a name
    /// the geometry configuration names. Every airfoil the configuration can
    /// name is checked, so this is a fallible boundary for external inputs.
    #[error("airfoil {0:?} did not resolve")]
    UnresolvedAirfoil(String),
    /// Shaping a wing section (`build_section`'s `repanel` step) failed.
    #[error(transparent)]
    Section(#[from] CubicSplineError),
    /// Subdividing a wing's cross-sections failed: an `n_subdivisions` below
    /// 2, or a blend between two distinct airfoils that failed to repanel.
    #[error(transparent)]
    Subdivide(#[from] SubdivideSectionsError),
    /// A fuselage cross-section's radius/width/height combination was invalid.
    #[error(transparent)]
    FuselageXSec(#[from] FuselageXSecError),
    /// The configured transport planform has invalid stations, chords, or
    /// sweep angles.
    #[error(transparent)]
    Planform(#[from] TransportPlanformError),
    /// A user-defined wing station is invalid or conflicts with the planform.
    #[error(transparent)]
    WingSection(#[from] WingSectionError),
    /// A user-defined fuselage station is invalid or conflicts with a
    /// generated station.
    #[error(transparent)]
    FuselageSection(#[from] FuselageSectionError),
}
