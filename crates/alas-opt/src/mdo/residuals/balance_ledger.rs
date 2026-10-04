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
use alas_mass::product_stations::product_component_stations;
use alas_mass::statement::{
    LedgerMethods, LoadState, MassStatement, MassStatementInputs, PayloadItemSummary,
};
use alas_mass::tanks::{resolve_product_layout, TankLayoutError};
use alas_payload::{build::build_payload_layout, oew::oew_and_cg};

use super::SizingOutcome;
use crate::envelope::{LedgerLandingState, LedgerLoadingBasis};

pub(super) fn loading_basis(
    outcome: &SizingOutcome,
    config: &AlasConfig,
) -> Result<LedgerLoadingBasis, String> {
    loading_bases(outcome, config).map(|(design, _)| design)
}

/// The design loading of [`loading_basis`] and, from the same item ledger,
/// the dispatched route's takeoff and landing points: the usable fuel the
/// dispatch plan loads at brake release, burned down from the same tank
/// state to its destination landing fuel, as the reporting mass statement
/// builds its flown states. `None` when the dispatch fuel does not load into
/// the tanks; the dispatch residuals already reject that candidate.
pub(super) fn loading_bases(
    outcome: &SizingOutcome,
    config: &AlasConfig,
) -> Result<
    (
        LedgerLoadingBasis,
        Option<(LedgerLoadingBasis, LedgerLandingState)>,
    ),
    String,
> {
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
    let stations = product_component_stations(&config, &outcome.history.dv, plane, &outcome.masses)
        .map_err(|error| format!("ledger stations: {error}"))?;
    let mut tanks = resolve_product_layout(&config, &outcome.history.dv, plane)
        .map_err(|error| format!("ledger tanks: {error}"))?;
    let groups = buildup.as_deref().map(|b| &b.systems_and_operating_items);
    if let Some(groups) = groups {
        tanks = tanks
            .with_unusable_fuel_total(groups.operating_items.unusable_fuel_kg)
            .map_err(|error| format!("ledger unusable fuel: {error}"))?;
    }
    let carried_kg = outcome
        .masses
        .physical_fuel_mass_kg()
        .ok_or_else(|| "nonphysical ledger fuel mass".to_owned())?;
    let fuel_kg = tanks.loadable_fuel_kg(carried_kg).map_err(|error| {
        fuel_load_error(
            &error,
            config.optimizer.objective.mtow_sizing,
            carried_kg,
            tanks.usable_capacity_kg(),
            tanks.unusable_fuel_kg(),
        )
    })?;
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
    let basis = |tow: &alas_mass::ledger::MassProperties| LedgerLoadingBasis {
        oew_mass_kg: oew.mass_kg,
        oew_cg_x_m: oew.cg_m[0],
        oew_cg_z_m: oew.cg_m[2],
        zero_fuel_mass_kg: zfw.mass_kg,
        zero_fuel_cg_x_m: zfw.cg_m[0],
        zero_fuel_cg_z_m: zfw.cg_m[2],
        takeoff_mass_kg: tow.mass_kg,
        takeoff_cg_x_m: tow.cg_m[0],
        takeoff_cg_z_m: tow.cg_m[2],
        takeoff_pitch_inertia_kg_m2: tow.inertia_cg.iyy,
    };
    let design = basis(&statement.state(LoadState::Takeoff));
    let dispatch = outcome.sized.flown_dispatch();
    // A report without a flown mission has no dispatched states to gate.
    let flown = config.mission.enabled.then_some(()).and_then(|()| {
        let takeoff_kg = tanks
            .loadable_fuel_kg(dispatch.plan.takeoff_fuel_kg())
            .ok()?;
        let landing_kg = (dispatch.destination_landing_mass_kg - dispatch.zero_fuel_mass_kg)
            .clamp(0.0, takeoff_kg);
        let takeoff = tanks.distribute(takeoff_kg).ok()?;
        let landing = takeoff.burned(&tanks, takeoff_kg - landing_kg).ok()?;
        let tow = statement.with_fuel_items(&takeoff.mass_items(&tanks));
        let lw = statement.with_fuel_items(&landing.mass_items(&tanks));
        Some((
            basis(&tow),
            LedgerLandingState {
                mass_kg: lw.mass_kg,
                cg_x_m: lw.cg_m[0],
                cg_z_m: lw.cg_m[2],
            },
        ))
    });
    Ok((design, flown))
}

