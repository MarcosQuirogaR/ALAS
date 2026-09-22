// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reference-conditioned FLOPS input cases through the product mass path.
//!
//! This is a deliberately small, mass-focused companion to
//! `mass_experiment_matrix`.  It changes only inputs that are stated by the
//! retained aircraft references (cabin count, MTOW/MLW, or fuel capacity),
//! evaluates the resolved configuration with `FullAnalysis`, and writes the
//! resulting eight-component OEW.  Reference values are evidence labels, not
//! calibration constants; the JSON keeps their definition and applicability
//! beside the numerical comparison.
//!
//! ```text
//! cargo run -p alas-pipeline --release --example flops_robustness -- \
//!     .agent/data/flops-robustness-20260920/reference-conditioned-cases.json
//! ```
#![allow(clippy::print_stdout)]

use std::error::Error;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use alas_config::{presets, AlasConfig, DesignMode};
use alas_mass::breakdown::OEW_KEYS;
use alas_pipeline::feasibility::{assess_physical_feasibility, takeoff_mass_properties};
use alas_pipeline::FullAnalysis;
use serde_json::{json, Value};

fn declare_count_cabin(
    config: &mut AlasConfig,
    first: (i64, f64, f64),
    business: (i64, f64, f64),
    economy: (i64, f64, f64),
) {
    // Count-mode rows are explicit source-conditioned installations.  The
    // named registered preset may otherwise re-materialize its own percent
    // mix during payload construction and overwrite these counts.
    config.requirements.cabin_preset = "Custom".to_owned();
    let cabin = &mut config.cabin.passenger;
    cabin.class_mix_mode = "count".to_owned();
    cabin.first.count = first.0;
    cabin.first.pitch_m = first.1;
    cabin.first.width_m = first.2;
    cabin.business.count = business.0;
    cabin.business.pitch_m = business.1;
    cabin.business.width_m = business.2;
    cabin.premium.count = 0;
    cabin.economy.count = economy.0;
    cabin.economy.pitch_m = economy.1;
    cabin.economy.width_m = economy.2;
    config.requirements.num_passengers = first.0 + business.0 + economy.0;
}

fn oew_kg(buildup: &alas_mass::breakdown::FlopsMassBuildup) -> f64 {
    OEW_KEYS
        .iter()
        .filter_map(|name| buildup.masses.get(name))
        .sum()
}

fn placed_passenger_counts(
    report: &alas_pipeline::AnalysisReport,
) -> Option<([i64; 3], i64, i64, i64)> {
    let layout = report.payload_layout.as_ref()?;
    let alas_payload::layout::LayoutSummary::Passenger(summary) = &layout.summary else {
        return None;
    };
    let mut counts = [0_i64; 3];
    for &(name, count) in &summary.classes {
        match name {
            "First" => counts[0] += count,
            "Business" => counts[1] += count,
            // Product payload folds any legacy Premium slot into tourist.
            "Economy" | "Premium" => counts[2] += count,
            _ => {}
        }
    }
    Some((
        counts,
        summary.total_pax,
        summary.seated_pax,
        summary.unseated_pax,
    ))
}

