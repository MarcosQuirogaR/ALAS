// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The controlled mass-model experiment matrix.
//!
//! One runnable command regenerates the evidence behind
//! `docs/mass-model-architecture.md`: for every registered preset it records
//! the three physically different questions the mass model can be asked,
//! side by side and from the same resolved configuration.
//!
//! * **Case A, fixed-aircraft mass estimation.** The registered geometry and
//!   configuration, every component evaluated at the declared design gross
//!   mass (`requirements.mtow_kg`) and the mode-aware design landing mass,
//!   through the same `FullAnalysis::run` path the product report uses.
//! * **Case B, fixed-aircraft mission evaluation.** The same airframe in
//!   `DesignMode::BaselineSandbox`, flown through the mission-sized closure
//!   in each `MtowSizing` mode, on the preset's operational route and on its
//!   declared FLOPS design range. The structural mass must not change with
//!   the mission; the sweep rows make that check explicit.
//! * **Case C, coupled new-aircraft sizing.** The same design vector in
//!   `DesignMode::CleanSheet`, where the sizing basis is allowed to follow
//!   the closure. Component deltas against case A show what the coupling
//!   changes.
//!
//! Failed closures are kept as rows with their typed reason; nothing is
//! weakened to make a case converge. Reconstructed cases itemize their
//! declared input changes; the registry is never edited.
//!
//! Usage:
//!
//! ```text
//! cargo run -p alas-pipeline --release --example mass_experiment_matrix -- \
//!     outputs/mass-model-consolidation/<label> [--presets A320-200,A220-300] [--no-missions] [--engine-fallback]
//! ```
//!
//! `--engine-fallback` evaluates every case with the FLOPS equation 76 engine
//! correlation instead of the registered certified dry engine masses, so the
//! before/after pair of that input change comes from one binary. Every run
//! also writes `oew-reference-registry.json`, the one OEW reference registry
//! (`alas_config::oew_reference`) that the reference columns are read from.
#![allow(clippy::print_stdout)]

mod fixed;
mod mission;
mod rows;
mod support;

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use alas_config::{presets, DesignMode, MtowSizing};
use serde_json::{json, Map, Value};

use fixed::{a320_sensitivities, fixed_design_weight_case};
use mission::{mission_case, MissionCase};
use support::{csv_line, fmt, opt, SWEEP_RANGES_NMI};

