// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What a mission plan has to satisfy before it is flown.

use std::collections::HashSet;

use crate::PayloadItemsConfig;

use super::{CruiseExtent, EventAction, MissionPlan, PlanStage, StageKind};

/// Why a plan, or a new order of its stages, is refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PlanError {
    /// The plan flies no stage.
    #[error("the plan has no stages")]
    Empty,
    /// A flying stage comes before any take-off.
    #[error("Takeoff Must Come First: stage {0} flies before the aircraft has taken off")]
    TakeoffMustComeFirst(String),
    /// The plan ends in the air, or a stage follows the landing.
    #[error(
        "Land Must Be Last: stage {0} is not preceded by a take-off, or the plan ends airborne"
    )]
    LandingMustBeLast(String),
    /// A stage that needs the aircraft on the ground follows a take-off.
    #[error("stage {0} needs the aircraft on the ground")]
    NeedsGround(String),
    /// A go-around that does not follow an approach, or an approach that
    /// ends in neither a landing nor a go-around.
    #[error("Go Around Must Follow An Approach: stage {0}")]
    GoAroundNeedsApproach(String),
    /// Two stages share an identifier.
    #[error("stage id '{0}' is empty or duplicated")]
    DuplicateId(String),
    /// A stage parameter is not a physical number.
    #[error("stage '{stage}': {message}")]
    Parameter {
        /// Stage id.
        stage: String,
        /// What is wrong.
        message: String,
    },
    /// An event refers to something that does not exist or cannot happen.
    #[error("stage '{stage}': {message}")]
    Event {
        /// Stage id.
        stage: String,
        /// What is wrong.
        message: String,
    },
    /// A stage index outside the list.
    #[error("there is no stage {0}")]
    NoSuchStage(usize),
    /// A plan-level number is not physical.
    #[error("{0}")]
    Plan(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Ground,
    Air,
    OnApproach,
}

/// Check the order of `stages`: on the ground, take off, fly, land.
pub(super) fn order(stages: &[PlanStage]) -> Result<(), PlanError> {
    if stages.is_empty() {
        return Err(PlanError::Empty);
    }
    let mut phase = Phase::Ground;
    let mut taken_off = false;
    let mut landed_since_takeoff = false;
    for stage in stages {
        let id = stage.id.clone();
        phase = match (&stage.kind, phase) {
            (StageKind::Taxi { .. }, Phase::Ground) => Phase::Ground,
            (StageKind::Taxi { .. }, _) => return Err(PlanError::NeedsGround(id)),
            (StageKind::Takeoff { .. }, Phase::Ground) => {
                taken_off = true;
                landed_since_takeoff = false;
                Phase::Air
            }
            (StageKind::Takeoff { .. }, _) => return Err(PlanError::NeedsGround(id)),
            (StageKind::Landing { .. }, Phase::Air | Phase::OnApproach) => {
                landed_since_takeoff = true;
                Phase::Ground
            }
            (StageKind::Landing { .. }, Phase::Ground) => {
                return Err(if taken_off {
                    PlanError::LandingMustBeLast(id)
                } else {
                    PlanError::TakeoffMustComeFirst(id)
                })
            }
            (StageKind::Approach { .. }, Phase::Air) => Phase::OnApproach,
            (StageKind::GoAround { .. }, Phase::OnApproach) => Phase::Air,
            (StageKind::GoAround { .. }, _) => return Err(PlanError::GoAroundNeedsApproach(id)),
            (_, Phase::Air) => Phase::Air,
            (_, Phase::OnApproach) => return Err(PlanError::GoAroundNeedsApproach(id)),
            (_, Phase::Ground) => {
                return Err(if landed_since_takeoff {
                    PlanError::LandingMustBeLast(id)
                } else {
                    PlanError::TakeoffMustComeFirst(id)
                })
            }
        };
    }
    let last = stages
        .last()
        .map(|stage| stage.id.clone())
        .unwrap_or_default();
    if phase != Phase::Ground {
        return Err(PlanError::LandingMustBeLast(last));
    }
    if !taken_off {
        return Err(PlanError::TakeoffMustComeFirst(last));
    }
    Ok(())
}

fn positive(stage: &str, name: &str, value: f64) -> Result<(), PlanError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(PlanError::Parameter {
            stage: stage.to_owned(),
            message: format!("{name} must be a positive number, got {value}"),
        })
    }
}

fn altitude(stage: &str, name: &str, value: f64) -> Result<(), PlanError> {
    if value.is_finite() && (-1_500.0..=70_000.0).contains(&value) {
        Ok(())
    } else {
        Err(PlanError::Parameter {
            stage: stage.to_owned(),
            message: format!("{name} must be between -1,500 and 70,000 ft, got {value}"),
        })
    }
}

