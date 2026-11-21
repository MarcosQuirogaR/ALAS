// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The published and modelled nominal fuel capacity of a registered preset.

use alas_config::AlasConfig;

use crate::mdo::sizing::SizingOutcome;
use crate::mdo::types::ConstraintFamily::Mass;
use crate::mdo::types::ConstraintResidual;
use crate::mdo::ResidualRole;

/// The published usable fuel mass of the registered aircraft a reference
/// adaptation redesigns, kg, or `None` in any other mode or when the preset
/// carries no published figure. A clean-sheet study has no such requirement,
/// and the working-default FLOPS capacity is a placeholder, not a source.
pub(super) fn published_fuel(config: &AlasConfig) -> Option<(Option<f64>, Option<f64>)> {
    if config.optimizer.design_space.mode != alas_config::DesignMode::ReferenceAdaptation {
        return None;
    }
    let reference = &alas_config::presets::get(&config.preset).ok()?.reference;
    let positive = |v: Option<f64>| v.filter(|x| x.is_finite() && *x > 0.0);
    Some((
        positive(reference.usable_fuel_mass_kg),
        positive(reference.usable_fuel_volume_l),
    ))
}

/// Modelled usable tank capacity of the registered preset design, kg, for a
/// reference adaptation; `None` in any other mode or when it cannot be
/// resolved. Resolved once per complete configuration
/// ([`crate::mdo::nominal_cache`]): geometry, structures, tank declarations and
/// fuel density all move it.
pub(super) fn nominal_tank_capacity_kg(config: &AlasConfig) -> Option<f64> {
    published_fuel(config)?;
    NOMINAL_TANK_CAPACITY_KG.get_or_resolve(config, |config| {
        let design = alas_config::presets::get(&config.preset)
            .ok()?
            .design_vector;
        let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .ok()?;
        crate::mdo::tanks::tank_capacity_kg(config, &plane, &design)
    })
}

/// The cache of [`nominal_tank_capacity_kg`].
pub(super) static NOMINAL_TANK_CAPACITY_KG: crate::mdo::nominal_cache::NominalCache<f64> =
    crate::mdo::nominal_cache::NominalCache::new();

/// The tank rows of a sized candidate (see the comment inside).
pub(super) fn tank_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let sized = &outcome.sized;
    let mut residuals = Vec::new();
    // A redesigned registered aircraft must carry in its own tanks the fuel
    // it needs to fly every mission the configuration requires of it; the
    // registered tank volume is not itself a requirement. The requirement
    // set is the mission the takeoff mass is closed on (the route under
    // `fixed_requirement` and `mtow_band`, the design mission under
    // `payload_adjusted`) and, when a design mission is closed, the route
    // flown off-design at the closed mass. Each one's fuel is the candidate's
    // own dispatch: ramp fuel, i.e. taxi plus trip (the block fuel) plus the
    // reserves the fuel policy computes on that trip. The required fuel is
    // the largest over the set. No maximum-fuel or ferry point is in the set,
    // because no configuration declares one as a requirement. The fuel the
    // Hard-MTOW loading puts on board under `fixed_requirement` is not a
    // requirement either: it is the most the mass and volume budgets admit
    // at takeoff, not fuel any required mission burns. The same rows are
    // evaluated in the search and at the reporting replay, so both
    // fidelities read the one dispatch quantity.
    let capacity_kg = sized.usable_capacity_kg;
    if published_fuel(config).is_some() && capacity_kg.is_finite() {
        let route_kg = sized
            .mtow
            .offdesign
            .as_ref()
            .filter(|flight| {
                matches!(
                    flight.dispatch.status,
                    alas_mass::dispatch::DispatchStatus::Converged
                )
            })
            .map(|flight| flight.dispatch.plan.ramp_fuel_kg());
        let required_kg = route_kg.map_or(sized.ramp_fuel_kg, |kg| kg.max(sized.ramp_fuel_kg));
        residuals.push(ConstraintResidual::scaled(
            "fuel_capacity_declared",
            Mass,
            capacity_kg,
            required_kg,
            "kg",
            required_kg - capacity_kg,
            role,
        ));
        // The registered aircraft's modelled tanks, for context only: both
        // come from one resolver (`alas_mass::tanks::resolve_product_layout`)
        // at one fuel density.
        if let Some(nominal_kg) = nominal_tank_capacity_kg(&outcome.registered_config(config)) {
            residuals.push(ConstraintResidual::scaled(
                "fuel_capacity_nominal",
                Mass,
                capacity_kg,
                nominal_kg,
                "kg",
                nominal_kg - capacity_kg,
                ResidualRole::Diagnostic,
            ));
        }
        // Published figures, for context only; never ranked.
        if let Some(published) = published_fuel(config) {
            if let Some(kg) = published.0 {
                residuals.push(ConstraintResidual::scaled(
                    "fuel_capacity_published",
                    Mass,
                    capacity_kg,
                    kg,
                    "kg",
                    kg - capacity_kg,
                    ResidualRole::Diagnostic,
                ));
            }
            if let Some(litres) = published.1 {
                let modelled_l = capacity_kg
                    / alas_mass::tanks::inventory_density_kg_m3(config).max(1e-9)
                    * 1_000.0;
                residuals.push(ConstraintResidual::scaled(
                    "fuel_volume_published",
                    Mass,
                    modelled_l,
                    litres,
                    "L",
                    litres - modelled_l,
                    ResidualRole::Diagnostic,
                ));
            }
        }
    }
    residuals
}
