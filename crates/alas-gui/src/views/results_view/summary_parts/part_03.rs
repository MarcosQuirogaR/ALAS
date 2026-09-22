// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

#[cfg(test)]
mod tests {
    use super::findings::finding_margin;
    use super::{
        fuel_margin_rows, localized_propulsion_label, mass_triplet_kg, payload_summary_metrics,
        propulsion_metric_card, propulsion_summary_entries, static_margin_rows,
        status_banner_title, takeoff_mass_margin,
    };
    use crate::theme::{apply_theme, AppTheme};
    use alas_payload::layout::{LayoutSummary, PassengerSummary, PayloadLayout};
    use alas_pipeline::feasibility::MissionFuelStatus;
    use alas_pipeline::FindingCode;
    use egui::{Context, FontFamily, RawInput, Shape};

    fn collect_text_families(shape: &Shape, families: &mut Vec<(String, FontFamily)>) {
        match shape {
            Shape::Text(text) => {
                for section in &text.galley.job.sections {
                    families.push((
                        text.galley.job.text[section.byte_range.clone()].to_owned(),
                        section.format.font_id.family.clone(),
                    ));
                }
            }
            Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text_families(shape, families);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn propulsion_cards_follow_the_proportional_ui_font_family() {
        let context = Context::default();
        apply_theme(AppTheme::Dark, &context);
        let output = context.run(RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                propulsion_metric_card(ui, "Thermal efficiency (eta_t)", "\u{03b7}\u{209c} = 0.42");
            });
        });
        let mut families = Vec::new();
        for shape in &output.shapes {
            collect_text_families(&shape.shape, &mut families);
        }

