// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::collections::HashSet;

use alas_config::{Entry, Field};
use egui::{DragValue, RichText, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::theme::card_frame;
use crate::views::form::{dynamic_form, FormEdit};
use crate::views::{tr, tr_fields};

use super::mission_profile_preview::show_mission_profile_preview;

const ACTIVE_FRACTION: f64 = 1.0e-6;
const MIN_CARD_COLUMN_WIDTH: f32 = 430.0;
const CRUISE_FRACTION_FIELDS: [&str; 3] = [
    "cruise_1_distance_fraction",
    "cruise_2_distance_fraction",
    "cruise_3_distance_fraction",
];
const DESCENT_ALTITUDE_FIELDS: [&str; 4] = [
    "descent_1_altitude_ft",
    "descent_2_altitude_ft",
    "descent_3_altitude_ft",
    "descent_4_altitude_ft",
];
const DEFAULT_CRUISE_FRACTIONS: [f64; 3] = [0.28169, 0.33803, 0.38028];
const DEFAULT_DESCENT_ALTITUDES_FT: [f64; 4] = [30_000.0, 17_000.0, 10_000.0, 6_500.0];
const DESCENT_1_FIELDS: [&str; 3] = [
    "descent_1_altitude_ft",
    "descent_1_air_speed_m_s",
    "descent_1_rate_m_s",
];
const DESCENT_2_FIELDS: [&str; 3] = [
    "descent_2_altitude_ft",
    "descent_2_air_speed_m_s",
    "descent_2_rate_m_s",
];
const DESCENT_3_FIELDS: [&str; 3] = [
    "descent_3_altitude_ft",
    "descent_3_air_speed_m_s",
    "descent_3_rate_m_s",
];
const DESCENT_4_FIELDS: [&str; 3] = [
    "descent_4_altitude_ft",
    "descent_4_air_speed_m_s",
    "descent_4_rate_m_s",
];

struct PhaseSection {
    id: &'static str,
    title: &'static str,
    fields: &'static [&'static str],
    default_open: bool,
}

/// Render the mission controls as actual solver phases instead of one nested,
/// visually uniform configuration tree.
pub(super) fn render_mission_form(
    state: &mut AppState,
    ui: &mut Ui,
    fields: &[Field],
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
) -> Vec<FormEdit> {
    let edits = {
        let Some(values) = state.group_mut("mission") else {
            return Vec::new();
        };
        let mut edits = Vec::new();
        render_run_and_routing(
            ui,
            fields,
            values,
            error_fields,
            lang,
            show_help,
            &mut edits,
        );
        ui.add_space(8.0);

        let Some(profile_fields) = profile_fields(fields) else {
            return edits;
        };
        let Some(profile_values) = values.get_mut("profile") else {
            return edits;
        };
        render_flight_profile(
            ui,
            &profile_fields,
            profile_values,
            error_fields,
            lang,
            show_help,
            &mut edits,
        );
        edits
    };

    ui.add_space(8.0);
    if let Some(config) = state.typed_config() {
        show_mission_profile_preview(ui, &config);
    } else {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            tr("The live profile will return when the mission inputs are valid."),
        );
    }
    edits
}

fn render_run_and_routing(
    ui: &mut Ui,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    edits: &mut Vec<FormEdit>,
) {
    let run_fields = fields_named(fields, &["enabled", "timeout_s"]);
    let routing_fields = fields_named(
        fields,
        &["great_circle_points", "simbrief_overrides_airports"],
    );
    let columns = card_column_count(ui.available_width());
    if columns == 2 {
        ui.columns(2, |column_uis| {
            render_field_card(
                &mut column_uis[0],
                "Mission run",
                "Enable the trajectory solve and set its time limit.",
                &run_fields,
                values,
                error_fields,
                lang,
                show_help,
                edits,
            );
            render_field_card(
                &mut column_uis[1],
                "Routing fallback",
                "Choose the smoothness of the great-circle fallback and how a filed plan may replace the selected airports.",
                &routing_fields,
                values,
                error_fields,
                lang,
                show_help,
                edits,
            );
        });
    } else {
        render_field_card(
            ui,
            "Mission run",
            "Enable the trajectory solve and set its time limit.",
            &run_fields,
            values,
            error_fields,
            lang,
            show_help,
            edits,
        );
        ui.add_space(8.0);
        render_field_card(
            ui,
            "Routing fallback",
            "Choose the smoothness of the great-circle fallback and how a filed plan may replace the selected airports.",
            &routing_fields,
            values,
            error_fields,
            lang,
            show_help,
            edits,
        );
    }
}

