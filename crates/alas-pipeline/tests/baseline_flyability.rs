// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Every registered aircraft, at its baseline (no optimization), flies its
//! own missions on the unified fuel model: the report's segment mission
//! model on a trip plan frozen for each mission
//! (`alas_opt::mdo::solve_planned_dispatch`), under the configured fuel
//! policy, the declared MTOW and the usable tank capacity.
//!
//! - (a) the design range with the full (achievable maximum) payload; for a
//!   payload-defined design mission (the C295: 6,000 kg, Airbus brochure
//!   TMMA0009/01/2025 pp.22-23, at the TCDS-certified 21,050 kg, EASA.A.186
//!   Issue 08 item 13 p.79) the design payload at the range the model solves
//!   for it, since the maximum payload leaves no fuel for reserves at the
//!   certified mass;
//! - (b) the configured route (the great-circle distance between the
//!   configured aerodromes) with the laid-out payload.
//!
//! A mission closes when its trip plus reserves fit the loadable fuel: the
//! takeoff-mass limit less the zero-fuel mass, or the tanks less taxi-out,
//! whichever binds. Flown at that load the aircraft then lands with at least
//! the reserves (and the taxi-in budget) the policy requires, because the
//! fuel remaining on landing is the loadable fuel less the trip.

// A test asserts on values it constructed or on registered presets, so a
// failed unwrap is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::airports::get as get_airport;
use alas_config::AlasConfig;
use alas_mass::dispatch::{DispatchLimits, DispatchStatus};
use alas_mass::fuel_plan::FuelBurnModel;
use alas_mass::payload_range::{max_range_with_reserves, RangeStatus};
use alas_opt::mdo::{solve_planned_dispatch, PlannedTrips};
use alas_opt::SegmentMissionModel;
use alas_pipeline::feasibility::assess_fuel_capacity;
use alas_pipeline::fuel_model::report_mission_model;
use alas_pipeline::quick_analysis::{payload_range_corners, report_oew_kg};
use alas_pipeline::FullAnalysis;

const NMI: f64 = 1_852.0;

/// Missions that honestly do not close, as `(preset, case, largest shortfall
/// kg)`. An entry fails the test when its shortfall grows past the bound or
/// clears, so it is re-measured rather than left behind.
///
/// - B747-400 design mission (4,900 nmi at the full-payload ZFW 246,073 kg):
///   short of reserves by 294 kg. Cause: the product parasite items (pylons,
///   wing-body fairing, upsweep and base drag, CD0 about +10 %); with only
///   those drag items removed the mission closes. Bound 400 kg.
const REGISTERED_SHORTFALLS: &[(&str, &str, f64)] = &[("B747-400", "design mission", 400.0)];

/// The reserve margin of the mission at `zero_fuel_mass_kg` over `range_m`,
/// kg: loadable fuel less the dispatch plan's trip plus reserves.
fn reserve_margin_kg(
    config: &AlasConfig,
    model: &SegmentMissionModel,
    capacity_kg: f64,
    zero_fuel_mass_kg: f64,
    range_m: f64,
) -> f64 {
    let policy = &config.fuel_policy;
    let mtow_kg = config.requirements.mtow_kg;
    let taxi_kg = model.taxi_fuel_flow_kg_s().unwrap() * policy.taxi_time_min * 60.0;
    let ceiling_kg = mtow_kg.min(zero_fuel_mass_kg + capacity_kg - taxi_kg);
    let limits = DispatchLimits {
        mtow_kg,
        mzfw_kg: None,
        mlw_kg: None,
        usable_capacity_kg: Some(capacity_kg),
    };
    let tolerance_kg = config.optimizer.objective.sizing_tolerance_kg;
    let solution = solve_planned_dispatch(
        model,
        zero_fuel_mass_kg,
        ceiling_kg,
        range_m,
        policy,
        &limits,
        50,
        tolerance_kg,
    )
    .unwrap_or_else(|error| {
        panic!(
            "dispatch at ZFW {zero_fuel_mass_kg:.0} kg over {:.0} nmi: {error:?}",
            range_m / NMI
        )
    });
    assert!(
        matches!(
            solution.status,
            DispatchStatus::Converged
                | DispatchStatus::MtowLimited { .. }
                | DispatchStatus::TankLimited { .. }
        ),
        "ZFW {zero_fuel_mass_kg:.0} kg over {:.0} nmi at MTOW {mtow_kg:.0} kg, tank {capacity_kg:.0} kg: {:?}",
        range_m / NMI,
        solution.status
    );
    ceiling_kg - zero_fuel_mass_kg - solution.plan.takeoff_fuel_kg()
}

