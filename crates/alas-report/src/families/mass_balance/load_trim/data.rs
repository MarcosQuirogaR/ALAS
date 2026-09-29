// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Build a [`LoadTrimSheetData`] from a pipeline result: the CG gate's
//! per-state physical limits, the payload items behind the loading
//! sequences, the fuel vector, and the preset's structural weights.

use alas_opt::ModelCgLoadingState;
use alas_payload::loading_sequence::LoadingSequence;
use alas_pipeline::PipelineResult;

use super::{
    loading_envelope_polygon, BalanceIndex, LimitVertex, LoadStep, LoadTrimSheetData, WeightLine,
};

/// Per-item (mass kg, station m) increments of a cumulative sequence.
fn items_of(sequence: &LoadingSequence) -> Vec<(f64, f64)> {
    sequence
        .points
        .windows(2)
        .filter_map(|pair| {
            let dm = pair[1].mass_kg - pair[0].mass_kg;
            let dmom = pair[1].mass_kg * pair[1].x_m - pair[0].mass_kg * pair[0].x_m;
            (dm > 1e-9).then(|| (dm, dmom / dm))
        })
        .collect()
}

/// Plain-language name of a limit mechanism from its governance variant.
fn describe(governance: &str) -> &'static str {
    match governance {
        g if g.contains("Rotation") => "nose-wheel lift-off at rotation governs",
        g if g.contains("LandingTrim") => "landing trim in ground effect governs",
        g if g.contains("TipBack") => "tip-back geometry governs",
        g if g.contains("MinimumNose") => "minimum nose-gear load governs",
        g if g.contains("MaxNose") => "maximum nose-gear load governs",
        g if g.contains("Scissor") => "tail-authority estimate governs",
        g if g.contains("Aero") => "neutral point less minimum margin governs",
        _ => "governing mechanism per CG gate",
    }
}