// The field-specific controls stay explicit so each phase card preserves its layout and edit wiring.
#[allow(clippy::too_many_arguments)]
fn render_field_card(
    ui: &mut Ui,
    title: &str,
    description: &str,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    edits: &mut Vec<FormEdit>,
) {
    if fields.is_empty() {
        return;
    }
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr(title)).strong());
        ui.label(RichText::new(tr(description)).weak().small());
        ui.add_space(4.0);
        edits.extend(dynamic_form(
            ui,
            fields,
            values,
            error_fields,
            lang,
            show_help,
        ));
    });
}

// The profile renderer receives independent phase controls to keep routing and values synchronized.
#[allow(clippy::too_many_arguments)]
fn render_flight_profile(
    ui: &mut Ui,
    profile_fields: &[Field],
    profile: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    edits: &mut Vec<FormEdit>,
) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Flight profile")).strong());
        ui.label(
            RichText::new(tr(
                "Each card below maps to an actual solver segment. Choose the active cruise legs and descent rungs before setting their targets.",
            ))
            .weak()
            .small(),
        );
        ui.add_space(6.0);
        phase_count_controls(ui, profile, edits);
        let active_cruise = active_cruise_count(profile);
        let active_descent = active_descent_count(profile);
        let cruise_share = active_cruise_share(profile, active_cruise);
        if (cruise_share - 1.0).abs() > 1.0e-4 {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr_fields(
                    "Active cruise-leg fractions sum to {sum}; set them to 1.0 so the scheduled cruise covers the route.",
                    &[("sum", format!("{cruise_share:.3}"))],
                ),
            );
        }
        ui.weak(tr_fields(
            "Configured segments: {count}",
            &[("count", (2 + active_cruise * 2 + active_descent).to_string())],
        ));
    });
    ui.add_space(8.0);

    let active_cruise = active_cruise_count(profile);
    let active_descent = active_descent_count(profile);
    let sections = profile_sections(active_cruise, active_descent);
    let columns = card_column_count(ui.available_width());
    for row in sections.chunks(columns) {
        ui.columns(columns, |column_uis| {
            for (index, section) in row.iter().enumerate() {
                render_phase_card(
                    &mut column_uis[index],
                    section,
                    profile_fields,
                    profile,
                    error_fields,
                    lang,
                    show_help,
                    edits,
                );
            }
        });
        ui.add_space(8.0);
    }
}

fn phase_count_controls(ui: &mut Ui, profile: &mut Value, edits: &mut Vec<FormEdit>) {
    let mut cruise_count = active_cruise_count(profile) as i64;
    let mut descent_count = active_descent_count(profile) as i64;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Active cruise legs")).strong());
        let cruise_response = ui.add(DragValue::new(&mut cruise_count).range(1..=3));
        ui.add_space(12.0);
        ui.label(RichText::new(tr("Active descent rungs")).strong());
        let descent_response = ui.add(DragValue::new(&mut descent_count).range(0..=4));
        if cruise_response.changed() {
            set_active_cruise_count(profile, cruise_count as usize);
            edits.push(FormEdit {
                label: tr("Active cruise legs"),
                value: cruise_count.to_string(),
            });
        }
        if descent_response.changed() {
            set_active_descent_count(profile, descent_count as usize);
            edits.push(FormEdit {
                label: tr("Active descent rungs"),
                value: descent_count.to_string(),
            });
        }
    });
}

