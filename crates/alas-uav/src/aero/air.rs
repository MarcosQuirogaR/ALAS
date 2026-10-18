// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Air state used by every low-Re aerodynamic model.
//!
//! All quantities SI.  Viscosity follows Sutherland's law
//! `mu = 1.458e-6 T^1.5 / (T + 110.4)` (Pa s, T in K), the same expression as
//! the powertrain's `Flight::new`, so the wing, the propeller and the
//! fuselage see one viscosity.

use alas_atmo::Atmosphere;

use super::AeroError;

/// Static air state and airspeed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AirState {
    /// True airspeed, m/s (> 0).
    pub v_m_s: f64,
    /// Density, kg/m3.
    pub rho_kg_m3: f64,
    /// Static temperature, K.
    pub t_k: f64,
    /// Dynamic viscosity, Pa s.
    pub mu_pa_s: f64,
    /// Speed of sound, m/s.
    pub a_m_s: f64,
}

impl AirState {
    /// Sutherland viscosity at `t_k`, Pa s.
    pub fn sutherland_mu(t_k: f64) -> f64 {
        1.458e-6 * t_k.powf(1.5) / (t_k + 110.4)
    }

    /// ISA state at geopotential `altitude_m` (alas-atmo).
    ///
    /// # Errors
    ///
    /// [`AeroError::InvalidInput`] for a non-finite or non-positive airspeed.
    pub fn from_isa(altitude_m: f64, v_m_s: f64) -> Result<Self, AeroError> {
        let atmosphere = Atmosphere::new(altitude_m);
        Self::checked(
            v_m_s,
            atmosphere.density(),
            atmosphere.temperature(),
            atmosphere.dynamic_viscosity(),
            atmosphere.speed_of_sound(),
        )
    }

    /// State from density and temperature (viscosity from Sutherland, speed
    /// of sound `sqrt(1.4 * 287.053 T)`), for non-ISA days.
    ///
    /// # Errors
    ///
    /// [`AeroError::InvalidInput`] for non-finite or non-positive inputs.
    pub fn from_rho_t(v_m_s: f64, rho_kg_m3: f64, t_k: f64) -> Result<Self, AeroError> {
        Self::checked(
            v_m_s,
            rho_kg_m3,
            t_k,
            Self::sutherland_mu(t_k),
            (1.4 * 287.052_874 * t_k).sqrt(),
        )
    }

    fn checked(v: f64, rho: f64, t: f64, mu: f64, a: f64) -> Result<Self, AeroError> {
        let ok = [v, rho, t, mu, a].iter().all(|x| x.is_finite() && *x > 0.0);
        if !ok {
            return Err(AeroError::InvalidInput(format!(
                "air state needs positive finite v, rho, T, mu, a (got v={v}, rho={rho}, T={t}, mu={mu}, a={a})"
            )));
        }
        Ok(Self {
            v_m_s: v,
            rho_kg_m3: rho,
            t_k: t,
            mu_pa_s: mu,
            a_m_s: a,
        })
    }

    /// Dynamic pressure `rho V^2 / 2`, Pa.
    pub fn q_pa(&self) -> f64 {
        0.5 * self.rho_kg_m3 * self.v_m_s * self.v_m_s
    }

    /// Mach number.
    pub fn mach(&self) -> f64 {
        self.v_m_s / self.a_m_s
    }

    /// Reynolds number on length `l_m` at the freestream speed.
    pub fn reynolds(&self, l_m: f64) -> f64 {
        self.rho_kg_m3 * self.v_m_s * l_m / self.mu_pa_s
    }
}
