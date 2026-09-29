// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The engine editor: model summary and the designer form for the selected engine.

use alas_config::{ActiveEngineModel, EngineConfig};
use egui::{RichText, Ui};

use crate::views::form::{dynamic_form, FormEdit};
use crate::views::tr;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum EngineEditorModel {
    Turbofan {
        rated_thrust_kn: f64,
        bypass_ratio: f64,
        takeoff_bypass_ratio: f64,
        overall_pressure_ratio: f64,
        fan_pressure_ratio: f64,
        turbine_inlet_temp_k: f64,
        cruise_tsfc: f64,
        takeoff_fuel_flow_kg_s: f64,
        off_design_evidence: String,
        provenance: String,
    },
    Turboprop {
        takeoff_kw: f64,
        reserve_kw: f64,
        continuous_kw: f64,
        climb_kw: f64,
        cruise_kw: f64,
        cruise_fuel_flow_kg_h: f64,
        propeller_model: String,
        diameter_m: f64,
        governed_rpm: f64,
        reduction_ratio: f64,
        provenance: String,
    },
}

pub(super) fn engine_editor_model(engine: &EngineConfig) -> Result<EngineEditorModel, String> {
    match engine.active_model().map_err(|error| error.to_string())? {
        ActiveEngineModel::Turbofan(spec) => Ok(EngineEditorModel::Turbofan {
            rated_thrust_kn: spec.rated_thrust_kn,
            bypass_ratio: spec.bypass_ratio,
            takeoff_bypass_ratio: spec.takeoff_bypass_ratio.unwrap_or(spec.bypass_ratio),
            overall_pressure_ratio: spec.overall_pressure_ratio,
            fan_pressure_ratio: spec.fan_pressure_ratio,
            turbine_inlet_temp_k: spec.turbine_inlet_temp_k,
            cruise_tsfc: spec.cruise_tsfc_kg_kgf_hr,
            takeoff_fuel_flow_kg_s: spec.takeoff_fuel_flow_kg_s,
            off_design_evidence: format!(
                "{}: {}",
                spec.off_design.evidence, spec.off_design.source
            ),
            provenance: spec.part_power_source.clone(),
        }),
        ActiveEngineModel::Turboprop(spec) => Ok(EngineEditorModel::Turboprop {
            takeoff_kw: spec.takeoff_shaft_power_kw,
            reserve_kw: spec.maximum_reserve_shaft_power_kw,
            continuous_kw: spec.maximum_continuous_shaft_power_kw,
            climb_kw: spec.maximum_climb_shaft_power_kw,
            cruise_kw: spec.maximum_cruise_shaft_power_kw,
            cruise_fuel_flow_kg_h: spec.maximum_cruise_fuel_flow_kg_h,
            propeller_model: spec.propeller_model.clone(),
            diameter_m: spec.propeller_diameter_m,
            governed_rpm: spec.governed_propeller_speed_rpm,
            reduction_ratio: spec.reduction_ratio,
            provenance: format!("{}; {}", spec.rating_source, spec.geometry_source),
        }),
        // PropulsionTechnology is non-exhaustive: a future technology must
        // receive a deliberate editor instead of silently inheriting a gas-
        // turbine form.
        #[allow(unreachable_patterns)]
        _ => Err("selected propulsion technology is not supported by this editor".to_owned()),
    }
}

pub(super) fn render_engine_physics_summary(ui: &mut Ui, model: &EngineEditorModel) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        match model {
        EngineEditorModel::Turbofan {
            rated_thrust_kn,
            bypass_ratio,
            takeoff_bypass_ratio,
            overall_pressure_ratio,
            fan_pressure_ratio,
            turbine_inlet_temp_k,
            cruise_tsfc,
            takeoff_fuel_flow_kg_s,
            off_design_evidence,
            provenance,
        } => {
            ui.label(RichText::new(tr("Turbofan rating & cycle anchors")).strong())
                .on_hover_text(format!(
                    "{}: {off_design_evidence}\n\n{}: {provenance}",
                    tr("Off-design thrust"),
                    tr("Provenance")
                ));
            readonly_metrics(
                ui,
                &[
                    ("Rated thrust per engine", *rated_thrust_kn, "kN"),
                    ("Bypass ratio", *bypass_ratio, "-"),
                    ("ICAO take-off bypass ratio", *takeoff_bypass_ratio, "-"),
                    ("Overall pressure ratio", *overall_pressure_ratio, "-"),
                    ("Fan pressure ratio", *fan_pressure_ratio, "-"),
                    ("Turbine inlet temperature", *turbine_inlet_temp_k, "K"),
                    ("Cruise TSFC reference", *cruise_tsfc, "kg/(kgf.hr)"),
                    (
                        "ICAO take-off fuel flow per engine",
                        *takeoff_fuel_flow_kg_s,
                        "kg/s",
                    ),
                ],
            );
        }
        EngineEditorModel::Turboprop {
            takeoff_kw,
            reserve_kw,
            continuous_kw,
            climb_kw,
            cruise_kw,
            cruise_fuel_flow_kg_h,
            propeller_model,
            diameter_m,
            governed_rpm,
            reduction_ratio,
            provenance,
        } => {
            ui.label(RichText::new(tr("Turboprop shaft-power system")).strong())
                .on_hover_text(format!(
                    "{}: {provenance}\n\n{}",
                    tr("Provenance"),
                    tr("Limitation: conceptual Level-1 propeller performance; no proprietary PW127M/568F engine deck or propeller map is claimed.")
                ));
            readonly_metrics(
                ui,
                &[
                    ("Take-off shaft power per engine", *takeoff_kw, "kW"),
                    ("Maximum reserve / OEI power", *reserve_kw, "kW"),
                    ("Maximum continuous power", *continuous_kw, "kW"),
                    ("Maximum climb power", *climb_kw, "kW"),
                    ("Maximum cruise power", *cruise_kw, "kW"),
                    (
                        "Two-engine maximum-cruise fuel flow",
                        *cruise_fuel_flow_kg_h,
                        "kg/h",
                    ),
                    ("Propeller diameter", *diameter_m, "m"),
                    ("Governed propeller speed", *governed_rpm, "rpm"),
                    ("Reduction ratio", *reduction_ratio, "-"),
                ],
            );
            ui.label(format!("{}: {propeller_model}", tr("Propeller model")));
        }
        }
    });
}

