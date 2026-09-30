// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Build a [`LoadTrimSheetData`] from a pipeline result: the CG gate's
//! per-state physical limits scoped by phase, the composed loading orders
//! behind the boarding potato, and the preset's structural weights.

use alas_opt::ModelCgLoadingState;
use alas_payload::loading_sequence::{LoadSequenceSet, LoadingPoint, LoadingSequence};
use alas_pipeline::PipelineResult;

use super::limits::{limit_sets, Bands};
use super::sequences::{load_sequence_set, potato_and_paths};
use super::LimitVertex;
use super::{BalanceIndex, LoadStep, LoadTrimSheetData, WeightLine};

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

/// Mass and moment about the nose after replaying the increments of the
/// path `sequence` (built from `origin`) on top of `(mass, moment)`.
fn add_stage(
    (mass, moment): (f64, f64),
    origin: LoadingPoint,
    sequence: Option<&LoadingSequence>,
) -> (f64, f64) {
    let Some(end) = sequence.and_then(|s| s.points.last()) else {
        return (mass, moment);
    };
    (
        mass + end.mass_kg - origin.mass_kg,
        moment + end.mass_kg * end.x_m - origin.mass_kg * origin.x_m,
    )
}

/// The fuel path of `set` on top of the zero-fuel state, (mass kg, %MAC).
fn fuel_curve(set: &LoadSequenceSet, zfw: (f64, f64), pct: impl Fn(f64) -> f64) -> Vec<(f64, f64)> {
    let (mass, moment) = zfw;
    let mut curve = vec![(mass, pct(moment / mass))];
    curve.extend(set.fuel.points.iter().filter(|p| p.mass_kg > 0.0).map(|p| {
        let total = mass + p.mass_kg;
        (total, pct((moment + p.mass_kg * p.x_m) / total))
    }));
    curve
}

