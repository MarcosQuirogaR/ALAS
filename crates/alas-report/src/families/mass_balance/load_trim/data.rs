// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Build a [`LoadTrimSheetData`] from a pipeline result: the CG gate's
//! per-state physical limits scoped by phase, the composed loading orders
//! behind the boarding potato, and the model's analyzed/design masses.

use alas_opt::ModelCgLoadingState;
use alas_payload::loading_sequence::{LoadSequenceSet, LoadingPoint, LoadingSequence};
use alas_pipeline::PipelineResult;

use super::limits::{limit_sets, Bands};
use super::sequences::{load_sequence_set, potato_and_paths};
use super::LimitVertex;
use super::{BalanceIndex, LoadStep, LoadTrimSheetData, MassRole, StepGate, WeightLine};

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
    build_data(
        report,
        &result.config,
        result.feasibility.model_cg.as_ref()?,
        result.feasibility.fuel_loading.analyzed_takeoff_mass_kg,
        result
            .feasibility
            .operational_envelope
            .as_ref()
            .and_then(|env| env.zfw_operational_limits_pct_mac),
    )
}

/// Build the sheet for a selected analysis or editable mass preview.
pub fn load_trim_data_from_report(
    report: &alas_pipeline::full_analysis::AnalysisReport,
    config: &alas_config::AlasConfig,
) -> Option<LoadTrimSheetData> {
    let assessment = crate::families::model_cg_gate_assessment(report, config).ok()?;
    build_data(report, config, &assessment, f64::NAN, None)
}

fn build_data(
    report: &alas_pipeline::full_analysis::AnalysisReport,
    config: &alas_config::AlasConfig,
    model_cg: &alas_opt::ModelCgEnvelopeAssessment,
    analyzed_takeoff_mass_kg: f64,
    zfw_operational_limits: Option<(f64, f64)>,
) -> Option<LoadTrimSheetData> {
    let frame = report.airplane.mac_frame()?;
    let pct = |x_m: f64| frame.pct_mac(x_m);
    let state = |s: ModelCgLoadingState| model_cg.loading_states.iter().find(|l| l.state == s);

    let heaviest = model_cg
        .loading_states
        .iter()
        .max_by(|a, b| a.mass_kg.total_cmp(&b.mass_kg))?;
    let preset = alas_config::presets::get(&config.preset).ok();
    let sized_takeoff_mass = report.sized_takeoff_mass_kg();
    let takeoff_role = if sized_takeoff_mass.is_some() {
        MassRole::SizedTakeoff
    } else {
        MassRole::AnalyzedTakeoff
    };
    let takeoff_mass = sized_takeoff_mass
        .or_else(|| {
            let analyzed = analyzed_takeoff_mass_kg;
            (analyzed.is_finite() && analyzed > 0.0).then_some(analyzed)
        })
        .unwrap_or(heaviest.mass_kg);
    let landing_mass = report
        .design_landing_mass_kg()
        .unwrap_or_else(|| config.design_landing_mass_at_closure(takeoff_mass))
        .min(takeoff_mass);
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
        gate: Some(StepGate::from_assessment(dow)),
    }];
    let tow = state(ModelCgLoadingState::AnalyzedTakeoff);
    let zfw_model = state(ModelCgLoadingState::AnalyzedZeroFuel);
    let fuel_kg = match (tow, zfw_model) {
        (Some(t), Some(z)) => (t.mass_kg - z.mass_kg).max(0.0),
        _ => 0.0,
    };
    let set = load_sequence_set(config, report, dow_point, fuel_kg);
    let mut load = (dow.mass_kg, dow.mass_kg * dow.cg_x_m);

    if let Some(set) = set.as_ref() {
        for (sequence, item, label) in [
            (set.cargo.first(), "Cargo", "+hold"),
            (set.pax.first(), "Passengers", "ZFW"),
        ] {
            if sequence.and_then(|s| s.points.last()).is_none_or(|last| {
                !last.mass_kg.is_finite() || !last.x_m.is_finite() || last.mass_kg <= dow.mass_kg
            }) {
                continue;
            }
            load = add_stage(load, dow_point, sequence);
            steps.push(LoadStep {
                item: item.to_owned(),
                state: label.to_owned(),
                mass_kg: load.0,
                pct_mac: pct(load.1 / load.0),
                // The gate evaluates the zero-fuel state, not the hold step.
                gate: (label == "ZFW")
                    .then(|| zfw_model.map(StepGate::from_assessment))
                    .flatten(),
            });
        }
    }
    if !steps.iter().any(|step| step.state == "ZFW") {
        let zfw = zfw_model?;
        load = (zfw.mass_kg, zfw.mass_kg * zfw.cg_x_m);
        steps.push(LoadStep {
            item: "Payload".to_owned(),
            state: "ZFW".to_owned(),
            mass_kg: zfw.mass_kg,
            pct_mac: zfw.cg_pct_mac,
            gate: Some(StepGate::from_assessment(zfw)),
        });
    }
    let fuel_path = set
        .as_ref()
        .map_or_else(Vec::new, |set| fuel_curve(set, load, pct));
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
            steps.push(LoadStep {
                item: item.to_owned(),
                state: label.to_owned(),
                mass_kg: s.mass_kg,
                pct_mac: on_path.unwrap_or(s.cg_pct_mac),
                gate: Some(StepGate::from_assessment(s)),
            });
        }
    }
    let (potato, sequences) = set.as_ref().map_or((Vec::new(), Vec::new()), |set| {
        let levels: Vec<f64> = steps.iter().map(|s| s.mass_kg).collect();
        potato_and_paths(&set.composed(), &levels, pct)
    });

    let mut zfw_limits = Vec::new();
    if let Some((fwd, aft)) = zfw_operational_limits {
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
            takeoff_kg: takeoff_mass,
            landing_kg: landing_mass,
        },
    );

    let weight_lines = vec![
        WeightLine {
            role: takeoff_role,
            mass_kg: takeoff_mass,
        },
        WeightLine {
            role: MassRole::DesignLanding,
            mass_kg: landing_mass,
        },
        WeightLine {
            role: MassRole::AnalyzedZeroFuel,
            mass_kg: zfw_mass,
        },
    ];
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
    Some(LoadTrimSheetData {
        title: format!("{name}  -  {}", super::panel::SHEET_NAME),
        x_lemac_m: frame.x_lemac_m,
        mac_m: frame.chord_m,
        index: BalanceIndex::for_aircraft(
            frame.x_lemac_m,
            frame.chord_m,
            takeoff_mass,
            fwd_min,
            aft_max,
        ),
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
    })
}
