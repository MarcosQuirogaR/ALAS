// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Quarter-chord source constraints must preserve the area-calibrated wing shape.

use alas_config::presets;

// Preset lookup and geometry construction errors are assertion failures.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[test]
fn published_quarter_chord_sweep_does_not_change_span_or_chord_distribution() {
    for (name, source_c4, previous_le, source_area) in [
        ("A380-800", 33.5_f64, 33.5, 845.0),
        ("DC-10", 35.0_f64, 35.0, 338.84),
    ] {
        let preset = presets::get(name).unwrap();
        let wing = &preset.geometry.wing;
        let design = &preset.design_vector;
        let corrected = wing.transport_planform(design).unwrap();
        let mut previous_wing = wing.clone();
        previous_wing.side_of_body_chord_ratio = None;
        let mut previous_design = *design;
        previous_design.sweep_deg = previous_le;
        let previous = previous_wing.transport_planform(&previous_design).unwrap();

        let outboard_c4 = ((corrected.tip.leading_edge_x_m + 0.25 * corrected.tip.chord_m
            - corrected.kink.leading_edge_x_m
            - 0.25 * corrected.kink.chord_m)
            / (corrected.tip.y_m - corrected.kink.y_m))
            .atan()
            .to_degrees();
        assert!(
            (outboard_c4 - source_c4).abs() < 1e-12,
            "{name} source c/4 sweep"
        );

        let corrected_stations = corrected.stations();
        let previous_stations = previous.stations();
        assert_eq!(corrected_stations.len(), previous_stations.len());
        for (current, previous) in corrected_stations.iter().zip(&previous_stations) {
            assert_eq!(current.y_m, previous.y_m, "{name} station span");
            assert!(
                (current.chord_m - previous.chord_m).abs() < 1e-12,
                "{name} chord at {} m",
                current.y_m
            );
        }
        let area = |stations: &[alas_config::MainWingStation]| {
            stations
                .windows(2)
                .map(|pair| (pair[1].y_m - pair[0].y_m) * (pair[0].chord_m + pair[1].chord_m))
                .sum::<f64>()
        };
        assert!((area(&corrected_stations) - area(&previous_stations)).abs() < 1e-10);
        assert!(
            (area(&corrected_stations) - source_area).abs() < 1e-9,
            "{name} published gross projected area"
        );
    }
}