fn source_case(label: &str) -> (&'static str, Option<f64>, &'static str, &'static str) {
    match label {
        "A220_140Y" => (
            "A220-300",
            Some(37_149.0),
            "primary_planning_oew",
            "Airbus A220 ARP planning OEW 37,149 kg; ACP 140-seat OWE 37,081 kg is retained as an alternate",
        ),
        "A320_FHDRF_77t_180Y" => (
            "A320-200",
            Some(41_052.0),
            "operator_empty_weight_secondary",
            "F-HDRF operator sheet: 77,000 kg MTOW, 180Y, wingtip fences, empty 41,052 kg; inclusion list unstated",
        ),
        "A340_typical_335" => (
            "A340-300",
            Some(131_215.0),
            "primary_jacking_figure_oew",
            "Airbus ACAP Rev 33 A340-300 jacking figure labelled OEW; weight variant and inclusion list unstated",
        ),
        "A380_typical_555" => (
            "A380-800",
            Some(277_000.0),
            "secondary_aggregator_typical",
            "Secondary typical three-class estimate for approximately 500-555 seats; no primary numeric OEW",
        ),
        "B787_typical_290" => (
            "B787-9",
            Some(128_850.0),
            "secondary_superseded_attribution",
            "Older Rev L attribution for a typical two-class 290-seat 787-9; current Rev Q gives no numeric OEW",
        ),
        "ATR72_2020" => (
            "ATR72-600",
            Some(13_450.0),
            "primary_operational_empty",
            "ATR 2020 factsheet typical in-service operational empty weight for PW127M/N generation; inclusion list unstated",
        ),
        "DC10_255_572k" => (
            "DC-10",
            Some(120_914.0),
            "primary_acap_operating_empty",
            "Douglas ACAP Series 30 passenger OWE with 572,000 lb footnote, 255-seat standard cabin",
        ),
        "AVE_7779_reference" => (
            "AVE",
            None,
            "no_numeric_primary",
            "User-selected Boeing 777-9 planning identity; current Boeing Rev G defines OEW but publishes no numeric value",
        ),
        _ => ("", None, "unknown", "unknown case"),
    }
}

fn apply_case(label: &str, config: &mut AlasConfig) {
    match label {
        "AVE_7779_reference" => {
            // The source-conditioned run uses Boeing's current 777-9
            // planning case.  AVE's geometry and conceptual installation are
            // retained so the residual is visible rather than silently
            // turning the notional aircraft into a 777-9 clone.
            declare_count_cabin(
                config,
                (0, 0.9144, 0.55),
                (42, 1.55, 0.70),
                (384, 0.81, 0.46),
            );
            config.requirements.mtow_kg = 351_534.0;
        }
        "A220_140Y" => declare_count_cabin(
            config,
            (0, 0.9144, 0.55),
            (0, 0.9144, 0.55),
            (140, 0.8128, 0.47),
        ),
        "A320_FHDRF_77t_180Y" => {
            declare_count_cabin(
                config,
                (0, 0.9144, 0.55),
                (0, 0.9144, 0.55),
                (180, 0.7874, 0.46),
            );
            config.requirements.mtow_kg = 77_000.0;
            config.mass_model.flops_structure.design_landing_mass_kg = Some(64_500.0);
            config.mass_model.flops_transport.maximum_fuel_capacity_kg = Some(19_476.0);
        }
        "A340_typical_335" => {
            declare_count_cabin(config, (30, 2.0, 0.95), (0, 1.55, 0.70), (305, 0.81, 0.46));
            config.mass_model.flops_transport.flight_attendant_count = Some(9);
        }
        "A380_typical_555" => {
            declare_count_cabin(config, (22, 2.0, 0.95), (96, 1.55, 0.70), (437, 0.81, 0.46))
        }
        "B787_typical_290" => {
            declare_count_cabin(config, (0, 2.0, 0.95), (28, 1.55, 0.70), (262, 0.81, 0.46))
        }
        "DC10_255_572k" => {
            declare_count_cabin(
                config,
                (0, 0.9144, 0.55),
                (0, 0.9144, 0.55),
                (255, 0.7874, 0.46),
            );
            config.requirements.mtow_kg = 259_454.0;
            config.mass_model.flops_structure.design_landing_mass_kg = Some(190_962.0);
        }
        "ATR72_2020" => {
            // ATR's factsheet is a nominal 72-seat case.  Preserve the
            // preset's actual geometry and report any seats that cannot be
            // physically placed in `evaluated_cabin` below.
            declare_count_cabin(
                config,
                (0, 0.7874, 0.46),
                (0, 0.7874, 0.46),
                (72, 0.7874, 0.46),
            );
        }
        _ => {}
    }
}

