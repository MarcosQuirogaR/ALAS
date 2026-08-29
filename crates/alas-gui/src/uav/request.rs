// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Snapshot construction and background execution for the UAV workflow.
//!
//! The desktop state deliberately owns only editable values. This module turns
//! that state into immutable source-bounded solver input, so a worker cannot
//! observe a half-edited component selection or silently revive the former
//! manual two-point propulsion path.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use alas_uav::optimizer::{
    optimize_with_control, MissionProfile, OptimizationError, OptimizationProblem,
    OptimizationProgress, OptimizedUav, PropulsionMap, PropulsionOperatingPoint,
};
use alas_uav::propulsion_electric::{
    native_propulsion_map, simulate_catalogue_mission, CataloguePowertrainSelection,
    ElectricFlightCondition, ElectricMissionPhase, ElectricMissionPlan, ElectricMissionResult,
};
use alas_uav::{
    optimization_catalog, verify_with_shared_core, Catalog, ComponentKind, ComponentRecord,
    SharedCoreFailure, SharedCoreVerification,
};

use super::{ComponentRole, MissionPlanMode, PropulsionInputMode, UavSelections, UavWorkflowState};

/// Result messages that cross from the owned solver worker to the desktop.
#[derive(Debug)]
pub(super) enum UavWorkerMessage {
    /// Monotonic optimizer progress suitable for the frame loop.
    Progress(OptimizationProgress),
    /// A terminal optimizer result and the independently resolved electrical mission.
    Finished(Box<UavWorkerFinished>),
}

/// Result that retains source-solved mission output even when no geometry is feasible.
#[derive(Debug)]
pub(super) struct UavWorkerFinished {
    pub(super) result: Result<UavAccepted, OptimizationError>,
    pub(super) electrical_mission: Option<ElectricMissionResult>,
}

#[derive(Debug)]
pub(super) struct UavAccepted {
    pub(super) optimized: OptimizedUav,
    pub(super) shared_core: Result<Box<SharedCoreVerification>, SharedCoreFailure>,
}

#[derive(Debug)]
pub(super) struct UavRunRequest {
    catalog: Catalog,
    propulsion_maps: Vec<PropulsionMap>,
    mission_profile: Option<MissionProfile>,
    electrical_mission: Option<ElectricMissionResult>,
    objectives: alas_uav::optimizer::DesignObjectives,
    geometry_bounds: alas_uav::optimizer::GeometrySearchBounds,
    model: alas_uav::optimizer::PreliminaryModel,
    systems: alas_uav::optimizer::SystemsDefinition,
    required_electronics_role: String,
    seed: u64,
    pub(super) evaluations: usize,
    shared_core_inputs: alas_uav::SharedCoreInputs,
}

impl UavWorkflowState {
    pub(super) fn run_request(&self) -> Result<UavRunRequest, String> {
        let catalog = self.selected_catalog()?;
        let (propulsion_maps, mission_profile, electrical_mission) = match self
            .propulsion_input_mode
        {
            PropulsionInputMode::Automatic => {
                let selection = CataloguePowertrainSelection {
                    battery_id: self.selections.battery.clone(),
                    motor_id: self.selections.motor.clone(),
                    esc_id: self.selections.esc.clone(),
                    propeller_id: self.selections.propeller.clone(),
                    motor_count: self.propulsion_motor_count,
                };
                let mission_plan = self.electric_mission_plan()?;
                let speeds = map_speeds(
                    &mission_plan,
                    self.objectives.maximum_stall_speed_m_s,
                    self.objectives.cruise_speed_m_s,
                )?;
                let map = native_propulsion_map(
                    &catalog,
                    &selection,
                    &speeds,
                    self.model.air_density_kg_m3,
                )
                .map_err(|error| {
                    format!(
                        "Automatic propulsion calculation could not resolve the selected hardware: {error}"
                    )
                })?;
                let mission = simulate_catalogue_mission(&catalog, &selection, &mission_plan)
                    .map_err(|error| {
                        format!(
                            "Automatic mission calculation could not resolve every phase: {error}"
                        )
                    })?;
                let profile = mission.optimizer_profile();
                (vec![map], Some(profile), Some(mission))
            }
            PropulsionInputMode::AdvancedManual => {
                if self.propulsion_evidence.trim().is_empty() {
                    return Err(
                        "Propulsion-map evidence is required; static thrust is not a cruise model."
                            .to_owned(),
                    );
                }
                (
                    vec![PropulsionMap {
                        motor_id: self.selections.motor.clone(),
                        propeller_id: self.selections.propeller.clone(),
                        series_cells: self.propulsion_series_cells,
                        motor_count: self.propulsion_motor_count,
                        evidence: self.propulsion_evidence.clone(),
                        points: vec![
                            PropulsionOperatingPoint {
                                speed_m_s: self.objectives.maximum_stall_speed_m_s,
                                thrust_n: self.stall_thrust_n,
                                motor_current_a: self.stall_current_a,
                                motor_power_w: self.stall_power_w,
                            },
                            PropulsionOperatingPoint {
                                speed_m_s: self.objectives.cruise_speed_m_s,
                                thrust_n: self.cruise_thrust_n,
                                motor_current_a: self.cruise_current_a,
                                motor_power_w: self.cruise_power_w,
                            },
                        ],
                    }],
                    None,
                    None,
                )
            }
        };
        Ok(UavRunRequest {
            catalog,
            propulsion_maps,
            mission_profile,
            electrical_mission,
            objectives: self.objectives,
            geometry_bounds: self.geometry_bounds,
            model: self.model,
            systems: self.systems,
            required_electronics_role: self.required_electronics_role.clone(),
            seed: self.seed,
            evaluations: self.evaluations,
            shared_core_inputs: self.shared_core_inputs.clone(),
        })
    }

