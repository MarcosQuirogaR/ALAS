// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mission stage editor's data steps: reading and writing the plan node,
//! new stages and events, and the schematic profile the plot draws.
//!
//! Altitudes are feet and distances kilometres as the plan stores them; the
//! plot's progress axis is a schematic duration, not a distance.

use alas_config::{
    CruiseExtent, EventAction, EventTime, MissionPlan, PlanStage, StageEvent, StageKind,
};
use serde_json::Value;

use crate::state::AppState;

const METRES_PER_FOOT: f64 = 0.3048;
/// Minutes the schematic spends per kilometre of cruise (a 800 km/h cruise).
const MINUTES_PER_KM: f64 = 0.075;

/// The stage kinds the Add Stage menu offers, in flight order.
pub(super) const STAGE_KINDS: [&str; 11] = [
    "Taxi",
    "Takeoff",
    "Climb",
    "Cruise",
    "Loiter",
    "Combat",
    "Descent",
    "Approach",
    "Go Around",
    "Diversion",
    "Landing",
];

/// The event kinds the Add Event menu offers.
pub(super) const EVENT_KINDS: [&str; 6] = [
    "Payload Release",
    "Store Add",
    "Aerial Refuel",
    "Fuel Transfer",
    "Trim Tank Control",
    "Trim Tank Off",
];

/// The plan stored in the configuration; the empty plan when there is none.
pub(super) fn read_plan(state: &AppState) -> MissionPlan {
    state
        .config_values
        .pointer("/mission/plan")
        .and_then(|node| serde_json::from_value(node.clone()).ok())
        .unwrap_or_default()
}

/// Store `plan` in a configuration value. An empty plan removes the node,
/// which is the standard mission. Returns whether the stored value changed.
pub(super) fn store_plan(config_values: &mut Value, plan: &MissionPlan) -> bool {
    let Some(mission) = config_values
        .get_mut("mission")
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    if plan.is_empty() {
        return mission.remove("plan").is_some();
    }
    let Ok(value) = serde_json::to_value(plan) else {
        return false;
    };
    let changed = mission.get("plan") != Some(&value);
    mission.insert("plan".to_owned(), value);
    changed
}

/// [`store_plan`] followed by the case's change handling.
pub(super) fn write_plan(state: &mut AppState, plan: &MissionPlan) -> bool {
    let changed = store_plan(&mut state.config_values, plan);
    if changed {
        state.on_config_modified();
    }
    changed
}

/// Cruise altitude of the case, ft.
pub(super) fn cruise_altitude_ft(state: &AppState) -> f64 {
    state.typed_config_arc().map_or(35_000.0, |config| {
        config.requirements.cruise_altitude_m / METRES_PER_FOOT
    })
}

/// A stage id not used in `plan`: `base`, then `base-2`, `base-3`, ...
pub(super) fn unique_id(plan: &MissionPlan, base: &str) -> String {
    let base = base.to_lowercase().replace(' ', "-");
    if plan.position(&base).is_none() {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| plan.position(id).is_none())
        .unwrap_or(base)
}

/// A new stage of the named kind with usual values.
pub(super) fn new_stage(plan: &MissionPlan, kind: &str, cruise_ft: f64) -> Option<PlanStage> {
    let kind = match kind {
        "Taxi" => StageKind::Taxi { time_min: 10.0 },
        "Takeoff" => StageKind::Takeoff { field: None },
        "Climb" => StageKind::Climb {
            altitude_ft: cruise_ft,
            agl: false,
            rate_m_s: None,
        },
        "Cruise" => StageKind::Cruise {
            extent: CruiseExtent::DistanceKm(500.0),
            altitude_ft: None,
            agl: false,
            speed_m_s: None,
        },
        "Loiter" => StageKind::Loiter {
            time_min: 30.0,
            altitude_ft: None,
            agl: false,
            speed_m_s: None,
        },
        "Combat" => StageKind::Combat {
            time_min: 5.0,
            load_factor: 3.0,
            altitude_ft: None,
            agl: false,
            speed_m_s: None,
        },
        "Descent" => StageKind::Descent {
            altitude_ft: 1_500.0,
            agl: true,
        },
        "Approach" => StageKind::Approach {
            decision_height_ft: alas_config::CATEGORY_I_DECISION_HEIGHT_FT,
            field_elevation_ft: None,
        },
        "Go Around" => StageKind::GoAround {
            altitude_ft: 3_000.0,
            agl: true,
        },
        "Diversion" => StageKind::Diversion {
            distance_km: 200.0,
            cruise_altitude_ft: cruise_ft.min(25_000.0),
            field_elevation_ft: None,
        },
        "Landing" => StageKind::Landing {
            field_elevation_ft: None,
        },
        _ => return None,
    };
    let id = unique_id(plan, kind.label());
    Some(PlanStage::new(&id, kind))
}

