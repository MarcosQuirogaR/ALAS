// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Partial physics runs (`alas --only <stages>`): one or a few physics stages
//! of the no-optimisation pipeline, with only the stages they need.
//!
//! Every stage is the pipeline's own code on the pipeline's own inputs: the
//! route is planned and recorded on the configuration as [`DesignPipeline::run`]
//! does, the full analysis runs with the baseline fuel artifacts beside it, and
//! the delivered report is the CPACS-canonical one the downstream stages read.
//! A partial run therefore reports what the same stage of a full
//! `--no-optimize` run reports.
//!
//! Stage dependencies:
//!
//! | stage        | needs                                    |
//! |--------------|------------------------------------------|
//! | geometry     | configuration                            |
//! | mass         | geometry                                 |
//! | aero         | the full analysis report (geometry, mass) |
//! | stability    | the full analysis report                 |
//! | field        | the report                               |
//! | mission      | the report and the planned route         |
//! | structures   | the report                               |
//! | feasibility  | the report, mission and structures       |
//!
//! # Cache
//!
//! The delivered report is the expensive upstream (the lattice trims and the
//! baseline sizing closure). A partial run that does not ask for `aero` or
//! `stability` reads it from the cache folder when an entry for the same
//! configuration and design exists, and writes one when it computes it. The
//! key is a hash of the complete effective configuration (route distance
//! included) and the design vector's bits; the entry records the program
//! version and build time it was computed with. A cached report is **not**
//! invalidated by a rebuild of the program: that is the point (isolate the
//! stage under work from the stages that are not), so a change to the
//! geometry, mass or aerodynamic code needs `--refresh-cache` or an
//! `--only aero` run, which always recomputes and rewrites the entry. The
//! stages asked for are always recomputed.

use std::path::PathBuf;
use std::time::Instant;

use alas_config::design_variables::DesignVector;
use alas_exec::RunEnvironment;
use serde_json::{json, Value};

use super::{prepare_analysis_workspace, DesignPipeline, MissionStageOutputs, PlannedRoute};
use crate::cpacs::{export_cpacs_document, read_cpacs};
use crate::feasibility::assess_physical_feasibility_on_route;
use crate::full_analysis::{AnalysisReport, FullAnalysis};

mod cache;
mod debug_json;
mod stage;

use cache::{digest, read_cached, stamp, write_cached, CacheEntry};
pub use stage::PartialStage;

/// What a partial run does and where it keeps its cache.
#[derive(Debug, Clone)]
pub struct PartialOptions {
    /// The stages to report, ascending.
    pub stages: Vec<PartialStage>,
    /// The cache folder.
    pub cache_dir: PathBuf,
    /// Recompute the report even when the cache has it, and rewrite it.
    pub refresh: bool,
    /// The run's workspace for exports and structural files; a temporary one
    /// when `None`.
    pub output_dir: Option<PathBuf>,
}

/// A result type without `Serialize`, as JSON read from its derived `Debug`
/// text (see `debug_json`); non-finite numbers are `null`.
fn debug_value<T: std::fmt::Debug>(value: &T) -> Value {
    debug_json::debug_json(value)
}