    /// Reset editable phases to the source-bounded standard mission template.
    pub fn reset_advanced_mission(&mut self) {
        self.mission_phases =
            standard_mission_phases(self.objectives, self.model.air_density_kg_m3)
                .unwrap_or_default();
    }

    fn electric_mission_plan(&self) -> Result<ElectricMissionPlan, String> {
        let phases = match self.mission_plan_mode {
            MissionPlanMode::Standard => {
                standard_mission_phases(self.objectives, self.model.air_density_kg_m3)?
            }
            MissionPlanMode::Advanced => {
                if self.mission_phases.is_empty() {
                    return Err(
                        "The advanced electrical mission needs at least one positive-duration phase."
                            .to_owned(),
                    );
                }
                self.mission_phases.clone()
            }
        };
        Ok(ElectricMissionPlan {
            evidence: match self.mission_plan_mode {
                MissionPlanMode::Standard => {
                    "native four-phase mission derived from endurance and range objectives"
                        .to_owned()
                }
                MissionPlanMode::Advanced => {
                    "native electrical solver with designer-entered steady mission phases"
                        .to_owned()
                }
            },
            phases,
        })
    }

    fn selected_catalog(&self) -> Result<Catalog, String> {
        let available = optimization_catalog().map_err(ToString::to_string)?;
        let mut records = Vec::with_capacity(ComponentRole::ALL.len());
        for role in ComponentRole::ALL {
            let id = self.selections.get(role);
            if !available
                .records
                .iter()
                .any(|record| role_matches(role, &record.kind))
            {
                return Err(format!(
                    "The catalogue has no priced, analysis-complete {}. Add primary-source evidence before running the solver.",
                    role.label()
                ));
            }
            let record = available
                .get(id)
                .filter(|record| role_matches(role, &record.kind))
                .ok_or_else(|| format!("No source-reviewed {} is selected.", role.label()))?;
            records.push(record.clone());
        }
        let catalog = Catalog {
            schema_version: available.schema_version,
            records,
        };
        catalog.validate().map_err(|error| error.to_string())?;
        Ok(catalog)
    }
}

/// Run an immutable request in the background without touching GUI state.
pub(super) fn run_worker(
    request: UavRunRequest,
    sender: Sender<UavWorkerMessage>,
    cancel_flag: Arc<AtomicBool>,
) {
    let progress_sender = sender.clone();
    let stride = (request.evaluations / 100).max(1);
    let result = {
        let problem = OptimizationProblem {
            catalog: &request.catalog,
            propulsion_maps: &request.propulsion_maps,
            mission_profile: request.mission_profile.as_ref(),
            objectives: request.objectives,
            geometry_bounds: request.geometry_bounds,
            model: request.model,
            systems: request.systems,
            required_electronics_role: &request.required_electronics_role,
            seed: request.seed,
            evaluations: request.evaluations,
        };
        optimize_with_control(
            &problem,
            |progress| {
                if progress.evaluated_candidates == progress.total_candidates
                    || progress.evaluated_candidates % stride == 0
                {
                    let _ = progress_sender.send(UavWorkerMessage::Progress(progress));
                }
            },
            || cancel_flag.load(Ordering::Relaxed),
        )
    }
    .and_then(|optimized| {
        if cancel_flag.load(Ordering::Relaxed) {
            return Err(OptimizationError::Cancelled {
                evaluated_candidates: request.evaluations,
            });
        }
        Ok(UavAccepted {
            shared_core: verify_with_shared_core(&optimized, &request.shared_core_inputs)
                .map(Box::new),
            optimized,
        })
    });
    let _ = sender.send(UavWorkerMessage::Finished(Box::new(UavWorkerFinished {
        result,
        electrical_mission: request.electrical_mission,
    })));
}