fn profile_sections(cruise_count: usize, descent_count: usize) -> Vec<PhaseSection> {
    let mut sections = vec![
        PhaseSection {
            id: "takeoff",
            title: "Take-off",
            fields: &[
                "takeoff_altitude_gain_m",
                "takeoff_air_speed_m_s",
                "takeoff_climb_rate_m_s",
            ],
            default_open: true,
        },
        PhaseSection {
            id: "initial_climb",
            title: "Initial climb",
            fields: &[
                "initial_climb_air_speed_m_s",
                "initial_climb_rate_m_s",
                "initial_climb_altitude_fraction",
            ],
            default_open: true,
        },
        PhaseSection {
            id: "cruise_1",
            title: "Cruise leg 1",
            fields: &["cruise_1_air_speed_m_s", "cruise_1_distance_fraction"],
            default_open: true,
        },
    ];
    if cruise_count >= 2 {
        sections.extend([
            PhaseSection {
                id: "step_climb_1",
                title: "Step climb 1",
                fields: &[
                    "step_climb_1_air_speed_m_s",
                    "step_climb_1_rate_m_s",
                    "step_climb_1_altitude_fraction",
                ],
                default_open: false,
            },
            PhaseSection {
                id: "cruise_2",
                title: "Cruise leg 2",
                fields: &["cruise_2_air_speed_m_s", "cruise_2_distance_fraction"],
                default_open: false,
            },
        ]);
    }
    if cruise_count >= 3 {
        sections.extend([
            PhaseSection {
                id: "step_climb_2",
                title: "Step climb 2",
                fields: &["step_climb_2_air_speed_m_s", "step_climb_2_rate_m_s"],
                default_open: false,
            },
            PhaseSection {
                id: "cruise_3",
                title: "Cruise leg 3",
                fields: &["cruise_3_air_speed_m_s", "cruise_3_distance_fraction"],
                default_open: false,
            },
        ]);
    }
    let descent_sections = [
        ("descent_1", "Descent rung 1", &DESCENT_1_FIELDS[..]),
        ("descent_2", "Descent rung 2", &DESCENT_2_FIELDS[..]),
        ("descent_3", "Descent rung 3", &DESCENT_3_FIELDS[..]),
        ("descent_4", "Descent rung 4", &DESCENT_4_FIELDS[..]),
    ];
    for (index, (id, title, fields)) in descent_sections.into_iter().enumerate() {
        if index >= descent_count {
            break;
        }
        sections.push(PhaseSection {
            id,
            title,
            fields,
            default_open: false,
        });
    }
    sections.push(PhaseSection {
        id: "final_approach",
        title: "Final approach",
        fields: &["landing_air_speed_m_s", "landing_descent_rate_m_s"],
        default_open: false,
    });
    sections
}

// The phase card's inputs are intentionally separate to keep the repeated form sections readable.
#[allow(clippy::too_many_arguments)]
fn render_phase_card(
    ui: &mut Ui,
    section: &PhaseSection,
    profile_fields: &[Field],
    profile: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    edits: &mut Vec<FormEdit>,
) {
    let fields = fields_named(profile_fields, section.fields);
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        egui::CollapsingHeader::new(RichText::new(tr(section.title)).strong())
            .id_salt(format!("mission_profile::{}", section.id))
            .default_open(section.default_open)
            .show(ui, |ui| {
                edits.extend(dynamic_form(
                    ui,
                    &fields,
                    profile,
                    error_fields,
                    lang,
                    show_help,
                ));
            });
    });
}

fn profile_fields(fields: &[Field]) -> Option<Vec<Field>> {
    fields.iter().find_map(|field| match &field.entry {
        Entry::Node(node) if field.name == "profile" => Some(node.fields.clone()),
        _ => None,
    })
}

fn fields_named(fields: &[Field], names: &[&str]) -> Vec<Field> {
    fields
        .iter()
        .filter(|field| names.contains(&field.name))
        .cloned()
        .map(|mut field| {
            field.advanced = false;
            field
        })
        .collect()
}

fn card_column_count(available_width: f32) -> usize {
    (available_width >= MIN_CARD_COLUMN_WIDTH * 2.0) as usize + 1
}
