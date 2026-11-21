// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A registered aircraft's declared landing-to-takeoff mass ratio, split out
//! of `settings.rs` to keep that module inside its size budget.

/// A registered aircraft's own certified landing-to-takeoff mass ratio.
///
/// `None` when either mass is absent or the pair is not physical (a landing
/// mass above the takeoff mass is a registry error, not a design choice), in
/// which case the generic [`crate::MassModelConfig`] fraction stays in force. A
/// notional preset such as AVE has no certified pair and keeps it by
/// construction.
pub(super) fn declared_landing_mass_ratio(preset: &crate::AircraftPreset) -> Option<f64> {
    let mlw_kg = preset.reference.mlw_kg?;
    let mtow_kg = preset.reference.mtow_kg?;
    if !super::is_valid_declared_mass_kg(mlw_kg) || !super::is_valid_declared_mass_kg(mtow_kg) {
        return None;
    }
    let ratio = mlw_kg / mtow_kg;
    (ratio > 0.0 && ratio <= 1.0).then_some(ratio)
}
