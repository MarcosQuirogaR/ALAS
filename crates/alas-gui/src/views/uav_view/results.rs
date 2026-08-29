// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Electrical mission and procurement result panels for the UAV workflow.

use alas_uav::optimizer::OptimizedUav;
use alas_uav::{estimate_optimized_aircraft_cost, optimized_aircraft_bom, Currency};
use egui::{Grid, Ui};

use super::super::{tr, tr_fields};
use super::presentation::{card, card_column_count};
use super::uav_fields::result_measure;

pub(super) fn electrical_mission(
    mission: &alas_uav::propulsion_electric::ElectricMissionResult,
    ui: &mut Ui,
) {
    card(ui, "Electrical mission simulation", None, |ui| {
        ui.weak(tr_fields(
            "Source: {evidence}",
            &[("evidence", mission.evidence.clone())],
        ));
        ui.add_space(4.0);
        Grid::new("uav_electrical_mission_summary")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                result_measure(ui, "Mission duration", mission.total_duration_s, "s");
                result_measure(ui, "Mission distance", mission.total_distance_m, "m");
                result_measure(ui, "Propulsion energy", mission.propulsion_energy_wh, "Wh");
                result_measure(
                    ui,
                    "Maximum battery current",
                    mission.maximum_battery_current_a,
                    "A",
                );
                result_measure(
                    ui,
                    "Maximum battery power",
                    mission.maximum_battery_power_w,
                    "W",
                );
            });
        ui.add_space(4.0);
        ui.weak(tr(
            "This is a source-bounded preliminary electrical calculation, not a flight-test, airworthiness, or certification result.",
        ));
    });
    ui.add_space(8.0);
    let columns = card_column_count(ui.available_width());
    for row in mission.phases.chunks(columns) {
        if columns == 1 {
            electrical_phase_card(&row[0], ui);
        } else {
            ui.columns(columns, |column_uis| {
                for (index, phase) in row.iter().enumerate() {
                    electrical_phase_card(phase, &mut column_uis[index]);
                }
            });
        }
        ui.add_space(8.0);
    }
}

fn electrical_phase_card(
    phase: &alas_uav::propulsion_electric::ElectricMissionPhaseResult,
    ui: &mut Ui,
) {
    card(ui, &phase.phase.name, None, |ui| {
        Grid::new(("uav_electrical_mission_phase", &phase.phase.name))
            .num_columns(2)
            .spacing([16.0, 7.0])
            .show(ui, |ui| {
                result_measure(ui, "Duration", phase.phase.duration_s, "s");
                result_measure(ui, "Speed", phase.phase.condition.speed_m_s, "m/s");
                result_measure(ui, "Low-speed thrust", phase.propulsion.total_thrust_n, "N");
                result_measure(
                    ui,
                    "Battery current",
                    phase.propulsion.battery_current_a,
                    "A",
                );
                result_measure(ui, "Propulsion energy", phase.propulsion_energy_wh, "Wh");
            });
    });
}

pub(super) fn aircraft_bom(optimized: &OptimizedUav, ui: &mut Ui) {
    card(ui, "Aircraft BOM and price estimate", None, |ui| {
        let bom = optimized_aircraft_bom(&optimized.components);
        let estimate = estimate_optimized_aircraft_cost(&optimized.components);
        for line in &bom.lines {
            let price = estimate
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
                if let Some(quote) = price.and_then(|item| item.quote.as_ref()) {
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
                "The displayed total is partial: shipping, tax, consumables, machining, and payload costs are not inferred.",
            ));
        }
    });
}

fn currency_amount(currency: Currency, amount_minor: u64) -> String {
    format!(
        "{} {}.{:02}",
        currency.code(),
        amount_minor / 100,
        amount_minor % 100
    )
}
