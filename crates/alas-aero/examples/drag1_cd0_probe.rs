// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per-preset parasite drag at the cruise point, component by component
//! (before/after the product parasite items).

use alas_aero::analysis::AeroAnalysis;
use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;

// A command-line diagnostic writes its comparison table to standard output.
#[allow(clippy::print_stdout)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "preset,mach,altitude_m,margin,cd_components,cd_drag_areas,cd_miscellaneous,cd0,swet_over_sref,cfe,swet_smooth_over_sref,items"
    );
    for name in presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name}))?;
        let dv = presets::get(name)?.design_vector;
        let plane = AircraftBuilder::new(Some(config.geometry.clone())).build(Some(&dv), true)?;
        let drag_model = config.resolved_drag_model();
        let margin = drag_model.margin();
        let analysis = AeroAnalysis::new(
            &plane,
            dv.sweep_deg,
            Some(config.geometry),
            Some(drag_model),
            Some(config.analysis),
        );
        let mach = config.requirements.cruise_mach;
        let altitude = config.requirements.cruise_altitude_m;
        let breakdown = analysis.parasite_breakdown(mach, altitude, None, None);
        let swet_ratio = breakdown.wetted_area_ratio(plane.s_ref);
        // Wetted area without the pylon and fairing items: the base of
        // the before-change equivalent skin friction.
        let smooth_ratio = breakdown
            .components
            .iter()
            .filter(|c| c.name != "Pylon" && c.name != "Wing-body fairing")
            .map(|c| c.wetted_area_m2)
            .sum::<f64>()
            / plane.s_ref;
        let items: Vec<String> = breakdown
            .components
            .iter()
            .map(|c| format!("{}={:.6}", c.name, c.cd))
            .chain(
                breakdown
                    .drag_areas
                    .iter()
                    .map(|d| format!("{}={:.6} ({:.4} m2)", d.name, d.cd, d.drag_area_m2)),
            )
            .collect();
        println!(
            "{name},{mach},{altitude},{margin},{:.6},{:.6},{:.6},{:.6},{:.4},{:.6},{:.4},{}",
            breakdown.cd_components,
            breakdown.cd_drag_areas,
            breakdown.cd_miscellaneous,
            breakdown.cd0,
            swet_ratio,
            breakdown.cd0 / swet_ratio,
            smooth_ratio,
            items.join(";")
        );
    }
    Ok(())
}
