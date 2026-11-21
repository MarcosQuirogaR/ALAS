// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The item-level mass ledger's loading points and the public ledger entry
//! points of the CG envelope assessment. Both entry points build the named
//! `(state, cg_x, cg_z, mass_kg)` tuples and hand them to
//! `ground_ends::assess_model_cg_envelope_from_states`.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

use super::ground_ends::assess_model_cg_envelope_from_states;
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
    /// Ledger pitch moment of inertia about the analyzed-takeoff centre of
    /// gravity, `I_yy`, kg m^2; `NaN` when not available, which selects
    /// Raymer's radius of gyration in the rotation criterion.
    pub takeoff_pitch_inertia_kg_m2: f64,
    /// Ledger `I_yy` about the operating-empty centre of gravity, kg m^2;
    /// `NaN` when not available.
    pub oew_pitch_inertia_kg_m2: f64,
    /// Ledger `I_yy` about the zero-fuel centre of gravity, kg m^2; `NaN`
    /// when not available.
    pub zero_fuel_pitch_inertia_kg_m2: f64,
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

impl LedgerLoadingBasis {
    /// Pitch moment of inertia `I_yy`, kg m^2, about the centre of gravity
    /// of a loading state of this basis (`mass_kg` at `cg_x_m`, `cg_z_m`):
    /// the operating-empty tensor plus the payload, plus, above the
    /// zero-fuel mass, the fraction of the take-off fuel the state carries,
    /// each moved to the state's centre of gravity by the parallel-axis
    /// theorem (`I = I_own + m d^2`, `d` in the x-z plane). The payload's
    /// and the fuel's own inertias are the differences of the ledger's
    /// three tensors; a partial fuel load carries the same fraction of the
    /// fuel's own inertia, at the centroid the state's centre of gravity
    /// implies (the states fill the fuel at one centroid,
    /// `super::operational_loading_states_with_z`). Exact at the three
    /// ledger points.
    ///
    /// Without the operating-empty and zero-fuel tensors, the take-off
    /// tensor scaled to the state's mass at the take-off radius of
    /// gyration; `NaN` without a finite take-off tensor.
    #[must_use]
    pub fn pitch_inertia_about_kg_m2(&self, mass_kg: f64, cg_x_m: f64, cg_z_m: f64) -> f64 {
        let moved = |own: f64, mass: f64, x: f64, z: f64| {
            own + mass * ((x - cg_x_m).powi(2) + (z - cg_z_m).powi(2))
        };
        let tensors = [
            self.oew_pitch_inertia_kg_m2,
            self.zero_fuel_pitch_inertia_kg_m2,
            self.takeoff_pitch_inertia_kg_m2,
        ];
        if !tensors
            .iter()
            .all(|value| value.is_finite() && *value > 0.0)
        {
            return if self.takeoff_pitch_inertia_kg_m2.is_finite()
                && self.takeoff_pitch_inertia_kg_m2 > 0.0
                && self.takeoff_mass_kg > 0.0
            {
                self.takeoff_pitch_inertia_kg_m2 / self.takeoff_mass_kg * mass_kg
            } else {
                f64::NAN
            };
        }
        let (payload, payload_x, payload_z, fuel, fuel_x, fuel_z) = self.payload_and_fuel();
        // Own inertias about each part's centroid, from the ledger tensors
        // about the zero-fuel and take-off centres of gravity.
        let distance2 = |x0: f64, z0: f64, x1: f64, z1: f64| (x0 - x1).powi(2) + (z0 - z1).powi(2);
        let payload_own = (self.zero_fuel_pitch_inertia_kg_m2
            - self.oew_pitch_inertia_kg_m2
            - self.oew_mass_kg
                * distance2(
                    self.oew_cg_x_m,
                    self.oew_cg_z_m,
                    self.zero_fuel_cg_x_m,
                    self.zero_fuel_cg_z_m,
                )
            - payload
                * distance2(
                    payload_x,
                    payload_z,
                    self.zero_fuel_cg_x_m,
                    self.zero_fuel_cg_z_m,
                ))
        .max(0.0);
        let fuel_own = (self.takeoff_pitch_inertia_kg_m2
            - self.zero_fuel_pitch_inertia_kg_m2
            - self.zero_fuel_mass_kg
                * distance2(
                    self.zero_fuel_cg_x_m,
                    self.zero_fuel_cg_z_m,
                    self.takeoff_cg_x_m,
                    self.takeoff_cg_z_m,
                )
            - fuel * distance2(fuel_x, fuel_z, self.takeoff_cg_x_m, self.takeoff_cg_z_m))
        .max(0.0);
        let oew = moved(
            self.oew_pitch_inertia_kg_m2,
            self.oew_mass_kg,
            self.oew_cg_x_m,
            self.oew_cg_z_m,
        );
        // The load above the base point, at the centroid the state's centre
        // of gravity implies.
        let load = |base_mass: f64, base_x: f64, base_z: f64, own_full: f64, full: f64| {
            let load_mass = (mass_kg - base_mass).max(0.0);
            if load_mass <= 0.0 || full <= 0.0 {
                return 0.0;
            }
            let x = (mass_kg * cg_x_m - base_mass * base_x) / load_mass;
            let z = (mass_kg * cg_z_m - base_mass * base_z) / load_mass;
            moved(own_full * (load_mass / full).min(1.0), load_mass, x, z)
        };
        if mass_kg <= self.zero_fuel_mass_kg {
            oew + load(
                self.oew_mass_kg,
                self.oew_cg_x_m,
                self.oew_cg_z_m,
                payload_own,
                payload,
            )
        } else {
            let zero_fuel = oew + moved(payload_own, payload, payload_x, payload_z);
            zero_fuel
                + load(
                    self.zero_fuel_mass_kg,
                    self.zero_fuel_cg_x_m,
                    self.zero_fuel_cg_z_m,
                    fuel_own,
                    fuel,
                )
        }
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
        Some(&ledger),
        ledger.takeoff_cg_x_m,
        x_np,
        critical_x_np,
        mac,
        config,
    )
}
