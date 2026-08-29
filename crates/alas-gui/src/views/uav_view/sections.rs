// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission, hardware, and propulsion controls for the fixed-wing UAV workflow.

use egui::{ComboBox, Grid, RichText, TextEdit, Ui};

use crate::theme::selectable_button;
use crate::uav::{ComponentRole, MissionPlanMode, PropulsionInputMode, UavWorkflowState};

use super::super::tr;
use super::presentation::{card, card_column_count, collapsing_card};
use super::uav_fields::{integer, value};

pub(super) fn mission_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    if card_column_count(ui.available_width()) == 2 {
        ui.columns(2, |columns| {
            mission_card(state, &mut columns[0]);
            payload_card(state, &mut columns[1]);
        });
    } else {
        mission_card(state, ui);
        ui.add_space(8.0);
        payload_card(state, ui);
    }
    ui.add_space(8.0);
    card(
        ui,
        "Electrical mission plan",
        Some(
            "The normal path derives departure, climb, cruise, and approach-reserve phases from the brief. The cruise duration is extended until both range and endurance are met.",
        ),
        |ui| {
            mission_mode_tabs(state, ui);
            ui.add_space(6.0);
            if state.mission_plan_mode == MissionPlanMode::Standard {
                ui.weak(tr(
                    "The standard plan keeps the normal workflow short. Each phase is solved at its own speed and command; no cruise-only energy surrogate is used.",
                ));
            } else {
                advanced_mission_inputs(state, ui);
            }
        },
    );
}

fn mission_card(state: &mut UavWorkflowState, ui: &mut Ui) {
    card(
        ui,
        "Mission objectives",
        Some("These values describe what the aircraft must do; they are not catalogue properties."),
        |ui| {
            form_grid(ui, "uav_mission_grid", |ui| {
                value(ui, "Endurance", &mut state.objectives.endurance_s, "s");
                value(ui, "Range", &mut state.objectives.range_m, "m");
                value(
                    ui,
                    "Cruise speed",
                    &mut state.objectives.cruise_speed_m_s,
                    "m/s",
                );
                value(
                    ui,
                    "Maximum stall speed",
                    &mut state.objectives.maximum_stall_speed_m_s,
                    "m/s",
                );
            });
        },
    );
}

fn payload_card(state: &mut UavWorkflowState, ui: &mut Ui) {
    card(ui, "Payload envelope", None, |ui| {
        form_grid(ui, "uav_payload_grid", |ui| {
            value(
                ui,
                "Payload mass",
                &mut state.objectives.payload_mass_kg,
                "kg",
            );
            value(
                ui,
                "Payload length",
                &mut state.objectives.payload_dimensions.length_m,
                "m",
            );
            value(
                ui,
                "Payload width",
                &mut state.objectives.payload_dimensions.width_m,
                "m",
            );
            value(
                ui,
                "Payload height",
                &mut state.objectives.payload_dimensions.height_m,
                "m",
            );
        });
        ui.add_space(8.0);
        ui.label(RichText::new(tr("Ranking policy")).strong());
        form_grid(ui, "uav_ranking_grid", |ui| {
            value(
                ui,
                "Minimum propulsive efficiency",
                &mut state.objectives.minimum_propulsive_efficiency,
                "",
            );
            value(
                ui,
                "Efficiency ranking weight",
                &mut state.objectives.efficiency_priority,
                "",
            );
        });
    });
}

fn mission_mode_tabs(state: &mut UavWorkflowState, ui: &mut Ui) {
    let modes = [MissionPlanMode::Standard, MissionPlanMode::Advanced];
    let columns = card_column_count(ui.available_width()).min(modes.len());
    for row in modes.chunks(columns) {
        ui.columns(columns, |column_uis| {
            for (index, mode) in row.iter().enumerate() {
                if column_uis[index]
                    .add_sized(
                        [column_uis[index].available_width(), 30.0],
                        selectable_button(tr(mode.label()), state.mission_plan_mode == *mode),
                    )
                    .clicked()
                {
                    state.mission_plan_mode = *mode;
                    if *mode == MissionPlanMode::Advanced && state.mission_phases.is_empty() {
                        state.reset_advanced_mission();
                    }
                }
            }
        });
        ui.add_space(4.0);
    }
}

