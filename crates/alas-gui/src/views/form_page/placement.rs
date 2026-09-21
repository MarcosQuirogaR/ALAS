// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Field placement between the Modeling pages and the Advanced Settings tabs.
//!
//! The schema stays authoritative for values, bounds and help; this module
//! decides which surface of a configuration group a page renders, which
//! fields are shown by another discipline instead (gear load limits under
//! Structures, airfoil sections under Aerodynamics, propulsion mass factors
//! under Advanced Settings > Propulsion, structural solver cases on Analyses)
//! and which legacy fields stay loadable but hidden.

use alas_config::{Entry, Field, Node};
use egui::{RichText, Ui};
use serde_json::Value;

use crate::nav::{Page, PageKind, Surface};
use crate::state::{AppState, PreviewTab};
use crate::views::form::{dynamic_form, FormEdit};
use crate::views::tr;

/// Dotted field paths a group's Advanced Settings tab renders; the Modeling
/// page renders the rest.
pub(super) fn advanced_paths(group: &str) -> &'static [&'static str] {
    match group {
        "mass_model" => &[
            "flops_transport",
            "flops_structure",
            "suspended_mass_fraction",
            "max_airspeed_for_flaps_ms",
            "flap_deflection_angle_deg",
        ],
        "structures" => &[
            "rib_buckling_coeff",
            "rib_radius_of_gyration_m",
            "num_ribs_override",
            "spanwise_stations",
            "mesh_chordwise_points",
            "timeout_s",
            "n_modes",
            "freq_sweep_max_hz",
            "freq_step_hz",
            "modal_damping_ratio",
            "random_force_psd_n2_per_hz",
        ],
        "mission" => &[
            "enabled",
            "timeout_s",
            "great_circle_points",
            "max_airway_stretch",
            "use_airway_endpoint_coordinates",
            "simbrief_username",
            "simbrief_timeout_s",
            "simbrief_overrides_airports",
        ],
        _ => &[],
    }
}

/// Dotted field paths another page renders (or that stay hidden as legacy),
/// so neither surface of the owning group shows them.
pub(super) fn relocated_paths(group: &str) -> &'static [&'static str] {
    match group {
        "mass_model" => &[
            "nlg_x_fraction",
            "mlg_x_fraction_mac",
            "pct_load_nlg_max",
            "pct_load_mlg_max",
            "pct_load_nlg_min",
            "mlw_fraction_mtow",
            "propulsion_twr_factor",
            "propulsion_installation_factor",
        ],
        "structures" => &[
            "enabled",
            "run_sol_static",
            "run_sol_modes",
            "run_sol_vibration_sine",
            "run_sol_vibration_random",
            "psd_base_g2_per_hz",
        ],
        "geometry" => &[
            "wing.root_airfoil",
            "wing.tip_airfoil",
            "empennage.tail_airfoil",
        ],
        "mission" => &["navdata_dir", "texture_path", "routes_dir"],
        _ => &[],
    }
}

/// Structural solver cases selected on Analyses > Structures.
pub(crate) const STRUCTURES_CASES: &[(&str, &str)] = &[
    ("run_sol_static", "Static analysis (SOL 101)"),
    ("run_sol_modes", "Normal modes (SOL 103)"),
    ("run_sol_vibration_sine", "Sine sweep (SOL 111)"),
    (
        "run_sol_vibration_random",
        "Random-vibration RMS from SOL 111",
    ),
];

/// The fields a page shows for its group.
pub(super) fn visible_fields(page: &Page, group: &str, fields: &[Field]) -> Vec<Field> {
    let fields: Vec<Field> = if group == "optimizer" {
        optimizer_ui_fields(fields)
    } else {
        fields.to_vec()
    };
    let relocated = relocated_paths(group);
    let advanced = advanced_paths(group);
    fields
        .into_iter()
        .filter_map(|field| prune(field, "", page.surface, relocated, advanced))
        .collect()
}

fn prune(
    mut field: Field,
    prefix: &str,
    surface: Surface,
    relocated: &[&str],
    advanced: &[&str],
) -> Option<Field> {
    let path = if prefix.is_empty() {
        field.name.to_owned()
    } else {
        format!("{prefix}.{}", field.name)
    };
    if relocated.contains(&path.as_str()) {
        return None;
    }
    let is_advanced = advanced.contains(&path.as_str());
    let child_surface = match surface {
        Surface::All => Surface::All,
        Surface::Modeling if is_advanced => return None,
        Surface::Modeling => Surface::Modeling,
        Surface::Advanced if is_advanced => Surface::All,
        Surface::Advanced => Surface::Advanced,
    };
    if let Entry::Node(node) = &mut field.entry {
        let children = std::mem::take(&mut node.fields);
        node.fields = children
            .into_iter()
            .filter_map(|child| prune(child, &path, child_surface, relocated, advanced))
            .collect();
        if node.fields.is_empty() {
            return None;
        }
        return Some(field);
    }
    (surface != Surface::Advanced || is_advanced).then_some(field)
}

