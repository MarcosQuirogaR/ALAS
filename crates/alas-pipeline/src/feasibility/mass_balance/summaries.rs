// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The published shape of the mass statement: loading states, tanks, ledger
//! rows and the assessment that carries them.

use alas_mass::ledger::InertiaTensor;
use alas_mass::statement::RadiiComparison;
use alas_mass::tanks::FuelCgPoint;

/// One named loading state of the statement.
#[derive(Debug, Clone, PartialEq)]
pub struct MassStateSummary {
    /// Stable report label.
    pub label: &'static str,
    /// Total mass, kg.
    pub mass_kg: f64,
    /// Centre of gravity in the geometry frame, m.
    pub cg_m: [f64; 3],
    /// Longitudinal centre of gravity in percent of the model MAC.
    pub cg_pct_mac: f64,
    /// Inertia tensor about the centre of gravity, kg m^2.
    pub inertia_cg: InertiaTensor,
}

/// One resolved tank.
#[derive(Debug, Clone, PartialEq)]
pub struct TankSummary {
    /// Stable identifier.
    pub id: String,
    /// Tank family.
    pub kind: &'static str,
    /// Usable capacity at the declared density, kg.
    pub usable_capacity_kg: f64,
    /// Unusable fuel, kg.
    pub unusable_kg: f64,
    /// Volume centroid, m.
    pub centroid_m: [f64; 3],
    /// Where the capacity came from.
    pub capacity_source: &'static str,
    /// Burn order, lower first.
    pub burn_priority: i64,
}

/// One ledger row.
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerItemSummary {
    /// Stable identifier.
    pub id: String,
    /// Functional group label.
    pub group: &'static str,
    /// Mass, kg.
    pub mass_kg: f64,
    /// Reference point, m.
    pub position_m: [f64; 3],
}

/// The mass statement of the run.
#[derive(Debug, Clone, PartialEq)]
pub struct MassBalanceAssessment {
    /// Named loading states, in the order they are listed in the report.
    pub states: Vec<MassStateSummary>,
    /// The tanks resolved on the built geometry.
    pub tanks: Vec<TankSummary>,
    /// Sum of usable tank capacity, kg.
    pub usable_capacity_kg: f64,
    /// Sum of unusable fuel, kg; part of the operating empty mass.
    pub unusable_fuel_kg: f64,
    /// Factor applied to geometric tank estimates to meet a published total.
    pub geometric_calibration_factor: f64,
    /// Centre of gravity as fuel is loaded in the reverse of the burn order.
    pub fuel_cg_curve: Vec<FuelCgPoint>,
    /// Ledger radii of gyration at the flown takeoff state against Raymer's
    /// jet-transport reference.
    pub radii_check: RadiiComparison,
    /// Every ledger row, fuel excluded.
    pub ledger_items: Vec<LedgerItemSummary>,
    /// Lumped-model takeoff centre of gravity, percent MAC, for comparison.
    pub lumped_takeoff_cg_pct_mac: f64,
}
