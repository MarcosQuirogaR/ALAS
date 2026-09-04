// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

/// Coarse-versus-fine mesh sensitivity of the cruise polar, so a reported L/D
/// can be told apart from a mesh artifact.
#[allow(dead_code)]
fn mesh_study(name: &str) -> Result<Value, String> {
    let preset = presets::get(name).map_err(|e| e.to_string())?;
    let config = AlasConfig::from_value(&json!({ "preset": name })).map_err(|e| e.to_string())?;
    let dv = &preset.design_vector;
    let report = FullAnalysis::new(config.clone())
        .run(dv, false)
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
            report.design.sweep_deg,
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

/// The same A-B-C-D Breguet curve the report figure draws, computed locally.
///
/// Duplicated rather than imported so this validation instrument stays
/// buildable while the report crate is being worked on elsewhere. If the two
/// ever disagree, the figure is authoritative and this is the bug.
fn payload_range(
    config: &AlasConfig,
    report: &alas_pipeline::full_analysis::AnalysisReport,
    oew_kg: f64,
    _s_ref: f64,
) -> Option<Value> {
    const G: f64 = 9.81;
    const M_TO_NM: f64 = 1852.0;
    let mtow_kg = config.requirements.mtow_kg;
    let published_mzfw_payload = presets::get(&config.preset)
        .ok()
        .and_then(|p| p.reference.mzfw_kg)
        .map(|mzfw| mzfw - oew_kg);
    let effective = report
        .geometry_summary
        .get("effective_structural_payload_limit_kg")
        .copied();
    let max_payload_kg = [
        config.requirements.max_structural_payload_kg,
        effective.unwrap_or(f64::NAN),
        published_mzfw_payload.unwrap_or(f64::NAN),
    ]
    .into_iter()
    .filter(|v| v.is_finite() && *v > 0.0)
    .reduce(f64::min)?;

    let fuel = alas_pipeline::feasibility::assess_fuel_capacity(config, &report.design, report);
    let mut fuel_capacity_kg = fuel.capacity_kg.filter(|v| v.is_finite())?;
    let structural_capacity_kg = (mtow_kg - oew_kg).max(0.0);
    let mut limit = "published usable capacity";
    if fuel_capacity_kg > structural_capacity_kg {
        fuel_capacity_kg = structural_capacity_kg;
        limit = "MTOW budget";
    }

    let l_over_d = report
        .trimmed_design_point
        .as_ref()
        .map_or(report.design_point.l_over_d, |p| p.l_over_d);
    let atmo = Atmosphere::new(config.requirements.cruise_altitude_m);
    let tas = config.requirements.cruise_mach * atmo.speed_of_sound();
    let tsfc_si = match config.geometry.engine.active_model().ok()? {
        ActiveEngineModel::Turbofan(spec) => spec.cruise_tsfc_kg_kgf_hr / (G * 3600.0),
        ActiveEngineModel::Turboprop(_) => {
            return Some(json!({
                "corners": null,
                "fuel_capacity_kg": fuel_capacity_kg,
                "fuel_capacity_limit": limit,
                "max_payload_kg": max_payload_kg,
                "oew_kg": oew_kg,
                "mtow_kg": mtow_kg,
                "method": "Capacity limits only; turbofan-TSFC Breguet range is inapplicable to turboprops",
            }))
        }
    };
    let range_nm = |start_kg: f64, end_kg: f64| {
        alas_perf::performance::breguet_range_m(tas, l_over_d, tsfc_si, start_kg, end_kg) / M_TO_NM
    };

    // A: max payload, no fuel. B: max payload, fuel to MTOW. C: full fuel,
    // payload cut back to MTOW. D: no payload, full fuel.
    let b_fuel = (mtow_kg - oew_kg - max_payload_kg)
        .max(0.0)
        .min(fuel_capacity_kg);
    let c_payload = (mtow_kg - oew_kg - fuel_capacity_kg)
        .max(0.0)
        .min(max_payload_kg);
    let corners = [
        ("A", 0.0, max_payload_kg),
        ("B", b_fuel, max_payload_kg),
        ("C", fuel_capacity_kg, c_payload),
        ("D", fuel_capacity_kg, 0.0),
    ];
    let points: Vec<Value> = corners
        .iter()
        .map(|&(label, fuel_kg, payload_kg)| {
            let start = oew_kg + payload_kg + fuel_kg;
            let nm = range_nm(start, start - fuel_kg);
            json!({
                "label": label,
                "range_nm": nm,
                "range_km": nm * 1.852,
                "payload_kg": payload_kg,
                "fuel_kg": fuel_kg,
                "takeoff_mass_kg": start,
            })
        })
        .collect();
    Some(json!({
        "corners": points,
        "fuel_capacity_kg": fuel_capacity_kg,
        "fuel_capacity_limit": limit,
        "max_payload_kg": max_payload_kg,
        "oew_kg": oew_kg,
        "mtow_kg": mtow_kg,
        "l_over_d_used": l_over_d,
        "cruise_tsfc_kg_kgf_h_used": match config.geometry.engine.active_model() {
            Ok(ActiveEngineModel::Turbofan(s)) => s.cruise_tsfc_kg_kgf_hr,
            _ => f64::NAN,
        },
        "method": "Breguet, no reserves, no climb/descent allowance",
    }))
}
