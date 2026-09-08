// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Quantify the exposed-main-wing correction against the gross-area convention.

use alas_aero::analysis::AeroAnalysis;
use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;

// A command-line diagnostic writes its comparison table to standard output.
#[allow(clippy::print_stdout)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("preset,mach,altitude_m,cd_parasite_gross,cd_parasite_exposed,change_percent");
    for name in presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name}))?;
        let dv = presets::get(name)?.design_vector;
        let plane = AircraftBuilder::new(Some(config.geometry.clone())).build(Some(&dv), true)?;
        let mut analysis = AeroAnalysis::new(
            &plane,
            dv.sweep_deg,
            Some(config.geometry),
            Some(config.drag_model),
            Some(config.analysis),
        );
        let mach = config.requirements.cruise_mach;
        let altitude = config.requirements.cruise_altitude_m;
        analysis.drag.exclude_buried_main_wing_area = false;
        let gross = analysis.parasite_drag(mach, altitude, 0.0, None, None);
        analysis.drag.exclude_buried_main_wing_area = true;
        let exposed = analysis.parasite_drag(mach, altitude, 0.0, None, None);
        println!(
            "{name},{mach},{altitude},{gross:.8},{exposed:.8},{:.3}",
            100.0 * (exposed / gross - 1.0)
        );
    }
    Ok(())
}