/// Keep the optimizer page focused on the one product search contract. Legacy
/// method/strategy controls remain loadable by the config and parity paths,
/// but exposing them here would suggest that the product still dispatches a
/// menu of algorithms. The dedicated Design Space page owns `design_space`.
const LEGACY_OPTIMIZER_FIELDS: &[&str] = &[
    "weights",
    "design_space",
    "method",
    "strategy",
    "finite_difference_step",
    "constraint_tolerance",
    "tolerance",
    "workers",
    "display_progress",
    "seed_near_initial_design",
    "seed_perturbation_fraction",
];

pub(super) fn optimizer_ui_fields(fields: &[Field]) -> Vec<Field> {
    fields
        .iter()
        .filter_map(|field| {
            if LEGACY_OPTIMIZER_FIELDS.contains(&field.name) {
                return None;
            }
            if field.name != "solver" {
                return Some(field.clone());
            }
            let Entry::Node(node) = &field.entry else {
                return Some(field.clone());
            };
            let mut filtered = field.clone();
            let mut solver = node.clone();
            solver.fields = solver
                .fields
                .into_iter()
                .filter(|child| !LEGACY_OPTIMIZER_FIELDS.contains(&child.name))
                .map(|mut child| {
                    match child.name {
                        "max_iterations" => {
                            child.label = "MADS poll/search iterations";
                            child.help = "Maximum number of MADS poll/search iterations before the run reports iteration_limit.";
                        }
                        "population_size" => {
                            child.label = "MADS evaluation budget multiplier";
                            child.help = "Multiplier used by the current product driver to derive the bounded MADS evaluation budget from the design dimension and poll iterations.";
                        }
                        "seed" => {
                            child.label = "MADS random seed";
                            child.help = "Optional integer seed for reproducible MADS search points and poll directions.";
                        }
                        _ => {}
                    }
                    child
                })
                .collect();
            filtered.entry = Entry::Node(solver);
            Some(filtered)
        })
        .collect()
}

/// The schema fields of the node at a JSON pointer such as `/geometry/wing`.
pub(super) fn node_fields_at(schema: &Node, path: &str) -> Option<Vec<Field>> {
    let mut fields = &schema.fields;
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        let field = fields.iter().find(|field| field.name == segment)?;
        match &field.entry {
            Entry::Node(node) => fields = &node.fields,
            _ => return None,
        }
    }
    Some(fields.clone())
}

/// Render the page's cards of fields owned by other groups.
pub(super) fn render_extra_sections(
    state: &mut AppState,
    ui: &mut Ui,
    page: &Page,
    error_fields: &std::collections::HashSet<String>,
    lang: Option<&str>,
) {
    let show_help = state.help_verbose;
    for section in page.extra {
        let Some(fields) = node_fields_at(&state.schema, section.path) else {
            continue;
        };
        let fields: Vec<Field> = fields
            .into_iter()
            .filter(|field| section.names.contains(&field.name))
            .collect();
        if fields.is_empty() {
            continue;
        }
        let locked = section.path.starts_with("/geometry") && state.manual_geometry_locked();
        let mut edits = Vec::new();
        crate::theme::card_frame(ui).show(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new(tr(section.title)).strong())
                .id_salt(format!("{}::{}", page.id, section.title))
                .default_open(true)
                .show(ui, |ui| {
                    if locked {
                        preset_lock_note(ui);
                    }
                    ui.add_enabled_ui(!locked, |ui| {
                        if let Some(values) = state.config_values.pointer_mut(section.path) {
                            edits =
                                dynamic_form(ui, &fields, values, error_fields, lang, show_help);
                        }
                    });
                });
        });
        ui.add_space(6.0);
        commit_edits(state, edits);
    }
}

fn commit_edits(state: &mut AppState, edits: Vec<FormEdit>) {
    if edits.is_empty() {
        return;
    }
    state.on_config_modified();
    for edit in edits {
        state.note_parameter_modified(edit.label, edit.value);
    }
}

fn preset_lock_note(ui: &mut Ui) {
    ui.label(
        RichText::new(tr(
            "Preset geometry is protected from manual edits here as well; open the sandbox for geometry experiments.",
        ))
        .color(ui.visuals().warn_fg_color)
        .small(),
    );
}

