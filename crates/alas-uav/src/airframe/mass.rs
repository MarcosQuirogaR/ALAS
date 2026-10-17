// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mass statement: items, totals, CG, inertia about the CG, load cases,
//! calibration and the ballast solve (UAV-PLAN 3.7).

use std::collections::BTreeMap;

use super::inertia::{Inertia, Vec3};
use super::AirframeError;

/// Plan 3.7: reported error band of an uncalibrated bottom-up total (ESTIMATE;
/// documented misses: 3.72 vs 3.37 kg, +6.25 %, 2.917 vs 3.2 kg).
pub const UNCALIBRATED_MASS_BAND: f64 = 0.08;

/// Where an item's mass comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MassBasis {
    /// A figure from an `alas-uav-db` row (vendor datasheet or measurement).
    Database,
    /// A weighed override entered by the user.
    Weighed,
    /// A user-entered mass (payload, ballast, allowance).
    UserInput,
    /// Computed from areal densities, geometry or statistics: ESTIMATE.
    Estimate,
}

/// Group an item belongs to in the mass breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MassGroup {
    /// Airframe structure (skins, spars, ribs, boom, fuselage shell).
    Structure,
    /// Fasteners, glue, covering and similar allowances.
    Allowance,
    /// Motor, propeller, ESC, battery.
    Propulsion,
    /// Servos, receiver, flight controller, telemetry, wiring.
    Avionics,
    /// Landing gear and wheels.
    Gear,
    /// Payload.
    Payload,
    /// Trim ballast.
    Ballast,
    /// Anything else.
    Other,
}

/// One lumped mass with its own CG and inertia about that CG.
#[derive(Debug, Clone, PartialEq)]
pub struct MassItem {
    /// Name (node name or generated label).
    pub name: String,
    /// Breakdown group.
    pub group: MassGroup,
    /// Mass, kg.
    pub mass_kg: f64,
    /// CG in body axes (x aft from the nose datum), m.
    pub cg_m: Vec3,
    /// Inertia tensor about the item's own CG in body axes, kg m^2.
    pub inertia_cg: Inertia,
    /// Provenance of the mass.
    pub basis: MassBasis,
    /// Whether the inertia is a modelled shape (false: point mass fallback).
    pub inertia_modelled: bool,
}

/// A payload / ballast scenario applied on top of the fixed tree.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadCase {
    /// Case name.
    pub name: String,
    /// Payload mass by `Part::Payload` role, kg (a role not listed is empty).
    pub payload_kg: BTreeMap<String, f64>,
    /// Extra ballast masses `(mass kg, position m)`.
    pub ballast: Vec<(f64, Vec3)>,
}

/// The assembled mass statement.
#[derive(Debug, Clone, PartialEq)]
pub struct MassStatement {
    /// All items.
    pub items: Vec<MassItem>,
}

impl MassStatement {
    /// Total mass, kg.
    #[must_use]
    pub fn total_mass_kg(&self) -> f64 {
        self.items.iter().map(|i| i.mass_kg).sum()
    }

    /// Mass of one group, kg.
    #[must_use]
    pub fn group_mass_kg(&self, group: MassGroup) -> f64 {
        self.items
            .iter()
            .filter(|i| i.group == group)
            .map(|i| i.mass_kg)
            .sum()
    }

    /// Mass carried on a given basis, kg.
    #[must_use]
    pub fn basis_mass_kg(&self, basis: MassBasis) -> f64 {
        self.items
            .iter()
            .filter(|i| i.basis == basis)
            .map(|i| i.mass_kg)
            .sum()
    }

    /// Fraction of the total that is an ESTIMATE (areal density, statistics).
    #[must_use]
    pub fn estimate_fraction(&self) -> f64 {
        let total = self.total_mass_kg();
        if total > 0.0 {
            self.basis_mass_kg(MassBasis::Estimate) / total
        } else {
            0.0
        }
    }