#[test]
fn every_preset_flies_its_design_mission_and_route_with_reserves_intact() {
    let mut failures = Vec::new();
    for name in alas_config::presets::available() {
        let preset = alas_config::presets::get(name).unwrap();
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let report = FullAnalysis::new(config.clone())
            .run(&preset.design_vector, true)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let model = report_mission_model(&config, &report).unwrap();
        let capacity_kg = assess_fuel_capacity(&config, &report.design, &report)
            .capacity_kg
            .unwrap();
        let oew_kg = report_oew_kg(&report);
        let max_payload_kg = payload_range_corners(&config, &report)
            .unwrap()
            .max_payload_kg;
        let laid_out_payload_kg = report.component_masses["Payload"];
        let origin = get_airport(&config.departure_airport).unwrap();
        let destination = get_airport(&config.arrival_airport).unwrap();
        let route_m = alas_route::route::haversine_m(
            origin.latitude_deg,
            origin.longitude_deg,
            destination.latitude_deg,
            destination.longitude_deg,
        );
        let design_nmi = config.optimizer.objective.design_range_nmi;
        let design_m = if design_nmi > 0.0 {
            design_nmi * NMI
        } else {
            route_m
        };
        let tolerance_kg = config.optimizer.objective.sizing_tolerance_kg;
        let (design_payload_kg, design_m) = if config.design_mission().range.is_model_defined() {
            // The range is the model's output at the certified mass for the
            // design payload: the largest the loadable fuel flies with
            // reserves.
            let payload_kg = config.design_payload_kg().0;
            let zero_fuel_kg = oew_kg + payload_kg;
            let taxi_kg =
                model.taxi_fuel_flow_kg_s().unwrap() * config.fuel_policy.taxi_time_min * 60.0;
            let fuel_kg = (config.requirements.mtow_kg - zero_fuel_kg).min(capacity_kg - taxi_kg);
            let corner = max_range_with_reserves(
                zero_fuel_kg + fuel_kg,
                fuel_kg,
                &config.fuel_policy,
                &PlannedTrips(&model),
                50.0,
            );
            assert!(
                matches!(corner.status, RangeStatus::Solved) && corner.range_m > 0.0,
                "{name}: the design payload has no range with reserves: {corner:?}"
            );
            (payload_kg, corner.range_m)
        } else {
            (max_payload_kg, design_m)
        };
        for (case, zero_fuel_mass_kg, range_m) in [
            ("design mission", oew_kg + design_payload_kg, design_m),
            ("configured route", oew_kg + laid_out_payload_kg, route_m),
        ] {
            let margin_kg =
                reserve_margin_kg(&config, &model, capacity_kg, zero_fuel_mass_kg, range_m);
            if let Some((_, _, bound_kg)) = REGISTERED_SHORTFALLS
                .iter()
                .find(|(preset, registered, _)| *preset == name && *registered == case)
            {
                if margin_kg >= -tolerance_kg || -margin_kg > *bound_kg {
                    failures.push(format!(
                        "{name} {case}: registered shortfall up to {bound_kg:.0} kg, margin now {margin_kg:.0} kg: re-measure the entry"
                    ));
                }
                continue;
            }
            if margin_kg < -tolerance_kg {
                failures.push(format!(
                    "{name} {case}: {:.0} nmi at ZFW {zero_fuel_mass_kg:.0} kg, reserves short by {:.0} kg",
                    range_m / NMI,
                    -margin_kg
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