/// Keep the Live Preview on the discipline being edited: entering Cabin &
/// Cargo shows the interior, leaving it for another discipline restores the
/// exterior. Navigation never changes geometry.
pub(super) fn sync_preview_tab(state: &mut AppState, ctx: &egui::Context, page: &Page) {
    let key = egui::Id::new("form_page::last_page");
    let last: Option<String> = ctx.data(|data| data.get_temp(key));
    if last.as_deref() == Some(page.id) {
        return;
    }
    if page.id == "cabin" {
        state.preview_tab = PreviewTab::Cabin;
    } else if last.as_deref() == Some("cabin") && page.kind == PageKind::Form {
        state.preview_tab = PreviewTab::Exterior;
    }
    ctx.data_mut(|data| data.insert_temp(key, page.id.to_owned()));
}

/// Section rows of a JSON object toggled from Analyses.
pub(crate) fn toggle_bool(values: &mut Value, name: &str, enabled: bool) {
    if let Some(object) = values.as_object_mut() {
        object.insert(name.to_owned(), Value::Bool(enabled));
    }
}

/// The propulsion editor split by surface: Modeling shows the engine
/// selector and the cycle assumptions; Advanced Settings shows the rating
/// and cycle anchors, installation and nacelle placement.
pub(super) fn render_propulsion_editor(
    state: &mut AppState,
    ui: &mut Ui,
    surface: Surface,
    cycle_fields: &[Field],
    error_fields: &std::collections::HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
) -> Vec<FormEdit> {
    if surface != Surface::Advanced {
        render_engine_selector(state, ui);
    }
    let model = state
        .typed_config()
        .ok_or_else(|| "configuration cannot be decoded".to_owned())
        .and_then(|config| super::engine_editor_model(&config.geometry.engine));
    let model = match model {
        Ok(model) => model,
        Err(error) => {
            ui.colored_label(ui.visuals().error_fg_color, error);
            return Vec::new();
        }
    };
    let mut edits = Vec::new();
    if surface == Surface::Advanced {
        super::render_engine_physics_summary(ui, &model);
        ui.add_space(6.0);
        render_installation_card(state, ui, &mut edits, error_fields, lang, show_help);
        return edits;
    }
    ui.add_space(6.0);
    let turboprop = matches!(model, super::EngineEditorModel::Turboprop { .. });
    let cycle_fields = propulsion_cycle_fields(cycle_fields, turboprop);
    if let Some(values) = state.group_mut("propulsion_cycle") {
        edits.extend(super::render_engine_designer_form(
            ui,
            &cycle_fields,
            values,
            error_fields,
            lang,
            show_help,
        ));
    }
    edits
}

fn propulsion_cycle_fields(fields: &[Field], turboprop: bool) -> Vec<Field> {
    const FAN_ONLY: &[&str] = &[
        "fan_polytropic_efficiency",
        "fan_nozzle_pressure_ratio",
        "fan_nozzle_efficiency",
        "fan_face_mach",
    ];
    fields
        .iter()
        .filter(|field| {
            if turboprop {
                !FAN_ONLY.contains(&field.name)
            } else {
                !field.name.starts_with("turboprop_")
            }
        })
        .cloned()
        .map(|mut field| {
            if turboprop {
                match field.name {
                    "hpt_polytropic_efficiency" => {
                        field.label = "Gas-generator turbine polytropic efficiency";
                        field.help = "Aggregate turbine efficiency for work supplied to the core compressors.";
                    }
                    "lpt_polytropic_efficiency" => {
                        field.label = "Power turbine polytropic efficiency";
                        field.help = "Free-turbine efficiency for the selected engine's cruise shaft-power output.";
                    }
                    _ => {}
                }
            }
            field
        })
        .collect()
}

fn render_engine_selector(state: &mut AppState, ui: &mut Ui) {
    let current_name = state
        .config_values
        .pointer("/geometry/engine/engine_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let mut chosen = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Engine model")).strong());
        egui::ComboBox::from_id_salt("engine_designer_engine")
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
        state.set_engine(&name);
    }
}

