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
        cruise_tsfc_evidence: String,
        cruise_tsfc_source: String,
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
    let catalogue = alas_config::engines::get(&engine.engine_name).ok();
    match engine.active_model().map_err(|error| error.to_string())? {
        ActiveEngineModel::Turbofan(spec) => Ok(EngineEditorModel::Turbofan {
            rated_thrust_kn: spec.rated_thrust_kn,
            bypass_ratio: spec.bypass_ratio,
            takeoff_bypass_ratio: spec.takeoff_bypass_ratio.unwrap_or(spec.bypass_ratio),
            overall_pressure_ratio: spec.overall_pressure_ratio,
            fan_pressure_ratio: spec.fan_pressure_ratio,
            turbine_inlet_temp_k: spec.turbine_inlet_temp_k,
            cruise_tsfc: spec.cruise_tsfc_kg_kgf_hr,
            // The off-design payload carries the value only; its evidence
            // class and source live on the catalogue entry it was bound to.
            cruise_tsfc_evidence: catalogue
                .map(|e| e.cruise_tsfc_evidence.clone())
                .unwrap_or_default(),
            cruise_tsfc_source: catalogue
                .map(|e| e.cruise_tsfc_source.clone())
                .unwrap_or_default(),
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
        // The summary collapses like its sibling cards.
        let (title, hover) = match model {
            EngineEditorModel::Turbofan {
                off_design_evidence,
                provenance,
                ..
            } => (
                tr("Turbofan rating & cycle anchors"),
                format!(
                    "{}: {off_design_evidence}\n\n{}: {provenance}",
                    tr("Off-design thrust"),
                    tr("Provenance")
                ),
            ),
            EngineEditorModel::Turboprop { provenance, .. } => (
                tr("Turboprop shaft-power system"),
                format!(
                    "{}: {provenance}\n\n{}",
                    tr("Provenance"),
                    tr("Limitation: conceptual Level-1 propeller performance; no proprietary PW127M/568F engine deck or propeller map is claimed.")
                ),
            ),
        };
        let header = egui::CollapsingHeader::new(crate::theme::card_title(ui, title))
            .id_salt("propulsion::engine_summary")
            .default_open(true);
        let header = crate::theme::show_header(ui, header, |ui| {
        match model {
        EngineEditorModel::Turbofan {
            rated_thrust_kn,
            bypass_ratio,
            takeoff_bypass_ratio,
            overall_pressure_ratio,
            fan_pressure_ratio,
            turbine_inlet_temp_k,
            cruise_tsfc,
            cruise_tsfc_evidence,
            cruise_tsfc_source,
            takeoff_fuel_flow_kg_s,
            ..
        } => {
            readonly_metrics(
                ui,
                &[
                    (tr("Rated thrust per engine"), *rated_thrust_kn, "kN"),
                    (tr("Bypass ratio"), *bypass_ratio, "-"),
                    (tr("ICAO take-off bypass ratio"), *takeoff_bypass_ratio, "-"),
                    (tr("Overall pressure ratio"), *overall_pressure_ratio, "-"),
                    (tr("Fan pressure ratio"), *fan_pressure_ratio, "-"),
                    (tr("Turbine inlet temperature"), *turbine_inlet_temp_k, "K"),
                    (tr("Cruise TSFC reference"), *cruise_tsfc, "kg/(kgf.hr)"),
                    (
                        tr("ICAO take-off fuel flow per engine"),
                        *takeoff_fuel_flow_kg_s,
                        "kg/s",
                    ),
                ],
            );
            ui.label(RichText::new(cruise_tsfc_provenance_text(cruise_tsfc_evidence, cruise_tsfc_source)).small().weak());
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
            ..
        } => {
            readonly_metrics(
                ui,
                &[
                    (tr("Take-off shaft power per engine"), *takeoff_kw, "kW"),
                    (tr("Maximum reserve / OEI power"), *reserve_kw, "kW"),
                    (tr("Maximum continuous power"), *continuous_kw, "kW"),
                    (tr("Maximum climb power"), *climb_kw, "kW"),
                    (tr("Maximum cruise power"), *cruise_kw, "kW"),
                    (
                        tr("Two-engine maximum-cruise fuel flow"),
                        *cruise_fuel_flow_kg_h,
                        "kg/h",
                    ),
                    (tr("Propeller diameter"), *diameter_m, "m"),
                    (tr("Governed propeller speed"), *governed_rpm, "rpm"),
                    (tr("Reduction ratio"), *reduction_ratio, "-"),
                ],
            );
            ui.label(format!("{}: {propeller_model}", tr("Propeller model")));
        }
        }
            });
        header.header_response.on_hover_text(hover);
    });
}

