// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Build a [`LoadTrimSheetData`] from a pipeline result: the CG gate's
//! per-state physical limits scoped by phase, the composed loading orders
//! behind the boarding potato, and the model's analyzed/design masses.

use alas_opt::ModelCgLoadingState;
use alas_payload::loading_sequence::{LoadSequenceSet, LoadingPoint, LoadingSequence};
use alas_pipeline::PipelineResult;

use super::limits::{limit_sets, Bands};
use super::mechanisms::mechanism_lines;
use super::published::published_for;
use super::sequences::{load_sequence_set, loading_curves, potato_and_paths};
use super::LimitVertex;
use super::{BalanceIndex, CgLines, LoadStep, LoadTrimSheetData, MassRole, StepGate, WeightLine};

/// The state name of the maximum-fuel takeoff of the sizing loading.
pub const MAX_FUEL_STATE: &str = "max-fuel TOW";

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
        result.feasibility.design_model_cg.as_ref(),
        result.feasibility.fuel_loading.analyzed_takeoff_mass_kg,
        result
            .feasibility
            .operational_envelope
            .as_ref()
            .and_then(|env| env.zfw_operational_limits_pct_mac),
    )
}

/// Build the sheet for a selected analysis or editable mass preview.
///
/// The CG assessment is the one the feasibility pipeline evaluates for the
/// report: the item-level mass statement with the tank-capped carried fuel,
/// not a lumped breakdown at the closure mass. A design whose closure takeoff
/// mass exceeds what its tanks hold is then drawn at the fuel it can carry,
/// as the run's own verdict judges it.
pub fn load_trim_data_from_report(
    report: &alas_pipeline::full_analysis::AnalysisReport,
    config: &alas_config::AlasConfig,
) -> Option<LoadTrimSheetData> {
    let feasibility =
        alas_pipeline::assess_physical_feasibility(config, &report.design, report, None);
    build_data(
        report,
        config,
        feasibility.model_cg.as_ref()?,
        feasibility.design_model_cg.as_ref(),
        feasibility.fuel_loading.analyzed_takeoff_mass_kg,
        feasibility
            .operational_envelope
            .as_ref()
            .and_then(|env| env.zfw_operational_limits_pct_mac),
    )
}