/// Assemble the balance-chart data, or `None` when the run carries no model
/// CG assessment or no main-wing MAC frame.
pub fn load_trim_data_from_pipeline(result: &PipelineResult) -> Option<LoadTrimSheetData> {
    let report = result
        .optimized_report
        .as_ref()
        .or(result.baseline_analysis.as_ref())?;
    let frame = report.airplane.mac_frame()?;
    let model_cg = result.feasibility.model_cg.as_ref()?;
    let pct = |x_m: f64| frame.pct_mac(x_m);
    let state = |s: ModelCgLoadingState| model_cg.loading_states.iter().find(|l| l.state == s);

    let mut takeoff_limits: Vec<LimitVertex> = model_cg
        .loading_states
        .iter()
        .map(|s| LimitVertex {
            mass_kg: s.mass_kg,
            fwd_pct_mac: s.physical_limits.fwd_limit_pct_mac,
            aft_pct_mac: s.physical_limits.aft_limit_pct_mac,
        })
        .filter(|v| v.mass_kg.is_finite() && v.fwd_pct_mac.is_finite() && v.aft_pct_mac.is_finite())
        .collect();
    takeoff_limits.sort_by(|a, b| a.mass_kg.total_cmp(&b.mass_kg));
    takeoff_limits.dedup_by(|a, b| (a.mass_kg - b.mass_kg).abs() < 1.0);
    let heaviest = model_cg
        .loading_states
        .iter()
        .max_by(|a, b| a.mass_kg.total_cmp(&b.mass_kg))?;
    let governance = [
        describe(&format!(
            "{:?}",
            heaviest.physical_limits.fwd_limit_governance
        ))
        .to_owned(),
        describe(&format!(
            "{:?}",
            heaviest.physical_limits.aft_limit_governance
        ))
        .to_owned(),
    ];

    let config = &result.config;
    let preset = alas_config::presets::get(&config.preset).ok();
    let reference = preset.map(|p| p.reference.clone());
    // The weight lines are the model's own masses: the pipeline's sized
    // takeoff mass (then the analyzed takeoff mass, then the heaviest gate
    // state) and the landing mass the model closes on it. The preset's
    // published structural weights are a labelled reference overlay only.
    let mtow = report
        .sized_takeoff_mass_kg()
        .or_else(|| {
            let analyzed = result.feasibility.fuel_loading.analyzed_takeoff_mass_kg;
            (analyzed.is_finite() && analyzed > 0.0).then_some(analyzed)
        })
        .unwrap_or(heaviest.mass_kg);
    let mlw = config.design_landing_mass_for(mtow).min(mtow);
    let mut notes = vec![
        "Limits are the ALAS CG gate's per-weight physical limits (model, not certified)."
            .to_owned(),
    ];
    if let Some(reference) = reference.as_ref() {
        let published =
            |label: &str, mass: Option<f64>| mass.map(|kg| format!("{label} {kg:.0} kg"));
        let parts: Vec<String> = [
            published("MTOW", reference.mtow_kg),
            published("MLW", reference.mlw_kg),
            published("MZFW", reference.mzfw_kg),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !parts.is_empty() {
            notes.push(format!(
                "Published reference (overlay, not used by the model): {}.",
                parts.join(", ")
            ));
        }
    }

    // Worked case: DOW, cargo, passengers (ZFW), fuel (TOW), burn (LW).
    let dow = state(ModelCgLoadingState::OperatingEmpty)?;
    let dow_x = frame.x_at_pct(dow.cg_pct_mac);
    let mut steps = vec![LoadStep {
        item: "Empty".to_owned(),
        state: "DOW".to_owned(),
        mass_kg: dow.mass_kg,
        pct_mac: dow.cg_pct_mac,
    }];
    let mut items = Vec::new();
    let mut fuel_curve = Vec::new();
    let mut zfw_limits = Vec::new();
    let (mut mass, mut moment) = (dow.mass_kg, dow.mass_kg * dow_x);
    if let Some(env) = result.feasibility.operational_envelope.as_ref() {
        for (sequence, item, label) in [
            (env.cargo_sequences.first(), "Cargo", "+hold"),
            (env.passenger_sequences.first(), "Passengers", "ZFW"),
        ] {
            let Some(sequence) = sequence else { continue };
            let seq_items = items_of(sequence);
            for &(m, x) in &seq_items {
                mass += m;
                moment += m * x;
            }
            items.extend(seq_items);
            steps.push(LoadStep {
                item: item.to_owned(),
                state: label.to_owned(),
                mass_kg: mass,
                pct_mac: pct(moment / mass),
            });
        }
        fuel_curve = env
            .fuel_vector_checks
            .iter()
            .map(|c| (c.mass_kg, c.cg_pct_mac))
            .collect();
        fuel_curve.sort_by(|a: &(f64, f64), b: &(f64, f64)| a.0.total_cmp(&b.0));
        if let Some((fwd, aft)) = env.zfw_operational_limits_pct_mac {
            let top = mass;
            zfw_limits.push(LimitVertex {
                mass_kg: dow.mass_kg.min(mass),
                fwd_pct_mac: fwd,
                aft_pct_mac: aft,
            });
            zfw_limits.push(LimitVertex {
                mass_kg: top,
                fwd_pct_mac: fwd,
                aft_pct_mac: aft,
            });
        }
    }
    for (s, item, label) in [
        (ModelCgLoadingState::AnalyzedTakeoff, "Fuel", "TOW"),
        (
            ModelCgLoadingState::OperationalReserve,
            "Trip fuel burn",
            "LW",
        ),
    ] {
        if let Some(s) = state(s) {
            steps.push(LoadStep {
                item: item.to_owned(),
                state: label.to_owned(),
                mass_kg: s.mass_kg,
                pct_mac: s.cg_pct_mac,
            });
        }
    }
    let loading_envelope =
        loading_envelope_polygon(dow.mass_kg, dow_x, &items, frame.x_lemac_m, frame.chord_m);

    let mut weight_lines = vec![
        WeightLine {
            label: "MTOW".to_owned(),
            mass_kg: mtow,
        },
        WeightLine {
            label: "MLW".to_owned(),
            mass_kg: mlw,
        },
    ];
    weight_lines.push(WeightLine {
        label: "MZFW".to_owned(),
        mass_kg: mass,
    });
    notes.push("MZFW line is the model zero-fuel mass of the analyzed loading.".to_owned());
    notes.push(format!(
        "Forward limit: {}; aft limit: {}.",
        governance[0], governance[1]
    ));

    let fwd_min = takeoff_limits
        .iter()
        .map(|v| v.fwd_pct_mac)
        .fold(f64::INFINITY, f64::min);
    let aft_max = takeoff_limits
        .iter()
        .map(|v| v.aft_pct_mac)
        .fold(f64::NEG_INFINITY, f64::max);
    let name = preset.map_or(config.preset.clone(), |p| p.display_name.to_owned());
    Some(LoadTrimSheetData {
        title: format!("{name}  -  LOAD & TRIM SHEET (ALAS model)"),
        x_lemac_m: frame.x_lemac_m,
        mac_m: frame.chord_m,
        index: BalanceIndex::for_aircraft(frame.x_lemac_m, frame.chord_m, mtow, fwd_min, aft_max),
        takeoff_limits,
        zfw_limits,
        weight_lines,
        loading_envelope,
        fuel_curve,
        steps,
        governance,
        notes,
    })
}