    /// Centre of gravity in body axes, m.
    ///
    /// # Errors
    ///
    /// [`AirframeError::Invalid`] for a statement of zero mass.
    pub fn cg_m(&self) -> Result<Vec3, AirframeError> {
        let m = self.total_mass_kg();
        if !(m.is_finite() && m > 0.0) {
            return Err(AirframeError::Invalid("total mass must be > 0".into()));
        }
        let mut c = [0.0; 3];
        for item in &self.items {
            for (acc, x) in c.iter_mut().zip(item.cg_m) {
                *acc += item.mass_kg * x;
            }
        }
        Ok(c.map(|v| v / m))
    }

    /// Inertia tensor about a point `p` (body axes), kg m^2.
    #[must_use]
    pub fn inertia_about(&self, p: Vec3) -> Inertia {
        self.items.iter().fold(Inertia::ZERO, |acc, i| {
            let d = [i.cg_m[0] - p[0], i.cg_m[1] - p[1], i.cg_m[2] - p[2]];
            acc.plus(i.inertia_cg.translated(i.mass_kg, d))
        })
    }

    /// Inertia tensor about the total CG, kg m^2.
    ///
    /// # Errors
    ///
    /// As [`Self::cg_m`].
    pub fn inertia_about_cg(&self) -> Result<Inertia, AirframeError> {
        Ok(self.inertia_about(self.cg_m()?))
    }

    /// Scale every ESTIMATE item by the team calibration factor `k_team`
    /// (as-built over predicted); inertias scale with the mass.
    ///
    /// # Errors
    ///
    /// [`AirframeError::Invalid`] for a non-positive or non-finite factor.
    pub fn calibrated(&self, k_team: f64) -> Result<Self, AirframeError> {
        if !(k_team.is_finite() && k_team > 0.0) {
            return Err(AirframeError::Invalid("k_team must be > 0".into()));
        }
        let items = self
            .items
            .iter()
            .map(|i| {
                if i.basis == MassBasis::Estimate {
                    MassItem {
                        mass_kg: i.mass_kg * k_team,
                        inertia_cg: i.inertia_cg.scaled(k_team),
                        ..i.clone()
                    }
                } else {
                    i.clone()
                }
            })
            .collect();
        Ok(Self { items })
    }

    /// Absolute half-band of the total, kg, for the uncalibrated model: the
    /// plan's +/-8 % applied to the estimated share only (database and weighed
    /// masses are taken as exact).
    #[must_use]
    pub fn uncalibrated_band_kg(&self) -> f64 {
        UNCALIBRATED_MASS_BAND * self.basis_mass_kg(MassBasis::Estimate)
    }

    /// Ballast mass to place at `x_ballast_m` so that the CG moves to
    /// `x_target_m` (both along body x).  With `M` the current total mass and
    /// `x_cg` its CG: `m = M (x_cg - x_t) / (x_t - x_b)`.
    ///
    /// # Errors
    ///
    /// [`AirframeError::NoSolution`] if the ballast would be negative (it sits
    /// on the wrong side of the target) or the positions coincide.
    pub fn ballast_for_cg(&self, x_target_m: f64, x_ballast_m: f64) -> Result<f64, AirframeError> {
        let cg = self.cg_m()?[0];
        let denom = x_target_m - x_ballast_m;
        if denom.abs() < 1e-9 {
            return Err(AirframeError::NoSolution(
                "ballast position coincides with the target CG".into(),
            ));
        }
        let m = self.total_mass_kg() * (cg - x_target_m) / denom;
        if m < 0.0 {
            return Err(AirframeError::NoSolution(
                "ballast at that position cannot move the CG to the target".into(),
            ));
        }
        Ok(m)
    }

    /// `(min, max)` CG x over several statements (load-case envelope), m.
    ///
    /// # Errors
    ///
    /// Empty input or a zero-mass statement.
    pub fn cg_x_envelope(cases: &[Self]) -> Result<(f64, f64), AirframeError> {
        if cases.is_empty() {
            return Err(AirframeError::Invalid("no load cases".into()));
        }
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for c in cases {
            let x = c.cg_m()?[0];
            lo = lo.min(x);
            hi = hi.max(x);
        }
        Ok((lo, hi))
    }
}