fn write(path: &Path, text: &str) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out_dir = PathBuf::from(
        args.first()
            .cloned()
            .unwrap_or_else(|| "outputs/mass-model-consolidation/run".to_owned()),
    );
    let mut selected: Option<Vec<String>> = None;
    let mut missions = true;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--presets" => {
                index += 1;
                selected = args
                    .get(index)
                    .map(|list| list.split(',').map(|s| s.trim().to_owned()).collect());
            }
            "--no-missions" => missions = false,
            "--engine-fallback" => {
                support::ENGINE_FALLBACK.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            other => println!("ignoring unknown argument {other}"),
        }
        index += 1;
    }
    let engine_fallback = support::ENGINE_FALLBACK.load(std::sync::atomic::Ordering::Relaxed);
    fs::create_dir_all(&out_dir)?;
    // The one OEW reference registry every artifact of this run reads.
    write(
        &out_dir.join("oew-reference-registry.json"),
        &serde_json::to_string_pretty(&alas_config::oew_reference::registry_json())?,
    )?;

    let mission_cases = mission::mission_cases();

    let mut presets_json = Map::new();
    let mut fixed_rows = vec![csv_line(
        &[
            "preset",
            "design_mode",
            "declared_mtow_kg",
            "design_gross_mass_kg",
            "design_landing_mass_kg",
            "landing_mass_source",
            "requirements_num_passengers",
            "seated_pax",
            "flops_first",
            "flops_business",
            "flops_tourist",
            "flight_attendants",
            "wing_kg",
            "h_stab_kg",
            "v_stab_kg",
            "fuselage_kg",
            "gear_kg",
            "propulsion_kg",
            "systems_kg",
            "furnishings_kg",
            "oew_kg",
            "reference_oew_kg",
            "oew_residual_kg",
            "oew_residual_pct",
            "reference_applicability",
            "reference_tier",
            "counts_toward_validation",
            "visible_anchor_kg",
            "visible_comparison_case",
            "declared_baseline_engine_mass_kg",
            "payload_kg",
            "actual_zfw_kg",
            "reference_mzfw_kg",
            "mzfw_margin_kg",
            "signed_fuel_closure_kg",
            "usable_fuel_capacity_kg",
            "status",
        ]
        .map(String::from),
    )];
    let mut mission_rows = vec![csv_line(
        &[
            "preset",
            "label",
            "design_mode",
            "mtow_sizing",
            "range_nmi_requested",
            "design_range_m",
            "status",
            "dispatch_status",
            "declared_mtow_kg",
            "sizing_basis",
            "design_gross_mass_kg",
            "design_landing_mass_kg",
            "takeoff_mass_kg",
            "oew_kg",
            "zero_fuel_mass_kg",
            "payload_kg",
            "carried_passengers",
            "wing_kg",
            "fuselage_kg",
            "gear_kg",
            "propulsion_kg",
            "systems_kg",
            "furnishings_kg",
            "block_fuel_kg",
            "takeoff_fuel_kg",
            "trip_fuel_kg",
            "contingency_fuel_kg",
            "alternate_fuel_kg",
            "final_reserve_fuel_kg",
            "destination_landing_mass_kg",
            "landing_mass_limit_kg",
            "usable_capacity_kg",
            "lift_to_drag",
            "sizing_iterations",
            "hard_feasible",
            "violated_hard_ids",
        ]
        .map(String::from),
    )];
    let mut sweep_rows = vec![csv_line(
        &[
            "preset",
            "range_nmi",
            "status",
            "takeoff_mass_kg",
            "oew_kg",
            "wing_kg",
            "fuselage_kg",
            "gear_kg",
            "propulsion_kg",
            "systems_kg",
            "furnishings_kg",
            "block_fuel_kg",
        ]
        .map(String::from),
    )];
    let mut failure_rows = vec![csv_line(&["preset", "case", "reason"].map(String::from))];
    let mut climb_rows = vec![csv_line(
        &[
            "preset",
            "variant",
            "status",
            "takeoff_mass_kg",
            "zero_fuel_mass_kg",
            "block_fuel_kg",
            "speed_reference",
            "step_climb_1_air_speed_m_s",
            "thrust_kn_per_engine",
            "dispatch_status",
            "note",
        ]
        .map(String::from),
    )];

    for preset in presets::registry() {
        if let Some(list) = &selected {
            if !list.iter().any(|name| name == preset.name) {
                continue;
            }
        }
        let name = preset.name;
        println!("== {name}");
        let design = preset.design_vector;
        let fixed = fixed_design_weight_case(name, &design, DesignMode::BaselineSandbox);
        let fixed_clean_sheet_mode =
            fixed_design_weight_case(name, &design, DesignMode::CleanSheet);
        let declared_range = fixed
            .pointer("/buildup/design_range_nmi")
            .and_then(Value::as_f64);
        let b = &fixed;
        let get = |ptr: &str| b.pointer(ptr).and_then(Value::as_f64);
        let gets = |ptr: &str| {
            b.pointer(ptr)
                .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
                .unwrap_or_default()
        };
        fixed_rows.push(csv_line(&[
            name.to_owned(),
            gets("/design_mode"),
            opt(get("/declared_mtow_kg")),
            opt(get("/buildup/design_gross_mass_kg")),
            opt(get("/buildup/design_landing_mass_kg")),
            gets("/buildup/sources/landing_mass"),
            gets("/requirements_num_passengers"),
            gets("/layout/seated_pax"),
            gets("/buildup/class_split/0"),
            gets("/buildup/class_split/1"),
            gets("/buildup/class_split/2"),
            gets("/buildup/flight_attendants"),
            opt(get("/buildup/masses_kg/Wing")),
            opt(get("/buildup/masses_kg/H-Stab")),
            opt(get("/buildup/masses_kg/V-Stab")),
            opt(get("/buildup/masses_kg/Fuselage")),
            opt(get("/buildup/masses_kg/Gear")),
            opt(get("/buildup/masses_kg/Propulsion")),
            opt(get("/buildup/masses_kg/Systems")),
            opt(get("/buildup/masses_kg/Furnishings")),
            opt(get("/oew_kg")),
            opt(get("/reference_oew_kg")),
            opt(get("/oew_residual_kg")),
            opt(get("/oew_reference/oew_residual_pct")),
            gets("/oew_reference/applicability"),
            gets("/oew_reference/visible_tier"),
            gets("/oew_reference/counts_toward_validation"),
            opt(get("/oew_reference/visible_value_kg")),
            gets("/oew_reference/visible_comparison_case"),
            opt(get("/declared_baseline_engine_mass_kg")),
            opt(get("/buildup/masses_kg/Payload")),
            opt(get("/actual_zfw_kg")),
            opt(get("/reference_mzfw_kg")),
            opt(get("/mzfw_margin_kg")),
            opt(get("/signed_fuel_closure_kg")),
            opt(get("/usable_fuel_capacity_kg")),
            gets("/status"),
        ]));
        if fixed.get("status").and_then(Value::as_str) != Some("ok") {
            failure_rows.push(csv_line(&[
                name.to_owned(),
                "A_fixed_design_weight".to_owned(),
                fixed
                    .get("reason")
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            ]));
        }
        let mut entry = Map::new();
        entry.insert("fixed_design_weight".to_owned(), fixed.clone());
        entry.insert(
            "fixed_design_weight_clean_sheet_mode".to_owned(),
            fixed_clean_sheet_mode,
        );
        entry.insert(
            "reference".to_owned(),
            json!({
                "mtow_kg": preset.reference.mtow_kg,
                "mlw_kg": preset.reference.mlw_kg,
                "mzfw_kg": preset.reference.mzfw_kg,
                "oew_kg": preset.reference.oew_kg,
                "usable_fuel_mass_kg": preset.reference.usable_fuel_mass_kg,
                "identity": {
                    "model": preset.identity.model,
                    "weight_variant": preset.identity.weight_variant,
                    "engine_model": preset.identity.engine_model,
                    "modification_state": preset.identity.modification_state,
                },
            }),
        );

        if missions && fixed.get("status").and_then(Value::as_str) == Some("ok") {
            let mut cases = Vec::new();
            for case in &mission_cases {
                println!("   {}", case.label);
                let result = mission_case(name, &design, case, declared_range);
                let m = &result;
                let g = |ptr: &str| m.pointer(ptr).and_then(Value::as_f64);
                let gs = |ptr: &str| {
                    m.pointer(ptr)
                        .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
                        .unwrap_or_default()
                };
                mission_rows.push(csv_line(&[
                    name.to_owned(),
                    case.label.to_owned(),
                    gs("/design_mode"),
                    gs("/mtow_sizing"),
                    opt(g("/range_nmi_requested")),
                    opt(g("/design_range_m")),
                    gs("/status"),
                    gs("/dispatch_status"),
                    opt(g("/declared_mtow_kg")),
                    gs("/sizing_basis"),
                    opt(g("/design_gross_mass_kg")),
                    opt(g("/design_landing_mass_kg")),
                    opt(g("/takeoff_mass_kg")),
                    opt(g("/oew_kg")),
                    opt(g("/zero_fuel_mass_kg")),
                    opt(g("/payload_kg")),
                    gs("/carried_passengers"),
                    opt(g("/masses_kg/Wing")),
                    opt(g("/masses_kg/Fuselage")),
                    opt(g("/masses_kg/Gear")),
                    opt(g("/masses_kg/Propulsion")),
                    opt(g("/masses_kg/Systems")),
                    opt(g("/masses_kg/Furnishings")),
                    opt(g("/block_fuel_kg")),
                    opt(g("/takeoff_fuel_kg")),
                    opt(g("/trip_fuel_kg")),
                    opt(g("/contingency_fuel_kg")),
                    opt(g("/alternate_fuel_kg")),
                    opt(g("/final_reserve_fuel_kg")),
                    opt(g("/destination_landing_mass_kg")),
                    opt(g("/landing_mass_limit_kg")),
                    opt(g("/usable_capacity_kg")),
                    opt(g("/lift_to_drag")),
                    gs("/sizing_iterations"),
                    gs("/hard_feasible"),
                    m.pointer("/violated_hard_ids")
                        .and_then(Value::as_array)
                        .map(|ids| {
                            ids.iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join("+")
                        })
                        .unwrap_or_default(),
                ]));
                let status = gs("/status");
                if status != "converged" {
                    failure_rows.push(csv_line(&[
                        name.to_owned(),
                        case.label.to_owned(),
                        if status == "error" {
                            gs("/reason")
                        } else {
                            format!("{status}: {}", gs("/dispatch_status"))
                        },
                    ]));
                }
                cases.push(result);
            }
            entry.insert("mission_cases".to_owned(), Value::Array(cases));

            // Fixed-aircraft mission sweep: structural mass must not move.
            let mut sweep = Vec::new();
            let mut ranges: Vec<f64> = SWEEP_RANGES_NMI.to_vec();
            if let Some(range) = declared_range {
                ranges.push(range);
            }
            for range in ranges {
                let case = MissionCase {
                    label: "B_sweep",
                    design_mode: DesignMode::BaselineSandbox,
                    mtow_sizing: MtowSizing::SizedByMission,
                    range_nmi: Some(range),
                };
                println!("   sweep {range:.0} nmi");
                let result = mission_case(name, &design, &case, declared_range);
                let g = |ptr: &str| result.pointer(ptr).and_then(Value::as_f64);
                sweep_rows.push(csv_line(&[
                    name.to_owned(),
                    fmt(range),
                    result
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    opt(g("/takeoff_mass_kg")),
                    opt(g("/oew_kg")),
                    opt(g("/masses_kg/Wing")),
                    opt(g("/masses_kg/Fuselage")),
                    opt(g("/masses_kg/Gear")),
                    opt(g("/masses_kg/Propulsion")),
                    opt(g("/masses_kg/Systems")),
                    opt(g("/masses_kg/Furnishings")),
                    opt(g("/block_fuel_kg")),
                ]));
                sweep.push(result);
            }
            entry.insert("mission_sweep".to_owned(), Value::Array(sweep));
        }
        if name == "A320-200" {
            entry.insert("a320_sensitivities".to_owned(), a320_sensitivities(&fixed));
        }
        if missions && matches!(name, "A320-200" | "A220-300") {
            let (rows, variants) = rows::climb_section(name, &design);
            climb_rows.extend(rows);
            entry.insert("climb_diagnosis".to_owned(), Value::Array(variants));
        }
        presets_json.insert(name.to_owned(), Value::Object(entry));
    }

    let (reconstructed_rows, reconstructed) = rows::reconstructed_section(selected.as_deref());
    write(
        &out_dir.join("reconstructed-cases.csv"),
        &(reconstructed_rows.join("\n") + "\n"),
    )?;

    let raw = json!({
        "schema_version": 1,
        "generated_unix_s": SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        "command": "cargo run -p alas-pipeline --release --example mass_experiment_matrix",
        "run_options": { "engine_fallback": engine_fallback },
        "sweep_ranges_nmi": SWEEP_RANGES_NMI,
        "presets": presets_json,
        "reconstructed_cases": reconstructed,
    });
    write(
        &out_dir.join("raw.json"),
        &serde_json::to_string_pretty(&raw)?,
    )?;
    write(
        &out_dir.join("fixed-design-weight.csv"),
        &(fixed_rows.join("\n") + "\n"),
    )?;
    write(
        &out_dir.join("mission-cases.csv"),
        &(mission_rows.join("\n") + "\n"),
    )?;
    write(
        &out_dir.join("mission-sweep.csv"),
        &(sweep_rows.join("\n") + "\n"),
    )?;
    write(
        &out_dir.join("failures.csv"),
        &(failure_rows.join("\n") + "\n"),
    )?;
    write(
        &out_dir.join("climb-diagnosis.csv"),
        &(climb_rows.join("\n") + "\n"),
    )?;
    println!("wrote {}", out_dir.display());
    Ok(())
}