fn render_installation_card(
    state: &mut AppState,
    ui: &mut Ui,
    edits: &mut Vec<FormEdit>,
    error_fields: &std::collections::HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
) {
    let installation_names = [
        "nacelle_profile",
        "radius_scale_m",
        "spanwise_positions_m",
        "z_m",
        "inlet_x_offset_m",
    ];
    let fields: Vec<Field> = alas_config::ConfigNode::schema(&alas_config::EngineConfig::default())
        .fields
        .into_iter()
        .filter(|field| installation_names.contains(&field.name))
        .collect();
    let locked = state.manual_geometry_locked();
    crate::theme::card_frame(ui).show(ui, |ui| {
        egui::CollapsingHeader::new(RichText::new(tr("Installation & nacelle")).strong())
            .id_salt("propulsion::installation")
            .default_open(true)
            .show(ui, |ui| {
                if locked {
                    preset_lock_note(ui);
                }
                ui.add_enabled_ui(!locked, |ui| {
                    if let Some(values) = state.config_values.pointer_mut("/geometry/engine") {
                        edits.extend(dynamic_form(
                            ui,
                            &fields,
                            values,
                            error_fields,
                            lang,
                            show_help,
                        ));
                    }
                });
            });
    });
}

#[cfg(test)]
mod tests {
    use super::{
        advanced_paths, node_fields_at, relocated_paths, toggle_bool, visible_fields,
        STRUCTURES_CASES,
    };
    use crate::nav::{all_pages, page, ADVANCED_SETTINGS_PAGES, NAV};
    use alas_config::{AlasConfig, ConfigNode, Entry, Field};

    #[test]
    fn cycle_controls_follow_the_active_engine_technology() {
        let fields = alas_config::PropulsionCycleConfig::default()
            .schema()
            .fields;
        let turboprop = super::propulsion_cycle_fields(&fields, true);
        let turbofan = super::propulsion_cycle_fields(&fields, false);
        for name in [
            "turboprop_overall_pressure_ratio",
            "turboprop_turbine_inlet_temperature_k",
        ] {
            assert!(turboprop.iter().any(|field| field.name == name));
            assert!(!turbofan.iter().any(|field| field.name == name));
        }
        for name in [
            "fan_polytropic_efficiency",
            "fan_nozzle_pressure_ratio",
            "fan_nozzle_efficiency",
            "fan_face_mach",
        ] {
            assert!(turbofan.iter().any(|field| field.name == name));
            assert!(!turboprop.iter().any(|field| field.name == name));
        }
        for name in [
            "lpc_pressure_ratio_split",
            "hpc_polytropic_efficiency",
            "lpt_polytropic_efficiency",
            "core_nozzle_efficiency",
            "cp_hot_j_kgk",
        ] {
            assert!(turboprop.iter().any(|field| field.name == name));
            assert!(turbofan.iter().any(|field| field.name == name));
        }
    }

    fn leaf_paths(fields: &[Field], prefix: &str, out: &mut Vec<String>) {
        for field in fields {
            let path = if prefix.is_empty() {
                field.name.to_owned()
            } else {
                format!("{prefix}.{}", field.name)
            };
            match &field.entry {
                Entry::Node(node) => leaf_paths(&node.fields, &path, out),
                _ => out.push(path),
            }
        }
    }

    fn covered(path: &str, list: &[&str]) -> bool {
        list.iter()
            .any(|entry| path == *entry || path.starts_with(&format!("{entry}.")))
    }

    #[test]
    fn every_field_of_a_split_group_is_shown_on_exactly_one_surface_or_relocated() {
        let schema = AlasConfig::default().schema();
        for group in ["mass_model", "structures", "geometry"] {
            let fields = node_fields_at(&schema, &format!("/{group}")).expect(group);
            let mut leaves = Vec::new();
            leaf_paths(&fields, "", &mut leaves);
            let pages: Vec<&crate::nav::Page> = all_pages()
                .filter(|page| page.group == Some(group))
                .collect();
            assert!(!pages.is_empty(), "{group} has no page");
            for leaf in &leaves {
                let relocated = covered(leaf, relocated_paths(group));
                let shown_on = pages
                    .iter()
                    .filter(|page| shown(&visible_fields(page, group, &fields), leaf))
                    .count();
                assert_eq!(
                    usize::from(relocated) + shown_on,
                    1,
                    "{group}.{leaf}: relocated={relocated} shown on {shown_on} page(s)"
                );
                if relocated {
                    assert!(
                        relocation_target_exists(group, leaf),
                        "{group}.{leaf} is relocated but no page renders it"
                    );
                }
            }
        }
    }

    fn shown(fields: &[Field], leaf: &str) -> bool {
        let mut leaves = Vec::new();
        leaf_paths(fields, "", &mut leaves);
        leaves.iter().any(|path| path == leaf)
    }

