// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

fn selected_analysis(result: &alas_pipeline::PipelineResult) -> Option<&AnalysisReport> {
    result
        .optimized_report
        .as_ref()
        .or(result.baseline_analysis.as_ref())
}

fn mass_triplet_kg(
    component_masses: &std::collections::HashMap<String, f64>,
    tow_kg: f64,
    mtow_kg: f64,
) -> Option<(f64, f64, f64)> {
    let total_kg: f64 = component_masses.values().copied().sum();
    let payload_kg = component_masses.get("Payload").copied().unwrap_or(0.0);
    let fuel_kg = component_masses.get("Fuel").copied().unwrap_or(0.0);
    let oew_kg = total_kg - payload_kg - fuel_kg;
    (oew_kg.is_finite()
        && oew_kg > 0.0
        && tow_kg.is_finite()
        && tow_kg > 0.0
        && mtow_kg.is_finite()
        && mtow_kg > 0.0)
        .then_some((oew_kg, tow_kg, mtow_kg))
}

fn result_payload_layout(result: &alas_pipeline::PipelineResult) -> Option<&PayloadLayout> {
    result
        .optimized_report
        .as_ref()
        .and_then(|report| report.payload_layout.as_ref())
        .or_else(|| {
            result
                .baseline_analysis
                .as_ref()
                .and_then(|report| report.payload_layout.as_ref())
        })
        .or_else(|| {
            result
                .baseline_report
                .as_ref()
                .and_then(|report| report.payload_layout.as_ref())
        })
}

fn tonnes(value_t: f64) -> String {
    format!("{value_t:.1} t")
}

/// One labelled row per value; no slash-joined composites.
fn payload_summary_metrics(layout: &PayloadLayout) -> Vec<(&'static str, String)> {
    match &layout.summary {
        LayoutSummary::Passenger(summary) => {
            let classes = summary
                .classes
                .iter()
                .filter(|(_, seats)| *seats > 0)
                .map(|(class, seats)| format!("{} {seats}", tr(class)))
                .collect::<Vec<_>>()
                .join(", ");
            vec![
                ("Seated passengers", summary.seated_pax.to_string()),
                ("Requested passengers", summary.total_pax.to_string()),
                ("Unseated passengers", summary.unseated_pax.to_string()),
                ("Cabin class mix", classes),
                ("Passenger payload", tonnes(summary.payload_t)),
                ("Hold load", tonnes(summary.hold_used_t)),
                ("Hold capacity", tonnes(summary.hold_capacity_t)),
                ("Hold ULDs", summary.hold_ulds.to_string()),
                ("Checked bags", tonnes(summary.bag_mass_t)),
                ("Belly freight", tonnes(summary.belly_cargo_t)),
                ("Seats abreast", summary.max_abreast.to_string()),
                ("Aisles", summary.n_aisles.to_string()),
                (
                    "Decks",
                    tr(if summary.double_deck {
                        "Double deck"
                    } else {
                        "Single deck"
                    }),
                ),
                ("Galleys", summary.galleys.to_string()),
                ("Lavatories", summary.lavatories.to_string()),
                ("Exit pairs", summary.exit_pairs.to_string()),
                (
                    "Accessible lavatories",
                    summary.accessible_lavatories.to_string(),
                ),
                (
                    "Wheelchair stowages",
                    summary.wheelchair_stowages.to_string(),
                ),
                ("Payload CG", format!("{:.1}% MAC", summary.cg_pct_mac)),
            ]
        }
        LayoutSummary::Cargo(summary) => vec![
            ("Cargo payload", tonnes(summary.payload_t)),
            ("Net cargo loaded", tonnes(summary.loaded_net_payload_t)),
            (
                "Net cargo requested",
                tonnes(summary.requested_net_payload_t),
            ),
            ("ULD tare", tonnes(summary.tare_mass_t)),
            ("ULDs loaded", summary.n_ulds.to_string()),
            ("ULD positions", summary.n_slots.to_string()),
            ("Cargo capacity", tonnes(summary.capacity_t)),
            ("Cargo capacity used", format!("{:.1} %", summary.fill_pct)),
            ("Cargo volume", format!("{:.1} m^3", summary.volume_m3)),
            ("Main deck ULDs", summary.n_main_deck.to_string()),
            ("Lower deck ULDs", summary.n_lower_deck.to_string()),
            (
                "Payload CG",
                format!("{:.1}% MAC", summary.achieved_cg_pct_mac),
            ),
            ("Loading strategy", tr(&summary.strategy)),
        ],
    }
}