/// Where a new stage goes: before a trailing landing (or diversion landing),
/// so adding to a finished plan keeps the order valid; at the end otherwise.
pub(super) fn insert_position(plan: &MissionPlan, stage: &PlanStage) -> usize {
    let lands = |kind: &StageKind| matches!(kind, StageKind::Landing { .. });
    if lands(&stage.kind) {
        return plan.stages.len();
    }
    match plan.stages.last() {
        Some(last) if lands(&last.kind) => plan.stages.len() - 1,
        _ => plan.stages.len(),
    }
}

/// A new event of the named kind. `releasable` offers the point masses a
/// release may name.
pub(super) fn new_event(kind: &str) -> Option<StageEvent> {
    let action = match kind {
        "Payload Release" => EventAction::PayloadRelease { items: Vec::new() },
        "Store Add" => EventAction::StoreAdd {
            id: "store".to_owned(),
            mass_kg: 100.0,
            x_m: 10.0,
            y_m: 0.0,
            z_m: 0.0,
            drag_area_m2: None,
        },
        "Aerial Refuel" => EventAction::AerialRefuel { mass_kg: 1_000.0 },
        "Fuel Transfer" => EventAction::FuelTransfer {
            from: String::new(),
            to: String::new(),
            mass_kg: 500.0,
        },
        "Trim Tank Control" => EventAction::TrimTankControl {
            target_min_pct_mac: 28.0,
            target_max_pct_mac: 32.0,
        },
        "Trim Tank Off" => EventAction::TrimTankOff,
        _ => return None,
    };
    Some(StageEvent {
        at: EventTime::End,
        action,
    })
}

/// The name of an event in the stage list.
pub(super) fn event_label(action: &EventAction) -> &'static str {
    match action {
        EventAction::PayloadRelease { .. } => "Payload Release",
        EventAction::StoreAdd { .. } => "Store Add",
        EventAction::AerialRefuel { .. } => "Aerial Refuel",
        EventAction::FuelTransfer { .. } => "Fuel Transfer",
        EventAction::TrimTankControl { .. } => "Trim Tank Control",
        EventAction::TrimTankOff => "Trim Tank Off",
    }
}

fn altitude_text(altitude_ft: f64, agl: bool) -> String {
    let suffix = if agl { " AGL" } else { "" };
    format!("{altitude_ft:.0} ft{suffix}")
}

/// One-line values of a stage for its list row.
pub(super) fn summary(kind: &StageKind) -> String {
    match kind {
        StageKind::Taxi { time_min } => format!("{time_min:.0} min"),
        StageKind::Takeoff { field } => field
            .as_ref()
            .map(|field| format!("{:.0} m", field.toda_m))
            .unwrap_or_default(),
        StageKind::Climb {
            altitude_ft, agl, ..
        }
        | StageKind::Descent { altitude_ft, agl }
        | StageKind::GoAround { altitude_ft, agl } => altitude_text(*altitude_ft, *agl),
        StageKind::Cruise { extent, .. } => match extent {
            CruiseExtent::DistanceKm(km) => format!("{km:.0} km"),
            CruiseExtent::TimeMin(min) => format!("{min:.0} min"),
            CruiseExtent::Remainder => crate::views::tr_label("Route"),
        },
        StageKind::Loiter { time_min, .. } => format!("{time_min:.0} min"),
        StageKind::Combat {
            time_min,
            load_factor,
            ..
        } => format!("{time_min:.0} min, {load_factor:.1} g"),
        StageKind::Approach {
            decision_height_ft, ..
        } => format!("{decision_height_ft:.0} ft"),
        StageKind::Diversion { distance_km, .. } => format!("{distance_km:.0} km"),
        StageKind::Landing { .. } => String::new(),
    }
}

/// Whether a plot segment climbs, holds altitude or descends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Trend {
    Climb,
    Level,
    Descent,
}