pub(super) fn default_selections() -> UavSelections {
    let mut selections = UavSelections::default();
    if let Ok(catalog) = optimization_catalog() {
        for role in ComponentRole::ALL {
            if let Some(record) = catalog
                .records
                .iter()
                .find(|record| role_matches(role, &record.kind))
            {
                selections.set(role, record.id.clone());
            }
        }
        for (role, id) in [
            (
                ComponentRole::Battery,
                "unmannedtech-gensace-gtech-2200-3s-45c-xt60",
            ),
            (ComponentRole::Motor, "tmotor-at2814-900kv"),
            (ComponentRole::Esc, "hobbywing-skywalker-30a-v2-mini"),
            (ComponentRole::Propeller, "apc-12x6e"),
        ] {
            if catalog
                .get(id)
                .is_some_and(|record| role_matches(role, &record.kind))
            {
                selections.set(role, id.to_owned());
            }
        }
    }
    selections
}

pub(super) fn selected_record(
    selections: &UavSelections,
    role: ComponentRole,
) -> Option<&'static ComponentRecord> {
    optimization_catalog()
        .ok()
        .and_then(|catalog| catalog.get(selections.get(role)))
        .filter(|record| role_matches(role, &record.kind))
}

pub(super) fn role_matches(role: ComponentRole, kind: &ComponentKind) -> bool {
    matches!(
        (role, kind),
        (ComponentRole::Battery, ComponentKind::Battery(_))
            | (ComponentRole::Motor, ComponentKind::Motor(_))
            | (ComponentRole::Esc, ComponentKind::Esc(_))
            | (ComponentRole::Propeller, ComponentKind::Propeller(_))
            | (ComponentRole::Servo, ComponentKind::Servo(_))
            | (ComponentRole::Material, ComponentKind::MaterialStock(_))
            | (ComponentRole::Receiver, ComponentKind::Receiver(_))
            | (ComponentRole::Electronics, ComponentKind::Electronics(_))
            | (ComponentRole::LandingGear, ComponentKind::LandingGear(_))
    )
}

fn map_speeds(
    mission: &ElectricMissionPlan,
    stall_speed_m_s: f64,
    cruise_speed_m_s: f64,
) -> Result<Vec<f64>, String> {
    let mut speeds = vec![stall_speed_m_s, cruise_speed_m_s];
    speeds.extend(mission.phases.iter().map(|phase| phase.condition.speed_m_s));
    if speeds
        .iter()
        .any(|speed| !speed.is_finite() || *speed < 0.0)
    {
        return Err("Mission and objective speeds must be finite and non-negative.".to_owned());
    }
    speeds.sort_by(f64::total_cmp);
    speeds.dedup_by(|lower, upper| (*lower - *upper).abs() <= f64::EPSILON);
    if speeds.len() < 2 {
        return Err(
            "The mission needs distinct stall and cruise speeds to generate a propulsion map."
                .to_owned(),
        );
    }
    Ok(speeds)
}

fn standard_mission_phases(
    objectives: alas_uav::optimizer::DesignObjectives,
    air_density_kg_m3: f64,
) -> Result<Vec<ElectricMissionPhase>, String> {
    for (name, value) in [
        ("endurance", objectives.endurance_s),
        ("range", objectives.range_m),
        ("cruise speed", objectives.cruise_speed_m_s),
        ("stall speed", objectives.maximum_stall_speed_m_s),
        ("air density", air_density_kg_m3),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(format!(
                "{name} must be positive before generating a mission plan."
            ));
        }
    }
    if objectives.cruise_speed_m_s <= objectives.maximum_stall_speed_m_s {
        return Err("Cruise speed must exceed the maximum stall speed.".to_owned());
    }
    let required_duration_s = objectives
        .endurance_s
        .max(objectives.range_m / objectives.cruise_speed_m_s);
    let departure_duration_s = (0.03 * required_duration_s).clamp(20.0, 60.0);
    let climb_duration_s = (0.07 * required_duration_s).clamp(20.0, 120.0);
    let reserve_duration_s = (0.05 * required_duration_s).clamp(20.0, 60.0);
    let low_speed_m_s = objectives.maximum_stall_speed_m_s;
    let climb_speed_m_s = 0.5 * (low_speed_m_s + objectives.cruise_speed_m_s);
    let non_cruise_duration_s = departure_duration_s + climb_duration_s + reserve_duration_s;
    let non_cruise_distance_m = low_speed_m_s * (departure_duration_s + reserve_duration_s)
        + climb_speed_m_s * climb_duration_s;
    let cruise_duration_s = (objectives.endurance_s - non_cruise_duration_s)
        .max(0.0)
        .max((objectives.range_m - non_cruise_distance_m).max(0.0) / objectives.cruise_speed_m_s)
        .max(1.0);
    let phase = |name: &str, duration_s, speed_m_s, throttle| ElectricMissionPhase {
        name: name.to_owned(),
        duration_s,
        condition: ElectricFlightCondition {
            speed_m_s,
            air_density_kg_m3,
            throttle,
        },
    };
    Ok(vec![
        phase("departure", departure_duration_s, low_speed_m_s, 1.0),
        phase("climb", climb_duration_s, climb_speed_m_s, 0.95),
        phase(
            "cruise",
            cruise_duration_s,
            objectives.cruise_speed_m_s,
            0.95,
        ),
        phase("approach reserve", reserve_duration_s, low_speed_m_s, 0.95),
    ])
}