    fn relocation_target_exists(group: &str, leaf: &str) -> bool {
        let legacy = group == "structures" && leaf == "psd_base_g2_per_hz";
        let case = group == "structures"
            && (leaf == "enabled" || STRUCTURES_CASES.iter().any(|(name, _)| *name == leaf));
        let extra = all_pages().any(|page| {
            page.extra.iter().any(|section| {
                section.path.trim_start_matches('/').replace('/', ".") == group
                    || section.names.iter().any(|name| {
                        format!(
                            "{}.{name}",
                            section.path.trim_start_matches('/').replace('/', ".")
                        ) == format!("{group}.{leaf}")
                    })
            })
        });
        legacy || case || extra
    }

    #[test]
    fn the_modeling_mass_page_hides_flops_and_the_advanced_tab_hides_fuel_density() {
        let schema = AlasConfig::default().schema();
        let fields = node_fields_at(&schema, "/mass_model").expect("mass_model");
        let modeling = visible_fields(page("mass_model").expect("page"), "mass_model", &fields);
        assert!(modeling.iter().all(|field| field.name != "flops_transport"));
        assert!(modeling
            .iter()
            .any(|field| field.name == "fuel_density_kg_m3"));
        assert!(modeling
            .iter()
            .all(|field| field.name != "pct_load_nlg_max"));
        let advanced = visible_fields(page("mass_advanced").expect("page"), "mass_model", &fields);
        assert!(advanced.iter().any(|field| field.name == "flops_transport"));
        assert!(advanced
            .iter()
            .any(|field| field.name == "max_airspeed_for_flaps_ms"));
        assert!(advanced
            .iter()
            .all(|field| field.name != "fuel_density_kg_m3"));
        assert!(advanced
            .iter()
            .all(|field| field.name != "propulsion_twr_factor"));
    }

    #[test]
    fn airfoil_sections_leave_geometry_and_appear_under_aerodynamics() {
        let schema = AlasConfig::default().schema();
        let geometry = node_fields_at(&schema, "/geometry").expect("geometry");
        let shown_fields = visible_fields(page("geometry").expect("page"), "geometry", &geometry);
        assert!(!shown(&shown_fields, "wing.root_airfoil"));
        assert!(shown(&shown_fields, "wing.root_datum_x_m"));
        let aerodynamics = page("aerodynamics").expect("page");
        assert_eq!(aerodynamics.group, Some("drag_model"));
        let wing = node_fields_at(&schema, "/geometry/wing").expect("wing node");
        for section in aerodynamics.extra {
            let fields = node_fields_at(&schema, section.path).expect(section.path);
            for name in section.names {
                assert!(fields.iter().any(|field| field.name == *name), "{name}");
            }
        }
        assert!(wing.iter().any(|field| field.name == "tip_airfoil"));
        assert!(advanced_paths("geometry").is_empty());
    }

    #[test]
    fn the_advanced_window_offers_airfoil_screening_and_external_tools_tabs() {
        let ids: Vec<&str> = ADVANCED_SETTINGS_PAGES.iter().map(|page| page.id).collect();
        assert!(ids.contains(&"setup_tools"));
        assert!(ids.contains(&"airfoil_screening"));
        assert!(!NAV.iter().any(|group| group.title == "Advanced Settings"));
    }

    #[test]
    fn analyses_toggle_writes_the_structural_case_flag() {
        let mut values = serde_json::json!({"run_sol_static": true, "run_sol_modes": false});
        toggle_bool(&mut values, "run_sol_modes", true);
        assert_eq!(values["run_sol_modes"], serde_json::Value::Bool(true));
    }
}

#[cfg(test)]
mod preview_sync_tests {
    use super::sync_preview_tab;
    use crate::nav::page;
    use crate::state::{AppState, PreviewTab};

    #[test]
    fn entering_cabin_shows_the_interior_and_leaving_it_restores_the_exterior() {
        let mut state = AppState::default();
        let ctx = egui::Context::default();
        state.preview_tab = PreviewTab::Exterior;
        sync_preview_tab(&mut state, &ctx, page("mass_model").expect("mass page"));
        assert_eq!(state.preview_tab, PreviewTab::Exterior);
        sync_preview_tab(&mut state, &ctx, page("cabin").expect("cabin page"));
        assert_eq!(state.preview_tab, PreviewTab::Cabin);
        sync_preview_tab(&mut state, &ctx, page("cabin").expect("cabin page"));
        assert_eq!(state.preview_tab, PreviewTab::Cabin);
        sync_preview_tab(
            &mut state,
            &ctx,
            page("structures").expect("structures page"),
        );
        assert_eq!(state.preview_tab, PreviewTab::Exterior);
    }
}
