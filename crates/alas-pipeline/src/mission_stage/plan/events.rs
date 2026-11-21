// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mass and tank events of a plan, resolved against the declared payload.
//!
//! An event is attached to a stage and happens at its start or its end. In the
//! schedule that is a boundary: the start of the first leg of a later stage.
//! The legs are split, merged and joined as the guidance revises them, so the
//! boundary is always found again from the tags (`<name>@<stage>`), never
//! stored as an index.

use alas_config::{EventAction, EventTime, MissionPlan, StageKind};
use alas_mission::segments::SegmentSpec;
use alas_mission::MassChange;
use alas_payload::{DeckItem, DeclaredMeta, ItemKind, ItemMeta, PayloadLayout};

use super::legs::stage_of_tag;

/// A declared mass that can be carried, released or added.
#[derive(Debug, Clone, PartialEq)]
pub struct CarriedItem {
    /// Identifier.
    pub id: String,
    /// Mass, kg.
    pub mass_kg: f64,
    /// Position, m (x aft of the nose, y starboard, z up).
    pub position_m: [f64; 3],
    /// Parasite drag area while carried, m2.
    pub drag_area_m2: f64,
    /// Whether a release event may drop it.
    pub releasable: bool,
    /// Principal inertias about its own centre, kg m2.
    pub inertia_kgm2: Option<[f64; 3]>,
}

impl CarriedItem {
    fn from_deck_item(item: &DeckItem, meta: &DeclaredMeta) -> Self {
        Self {
            id: meta.id.clone(),
            mass_kg: item.mass,
            position_m: [item.x, item.y, item.z],
            drag_area_m2: meta.drag_area_m2.unwrap_or(0.0),
            releasable: meta.releasable,
            inertia_kgm2: meta.inertia_kgm2,
        }
    }
}

/// What an event did, with the numbers resolved.
#[derive(Debug, Clone, PartialEq)]
pub enum Applied {
    /// Items left the aircraft.
    Released(Vec<CarriedItem>),
    /// An item was hung on.
    Added(CarriedItem),
    /// Fuel was received (positive) or given away (negative) in flight.
    Refuel {
        /// Fuel mass, kg.
        mass_kg: f64,
    },
    /// Fuel moved between two tanks.
    Transfer {
        /// Source tank id.
        from: String,
        /// Destination tank id.
        to: String,
        /// Mass, kg.
        mass_kg: f64,
    },
    /// The trim tank holds the centre of gravity in a band from here on.
    TrimOn {
        /// Forward edge, percent MAC.
        min_pct_mac: f64,
        /// Aft edge, percent MAC.
        max_pct_mac: f64,
    },
    /// The trim control is released.
    TrimOff,
    /// Taxi fuel burned on the ground.
    Taxi {
        /// Fuel burned, kg.
        fuel_kg: f64,
    },
}

/// One event in flight order.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedEvent {
    /// The stage it belongs to.
    pub stage: usize,
    /// Start or end of the stage.
    pub at: EventTime,
    /// What it did.
    pub applied: Applied,
}

impl ResolvedEvent {
    /// The mass change it makes, kg.
    pub(super) fn change(&self) -> MassChange {
        match &self.applied {
            Applied::Released(items) => MassChange {
                dry_kg: -items.iter().map(|item| item.mass_kg).sum::<f64>(),
                fuel_kg: 0.0,
            },
            Applied::Added(item) => MassChange {
                dry_kg: item.mass_kg,
                fuel_kg: 0.0,
            },
            Applied::Refuel { mass_kg } => MassChange {
                dry_kg: 0.0,
                fuel_kg: *mass_kg,
            },
            Applied::Taxi { fuel_kg } => MassChange {
                dry_kg: 0.0,
                fuel_kg: -fuel_kg,
            },
            Applied::Transfer { .. } | Applied::TrimOn { .. } | Applied::TrimOff => {
                MassChange::default()
            }
        }
    }

    /// The first leg stage whose start the event waits for: events at the
    /// start of stage `k` happen before the first leg of a stage `>= k`,
    /// events at its end before the first leg of a stage `> k`.
    fn first_stage_after(&self) -> usize {
        match self.at {
            EventTime::Start => self.stage,
            EventTime::End => self.stage + 1,
        }
    }
}

/// The declared point masses as the payload layout placed them (after the
/// auto-balance moved the movable ones), carried from brake release.
#[must_use]
pub fn declared_items(layout: Option<&PayloadLayout>) -> Vec<CarriedItem> {
    layout
        .into_iter()
        .flat_map(|layout| &layout.items)
        .filter_map(|item| match &item.meta {
            ItemMeta::Declared(meta) if item.kind == ItemKind::PointMass => {
                Some(CarriedItem::from_deck_item(item, meta))
            }
            _ => None,
        })
        .collect()
}