/// One stage on the schematic profile.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ProfileSegment {
    /// Index of the stage in the plan.
    pub stage: usize,
    /// Progress at the start and end, 0 to 1.
    pub progress: [f64; 2],
    /// Altitude at the start and end, ft.
    pub altitude_ft: [f64; 2],
    /// A level reached in between and held (a diversion's cruise), ft.
    pub via_ft: Option<f64>,
    pub trend: Trend,
    /// Whether the stage carries events (drawn as diamonds at its end).
    pub has_events: bool,
    /// Whether the stage is a reserve stage, drawn secondary to the trip.
    pub reserve: bool,
    /// When the stage drops payload (a Payload Release event), if it does:
    /// the plot marks that point.
    pub release: Option<EventTime>,
}

/// The schematic profile of `plan`: altitude in ft against mission progress.
///
/// `departure_ft` and `arrival_ft` are the field elevations. A stage's `agl`
/// altitude is measured from the field the aircraft is working from, as the
/// plan engine does: the departure field until an approach, then the field of
/// that approach (its own elevation, else the arrival field).
pub(super) fn profile(
    plan: &MissionPlan,
    departure_ft: f64,
    arrival_ft: f64,
    approach_ft: f64,
) -> Vec<ProfileSegment> {
    let mut altitude = departure_ft;
    let mut ground = departure_ft;
    let mut raw = Vec::with_capacity(plan.stages.len());
    for (index, stage) in plan.stages.iter().enumerate() {
        let start = altitude;
        let mut via = None;
        let (end, weight) = match &stage.kind {
            StageKind::Taxi { time_min } => (start, *time_min * 0.2),
            StageKind::Takeoff { .. } => (start, 2.0),
            StageKind::Climb {
                altitude_ft, agl, ..
            } => {
                let target = target(*altitude_ft, *agl, ground);
                (target, ((target - start).abs() / 2_500.0).max(1.0))
            }
            StageKind::Cruise {
                extent,
                altitude_ft,
                agl,
                ..
            } => {
                let end = altitude_ft.map_or(start, |ft| target(ft, *agl, ground));
                let weight = match extent {
                    CruiseExtent::DistanceKm(km) => *km * MINUTES_PER_KM,
                    CruiseExtent::TimeMin(min) => *min,
                    CruiseExtent::Remainder => 60.0,
                };
                (end, weight.max(1.0))
            }
            StageKind::Loiter {
                time_min,
                altitude_ft,
                agl,
                ..
            }
            | StageKind::Combat {
                time_min,
                altitude_ft,
                agl,
                ..
            } => (
                altitude_ft.map_or(start, |ft| target(ft, *agl, ground)),
                *time_min,
            ),
            StageKind::Descent { altitude_ft, agl } => {
                let target = target(*altitude_ft, *agl, ground);
                (target, ((start - target).abs() / 2_500.0).max(1.0))
            }
            StageKind::Approach {
                decision_height_ft,
                field_elevation_ft,
            } => {
                ground = field_elevation_ft.unwrap_or(arrival_ft);
                (ground + decision_height_ft, 3.0)
            }
            StageKind::GoAround { altitude_ft, agl } => (target(*altitude_ft, *agl, ground), 2.0),
            StageKind::Diversion {
                distance_km,
                cruise_altitude_ft,
                field_elevation_ft,
            } => {
                via = Some(*cruise_altitude_ft);
                ground = field_elevation_ft.unwrap_or(arrival_ft);
                (
                    approach_ft.max(ground),
                    (*distance_km * MINUTES_PER_KM).max(1.0) + 6.0,
                )
            }
            StageKind::Landing { field_elevation_ft } => {
                ground = field_elevation_ft.unwrap_or(arrival_ft);
                (ground, 2.0)
            }
        };
        altitude = end;
        let trend = if end > start + 50.0 {
            Trend::Climb
        } else if end < start - 50.0 {
            Trend::Descent
        } else {
            Trend::Level
        };
        raw.push((index, [start, end], via, weight.max(0.5), trend, stage));
    }
    let total: f64 = raw.iter().map(|(_, _, _, weight, _, _)| weight).sum();
    let mut progress = 0.0;
    raw.into_iter()
        .map(|(stage, altitude_ft, via_ft, weight, trend, source)| {
            let from = progress;
            progress += weight / total.max(f64::EPSILON);
            ProfileSegment {
                stage,
                progress: [from, progress],
                altitude_ft,
                via_ft,
                trend,
                has_events: !source.events.is_empty(),
                reserve: source.reserve,
                release: release_time(source),
            }
        })
        .collect()
}