fn optional_positive(stage: &str, name: &str, value: Option<f64>) -> Result<(), PlanError> {
    value.map_or(Ok(()), |value| positive(stage, name, value))
}

fn parameters(stage: &PlanStage) -> Result<(), PlanError> {
    let id = stage.id.as_str();
    if !stage.extra_drag_area_m2.is_finite() || stage.extra_drag_area_m2 < 0.0 {
        return Err(PlanError::Parameter {
            stage: id.to_owned(),
            message: "the added drag area must not be negative".to_owned(),
        });
    }
    if stage.extra_drag_area_m2 > 0.0 && stage.extra_drag_basis.trim().is_empty() {
        return Err(PlanError::Parameter {
            stage: id.to_owned(),
            message: "an added drag area needs its basis (a source, or 'study input')".to_owned(),
        });
    }
    match &stage.kind {
        StageKind::Taxi { time_min } => positive(id, "time", *time_min),
        StageKind::Takeoff { field } => field.as_ref().map_or(Ok(()), |field| {
            altitude(id, "field elevation", field.elevation_ft)?;
            positive(id, "take-off distance available", field.toda_m)?;
            if field.isa_deviation_k.is_finite() && field.isa_deviation_k.abs() <= 60.0 {
                Ok(())
            } else {
                Err(PlanError::Parameter {
                    stage: id.to_owned(),
                    message: "the temperature deviation must be within 60 K".to_owned(),
                })
            }
        }),
        StageKind::Landing { .. } => Ok(()),
        StageKind::Climb {
            altitude_ft,
            rate_m_s,
            ..
        } => {
            altitude(id, "altitude", *altitude_ft)?;
            optional_positive(id, "rate of climb", *rate_m_s)
        }
        StageKind::Cruise {
            extent,
            altitude_ft,
            speed_m_s,
            ..
        } => {
            match extent {
                CruiseExtent::DistanceKm(km) => positive(id, "distance", *km)?,
                CruiseExtent::TimeMin(min) => positive(id, "time", *min)?,
                CruiseExtent::Remainder => {}
            }
            altitude_ft.map_or(Ok(()), |value| altitude(id, "altitude", value))?;
            optional_positive(id, "speed", *speed_m_s)
        }
        StageKind::Loiter {
            time_min,
            altitude_ft,
            speed_m_s,
            ..
        } => {
            positive(id, "time", *time_min)?;
            altitude_ft.map_or(Ok(()), |value| altitude(id, "altitude", value))?;
            optional_positive(id, "speed", *speed_m_s)
        }
        StageKind::Combat {
            time_min,
            load_factor,
            altitude_ft,
            speed_m_s,
            ..
        } => {
            positive(id, "time", *time_min)?;
            if !load_factor.is_finite() || !(1.0..=12.0).contains(load_factor) {
                return Err(PlanError::Parameter {
                    stage: id.to_owned(),
                    message: format!(
                        "the load factor must be between 1 and 12 g, got {load_factor}"
                    ),
                });
            }
            altitude_ft.map_or(Ok(()), |value| altitude(id, "altitude", value))?;
            optional_positive(id, "speed", *speed_m_s)
        }
        StageKind::Descent { altitude_ft, .. } | StageKind::GoAround { altitude_ft, .. } => {
            altitude(id, "altitude", *altitude_ft)
        }
        StageKind::Approach {
            decision_height_ft,
            field_elevation_ft,
        } => {
            positive(id, "decision height", *decision_height_ft)?;
            field_elevation_ft.map_or(Ok(()), |value| altitude(id, "field elevation", value))
        }
        StageKind::Diversion {
            distance_km,
            cruise_altitude_ft,
            field_elevation_ft,
        } => {
            positive(id, "distance", *distance_km)?;
            altitude(id, "cruise altitude", *cruise_altitude_ft)?;
            field_elevation_ft.map_or(Ok(()), |value| altitude(id, "field elevation", value))
        }
    }
}