fn fuel_load_error(
    error: &TankLayoutError,
    mode: alas_config::MtowSizing,
    usable_remainder_kg: f64,
    usable_capacity_kg: f64,
    unusable_kg: f64,
) -> String {
    if mode == alas_config::MtowSizing::FixedRequirement
        && matches!(error, TankLayoutError::Overflow { .. })
    {
        // FLOPS unusable fuel is already in OEW; the MTOW-minus-ZFW remainder
        // is usable fuel. Adding unusable fuel converts both sides to total fuel.
        format!("tank overflow at fixed MTOW: usable fuel remainder {usable_remainder_kg:.3} kg exceeds usable tank capacity {usable_capacity_kg:.3} kg by {:.3} kg; unusable fuel {unusable_kg:.3} kg is excluded from usable capacity (total fuel {:.3} kg, total capacity {:.3} kg)",
            usable_remainder_kg - usable_capacity_kg,
            usable_remainder_kg + unusable_kg, usable_capacity_kg + unusable_kg)
    } else {
        format!("ledger fuel load: {error}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_mtow_overflow_reports_usable_and_total_fuel_on_matching_bases() {
        let usable_volume_m3 = 25.0;
        let density_kg_m3 = 800.0;
        let capacity_kg = usable_volume_m3 * density_kg_m3;
        let unusable_kg = 100.0;
        let fixed_mtow_kg = 80_000.0;
        let oew_including_unusable_kg = 40_000.0;
        let payload_kg = 15_000.0;
        let remainder_kg = fixed_mtow_kg - oew_including_unusable_kg - payload_kg;
        let error = TankLayoutError::Overflow {
            excess_kg: remainder_kg - capacity_kg,
        };
        let detail = fuel_load_error(
            &error,
            alas_config::MtowSizing::FixedRequirement,
            remainder_kg,
            capacity_kg,
            unusable_kg,
        );
        assert_eq!(detail, "tank overflow at fixed MTOW: usable fuel remainder 25000.000 kg exceeds usable tank capacity 20000.000 kg by 5000.000 kg; unusable fuel 100.000 kg is excluded from usable capacity (total fuel 25100.000 kg, total capacity 20100.000 kg)");
        let invalid = TankLayoutError::InvalidFuelMass { fuel_kg: -1.0 };
        assert!(fuel_load_error(
            &invalid,
            alas_config::MtowSizing::FixedRequirement,
            -1.0,
            capacity_kg,
            unusable_kg
        )
        .starts_with("ledger fuel load:"));
    }

    #[test]
    fn mission_sized_preset_ledgers_close_search_mass_and_keep_forward_rejections_visible() {
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
            // The ledger conservation check uses a mission-sized, loadable
            // dispatch; fixed MTOW does not guarantee a loadable fuel remainder.
            let config = AlasConfig::from_value(&serde_json::json!({"preset": name, "optimizer": {
                "design_space": {"mode": "reference_adaptation"},
                "objective": {"mtow_sizing": "sized_by_mission"}
            }}))
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
            // The tanks carry the whole dispatched fuel: none is clipped away.
            let carried_kg = outcome.masses.physical_fuel_mass_kg().unwrap_or(f64::NAN);
            assert!(
                (ledger.takeoff_mass_kg - ledger.zero_fuel_mass_kg - carried_kg).abs()
                    < 1.0e-6 * scale,
                "{name}: ledger fuel {} differs from carried {carried_kg}",
                ledger.takeoff_mass_kg - ledger.zero_fuel_mass_kg
            );
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

    #[test]
    fn hard_mtow_preset_ledgers_conserve_mass_and_report_volume_limits() {
        for name in alas_config::presets::available() {
            let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
            let design = alas_config::presets::get(name).unwrap().design_vector;
            let outcome = crate::mdo::sizing::run_candidate(&config, &design.to_array()).unwrap();
            let ledger =
                loading_basis(&outcome, &config).unwrap_or_else(|error| panic!("{name}: {error}"));
            let load = outcome.sized.takeoff_loading.unwrap();
            let tanks = resolve_product_layout(&config, &design, &outcome.plane).unwrap();
            let capacity_kg: f64 = tanks
                .tanks()
                .iter()
                .map(|tank| tank.usable_volume_m3 * tanks.density_kg_m3)
                .sum();
            let carried = outcome.masses.physical_fuel_mass_kg().unwrap();
            let scale = ledger.takeoff_mass_kg.max(1.0);
            assert!(
                (ledger.takeoff_mass_kg - ledger.zero_fuel_mass_kg - carried).abs() < 1e-6 * scale
            );
            assert!(
                (ledger.oew_mass_kg - outcome.sized.operating_empty_mass_kg).abs() < 1e-5 * scale
            );
            assert!((ledger.takeoff_mass_kg - load.takeoff_mass_kg).abs() < 1e-6 * scale);
            assert_eq!(carried, load.mtow_fuel_budget_kg.min(capacity_kg));
            assert!(ledger.takeoff_mass_kg <= config.requirements.mtow_kg + 1e-6 * scale);
            assert_eq!(
                outcome.sized.design_gross_mass_kg,
                config.requirements.mtow_kg
            );
            let envelope = crate::envelope::assess_model_cg_envelope_with_ledger(
                &outcome.plane,
                ledger,
                outcome.x_np,
                outcome.x_np,
                outcome.mac,
                &config,
            )
            .unwrap_or_else(|error| panic!("{name}: {error}"));
            for state in &envelope.loading_states {
                assert!(
                    state.mass_kg <= load.takeoff_mass_kg + 1e-6 * scale,
                    "{name}: {state:?}"
                );
                if state.state != crate::envelope::ModelCgLoadingState::OperatingEmpty {
                    let fuel_kg = state.mass_kg - ledger.zero_fuel_mass_kg;
                    assert!(
                        fuel_kg >= -1e-6 * scale && fuel_kg <= capacity_kg + 1e-6 * scale,
                        "{name}: {state:?}"
                    );
                }
            }
            let landing_row = super::super::mass_residuals(&outcome, &config)
                .into_iter()
                .find(|row| row.id == "landing_mass")
                .unwrap();
            assert_eq!(landing_row.limit, outcome.sized.design_landing_mass_kg);
            assert_eq!(
                landing_row.actual,
                outcome.sized.dispatch.destination_landing_mass_kg
            );
            assert_eq!(landing_row.role, crate::mdo::ResidualRole::Constraint);
            if matches!(name, "A320-200" | "DC-10") {
                assert_eq!(
                    load.status,
                    alas_mass::loading::MtowFuelLoadingStatus::VolumeLimited,
                    "{name}: {load:?}"
                );
                assert!(load.mtow_margin_kg > 0.0);
                assert_eq!(load.usable_capacity_margin_kg, Some(0.0));
                assert_eq!(carried, capacity_kg);
            }
        }
    }

    #[test]
    fn fixed_mtow_cannot_admit_more_fuel_than_the_tanks_hold() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"}))
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            config.optimizer.objective.mtow_sizing,
            alas_config::MtowSizing::FixedRequirement
        );
        let design = alas_config::presets::get("A320-200")
            .unwrap_or_else(|error| panic!("{error}"))
            .design_vector;
        let mut outcome = crate::mdo::sizing::run_candidate(&config, &design.to_array())
            .unwrap_or_else(|error| panic!("{}", error.reason));
        let tanks = resolve_product_layout(&config, &design, &outcome.plane)
            .unwrap_or_else(|error| panic!("{error}"));
        // Even treating all unusable fuel as usable cannot fit twice the
        // complete tank inventory; no empirical aircraft mass is pinned here.
        outcome.masses.fuel = 2.0 * (tanks.usable_capacity_kg() + tanks.unusable_fuel_kg());
        let error = loading_basis(&outcome, &config)
            .err()
            .unwrap_or_else(|| panic!("excess fuel must reject the ledger"));
        assert!(error.starts_with("tank overflow at fixed MTOW:"), "{error}");
        let rows = super::super::balance_residuals(
            &outcome,
            &config,
            crate::mdo::ResidualRole::Constraint,
        );
        assert!(rows.iter().any(|row| row.id == "cg_model_error"
            && row.role == crate::mdo::ResidualRole::Constraint
            && row.violated()
            && row.actual == 1.0
            && row.limit == 0.0
            && row.normalized_violation == 1.0
            && row.detail.as_deref() == Some(error.as_str())));
    }
}