fn configure_mass_focused_analysis(config: &mut AlasConfig) {
    // FullAnalysis owns the public mass/report path but also computes a VLM
    // polar.  Keep that non-mass work bounded for this matrix; no mass input
    // depends on these mesh settings, and the output explicitly says that
    // the aerodynamic result is not a validation target here.
    config.analysis.sweep_n_points = 3;
    config.analysis.chordwise_resolution = 3;
    config.analysis.fine_chordwise_resolution = 8;
    config.analysis.spanwise_resolution = 1;
    config.analysis.fine_spanwise_resolution = 1;
}

fn run_case(label: &str, source_conditioned: bool) -> Value {
    let (preset_name, reference_kg, reference_kind, reference_note) = source_case(label);
    let Ok(preset) = presets::get(preset_name) else {
        return json!({"label": label, "status": "preset_error"});
    };
    let mut config = match AlasConfig::from_value(&json!({"preset": preset_name})) {
        Ok(config) => config,
        Err(error) => {
            return json!({"label": label, "status": "config_error", "reason": error.to_string()})
        }
    };
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    if source_conditioned {
        apply_case(label, &mut config);
    }
    configure_mass_focused_analysis(&mut config);
    // Engine geometry stays in this path because nacelle and installation
    // inputs are part of the mass evidence.  This remains a mass-focused
    // runner: it does not fly a mission or run an optimizer.
    let report = match FullAnalysis::new(config.clone()).run(&preset.design_vector, true) {
        Ok(report) => report,
        Err(error) => {
            return json!({"label": label, "preset": preset_name, "status": "failed", "reason": error})
        }
    };
    let Some(buildup) = report.flops_mass_buildup.as_deref() else {
        return json!({"label": label, "preset": preset_name, "status": "no_flops_buildup"});
    };
    let predicted_kg = oew_kg(buildup);
    let residual = reference_kg.map(|reference| predicted_kg - reference);
    let evaluated_cabin = report
        .payload_layout
        .as_ref()
        .and_then(|layout| match &layout.summary {
            alas_payload::layout::LayoutSummary::Passenger(summary) => Some(json!({
                "seated_pax": summary.seated_pax,
                "unseated_pax": summary.unseated_pax,
                "total_pax": summary.total_pax,
                "classes": summary.classes,
            })),
            alas_payload::layout::LayoutSummary::Cargo(_) => None,
        });
    let installed_counts = [
        buildup.inputs.first_class_passenger_count as i64,
        buildup.inputs.business_class_passenger_count as i64,
        buildup.inputs.tourist_class_passenger_count as i64,
    ];
    let placed = placed_passenger_counts(&report);
    let count_consistency = placed.map(|(placed_counts, total, seated, unseated)| {
        let per_class_within = placed_counts
            .iter()
            .zip(installed_counts)
            .all(|(placed, installed)| *placed <= installed);
        let installed_total: i64 = installed_counts.iter().sum();
        let within_declared_installation = per_class_within && seated <= installed_total;
        let requested_total_matches_installed = total == installed_total;
        let shortfall_accounting_consistent = unseated == (total - seated).max(0);
        let full_cabin_physical_match = within_declared_installation
            && placed_counts == installed_counts
            && requested_total_matches_installed
            && shortfall_accounting_consistent
            && unseated == 0;
        json!({
            "installed_flops": {"first": installed_counts[0], "business": installed_counts[1], "tourist": installed_counts[2], "total": installed_total},
            "placed_layout": {"first": placed_counts[0], "business": placed_counts[1], "tourist": placed_counts[2], "total_requested": total, "seated": seated, "unseated": unseated},
            "within_declared_installation": within_declared_installation,
            "requested_total_matches_installed": requested_total_matches_installed,
            "shortfall_accounting_consistent": shortfall_accounting_consistent,
            "full_cabin_physical_match": full_cabin_physical_match,
            "shortfall_allowed": within_declared_installation && requested_total_matches_installed && shortfall_accounting_consistent && !full_cabin_physical_match,
        })
    });
    let takeoff = takeoff_mass_properties(&config, &report);
    let takeoff_json = takeoff.map(|properties| {
        json!({
            "mass_kg": properties.mass_kg,
            "cg_m": properties.cg_m,
            "inertia_cg_kg_m2": {
                "ixx": properties.inertia_cg.ixx,
                "iyy": properties.inertia_cg.iyy,
                "izz": properties.inertia_cg.izz,
                "pxy": properties.inertia_cg.pxy,
                "pxz": properties.inertia_cg.pxz,
                "pyz": properties.inertia_cg.pyz,
            },
            "finite": properties.mass_kg.is_finite()
                && properties.cg_m.iter().all(|value| value.is_finite())
                && properties.inertia_cg.is_finite(),
            "inertia_physical": properties.inertia_cg.is_physical(),
        })
    });
    let fallback_feasibility = if takeoff_json.is_none() {
        let feasibility =
            assess_physical_feasibility(&config, &preset.design_vector, &report, None);
        Some(json!({
            "mass_balance_available": feasibility.mass_balance.is_some(),
            "is_feasible_all_checks": feasibility.is_feasible(),
            "findings": feasibility.findings.iter().map(|finding| json!({
                "code": format!("{:?}", finding.code),
                "severity": format!("{:?}", finding.severity),
                "message": finding.message,
                "actual": finding.actual,
                "limit": finding.limit,
                "unit": finding.unit,
            })).collect::<Vec<_>>(),
        }))
    } else {
        None
    };
    let ledger_available = takeoff_json.is_some()
        || fallback_feasibility
            .as_ref()
            .is_some_and(|value| value["mass_balance_available"] == true);
    let remaining_geometry_mismatches = if source_conditioned && label == "A320_FHDRF_77t_180Y" {
        vec!["the F-HDRF 34.10 m wingtip-fence span is not applied to the registered Sharklet geometry"]
    } else if source_conditioned && label == "AVE_7779_reference" {
        vec!["AVE geometry, fuel capacity, nacelle/engine installation and other notional inputs remain different from Boeing 777-9 Rev G"]
    } else {
        Vec::new()
    };
    json!({
        "label": label,
        "preset": preset_name,
        "status": "ok",
        "run_kind": if source_conditioned { "source_conditioned_reference_case" } else { "registered_product_baseline" },
        "evaluation": "FullAnalysis::run(include_engines=true), BaselineSandbox, mass-focused report path",
        "aerodynamic_fidelity": {
            "sweep_n_points": config.analysis.sweep_n_points,
            "chordwise_resolution": config.analysis.chordwise_resolution,
            "fine_chordwise_resolution": config.analysis.fine_chordwise_resolution,
            "note": "Reduced only to bound non-mass VLM work; this artifact validates mass/ledger finiteness, not aerodynamic fidelity.",
        },
        "declared_inputs": {
            "mtow_kg": config.requirements.mtow_kg,
            "mlw_kg": config.mass_model.flops_structure.design_landing_mass_kg,
            "passenger_target": config.requirements.num_passengers,
            "cabin_preset": config.requirements.cabin_preset,
            "cabin_mode": config.cabin.passenger.class_mix_mode,
        },
        "evaluated_cabin": evaluated_cabin,
        "evaluated_flops_passenger_counts": {
            "first": buildup.inputs.first_class_passenger_count,
            "business": buildup.inputs.business_class_passenger_count,
            "tourist": buildup.inputs.tourist_class_passenger_count,
            "total": buildup.inputs.passenger_count(),
        },
        "passenger_count_consistency": count_consistency,
        "predicted_oew_kg": predicted_kg,
        "reference_oew_kg": reference_kg,
        "reference_kind": reference_kind,
        "reference_note": reference_note,
        "diagnostic_residual_kg": residual,
        "diagnostic_residual_pct_of_reference": residual.zip(reference_kg).map(|(delta, reference)| 100.0 * delta / reference),
        "remaining_geometry_mismatches": remaining_geometry_mismatches,
        "takeoff_mass_properties": takeoff_json,
        "ledger_available": ledger_available,
        "fallback_feasibility": fallback_feasibility,
        "masses_kg": buildup.masses.as_pairs().into_iter().collect::<std::collections::BTreeMap<_, _>>(),
        "validation_eligible": false,
    })
}

