// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Maximum available takeoff fuel under independent weight and volume limits.
//!
//! With unusable fuel already in operating empty mass, ZFW = OEW + payload
//! and TOW = ZFW + usable fuel. The available load is therefore
//! `min(MTOW - ZFW, usable capacity)`. MTOW remains a structural design
//! limit, not a fuel-loading requirement. Mission fuel including reserves
//! and taxi is a separate requirement and must still fit the tanks.
//! This follows the loading/maximum-weight distinction in FAA-H-8083-1B,
//! Aircraft Weight and Balance Handbook, chapters 1 and 10. No empirical
//! coefficient or tolerance enters this mass identity. All masses are kg.

use alas_config::{AlasConfig, DesignVector};
use alas_geom::aircraft::airplane::Airplane;

use crate::breakdown::{calculate_physical_cg, MassBreakdown, MassCoordinates, OEW_KEYS};
use crate::tanks::resolve_product_layout;

/// Governing limit of the maximum available takeoff fuel load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtowFuelLoadingStatus {
    /// The available fuel reaches MTOW without filling all usable volume.
    MassLimited,
    /// Full usable tanks leave the takeoff mass below MTOW.
    VolumeLimited,
    /// Capacity is unavailable; the weight remainder is not verified loadable.
    CapacityUnverified,
}

impl MtowFuelLoadingStatus {
    /// Stable report identifier for the governing limit.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MassLimited => "mass_limited",
            Self::VolumeLimited => "volume_limited",
            Self::CapacityUnverified => "capacity_unverified",
        }
    }
}

/// One maximum-fuel load case, distinct from a mission's required fuel plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MtowFuelLoading {
    /// Operating empty mass, including unusable fuel, plus carried payload.
    pub zero_fuel_mass_kg: f64,
    /// The weight-budget fuel ceiling, `MTOW - ZFW`.
    pub mtow_fuel_budget_kg: f64,
    /// Usable tank capacity, when independently established.
    pub usable_capacity_kg: Option<f64>,
    /// Usable fuel physically carried at brake release.
    pub carried_usable_fuel_kg: f64,
    /// `ZFW + carried usable fuel`, never above MTOW.
    pub takeoff_mass_kg: f64,
    /// Nonnegative available weight below MTOW.
    pub mtow_margin_kg: f64,
    /// Unfilled usable capacity, when established; zero at the volume limit.
    pub usable_capacity_margin_kg: Option<f64>,
    /// The governing physical limit, or missing capacity evidence.
    pub status: MtowFuelLoadingStatus,
}

/// A loading input that cannot describe a nonnegative physical mass budget.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MtowFuelLoadingError {
    /// A mass input is nonfinite, negative, or has a nonpositive MTOW.
    #[error("invalid MTOW fuel-loading mass input")]
    InvalidMass,
    /// Operating empty mass plus payload already exceeds the takeoff limit.
    #[error("zero-fuel mass exceeds MTOW by {excess_kg} kg")]
    ZeroFuelAboveMtow {
        /// Signed excess over the takeoff limit, positive here.
        excess_kg: f64,
    },
    /// A resolved inventory cannot place the requested physical fuel load.
    #[error("takeoff fuel distribution: {0}")]
    FuelDistribution(crate::tanks::TankLayoutError),
    /// The resolved fuel items do not have a finite physical centroid.
    #[error("takeoff fuel distribution produced a nonfinite centroid")]
    InvalidFuelMoment,
}

