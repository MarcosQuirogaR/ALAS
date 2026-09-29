// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Component selectors and the selected bill-of-materials preview.

use egui::{ComboBox, RichText, TextEdit, Ui};

use crate::uav::{ComponentRole, UavWorkflowState};

use super::super::presentation::{card, card_column_count, collapsing_card};
use crate::views::tr;

pub(in crate::views::uav_view) fn component_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    card(
        ui,
        "Reviewed component catalogue",
        Some("Selections are restricted to reviewed physical records; price and stock never enter the physics."),
        |ui| {
            let evidence_gaps = state.selected_evidence_gaps();
            if !evidence_gaps.is_empty() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr("Selected records lack complete optimizer evidence"),
                )
                .on_hover_text(tr("Selected records are reviewable catalogue entries, but not yet complete optimizer evidence."));
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
                ui.weak(tr("Partial hardware estimate")).on_hover_text(tr(
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