impl DesignPipeline {
    /// Run the stages `options` asks for, and the stages they need.
    ///
    /// The result is a JSON object with one entry per stage asked for, plus
    /// `timing_s` (per step) and `cache` (whether the report was read from
    /// or written to the cache).
    ///
    /// # Errors
    ///
    /// A stage the requested ones need fails; the message names it.
    pub fn run_partial(
        &self,
        options: &PartialOptions,
        environment: &RunEnvironment,
    ) -> Result<Value, String> {
        let run_clock = Instant::now();
        let _sizing_memo = alas_opt::mdo::sizing_memo_scope();
        if self.aircraft_override.is_some() {
            return Err("--only does not take a CPACS input yet".to_owned());
        }
        let asks = |stage: PartialStage| options.stages.contains(&stage);
        let mut timing = serde_json::Map::new();
        let mut out = serde_json::Map::new();
        let mut time = |name: &str, since: Instant| {
            timing.insert(name.to_owned(), json!(since.elapsed().as_secs_f64()));
        };

        // The route is part of the configuration every later stage reads
        // (`MissionConfig::route_distance_m`), exactly as in `run_inner`.
        let clock = Instant::now();
        let planned_route: Option<PlannedRoute> = if self.config.mission.enabled {
            self.plan_active_route(None)?
        } else {
            None
        };
        let planned_distance_m = planned_route
            .as_ref()
            .map_or(0.0, |planned| planned.route.total_distance_m());
        let recorded = DesignPipeline {
            config: alas_config::AlasConfig {
                mission: alas_config::MissionConfig {
                    route_distance_m: planned_distance_m,
                    ..self.config.mission.clone()
                },
                ..self.config.clone()
            },
            ..self.clone()
        };
        time("route", clock);
        let nominal_design = recorded.config.band_design(
            recorded
                .imported_design
                .unwrap_or_else(|| recorded.configured_nominal_design()),
        );
        let band = recorded.at_band_design(&nominal_design);
        let this = band.as_ref().unwrap_or(&recorded);
        let full = FullAnalysis::new(this.config.clone());

        if asks(PartialStage::Geometry) {
            let clock = Instant::now();
            let plane = full.build_airplane(&nominal_design, true)?;
            out.insert(
                "geometry".to_owned(),
                json!({
                    "s_ref_m2": plane.s_ref,
                    "b_ref_m": plane.b_ref,
                    "c_ref_m": plane.c_ref,
                    "surfaces": plane.wings.iter().map(|wing| json!({
                        "name": wing.name,
                        "area_m2": wing.area(),
                        "span_m": wing.span(),
                        "sections": wing.xsecs.len(),
                    })).collect::<Vec<_>>(),
                    "fuselages": plane.fuselages.len(),
                    "design_vector": nominal_design,
                }),
            );
            time("geometry", clock);
        }
        let needs_report = options.stages.iter().any(|stage| stage.needs_report());
        if asks(PartialStage::Mass) && !needs_report {
            let clock = Instant::now();
            let plane = full.build_airplane(&nominal_design, true)?;
            let mass = full.mass_stage(&nominal_design, plane)?;
            out.insert("mass".to_owned(), mass);
            time("mass", clock);
        }
        if !needs_report {
            out.insert("timing_s".to_owned(), Value::Object(timing));
            out.insert(
                "wall_s".to_owned(),
                json!(run_clock.elapsed().as_secs_f64()),
            );
            return Ok(Value::Object(out));
        }

        // The delivered report: cached unless the aerodynamics are the stage
        // under test or a refresh is asked for.
        let key = digest(&format!(
            "{:?}|{:?}",
            this.config,
            nominal_design
                .to_array()
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>()
        ));
        let cache_path = options.cache_dir.join(format!("report-{key}.yaml"));
        let recompute =
            options.refresh || asks(PartialStage::Aero) || asks(PartialStage::Stability);
        let clock = Instant::now();
        let cached = (!recompute).then(|| read_cached(&cache_path)).flatten();
        let (report, cache_note) = match cached {
            Some(entry) => {
                let note = json!({
                    "report": "read",
                    "path": cache_path.display().to_string(),
                    "computed_with": entry.program_version,
                    "built": entry.built,
                    "original_compute_s": entry.computed_s,
                });
                (entry.report, note)
            }
            None => {
                let report = delivered_report(&full, &nominal_design, this, options)?;
                let computed_s = clock.elapsed().as_secs_f64();
                let entry = CacheEntry {
                    program_version: env!("CARGO_PKG_VERSION").to_owned(),
                    built: stamp(),
                    computed_s,
                    report,
                };
                let note = match write_cached(&cache_path, &entry) {
                    Ok(()) => {
                        json!({"report": "computed and written", "path": cache_path.display().to_string()})
                    }
                    Err(error) => json!({"report": "computed", "write_error": error}),
                };
                (entry.report, note)
            }
        };
        time("report", clock);
        out.insert("cache".to_owned(), cache_note);
        let delivered = crate::gear_stations::report_config(&this.config, &report);

        if asks(PartialStage::Mass) {
            out.insert(
                "mass".to_owned(),
                json!({
                    "component_masses_kg": report.component_masses,
                    "total_mass_kg": report.component_masses.values().sum::<f64>(),
                    "physical_cg_m": report.physical_cg,
                    "mass_coordinates_m": report.mass_coordinates,
                    "cg_envelope_ok": report.cg_envelope_ok,
                }),
            );
        }
        if asks(PartialStage::Aero) {
            out.insert(
                "aero".to_owned(),
                json!({
                    "design_point": report.design_point,
                    "polar_fit": report.polar_fit,
                    "trimmed_design_point": report.trimmed_design_point,
                    "polar": debug_value(&report.polar),
                }),
            );
        }
        if asks(PartialStage::Stability) {
            out.insert(
                "stability".to_owned(),
                json!({
                    "x_neutral_point_m": report.x_neutral_point,
                    "static_margin": report.static_margin,
                    "physical_cg_m": report.physical_cg,
                    "cg_envelope_ok": report.cg_envelope_ok,
                    "neutral_point_conditions": debug_value(&report.neutral_point_conditions),
                }),
            );
        }
        if asks(PartialStage::Field) {
            let clock = Instant::now();
            let mtow = delivered.requirements.mtow_kg;
            let mlw = delivered.landing_mass_limit_kg(mtow);
            let field = crate::field_reference::report_isa_sea_level_field_reference(
                &delivered,
                &report,
                report.airplane.s_ref,
                mtow,
                mlw,
            );
            out.insert(
                "field".to_owned(),
                match field {
                    Ok(field) => debug_value(&field),
                    Err(error) => json!({ "error": error }),
                },
            );
            time("field", clock);
        }
        let needs_mission = asks(PartialStage::Mission) || asks(PartialStage::Feasibility);
        let mission: Option<MissionStageOutputs> = if needs_mission {
            let clock = Instant::now();
            let outputs = this.evaluate_active_mission(&report, planned_route.as_ref(), None)?;
            time("mission", clock);
            Some(outputs)
        } else {
            None
        };
        if asks(PartialStage::Mission) {
            if let Some((route, status, result, load_case)) = &mission {
                out.insert(
                    "mission".to_owned(),
                    json!({
                        "route_distance_m": route.as_ref().map(|r| r.total_distance_m()),
                        "route_status": debug_value(status),
                        "summary": result.as_ref().map(|r| debug_value(&r.completed_summary())),
                        "load_case": debug_value(load_case),
                    }),
                );
            }
        }
        let needs_structures = asks(PartialStage::Structures) || asks(PartialStage::Feasibility);
        let structural = if needs_structures && delivered.structures.enabled {
            let clock = Instant::now();
            let analysis_dir = prepare_analysis_workspace(options.output_dir.clone())?;
            let structural_config =
                crate::feasibility::structure::design_config(&delivered, &report);
            let result = crate::structural::run_structural_analysis_with_environment_events(
                &structural_config,
                &report,
                Some(analysis_dir.join("structures").as_path()),
                environment,
                None,
                run_clock,
            );
            time("structures", clock);
            Some(result)
        } else {
            None
        };
        if asks(PartialStage::Structures) {
            out.insert("structures".to_owned(), debug_value(&structural));
        }
        if asks(PartialStage::Feasibility) {
            let clock = Instant::now();
            let route = planned_route.as_ref().and_then(|planned| {
                crate::mission_route::route_endpoints(&this.config, &planned.route).ok()
            });
            let (mission_result, load_case) = mission
                .as_ref()
                .map_or((None, None), |(_, _, result, load)| {
                    (result.as_ref(), load.as_ref())
                });
            let mut feasibility = assess_physical_feasibility_on_route(
                &delivered,
                &nominal_design,
                &report,
                mission_result,
                load_case,
                route
                    .as_ref()
                    .map(|(origin, destination)| (*origin, *destination)),
            );
            crate::feasibility::structure::append_delivery(
                &delivered,
                &nominal_design,
                &report,
                structural.as_ref(),
                &mut feasibility.findings,
            );
            out.insert(
                "feasibility".to_owned(),
                json!({
                    "summary": crate::feasibility::format_feasibility(&feasibility),
                    "findings": debug_value(&feasibility.findings),
                }),
            );
            time("feasibility", clock);
        }
        out.insert("timing_s".to_owned(), Value::Object(timing));
        out.insert(
            "wall_s".to_owned(),
            json!(run_clock.elapsed().as_secs_f64()),
        );
        Ok(Value::Object(out))
    }
}

