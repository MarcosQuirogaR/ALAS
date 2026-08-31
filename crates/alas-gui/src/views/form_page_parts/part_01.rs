// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_config::{ActiveEngineModel, ConfigNode, EngineConfig};
use egui::{vec2, ComboBox, RichText, Ui};

use crate::nav::Page;
use crate::state::AppState;
use crate::views::form::{dynamic_form, dynamic_form_with_open_root_nodes, FormEdit};
use crate::views::tr;

#[path = "../form_page/aux_preset.rs"]
mod aux_preset;
#[path = "../mission_form.rs"]
mod mission_form;
#[path = "../mission_profile_preview.rs"]
mod mission_profile_preview;
#[path = "../form_page_sections.rs"]
mod sections;

use aux_preset::show_aux_preset_picker;
use mission_form::render_mission_form;
use sections::{page_sections, render_sectioned_form};

/// Render one Advanced Settings form page.
pub fn show_form_page(state: &mut AppState, ui: &mut Ui, page: &Page) {
    let Some(group) = page.group else { return };

    let heading = ui.heading(alas_i18n::t(Some(page.title), None));
    if let Some(desc) = page.description {
        heading.on_hover_text(alas_i18n::t(Some(desc), None));
    }
    if state.help_verbose {
        if let Some(desc) = page.description {
            ui.label(RichText::new(alas_i18n::t(Some(desc), None)).weak());
        }
    }
    ui.add_space(4.0);

    if state.help_verbose && !page.detail.is_empty() {
        egui::CollapsingHeader::new(tr("How this works"))
            .default_open(false)
            .show(ui, |ui| {
                for para in page.detail {
                    ui.label(alas_i18n::t(Some(para), None));
                    ui.add_space(4.0);
                }
            });
        ui.add_space(4.0);
    }

    if let Some(kind) = page.preset_kind {
        show_aux_preset_picker(state, ui, kind);
        ui.add_space(4.0);
    }

    ui.add_space(8.0);

    let error_fields: std::collections::HashSet<String> = state
        .validation_findings
        .iter()
        .filter(|f| f.field_path.starts_with(group))
        .map(|f| {
            f.field_path
                .rsplit('.')
                .next()
                .unwrap_or(&f.field_path)
                .to_owned()
        })
        .collect();
    let lang = Some(state.language.code());

    let group_node = state.schema.field(group).and_then(|f| match &f.entry {
        alas_config::Entry::Node(n) => Some(n.fields.clone()),
        _ => None,
    });
    let Some(fields) = group_node else { return };

    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Parameter values")).strong());
        if ui
            .add(egui::Button::new(tr("Reset page")).small().frame(false))
            .on_hover_text(tr(
                "Restore this page's default values; other pages are unchanged.",
            ))
            .clicked()
        {
            state.reset_group_to_defaults(group);
        }
    });
    ui.add_space(4.0);

    let visible_fields: Vec<alas_config::Field> = fields
        .iter()
        .filter(|field| !is_external_tools_field(group, field.name))
        .cloned()
        .collect();
    render_editor(state, ui, group, &visible_fields, &error_fields, lang);
    render_preview(state, ui, page.preview, page.preview_title);
}

/// Keep machine- and user-specific locations out of the model form. Their
/// values remain in the mission configuration at the pipeline boundary, but
/// Setup > External Tools is the single human-facing place to manage them.
fn is_external_tools_field(group: &str, name: &str) -> bool {
    group == "mission" && matches!(name, "navdata_dir" | "texture_path" | "routes_dir")
}

fn render_editor(
    state: &mut AppState,
    ui: &mut Ui,
    group: &str,
    fields: &[alas_config::Field],
    error_fields: &std::collections::HashSet<String>,
    lang: Option<&str>,
) {
    // The page-level scroll area in `route_page` owns both the editor and its
    // preview so the preview cannot disappear behind the resizable run log.
    ui.vertical(|ui| {
        let show_help = state.help_verbose;
        let edits = if group == "mission" {
            render_mission_form(state, ui, fields, error_fields, lang, show_help)
        } else if group == "propulsion_cycle" {
            render_propulsion_editor(state, ui, fields, error_fields, lang, show_help)
        } else if let Some(values) = state.group_mut(group) {
            if let Some(sections) = page_sections(group) {
                render_sectioned_form(
                    ui,
                    group,
                    fields,
                    values,
                    error_fields,
                    lang,
                    show_help,
                    sections,
                )
            } else if group == "optimizer" {
                // These are the two first-class halves of one optimizer, not
                // nested detail menus. Their direct sections should be ready
                // to scan when the page first opens.
                dynamic_form_with_open_root_nodes(
                    ui,
                    fields,
                    values,
                    error_fields,
                    lang,
                    show_help,
                    true,
                )
            } else {
                dynamic_form(ui, fields, values, error_fields, lang, show_help)
            }
        } else {
            Vec::new()
        };
        if !edits.is_empty() {
            state.on_config_modified();
            for edit in edits {
                state.note_parameter_modified(edit.label, edit.value);
            }
        }
    });
}