/// Resolve every event of `plan`, in flight order, against the items carried
/// from the start. Taxi stages become fuel events of `taxi_flow_kg_s`.
///
/// # Errors
///
/// A message when a release names an item that is not carried.
pub fn resolve(
    plan: &MissionPlan,
    declared: &[CarriedItem],
    taxi_flow_kg_s: f64,
) -> Result<Vec<ResolvedEvent>, String> {
    let mut carried: Vec<CarriedItem> = declared.to_vec();
    let mut events = Vec::new();
    for (stage_index, stage) in plan.stages.iter().enumerate() {
        let mut ordered: Vec<_> = stage.events.iter().collect();
        ordered.sort_by_key(|event| u8::from(event.at == EventTime::End));
        let taxi = match &stage.kind {
            StageKind::Taxi { time_min } => Some(taxi_flow_kg_s * time_min * 60.0),
            _ => None,
        };
        let mut push = |at: EventTime, applied: Applied| {
            events.push(ResolvedEvent {
                stage: stage_index,
                at,
                applied,
            });
        };
        for event in ordered.iter().filter(|event| event.at == EventTime::Start) {
            apply(&event.action, &mut carried, &mut |applied| {
                push(EventTime::Start, applied)
            })
            .map_err(|message| format!("stage '{}': {message}", stage.id))?;
        }
        if let Some(fuel_kg) = taxi {
            push(EventTime::End, Applied::Taxi { fuel_kg });
        }
        for event in ordered.iter().filter(|event| event.at == EventTime::End) {
            apply(&event.action, &mut carried, &mut |applied| {
                push(EventTime::End, applied)
            })
            .map_err(|message| format!("stage '{}': {message}", stage.id))?;
        }
    }
    Ok(events)
}

fn apply(
    action: &EventAction,
    carried: &mut Vec<CarriedItem>,
    emit: &mut dyn FnMut(Applied),
) -> Result<(), String> {
    match action {
        EventAction::PayloadRelease { items } => {
            let mut released = Vec::new();
            if items.is_empty() {
                let (gone, kept): (Vec<_>, Vec<_>) =
                    carried.drain(..).partition(|item| item.releasable);
                *carried = kept;
                released = gone;
            }
            for id in items {
                let index = carried
                    .iter()
                    .position(|item| &item.id == id)
                    .ok_or_else(|| format!("'{id}' is not carried"))?;
                released.push(carried.remove(index));
            }
            emit(Applied::Released(released));
        }
        EventAction::StoreAdd {
            id,
            mass_kg,
            x_m,
            y_m,
            z_m,
            drag_area_m2,
        } => {
            let item = CarriedItem {
                id: id.clone(),
                mass_kg: *mass_kg,
                position_m: [*x_m, *y_m, *z_m],
                drag_area_m2: drag_area_m2.unwrap_or(0.0),
                releasable: true,
                inertia_kgm2: None,
            };
            carried.push(item.clone());
            emit(Applied::Added(item));
        }
        EventAction::AerialRefuel { mass_kg } => emit(Applied::Refuel { mass_kg: *mass_kg }),
        EventAction::FuelTransfer { from, to, mass_kg } => emit(Applied::Transfer {
            from: from.clone(),
            to: to.clone(),
            mass_kg: *mass_kg,
        }),
        EventAction::TrimTankControl {
            target_min_pct_mac,
            target_max_pct_mac,
        } => emit(Applied::TrimOn {
            min_pct_mac: *target_min_pct_mac,
            max_pct_mac: *target_max_pct_mac,
        }),
        EventAction::TrimTankOff => emit(Applied::TrimOff),
    }
    Ok(())
}

/// Index of the first leg of `specs` that comes at or after stage `stage`;
/// `specs.len()` when none does.
fn first_leg_at_or_after(specs: &[SegmentSpec], stage: usize) -> usize {
    specs
        .iter()
        .position(|spec| stage_of_tag(&spec.tag).is_some_and(|leg_stage| leg_stage >= stage))
        .unwrap_or(specs.len())
}

/// The leg each event waits for (`specs.len()` for one after the last leg).
#[must_use]
pub fn trigger_legs(specs: &[SegmentSpec], events: &[ResolvedEvent]) -> Vec<usize> {
    events
        .iter()
        .map(|event| first_leg_at_or_after(specs, event.first_stage_after()))
        .collect()
}

/// The mass change before each leg of `specs`.
#[must_use]
pub fn changes(specs: &[SegmentSpec], events: &[ResolvedEvent]) -> Vec<MassChange> {
    let mut changes = vec![MassChange::default(); specs.len()];
    for (event, leg) in events.iter().zip(trigger_legs(specs, events)) {
        if let Some(slot) = changes.get_mut(leg) {
            let change = event.change();
            slot.dry_kg += change.dry_kg;
            slot.fuel_kg += change.fuel_kg;
        }
    }
    changes
}

