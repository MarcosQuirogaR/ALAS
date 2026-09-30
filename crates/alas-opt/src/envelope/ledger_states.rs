// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The item-level mass ledger's loading points and the public ledger entry
//! points of the CG envelope assessment. Both entry points build the named
//! `(state, cg_x, cg_z, mass_kg)` tuples and hand them to
//! `ledger_basis::assess_model_cg_envelope_from_states`.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

use super::ledger_basis::assess_model_cg_envelope_from_states;
use super::{ModelCgEnvelopeAssessment, ModelCgEnvelopeError, ModelCgLoadingState};

/// The OEW/analyzed-ZFW/analyzed-TOW points the item-level mass ledger
/// placed, in the shape [`assess_model_cg_envelope_with_ledger`]
/// needs to anchor the same five named states the shared loading-state
/// builder produces. Mid-mission/reserve are not carried here: the ledger
/// has no named state for them (see [`Self::payload_and_fuel`]'s doc
/// comment).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LedgerLoadingBasis {
    /// Ledger operating-empty mass, kg.
    pub oew_mass_kg: f64,
    /// Ledger operating-empty longitudinal CG, m.
    pub oew_cg_x_m: f64,
    /// Ledger operating-empty vertical CG, m.
    pub oew_cg_z_m: f64,
    /// Ledger zero-fuel mass, kg.
    pub zero_fuel_mass_kg: f64,
    /// Ledger zero-fuel longitudinal CG, m.
    pub zero_fuel_cg_x_m: f64,
    /// Ledger zero-fuel vertical CG, m.
    pub zero_fuel_cg_z_m: f64,
    /// Ledger analyzed-takeoff mass, kg.
    pub takeoff_mass_kg: f64,
    /// Ledger analyzed-takeoff longitudinal CG, m.
    pub takeoff_cg_x_m: f64,
    /// Ledger analyzed-takeoff vertical CG, m.
    pub takeoff_cg_z_m: f64,
}

impl LedgerLoadingBasis {
    /// Back-solve the payload and fuel mass/CG deltas the shared
    /// operational-loading-state builder needs from this basis's three
    /// ledger points, so feeding them back in reproduces the ledger's own
    /// OEW/ZFW/TOW points exactly (mass and moment both close over the
    /// subtraction by construction) while still deriving mid-mission/
    /// reserve the same way the lumped path always did: the ledger itself
    /// carries no named mid-mission/reserve state, only the four
    /// `LoadState` points (`OperatingEmpty`, `ZeroFuel`, `Takeoff`,
    /// `Landing`). The landing point enters separately, as a
    /// [`LedgerLandingState`].
    ///
    /// Returns `(payload_mass_kg, payload_cg_x_m, payload_cg_z_m,
    /// fuel_mass_kg, fuel_cg_x_m, fuel_cg_z_m)`.
    #[must_use]
    pub fn payload_and_fuel(self) -> (f64, f64, f64, f64, f64, f64) {
        let payload_mass = (self.zero_fuel_mass_kg - self.oew_mass_kg).max(0.0);
        let (payload_cg_x, payload_cg_z) = if payload_mass > 0.0 {
            (
                (self.zero_fuel_mass_kg * self.zero_fuel_cg_x_m
                    - self.oew_mass_kg * self.oew_cg_x_m)
                    / payload_mass,
                (self.zero_fuel_mass_kg * self.zero_fuel_cg_z_m
                    - self.oew_mass_kg * self.oew_cg_z_m)
                    / payload_mass,
            )
        } else {
            (self.oew_cg_x_m, self.oew_cg_z_m)
        };
        let fuel_mass = (self.takeoff_mass_kg - self.zero_fuel_mass_kg).max(0.0);
        let (fuel_cg_x, fuel_cg_z) = if fuel_mass > 0.0 {
            (
                (self.takeoff_mass_kg * self.takeoff_cg_x_m
                    - self.zero_fuel_mass_kg * self.zero_fuel_cg_x_m)
                    / fuel_mass,
                (self.takeoff_mass_kg * self.takeoff_cg_z_m
                    - self.zero_fuel_mass_kg * self.zero_fuel_cg_z_m)
                    / fuel_mass,
            )
        } else {
            (self.zero_fuel_cg_x_m, self.zero_fuel_cg_z_m)
        };
        (
            payload_mass,
            payload_cg_x,
            payload_cg_z,
            fuel_mass,
            fuel_cg_x,
            fuel_cg_z,
        )
    }
}