fn readonly_metrics(ui: &mut Ui, metrics: &[(&str, f64, &str)]) {
    // Each column lays out independently, so a taller metric in one column
    // does not leave an empty row beside it. Narrow windows use one column.
    let columns = ((ui.available_width() / 380.0).floor() as usize)
        .clamp(1, 3)
        .min(metrics.len());
    let rows_per_column = metrics.len().div_ceil(columns);
    ui.columns(columns, |column_uis| {
        for (index, &(label, value, unit)) in metrics.iter().enumerate() {
            column_uis[index / rows_per_column].label(format!("{}: {value:.3} {unit}", tr(label)));
        }
    });
}

/// The model schema remains the single source of field metadata; this only
/// supplies visual hierarchy for a form with no nested configuration nodes.
pub(super) fn render_engine_designer_form(
    ui: &mut Ui,
    fields: &[alas_config::Field],
    values: &mut serde_json::Value,
    error_fields: &std::collections::HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
) -> Vec<FormEdit> {
    const GROUPS: [(&str, &[&str], bool); 5] = [
        (
            "Cycle design inputs",
            &[
                "turboprop_overall_pressure_ratio",
                "turboprop_turbine_inlet_temperature_k",
            ],
            true,
        ),
        (
            "Intake & compressors",
            &[
                "inlet_pressure_recovery",
                "lpc_pressure_ratio_split",
                "fan_polytropic_efficiency",
                "lpc_polytropic_efficiency",
                "hpc_polytropic_efficiency",
            ],
            true,
        ),
        (
            "Combustor",
            &["combustor_pressure_ratio", "combustor_efficiency"],
            true,
        ),
        (
            "Turbines",
            &[
                "hpt_polytropic_efficiency",
                "lpt_polytropic_efficiency",
                "turbine_mechanical_efficiency",
            ],
            true,
        ),
        (
            "Nozzles",
            &[
                "core_nozzle_pressure_ratio",
                "fan_nozzle_pressure_ratio",
                "core_nozzle_efficiency",
                "fan_nozzle_efficiency",
            ],
            true,
        ),
    ];

    let mut edits = Vec::new();
    for (title, names, default_open) in GROUPS {
        let section: Vec<alas_config::Field> = fields
            .iter()
            .filter(|field| names.contains(&field.name))
            .cloned()
            .collect();
        if section.is_empty() {
            continue;
        }
        crate::theme::card_frame(ui).show(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new(tr(title)).strong())
                .id_salt(format!("propulsion_cycle::{title}"))
                .default_open(default_open)
                .show(ui, |ui| {
                    edits.extend(dynamic_form(
                        ui,
                        &section,
                        values,
                        error_fields,
                        lang,
                        show_help,
                    ));
                });
        });
        ui.add_space(6.0);
    }

    let remaining: Vec<alas_config::Field> = fields
        .iter()
        .filter(|field| {
            !GROUPS
                .iter()
                .any(|(_, names, _)| names.contains(&field.name))
        })
        .cloned()
        .collect();
    if !remaining.is_empty() {
        crate::theme::card_frame(ui).show(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new(tr("Other parameters")).strong())
                .id_salt("propulsion_cycle::other")
                .default_open(true)
                .show(ui, |ui| {
                    edits.extend(dynamic_form(
                        ui,
                        &remaining,
                        values,
                        error_fields,
                        lang,
                        show_help,
                    ));
                });
        });
    }
    edits
}