/// The %MAC of an ascending (mass, %MAC) curve at `mass_kg`; `None` outside
/// the curve.
fn interpolate_curve(curve: &[(f64, f64)], mass_kg: f64) -> Option<f64> {
    curve.windows(2).find_map(|w| {
        (mass_kg >= w[0].0 && mass_kg <= w[1].0 && w[1].0 > w[0].0)
            .then(|| w[0].1 + (mass_kg - w[0].0) / (w[1].0 - w[0].0) * (w[1].1 - w[0].1))
    })
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
    let mlw = report
        .design_landing_mass_kg()
        .unwrap_or_else(|| config.design_landing_mass_at_closure(mtow))
        .min(mtow);
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
    let dow_point = LoadingPoint {
        mass_kg: dow.mass_kg,
        x_m: dow.cg_x_m,
    };
    let mut steps = vec![LoadStep {
        item: "Empty".to_owned(),
        state: "DOW".to_owned(),
        mass_kg: dow.mass_kg,
        pct_mac: dow.cg_pct_mac,
    }];
    let tow = state(ModelCgLoadingState::AnalyzedTakeoff);
    let zfw_model = state(ModelCgLoadingState::AnalyzedZeroFuel);
    let fuel_kg = match (tow, zfw_model) {
        (Some(t), Some(z)) => (t.mass_kg - z.mass_kg).max(0.0),
        _ => 0.0,
    };
    let set = load_sequence_set(config, report, dow_point, fuel_kg);
    let mut load = (dow.mass_kg, dow.mass_kg * dow.cg_x_m);
    let mut fuel_path = Vec::new();
    if let Some(set) = set.as_ref() {
        for (sequence, item, label) in [
            (set.cargo.first(), "Cargo", "+hold"),
            (set.pax.first(), "Passengers", "ZFW"),
        ] {
            if sequence.is_none() {
                continue;
            }
            load = add_stage(load, dow_point, sequence);
            steps.push(LoadStep {
                item: item.to_owned(),
                state: label.to_owned(),
                mass_kg: load.0,
                pct_mac: pct(load.1 / load.0),
            });
        }
        fuel_path = fuel_curve(set, load, pct);
    }
    let zfw_mass = load.0;
    for (s, item, label) in [
        (ModelCgLoadingState::AnalyzedTakeoff, "Fuel", "TOW"),
        (
            ModelCgLoadingState::OperationalReserve,
            "Trip fuel burn",
            "LW",
        ),
    ] {
        if let Some(s) = state(s) {
            // The burn retraces the tank-burn path drawn as the fuel curve,
            // so the landing point reads its CG from that path when the run
            // has one; the gate's own value is kept otherwise.
            let on_path = (label == "LW")
                .then(|| interpolate_curve(&fuel_path, s.mass_kg))
                .flatten();
            if let Some(path_pct) = on_path {
                if (path_pct - s.cg_pct_mac).abs() > 0.05 {
                    notes.push(format!(
                        "Landing point CG {path_pct:.1} %MAC follows the tank-burn path; the CG gate's reserve state assumes {:.1} %MAC.",
                        s.cg_pct_mac
                    ));
                }
            }
            steps.push(LoadStep {
                item: item.to_owned(),
                state: label.to_owned(),
                mass_kg: s.mass_kg,
                pct_mac: on_path.unwrap_or(s.cg_pct_mac),
            });
        }
    }
    let (potato, sequences) = set.as_ref().map_or((Vec::new(), Vec::new()), |set| {
        let levels: Vec<f64> = steps.iter().map(|s| s.mass_kg).collect();
        potato_and_paths(&set.composed(), &levels, pct)
    });

    let mut zfw_limits = Vec::new();
    if let Some((fwd, aft)) = result
        .feasibility
        .operational_envelope
        .as_ref()
        .and_then(|env| env.zfw_operational_limits_pct_mac)
    {
        for mass_kg in [dow.mass_kg.min(zfw_mass), zfw_mass] {
            zfw_limits.push(LimitVertex {
                mass_kg,
                fwd_pct_mac: fwd,
                aft_pct_mac: aft,
            });
        }
    }

    let states: Vec<_> = model_cg.loading_states.iter().collect();
    let sets = limit_sets(
        &states,
        Bands {
            zfw_kg: zfw_model.map_or(zfw_mass, |z| z.mass_kg),
            mtow_kg: mtow,
            mlw_kg: mlw,
        },
    );

    let weight_lines = vec![
        WeightLine {
            label: "MTOW".to_owned(),
            mass_kg: mtow,
        },
        WeightLine {
            label: "MLW".to_owned(),
            mass_kg: mlw,
        },
        WeightLine {
            label: "MZFW".to_owned(),
            mass_kg: zfw_mass,
        },
    ];
    notes.push("MZFW line is the model zero-fuel mass of the analyzed loading.".to_owned());
    notes.push(format!(
        "Forward limit at takeoff mass: {}; aft limit: {}.",
        governance[0], governance[1]
    ));
    notes.push(
        "Ground limits use the maximum-nose-load forward and the minimum-nose-load and tip-back aft mechanisms only; the forward line at empty weight is a ground limit, not a flight limit."
            .to_owned(),
    );

    let all_limits = [&sets.ground, &sets.takeoff, &sets.flight, &sets.landing];
    let fwd_min = all_limits
        .iter()
        .flat_map(|set| set.iter())
        .map(|v| v.fwd_pct_mac)
        .fold(f64::INFINITY, f64::min);
    let aft_max = all_limits
        .iter()
        .flat_map(|set| set.iter())
        .map(|v| v.aft_pct_mac)
        .fold(f64::NEG_INFINITY, f64::max);
    let name = preset.map_or(config.preset.clone(), |p| p.display_name.to_owned());
    let mut data = LoadTrimSheetData {
        title: format!("{name}  -  LOAD & TRIM SHEET (ALAS model)"),
        x_lemac_m: frame.x_lemac_m,
        mac_m: frame.chord_m,
        index: BalanceIndex::for_aircraft(frame.x_lemac_m, frame.chord_m, mtow, fwd_min, aft_max),
        ground_limits: sets.ground,
        takeoff_limits: sets.takeoff,
        flight_limits: sets.flight,
        landing_limits: sets.landing,
        zfw_limits,
        weight_lines,
        potato,
        sequences,
        fuel_curve: fuel_path,
        steps,
        governance,
        notes,
    };
    if !data.potato.is_empty() {
        let excess = data.potato_ground_exceedance_pct_mac();
        data.notes.push(if excess > 1e-6 {
            format!(
                "Boarding potato ({} composed orders) leaves the ground limits by up to {excess:.1} %MAC.",
                data.sequences.len()
            )
        } else {
            format!(
                "Boarding potato ({} composed orders) lies inside the ground limits.",
                data.sequences.len()
            )
        });
    }
    Some(data)
}