/// The flown mission's landing point, from the mass ledger's landing state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LedgerLandingState {
    /// Landing mass, kg.
    pub mass_kg: f64,
    /// Landing longitudinal CG, m aft of the nose tip.
    pub cg_x_m: f64,
    /// Landing vertical CG, m, same body frame as the other ledger points.
    pub cg_z_m: f64,
}

/// The same hard gate as `super::assess_model_cg_envelope`,
/// but its OEW/ZFW/TOW states come verbatim from the item-level mass
/// ledger (`ledger`) instead of the lumped model's centroids -- which
/// removes the `MassModelDisagreement` disagreement. Mid-mission/reserve stay
/// the same linear fuel-fraction mix, anchored to the ledger's own
/// endpoints instead of the lumped ones (the ledger carries no named
/// state for them). The lumped path remains the fallback with no ledger.
/// No landing state is assessed; see
/// [`assess_model_cg_envelope_with_ledger_and_landing`].
pub fn assess_model_cg_envelope_with_ledger(
    plane: &Airplane,
    ledger: LedgerLoadingBasis,
    x_np: f64,
    critical_x_np: f64,
    mac: f64,
    config: &AlasConfig,
) -> Result<ModelCgEnvelopeAssessment, ModelCgEnvelopeError> {
    assess_model_cg_envelope_with_ledger_and_landing(
        plane,
        ledger,
        None,
        x_np,
        critical_x_np,
        mac,
        config,
    )
}

/// [`assess_model_cg_envelope_with_ledger`] plus, when `landing` is given,
/// an [`ModelCgLoadingState::AnalyzedLanding`] state, appended after the
/// analyzed takeoff and gated by landing trim, the static-margin floor and the ground mechanisms
/// ([`super::PhaseLimits::LANDING`]). Gear sizing and the envelope-wide
/// top-level limits do not depend on the landing state.
#[allow(clippy::too_many_arguments)] // the ledger seam plus the optional landing point
pub fn assess_model_cg_envelope_with_ledger_and_landing(
    plane: &Airplane,
    ledger: LedgerLoadingBasis,
    landing: Option<LedgerLandingState>,
    x_np: f64,
    critical_x_np: f64,
    mac: f64,
    config: &AlasConfig,
) -> Result<ModelCgEnvelopeAssessment, ModelCgEnvelopeError> {
    let (payload_mass, payload_cg_x, payload_cg_z, fuel_mass, fuel_cg_x, fuel_cg_z) =
        ledger.payload_and_fuel();
    let mut states = super::operational_loading_states_with_z(
        ledger.oew_mass_kg,
        ledger.oew_cg_x_m,
        ledger.oew_cg_z_m,
        payload_mass,
        payload_cg_x,
        payload_cg_z,
        fuel_mass,
        fuel_cg_x,
        fuel_cg_z,
        ledger.takeoff_cg_x_m,
    );
    if let Some(landing) = landing {
        states.push((
            ModelCgLoadingState::AnalyzedLanding,
            landing.cg_x_m,
            landing.cg_z_m,
            landing.mass_kg,
        ));
    }
    assess_model_cg_envelope_from_states(
        plane,
        states,
        ledger.takeoff_cg_x_m,
        x_np,
        critical_x_np,
        mac,
        config,
    )
}
