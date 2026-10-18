// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bill of materials and price estimate of the selected competition design.

use alas_uav::optimizer::SelectedComponents;
use alas_uav::{estimate_optimized_aircraft_cost, optimized_aircraft_bom, Currency};
use egui::Ui;

use super::super::tr;
use super::presentation::card;

pub(super) fn aircraft_bom(components: &SelectedComponents, ui: &mut Ui) {
    card(ui, "Aircraft BOM and price estimate", None, |ui| {
        let bom = optimized_aircraft_bom(components);
        let estimate = estimate_optimized_aircraft_cost(components);
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
            ui.weak(tr("Partial price estimate")).on_hover_text(tr(
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
