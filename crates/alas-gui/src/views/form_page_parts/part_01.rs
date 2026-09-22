// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_config::{ActiveEngineModel, EngineConfig};
use egui::{vec2, RichText, Ui};

use crate::nav::{Page, Surface};
use crate::state::AppState;
use crate::views::form::{dynamic_form, dynamic_form_with_open_root_nodes, FormEdit};
use crate::views::tr;

#[path = "../form_page/aux_preset.rs"]
mod aux_preset;
#[path = "../mission_form.rs"]
mod mission_form;
#[path = "../form_page/placement.rs"]
pub(crate) mod placement;
#[path = "../form_page_sections.rs"]
mod sections;

use aux_preset::show_aux_preset_picker;
use mission_form::render_mission_form;
use sections::{page_sections, render_sectioned_form};

/// The notice a page carries while a registered preset protects its geometry.
pub const PRESET_LOCK_NOTICE: &str =
    "Preset geometry is protected from manual edits here as well; open the sandbox for geometry experiments.";

/// Render one Advanced Settings form page.
pub fn show_form_page(state: &mut AppState, ui: &mut Ui, page: &Page) {
    show_form_page_locked(state, ui, page, false);
}

/// Render one Advanced Settings form page, with its editors optionally locked.
///
/// Locking a page is not done by wrapping it whole in `add_enabled_ui(false)`. egui's
/// disabled scope fades every painted colour toward the background, so the page
/// title, its description, every field *label* and the page actions all dropped
/// to the disabled token together (measured 4.28-4.68:1 against 12-15:1 on an
/// active page) and the page read as failed to load rather than as protected.
/// Only the editors are disabled here, and the lock notice sits under the page
/// title instead of above it, inside the page's own heading hierarchy.
pub fn show_form_page_locked(state: &mut AppState, ui: &mut Ui, page: &Page, locked: bool) {
    let Some(group) = page.group else { return };
    placement::sync_preview_tab(state, ui.ctx(), page);

    let heading = ui.heading(alas_i18n::t(Some(page.title), None));
    if let Some(desc) = page.description {
        heading.on_hover_text(alas_i18n::t(Some(desc), None));
    }
    if locked {
        ui.add_space(2.0);
        ui.label(
            RichText::new(tr(PRESET_LOCK_NOTICE))
                .color(ui.visuals().warn_fg_color)
                .small(),
        );
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
        // A framed button, not frameless text: "Reset page" changes every
        // value on the page and must look like the action it is.
        if ui
            .add_enabled(!locked, egui::Button::new(tr("Reset page")).small())
            .on_hover_text(tr(
                "Restore this page's default values; other pages are unchanged.",
            ))
            .clicked()
        {
            state.reset_group_to_defaults(group);
        }
    });
    ui.add_space(4.0);

    let visible_fields = placement::visible_fields(page, group, &fields);
    ui.add_enabled_ui(!locked, |ui| {
        render_editor(
            state,
            ui,
            group,
            page.surface,
            &visible_fields,
            &error_fields,
            lang,
        );
        placement::render_extra_sections(state, ui, page, &error_fields, lang);
    });
    if group == "mission" && page.surface == Surface::Advanced {
        crate::views::mission_profile_inputs::show_mission_profile_advanced(state, ui);
    }
    render_preview(state, ui, page.preview, page.preview_title);
}

fn render_editor(
    state: &mut AppState,
    ui: &mut Ui,
    group: &str,
    surface: Surface,
    fields: &[alas_config::Field],
    error_fields: &std::collections::HashSet<String>,
    lang: Option<&str>,
) {
    // The page-level scroll area in `route_page` owns both the editor and its
    // preview so the preview cannot disappear behind the resizable run log.
    ui.vertical(|ui| {
        let show_help = state.help_verbose;
        let edits = if group == "mission" && surface != Surface::Advanced {
            render_mission_form(state, ui, fields, error_fields, lang, show_help)
        } else if group == "propulsion_cycle" {
            placement::render_propulsion_editor(
                state,
                ui,
                surface,
                fields,
                error_fields,
                lang,
                show_help,
            )
        } else if group == "cabin" {
            // Passenger and cargo are the two primary cabin controls. Keep
            // both root nodes open and render one root node per form pass so
            // the page reads as two stacked, immediately editable cards even
            // in a wide window. The generic form intentionally uses adaptive
            // columns for dense scalar settings; cabin's two large cards need
            // the available width for their nested fields instead.
            if let Some(values) = state.group_mut(group) {
                let mut edits = Vec::new();
                for field in fields {
                    edits.extend(dynamic_form_with_open_root_nodes(
                        ui,
                        std::slice::from_ref(field),
                        values,
                        error_fields,
                        lang,
                        show_help,
                        true,
                    ));
                }
                edits
            } else {
                Vec::new()
            }
        } else if let Some(values) = state.group_mut(group) {
            if let Some(sections) = page_sections(group, surface) {
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