fn advanced_mission_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    ui.colored_label(
        ui.visuals().warn_fg_color,
        tr("Advanced phases are steady flight conditions. Their total duration and still-air distance must meet the mission objectives."),
    );
    let mut remove = None;
    for (index, phase) in state.mission_phases.iter_mut().enumerate() {
        egui::CollapsingHeader::new(
            RichText::new(format!("{} {}", tr("Phase"), index + 1)).strong(),
        )
        .id_salt(format!("uav_mission_phase_{index}"))
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Phase name")).strong());
                ui.add_sized(
                    [
                        ui.available_width().clamp(140.0, 320.0),
                        ui.spacing().interact_size.y,
                    ],
                    TextEdit::singleline(&mut phase.name),
                );
                if ui.small_button(tr("Remove")).clicked() {
                    remove = Some(index);
                }
            });
            form_grid(ui, format!("uav_mission_phase_values_{index}"), |ui| {
                value(ui, "Duration", &mut phase.duration_s, "s");
                value(ui, "Speed", &mut phase.condition.speed_m_s, "m/s");
                value(
                    ui,
                    "Air density",
                    &mut phase.condition.air_density_kg_m3,
                    "kg/m3",
                );
                value(ui, "Throttle", &mut phase.condition.throttle, "");
            });
        });
        ui.add_space(4.0);
    }
    if let Some(index) = remove {
        state.mission_phases.remove(index);
    }
    ui.horizontal(|ui| {
        if ui.button(tr("Add mission phase")).clicked() {
            state
                .mission_phases
                .push(alas_uav::propulsion_electric::ElectricMissionPhase {
                    name: tr("new phase"),
                    duration_s: 60.0,
                    condition: alas_uav::propulsion_electric::ElectricFlightCondition {
                        speed_m_s: state.objectives.cruise_speed_m_s,
                        air_density_kg_m3: state.model.air_density_kg_m3,
                        throttle: 0.95,
                    },
                });
        }
        if ui.button(tr("Restore standard mission phases")).clicked() {
            state.reset_advanced_mission();
        }
    });
}

pub(super) fn component_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    card(
        ui,
        "Reviewed component catalogue",
        Some("Selections are restricted to reviewed physical records; price and stock never enter the physics."),
        |ui| {
            let evidence_gaps = state.selected_evidence_gaps();
            if !evidence_gaps.is_empty() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr("Selected records are reviewable catalogue entries, but not yet complete optimizer evidence."),
                );
                ui.add_space(4.0);
            }
            ui.label(RichText::new(tr("Required hardware")).strong());
            ui.add_space(4.0);
            component_selectors(state, ui);
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Model-specific requirement")).strong());
            ui.add_sized(
                [ui.available_width(), ui.spacing().interact_size.y],
                TextEdit::singleline(&mut state.required_electronics_role)
                    .hint_text(tr("Required electronics function")),
            )
            .on_hover_text(tr(
                "The catalogue role that must be present for the mission electronics check.",
            ));
        },
    );
    let evidence_gaps = state.selected_evidence_gaps();
    if !evidence_gaps.is_empty() {
        ui.add_space(8.0);
        collapsing_card(
            ui,
            "uav_missing_evidence",
            "Missing required evidence",
            None,
            false,
            |ui| {
                for gap in &evidence_gaps {
                    ui.label(format!("- {gap}"));
                }
            },
        );
    }
    ui.add_space(8.0);
    selected_bom_preview(state, ui);
}

fn component_selectors(state: &mut UavWorkflowState, ui: &mut Ui) {
    let columns = card_column_count(ui.available_width());
    for row in ComponentRole::ALL.chunks(columns) {
        ui.columns(columns, |column_uis| {
            for (index, role) in row.iter().enumerate() {
                component_selector(state, &mut column_uis[index], *role);
            }
        });
        ui.add_space(6.0);
    }
}

