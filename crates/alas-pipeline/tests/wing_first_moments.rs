// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The delivered item ledger and the trim groups place the same wing mass
//! at the same complete-wing station, in the aircraft basic frame (SI).

use alas_config::{presets, AlasConfig};
use alas_pipeline::{assess_physical_feasibility, FullAnalysis};

#[test]
fn delivered_and_lumped_wings_have_identical_first_moments(
) -> Result<(), Box<dyn std::error::Error>> {
    for name in ["AVE", "A320-200", "B787-9", "A220-300"] {
        for geometric_stations in [true, false] {
            let mut config = AlasConfig::from_value(&serde_json::json!({"preset": name}))?;
            config.mass_model.geometric_component_stations = geometric_stations;
            config.mission.enabled = false;
            config.mses.enabled = false;
            config.structures.enabled = false;
            let design = presets::get(name)?.design_vector;
            let report = FullAnalysis::new(config.clone()).run(&design, true)?;
            let physical = assess_physical_feasibility(&config, &design, &report, None);
            let statement = physical
                .mass_balance
                .as_ref()
                .ok_or("missing item ledger")?;
            let wing = statement
                .ledger_items
                .iter()
                .find(|item| item.id == "wing")
                .ok_or("missing wing ledger row")?;
            let mass = report.component_masses["Wing"];
            let centroid = report.mass_coordinates["Wing"];
            assert_eq!(wing.mass_kg, mass, "{name}: wing mass");
            assert_eq!(wing.position_m, centroid, "{name}: shared wing station");
            for (axis, coordinate) in centroid.into_iter().enumerate() {
                assert_eq!(
                    wing.mass_kg * wing.position_m[axis],
                    mass * coordinate,
                    "{name}: wing first moment, axis {axis}"
                );
            }
        }
    }
    Ok(())
}
