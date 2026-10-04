// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Public screening entry points: product and reference-compatible mass models.

use std::path::Path;

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;

use super::run::run_airfoil_screening_with_mass_model;
use crate::refine::ScreeningMassModel;
use crate::types::{AirfoilScreeningOptions, AirfoilScreeningResult};

/// Execute the full multi-stage airfoil screening sweep.
#[allow(clippy::too_many_arguments)] // keeps the public screening contract explicit
pub fn run_airfoil_screening(
    config: &AlasConfig,
    dv: Option<&DesignVector>,
    options: &AirfoilScreeningOptions,
    mses_dir: Option<&Path>,
    progress_callback: Option<&mut dyn FnMut(&str)>,
    should_cancel: Option<&(dyn Fn() -> bool + Sync)>,
) -> Result<AirfoilScreeningResult, String> {
    run_airfoil_screening_product(
        config,
        dv,
        options,
        mses_dir,
        progress_callback,
        should_cancel,
    )
}

/// Execute screening with the frozen reference-compatible mass-coordinate
/// model for parity evidence. Product callers use the unsuffixed
/// [`run_airfoil_screening`] or the explicitly equivalent
/// [`run_airfoil_screening_product`].
#[allow(clippy::too_many_arguments)] // parity entry keeps the callback and cancellation contract explicit
pub fn run_airfoil_screening_reference_compatibility(
    config: &AlasConfig,
    dv: Option<&DesignVector>,
    options: &AirfoilScreeningOptions,
    mses_dir: Option<&Path>,
    progress_callback: Option<&mut dyn FnMut(&str)>,
    should_cancel: Option<&(dyn Fn() -> bool + Sync)>,
) -> Result<AirfoilScreeningResult, String> {
    run_airfoil_screening_with_mass_model(
        config,
        dv,
        options,
        mses_dir,
        progress_callback,
        should_cancel,
        ScreeningMassModel::ReferenceCompatibility,
    )
}

/// Execute screening with the physical product mass-coordinate model.
///
/// [`run_airfoil_screening`] is an alias for this product path, not a frozen
/// one: it forwards here with the same arguments. The reference-compatible
/// path whose fixture a product-model improvement must not change is
/// [`run_airfoil_screening_reference_compatibility`]. Desktop and other
/// product callers reach this function either way.
#[allow(clippy::too_many_arguments)] // product entry keeps the callback and cancellation contract explicit
pub fn run_airfoil_screening_product(
    config: &AlasConfig,
    dv: Option<&DesignVector>,
    options: &AirfoilScreeningOptions,
    mses_dir: Option<&Path>,
    progress_callback: Option<&mut dyn FnMut(&str)>,
    should_cancel: Option<&(dyn Fn() -> bool + Sync)>,
) -> Result<AirfoilScreeningResult, String> {
    run_airfoil_screening_with_mass_model(
        config,
        dv,
        options,
        mses_dir,
        progress_callback,
        should_cancel,
        ScreeningMassModel::StructuralWingbox,
    )
}
