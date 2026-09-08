// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


#[cfg(test)]
mod tests {
    use super::{finding_margin, mass_triplet_kg, payload_summary_metrics};
    use alas_payload::layout::{LayoutSummary, PassengerSummary, PayloadLayout};
    use alas_pipeline::FindingCode;

    #[test]
    fn passenger_summary_metrics_come_from_the_built_layout() {
        let layout = PayloadLayout {
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
        };
        let metrics = payload_summary_metrics(&layout);
        assert!(metrics.iter().any(|(label, value)| {
            *label == "Seating capacity" && value == "198 / 204 seats requested"
        }));
        assert!(metrics
            .iter()
            .any(|(label, value)| *label == "Cabin class mix" && value.contains("Business 18")));
        assert!(metrics
            .iter()
            .any(|(label, value)| *label == "Hold loading" && value.contains("3.5 / 8.0 t")));
        assert!(metrics.iter().any(|(label, value)| {
            *label == "Accessibility provisions" && value.contains("1 accessible lavatory")
        }));
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
}

