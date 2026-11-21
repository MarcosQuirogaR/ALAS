// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Running one aircraft case and extracting the data the rules judge.
//!
//! A case is a registered preset (or a clean-sheet brief merged onto a base
//! preset) flown in one MTOW mode on its registered design, native analyses
//! only, no optimization: the same run the acceptance matrix makes, so what is
//! judged is what the product reports.

use alas_config::airports::Airport;
use alas_config::MtowSizing;
use alas_config::{presets, AlasConfig};
use alas_mass::breakdown::OEW_KEYS;
use alas_payload::layout::{ItemKind, ItemMeta};
use alas_perf::performance::compute_v_speeds_at_masses;
use alas_pipeline::full_analysis::AnalysisReport;
use alas_pipeline::{DesignPipeline, PipelineOptions, PipelineResult, RunEnvironment};
use alas_report::families::performance::payload_range_data;
use serde_json::{json, Value};

use super::aero::{
    check_cl_max, check_payload_range, check_polar, check_v_speeds, check_vn, VSpeedFacts, VnFacts,
};
use super::figures::{check_span_lift, check_thrust_lapse, SpanLiftFacts};
use super::geometry::{check_cabin, check_fuselage, ItemClass, ItemFacts, Station};
use super::mass::{
    check_cg, check_mass, check_published_aft_limit, check_structure, CgState, MassFacts,
    StructFacts,
};
use super::mission::{check_mission, MissionFacts};
use super::{Finding, Findings};

/// The three MTOW modes the application offers a user.
pub const USER_MODES: [MtowSizing; 3] = [
    MtowSizing::FixedRequirement,
    MtowSizing::MtowBand,
    MtowSizing::PayloadAdjusted,
];

/// Result of judging one case.
#[derive(Debug, Clone)]
pub struct CaseOutcome {
    /// `<aircraft>@<mode>`.
    pub label: String,
    /// Violated rules.
    pub findings: Vec<Finding>,
    /// Checks that could not run, with the reason (never silent).
    pub skipped: Vec<String>,
}

/// Case label.
pub fn label(aircraft: &str, mode: MtowSizing) -> String {
    format!("{aircraft}@{}", mode.as_str())
}

fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                merge(target.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

/// Build the configuration of a case: preset `base`, an optional brief patch,
/// the MTOW mode, and no external tools.
pub fn case_config(
    base: &str,
    brief: Option<&Value>,
    mode: MtowSizing,
) -> Result<AlasConfig, String> {
    let mut value = json!({ "preset": base });
    if let Some(patch) = brief {
        merge(&mut value, patch);
    }
    merge(
        &mut value,
        &json!({
            "optimizer": {"objective": {"mtow_sizing": mode.as_str()}},
            "mses": {"enabled": false},
            "downstream": {"openvsp": false, "vspaero": false, "avl": false, "flowunsteady": false},
            "structures": {"run_nastran": false, "run_patran_export": false}
        }),
    );
    AlasConfig::from_value(&value).map_err(|e| format!("configuration of {base}: {e}"))
}

/// Run a case through the product pipeline.
pub fn run_pipeline(config: &AlasConfig) -> Result<PipelineResult, String> {
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: true,
        parallel: true,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    DesignPipeline::new(config.clone())
        .run(&options, &RunEnvironment::default())
        .map_err(|e| format!("pipeline: {e}"))
}

/// Judge a pipeline result with every rule.
pub fn judge(label: &str, result: &PipelineResult) -> CaseOutcome {
    let mut out = Findings::new(label);
    let mut skipped = Vec::new();
    let config = &result.config;

    let mut native_facts = None;
    match result.mission_result.as_ref() {
        Some(mission) => {
            let mut facts = MissionFacts::from_result(mission, config.requirements.cruise_mach);
            let turboprop = matches!(
                config.geometry.engine.active_model(),
                Ok(alas_config::ActiveEngineModel::Turboprop(_))
            );
            facts.climb_reference =
                super::climb_reference::reference_for(label.split('@').next().unwrap_or(label))
                    .or_else(|| {
                        turboprop
                            .then(|| super::climb_reference::reference_for("CLASS:turboprop"))
                            .flatten()
                    });
            check_mission(&mut out, &facts);
            native_facts = Some(facts);
        }
        None => skipped.push("mission: the run produced no mission telemetry".into()),
    }

    let report = result
        .baseline_analysis
        .as_ref()
        .or(result.optimized_report.as_ref());
    if let (Some(native), Some(report)) = (native_facts.as_ref(), report) {
        judge_priced(&mut out, result, report, native);
    }
    match report {
        Some(report) => judge_report(&mut out, &mut skipped, result, report),
        None => skipped
            .push("analysis report missing: aero, mass, cabin and geometry rules skipped".into()),
    }

    if let Some(cg) = result.feasibility.model_cg.as_ref() {
        let states: Vec<CgState> = cg
            .loading_states
            .iter()
            .map(|s| CgState {
                label: format!("{:?}", s.state),
                cg_pct_mac: s.cg_pct_mac,
                static_margin: s.static_margin,
                critical_static_margin: s.critical_static_margin,
                nose_load_fraction: s.nose_gear_load_fraction,
                ground_reactions_admissible: s.ground_reactions_admissible,
                violated: s
                    .constraints
                    .iter()
                    .filter(|c| !c.is_diagnostic() && c.violated)
                    .map(|c| format!("{:?}", c.constraint))
                    .collect(),
                worst_exceedance: s
                    .constraints
                    .iter()
                    .filter(|c| !c.is_diagnostic() && c.violated)
                    .map(|c| c.normalized_exceedance)
                    .fold(0.0, f64::max),
            })
            .collect();
        check_cg(&mut out, &states);
        super::mass::check_gear_reaction(
            &mut out,
            cg.gear_design_weight_kg,
            cg.gear_design_main_reaction_kg,
            cg.gear_raw_aft_nose_reaction_kg,
            cg.gear_raw_aft_cg_is_takeoff_state,
        );
        if let (Some(empty), Some(reference)) = (
            cg.loading_states
                .iter()
                .find(|s| s.state == alas_opt::ModelCgLoadingState::OperatingEmpty),
            alas_config::oew_reference::oew_cg_reference(&config.preset),
        ) {
            super::oew_cg::check_oew_cg_reference(&mut out, empty.cg_pct_mac, reference);
        }
        // The critical neutral point is state-independent: read it off any state.
        let published_aft = presets::get(&config.preset)
            .ok()
            .and_then(|preset| preset.reference.aft_cg_nose_load)
            .and_then(|split| {
                split
                    .points
                    .iter()
                    .filter_map(|point| point.aft_cg_pct_mac)
                    .reduce(f64::max)
            });
        if let Some(first) = states.first() {
            check_published_aft_limit(
                &mut out,
                first.cg_pct_mac + 100.0 * first.critical_static_margin,
                published_aft,
            );
        }
    } else {
        skipped.push("model CG assessment unavailable".into());
    }

    if let Some(analysis) = result
        .structural_result
        .as_ref()
        .and_then(|s| s.analysis.as_ref())
    {
        let worst = analysis.load_cases.iter().max_by(|a, b| {
            a.tip_deflection_m
                .abs()
                .total_cmp(&b.tip_deflection_m.abs())
        });
        let semi_span = report.map_or(0.0, |r| 0.5 * r.airplane.b_ref);
        if let Some(case) = worst {
            check_structure(
                &mut out,
                &StructFacts {
                    frequencies_hz: analysis.modal.frequencies_hz.clone(),
                    semi_span_m: semi_span,
                    y_m: case.y.clone(),
                    deflection_m: case.deflection_m.clone(),
                },
            );
        }
    } else {
        skipped.push("native structural analysis unavailable".into());
    }

    super::engine::judge_engine(&mut out, config, &result.feasibility.findings);

    CaseOutcome {
        label: label.to_owned(),
        findings: out.into_items(),
        skipped,
    }
}

fn judge_report(
    out: &mut Findings,
    skipped: &mut Vec<String>,
    result: &PipelineResult,
    report: &AnalysisReport,
) {
    let config = &result.config;
    check_polar(
        out,
        &report.polar.cl,
        &report.polar.cd,
        &report.polar.l_over_d,
        report.design_point.cl,
    );
    check_cl_max(
        out,
        config.performance.cl_max_to,
        config.performance.cl_max_land,
    );

    match alas_report::families::aerodynamics::span_loading_balance(report, config) {
        Some(balance) => check_span_lift(
            out,
            &SpanLiftFacts {
                wing_lift_n: balance.wing_lift_n,
                total_lift_n: balance.total_lift_n,
                weight_n: balance.weight_n,
            },
        ),
        None => skipped.push("span-loading balance unavailable".into()),
    }
    let turbofan = matches!(
        config.geometry.engine.active_model(),
        Ok(alas_config::ActiveEngineModel::Turbofan(_))
    );
    // The lapse rule is a turbofan rule: on another engine class it does not
    // apply, which is not a skipped check. A turbofan without its deck is.
    if turbofan {
        match alas_report::families::propulsion::cruise_thrust_lapse(config) {
            Some(lapse) => check_thrust_lapse(out, lapse),
            None => {
                skipped.push("propulsion lapse not judged: the turbofan deck is unavailable".into())
            }
        }
    }

    match payload_range_data(report, config) {
        Some(data) => {
            let corners: [(f64, f64); 4] =
                std::array::from_fn(|i| (data.points[i].range_nm, data.points[i].payload_kg));
            check_payload_range(out, &corners, data.oew_kg, data.mtow_kg);
        }
        None => skipped.push("payload-range corners unavailable".into()),
    }

    let vn = alas_pipeline::design_vn_diagram(config, report);
    check_vn(
        out,
        &VnFacts {
            vs_kt: vn.v_s_kt,
            va_kt: vn.v_a_kt,
            vc_kt: vn.v_c_kt,
            vd_kt: vn.v_d_kt,
            v_cruise_op_kt: vn.v_cruise_op_kt,
            n_pos: vn.n_lim_pos,
            n_neg: vn.n_lim_neg,
            n_ult_pos: vn.n_ult_pos,
            mass_kg: report.analysis_takeoff_mass_kg(config.requirements.mtow_kg),
        },
    );

    let takeoff = report.analysis_takeoff_mass_kg(config.requirements.mtow_kg);
    let landing = report.design_landing_mass_kg().unwrap_or(0.85 * takeoff);
    let area = report
        .geometry_summary
        .get("wing_area_m2")
        .copied()
        .unwrap_or(report.airplane.s_ref);
    if area.is_finite() && area > 0.0 {
        let airport = Airport {
            name: "ISA sea level".into(),
            icao: "ISA0".into(),
            elevation_m: 0.0,
            toda_m: 3000.0,
            lda_m: 3000.0,
            isa_deviation_c: 0.0,
            notes: String::new(),
            latitude_deg: 0.0,
            longitude_deg: 0.0,
        };
        let v = compute_v_speeds_at_masses(
            takeoff,
            landing,
            area,
            &airport,
            config.performance.cl_max_to,
            config.performance.cl_max_land,
            &config.performance,
        );
        check_v_speeds(
            out,
            &VSpeedFacts {
                v_stall_to: v.v_stall_to_ms,
                v_stall_land: v.v_stall_land_ms,
                v_mc: v.v_mc_ms,
                v1: v.v1_ms,
                vr: v.v_r_ms,
                v2: v.v2_ms,
                v_app: v.v_app_ms,
                v_td: v.v_td_ms,
            },
        );
    } else {
        skipped.push("V-speeds: wing area unavailable".into());
    }

    let masses = &report.component_masses;
    let oew: f64 = OEW_KEYS
        .iter()
        .map(|k| masses.get(*k).copied().unwrap_or(0.0))
        .sum();
    check_mass(
        out,
        &MassFacts {
            takeoff_kg: takeoff,
            oew_kg: oew,
            payload_kg: masses.get("Payload").copied().unwrap_or(0.0),
            fuel_kg: masses.get("Fuel").copied().unwrap_or(0.0),
        },
    );

    let stations = fuselage_stations(report);
    if stations.is_empty() {
        skipped.push("fuselage stations unavailable".into());
    } else {
        check_fuselage(out, &stations);
        match report.payload_layout.as_ref() {
            Some(layout) => check_cabin(out, &stations, &item_facts(layout)),
            None => skipped.push("cabin layout unavailable".into()),
        }
    }
}

/// Stations of the main (first, largest) fuselage.
pub fn fuselage_stations(report: &AnalysisReport) -> Vec<Station> {
    report
        .airplane
        .fuselages
        .iter()
        .filter(|f| !f.name.to_ascii_lowercase().contains("nacelle"))
        .max_by(|a, b| a.xsecs.len().cmp(&b.xsecs.len()))
        .map(|f| {
            f.xsecs
                .iter()
                .map(|s| Station {
                    x: s.xyz_c[0],
                    width: s.width,
                    height: s.height,
                    zc: s.xyz_c[2],
                    shape: s.shape,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Cabin items as the geometry rules read them.
pub fn item_facts(layout: &alas_payload::layout::PayloadLayout) -> Vec<ItemFacts> {
    layout
        .items
        .iter()
        .map(|item| {
            let class = match item.kind {
                ItemKind::SeatRow => ItemClass::Seat,
                ItemKind::Uld | ItemKind::Bag => ItemClass::Cargo,
                ItemKind::Exit => ItemClass::Exit,
                _ => ItemClass::Monument,
            };
            let (aisle_w, seats) = match &item.meta {
                ItemMeta::Seat(meta) => (Some(meta.aisle_w), meta.filled),
                _ => (None, 0),
            };
            // A seat row's box starts above the floor it stands on by the layout's
            // own data (its floor height), not by a constant of the code under test.
            let base_above_floor = match &item.meta {
                ItemMeta::Seat(meta) => (item.z - 0.5 * item.height - meta.floor_z).max(0.0),
                _ => 0.0,
            };
            ItemFacts {
                class,
                deck: item.deck.to_owned(),
                label: item.label.clone(),
                x: item.x,
                y: item.y,
                z: item.z,
                length: item.length,
                width: item.width,
                height: item.height,
                aisle_w,
                seats,
                base_above_floor,
            }
        })
        .collect()
}

/// Every registered preset in every user MTOW mode.
pub fn registered_cases() -> Vec<(String, MtowSizing)> {
    let mut cases = Vec::new();
    for name in presets::available() {
        for mode in USER_MODES {
            cases.push((name.to_owned(), mode));
        }
    }
    cases
}

/// Run and judge one registered preset in one mode.
pub fn run_preset_case(preset: &str, mode: MtowSizing) -> Result<CaseOutcome, String> {
    let config = case_config(preset, None, mode)?;
    let result = run_pipeline(&config)?;
    Ok(judge(&label(preset, mode), &result))
}

/// The priced route model that sizes the aircraft, read as a flown mission:
/// the mission rules on its profile (recorded under `<case>#priced`) and its
/// consistency with the native flight.
fn judge_priced(
    out: &mut Findings,
    result: &PipelineResult,
    report: &AnalysisReport,
    native: &MissionFacts,
) {
    let model = match alas_pipeline::fuel_model::report_route_mission_model(&result.config, report)
    {
        Ok(model) => model,
        Err(error) => {
            super::mission_priced::record_unflown(out, "cannot be built", &error);
            return;
        }
    };
    let steps = match super::mission_priced::priced_flight(&model, native) {
        Ok(steps) => steps,
        Err(error) => {
            super::mission_priced::record_unflown(out, "cannot fly the native route", &error);
            return;
        }
    };
    let priced = MissionFacts::from_priced(&steps, native.design_mach);
    let policy = model.profile.cruise_altitude_policy;
    if policy != alas_config::mission::CruiseAltitudePolicy::OptimumStep {
        let declared_m = if policy == alas_config::mission::CruiseAltitudePolicy::Design {
            model.design_cruise_altitude_m
        } else {
            model.cruise_altitude_m
        };
        let flown_m = super::mission_priced::LegTotals::of(&priced).top_m;
        super::mission_priced::check_declared_level(out, declared_m, flown_m);
    }
    let mut priced_out = Findings::new(format!(
        "{}{}",
        out.case(),
        super::mission_priced::PRICED_SUFFIX
    ));
    let mut priced_facts = priced.clone();
    priced_facts.climb_reference = native.climb_reference.clone();
    super::mission_priced::check_priced_profile(&mut priced_out, &priced_facts);
    super::mission_priced::check_consistency(
        out,
        &super::mission_priced::LegTotals::of(native),
        &super::mission_priced::LegTotals::of(&priced),
    );
    super::leg_rules::check_legs(out, native, &steps);
    out.extend(priced_out);
}