impl MtowFuelLoading {
    /// Resolve the maximum available usable fuel at a declared MTOW.
    ///
    /// Unknown capacity remains explicitly unverified. An invalid declared
    /// capacity is an input error, never permission to fill a mass remainder.
    pub fn resolve(
        mtow_kg: f64,
        zero_fuel_mass_kg: f64,
        usable_capacity_kg: Option<f64>,
    ) -> Result<Self, MtowFuelLoadingError> {
        if !mtow_kg.is_finite()
            || mtow_kg <= 0.0
            || !zero_fuel_mass_kg.is_finite()
            || zero_fuel_mass_kg < 0.0
            || usable_capacity_kg.is_some_and(|kg| !kg.is_finite() || kg < 0.0)
        {
            return Err(MtowFuelLoadingError::InvalidMass);
        }
        let budget_kg = mtow_kg - zero_fuel_mass_kg;
        if budget_kg < 0.0 {
            return Err(MtowFuelLoadingError::ZeroFuelAboveMtow {
                excess_kg: -budget_kg,
            });
        }
        let fuel_kg = usable_capacity_kg.map_or(budget_kg, |kg| budget_kg.min(kg));
        let takeoff_mass_kg = zero_fuel_mass_kg + fuel_kg;
        Ok(Self {
            zero_fuel_mass_kg,
            mtow_fuel_budget_kg: budget_kg,
            usable_capacity_kg,
            carried_usable_fuel_kg: fuel_kg,
            takeoff_mass_kg,
            mtow_margin_kg: mtow_kg - takeoff_mass_kg,
            usable_capacity_margin_kg: usable_capacity_kg.map(|kg| kg - fuel_kg),
            status: match usable_capacity_kg {
                Some(kg) if kg < budget_kg => MtowFuelLoadingStatus::VolumeLimited,
                Some(_) => MtowFuelLoadingStatus::MassLimited,
                None => MtowFuelLoadingStatus::CapacityUnverified,
            },
        })
    }
}

/// Apply the maximum available fuel to a pure-FLOPS product mass state.
///
/// Components retain their design weights. Unusable fuel is already an OEW
/// operating item, so only the usable-fuel slot and its tank centroid change.
/// Coordinates are metres aft of the nose, z up. Missing tank evidence keeps
/// its existing coordinate and is recorded as `CapacityUnverified`.
pub fn apply_mtow_fuel_loading(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &mut MassBreakdown,
    coordinates: &mut MassCoordinates,
) -> Result<(MtowFuelLoading, [f64; 3]), MtowFuelLoadingError> {
    let oew_kg: f64 = OEW_KEYS
        .iter()
        .map(|key| masses.get(key).unwrap_or(0.0))
        .sum();
    let tanks = resolve_product_layout(config, design, plane).ok();
    let loading = MtowFuelLoading::resolve(
        config.requirements.mtow_kg,
        oew_kg + masses.payload,
        tanks.as_ref().map(|layout| layout.usable_capacity_kg()),
    )?;
    apply_loading(loading, tanks.as_ref(), masses, coordinates)?;
    Ok((loading, calculate_physical_cg(masses, coordinates)))
}