/// When `stage` releases payload: the end of the stage if any release is at
/// its end, else its start.
fn release_time(stage: &PlanStage) -> Option<EventTime> {
    let mut found = None;
    for event in &stage.events {
        if matches!(event.action, EventAction::PayloadRelease { .. }) {
            if event.at == EventTime::End {
                return Some(EventTime::End);
            }
            found = Some(EventTime::Start);
        }
    }
    found
}

fn target(altitude_ft: f64, agl: bool, field_ft: f64) -> f64 {
    if agl {
        field_ft + altitude_ft
    } else {
        altitude_ft
    }
}

/// Departure and arrival field elevations of the case, ft.
pub(super) fn field_elevations_ft(state: &AppState) -> [f64; 3] {
    state
        .typed_config_arc()
        .map_or([0.0, 0.0, 3_000.0], |config| {
            let elevation = |name: &str| {
                crate::views::mission_profile_preview::airport_elevation_m(name) / METRES_PER_FOOT
            };
            [
                elevation(&config.departure_airport),
                elevation(&config.arrival_airport),
                config.mission.profile.descent_4_altitude_ft,
            ]
        })
}

/// Identifiers of the fuel tanks the case's aircraft has, in layout order:
/// what a fuel transfer may name. Built from the product tank layout and kept
/// in the egui context while the configuration is unchanged.
pub(super) fn tank_ids(state: &AppState, ctx: &egui::Context) -> Vec<String> {
    #[derive(Clone)]
    struct Cached(u64, Vec<String>);
    let key = state.config_fingerprint_now();
    let id = egui::Id::new("mission_stage_editor_tank_ids");
    if let Some(Cached(cached_key, ids)) = ctx.data(|data| data.get_temp::<Cached>(id)) {
        if cached_key == key {
            return ids;
        }
    }
    let ids: Vec<String> = state
        .typed_config_arc()
        .and_then(|config| {
            let design = state.current_design().unwrap_or_default();
            let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&design), false)
                .ok()?;
            let layout = alas_mass::tanks::resolve_product_layout(&config, &design, &plane).ok()?;
            Some(layout.tanks().iter().map(|tank| tank.id.clone()).collect())
        })
        .unwrap_or_default();
    ctx.data_mut(|data| data.insert_temp(id, Cached(key, ids.clone())));
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standard() -> MissionPlan {
        MissionPlan::from_profile(&alas_config::MissionProfileConfig::default(), 35_000.0)
    }

    #[test]
    fn ids_stay_unique_and_added_stages_keep_the_order_valid() {
        let mut plan = standard();
        let cruise = new_stage(&plan, "Cruise", 35_000.0).unwrap();
        assert_eq!(cruise.id, "cruise-2");
        let at = insert_position(&plan, &cruise);
        assert_eq!(at, plan.stages.len() - 1, "before the landing");
        plan.stages.insert(at, cruise);
        assert!(plan.validate().is_ok());
        let landing = new_stage(&plan, "Landing", 35_000.0).unwrap();
        assert_eq!(landing.id, "landing-2");
        assert!(new_stage(&plan, "Nonsense", 0.0).is_none());
    }

    #[test]
    fn every_menu_kind_builds_a_stage_or_event() {
        let plan = MissionPlan::default();
        for kind in STAGE_KINDS {
            let stage = new_stage(&plan, kind, 30_000.0).expect(kind);
            assert_eq!(stage.kind.label(), kind);
        }
        for kind in EVENT_KINDS {
            let event = new_event(kind).expect(kind);
            assert_eq!(event_label(&event.action), kind);
        }
    }

    #[test]
    fn the_profile_climbs_holds_and_descends_with_progress_from_zero_to_one() {
        let plan = standard();
        let segments = profile(&plan, 0.0, 0.0, 3_000.0);
        assert_eq!(segments.len(), plan.stages.len());
        assert_eq!(segments[0].progress[0], 0.0);
        assert!((segments.last().unwrap().progress[1] - 1.0).abs() < 1e-9);
        assert_eq!(segments[1].trend, Trend::Climb);
        assert_eq!(segments[2].trend, Trend::Level);
        assert_eq!(segments[3].trend, Trend::Descent);
        assert!(segments
            .windows(2)
            .all(|w| w[0].progress[1] == w[1].progress[0]));
    }

    #[test]
    fn events_mark_their_segment() {
        let mut plan = standard();
        plan.stages[2]
            .events
            .push(new_event("Aerial Refuel").unwrap());
        let segments = profile(&plan, 0.0, 0.0, 3_000.0);
        assert!(segments[2].has_events && !segments[1].has_events);
    }
}
