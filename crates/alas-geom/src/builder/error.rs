// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_config::{
    FuselageSectionError, TransportPlanformError, WingProfileError, WingSectionError,
};
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
    /// A spanwise dihedral or thickness profile station is invalid.
    #[error(transparent)]
    WingProfile(#[from] WingProfileError),
    /// The wingtip device is invalid.
    #[error(transparent)]
    WingtipDevice(#[from] alas_config::WingtipDeviceError),
    /// A user-defined fuselage station is invalid or conflicts with a
    /// generated station.
    #[error(transparent)]
    FuselageSection(#[from] FuselageSectionError),
    /// The loft a fuselage references is unknown or its file is invalid.
    #[error(transparent)]
    Loft(#[from] alas_config::LoftSpecError),
    /// A tail placement option (root station, dorsal fin) is inconsistent.
    #[error("tail placement: {0}")]
    TailPlacement(String),
    /// The loft could not be instantiated at the requested length.
    #[error(transparent)]
    LoftBuild(#[from] crate::loft::LoftError),
    /// The loft's widest or tallest section disagrees with the configured
    /// diameter or height by more than 2 %.
    #[error(
        "loft {loft_id}: maximum {quantity} {loft_m:.3} m disagrees with the configured \
         {config_m:.3} m by more than 2 %"
    )]
    LoftExtent {
        /// Id of the loft.
        loft_id: String,
        /// `width` or `height`.
        quantity: &'static str,
        /// Extent of the loft, m.
        loft_m: f64,
        /// Configured extent, m.
        config_m: f64,
    },
}
