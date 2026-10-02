// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mesh-sensitivity and payload-range studies.

use super::*;

/// Least-squares slope and intercept of `y` against `x`.
pub(super) fn lin(x: &[f64], y: &[f64]) -> (f64, f64) {
    let n = x.len() as f64;
    let mx = x.iter().sum::<f64>() / n;
    let my = y.iter().sum::<f64>() / n;
    let num: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let den: f64 = x.iter().map(|a| (a - mx).powi(2)).sum();
    (num / den, my - num / den * mx)
}

/// Coarse-versus-fine mesh sensitivity of the cruise polar, so a reported L/D
/// can be told apart from a mesh artifact.
#[allow(dead_code)]
pub(super) fn mesh_study(name: &str) -> Result<Value, String> {
    let preset = presets::get(name).map_err(|e| e.to_string())?;
    let config = AlasConfig::from_value(&json!({ "preset": name })).map_err(|e| e.to_string())?;
    let dv = &preset.design_vector;
    let report = FullAnalysis::new(config.clone())
        .run(dv, true)
        .map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for (label, span, chord) in [
        (
            "coarse",
            config.analysis.spanwise_resolution,
            config.analysis.chordwise_resolution,
        ),
        (
            "fine",
            config.analysis.fine_spanwise_resolution,
            config.analysis.fine_chordwise_resolution,
        ),
        (
            "2x fine",
            config.analysis.fine_spanwise_resolution * 2,
            config.analysis.fine_chordwise_resolution * 2,
        ),
    ] {
        let mut analysis = config.analysis.clone();
        analysis.spanwise_resolution = span;
        analysis.chordwise_resolution = chord;
        let sweep = AeroAnalysis::new(
            &report.airplane,
            AeroAnalysis::quarter_chord_sweep_deg(&report.airplane, dv.sweep_deg),
            Some(config.geometry.clone()),
            Some(config.drag_model.clone()),
            Some(analysis),
        )
        .run_sweep(
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
        )
        .map_err(|e| e.to_string())?;
        // interpolate L/D at the reported cruise CL
        let target = report.design_point.cl;
        let mut best = (f64::INFINITY, 0usize);
        for (i, &cl) in sweep.cl.iter().enumerate() {
            let d = (cl - target).abs();
            if d < best.0 {
                best = (d, i);
            }
        }
        rows.push(json!({
            "mesh": label,
            "spanwise": span,
            "chordwise": chord,
            "cl": sweep.cl[best.1],
            "cd": sweep.cd[best.1],
            "l_over_d": sweep.l_over_d[best.1],
        }));
    }
    Ok(json!({ "preset": name, "cruise_cl": report.design_point.cl, "meshes": rows }))
}

/// The A-B-C-D payload-range corners of the report figure and the sandbox,
/// from the shared corner solver
/// (`alas_pipeline::quick_analysis::payload_range_corners`): every corner
/// the largest still-air range whose reserve-inclusive fuel plan fits its
/// fuel, on the report's segment mission model.
pub(super) fn payload_range(
    config: &AlasConfig,
    report: &alas_pipeline::full_analysis::AnalysisReport,
) -> Option<Value> {
    const M_TO_NM: f64 = 1852.0;
    let quick = alas_pipeline::quick_analysis::payload_range_corners(config, report).ok()?;
    let labels = ["A", "B", "C", "D"];
    let points: Vec<Value> = quick
        .points
        .iter()
        .zip(labels)
        .zip(&quick.reserve_fuel_kg)
        .map(|((&(range_m, payload_kg), label), &reserve_kg)| {
            json!({
                "label": label,
                "range_nm": range_m / M_TO_NM,
                "range_km": range_m / 1000.0,
                "payload_kg": payload_kg,
                "reserve_fuel_kg": reserve_kg,
            })
        })
        .collect();
    Some(json!({
        "corners": points,
        "fuel_capacity_kg": quick.fuel_capacity_kg,
        "fuel_capacity_limit": quick.fuel_capacity_basis,
        "max_payload_kg": quick.max_payload_kg,
        "oew_kg": quick.oew_kg,
        "mtow_kg": quick.mtow_kg,
        "method": quick.note,
    }))
}
