// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Item-level loading states for the search's balance gate. The finalist
//! uses MassStatement, so a group centroid must not stand in for its systems,
//! split landing gear, payload items, and tank fuel on the search path.
//! Positions are metres aft of the nose and z up; masses are kilograms.

use alas_config::AlasConfig;
use alas_mass::breakdown::{
    run_product_mass_analysis_with_groups, FlopsMassBuildup, MassCoordinateModel,
    PayloadLayoutSummary,
};
use alas_mass::statement::{
    LedgerMethods, LoadState, MassStatement, MassStatementInputs, PayloadItemSummary,
};
use alas_mass::stations::component_stations_with_gear;
use alas_mass::tanks::resolve_product_layout;
use alas_payload::{build::build_payload_layout, oew::oew_and_cg};

use super::SizingOutcome;
use crate::envelope::LedgerLoadingBasis;

pub(super) fn loading_basis(
    outcome: &SizingOutcome,
    config: &AlasConfig,
) -> Result<LedgerLoadingBasis, String> {
    let mut config = config.at_sized_closure_mass(outcome.sized.takeoff_mass_kg);
    crate::objective::apply_candidate_payload_load_case(&mut config, &outcome.history.dv)
        .map_err(|error| format!("ledger load case: {error}"))?;
    let plane = &outcome.plane;
    let (oew, x_oew) = oew_and_cg(&outcome.masses, &outcome.coords);
    let payload = build_payload_layout(plane, &config, oew, x_oew)
        .map_err(|error| format!("payload ledger: {error}"))?;
    let summary = PayloadLayoutSummary {
        total_mass: payload.total_mass,
        cg_x: payload.cg_x,
        cg_y: payload.cg_y,
    };
    let model = config.analysis_mass_model(config.requirements.mtow_kg);
    // Re-evaluate the grouped decomposition at exactly the closed design
    // mass, using the same FLOPS call as the sizing mass pass. This keeps
    // individual systems and operating-item stations available to the ledger.
    let (_, _, _, buildup) = run_product_mass_analysis_with_groups(
        plane,
        &config.requirements,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&model),
        Some(&summary),
        MassCoordinateModel::StructuralWingbox(&config.structures),
        &config.landing_gear,
    )
    .map_err(|error| format!("grouped mass ledger: {error}"))?;
    // The wingbox centroid belongs to the structure's design load, not this
    // sector's dispatch load. Reference aircraft retain their declared gross
    // mass; only coupled clean-sheet structures follow the closed mass.
    let mut station_requirements = config.requirements.clone();
    station_requirements.mtow_kg = outcome.sized.design_gross_mass_kg;
    let stations = component_stations_with_gear(
        plane,
        &config.geometry,
        &station_requirements,
        &config.mass_model,
        &config.structures,
        &config.landing_gear,
    )
    .map_err(|error| format!("ledger stations: {error}"))?;
    let mut tanks = resolve_product_layout(&config, &outcome.history.dv, plane)
        .map_err(|error| format!("ledger tanks: {error}"))?;
    let groups = buildup.as_deref().map(|b| &b.systems_and_operating_items);
    if let Some(groups) = groups {
        tanks = tanks
            .with_unusable_fuel_total(groups.operating_items.unusable_fuel_kg)
            .map_err(|error| format!("ledger unusable fuel: {error}"))?;
    }
    let fuel_kg = outcome
        .masses
        .physical_fuel_mass_kg()
        .ok_or_else(|| "nonphysical ledger fuel mass".to_owned())?
        .min(tanks.usable_capacity_kg());
    let fuel = tanks
        .distribute(fuel_kg)
        .map_err(|error| format!("ledger fuel distribution: {error}"))?;
    let items = payload
        .items
        .iter()
        .filter(|item| item.mass > 0.0)
        .enumerate()
        .map(|(index, item)| PayloadItemSummary {
            label: format!("{}_{index}", item.kind.as_str()),
            mass_kg: item.mass,
            position_m: [item.x, item.y, item.z],
            extent_m: [item.length, item.width, item.height],
        })
        .collect::<Vec<_>>();
    let statement = MassStatement::build_with_methods(
        MassStatementInputs {
            masses: &outcome.masses,
            stations: &stations,
            payload_items: &items,
            takeoff_fuel_items: fuel.mass_items(&tanks),
            // This gate uses OEW/ZFW/TOW and derives its reserve/mid-mission
            // states from those endpoints, as does the finalist envelope.
            landing_fuel_items: Vec::new(),
            unusable_fuel_items: tanks.unusable_items(),
            flops: groups,
            flops_gear_split_kg: buildup.as_deref().and_then(FlopsMassBuildup::gear_split_kg),
        },
        buildup.as_deref().map_or_else(
            || LedgerMethods::from_mass_model(&config.mass_model),
            LedgerMethods::from_buildup,
        ),
    )
    .map_err(|error| format!("mass statement: {error}"))?;
    let oew = statement.state(LoadState::OperatingEmpty);
    let zfw = statement.state(LoadState::ZeroFuel);
    let tow = statement.state(LoadState::Takeoff);
    Ok(LedgerLoadingBasis {
        oew_mass_kg: oew.mass_kg,
        oew_cg_x_m: oew.cg_m[0],
        oew_cg_z_m: oew.cg_m[2],
        zero_fuel_mass_kg: zfw.mass_kg,
        zero_fuel_cg_x_m: zfw.cg_m[0],
        zero_fuel_cg_z_m: zfw.cg_m[2],
        takeoff_mass_kg: tow.mass_kg,
        takeoff_cg_x_m: tow.cg_m[0],
        takeoff_cg_z_m: tow.cg_m[2],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_item_ledgers_close_the_search_mass_and_keep_forward_rejections_visible() {
        for name in [
            "A320-200",
            "A220-300",
            "A340-300",
            "A380-800",
            "DC-10",
            "B787-9",
            "AVE",
            "ATR72-600",
        ] {
            let config = AlasConfig::from_value(&serde_json::json!({"preset": name, "optimizer": {"design_space": {"mode": "reference_adaptation"}}}))
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let design = alas_config::presets::get(name)
                .unwrap_or_else(|error| panic!("{name}: {error}"))
                .design_vector;
            let outcome = crate::mdo::sizing::run_candidate(&config, &design.to_array())
                .unwrap_or_else(|error| panic!("{name}: {}", error.reason));
            let ledger =
                loading_basis(&outcome, &config).unwrap_or_else(|error| panic!("{name}: {error}"));
            let scale = ledger.takeoff_mass_kg.max(1.0);
            assert!(
                (ledger.oew_mass_kg - outcome.sized.operating_empty_mass_kg).abs() < 1.0e-5 * scale,
                "{name}: item ledger OEW {} differs from search {}",
                ledger.oew_mass_kg,
                outcome.sized.operating_empty_mass_kg
            );
            assert!(
                (ledger.zero_fuel_mass_kg - ledger.oew_mass_kg - outcome.masses.payload).abs()
                    < 1.0e-6 * scale
            );
            assert!(ledger.takeoff_mass_kg >= ledger.zero_fuel_mass_kg);
            let assessment = crate::envelope::assess_model_cg_envelope_with_ledger(
                &outcome.plane,
                ledger,
                outcome.x_np,
                outcome.x_np,
                outcome.mac,
                &config,
            )
            .unwrap_or_else(|error| panic!("{name}: {error}"));
            let takeoff = assessment
                .loading_states
                .iter()
                .find(|state| state.state == crate::envelope::ModelCgLoadingState::AnalyzedTakeoff)
                .unwrap_or_else(|| panic!("{name}: missing takeoff"));
            assert!((takeoff.cg_x_m - ledger.takeoff_cg_x_m).abs() < 1.0e-9);
            assert!(takeoff
                .constraints
                .iter()
                .any(|constraint| constraint.constraint
                    == crate::envelope::ModelCgConstraint::PhysicalForwardCgLimit));
        }
    }
}