fn component_selector(state: &mut UavWorkflowState, ui: &mut Ui, role: ComponentRole) {
    ui.label(RichText::new(tr(role.label())).strong());
    let records = state.records_for(role);
    let current = state.selections.get(role).to_owned();
    let current_text = records
        .iter()
        .find(|record| record.id == current)
        .map_or_else(
            || tr("No reviewed selection"),
            |record| record.model.clone(),
        );
    let mut selected = None;
    ComboBox::from_id_salt(format!("uav_component_{role:?}"))
        .width(ui.available_width().clamp(170.0, 380.0))
        .selected_text(current_text)
        .show_ui(ui, |ui| {
            for record in &records {
                let label = format!("{} - {}", record.manufacturer, record.model);
                if ui.selectable_label(record.id == current, label).clicked() {
                    selected = Some(record.id.clone());
                }
            }
        });
    if let Some(id) = selected {
        state.selections.set(role, id);
    }
    if let Some(record) = state.selected_record(role) {
        ui.horizontal_wrapped(|ui| {
            ui.weak(format!("{}: {}", tr("Source"), record.provenance.publisher));
            ui.hyperlink_to(tr("Open evidence"), &record.provenance.source_url);
        });
    }
}

fn selected_bom_preview(state: &UavWorkflowState, ui: &mut Ui) {
    collapsing_card(
        ui,
        "uav_selected_bom",
        "Current aircraft BOM and price estimate",
        Some(
            "Quantities follow the selected motor count. Price evidence is dated, remains in its original currency, and does not alter the physics.",
        ),
        false,
        |ui| {
            let bom = state.selected_aircraft_bom();
            let estimate = bom.procurement_estimate();
            for line in &bom.lines {
                let quoted = estimate
                    .lines
                    .iter()
                    .find(|item| item.component_id == line.selection.component_id);
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!(
                        "{} x{}: {}",
                        tr(line.category.label()),
                        line.selection.quantity,
                        line.selection.component_id
                    ));
                    if let Some(quote) = quoted.and_then(|item| item.quote.as_ref()) {
                        ui.label(format!(
                            "{} each",
                            currency_amount(quote.currency, quote.unit_price_minor)
                        ));
                        ui.hyperlink_to(tr("Open price evidence"), &quote.source_url);
                    } else {
                        ui.weak(tr("No dated price quote"));
                    }
                });
            }
            ui.add_space(4.0);
            for subtotal in &estimate.subtotals {
                ui.label(format!(
                    "{}: {}",
                    tr("Known subtotal"),
                    currency_amount(subtotal.currency, subtotal.amount_minor)
                ));
            }
            if !estimate.complete {
                ui.weak(tr(
                    "This is a partial hardware estimate. Shipping, tax, consumables, machining, and payload costs are not invented.",
                ));
            }
        },
    );
}

fn currency_amount(currency: alas_uav::Currency, amount_minor: u64) -> String {
    format!(
        "{} {}.{:02}",
        currency.code(),
        amount_minor / 100,
        amount_minor % 100
    )
}

pub(super) fn propulsion_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    card(
        ui,
        "Propulsion map evidence",
        Some(
            "The normal path resolves thrust, motor current, battery current, and power from the selected battery, motor, ESC, and APC performance table.",
        ),
        |ui| {
            propulsion_mode_tabs(state, ui);
            ui.add_space(6.0);
            form_grid(ui, "uav_propulsion_count_grid", |ui| {
                integer(ui, "Installed motor count", &mut state.propulsion_motor_count);
            });
        },
    );
    ui.add_space(8.0);
    match state.propulsion_input_mode {
        PropulsionInputMode::Automatic => automatic_propulsion_summary(state, ui),
        PropulsionInputMode::AdvancedManual => manual_propulsion_inputs(state, ui),
    }
}

fn propulsion_mode_tabs(state: &mut UavWorkflowState, ui: &mut Ui) {
    let modes = [
        PropulsionInputMode::Automatic,
        PropulsionInputMode::AdvancedManual,
    ];
    let columns = card_column_count(ui.available_width()).min(modes.len());
    for row in modes.chunks(columns) {
        ui.columns(columns, |column_uis| {
            for (index, mode) in row.iter().enumerate() {
                if column_uis[index]
                    .add_sized(
                        [column_uis[index].available_width(), 30.0],
                        selectable_button(tr(mode.label()), state.propulsion_input_mode == *mode),
                    )
                    .clicked()
                {
                    state.propulsion_input_mode = *mode;
                }
            }
        });
        ui.add_space(4.0);
    }
}

