// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The structural payload bound of an unchanged registered preset.

use alas_config::design_variables::DesignVector;
use alas_config::{presets, AlasConfig};

/// Resolve the structural payload bound for an unchanged registered preset:
/// the smaller of the configured cap (normally the published `MZFW - OEW`)
/// and `MZFW - modeled OEW`, because the modeled OEW can be heavier than the
/// source OEW and the layout would otherwise respect the cap while producing
/// an overweight zero-fuel mass. Notional designs inherit no published MZFW.
pub(crate) fn effective_structural_payload_limit_kg(
    config: &AlasConfig,
    design: &DesignVector,
    modeled_oew_kg: f64,
) -> Option<f64> {
    let preset = presets::get(&config.preset).ok()?;
    if *design != preset.design_vector {
        return None;
    }
    let mzfw_kg = preset.reference.mzfw_kg?;
    let available_payload_kg = mzfw_kg - modeled_oew_kg;
    if !mzfw_kg.is_finite() || !modeled_oew_kg.is_finite() || available_payload_kg <= 0.0 {
        return None;
    }
    let configured_limit_kg = config.requirements.max_structural_payload_kg;
    Some(
        if configured_limit_kg.is_finite() && configured_limit_kg > 0.0 {
            configured_limit_kg.min(available_payload_kg)
        } else {
            available_payload_kg
        },
    )
}