fn apply_loading(
    loading: MtowFuelLoading,
    tanks: Option<&crate::tanks::FuelTankLayout>,
    masses: &mut MassBreakdown,
    coordinates: &mut MassCoordinates,
) -> Result<(), MtowFuelLoadingError> {
    let centroid = match tanks.filter(|_| loading.carried_usable_fuel_kg > 0.0) {
        Some(tanks) => {
            let fuel = tanks
                .distribute(loading.carried_usable_fuel_kg)
                .map_err(MtowFuelLoadingError::FuelDistribution)?;
            let centroid = fuel.properties(tanks).cg_m;
            if !centroid.iter().all(|value| value.is_finite()) {
                return Err(MtowFuelLoadingError::InvalidFuelMoment);
            }
            Some(centroid)
        }
        None => None,
    };
    masses.fuel = loading.carried_usable_fuel_kg;
    if let Some(centroid) = centroid {
        coordinates.fuel = centroid;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_weight_and_volume_limits_conserve_every_loading_case() {
        for mtow_kg in [1.0, 100.0, 80_000.0, 560_000.0] {
            for fraction in [0.0, 0.25, 0.8, 1.0] {
                let zfw_kg = mtow_kg * fraction;
                for capacity_kg in [0.0, 0.1 * mtow_kg, 0.5 * mtow_kg, 2.0 * mtow_kg] {
                    let load =
                        MtowFuelLoading::resolve(mtow_kg, zfw_kg, Some(capacity_kg)).unwrap();
                    assert_eq!(
                        load.carried_usable_fuel_kg,
                        (mtow_kg - zfw_kg).min(capacity_kg)
                    );
                    assert_eq!(load.takeoff_mass_kg, zfw_kg + load.carried_usable_fuel_kg);
                    assert!(load.takeoff_mass_kg <= mtow_kg);
                    assert!(load.carried_usable_fuel_kg <= capacity_kg);
                    assert_eq!(load.mtow_margin_kg, mtow_kg - load.takeoff_mass_kg);
                    assert_eq!(
                        load.usable_capacity_margin_kg,
                        Some(capacity_kg - load.carried_usable_fuel_kg)
                    );
                    assert_eq!(
                        load.status == MtowFuelLoadingStatus::VolumeLimited,
                        capacity_kg < mtow_kg - zfw_kg
                    );
                }
            }
        }
    }

    #[test]
    fn invalid_or_overweight_inputs_never_become_clamped_feasible_loads() {
        assert_eq!(
            MtowFuelLoading::resolve(100.0, 101.0, Some(20.0)),
            Err(MtowFuelLoadingError::ZeroFuelAboveMtow { excess_kg: 1.0 })
        );
        for invalid in [f64::NAN, f64::INFINITY, -1.0] {
            assert!(MtowFuelLoading::resolve(100.0, invalid, Some(20.0)).is_err());
            assert!(MtowFuelLoading::resolve(100.0, 50.0, Some(invalid)).is_err());
        }
        let unknown = MtowFuelLoading::resolve(100.0, 50.0, None).unwrap();
        assert_eq!(unknown.status, MtowFuelLoadingStatus::CapacityUnverified);
        assert_eq!(unknown.usable_capacity_margin_kg, None);
    }

    #[test]
    fn capacity_reductions_cannot_increase_takeoff_mass_or_change_zero_fuel_mass() {
        let zfw_kg = 55_000.0;
        let mut previous_mass = f64::INFINITY;
        for capacity_kg in [50_000.0, 25_000.0, 20_000.0, 10_000.0, 0.0] {
            let load = MtowFuelLoading::resolve(80_000.0, zfw_kg, Some(capacity_kg)).unwrap();
            assert_eq!(load.zero_fuel_mass_kg, zfw_kg);
            assert!(load.takeoff_mass_kg <= previous_mass);
            previous_mass = load.takeoff_mass_kg;
        }
    }

    #[test]
    fn a_distribution_failure_cannot_publish_a_new_mass_with_a_stale_moment() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
        let design = alas_config::presets::get("A320-200").unwrap().design_vector;
        let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap();
        let tanks = resolve_product_layout(&config, &design, &plane).unwrap();
        let mut masses = MassBreakdown {
            wing: 1.0,
            h_stab: 0.0,
            v_stab: 0.0,
            fuselage: 0.0,
            gear: 0.0,
            propulsion: 0.0,
            systems: 0.0,
            furnishings: 0.0,
            payload: 0.0,
            fuel: 2.0,
        };
        let mut coordinates = MassCoordinates {
            wing: [0.0; 3],
            h_stab: [0.0; 3],
            v_stab: [0.0; 3],
            fuselage: [0.0; 3],
            gear: [0.0; 3],
            propulsion: [0.0; 3],
            systems: [0.0; 3],
            furnishings: [0.0; 3],
            payload: [0.0; 3],
            fuel: [42.0; 3],
        };
        let before = (masses, coordinates);
        let loading = MtowFuelLoading::resolve(100_000.0, 1.0, None).unwrap();
        assert!(matches!(
            apply_loading(loading, Some(&tanks), &mut masses, &mut coordinates),
            Err(MtowFuelLoadingError::FuelDistribution(
                crate::tanks::TankLayoutError::Overflow { .. }
            ))
        ));
        assert_eq!((masses, coordinates), before);
    }
}