fn build_data(
    report: &alas_pipeline::full_analysis::AnalysisReport,
    config: &alas_config::AlasConfig,
    model_cg: &alas_opt::ModelCgEnvelopeAssessment,
    design_model_cg: Option<&alas_opt::ModelCgEnvelopeAssessment>,
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
    // The limit lines are one assessment, so one gear sizing: the design
    // (maximum-fuel) loading's, which spans the empty weight to the maximum
    // takeoff weight, or the flown loading's when the run has no other.
    let limit_cg = design_model_cg.unwrap_or(model_cg);
    // One MLW: the mass the gate scoped every state and evaluated the landing
    // trim at, so the drawn landing band, the flight band's end and the gated
    // rows agree with the verdict.
    let landing_mass = Some(limit_cg.design_landing_mass_kg)
        .filter(|m| m.is_finite() && *m > 0.0)
        .or_else(|| report.design_landing_mass_kg())
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

    // The holds loaded: the cargo path of the loading sequences. The cabin
    // then completes the zero-fuel state, which is the gate's own state so the
    // marker and the verdict are one point.
    let hold_mass_kg = set.as_ref().and_then(|set| {
        let sequence = set.cargo.first();
        let last = sequence.and_then(|s| s.points.last())?;
        (last.mass_kg.is_finite() && last.x_m.is_finite() && last.mass_kg > dow.mass_kg).then(
            || {
                load = add_stage(load, dow_point, sequence);
                steps.push(LoadStep {
                    item: "Cargo".to_owned(),
                    state: "+hold".to_owned(),
                    mass_kg: load.0,
                    pct_mac: pct(load.1 / load.0),
                    gate: None,
                });
                load.0
            },
        )
    });
    let zfw = zfw_model?;
    load = (zfw.mass_kg, zfw.mass_kg * zfw.cg_x_m);
    steps.push(LoadStep {
        item: "Passengers".to_owned(),
        state: "ZFW".to_owned(),
        mass_kg: zfw.mass_kg,
        pct_mac: zfw.cg_pct_mac,
        gate: Some(StepGate::from_assessment(zfw)),
    });
    // The flown landing state when the run has one, else the reserve state
    // (zero fuel plus reserve fuel), which is where a landing sits on the
    // tank-burn path.
    let landing_state = if state(ModelCgLoadingState::AnalyzedLanding).is_some() {
        ModelCgLoadingState::AnalyzedLanding
    } else {
        ModelCgLoadingState::OperationalReserve
    };
    // The maximum-fuel takeoff of the design (sizing) loading, when the run
    // flies a different mass: the fuel-loading line runs on to it so the
    // sheet shows the whole loading the MTOW panels are drawn at.
    let max_fuel = design_model_cg.and_then(|design| {
        let takeoff = design
            .loading_states
            .iter()
            .find(|l| l.state == ModelCgLoadingState::AnalyzedTakeoff)?;
        let flown_mass = tow.map_or(f64::NAN, |t| t.mass_kg);
        (takeoff.mass_kg.is_finite() && (takeoff.mass_kg - flown_mass).abs() > 1.0)
            .then_some(takeoff)
    });
    let mut fuel_path = set
        .as_ref()
        .map_or_else(Vec::new, |set| fuel_curve(set, load, pct));
    if let Some(max) = max_fuel {
        if let Some(longer) = load_sequence_set(config, report, dow_point, max.mass_kg - zfw.mass_kg) {
            fuel_path = fuel_curve(&longer, load, pct);
        }
    }
    let zfw_mass = load.0;
    for (s, item, label) in [
        (ModelCgLoadingState::AnalyzedTakeoff, "Fuel", "TOW"),
        (landing_state, "Trip fuel burn", "LW"),
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
    // Levels of the potato are those of the flown loading; the maximum-fuel
    // state is added to the table and the chart after them.
    let (potato, sequences) = set.as_ref().map_or((Vec::new(), Vec::new()), |set| {
        let levels: Vec<f64> = steps.iter().map(|s| s.mass_kg).collect();
        potato_and_paths(&set.composed(), &levels, pct)
    });
    if let Some(max) = max_fuel {
        steps.push(LoadStep {
            item: "Maximum fuel".to_owned(),
            state: MAX_FUEL_STATE.to_owned(),
            mass_kg: max.mass_kg,
            pct_mac: max.cg_pct_mac,
            gate: Some(StepGate::from_assessment(max)),
        });
    }
    let loading_curves = set.as_ref().map_or_else(Vec::new, |set| {
        let cabin_end_kg = set
            .pax
            .first()
            .and_then(|s| s.points.last())
            .map_or(zfw_mass, |p| p.mass_kg);
        loading_curves(&set.composed(), hold_mass_kg, cabin_end_kg, pct)
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

    // The limits are drawn over the states of `limit_cg`.
    let states: Vec<_> = limit_cg.loading_states.iter().collect();
    let sets = limit_sets(
        &states,
        Bands {
            zfw_kg: zfw_model.map_or(zfw_mass, |z| z.mass_kg),
            takeoff_kg: takeoff_mass,
            landing_kg: landing_mass,
        },
    );

    // The maximum zero-fuel mass the preset registers, when it lies at or
    // above the analyzed zero-fuel mass; the analyzed mass otherwise.
    let mzfw_mass = preset
        .as_ref()
        .and_then(|p| p.reference.mzfw_kg)
        .filter(|m| m.is_finite() && *m >= zfw_mass && *m <= landing_mass.max(takeoff_mass));
    let mut weight_lines = vec![
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
    // The registered MTOW, which the tanks may keep the aircraft from
    // reaching: its own line, apart from the maximum-fuel takeoff state.
    let registered_mtow = config.requirements.mtow_kg;
    if registered_mtow.is_finite() && registered_mtow > 0.0 {
        weight_lines.push(WeightLine {
            role: MassRole::RegisteredMtow,
            mass_kg: registered_mtow,
        });
    }
    if let Some(mzfw) = mzfw_mass {
        weight_lines.push(WeightLine {
            role: MassRole::MaximumZeroFuel,
            mass_kg: mzfw,
        });
    }
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
        title: format!("{name} \u{2013} {}", super::panel::SHEET_NAME),
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
        en_route_above_mlw: sets.en_route_above_mlw,
        zfw_limits,
        lines: CgLines {
            mechanisms: mechanism_lines(&states, takeoff_mass, landing_mass),
            published: published_for(&config.preset, frame.x_lemac_m, frame.chord_m),
        },
        weight_lines,
        potato,
        sequences,
        loading_curves,
        fuel_curve: fuel_path,
        steps,
    };
    // The sheet draws one envelope per phase. Each gated row states its own
    // phase's envelope as drawn at its mass, so the table and the drawing
    // agree; the gate's verdict and constraint results are unchanged. The
    // empty state keeps the gate's own ground limits, and an en-route state
    // above the MLW (no landing or zero-fuel envelope applies) its own.
    for k in 0..data.steps.len() {
        let mass = data.steps[k].mass_kg;
        let kind = match data.steps[k].gate.as_ref().map(|g| g.phase) {
            Some(super::GatePhase::Takeoff) => Some(super::EnvelopeKind::Takeoff),
            Some(super::GatePhase::Landing) => Some(super::EnvelopeKind::Landing),
            Some(super::GatePhase::Flight) => Some(if data.steps[k].state == "LW" {
                super::EnvelopeKind::Landing
            } else {
                super::EnvelopeKind::ZeroFuel
            }),
            _ => None,
        };
        if let Some(kind) = kind {
            let fwd = data.envelope_limit_at(kind, mass, true);
            let aft = data.envelope_limit_at(kind, mass, false);
            if let Some(gate) = data.steps[k].gate.as_mut() {
                if fwd.is_finite() && aft.is_finite() {
                    gate.fwd_pct_mac = fwd;
                    gate.aft_pct_mac = aft;
                }
            }
        }
    }
    Some(data)
}
