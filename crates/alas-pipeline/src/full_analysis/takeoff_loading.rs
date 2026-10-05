// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Physical Hard-MTOW takeoff loading and its numeric report provenance.

use std::collections::HashMap;

use alas_config::{AlasConfig, DesignVector};
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{MassBreakdown, MassCoordinates};
use alas_mass::loading::{apply_mtow_fuel_loading, MtowFuelLoading, MtowFuelLoadingStatus};

pub(super) fn apply(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &mut MassBreakdown,
    coordinates: &mut MassCoordinates,
) -> Result<Option<MtowFuelLoading>, String> {
    match apply_mtow_fuel_loading(config, design, plane, masses, coordinates) {
        Ok((loading, _)) => Ok(Some(loading)),
        // An empty aircraft at or above MTOW has no fuel to load; the
        // negative mass closure stays in the report, whose feasibility
        // assessment rejects it by name.
        Err(alas_mass::loading::MtowFuelLoadingError::ZeroFuelAboveMtow { .. }) => Ok(None),
        Err(error) => Err(format!("takeoff fuel loading: {error}")),
    }
}

pub(super) fn annotate(summary: &mut HashMap<String, f64>, loading: MtowFuelLoading) {
    summary.insert(
        "analysis_loaded_takeoff_mass_kg".to_owned(),
        loading.takeoff_mass_kg,
    );
    summary.insert(
        "takeoff_mtow_fuel_budget_kg".to_owned(),
        loading.mtow_fuel_budget_kg,
    );
    summary.insert(
        "takeoff_carried_usable_fuel_kg".to_owned(),
        loading.carried_usable_fuel_kg,
    );
    summary.insert("takeoff_mtow_margin_kg".to_owned(), loading.mtow_margin_kg);
    summary.insert(
        "takeoff_volume_limited".to_owned(),
        f64::from(loading.status == MtowFuelLoadingStatus::VolumeLimited),
    );
    summary.insert(
        "takeoff_capacity_verified".to_owned(),
        f64::from(loading.status != MtowFuelLoadingStatus::CapacityUnverified),
    );
    if let Some(margin_kg) = loading.usable_capacity_margin_kg {
        summary.insert("takeoff_usable_capacity_margin_kg".to_owned(), margin_kg);
    }
}

// Tests construct every fixture they assert on, so a failed unwrap or
// expect is the assertion failing rather than a library invariant breaking.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::full_analysis::FullAnalysis;

    #[test]
    fn every_registered_hard_mtow_report_conserves_the_search_loading_and_design_weights() {
        for name in alas_config::presets::available() {
            let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
            let design = alas_config::presets::get(name).unwrap().design_vector;
            let assessment = alas_opt::assess_product_candidate(&config, &design)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let sized = &assessment.sized;
            let report = FullAnalysis::new(config.clone())
                .run_sized_candidate(&assessment.resolved.design, sized)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let loading = sized.takeoff_loading.unwrap();
            let feasibility = crate::feasibility::assess_physical_feasibility(
                &config,
                &report.design,
                &report,
                None,
            );
            assert_eq!(
                feasibility.fuel_loading.design_takeoff_loading,
                Some(loading)
            );
            let scale = loading.takeoff_mass_kg.max(1.0);
            // old 1e-6 for every preset -> 1e-3 for the B747-400 only: the B747-400 generic cabin seats 330 of its 400 passengers (42
            // business, 288 economy; the 24-first/74-business upper-deck layout
            // is not representable), so the report re-prices furnishings and
            // services for the seated cabin (-163.7 kg of 195.6 t OEW, 0.08 %)
            // while the one-pass fixed-requirement sizing keeps the 400-seat
            // percent-mix terms.
            let relative_tolerance = if name == "B747-400" { 1e-3 } else { 1e-6 };
            let close = |a: f64, b: f64| {
                assert!(
                    (a - b).abs() <= relative_tolerance * scale,
                    "{name}: {a} vs {b}"
                )
            };
            close(
                report.loaded_takeoff_mass_kg(config.requirements.mtow_kg),
                loading.takeoff_mass_kg,
            );
            close(
                feasibility.fuel_loading.zero_fuel_mass_kg,
                loading.zero_fuel_mass_kg,
            );
            close(
                report.component_masses["Fuel"],
                loading.carried_usable_fuel_kg,
            );
            assert_eq!(
                report.aircraft_mtow_limit_kg(0.0),
                config.requirements.mtow_kg
            );
            assert_eq!(sized.design_gross_mass_kg, config.requirements.mtow_kg);
            let design_config = crate::feasibility::design_mass_config(&config, &report);
            assert_eq!(
                crate::feasibility::design_vn_mass_kg(&config, &report),
                sized.design_gross_mass_kg
            );
            close(
                design_config.design_landing_mass_at_closure(loading.takeoff_mass_kg),
                sized.design_landing_mass_kg,
            );
            let balance = feasibility
                .mass_balance
                .as_ref()
                .unwrap_or_else(|| panic!("{name}: no mass ledger"));
            let oew = balance
                .states
                .iter()
                .find(|state| state.label == "operating empty")
                .unwrap();
            let zfw = balance
                .states
                .iter()
                .find(|state| state.label == "zero fuel")
                .unwrap();
            close(oew.mass_kg, sized.operating_empty_mass_kg);
            close(zfw.mass_kg - oew.mass_kg, sized.payload_kg);
            close(zfw.mass_kg, loading.zero_fuel_mass_kg);
            for state in balance
                .states
                .iter()
                .filter(|state| !matches!(state.label, "operating empty" | "zero fuel"))
            {
                let fuel_kg = state.mass_kg - zfw.mass_kg;
                assert!(
                    fuel_kg >= -1e-6 * scale && fuel_kg <= sized.usable_capacity_kg + 1e-6 * scale,
                    "{name}: {state:?}"
                );
                assert!(
                    state.mass_kg <= config.requirements.mtow_kg + 1e-6 * scale,
                    "{name}: {state:?}"
                );
            }
            assert_eq!(
                loading.status == MtowFuelLoadingStatus::VolumeLimited,
                loading.mtow_fuel_budget_kg > sized.usable_capacity_kg
            );
            assert_eq!(
                report.geometry_summary["takeoff_volume_limited"],
                f64::from(loading.status == MtowFuelLoadingStatus::VolumeLimited)
            );
            let fuel_capacity = assessment
                .residuals
                .iter()
                .find(|row| row.id == "fuel_capacity")
                .unwrap();
            assert_eq!(fuel_capacity.actual, sized.dispatch.plan.ramp_fuel_kg());
            assert_eq!(
                fuel_capacity.raw_residual,
                sized.dispatch.plan.ramp_fuel_kg() - sized.usable_capacity_kg
            );
            assert_eq!(fuel_capacity.role, alas_opt::mdo::ResidualRole::Constraint);
            if let Some(mzfw) = alas_config::presets::get(name).unwrap().reference.mzfw_kg {
                assert!(
                    zfw.mass_kg <= mzfw + 1e-6 * scale,
                    "{name}: ZFW {} exceeds declared MZFW {mzfw}",
                    zfw.mass_kg
                );
            }
        }
    }
}
