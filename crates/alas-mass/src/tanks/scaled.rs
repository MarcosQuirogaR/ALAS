// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Resolving a registered aircraft's tank arrangement on a redesigned
//! geometry.
//!
//! A registered preset declares a published volume on every cell, which is
//! the right answer for the preset itself and the wrong answer for a
//! candidate whose wing the optimizer has just changed: a typed litre count
//! does not grow with the spar box. [`FuelTankLayout::resolve_scaled`] keeps
//! the published evidence where it belongs, on the reference geometry, and
//! carries it to the candidate as a per-cell factor between the published
//! volume and the geometric estimate on that reference, so the candidate's
//! capacity is its own spar-box volume corrected by the same factor the
//! reference needed. The factor is the same kind of calibration
//! [`FuelTankLayout::resolve`] applies to a published total, taken cell by
//! cell and anchored at the reference design instead of at the candidate.

use alas_config::{FuelPolicyConfig, FuelTankLayoutConfig, GeometryConfig, StructuresConfig};
use alas_geom::aircraft::airplane::Airplane;

use super::types::{CapacitySource, FuelTank, FuelTankLayout, TankLayoutError};

impl FuelTankLayout {
    /// Resolve `config` on `plane`, scaling each geometric cell by the factor
    /// that reproduces its published volume on `reference`.
    ///
    /// `published_total_usable_volume_l` is the reference aircraft's
    /// registered total, applied on the reference exactly as
    /// [`FuelTankLayout::resolve`] applies it. A cell that has no geometric
    /// volume on the reference (a declared-volume auxiliary tank) keeps its
    /// declared volume on the candidate; a cell without a published volume
    /// carries the reference's total calibration factor, as it would on the
    /// reference itself.
    ///
    /// # Errors
    ///
    /// Any [`TankLayoutError`] from resolving the reference or the candidate,
    /// or [`TankLayoutError::InvalidCalibrationFactor`] when a cell's
    /// reference factor is not finite and positive.
    #[allow(clippy::too_many_arguments)] // one parameter per resolution input, mirroring `resolve`
    pub fn resolve_scaled(
        plane: &Airplane,
        reference: &Airplane,
        geometry: &GeometryConfig,
        structures: &StructuresConfig,
        config: &FuelTankLayoutConfig,
        policy: &FuelPolicyConfig,
        density_kg_m3: f64,
        published_total_usable_volume_l: Option<f64>,
    ) -> Result<Self, TankLayoutError> {
        let published = Self::resolve(
            reference,
            geometry,
            structures,
            config,
            policy,
            density_kg_m3,
            published_total_usable_volume_l,
        )?;
        let geometric_config = geometric_only(config);
        let reference_geometric = Self::resolve(
            reference,
            geometry,
            structures,
            &geometric_config,
            policy,
            density_kg_m3,
            None,
        )?;
        let mut candidate = Self::resolve(
            plane,
            geometry,
            structures,
            &geometric_config,
            policy,
            density_kg_m3,
            None,
        )?;

        let mut factors = Vec::with_capacity(candidate.tanks.len());
        for tank in &candidate.tanks {
            let factor = match (
                find(&published.tanks, &tank.id),
                find(&reference_geometric.tanks, &tank.id),
            ) {
                (Some(target), Some(estimate)) if estimate.usable_volume_m3 > 0.0 => {
                    target.usable_volume_m3 / estimate.usable_volume_m3
                }
                // A declared-volume tank has no geometric estimate to scale.
                (Some(_), _) if matches!(tank.capacity_source, CapacitySource::Declared) => 1.0,
                _ => return Err(TankLayoutError::InvalidCalibrationFactor { factor: f64::NAN }),
            };
            if !(factor.is_finite() && factor > 0.0) {
                return Err(TankLayoutError::InvalidCalibrationFactor { factor });
            }
            factors.push(factor);
        }
        for (tank, factor) in candidate.tanks.iter_mut().zip(factors) {
            if matches!(tank.capacity_source, CapacitySource::Declared) {
                continue;
            }
            tank.usable_volume_m3 *= factor;
            tank.usable_capacity_kg = tank.usable_volume_m3 * density_kg_m3;
            tank.unusable_kg = policy.unusable_fuel_fraction * tank.usable_capacity_kg;
            tank.capacity_source = CapacitySource::GeometricCalibrated;
        }
        candidate.geometric_calibration_factor = published.geometric_calibration_factor;
        Ok(candidate)
    }
}

/// `config` with every published cell volume removed and the total
/// calibration switched off, so a resolution yields pure geometric estimates.
fn geometric_only(config: &FuelTankLayoutConfig) -> FuelTankLayoutConfig {
    let mut geometric = config.clone();
    for cell in [
        &mut geometric.inner_wing,
        &mut geometric.mid_wing,
        &mut geometric.outer_wing,
    ] {
        cell.published_usable_volume_l = None;
    }
    geometric.center.published_usable_volume_l = None;
    geometric.trim.published_usable_volume_l = None;
    geometric.calibrate_to_published_capacity = false;
    geometric
}

fn find<'a>(tanks: &'a [FuelTank], id: &str) -> Option<&'a FuelTank> {
    tanks.iter().find(|tank| tank.id == id)
}