/// The evidence class and source behind the catalogue cruise TSFC, so a
/// printed value, a sibling-engine proxy and an unsourced estimate are not
/// shown alike.
fn cruise_tsfc_provenance_text(evidence: &str, source: &str) -> String {
    let evidence = match evidence {
        "published" => tr("Published"),
        "family-proxy" => tr("Family proxy"),
        "unsourced-estimate" => tr("Unsourced estimate (not validated)"),
        "" => tr("Not recorded"),
        other => other.to_owned(),
    };
    let source = if source.is_empty() {
        tr("Not recorded")
    } else {
        plain_source(source)
    };
    crate::views::tr_fields(
        "Cruise TSFC evidence: {evidence}. Source: {source}",
        &[("evidence", evidence), ("source", source)],
    )
}

/// A catalogue source note without internal tracking tags: the leading
/// `NEEDS-DATA(...)` marker goes, and a sentence that only repeats the
/// evidence class ("Unsourced estimate") is not said twice.
fn plain_source(source: &str) -> String {
    let mut text = source.trim();
    // A tracking tag opens with the open datum itself ("sourced ... not on
    // disk."): that is a to-do for the maintainers, not a note for the user.
    if let Some(rest) = text.strip_prefix("NEEDS-DATA(") {
        if let Some((_, after)) = rest.split_once("):") {
            text = after.trim();
            if let Some((_, remainder)) = text.split_once(". ") {
                text = remainder.trim();
            }
        }
    }
    let text = text.strip_prefix("Unsourced estimate: ").unwrap_or(text);
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// A value to four significant figures without trailing zeros.
fn significant(value: f64) -> String {
    if !value.is_finite() || value == 0.0 {
        return format!("{value}");
    }
    let decimals = (3 - value.abs().log10().floor() as i32).clamp(0, 6) as usize;
    let text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

fn readonly_metrics(ui: &mut Ui, metrics: &[(String, f64, &str)]) {
    // Each column lays out independently, so a taller metric in one column
    // does not leave an empty row beside it. Narrow windows use one column.
    let columns = ((ui.available_width() / 380.0).floor() as usize)
        .clamp(1, 3)
        .min(metrics.len());
    let rows_per_column = metrics.len().div_ceil(columns);
    ui.columns(columns, |column_uis| {
        for (index, (label, value, unit)) in metrics.iter().enumerate() {
            // The schema's unit convention: a dimensionless ratio is a bare
            // number, and a physical unit gets its typographic form.
            let unit = crate::views::form::display_unit(unit);
            let text = if unit.is_empty() {
                format!("{label}: {}", significant(*value))
            } else {
                format!("{label}: {} {unit}", significant(*value))
            };
            column_uis[index / rows_per_column].label(text);
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
    // The cards, each with the fields it holds; the rest go to "Other".
    let mut cards: Vec<(&str, &str, bool, Vec<alas_config::Field>)> = Vec::new();
    for (title, names, default_open) in GROUPS {
        let section: Vec<alas_config::Field> = fields
            .iter()
            .filter(|field| names.contains(&field.name))
            .cloned()
            .collect();
        if !section.is_empty() {
            cards.push((title, title, default_open, section));
        }
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
        cards.push(("Other parameters", "other", true, remaining));
    }
    // Short cards share a row; a lone short card spreads over its own width.
    let cols: Vec<usize> = cards
        .iter()
        .map(|(_, _, _, list)| crate::views::form::leaf_columns(list, values))
        .collect();
    crate::views::form::flow_cards(ui, "engine_cards", &cols, |ui, k| {
        let (title, key, default_open, section) = &cards[k];
        // A search jump to a field of this card opens it.
        let forced = section.iter().any(|field| {
            crate::search::focus_wants(ui.ctx(), &format!("propulsion_cycle.{}", field.name))
        });
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let mut header = egui::CollapsingHeader::new(crate::theme::card_title(ui, tr(title)))
                .id_salt(format!("propulsion_cycle::{key}"))
                .default_open(*default_open)
                .open(crate::perf_tour::force_open());
            if forced {
                header = header.open(Some(true));
            }
            crate::theme::show_header(ui, header, |ui| {
                edits.extend(dynamic_form(
                    ui,
                    section,
                    values,
                    error_fields,
                    lang,
                    show_help,
                ));
            });
            crate::views::form::equalise(ui);
        });
        ui.add_space(6.0);
    });
    edits
}

#[cfg(test)]
mod display_tests {
    use super::{plain_source, significant};

    #[test]
    fn numbers_drop_trailing_zeros_and_keep_four_significant_figures() {
        assert_eq!(significant(10.0), "10");
        assert_eq!(significant(1670.0), "1670");
        assert_eq!(significant(467.0), "467");
        assert_eq!(significant(0.545), "0.545");
        assert_eq!(significant(0.0), "0");
        assert_eq!(significant(0.0123456), "0.01235");
    }

    #[test]
    fn a_source_note_loses_its_tracking_tag_and_repeated_class() {
        let note = plain_source(
            "NEEDS-DATA(PR2b, GE9X): sourced cruise TSFC datum (GE9X uninstalled) not on disk. Unsourced estimate: no public cruise TSFC was found.",
        );
        assert_eq!(note, "No public cruise TSFC was found.");
        assert!(!note.contains("NEEDS-DATA"));
        assert_eq!(plain_source("Scholz engine table"), "Scholz engine table");
    }
}
