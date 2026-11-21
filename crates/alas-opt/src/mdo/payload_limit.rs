// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The structural payload bound of an unchanged registered preset.

use alas_config::design_variables::DesignVector;
use alas_config::{presets, AlasConfig};

/// Resolve the structural payload bound for an unchanged registered preset:
/// the published MZFW less the modeled OEW. One rule for every preset that
/// publishes an MZFW: the declared `max_structural_payload_kg` (historically
/// the published MZFW less the published OEW) does not bind, because the
/// modeled aircraft has its own empty mass and the maximum-payload corner sits
/// on the certified MZFW line whatever that mass is. The declared value only
/// binds designs that inherit no published MZFW. Notional designs inherit no
/// published MZFW; an explicit `requirements.mzfw_kg` is a requirement on the
/// design being evaluated and bounds every candidate, whatever its geometry.
pub fn effective_structural_payload_limit_kg(
    config: &AlasConfig,
    design: &DesignVector,
    modeled_oew_kg: f64,
) -> Option<f64> {
    if config.requirements.mzfw_kg.is_none() {
        let preset = presets::get(&config.preset).ok()?;
        if !design.same_geometry(&preset.design_vector) {
            return None;
        }
    }
    let mzfw_kg = presets::published_mzfw_kg(config)?;
    let available_payload_kg = mzfw_kg - modeled_oew_kg;
    if !mzfw_kg.is_finite() || !modeled_oew_kg.is_finite() || available_payload_kg <= 0.0 {
        return None;
    }
    Some(available_payload_kg)
}
