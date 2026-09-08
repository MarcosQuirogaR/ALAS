// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn selected_analysis(result: &alas_pipeline::PipelineResult) -> Option<&AnalysisReport> {
    result
        .optimized_report
        .as_ref()
        .or(result.baseline_analysis.as_ref())
}

fn aircraft_summary_metrics(result: &alas_pipeline::PipelineResult) -> Vec<(&'static str, String)> {
    let mut metrics = Vec::new();
    let fuel = &result.feasibility.fuel_loading;
    if let Some(report) = selected_analysis(result) {
        if let Some((oew_kg, tow_kg, mtow_kg)) = mass_triplet_kg(
            &report.component_masses,
            fuel.analyzed_takeoff_mass_kg,
            result.config.requirements.mtow_kg,
        ) {
            metrics.push((
                "Masses (OEW / TOW / MTOW)",
                format!(
                    "{:.1} / {:.1} / {:.1} t",
                    oew_kg / 1_000.0,
                    tow_kg / 1_000.0,
                    mtow_kg / 1_000.0
                ),
            ));
        }
        if report.airplane.b_ref.is_finite()
            && report.airplane.b_ref > 0.0
            && report.airplane.s_ref.is_finite()
            && report.airplane.s_ref > 0.0
        {
            metrics.push((
                "Wing geometry",
                tr_fields(
                    "{span} m span | {area} m^2 area",
                    &[
                        ("span", format!("{:.1}", report.airplane.b_ref)),
                        ("area", format!("{:.1}", report.airplane.s_ref)),
                    ],
                ),
            ));
        }
        let (l_over_d, provenance) = report
            .trimmed_design_point
            .as_ref()
            .map(|point| (point.l_over_d, "trimmed"))
            .unwrap_or((report.design_point.l_over_d, "untrimmed"));
        if l_over_d.is_finite() && l_over_d > 0.0 {
            metrics.push(("Cruise L/D", format!("{l_over_d:.1} ({})", tr(provenance))));
        }
    }

    metrics
}

fn fuel_detail_metrics(result: &alas_pipeline::PipelineResult) -> Vec<(&'static str, String)> {
    let fuel = &result.feasibility.fuel_loading;
    let mut metrics = Vec::new();
    if fuel.analyzed_carried_fuel_kg.is_finite() && fuel.analyzed_carried_fuel_kg >= 0.0 {
        let capacity = match fuel.usable_capacity.capacity_kg {
            Some(capacity_kg) if capacity_kg.is_finite() && capacity_kg >= 0.0 => tr_fields(
                "{capacity} t ({evidence})",
                &[
                    ("capacity", format!("{:.1}", capacity_kg / 1_000.0)),
                    (
                        "evidence",
                        tr(match fuel.usable_capacity.evidence {
                            FuelCapacityEvidence::PublishedPreset => "published",
                            FuelCapacityEvidence::GeometryEstimate => "geometry estimate",
                            FuelCapacityEvidence::Unavailable => "unavailable",
                        }),
                    ),
                ],
            ),
            _ => tr("unavailable"),
        };
        metrics.push((
            "Fuel carried / usable capacity",
            tr_fields(
                "{carried} t / {capacity}",
                &[
                    (
                        "carried",
                        format!("{:.1}", fuel.analyzed_carried_fuel_kg / 1_000.0),
                    ),
                    ("capacity", capacity),
                ],
            ),
        ));
    }
    if fuel.zero_fuel_mass_kg.is_finite() && fuel.zero_fuel_mass_kg >= 0.0 {
        metrics.push((
            "Zero-fuel mass / MTOW fuel budget",
            format!(
                "{:.1} t / {:.1} t",
                fuel.zero_fuel_mass_kg / 1_000.0,
                fuel.mtow_closure_fuel_kg / 1_000.0
            ),
        ));
    }
    if let Some(burned_kg) = fuel
        .mission
        .burned_fuel_kg
        .filter(|value| value.is_finite())
    {
        metrics.push((
            "Mission burn from available telemetry",
            format!(
                "{:.2} t | {}",
                burned_kg / 1_000.0,
                mission_status_label(result)
            ),
        ));
    }
    metrics
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

fn payload_summary_metrics(layout: &PayloadLayout) -> Vec<(&'static str, String)> {
    match &layout.summary {
        LayoutSummary::Passenger(summary) => {
            let classes = summary
                .classes
                .iter()
                .filter(|(_, seats)| *seats > 0)
                .map(|(class, seats)| format!("{} {seats}", tr(class)))
                .collect::<Vec<_>>()
                .join(" | ");
            vec![
                (
                    "Seating capacity",
                    tr_fields(
                        "{seated} / {requested} seats requested",
                        &[
                            ("seated", summary.seated_pax.to_string()),
                            ("requested", summary.total_pax.to_string()),
                        ],
                    ),
                ),
                ("Unseated passengers", summary.unseated_pax.to_string()),
                ("Cabin class mix", classes),
                ("Passenger payload", format!("{:.1} t", summary.payload_t)),
                (
                    "Hold loading",
                    format!(
                        "{:.1} / {:.1} t | {} ULD",
                        summary.hold_used_t, summary.hold_capacity_t, summary.hold_ulds
                    ),
                ),
                (
                    "Checked bags / belly freight",
                    format!("{:.1} / {:.1} t", summary.bag_mass_t, summary.belly_cargo_t),
                ),
                (
                    "Cabin arrangement",
                    tr_fields(
                        "{abreast} abreast | {aisles} aisle(s) | {decks}",
                        &[
                            ("abreast", summary.max_abreast.to_string()),
                            ("aisles", summary.n_aisles.to_string()),
                            (
                                "decks",
                                tr(if summary.double_deck {
                                    "double deck"
                                } else {
                                    "single deck"
                                }),
                            ),
                        ],
                    ),
                ),
                (
                    "Galleys / lavatories / exit pairs",
                    format!(
                        "{} / {} / {}",
                        summary.galleys, summary.lavatories, summary.exit_pairs
                    ),
                ),
                (
                    "Accessibility provisions",
                    format!(
                        "{} accessible lavatory / {} wheelchair stowage",
                        summary.accessible_lavatories, summary.wheelchair_stowages
                    ),
                ),
                ("Payload CG", format!("{:.1}% MAC", summary.cg_pct_mac)),
            ]
        }
        LayoutSummary::Cargo(summary) => vec![
            ("Cargo payload", format!("{:.1} t", summary.payload_t)),
            (
                "Net cargo / requested",
                format!(
                    "{:.1} / {:.1} t",
                    summary.loaded_net_payload_t, summary.requested_net_payload_t
                ),
            ),
            ("ULD tare", format!("{:.1} t", summary.tare_mass_t)),
            (
                "ULD loading",
                tr_fields(
                    "{loaded} / {slots} positions",
                    &[
                        ("loaded", summary.n_ulds.to_string()),
                        ("slots", summary.n_slots.to_string()),
                    ],
                ),
            ),
            (
                "Cargo capacity",
                format!("{:.1} t | {:.1}%", summary.capacity_t, summary.fill_pct),
            ),
            ("Cargo volume", format!("{:.1} m^3", summary.volume_m3)),
            (
                "Main / lower deck ULDs",
                format!("{} / {}", summary.n_main_deck, summary.n_lower_deck),
            ),
            (
                "Payload CG",
                format!("{:.1}% MAC", summary.achieved_cg_pct_mac),
            ),
            ("Loading strategy", tr(&summary.strategy)),
        ],
    }
}