fn events(stage: &PlanStage) -> Result<(), PlanError> {
    let bad = |message: String| PlanError::Event {
        stage: stage.id.clone(),
        message,
    };
    for event in &stage.events {
        match &event.action {
            EventAction::PayloadRelease { .. } | EventAction::TrimTankOff => {}
            EventAction::StoreAdd {
                id,
                mass_kg,
                x_m,
                y_m,
                z_m,
                drag_area_m2,
            } => {
                if id.trim().is_empty() {
                    return Err(bad("a store needs an id".to_owned()));
                }
                if !mass_kg.is_finite() || *mass_kg <= 0.0 {
                    return Err(bad(format!("store '{id}' needs a positive mass")));
                }
                if ![x_m, y_m, z_m].iter().all(|v| v.is_finite()) {
                    return Err(bad(format!("store '{id}' needs a finite position")));
                }
                if drag_area_m2.is_some_and(|a| !a.is_finite() || a < 0.0) {
                    return Err(bad(format!("store '{id}' drag area must not be negative")));
                }
            }
            EventAction::AerialRefuel { mass_kg } => {
                if !mass_kg.is_finite() || *mass_kg == 0.0 {
                    return Err(bad("an aerial refuelling needs a non-zero mass".to_owned()));
                }
            }
            EventAction::FuelTransfer { from, to, mass_kg } => {
                if from == to || from.is_empty() || to.is_empty() {
                    return Err(bad("a fuel transfer needs two different tanks".to_owned()));
                }
                if !mass_kg.is_finite() || *mass_kg <= 0.0 {
                    return Err(bad("a fuel transfer needs a positive mass".to_owned()));
                }
            }
            EventAction::TrimTankControl {
                target_min_pct_mac,
                target_max_pct_mac,
            } => {
                let finite = target_min_pct_mac.is_finite() && target_max_pct_mac.is_finite();
                if !finite || target_min_pct_mac > target_max_pct_mac {
                    return Err(bad("the trim band needs min <= max".to_owned()));
                }
            }
        }
    }
    Ok(())
}

impl MissionPlan {
    /// Check the plan on its own: order, identifiers, parameters, events.
    ///
    /// An empty plan (the standard mission) is valid. The release events are
    /// checked against the declared payload by [`Self::validate_items`].
    ///
    /// # Errors
    ///
    /// The first [`PlanError`] found.
    pub fn validate(&self) -> Result<(), PlanError> {
        if !self.contingency_fraction.is_finite()
            || !(0.0..=1.0).contains(&self.contingency_fraction)
        {
            return Err(PlanError::Plan(
                "the contingency fraction must be between 0 and 1".to_owned(),
            ));
        }
        if self
            .takeoff_fuel_kg
            .is_some_and(|kg| !kg.is_finite() || kg < 0.0)
        {
            return Err(PlanError::Plan(
                "the take-off fuel must not be negative".to_owned(),
            ));
        }
        if self
            .balance_target_pct_mac
            .is_some_and(|target| !target.is_finite())
        {
            return Err(PlanError::Plan(
                "the balance target must be a number".to_owned(),
            ));
        }
        if self.stages.is_empty() {
            return Ok(());
        }
        let mut seen = HashSet::new();
        for stage in &self.stages {
            if stage.id.trim().is_empty() || !seen.insert(stage.id.as_str()) {
                return Err(PlanError::DuplicateId(stage.id.clone()));
            }
            parameters(stage)?;
            events(stage)?;
        }
        order(&self.stages)
    }

    /// Check that every release refers to an item that is carried and
    /// releasable at that point of the plan, and that no item is released
    /// twice. A plan without stages is always consistent.
    ///
    /// # Errors
    ///
    /// [`PlanError::Event`] naming the stage and the item.
    pub fn validate_items(&self, items: &PayloadItemsConfig) -> Result<(), PlanError> {
        let mut carried: Vec<(&str, bool)> = items
            .point_masses
            .iter()
            .map(|item| (item.id.as_str(), item.releasable))
            .collect();
        for stage in &self.stages {
            for event in &stage.events {
                let bad = |message: String| PlanError::Event {
                    stage: stage.id.clone(),
                    message,
                };
                match &event.action {
                    EventAction::StoreAdd { id, .. } => {
                        if carried.iter().any(|(other, _)| other == id) {
                            return Err(bad(format!("store id '{id}' is already in use")));
                        }
                        carried.push((id.as_str(), true));
                    }
                    EventAction::PayloadRelease { items: ids } => {
                        for id in ids {
                            match carried.iter().position(|(other, _)| other == id) {
                                None => {
                                    return Err(bad(format!(
                                        "'{id}' is not carried at this point (unknown or already released)"
                                    )))
                                }
                                Some(index) if !carried[index].1 => {
                                    return Err(bad(format!("'{id}' is not releasable")))
                                }
                                Some(index) => {
                                    carried.remove(index);
                                }
                            }
                        }
                        if ids.is_empty() {
                            carried.retain(|(_, releasable)| !releasable);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

impl MissionPlan {
    /// [`Self::validate_items`] as a blocking validation issue, if it fails.
    pub(crate) fn items_issue(&self, items: &PayloadItemsConfig) -> Option<crate::ValidationIssue> {
        self.validate_items(items)
            .err()
            .map(|error| crate::ValidationIssue {
                severity: crate::Severity::Error,
                field_path: "mission.plan".to_owned(),
                message: error.to_string(),
            })
    }
}