/// The parasite drag area of what is carried during each leg, m2: the declared
/// items less those released before it, plus the stores added.
#[must_use]
pub fn carried_drag_area_m2(
    specs: &[SegmentSpec],
    events: &[ResolvedEvent],
    declared: &[CarriedItem],
) -> Vec<f64> {
    let triggers = trigger_legs(specs, events);
    let mut area: f64 = declared.iter().map(|item| item.drag_area_m2).sum();
    let mut next = 0;
    let mut order: Vec<usize> = (0..events.len()).collect();
    order.sort_by_key(|&index| triggers[index]);
    specs
        .iter()
        .enumerate()
        .map(|(leg, _)| {
            while next < order.len() && triggers[order[next]] <= leg {
                match &events[order[next]].applied {
                    Applied::Released(items) => {
                        area -= items.iter().map(|item| item.drag_area_m2).sum::<f64>();
                    }
                    Applied::Added(item) => area += item.drag_area_m2,
                    _ => {}
                }
                next += 1;
            }
            area.max(0.0)
        })
        .collect()
}

// Tests build their own specs, so a failed unwrap is the assertion failing.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission_stage::schedule::cruise;
    use alas_config::{PlanStage, StageEvent};

    fn leg(tag: &str) -> SegmentSpec {
        cruise(tag, None, 1_000.0, 100.0, 0.0)
    }

    fn release_plan() -> MissionPlan {
        let mut cruise = PlanStage::new(
            "out",
            StageKind::Cruise {
                extent: alas_config::CruiseExtent::DistanceKm(10.0),
                altitude_ft: None,
                agl: false,
                speed_m_s: None,
            },
        );
        cruise.events.push(StageEvent {
            at: EventTime::End,
            action: EventAction::PayloadRelease { items: Vec::new() },
        });
        MissionPlan {
            stages: vec![
                PlanStage::new("taxi", StageKind::Taxi { time_min: 10.0 }),
                cruise,
                PlanStage::new(
                    "hold",
                    StageKind::Loiter {
                        time_min: 5.0,
                        altitude_ft: None,
                        agl: false,
                        speed_m_s: None,
                    },
                ),
            ],
            ..MissionPlan::default()
        }
    }

    fn item(id: &str, kg: f64, releasable: bool, drag: f64) -> CarriedItem {
        CarriedItem {
            id: id.to_owned(),
            mass_kg: kg,
            position_m: [10.0, 0.0, 0.0],
            drag_area_m2: drag,
            releasable,
            inertia_kgm2: None,
        }
    }

    #[test]
    fn a_tag_names_its_stage_even_after_pieces_and_speed_changes() {
        assert_eq!(stage_of_tag("cruise@03"), Some(3));
        assert_eq!(stage_of_tag("descent_2@12_path_entry"), Some(12));
        assert_eq!(stage_of_tag("speed_change_to_loiter@07"), Some(7));
        assert_eq!(stage_of_tag("takeoff"), None);
    }

    #[test]
    fn an_end_event_waits_for_the_first_leg_of_a_later_stage_and_taxi_burns_fuel_there() {
        let items = vec![
            item("a", 1_000.0, true, 0.5),
            item("fixed", 200.0, false, 0.1),
        ];
        let events = resolve(&release_plan(), &items, 0.1).unwrap();
        // The taxi fuel (0.1 kg/s for 600 s) then the release of what is releasable.
        assert!(
            matches!(&events[0].applied, Applied::Taxi { fuel_kg } if (*fuel_kg - 60.0).abs() < 1e-9)
        );
        assert!(
            matches!(&events[1].applied, Applied::Released(gone) if gone.len() == 1 && gone[0].id == "a")
        );
        let legs = [
            leg("cruise@01"),
            leg("speed_change_to_loiter@02"),
            leg("loiter@02"),
        ];
        // Taxi at the end of stage 0 and the release at the end of stage 1 both
        // wait for the first leg of a later stage.
        assert_eq!(trigger_legs(&legs, &events), vec![0, 1]);
        let changes = changes(&legs, &events);
        assert_eq!(changes[0].fuel_kg, -60.0);
        assert_eq!(changes[1].dry_kg, -1_000.0);
        // The released item's drag leaves with it; the fixed one stays.
        let drag = carried_drag_area_m2(&legs, &events, &items);
        assert!(
            (drag[0] - 0.6).abs() < 1e-12
                && (drag[1] - 0.1).abs() < 1e-12
                && (drag[2] - 0.1).abs() < 1e-12
        );
    }
}
