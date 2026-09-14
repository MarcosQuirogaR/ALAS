// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Freestream turbulence inputs and derived SST state.

use serde::{Deserialize, Serialize};

/// Freestream turbulence specification used to derive the SST inlet fields.
///
/// The viscosity-ratio form is the default for external flow because it
/// constrains the far-field eddy viscosity directly.  The length-scale form
/// remains available for studies whose tunnel or inflow specification gives a
/// physical integral scale instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurbulenceSpecification {
    /// Derive omega from `nu_t/nu` and molecular kinematic viscosity.
    ViscosityRatio,
    /// Derive omega from the configured turbulence length scale.
    LengthScale,
}

impl Default for TurbulenceSpecification {
    fn default() -> Self {
        // This preserves the interpretation of configurations written before
        // the explicit specification field existed.  CfdStudyConfig::default
        // opts into the documented external-flow viscosity-ratio setup.
        Self::LengthScale
    }
}

impl TurbulenceSpecification {
    /// Stable label for settings, reports and lifecycle messages.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ViscosityRatio => "viscosity ratio",
            Self::LengthScale => "length scale",
        }
    }
}

/// Effective freestream turbulence values written to the OpenFOAM fields.
///
/// `k` is specific turbulent kinetic energy in m^2/s^2, `omega` is specific
/// dissipation rate in 1/s, and `nu_t` is the estimated k-omega eddy
/// viscosity in m^2/s.  The ratio is an audit estimate of the boundary-state
/// turbulence level; the SST model may evolve it inside the domain.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TurbulenceState {
    /// Input form used for the derivation.
    pub specification: TurbulenceSpecification,
    /// Turbulence intensity as a fraction of freestream speed.
    pub intensity_fraction: f64,
    /// Configured length scale in metres.
    pub configured_length_m: f64,
    /// Configured turbulent-to-molecular viscosity ratio.
    pub configured_viscosity_ratio: f64,
    /// Specific turbulent kinetic energy in m^2/s^2.
    pub k_m2_s2: f64,
    /// Specific dissipation rate in 1/s.
    pub omega_s_inv: f64,
    /// Estimated k-omega eddy viscosity in m^2/s.
    pub nu_t_m2_s: f64,
    /// Estimated turbulent-to-molecular viscosity ratio.
    pub nu_t_over_nu: f64,
    /// Length scale implied by the effective omega, in metres.
    pub effective_length_m: f64,
}
