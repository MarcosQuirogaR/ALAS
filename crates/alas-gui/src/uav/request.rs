// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Competition-search requests: validation of the page inputs, the
//! propulsion maps resolved from the catalogue, and the background worker.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use alas_uav::catalog::{Catalog, ComponentKind};
use alas_uav::optimization_catalog;
use alas_uav::optimizer::{
    optimize_competition_observed, CompetitionOptimum, CompetitionProblem, CompetitionProgress,
    DesignObjectives, GeometrySearchBounds, OptimizationError, OptimizationProblem,
    OptimizationProgress, PreliminaryModel, PropulsionMap, SystemsDefinition, TailVolumeBounds,
};
use alas_uav::propulsion_electric::prop_data::prop_data_unavailable_reason;
use alas_uav::propulsion_electric::{native_propulsion_map, CataloguePowertrainSelection};

use super::{CompetitionSettings, UavWorkflowState};

/// Electronics role the search must fill (a GPS sensor in the reviewed catalogue).
const REQUIRED_ELECTRONICS_ROLE: &str = "gps_sensor";
/// Smallest search budget accepted by the competition search.
pub(super) const MINIMUM_EVALUATIONS: usize = 4;
/// Points on the speed grid of each propulsion map (plus stall and cruise).
const SPEED_GRID_POINTS: u16 = 16;

/// Messages from the search worker to the desktop.
#[derive(Debug)]
pub(super) enum UavWorkerMessage {
    /// Monotonic search progress suitable for the frame loop.
    Progress(OptimizationProgress),
    /// The terminal search result.
    Finished(Box<CompetitionFinished>),
}

/// Terminal result of one worker run.
#[derive(Debug)]
pub(super) struct CompetitionFinished {
    pub(super) preset_id: String,
    pub(super) result: Result<CompetitionOptimum, OptimizationError>,
}

/// Immutable snapshot of the page inputs taken when a search starts.
#[derive(Debug)]
pub(super) struct CompetitionRequest {
    preset_id: String,
    settings: CompetitionSettings,
    objectives: DesignObjectives,
    geometry_bounds: GeometrySearchBounds,
    model: PreliminaryModel,
    systems: SystemsDefinition,
    seed: u64,
    pub(super) evaluations: usize,
}

impl UavWorkflowState {
    /// Validate the page inputs and snapshot them for the worker.
    pub(super) fn competition_request(&self) -> Result<CompetitionRequest, String> {
        let preset_id = self.competition.preset_id.clone();
        if !alas_uav::score::presets::ids().contains(&preset_id.as_str()) {
            return Err(format!("Unknown competition preset: {preset_id}"));
        }
        if self.evaluations < MINIMUM_EVALUATIONS {
            return Err(format!(
                "The competition search needs at least {MINIMUM_EVALUATIONS} evaluations."
            ));
        }
        if let Some(reason) = prop_data_unavailable_reason() {
            return Err(format!(
                "The propulsion maps cannot be resolved from the catalogue: {reason}"
            ));
        }
        Ok(CompetitionRequest {
            preset_id,
            settings: self.competition.clone(),
            objectives: self.objectives,
            geometry_bounds: self.geometry_bounds,
            model: self.model,
            systems: self.systems,
            seed: self.seed,
            evaluations: self.evaluations,
        })
    }
}

/// Rule quantities of the preset, keyed as the score engine reads them.
fn rule_inputs(settings: &CompetitionSettings) -> BTreeMap<String, f64> {
    BTreeMap::from([
        ("payload_count".to_owned(), settings.payload_count),
        ("declared_payload_count".to_owned(), settings.payload_count),
        ("payload_volume".to_owned(), settings.payload_volume_m3),
    ])
}