#[derive(Debug, Clone, PartialEq)]
enum EngineEditorModel {
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

fn engine_editor_model(engine: &EngineConfig) -> Result<EngineEditorModel, String> {
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

fn render_propulsion_editor(
    state: &mut AppState,
    ui: &mut Ui,
    cycle_fields: &[alas_config::Field],
    error_fields: &std::collections::HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
) -> Vec<FormEdit> {
    let current_name = state
        .config_values
        .pointer("/geometry/engine/engine_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_owned();
    let mut chosen = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Engine model")).strong());
        ComboBox::from_id_salt("engine_designer_engine")
            .selected_text(&current_name)
            .show_ui(ui, |ui| {
                for name in &state.engine_names {
                    if ui.selectable_label(*name == current_name, name).clicked() {
                        chosen = Some(name.clone());
                    }
                }
            });
    });
    if let Some(name) = chosen {
        let _ = state.set_engine(&name);
    }

    let model = state
        .typed_config()
        .ok_or_else(|| "configuration cannot be decoded".to_owned())
        .and_then(|config| engine_editor_model(&config.geometry.engine));
    match &model {
        Ok(model) => render_engine_physics_summary(ui, model),
        Err(error) => {
            ui.colored_label(ui.visuals().error_fg_color, error);
            return Vec::new();
        }
    }

    ui.add_space(6.0);
    let mut edits = Vec::new();
    let installation_names = [
        "nacelle_profile",
        "radius_scale_m",
        "spanwise_positions_m",
        "z_m",
        "inlet_x_offset_m",
    ];
    let installation_fields: Vec<_> = EngineConfig::default()
        .schema()
        .fields
        .into_iter()
        .filter(|field| installation_names.contains(&field.name))
        .collect();
    crate::theme::card_frame(ui).show(ui, |ui| {
        egui::CollapsingHeader::new(RichText::new(tr("Installation & nacelle")).strong())
            .id_salt("propulsion::installation")
            .default_open(true)
            .show(ui, |ui| {
                if let Some(values) = state.config_values.pointer_mut("/geometry/engine") {
                    edits.extend(dynamic_form(
                        ui,
                        &installation_fields,
                        values,
                        error_fields,
                        lang,
                        show_help,
                    ));
                }
            });
    });

    if matches!(model, Ok(EngineEditorModel::Turbofan { .. })) {
        ui.add_space(6.0);
        if let Some(values) = state.group_mut("propulsion_cycle") {
            edits.extend(render_engine_designer_form(
                ui,
                cycle_fields,
                values,
                error_fields,
                lang,
                show_help,
            ));
        }
    } else {
        ui.label(
            RichText::new(tr(
                "Turbofan BPR, OPR, FPR, T4, TSFC and ICAO LTO controls do not apply to this shaft-power propulsion model.",
            ))
            .weak()
            .small(),
        );
    }
    edits
}

fn render_engine_physics_summary(ui: &mut Ui, model: &EngineEditorModel) {
    crate::theme::card_frame(ui).show(ui, |ui| match model {
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
            ui.label(RichText::new(tr("Turbofan rating & cycle anchors")).strong());
            readonly_metric(ui, "Rated thrust per engine", *rated_thrust_kn, "kN");
            readonly_metric(ui, "Bypass ratio", *bypass_ratio, "-");
            readonly_metric(
                ui,
                "ICAO take-off bypass ratio",
                *takeoff_bypass_ratio,
                "-",
            );
            readonly_metric(ui, "Overall pressure ratio", *overall_pressure_ratio, "-");
            readonly_metric(ui, "Fan pressure ratio", *fan_pressure_ratio, "-");
            readonly_metric(ui, "Turbine inlet temperature", *turbine_inlet_temp_k, "K");
            readonly_metric(ui, "Cruise TSFC reference", *cruise_tsfc, "kg/(kgf.hr)");
            readonly_metric(
                ui,
                "ICAO take-off fuel flow per engine",
                *takeoff_fuel_flow_kg_s,
                "kg/s",
            );
            ui.label(
                RichText::new(format!("{}: {off_design_evidence}", tr("Off-design thrust")))
                    .weak()
                    .small(),
            );
            ui.label(RichText::new(format!("{}: {provenance}", tr("Provenance"))).weak().small());
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
            ui.label(RichText::new(tr("Turboprop shaft-power system")).strong());
            readonly_metric(ui, "Take-off shaft power per engine", *takeoff_kw, "kW");
            readonly_metric(ui, "Maximum reserve / OEI power", *reserve_kw, "kW");
            readonly_metric(ui, "Maximum continuous power", *continuous_kw, "kW");
            readonly_metric(ui, "Maximum climb power", *climb_kw, "kW");
            readonly_metric(ui, "Maximum cruise power", *cruise_kw, "kW");
            readonly_metric(
                ui,
                "Two-engine maximum-cruise fuel flow",
                *cruise_fuel_flow_kg_h,
                "kg/h",
            );
            ui.label(format!("{}: {propeller_model}", tr("Propeller model")));
            readonly_metric(ui, "Propeller diameter", *diameter_m, "m");
            readonly_metric(ui, "Governed propeller speed", *governed_rpm, "rpm");
            readonly_metric(ui, "Reduction ratio", *reduction_ratio, "-");
            ui.label(RichText::new(format!("{}: {provenance}", tr("Provenance"))).weak().small());
            ui.label(
                RichText::new(tr(
                    "Limitation: conceptual Level-1 propeller performance; no proprietary PW127M/568F engine deck or propeller map is claimed.",
                ))
                .weak()
                .small(),
            );
        }
    });
}

fn readonly_metric(ui: &mut Ui, label: &str, value: f64, unit: &str) {
    ui.label(format!("{}: {value:.3} {unit}", tr(label)));
}