fn write(path: &Path, value: &Value) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(".agent/data/flops-robustness-20260920/reference-conditioned-cases.json")
        });
    let labels = [
        "AVE_7779_reference",
        "A340_typical_335",
        "A380_typical_555",
        "B787_typical_290",
        "A320_FHDRF_77t_180Y",
        "A220_140Y",
        "ATR72_2020",
        "DC10_255_572k",
    ];
    let source_conditioned_cases: Vec<Value> = labels
        .iter()
        .map(|label| {
            println!("running source-conditioned {label}");
            let _ = io::stdout().flush();
            run_case(label, true)
        })
        .collect();
    let product_baseline_cases: Vec<Value> = labels
        .iter()
        .map(|label| {
            println!("running product baseline {label}");
            let _ = io::stdout().flush();
            run_case(label, false)
        })
        .collect();
    let mut cases = source_conditioned_cases.clone();
    cases.extend(product_baseline_cases.clone());
    let artifact = json!({
        "schema_version": 1,
        "generated_by": "crates/alas-pipeline/examples/flops_robustness.rs",
        "purpose": "Numerical reproduction of explicitly reference-conditioned inputs; comparisons are diagnostic and not calibration or physical validation",
        "include_engines": true,
        "aerodynamic_fidelity": "Reduced VLM settings above; only mass/ledger values are interpreted by this runner.",
        "source_conditioned_cases": source_conditioned_cases,
        "product_baseline_cases": product_baseline_cases,
        "cases": cases,
    });
    write(&output, &artifact)?;
    let quality_failures: Vec<String> = cases
        .iter()
        .filter_map(|case| {
            let label = case["label"].as_str().unwrap_or("<unknown>");
            let run_kind = case["run_kind"].as_str().unwrap_or("<unknown>");
            let status_ok = case["status"].as_str() == Some("ok");
            let ledger_ok = case["ledger_available"].as_bool() == Some(true);
            let finite_ok = case["takeoff_mass_properties"]["finite"].as_bool() == Some(true);
            let inertia_ok = case["takeoff_mass_properties"]["inertia_physical"].as_bool() == Some(true);
            let consistency = &case["passenger_count_consistency"];
            let within_ok = consistency["within_declared_installation"].as_bool() == Some(true);
            let accounting_ok = consistency["requested_total_matches_installed"].as_bool() == Some(true)
                && consistency["shortfall_accounting_consistent"].as_bool() == Some(true);
            let count_mode = case["declared_inputs"]["cabin_mode"].as_str() == Some("count");
            let exact_percent_ok = count_mode
                || consistency["full_cabin_physical_match"].as_bool() == Some(true);
            if status_ok && ledger_ok && finite_ok && inertia_ok && within_ok && accounting_ok && exact_percent_ok {
                None
            } else {
                Some(format!(
                    "{run_kind}/{label}: status_ok={status_ok}, ledger_available={ledger_ok}, takeoff_finite={finite_ok}, inertia_physical={inertia_ok}, within_declared_installation={within_ok}, count_accounting={accounting_ok}, exact_percent_counts={exact_percent_ok}"
                ))
            }
        })
        .collect();
    if !quality_failures.is_empty() {
        return Err(format!(
            "reference-conditioned matrix quality gate failed: {}",
            quality_failures.join("; ")
        )
        .into());
    }
    println!("wrote {}", output.display());
    Ok(())
}