fn automatic_propulsion_summary(state: &UavWorkflowState, ui: &mut Ui) {
    card(ui, "Required hardware", None, |ui| {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            tr("The solver stays inside each APC table's speed and RPM bounds. A combination without enough source evidence stops with a typed error rather than an invented curve."),
        );
        ui.add_space(4.0);
        for role in [
            ComponentRole::Battery,
            ComponentRole::Motor,
            ComponentRole::Esc,
            ComponentRole::Propeller,
        ] {
            if let Some(record) = state.selected_record(role) {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{}: {}", tr(role.label()), record.model));
                    ui.hyperlink_to(tr("Open evidence"), &record.provenance.source_url);
                });
            }
        }
        ui.weak(tr(
            "One selected battery feeds every identical propulsor. Per-motor ratings remain per motor; pack current, mass, thrust, and BOM quantities are summed across the installed count.",
        ));
    });
}

fn manual_propulsion_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    ui.colored_label(
        ui.visuals().warn_fg_color,
        tr("Advanced fallback: enter measured or solver-derived points only when the automatic source-bounded model is not the required evidence. A retail static-thrust value is not accepted for cruise."),
    );
    ui.add_space(8.0);
    card(ui, "Propulsion data source or test ID", None, |ui| {
        ui.add_sized(
            [ui.available_width(), ui.spacing().interact_size.y],
            TextEdit::singleline(&mut state.propulsion_evidence)
                .hint_text(tr("Example: dyno-2026-03 or solver-case-17")),
        )
        .on_hover_text(tr(
            "Enter a test ID, report name, solver case, or source URL that supports both operating points.",
        ));
        ui.add_space(6.0);
        form_grid(ui, "uav_propulsion_manual_grid", |ui| {
            integer(
                ui,
                "Battery series cells",
                &mut state.propulsion_series_cells,
            );
        });
    });
    ui.add_space(8.0);
    if card_column_count(ui.available_width()) == 2 {
        ui.columns(2, |columns| {
            operating_point_card(state, &mut columns[0], true);
            operating_point_card(state, &mut columns[1], false);
        });
    } else {
        operating_point_card(state, ui, true);
        ui.add_space(8.0);
        operating_point_card(state, ui, false);
    }
    ui.add_space(6.0);
    ui.weak(tr(
        "The manual fallback retains the historic two-point energy surrogate; use the automatic catalogue solver for the multi-phase electrical simulation.",
    ));
}

fn operating_point_card(state: &mut UavWorkflowState, ui: &mut Ui, low_speed: bool) {
    let (title, description) = if low_speed {
        (
            "Low-speed operating point",
            "Use the speed entered as Maximum stall speed in Mission; static thrust alone is not enough.",
        )
    } else {
        (
            "Cruise operating point",
            "Use the cruise speed entered in Mission.",
        )
    };
    card(ui, title, Some(description), |ui| {
        form_grid(
            ui,
            if low_speed {
                "uav_low_speed_propulsion_grid"
            } else {
                "uav_cruise_propulsion_grid"
            },
            |ui| {
                if low_speed {
                    value(ui, "Low-speed thrust", &mut state.stall_thrust_n, "N");
                    value(ui, "Low-speed current", &mut state.stall_current_a, "A");
                    value(ui, "Low-speed power", &mut state.stall_power_w, "W");
                } else {
                    value(ui, "Cruise thrust", &mut state.cruise_thrust_n, "N");
                    value(ui, "Cruise current", &mut state.cruise_current_a, "A");
                    value(ui, "Cruise power", &mut state.cruise_power_w, "W");
                }
            },
        );
    });
}

fn form_grid(ui: &mut Ui, id: impl std::hash::Hash, contents: impl FnOnce(&mut Ui)) {
    Grid::new(id)
        .num_columns(2)
        .spacing([20.0, 8.0])
        .show(ui, contents);
}