/// The full-analysis report the no-optimisation pipeline delivers to its
/// downstream stages: analysed with the baseline fuel artifacts beside it,
/// then given the CPACS-canonical airplane (`run_planned`, stages 3 and 4).
fn delivered_report(
    full: &FullAnalysis,
    design: &DesignVector,
    this: &DesignPipeline,
    options: &PartialOptions,
) -> Result<AnalysisReport, String> {
    let mut report = crate::fuel_model::run_with_baseline_artifacts(full, design)?;
    let delivered = crate::gear_stations::report_config(&this.config, &report);
    let analysis_dir = prepare_analysis_workspace(options.output_dir.clone())?;
    let cpacs_path = analysis_dir.join("cpacs/optimized_aircraft.cpacs.xml");
    let (_, xml) = export_cpacs_document(&report, &delivered, &cpacs_path)
        .map_err(|error| format!("CPACS export failed: {error}"))?;
    let document =
        read_cpacs(&xml).map_err(|error| format!("CPACS canonicalization read failed: {error}"))?;
    let mut canonical = document
        .to_airplane()
        .map_err(|error| format!("CPACS canonicalization failed: {error}"))?;
    super::helpers::carry_reference_wing(&mut canonical, &report.airplane);
    super::helpers::carry_body_lofts(&mut canonical, &report.airplane);
    report.airplane = canonical;
    Ok(report)
}