/// Propulsion maps for every motor, propeller and battery-cell count of the
/// catalogue. For each (motor, propeller, cells) the first ESC that resolves
/// is used. Maps are full-throttle, per propulsor, with the speed grid
/// covering take-off (0 m/s) up to 1.5 times the larger objective speed; the reviewed tables bound the upper end.
pub(super) fn competition_maps(
    catalog: &Catalog,
    objectives: &DesignObjectives,
    air_density_kg_m3: f64,
) -> Result<Vec<PropulsionMap>, String> {
    let top = 1.5
        * objectives
            .cruise_speed_m_s
            .max(objectives.maximum_stall_speed_m_s);
    let mut speeds: Vec<f64> = (0..=SPEED_GRID_POINTS)
        .map(|index| top * f64::from(index) / f64::from(SPEED_GRID_POINTS))
        .collect();
    speeds.push(objectives.maximum_stall_speed_m_s);
    speeds.push(objectives.cruise_speed_m_s);
    speeds.sort_by(f64::total_cmp);
    speeds.dedup_by(|a, b| (*a - *b).abs() < 1e-9);

    let batteries: Vec<(&str, u16)> = catalog
        .records
        .iter()
        .filter_map(|record| match &record.kind {
            ComponentKind::Battery(spec) => {
                spec.series_cells.map(|cells| (record.id.as_str(), cells))
            }
            _ => None,
        })
        .collect();
    let motors = ids_of(catalog, |kind| matches!(kind, ComponentKind::Motor(_)));
    let propellers = ids_of(catalog, |kind| matches!(kind, ComponentKind::Propeller(_)));
    let escs = ids_of(catalog, |kind| matches!(kind, ComponentKind::Esc(_)));
    let mut cell_counts: Vec<u16> = batteries.iter().map(|(_, cells)| *cells).collect();
    cell_counts.sort_unstable();
    cell_counts.dedup();

    let mut maps: Vec<PropulsionMap> = Vec::new();
    // The first resolver error is kept so a refusal names its cause.
    let mut first_error: Option<String> = None;
    for motor in &motors {
        for propeller in &propellers {
            for cells in &cell_counts {
                let Some((battery, _)) = batteries.iter().find(|(_, c)| c == cells) else {
                    continue;
                };
                for esc in &escs {
                    let selection = CataloguePowertrainSelection {
                        battery_id: (*battery).to_owned(),
                        motor_id: motor.clone(),
                        esc_id: esc.clone(),
                        propeller_id: propeller.clone(),
                        motor_count: 1,
                    };
                    match native_propulsion_map(catalog, &selection, &speeds, air_density_kg_m3) {
                        Ok(map) => {
                            maps.push(map);
                            break;
                        }
                        Err(error) => {
                            first_error.get_or_insert_with(|| error.to_string());
                        }
                    }
                }
            }
        }
    }
    if maps.is_empty() {
        return Err(format!(
            "No propulsion map could be resolved from the catalogue. First resolver error: {}",
            first_error.unwrap_or_else(|| "no motor, propeller and battery combination".to_owned())
        ));
    }
    Ok(maps)
}

fn ids_of(catalog: &Catalog, keep: impl Fn(&ComponentKind) -> bool) -> Vec<String> {
    catalog
        .records
        .iter()
        .filter(|record| keep(&record.kind))
        .map(|record| record.id.clone())
        .collect()
}

/// Run one competition search in the background and report progress.
pub(super) fn run_worker(
    request: CompetitionRequest,
    sender: Sender<UavWorkerMessage>,
    cancel_flag: Arc<AtomicBool>,
) {
    let result = run_search(&request, &sender, &cancel_flag);
    let _ = sender.send(UavWorkerMessage::Finished(Box::new(CompetitionFinished {
        preset_id: request.preset_id,
        result,
    })));
}

fn run_search(
    request: &CompetitionRequest,
    sender: &Sender<UavWorkerMessage>,
    cancel_flag: &Arc<AtomicBool>,
) -> Result<CompetitionOptimum, OptimizationError> {
    let catalog = optimization_catalog().map_err(|error| {
        OptimizationError::InvalidProblem(format!("catalogue unavailable: {error}"))
    })?;
    let maps = competition_maps(
        catalog,
        &request.objectives,
        request.model.air_density_kg_m3,
    )
    .map_err(OptimizationError::InvalidProblem)?;
    let base = OptimizationProblem {
        catalog,
        propulsion_maps: &maps,
        mission_profile: None,
        objectives: request.objectives,
        geometry_bounds: request.geometry_bounds,
        model: request.model,
        systems: request.systems,
        required_electronics_role: REQUIRED_ELECTRONICS_ROLE,
        seed: request.seed,
        evaluations: request.evaluations,
    };
    let problem = CompetitionProblem {
        base,
        preset_id: &request.preset_id,
        preset_variant: None,
        announced_takeoff_m: request.settings.announced_takeoff_m,
        extra_inputs: rule_inputs(&request.settings),
        tail_volume_bounds: TailVolumeBounds::default(),
        population: None,
        include_landing: true,
    };
    let stride = (request.evaluations / 100).max(1);
    let mut observer = |progress: CompetitionProgress| {
        if progress.evaluated == progress.budget || progress.evaluated % stride == 0 {
            let _ = sender.send(UavWorkerMessage::Progress(OptimizationProgress {
                evaluated_candidates: progress.evaluated,
                total_candidates: progress.budget,
                verified_candidates: progress.feasible,
                best_score: progress.best_score,
            }));
        }
        !cancel_flag.load(Ordering::Relaxed)
    };
    optimize_competition_observed(&problem, &mut observer)
}