        for label in ["Thermal efficiency (eta_t)", "\u{03b7}\u{209c} = 0.42"] {
            let family = families
                .iter()
                .find(|(text, _)| text == label)
                .unwrap_or_else(|| panic!("missing rendered summary text: {label}"))
                .1
                .clone();
            assert_eq!(family, FontFamily::Proportional, "{label}");
        }
    }

    fn passenger_layout() -> PayloadLayout {
        PayloadLayout {
            mode: alas_payload::layout::Mode::Passenger,
            items: Vec::new(),
            total_mass: 21_000.0,
            cg_x: 10.0,
            cg_y: 0.0,
            summary: LayoutSummary::Passenger(Box::new(PassengerSummary {
                total_pax: 204,
                seated_pax: 198,
                unseated_pax: 6,
                classes: vec![("Business", 18), ("Economy", 180)],
                lavatories: 4,
                galleys: 3,
                accessible_lavatories: 1,
                wheelchair_stowages: 1,
                exit_type: "A",
                exit_pairs: 4,
                exit_capacity: 220,
                max_certifiable_capacity: 220,
                geometric_capacity: 220,
                source_capacity_cap: None,
                source_exit_layout: None,
                capacity_binding: "geometry_exit_limit",
                payload_t: 21.0,
                seat_mass_t: 17.5,
                bag_mass_t: 2.8,
                belly_cargo_t: 0.7,
                hold_capacity_t: 8.0,
                hold_used_t: 3.5,
                hold_ulds: 5,
                aisle_width_m: 0.51,
                max_abreast: 6,
                n_aisles: 1,
                deck_utilization: vec![("main", 0.82)],
                cg_pct_mac: 25.4,
                double_deck: false,
            })),
        }
    }

    #[test]
    fn passenger_summary_metrics_come_from_the_built_layout_one_value_per_row() {
        let metrics = payload_summary_metrics(&passenger_layout());
        for (label, value) in [
            ("Seated passengers", "198"),
            ("Requested passengers", "204"),
            ("Hold load", "3.5 t"),
            ("Hold capacity", "8.0 t"),
            ("Hold ULDs", "5"),
            ("Galleys", "3"),
            ("Lavatories", "4"),
            ("Exit pairs", "4"),
            ("Accessible lavatories", "1"),
            ("Seats abreast", "6"),
        ] {
            assert!(
                metrics
                    .iter()
                    .any(|(candidate, candidate_value)| *candidate == label
                        && candidate_value == value),
                "{label} = {value} missing from {metrics:?}"
            );
        }
        assert!(metrics
            .iter()
            .any(|(label, value)| *label == "Cabin class mix" && value.contains("Business 18")));
        assert!(
            metrics.iter().all(|(_, value)| !value.contains(" / ")),
            "slash-joined value remains: {metrics:?}"
        );
    }

    #[test]
    fn static_margins_are_shown_together_in_percent_mac_with_an_explicit_missing_case() {
        let rows = static_margin_rows(Some(0.052), Some(0.081));
        assert_eq!(
            rows[0],
            ("Static margin (baseline)", "5.2 % MAC".to_owned())
        );
        assert_eq!(
            rows[1],
            ("Static margin (optimized)", "8.1 % MAC".to_owned())
        );
        let rows = static_margin_rows(Some(0.052), None);
        assert_eq!(rows[1].0, "Static margin (optimized)");
        assert_eq!(rows[1].1, "No optimized design in this run");
    }

    #[test]
    fn fuel_margin_rows_split_mass_and_share_and_name_the_stop_case() {
        let rows = fuel_margin_rows(MissionFuelStatus::Completed, 20_000.0, Some(15_000.0));
        assert_eq!(
            rows[0],
            ("Fuel margin at destination", "+5.00 t".to_owned())
        );
        assert_eq!(
            rows[1],
            ("Fuel margin, share of carried fuel", "+25.0 %".to_owned())
        );
        let rows = fuel_margin_rows(MissionFuelStatus::Exhausted, 20_000.0, Some(20_000.0));
        assert_eq!(rows[0].0, "Fuel margin at stop");
        let rows = fuel_margin_rows(MissionFuelStatus::NotRequested, f64::NAN, None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "Not evaluated");
    }

    #[test]
    fn takeoff_mass_margin_reads_as_one_value() {
        assert_eq!(takeoff_mass_margin(0.2), "At MTOW");
        assert_eq!(takeoff_mass_margin(1_250.0), "1.25 t below MTOW");
        assert_eq!(takeoff_mass_margin(f64::NAN), "Not established");
    }

    #[test]
    fn aircraft_mass_summary_excludes_payload_and_fuel_from_oew() {
        let masses = std::collections::HashMap::from([
            ("Wing".to_owned(), 12_000.0),
            ("Fuselage".to_owned(), 8_000.0),
            ("Payload".to_owned(), 6_000.0),
            ("Fuel".to_owned(), 4_000.0),
        ]);
        assert_eq!(
            mass_triplet_kg(&masses, 29_000.0, 32_000.0),
            Some((20_000.0, 29_000.0, 32_000.0))
        );
    }

    #[test]
    fn propulsion_summary_splits_compact_cycle_metrics_into_independent_cards() {
        let lines = vec![
            "Engine: Demo (high-bypass turbofan)".to_owned(),
            "BPR = 8.0    OPR = 32.0    FPR = 1.6    TIT = 1450 K".to_owned(),
            "Thermal efficiency (eta_t) = 0.42".to_owned(),
        ];
        let entries = propulsion_summary_entries(&lines);
        assert_eq!(entries.len(), 6);
        assert!(entries
            .iter()
            .any(|(label, value)| { label == "BPR" && value == "8.0" }));
        assert!(entries
            .iter()
            .any(|(label, value)| { label == "TIT" && value == "1450 K" }));
        assert!(entries
            .iter()
            .any(|(label, value)| { label == "Thermal efficiency (eta_t)" && value == "0.42" }));
    }

    #[test]
    fn propulsion_summary_localizes_all_catalogued_engine_labels_without_gluing_suffixes() {
        assert_eq!(
            localized_propulsion_label("TSFC (computed)"),
            "TSFC (computed)"
        );
        assert_eq!(
            localized_propulsion_label("Per-engine thrust, this cruise pt"),
            "Per-engine thrust, this cruise pt"
        );
        assert_eq!(
            localized_propulsion_label("Total installed thrust (x4)"),
            "Total installed thrust (x4)"
        );
    }

    #[test]
    fn propulsion_summary_uses_existing_spanish_cycle_labels() {
        alas_i18n::es::install();
        alas_i18n::set_language(Some("es"));

        assert_eq!(
            localized_propulsion_label("TSFC (computed)"),
            "TSFC (calculado)"
        );
        assert_eq!(
            localized_propulsion_label("Per-engine thrust, this cruise pt"),
            "Empuje por motor, en este punto de crucero"
        );
        assert_eq!(
            localized_propulsion_label("Total installed thrust (x4)"),
            "Empuje total instalado (x4)"
        );

        alas_i18n::set_language(Some("en"));
    }

    #[test]
    fn finding_margins_are_negative_on_both_upper_and_lower_bound_failures() {
        assert_eq!(
            finding_margin(FindingCode::MissionFuelShortfall, 51_410.0, 50_400.0),
            -1_010.0
        );
        assert!(
            (finding_margin(FindingCode::InsufficientStaticMargin, 0.03, 0.05) + 0.02).abs()
                < 1.0e-12
        );
    }

    #[test]
    fn incomplete_snapshots_never_receive_a_feasibility_verdict() {
        assert_eq!(
            status_banner_title(false, 0, 0),
            "Assessment incomplete"
        );
        assert_ne!(
            status_banner_title(false, 0, 0),
            "Feasible under implemented checks"
        );
        assert_eq!(
            status_banner_title(true, 0, 0),
            "Feasible under implemented checks"
        );
    }
}
